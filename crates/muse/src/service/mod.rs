// SPDX-License-Identifier: AGPL-3.0-only

mod client;
mod protocol;
mod runtime;

use crate::playback::{Current, View, Worker};
pub(crate) use client::Client;
pub(crate) use protocol::Command;
use protocol::{Request, Response, TIMEOUT, VERSION};
use runtime::Endpoint;
pub(crate) use runtime::Runtime;
use std::{
    fs::{self, TryLockError},
    io,
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::Path,
    process::{Command as Process, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub(crate) struct Remote {
    endpoint: Endpoint,
}

impl Remote {
    pub fn find(path: &Path) -> Result<Option<Self>, String> {
        let Some(runtime) = Runtime::existing(path)? else {
            return Ok(None);
        };
        let Some(lock) = runtime.lock(false)? else {
            return Ok(None);
        };
        match lock.try_lock() {
            Ok(()) => Ok(None),
            Err(TryLockError::WouldBlock) => {
                // The owner takes its lock before atomically publishing the endpoint.
                let deadline = Instant::now() + Duration::from_secs(5);
                while !runtime.endpoint_path().exists() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(10));
                }
                Ok(Some(Self {
                    endpoint: runtime.endpoint()?,
                }))
            }
            Err(TryLockError::Error(e)) => Err(e.to_string()),
        }
    }

    pub fn attach(path: &Path) -> Result<Self, String> {
        if let Some(remote) = Self::find(path)? {
            remote.call(Command::Status, None)?;
            return Ok(remote);
        }
        let runtime = Runtime::existing(path)?.ok_or("library does not exist")?;
        runtime.prepare()?;
        let mut child = Process::new(std::env::current_exe().map_err(|e| e.to_string())?);
        child
            .arg("--internal-service")
            .arg("--library")
            .arg(&runtime.library);
        detached_process(&mut child);
        let mut child = child
            .spawn()
            .map_err(|e| format!("cannot start playback service: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut last_error = "service did not publish an endpoint".to_string();
        loop {
            match Self::find(path) {
                Ok(Some(remote)) => match remote.call(Command::Status, None) {
                    Ok(_) => {
                        // Reap this launcher-owned child on shutdown without tying its lifetime to ours.
                        thread::spawn(move || {
                            let _ = child.wait();
                        });
                        return Ok(remote);
                    }
                    Err(e) => last_error = e,
                },
                Ok(None) => (),
                Err(e) => last_error = e,
            }
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())?
                && !status.success()
            {
                return Err(format!(
                    "playback service failed to start ({status}): {last_error}"
                ));
            }
            if Instant::now() >= deadline {
                thread::spawn(move || {
                    let _ = child.wait();
                });
                return Err(format!("cannot connect to playback service: {last_error}"));
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    pub fn call(&self, command: Command, expected: Option<Current>) -> Result<View, String> {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, self.endpoint.port));
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1))
            .map_err(|e| format!("playback service unavailable: {e}"))?;
        stream
            .set_read_timeout(Some(TIMEOUT))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let request = Request {
            version: VERSION,
            token: self.endpoint.token.clone(),
            id,
            command,
            expected,
        };
        let response = (|| -> io::Result<Response> {
            protocol::write(&mut stream, &request)?;
            protocol::read(&mut stream, TIMEOUT)
        })()
        .map_err(|e| {
            format!(
                "service response lost; outcome unconfirmed, inspect status before retrying: {e}"
            )
        })?;
        if response.version != VERSION || response.id != id {
            return Err("invalid playback service response".into());
        }
        response.result.map(Into::into)
    }
}

pub(crate) fn detached_process(command: &mut Process) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid is async-signal-safe and the closure touches no shared state.
        unsafe {
            command.pre_exec(|| rustix::process::setsid().map(|_| ()).map_err(Into::into));
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
        command.creation_flags(0x0000_0008 | 0x0000_0200);
    }
}

pub(crate) fn run(path: &Path) -> Result<(), String> {
    run_with(path, |path| Worker::start(path.to_owned()))
}

