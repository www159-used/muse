// SPDX-License-Identifier: AGPL-3.0-only

mod decode;
mod output;

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    AudioSource, Playback, PlaybackError, PlaybackErrorKind as Kind, PlaybackState, PlaybackStatus,
    SeekTarget,
};
use decode::{Decoded, OwnedReader};
use output::{Buffer, DeviceFault, Output, PcmSource, Session, lock};

struct Shared {
    session: Mutex<Option<Arc<Session>>>,
    device: Mutex<Arc<DeviceFault>>,
    alive: AtomicBool,
}

impl Shared {
    fn status(&self) -> Result<PlaybackStatus, PlaybackError> {
        if !self.alive.load(Ordering::Acquire) {
            return Err(PlaybackError::message(
                Kind::Worker,
                "playback worker has stopped",
            ));
        }
        let device = Arc::clone(&lock(&self.device));
        if let Some(error) = lock(&device.error).clone() {
            return Err(error);
        }
        let session = lock(&self.session).clone();
        match session {
            Some(session) => lock(&session.buffer).status(),
            None => Ok(PlaybackStatus {
                state: PlaybackState::Empty,
                position: Duration::ZERO,
                duration: None,
            }),
        }
    }

    fn silence(&self) {
        if let Some(session) = lock(&self.session).as_ref() {
            session.active.store(false, Ordering::Release);
        }
    }
}

enum Operation {
    Load(Box<dyn AudioSource>),
    Play,
    Pause,
    Stop,
    Seek(SeekTarget),
    Shutdown,
}
struct Command {
    operation: Operation,
    reply: Sender<Result<Duration, PlaybackError>>,
}

/// Built-in MP3/FLAC playback using Symphonia decoding and Rodio device output.
///
/// Enable the `native` feature. Construction opens the default output device;
/// failures are returned as [`PlaybackError`]. Methods wait for their control
/// operation to complete on the owned worker. `status` only reads a snapshot.
/// Inputs must be stable owned local readers whose reads eventually return.
/// Loading prepares a new source before replacing a healthy old one. Fatal
/// errors remain visible until a successful load or stop rebuilds playback.
/// Dropping stops submitting music and joins the worker; it never detaches audio.
pub struct NativePlayback {
    commands: Sender<Command>,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl NativePlayback {
    pub fn open_default() -> Result<Self, PlaybackError> {
        Self::start(Output::open)
    }

    fn start(
        open: impl FnOnce() -> Result<Output, PlaybackError> + Send + 'static,
    ) -> Result<Self, PlaybackError> {
        let shared = Arc::new(Shared {
            session: Mutex::new(None),
            device: Mutex::new(Arc::new(DeviceFault::default())),
            alive: AtomicBool::new(true),
        });
        let (commands, receiver) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("muse-audio".into())
            .spawn(move || {
                struct Exit(Arc<Shared>);
                impl Drop for Exit {
                    fn drop(&mut self) {
                        self.0.silence();
                        self.0.alive.store(false, Ordering::Release);
                    }
                }
                let _exit = Exit(Arc::clone(&worker_shared));
                match open() {
                    Ok(output) => {
                        *lock(&worker_shared.device) = Arc::clone(&output.fault);
                        let _ = ready_tx.send(Ok(()));
                        Engine {
                            output,
                            decoded: None,
                            session: None,
                            shared: worker_shared,
                        }
                        .run(receiver);
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                    }
                }
            })
            .map_err(|error| PlaybackError::new(Kind::Worker, error))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                commands,
                shared,
                worker: Some(worker),
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(error) => {
                let _ = worker.join();
                Err(PlaybackError::new(Kind::Worker, error))
            }
        }
    }

    fn request(&mut self, operation: Operation) -> Result<Duration, PlaybackError> {
        let (reply, result) = mpsc::channel();
        self.commands
            .send(Command { operation, reply })
            .map_err(|error| PlaybackError::message(Kind::Worker, error.to_string()))?;
        result
            .recv()
            .map_err(|error| PlaybackError::new(Kind::Worker, error))?
    }

