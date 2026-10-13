// SPDX-License-Identifier: AGPL-3.0-only

use crate::playback::{Action, Current, View};
use muse_playback::{PlaybackState, PlaybackStatus, SeekTarget};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    io::{self, Read, Write},
    net::TcpStream,
    time::{Duration, Instant},
};

pub const VERSION: u32 = 2;
pub const TIMEOUT: Duration = Duration::from_secs(12);
const MAX_FRAME: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Command {
    Status,
    Load(i64),
    Play,
    Pause,
    Toggle,
    Seek { direction: i8, millis: u64 },
    Stop,
    Shutdown,
}

impl Command {
    pub fn action(self) -> Option<Action> {
        Some(match self {
            Self::Load(id) => Action::Load(id),
            Self::Play => Action::Play,
            Self::Pause => Action::Pause,
            Self::Toggle => Action::Toggle,
            Self::Seek { direction, millis } => Action::Seek(match direction {
                -1 => SeekTarget::Backward(Duration::from_millis(millis)),
                0 => SeekTarget::Absolute(Duration::from_millis(millis)),
                _ => SeekTarget::Forward(Duration::from_millis(millis)),
            }),
            Self::Stop | Self::Shutdown => Action::Stop,
            Self::Status => return None,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub version: u32,
    pub token: String,
    pub id: u64,
    pub command: Command,
    pub expected: Option<Current>,
}

#[derive(Serialize, Deserialize)]
pub struct Response {
    pub version: u32,
    pub id: u64,
    pub result: Result<Snapshot, String>,
}

#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    current: Option<Current>,
    pending: Option<i64>,
    status: Option<Status>,
    error: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Status {
    state: State,
    position: Duration,
    duration: Option<Duration>,
}

#[derive(Serialize, Deserialize)]
enum State {
    Empty,
    Ready,
    Playing,
    Paused,
    Finished,
}

impl From<View> for Snapshot {
    fn from(view: View) -> Self {
        Self {
            current: view.current,
            pending: view.pending,
            error: view.error,
            status: view.status.map(|s| Status {
                position: s.position,
                duration: s.duration,
                state: match s.state {
                    PlaybackState::Empty => State::Empty,
                    PlaybackState::Ready => State::Ready,
                    PlaybackState::Playing => State::Playing,
                    PlaybackState::Paused => State::Paused,
                    PlaybackState::Finished => State::Finished,
                },
            }),
        }
    }
}

impl From<Snapshot> for View {
    fn from(snapshot: Snapshot) -> Self {
        Self {
            current: snapshot.current,
            pending: snapshot.pending,
            error: snapshot.error,
            status: snapshot.status.map(|s| PlaybackStatus {
                position: s.position,
                duration: s.duration,
                state: match s.state {
                    State::Empty => PlaybackState::Empty,
                    State::Ready => PlaybackState::Ready,
                    State::Playing => PlaybackState::Playing,
                    State::Paused => PlaybackState::Paused,
                    State::Finished => PlaybackState::Finished,
                },
            }),
        }
    }
}

pub fn read<T: DeserializeOwned>(stream: &mut TcpStream, timeout: Duration) -> io::Result<T> {
    let input = &mut Deadline {
        stream,
        end: Instant::now() + timeout,
    };
    let mut header = [0; 4];
    input.read_exact(&mut header)?;
    let size = u32::from_be_bytes(header) as usize;
    if size > MAX_FRAME {
        return Err(io::Error::other("service frame too large"));
    }
    let mut data = vec![0; size];
    input.read_exact(&mut data)?;
    serde_json::from_slice(&data).map_err(io::Error::other)
}

pub fn write(stream: &mut TcpStream, value: &impl Serialize) -> io::Result<()> {
    let output = &mut Deadline {
        stream,
        end: Instant::now() + Duration::from_secs(2),
    };
    let data = serde_json::to_vec(value).map_err(io::Error::other)?;
    if data.len() > MAX_FRAME {
        return Err(io::Error::other("service frame too large"));
    }
    output.write_all(&(data.len() as u32).to_be_bytes())?;
    output.write_all(&data)?;
    output.flush()
}

// Each frame has one deadline: a peer cannot hold a handler forever by trickling bytes.
struct Deadline<'a> {
    stream: &'a mut TcpStream,
    end: Instant,
}
impl Deadline<'_> {
    fn remaining(&self) -> io::Result<Duration> {
        self.end
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "service frame timed out"))
    }
}
impl Read for Deadline<'_> {
    fn read(&mut self, data: &mut [u8]) -> io::Result<usize> {
        self.stream.set_read_timeout(Some(self.remaining()?))?;
        self.stream.read(data)
    }
}
impl Write for Deadline<'_> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        self.stream.write(data)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}