pub(crate) fn run_with(
    path: &Path,
    make_worker: impl FnOnce(&Path) -> Result<Worker, String>,
) -> Result<(), String> {
    let _library_ownership =
        muse_library::Library::open_existing(path).map_err(|e| e.to_string())?;
    let runtime = Runtime::existing(path)?.ok_or("library does not exist")?;
    let lock = runtime.lock(true)?.ok_or("cannot open service lock")?;
    match lock.try_lock() {
        Ok(()) => (),
        Err(TryLockError::WouldBlock) => return Ok(()),
        Err(TryLockError::Error(e)) => return Err(e.to_string()),
    }
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let endpoint = runtime.publish(listener.local_addr().map_err(|e| e.to_string())?.port())?;
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let cleanup = Cleanup(runtime.endpoint_path());
    let worker = make_worker(&runtime.library)?;
    let running = AtomicBool::new(true);
    let active = AtomicUsize::new(0);
    let controls = Mutex::new(());
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let result = thread::scope(|scope| -> Result<(), String> {
        // Join completed handlers as we go, so an idle long-lived service stays bounded.
        let mut handlers = Vec::new();
        while running.load(Ordering::Acquire) {
            let mut retained = Vec::new();
            for handle in handlers.drain(..) {
                let handle: thread::ScopedJoinHandle<'_, ()> = handle;
                if handle.is_finished() {
                    let _ = handle.join();
                } else {
                    retained.push(handle);
                }
            }
            handlers = retained;
            match listener.accept() {
                Ok((mut stream, _)) => {
                    // accept() can inherit O_NONBLOCK on Unix hosts. Each handler
                    // uses bounded blocking frame I/O, independently of the listener.
                    if stream.set_nonblocking(false).is_err() {
                        continue;
                    }
                    if active.load(Ordering::Acquire) >= 16 {
                        continue;
                    }
                    active.fetch_add(1, Ordering::AcqRel);
                    let (worker, running, active, controls, endpoint, shutdown_tx) = (
                        &worker,
                        &running,
                        &active,
                        &controls,
                        &endpoint,
                        &shutdown_tx,
                    );
                    handlers.push(scope.spawn(move || {
                        struct Active<'a>(&'a AtomicUsize);
                        impl Drop for Active<'_> {
                            fn drop(&mut self) {
                                self.0.fetch_sub(1, Ordering::AcqRel);
                            }
                        }
                        let _active = Active(active);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                        let Ok(request) =
                            protocol::read::<Request>(&mut stream, Duration::from_secs(2))
                        else {
                            return;
                        };
                        let result = if request.version != VERSION
                            || request.token != endpoint.token
                        {
                            Err("incompatible or unauthenticated playback request".into())
                        } else if matches!(request.command, Command::Status) {
                            Ok(worker.view())
                        } else if request.command.loads() && worker.view().pending.is_some() {
                            Err("a track is loading; please wait".into())
                        } else {
                            let _control = controls.lock().unwrap_or_else(|e| e.into_inner());
                            if !running.load(Ordering::Acquire) {
                                Err("playback service is shutting down".into())
                            } else {
                                let result = worker
                                    .execute(request.command.action().unwrap(), request.expected);
                                if matches!(request.command, Command::Shutdown) && result.is_ok() {
                                    running.store(false, Ordering::Release);
                                    let _ = shutdown_tx.send((stream, request.id, result));
                                    return;
                                }
                                result
                            }
                        };
                        let _ = protocol::write(
                            &mut stream,
                            &Response {
                                version: VERSION,
                                id: request.id,
                                result: result.map(Into::into),
                            },
                        );
                    }));
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(e) => {
                    running.store(false, Ordering::Release);
                    return Err(e.to_string());
                }
            }
        }
        Ok(())
    });
    drop(listener);
    drop(worker); // Silence and join before acknowledging shutdown.
    drop(cleanup);
    drop(lock); // Do not unlink the lock inode: waiting contenders must see the same lock.
    if let Ok((mut stream, id, result)) = shutdown_rx.try_recv() {
        let _ = protocol::write(
            &mut stream,
            &Response {
                version: VERSION,
                id,
                result: result.map(Into::into),
            },
        );
    }
    result
}