    #[cfg(test)]
    fn offline() -> (Self, rodio::mixer::MixerSource) {
        let (output, samples) = Output::offline();
        (Self::start(move || Ok(output)).unwrap(), samples)
    }
}

impl Playback for NativePlayback {
    type Error = PlaybackError;
    fn load(&mut self, source: Box<dyn AudioSource>) -> Result<(), Self::Error> {
        self.request(Operation::Load(source)).map(|_| ())
    }
    fn play(&mut self) -> Result<(), Self::Error> {
        self.request(Operation::Play).map(|_| ())
    }
    fn pause(&mut self) -> Result<(), Self::Error> {
        self.request(Operation::Pause).map(|_| ())
    }
    fn stop(&mut self) -> Result<(), Self::Error> {
        self.request(Operation::Stop).map(|_| ())
    }
    fn seek(&mut self, target: SeekTarget) -> Result<Duration, Self::Error> {
        self.request(Operation::Seek(target))
    }
    fn status(&self) -> Result<PlaybackStatus, Self::Error> {
        self.shared.status()
    }
}

impl Drop for NativePlayback {
    fn drop(&mut self) {
        self.shared.silence();
        let (reply, _) = mpsc::channel();
        let _ = self.commands.send(Command {
            operation: Operation::Shutdown,
            reply,
        });
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Engine {
    output: Output,
    decoded: Option<Decoded>,
    session: Option<Arc<Session>>,
    shared: Arc<Shared>,
}

impl Engine {
    fn run(mut self, commands: Receiver<Command>) {
        loop {
            match commands.try_recv() {
                Ok(command) => {
                    if matches!(command.operation, Operation::Shutdown) {
                        break;
                    }
                    let result = self.control(command.operation);
                    let _ = command.reply.send(result);
                }
                Err(mpsc::TryRecvError::Disconnected) => break,
                Err(mpsc::TryRecvError::Empty) => {
                    if self.fill() {
                        continue;
                    }
                    match commands.recv_timeout(Duration::from_millis(3)) {
                        Ok(command) => {
                            if matches!(command.operation, Operation::Shutdown) {
                                break;
                            }
                            let _ = command.reply.send(self.control(command.operation));
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
            }
        }
    }

    fn publish(&mut self, anchor: Duration, state: PlaybackState) {
        self.shared.silence();
        let decoded = self.decoded.as_ref().unwrap();
        let budget = (decoded.rate as usize / 4).max(1) * usize::from(decoded.channels);
        let session = Arc::new(Session {
            active: AtomicBool::new(true),
            buffer: Mutex::new(Buffer {
                state,
                anchor,
                frames: 0,
                rate: decoded.rate,
                duration: decoded.duration,
                pcm: VecDeque::with_capacity(budget),
                eof: decoded.eof,
                failure: None,
            }),
        });
        self.output.mixer.add(PcmSource::new(
            Arc::clone(&session),
            Arc::clone(&self.output.fault),
            decoded.channels,
            decoded.rate,
        ));
        *lock(&self.shared.session) = Some(Arc::clone(&session));
        self.session = Some(session);
        self.fill();
    }

    fn recover_output(&mut self) -> Result<(), PlaybackError> {
        if self.output.fault.failed.load(Ordering::Acquire) {
            let output = Output::open()?;
            *lock(&self.shared.device) = Arc::clone(&output.fault);
            self.output = output;
        }
        Ok(())
    }

    fn fail(&self, error: PlaybackError) {
        if let Some(session) = &self.session {
            session.active.store(false, Ordering::Release);
            lock(&session.buffer).failure = Some(error);
        }
    }

    fn fill(&mut self) -> bool {
        if let Some(error) = lock(&self.output.fault.error).clone() {
            self.fail(error);
            return false;
        }
        let (Some(decoded), Some(session)) = (&mut self.decoded, &self.session) else {
            return false;
        };
        let room = {
            let buffer = lock(&session.buffer);
            if buffer.failure.is_some()
                || buffer.state == PlaybackState::Finished
                || !session.active.load(Ordering::Acquire)
            {
                return false;
            }
            let budget = (decoded.rate as usize / 4).max(1) * usize::from(decoded.channels);
            budget.saturating_sub(buffer.pcm.len())
        };
        if room == 0 || decoded.eof {
            return false;
        }
        if decoded.offset == decoded.pending.len() {
            match decoded.next_block() {
                Ok(false) => {
                    lock(&session.buffer).eof = true;
                    return false;
                }
                Ok(true) => {}
                Err(error) => {
                    self.fail(error);
                    return false;
                }
            }
        }
        let count = room.min(decoded.pending.len() - decoded.offset);
        lock(&session.buffer).pcm.extend(
            decoded.pending[decoded.offset..decoded.offset + count]
                .iter()
                .copied(),
        );
        decoded.offset += count;
        true
    }

    fn rebuild(&mut self) -> Result<(), PlaybackError> {
        self.shared.silence();
        let input = self
            .decoded
            .as_ref()
            .ok_or_else(|| PlaybackError::message(Kind::NoSource, "no source loaded"))?
            .input
            .clone();
        match Decoded::open(input).and_then(|decoded| {
            self.recover_output()?;
            Ok(decoded)
        }) {
            Ok(decoded) => {
                self.decoded = Some(decoded);
                self.publish(Duration::ZERO, PlaybackState::Ready);
                Ok(())
            }
            Err(error) => {
                self.fail(error.clone());
                Err(error)
            }
        }
    }

    fn control(&mut self, operation: Operation) -> Result<Duration, PlaybackError> {
        match operation {
            Operation::Load(source) => {
                let candidate = Decoded::open(OwnedReader::new(source)?)?;
                self.recover_output()?;
                self.decoded = Some(candidate);
                self.publish(Duration::ZERO, PlaybackState::Ready);
                Ok(Duration::ZERO)
            }
            Operation::Stop => {
                if self.decoded.is_some() {
                    self.rebuild()?;
                } else {
                    self.recover_output()?;
                }
                Ok(Duration::ZERO)
            }
            Operation::Play => {
                let status = self.shared.status()?;
                if status.state == PlaybackState::Empty {
                    return Err(PlaybackError::message(Kind::NoSource, "no source loaded"));
                }
                if status.state == PlaybackState::Finished {
                    self.rebuild()?;
                }
                lock(&self.session.as_ref().unwrap().buffer).state = PlaybackState::Playing;
                Ok(Duration::ZERO)
            }
            Operation::Pause => {
                let status = self.shared.status()?;
                if status.state == PlaybackState::Playing {
                    lock(&self.session.as_ref().unwrap().buffer).state = PlaybackState::Paused;
                }
                Ok(status.position)
            }
            Operation::Seek(target) => {
                let status = self.shared.status()?;
                if status.state == PlaybackState::Empty {
                    return Err(PlaybackError::message(Kind::NoSource, "no source loaded"));
                }
                let target = match target {
                    SeekTarget::Absolute(time) => time,
                    SeekTarget::Forward(time) => status.position.saturating_add(time),
                    SeekTarget::Backward(time) => status.position.saturating_sub(time),
                };
                let target = status
                    .duration
                    .map_or(target, |duration| target.min(duration));
                self.shared.silence();
                let result = if target.is_zero() {
                    let input = self.decoded.as_ref().unwrap().input.clone();
                    Decoded::open(input).map(|decoded| {
                        self.decoded = Some(decoded);
                        Duration::ZERO
                    })
                } else if status.duration == Some(target) {
                    let decoded = self.decoded.as_mut().unwrap();
                    decoded.pending.clear();
                    decoded.offset = 0;
                    decoded.eof = true;
                    Ok(target)
                } else {
                    self.decoded.as_mut().unwrap().seek(target)
                };
                match result {
                    Ok(actual) => {
                        let state = if status.duration == Some(actual) {
                            PlaybackState::Finished
                        } else if status.state == PlaybackState::Playing {
                            PlaybackState::Playing
                        } else if status.state == PlaybackState::Ready && actual.is_zero() {
                            PlaybackState::Ready
                        } else {
                            PlaybackState::Paused
                        };
                        self.publish(actual, state);
                        Ok(actual)
                    }
                    Err(error) => {
                        self.fail(error.clone());
                        Err(error)
                    }
                }
            }
            Operation::Shutdown => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests;
