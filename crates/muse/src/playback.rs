// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use muse_library::Library;
use muse_playback::{NativePlayback, Playback, PlaybackState, PlaybackStatus, SeekTarget};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Current {
    pub id: i64,
    generation: u64,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct View {
    pub current: Option<Current>,
    pub pending: Option<i64>,
    pub status: Option<PlaybackStatus>,
    pub error: Option<String>,
}

#[derive(Clone, Copy)]
enum Action {
    Load(i64),
    Toggle,
    Seek(SeekTarget),
    Stop,
}
struct Request {
    expected: Option<Current>,
    action: Action,
}

pub(crate) struct Worker {
    commands: SyncSender<Request>,
    view: Arc<Mutex<View>>,
    quit: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn start(path: PathBuf) -> Self {
        Self::start_with(path, || {
            NativePlayback::open_default().map_err(|error| error.to_string())
        })
    }

    fn start_with(
        path: PathBuf,
        open: impl Fn() -> Result<NativePlayback, String> + Send + 'static,
    ) -> Self {
        let (commands, receiver) = mpsc::sync_channel::<Request>(16);
        let view = Arc::new(Mutex::new(View::default()));
        let thread_view = Arc::clone(&view);
        let quit = Arc::new(AtomicBool::new(false));
        let thread_quit = Arc::clone(&quit);
        let alive = Arc::new(AtomicBool::new(true));
        let thread_alive = Arc::clone(&alive);
        let thread = thread::spawn(move || {
            struct Exit(Arc<AtomicBool>);
            impl Drop for Exit {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::Release);
                }
            }
            let _exit = Exit(thread_alive);
            let mut player: Option<NativePlayback> = None;
            let mut current = None;
            let mut generation = 0;
            while !thread_quit.load(Ordering::Acquire) {
                match receiver.recv_timeout(Duration::from_millis(40)) {
                    Ok(request) => {
                        if thread_quit.load(Ordering::Acquire) {
                            break;
                        }
                        let result: Result<(), String> = match request.action {
                            Action::Load(id) => (|| {
                                let source = Library::open(&path)
                                    .and_then(|library| library.open_audio(id))
                                    .map_err(|error| error.to_string())?;
                                if player.is_none() {
                                    player = Some(open()?);
                                }
                                let player = player.as_mut().unwrap();
                                player
                                    .load(Box::new(source))
                                    .map_err(|error| error.to_string())?;
                                // The new source belongs to this track even if starting output fails.
                                generation += 1;
                                current = Some(Current { id, generation });
                                if thread_quit.load(Ordering::Acquire) {
                                    return Ok(());
                                }
                                player.play().map_err(|error| error.to_string())
                            })(),
                            action if accepts(request.expected, current) => match player.as_mut() {
                                Some(player) => match action {
                                    Action::Toggle => player.status().and_then(|status| {
                                        if status.state == PlaybackState::Playing {
                                            player.pause()
                                        } else {
                                            player.play()
                                        }
                                    }),
                                    Action::Seek(target) => player.seek(target).map(|_| ()),
                                    Action::Stop => player.stop(),
                                    Action::Load(_) => unreachable!(),
                                }
                                .map_err(|error| error.to_string()),
                                None => {
                                    Err("no track loaded; press Enter to play the selected track"
                                        .into())
                                }
                            },
                            _ => Err("the track changed; ignored a stale playback request".into()),
                        };
                        let mut view = lock(&thread_view);
                        view.current = current;
                        if matches!(request.action, Action::Load(_)) {
                            view.pending = None;
                        }
                        view.error = result.err();
                        update_status(player.as_ref(), &mut view);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        let mut view = lock(&thread_view);
                        update_status(player.as_ref(), &mut view);
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            // NativePlayback::drop silences output before joining its decoder worker.
            drop(player);
        });
        Self {
            commands,
            view,
            quit,
            alive,
            thread: Some(thread),
        }
    }

    pub fn view(&self) -> View {
        let mut view = lock(&self.view).clone();
        if !self.alive.load(Ordering::Acquire) && !self.quit.load(Ordering::Acquire) {
            view.error = Some("the playback worker stopped unexpectedly".into());
        }
        view
    }

    fn send(&self, action: Action) -> Result<(), String> {
        let mut view = lock(&self.view);
        if let Action::Load(id) = action {
            if view.pending.is_some() {
                return Err("a track is loading; please wait".into());
            }
            view.pending = Some(id);
        }
        let loading = matches!(action, Action::Load(_));
        if let Err(error) = self.commands.try_send(Request {
            expected: view.current,
            action,
        }) {
            if loading {
                view.pending = None;
            }
            return Err(format!("cannot submit playback operation: {error}"));
        }
        Ok(())
    }

    pub fn load(&self, id: i64) -> Result<(), String> {
        self.send(Action::Load(id))
    }
    pub fn toggle(&self) -> Result<(), String> {
        self.send(Action::Toggle)
    }
    pub fn seek(&self, target: SeekTarget) -> Result<(), String> {
        self.send(Action::Seek(target))
    }
    pub fn stop(&self) -> Result<(), String> {
        self.send(Action::Stop)
    }
}

fn accepts(expected: Option<Current>, current: Option<Current>) -> bool {
    expected == current
}

fn update_status(player: Option<&NativePlayback>, view: &mut View) {
    if let Some(player) = player {
        match player.status() {
            Ok(status) => view.status = Some(status),
            Err(error) => {
                view.status = None;
                view.error = Some(error.to_string());
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.quit.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_failure_preserves_import_and_reports_without_a_current_track() {
        let temporary = tempfile::TempDir::new().unwrap();
        let path = temporary.path().join("library.muse");
        let song = temporary.path().join("song.flac");
        std::fs::write(
            &song,
            include_bytes!("../../muse-library/tests/fixtures/tone.flac"),
        )
        .unwrap();
        let mut library = Library::open(&path).unwrap();
        library.import(&song);
        let id = library.list().unwrap()[0].id;
        let worker = Worker::start_with(path, || Err("injected device failure".into()));
        worker.load(id).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while worker.view().pending.is_some() {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
        assert!(worker.view().current.is_none());
        assert!(
            worker
                .view()
                .error
                .unwrap()
                .contains("injected device failure")
        );
        assert_eq!(library.list().unwrap().len(), 1);
    }

    #[test]
    #[ignore = "requires a physical default output device"]
    fn library_playback_controls_and_stale_requests() {
        fn wait(worker: &Worker, predicate: impl Fn(&View) -> bool) {
            let deadline = std::time::Instant::now() + Duration::from_secs(4);
            loop {
                let view = worker.view();
                assert!(view.error.is_none(), "{view:?}");
                if predicate(&view) {
                    return;
                }
                assert!(std::time::Instant::now() < deadline, "{view:?}");
                thread::sleep(Duration::from_millis(2));
            }
        }
        let temporary = tempfile::TempDir::new().unwrap();
        let path = temporary.path().join("library.muse");
        let song = temporary.path().join("song.flac");
        std::fs::write(
            &song,
            include_bytes!("../../muse-library/tests/fixtures/tone.flac"),
        )
        .unwrap();
        let mut library = Library::open(&path).unwrap();
        library.import(&song);
        let id = library.list().unwrap()[0].id;
        std::fs::remove_file(&song).unwrap();
        drop(library);
        let worker = Worker::start(path);
        worker.load(id).unwrap();
        wait(&worker, |view| {
            view.pending.is_none() && view.current.is_some()
        });
        let old = worker.view().current;
        worker.toggle().unwrap();
        wait(&worker, |view| {
            view.status
                .is_some_and(|status| status.state == PlaybackState::Paused)
        });
        worker
            .seek(SeekTarget::Absolute(Duration::from_millis(400)))
            .unwrap();
        wait(&worker, |view| {
            view.status.is_some_and(|status| {
                status.state == PlaybackState::Paused
                    && status.position >= Duration::from_millis(399)
            })
        });
        worker.load(id).unwrap();
        wait(&worker, |view| {
            view.pending.is_none() && view.current != old
        });
        // Deliver a control from the old generation after the same song was reloaded.
        worker
            .commands
            .try_send(Request {
                expected: old,
                action: Action::Seek(SeekTarget::Forward(Duration::from_secs(10))),
            })
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while worker.view().error.is_none() {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
        assert!(worker.view().error.unwrap().contains("stale"));
        assert!(worker.view().status.unwrap().position < Duration::from_secs(1));
        worker.stop().unwrap();
        // Wait for the recovery command to replace the stale-command message.
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while worker
            .view()
            .status
            .is_none_or(|status| status.state != PlaybackState::Ready)
        {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
        let alive = Arc::clone(&worker.alive);
        drop(worker);
        assert!(!alive.load(Ordering::Acquire));
    }
}