/// Shared by CLI `pause` / `play` / `prev` and colon transport when no daemon is up.
pub(crate) const TRANSPORT_NEEDS_SERVICE: &str =
    "no playback service running; transport commands need a live playback service";

pub(crate) fn control_text(path: &Path, command: Command) -> Result<String, String> {
    let remote = match command {
        Command::Load(_)
        | Command::PlayPlaylist(_)
        | Command::UsePlaylist(_)
        | Command::Next
        | Command::PlayNextOnce(_)
        | Command::CycleMode
        | Command::SetMode(_) => Some(Remote::attach(path)?),
        _ => Remote::find(path)?,
    };
    let Some(remote) = remote else {
        return match command {
            Command::Status | Command::Stop | Command::Shutdown => {
                Ok("No playback service running".into())
            }
            Command::Play | Command::Pause | Command::Prev => Err(TRANSPORT_NEEDS_SERVICE.into()),
            _ => Err("no playback session; use play <track-id> or open muse".into()),
        };
    };
    let view = remote.call(command, None)?;
    let output = if matches!(command, Command::Status) {
        format_status(path, &view)?
    } else {
        "Playback command completed".into()
    };
    Ok(output)
}

pub(crate) fn format_status(path: &Path, view: &crate::playback::View) -> Result<String, String> {
    let state = view.status.map_or("Empty", |s| match s.state {
        muse_playback::PlaybackState::Empty => "Empty",
        muse_playback::PlaybackState::Ready => "Stopped",
        muse_playback::PlaybackState::Playing => "Playing",
        muse_playback::PlaybackState::Paused => "Paused",
        muse_playback::PlaybackState::Finished => "Finished",
    });
    let track = view.current.map_or("none".into(), |t| t.id.to_string());
    let position = view.status.map_or(0.0, |s| s.position.as_secs_f64());
    let duration = view
        .status
        .and_then(|s| s.duration)
        .map_or("unknown".into(), |d| format!("{:.3}s", d.as_secs_f64()));
    let playlist = if view.playlist_name.is_empty() {
        "none".to_owned()
    } else {
        view.playlist_name.clone()
    };
    let volume_file = Runtime::existing(path)?
        .ok_or_else(|| "library does not exist".to_string())?
        .settings_path();
    let mut output = format!(
        "Track: {track}\nState: {state}\nPosition: {position:.3}s\nDuration: {duration}\nPlaylist: {playlist}\nMode: {}\n{}\nVolume file: {}",
        view.mode.label(),
        crate::volume::format_observation(view.volume),
        volume_file.display()
    );
    if let Some(index) = view.playlist_index {
        output.push_str(&format!("\nIndex: {}/{}", index + 1, view.playlist_len));
    }
    // Older services omit next_index. The playlist position still decides the follower.
    if view.playlist_index.is_some() || view.next_index.is_some() || view.next_track_id.is_some() {
        match resolved_next(view) {
            Some(index) => {
                output.push_str(&format!("\nNext: {}/{}", index + 1, view.playlist_len));
                if let Some(title) = next_title(path, view, index) {
                    output.push_str(&format!("\nNext title: {title}"));
                }
            }
            None => output.push_str("\nNext: none"),
        }
    }
    if let Some(id) = view.pending.filter(|id| *id > 0) {
        output.push_str(&format!("\nLoading: {id}"));
    }
    if let Some(error) = &view.error {
        output.push_str(&format!("\nPlayback error: {error}"));
    }
    Ok(output)
}

fn resolved_next(view: &crate::playback::View) -> Option<u32> {
    if let Some(index) = view.next_index {
        return Some(index);
    }
    let index = view.playlist_index?;
    crate::playback::follower_index(view.mode, index, view.playlist_len)
}

fn next_title(path: &Path, view: &crate::playback::View, index: u32) -> Option<String> {
    if view.next_index.is_some()
        && let Some(id) = view.next_track_id
        && let Some(title) = track_title(path, id)
    {
        return Some(title);
    }
    let library = muse_library::Library::open(path).ok()?;
    let playlist_id = view
        .playlist_id
        .or_else(|| library.all_local().ok().map(|playlist| playlist.id))?;
    let index = usize::try_from(index).ok()?;
    library
        .playlist_tracks(playlist_id)
        .ok()?
        .into_iter()
        .nth(index)
        .map(|track| track.title)
}

