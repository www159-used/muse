// SPDX-License-Identifier: AGPL-3.0-only

use muse_playback::{AudioSource, Playback, PlaybackState, PlaybackStatus, SeekTarget};
use std::{
    fs,
    io::{Read, Write},
    net::{Ipv4Addr, TcpStream},
    path::Path,
    process::{Child, Command, Output},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

// This fake is linked only into test executables. Production binaries always open NativePlayback.
#[derive(Default)]
pub struct ClockPlayer {
    status: Option<PlaybackStatus>,
    started: Option<Instant>,
}
impl Playback for ClockPlayer {
    type Error = std::io::Error;
    fn load(&mut self, mut source: Box<dyn AudioSource>) -> Result<(), Self::Error> {
        let mut bytes = Vec::new();
        source.read_to_end(&mut bytes)?;
        if bytes.is_empty() {
            return Err(std::io::Error::other("empty test audio"));
        }
        thread::sleep(Duration::from_millis(150));
        self.status = Some(PlaybackStatus {
            state: PlaybackState::Ready,
            position: Duration::ZERO,
            duration: Some(Duration::from_secs(3600)),
        });
        self.started = None;
        Ok(())
    }
    fn play(&mut self) -> Result<(), Self::Error> {
        let status = self
            .status
            .as_mut()
            .ok_or_else(|| std::io::Error::other("no track"))?;
        if status.state != PlaybackState::Playing {
            status.state = PlaybackState::Playing;
            self.started = Some(Instant::now());
        }
        Ok(())
    }
    fn pause(&mut self) -> Result<(), Self::Error> {
        let snapshot = self.status()?;
        if snapshot.state == PlaybackState::Playing {
            self.status = Some(PlaybackStatus {
                state: PlaybackState::Paused,
                ..snapshot
            });
            self.started = None;
        }
        Ok(())
    }
    fn stop(&mut self) -> Result<(), Self::Error> {
        if let Some(status) = self.status.as_mut() {
            status.state = PlaybackState::Ready;
            status.position = Duration::ZERO;
        }
        self.started = None;
        Ok(())
    }
    fn seek(&mut self, target: SeekTarget) -> Result<Duration, Self::Error> {
        let mut status = self.status()?;
        status.position = match target {
            SeekTarget::Absolute(d) => d,
            SeekTarget::Forward(d) => status.position.saturating_add(d),
            SeekTarget::Backward(d) => status.position.saturating_sub(d),
        }
        .min(Duration::from_secs(3600));
        self.started = (status.state == PlaybackState::Playing).then(Instant::now);
        self.status = Some(status);
        Ok(status.position)
    }
    fn set_volume(&mut self, _volume: u8) -> Result<(), Self::Error> {
        Ok(())
    }

    fn status(&self) -> Result<PlaybackStatus, Self::Error> {
        let mut status = self.status.unwrap_or(PlaybackStatus {
            state: PlaybackState::Empty,
            position: Duration::ZERO,
            duration: None,
        });
        if let Some(started) = self.started {
            status.position += started.elapsed();
        }
        Ok(status)
    }
}

pub fn child() {
    let Some(path) = std::env::var_os("MUSE_TEST_CHILD_LIBRARY") else {
        return;
    };
    crate::service::run_with(Path::new(&path), |path| {
        crate::playback::Worker::start_with(path.to_owned(), || Ok(ClockPlayer::default()))
    })
    .unwrap();
}

pub fn launch(directory: &TempDir, library: &Path) -> Child {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "service_child", "--ignored", "--nocapture"])
        .env("MUSE_TEST_CHILD_LIBRARY", library)
        .env("MUSE_RUNTIME_DIR", directory.path().join("runtime"));
    crate::service::detached_process(&mut command);
    command.spawn().unwrap()
}

pub fn command(directory: &TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_muse"));
    command
        .current_dir(directory.path())
        .env("MUSE_RUNTIME_DIR", directory.path().join("runtime"));
    command
}

pub fn cli(directory: &TempDir, args: &[&str]) -> Output {
    command(directory).args(args).output().unwrap()
}

pub fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

pub fn wait_ready(directory: &TempDir) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let result = cli(directory, &["status"]);
        if result.status.success() && String::from_utf8_lossy(&result.stdout).contains("State:") {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "service failed to start: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        thread::sleep(Duration::from_millis(10));
    }
}

pub struct Session {
    pub directory: TempDir,
    pub child: Child,
    pub id: i64,
}
impl Session {
    pub fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("library.muse");
        let mut library = muse_library::Library::open(&path).unwrap();
        let audio = directory.path().join("song.mp3");
        fs::write(
            &audio,
            include_bytes!("../../../muse-library/tests/fixtures/silence.mp3"),
        )
        .unwrap();
        assert_eq!(library.import(&audio).imported, 1);
        let id = library.list().unwrap()[0].id;
        drop(library);
        let child = launch(&directory, &path);
        wait_ready(&directory);
        Self {
            directory,
            child,
            id,
        }
    }
    pub fn play(&self) {
        success(cli(&self.directory, &["play", &self.id.to_string()]));
    }
    pub fn status(&self) -> String {
        success(cli(&self.directory, &["status"]))
    }
    pub fn endpoint(&self) -> serde_json::Value {
        let file = fs::read_dir(self.directory.path().join("runtime"))
            .unwrap()
            .map(Result::unwrap)
            .find(|e| e.path().extension().is_some_and(|s| s == "json"))
            .unwrap();
        serde_json::from_slice(&fs::read(file.path()).unwrap()).unwrap()
    }
    pub fn rpc(
        &self,
        command: serde_json::Value,
        expected: serde_json::Value,
    ) -> serde_json::Value {
        let endpoint = self.endpoint();
        let mut stream = TcpStream::connect((
            Ipv4Addr::LOCALHOST,
            endpoint["port"].as_u64().unwrap() as u16,
        ))
        .unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let request = serde_json::json!({"version": endpoint["version"], "token": endpoint["token"], "id": 17, "command": command, "expected": expected});
        let bytes = serde_json::to_vec(&request).unwrap();
        stream
            .write_all(&(bytes.len() as u32).to_be_bytes())
            .unwrap();
        stream.write_all(&bytes).unwrap();
        let mut size = [0; 4];
        stream.read_exact(&mut size).unwrap();
        let mut bytes = vec![0; u32::from_be_bytes(size) as usize];
        stream.read_exact(&mut bytes).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = cli(&self.directory, &["shutdown"]);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