fn track_title(path: &Path, id: i64) -> Option<String> {
    muse_library::Library::open(path)
        .ok()?
        .track(id)
        .ok()
        .map(|track| track.title)
}

/// Colon `:pause`, `:play`, `:play <id>`, and `:prev`.
///
/// Unlike CLI `play <id>`, this does not start a daemon. A missing service is
/// the same error as `pause` / `prev`.
pub(crate) fn transport(path: &Path, command: Command) -> Result<View, String> {
    let remote = Remote::find(path)?.ok_or_else(|| TRANSPORT_NEEDS_SERVICE.to_string())?;
    remote.call(command, None)
}

pub(crate) fn volume_text(
    path: &Path,
    command: crate::volume::VolumeCommand,
) -> Result<String, String> {
    let Some(remote) = Remote::find(path)? else {
        return Err(
            "no playback service running; volume commands need a live playback service".into(),
        );
    };
    let view = match command {
        crate::volume::VolumeCommand::Query => remote.call(Command::Status, None)?,
        crate::volume::VolumeCommand::Set(volume) => {
            remote.call(Command::SetVolume(volume), None)?
        }
        crate::volume::VolumeCommand::Inc(steps) => remote.call(Command::IncVolume(steps), None)?,
        crate::volume::VolumeCommand::Dec(steps) => remote.call(Command::DecVolume(steps), None)?,
    };
    Ok(crate::volume::format_observation(view.volume))
}

pub(crate) fn control(path: &Path, command: Command) -> Result<(), String> {
    println!("{}", control_text(path, command)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::{Current, RepeatMode, View};
    use muse_library::Library;
    use muse_playback::{PlaybackState, PlaybackStatus};
    use std::io::{Read, Write};
    use std::time::Duration;

    #[test]
    fn lost_reply_is_reported_without_replaying_the_mutation() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let peer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request: Request = protocol::read(&mut stream, Duration::from_secs(2)).unwrap();
            assert!(matches!(request.command, Command::Toggle));
            drop(stream); // The operation happened, but its completion response was lost.
            listener.set_nonblocking(true).unwrap();
            thread::sleep(Duration::from_millis(150));
            assert_eq!(
                listener.accept().unwrap_err().kind(),
                io::ErrorKind::WouldBlock
            );
        });
        let remote = Remote {
            endpoint: Endpoint {
                version: VERSION,
                port,
                token: "test-only".into(),
                library: "test".into(),
            },
        };
        assert!(
            remote
                .call(Command::Toggle, None)
                .unwrap_err()
                .contains("outcome unconfirmed")
        );
        peer.join().unwrap();
    }

    #[test]
    fn partial_frame_expires_even_when_the_peer_keeps_sending_bytes() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let peer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let started = Instant::now();
            let result = protocol::read::<Request>(&mut stream, Duration::from_millis(120));
            assert!(result.is_err());
            assert!(started.elapsed() < Duration::from_secs(1));
        });
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        for _ in 0..3 {
            let _ = stream.write_all(&[0]);
            thread::sleep(Duration::from_millis(50));
        }
        let mut byte = [0];
        assert!(matches!(stream.read(&mut byte), Ok(0) | Err(_)));
        peer.join().unwrap();
    }

    fn playing(index: u32, len: u32, mode: RepeatMode) -> View {
        View {
            current: Some(Current {
                id: i64::from(index) + 1,
                generation: 1,
            }),
            status: Some(PlaybackStatus {
                state: PlaybackState::Playing,
                position: Duration::from_secs(1),
                duration: Some(Duration::from_secs(180)),
            }),
            playlist_name: "全部本地".into(),
            playlist_index: Some(index),
            playlist_len: len,
            mode,
            ..View::default()
        }
    }

    #[test]
    fn mid_list_sequential_status_names_the_following_position() {
        let temporary = tempfile::TempDir::new().unwrap();
        let path = temporary.path().join("library.muse");
        Library::open(&path).unwrap();
        let text = format_status(&path, &playing(113, 168, RepeatMode::Sequential)).unwrap();
        assert!(text.contains("Index: 114/168"), "{text}");
        assert!(text.contains("Next: 115/168"), "{text}");
        assert!(text.contains("Volume:"), "{text}");
        assert!(text.contains("Gain:"), "{text}");
        assert!(text.contains("Volume file:"), "{text}");
        assert!(!text.contains("Next: none"), "{text}");
        assert!(!text.contains("Next title:"), "{text}");

        let last = format_status(&path, &playing(167, 168, RepeatMode::Sequential)).unwrap();
        assert!(last.contains("Index: 168/168"), "{last}");
        assert!(last.contains("Next: none"), "{last}");
        assert!(!last.contains("Next title:"), "{last}");

        let wrapped = format_status(&path, &playing(167, 168, RepeatMode::List)).unwrap();
        assert!(wrapped.contains("Next: 1/168"), "{wrapped}");
        assert!(!wrapped.contains("Next: none"), "{wrapped}");

        let repeated = format_status(&path, &playing(113, 168, RepeatMode::Track)).unwrap();
        assert!(repeated.contains("Next: 114/168"), "{repeated}");
        assert!(!repeated.contains("Next: none"), "{repeated}");

        let idle = format_status(&path, &View::default()).unwrap();
        assert!(idle.contains("Volume: 100%"), "{idle}");
        assert!(idle.lines().all(|line| !line.starts_with("Next")));
    }

    #[test]
    fn omitted_next_index_uses_playlist_position_not_track_id() {
        let temporary = tempfile::TempDir::new().unwrap();
        let path = temporary.path().join("library.muse");
        let mut library = Library::open(&path).unwrap();
        for (name, bytes) in [
            (
                "a.mp3",
                &include_bytes!("../../../muse-library/tests/fixtures/silence.mp3")[..],
            ),
            (
                "b.flac",
                &include_bytes!("../../../muse-library/tests/fixtures/tone.flac")[..],
            ),
            (
                "c.mp3",
                &include_bytes!("../../../muse-e2e/fixtures/tagged.mp3")[..],
            ),
            (
                "d.mp3",
                &include_bytes!("../../../muse-e2e/fixtures/other-artist.mp3")[..],
            ),
        ] {
            let file = temporary.path().join(name);
            std::fs::write(&file, bytes).unwrap();
            assert_eq!(library.import(&file).imported, 1);
        }
        let removed = library.list().unwrap()[2].id;
        assert!(library.remove(removed).unwrap());
        let playlist = library.all_local().unwrap();
        let tracks = library.playlist_tracks(playlist.id).unwrap();
        assert_eq!(tracks.len(), 3);
        assert_ne!(tracks[2].id, tracks[1].id.saturating_add(1));
        drop(library);

        let mut view = playing(1, 3, RepeatMode::Sequential);
        view.playlist_id = Some(playlist.id);
        view.current = Some(Current {
            id: tracks[1].id,
            generation: 1,
        });
        view.next_index = None;
        view.next_track_id = None;
        let text = format_status(&path, &view).unwrap();
        assert!(text.contains("Index: 2/3"), "{text}");
        assert!(text.contains("Next: 3/3"), "{text}");
        assert!(
            text.contains(&format!("Next title: {}", tracks[2].title)),
            "{text}"
        );
        assert!(!text.contains("Next: none"), "{text}");
        assert!(
            !text.contains(&format!("Next title: {}", tracks[1].title)),
            "{text}"
        );

        let mut once = view.clone();
        once.next_index = Some(0);
        once.next_track_id = Some(tracks[0].id);
        let queued = format_status(&path, &once).unwrap();
        assert!(queued.contains("Next: 1/3"), "{queued}");
        assert!(
            queued.contains(&format!("Next title: {}", tracks[0].title)),
            "{queued}"
        );
        assert!(!queued.contains("Next: 3/3"), "{queued}");

        view.playlist_index = Some(2);
        let end = format_status(&path, &view).unwrap();
        assert!(end.contains("Index: 3/3"), "{end}");
        assert!(end.contains("Next: none"), "{end}");
        assert!(!end.contains("Next title:"), "{end}");
    }
}
