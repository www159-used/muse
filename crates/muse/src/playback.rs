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

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Current {
    pub id: i64,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RepeatMode {
    #[default]
    Sequential,
    List,
    Track,
}

impl RepeatMode {
    pub fn cycle(self) -> Self {
        match self {
            Self::Sequential => Self::List,
            Self::List => Self::Track,
            Self::Track => Self::Sequential,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sequential => "sequential",
            Self::List => "list loop",
            Self::Track => "single loop",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct View {
    pub current: Option<Current>,
    pub pending: Option<i64>,
    pub status: Option<PlaybackStatus>,
    pub error: Option<String>,
    pub playlist_id: Option<i64>,
    pub playlist_name: String,
    pub playlist_index: Option<u32>,
    pub playlist_len: u32,
    pub mode: RepeatMode,
    /// Linear UI volume, 0–100. Amplitude uses [`muse_playback::perceptual_gain`].
    pub volume: u8,
}

impl Default for View {
    fn default() -> Self {
        Self {
            current: None,
            pending: None,
            status: None,
            error: None,
            playlist_id: None,
            playlist_name: String::new(),
            playlist_index: None,
            playlist_len: 0,
            mode: RepeatMode::Sequential,
            volume: 100,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Action {
    Load(i64),
    PlayPlaylist(i64),
    UsePlaylist(i64),
    Next,
    CycleMode,
    SetMode(RepeatMode),
    Toggle,
    Play,
    Pause,
    Seek(SeekTarget),
    Stop,
    SetVolume(u8),
    IncVolume(u32),
    DecVolume(u32),
}

struct PlaylistSession {
    id: Option<i64>,
    name: String,
    mode: RepeatMode,
    order: Vec<i64>,
    index: Option<usize>,
    finish_handled: bool,
}

impl PlaylistSession {
    fn publish(&self, view: &mut View) {
        view.playlist_id = self.id;
        view.playlist_name.clone_from(&self.name);
        view.playlist_index = self
            .index
            .map(|index| u32::try_from(index).unwrap_or(u32::MAX));
        view.playlist_len = u32::try_from(self.order.len()).unwrap_or(u32::MAX);
        view.mode = self.mode;
    }
}
struct Request {
    expected: Option<Current>,
    action: Action,
    reply: Option<mpsc::Sender<Result<View, String>>>,
}

pub(crate) struct Worker {
    commands: SyncSender<Request>,
    view: Arc<Mutex<View>>,
    quit: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn start(path: PathBuf) -> Result<Self, String> {
        Self::start_with(path, || {
            NativePlayback::open_default().map_err(|error| error.to_string())
        })
    }

    pub(crate) fn start_with<P: Playback + Send + 'static>(
        path: PathBuf,
        open: impl Fn() -> Result<P, String> + Send + 'static,
    ) -> Result<Self, String> {
        let volume = load_saved_volume(&path)?;
        let (commands, receiver) = mpsc::sync_channel::<Request>(16);
        let view = Arc::new(Mutex::new(View {
            volume,
            ..View::default()
        }));
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
            let mut player: Option<P> = None;
            let mut current = None;
            let mut generation = 0_u64;
            let mut volume = volume;
            let mut session = PlaylistSession {
                id: None,
                name: String::new(),
                mode: RepeatMode::Sequential,
                order: Vec::new(),
                index: None,
                finish_handled: false,
            };
            let _ = refresh_order(&path, &mut session);
            macro_rules! task {
                () => {
                    PlaybackTask {
                        path: &path,
                        open: &open,
                        player: &mut player,
                        session: &mut session,
                        current: &mut current,
                        generation: &mut generation,
                        quit: &thread_quit,
                        volume,
                    }
                };
            }
            while !thread_quit.load(Ordering::Acquire) {
                match receiver.recv_timeout(Duration::from_millis(40)) {
                    Ok(request) => {
                        if thread_quit.load(Ordering::Acquire) {
                            break;
                        }
                        let result: Result<(), String> = match request.action {
                            Action::Load(id) => load_track(&mut task!(), id),
                            Action::PlayPlaylist(id) => {
                                let previous = session.id;
                                session.id = Some(id);
                                match refresh_order(&path, &mut session) {
                                    Ok(()) if session.order.is_empty() => {
                                        session.id = previous;
                                        let _ = refresh_order(&path, &mut session);
                                        Err("playlist is empty".into())
                                    }
                                    Ok(()) => {
                                        let first = session.order[0];
                                        load_track(&mut task!(), first)
                                    }
                                    Err(error) => {
                                        session.id = previous;
                                        let _ = refresh_order(&path, &mut session);
                                        Err(error)
                                    }
                                }
                            }
                            Action::UsePlaylist(id) => {
                                let previous = session.id;
                                session.id = Some(id);
                                match refresh_order(&path, &mut session) {
                                    Ok(()) => {
                                        place_index(&mut session, current);
                                        Ok(())
                                    }
                                    Err(error) => {
                                        session.id = previous;
                                        let _ = refresh_order(&path, &mut session);
                                        place_index(&mut session, current);
                                        Err(error)
                                    }
                                }
                            }
                            Action::Next => match advance(&mut task!(), false) {
                                Ok(true) => Ok(()),
                                Ok(false) => Err("end of playlist".into()),
                                Err(error) => Err(error),
                            },
                            Action::CycleMode => {
                                session.mode = session.mode.cycle();
                                Ok(())
                            }
                            Action::SetMode(mode) => {
                                session.mode = mode;
                                Ok(())
                            }
                            Action::SetVolume(requested) => commit_volume(
                                &path,
                                &mut volume,
                                player.as_mut(),
                                requested.min(100),
                            ),
                            Action::IncVolume(steps) => {
                                let next = u32::from(volume).saturating_add(steps).min(100) as u8;
                                commit_volume(&path, &mut volume, player.as_mut(), next)
                            }
                            Action::DecVolume(steps) => {
                                let next = u32::from(volume).saturating_sub(steps) as u8;
                                commit_volume(&path, &mut volume, player.as_mut(), next)
                            }
                            action
                                if matches!(
                                    action,
                                    Action::Stop | Action::Play | Action::Pause
                                ) || accepts(request.expected, current) =>
                            {
                                match player.as_mut() {
                                    Some(player) => match action {
                                        Action::Play => player.play(),
                                        Action::Pause => player.pause(),
                                        Action::Toggle => player.status().and_then(|status| {
                                            if status.state == PlaybackState::Playing {
                                                player.pause()
                                            } else {
                                                player.play()
                                            }
                                        }),
                                        Action::Seek(target) => player.seek(target).map(|_| ()),
                                        Action::Stop => player.stop(),
                                        _ => unreachable!(),
                                    }
                                    .map_err(|error| error.to_string()),
                                    None if matches!(action, Action::Stop | Action::Pause) => {
                                        Ok(())
                                    }
                                    None => Err(
                                        "no track loaded; press Enter to play the selected track"
                                            .into(),
                                    ),
                                }
                            }
                            _ => Err("the track changed; ignored a stale playback request".into()),
                        };
                        let mut view = lock(&thread_view);
                        view.current = current;
                        if matches!(
                            request.action,
                            Action::Load(_) | Action::PlayPlaylist(_) | Action::Next
                        ) {
                            view.pending = None;
                        }
                        view.error = result.as_ref().err().cloned();
                        update_status(player.as_ref(), &mut view, &mut session, volume);
                        if let Some(reply) = request.reply {
                            let _ = reply.send(result.map(|()| view.clone()));
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        let finished = {
                            let mut view = lock(&thread_view);
                            update_status(player.as_ref(), &mut view, &mut session, volume);
                            view.error.is_none()
                                && view
                                    .status
                                    .is_some_and(|status| status.state == PlaybackState::Finished)
                                && !session.finish_handled
                        };
                        if finished {
                            session.finish_handled = true;
                            let result = advance(&mut task!(), true);
                            let mut view = lock(&thread_view);
                            view.current = current;
                            view.pending = None;
                            match result {
                                Ok(true) => view.error = None,
                                Ok(false) => {}
                                Err(error) => view.error = Some(error),
                            }
                            update_status(player.as_ref(), &mut view, &mut session, volume);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            // NativePlayback::drop silences output before joining its decoder worker.
            drop(player);
        });
        Ok(Self {
            commands,
            view,
            quit,
            alive,
            thread: Some(thread),
        })
    }

    pub fn view(&self) -> View {
        let mut view = lock(&self.view).clone();
        if !self.alive.load(Ordering::Acquire) && !self.quit.load(Ordering::Acquire) {
            view.error = Some("the playback worker stopped unexpectedly".into());
        }
        view
    }

    #[cfg(test)]
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
            reply: None,
        }) {
            if loading {
                view.pending = None;
            }
            return Err(format!("cannot submit playback operation: {error}"));
        }
        Ok(())
    }

    /// Completion-based service entry point; a session stop is never generation-scoped.
    pub fn execute(&self, action: Action, expected: Option<Current>) -> Result<View, String> {
        let (reply, result) = mpsc::channel();
        {
            let mut view = lock(&self.view);
            let loading = match action {
                Action::Load(id) | Action::PlayPlaylist(id) => Some(id),
                Action::Next => Some(view.current.map(|current| current.id).unwrap_or(0)),
                _ => None,
            };
            if loading.is_some() {
                if view.pending.is_some() {
                    return Err("a track is loading; please wait".into());
                }
                view.pending = loading;
            }
            if let Err(error) = self.commands.try_send(Request {
                expected,
                action,
                reply: Some(reply),
            }) {
                if loading.is_some() {
                    view.pending = None;
                }
                return Err(format!("cannot submit playback operation: {error}"));
            }
        }
        result.recv_timeout(Duration::from_secs(10)).map_err(|_| {
            "playback outcome unconfirmed; inspect status before retrying".to_string()
        })?
    }

    #[cfg(test)]
    pub fn load(&self, id: i64) -> Result<(), String> {
        self.send(Action::Load(id))
    }
    #[cfg(test)]
    pub fn toggle(&self) -> Result<(), String> {
        self.send(Action::Toggle)
    }
    #[cfg(test)]
    pub fn seek(&self, target: SeekTarget) -> Result<(), String> {
        self.send(Action::Seek(target))
    }
    #[cfg(test)]
    pub fn stop(&self) -> Result<(), String> {
        self.send(Action::Stop)
    }
}

fn accepts(expected: Option<Current>, current: Option<Current>) -> bool {
    expected == current
}

fn load_saved_volume(path: &std::path::Path) -> Result<u8, String> {
    // Missing file means the daemon default (100). Callers that have no live
    // service must not use this as a substitute for a volume reading.
    crate::service::Runtime::existing(path)?
        .ok_or_else(|| "library does not exist".to_string())?
        .load_volume()
}

fn save_volume(path: &std::path::Path, volume: u8) -> Result<(), String> {
    crate::service::Runtime::existing(path)?
        .ok_or_else(|| "library does not exist".to_string())?
        .store_volume(volume)
}

fn commit_volume<P: Playback>(
    path: &std::path::Path,
    volume: &mut u8,
    mut player: Option<&mut P>,
    next: u8,
) -> Result<(), String> {
    if let Some(player) = player.as_mut() {
        player.set_volume(next).map_err(|error| error.to_string())?;
    }
    if let Err(error) = save_volume(path, next) {
        if let Some(player) = player.as_mut() {
            let _ = player.set_volume(*volume);
        }
        return Err(error);
    }
    *volume = next;
    Ok(())
}

fn update_status<P: Playback>(
    player: Option<&P>,
    view: &mut View,
    session: &mut PlaylistSession,
    volume: u8,
) {
    view.volume = volume;
    if let Some(player) = player {
        match player.status() {
            Ok(status) => {
                if status.state != PlaybackState::Finished {
                    session.finish_handled = false;
                }
                view.status = Some(status);
            }
            Err(error) => {
                view.status = None;
                view.error = Some(error.to_string());
            }
        }
    }
    session.publish(view);
}

fn refresh_order(path: &std::path::Path, session: &mut PlaylistSession) -> Result<(), String> {
    let library = Library::open(path).map_err(|error| error.to_string())?;
    if session.id.is_none() {
        let playlist = library.all_local().map_err(|error| error.to_string())?;
        session.id = Some(playlist.id);
    }
    let id = session.id.unwrap();
    let playlist = library.playlist(id).map_err(|error| error.to_string())?;
    session.name = playlist.name;
    session.order = library
        .playlist_tracks(id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|track| track.id)
        .collect();
    Ok(())
}

fn place_index(session: &mut PlaylistSession, current: Option<Current>) {
    session.index =
        current.and_then(|current| session.order.iter().position(|id| *id == current.id));
}

struct PlaybackTask<'a, P, F> {
    path: &'a std::path::Path,
    open: &'a F,
    player: &'a mut Option<P>,
    session: &'a mut PlaylistSession,
    current: &'a mut Option<Current>,
    generation: &'a mut u64,
    quit: &'a AtomicBool,
    volume: u8,
}

fn load_track<P: Playback, F: Fn() -> Result<P, String>>(
    task: &mut PlaybackTask<'_, P, F>,
    id: i64,
) -> Result<(), String> {
    refresh_order(task.path, task.session)?;
    if !task.session.order.contains(&id) {
        return Err(format!("track {id} is not in the current playlist"));
    }
    let source = Library::open(task.path)
        .and_then(|library| library.open_audio(id))
        .map_err(|error| error.to_string())?;
    if task.player.is_none() {
        *task.player = Some((task.open)()?);
        task.player
            .as_mut()
            .unwrap()
            .set_volume(task.volume)
            .map_err(|error| error.to_string())?;
    }
    let player = task.player.as_mut().unwrap();
    player
        .load(Box::new(source))
        .map_err(|error| error.to_string())?;
    // The new source belongs to this track even if starting output fails.
    *task.generation += 1;
    *task.current = Some(Current {
        id,
        generation: *task.generation,
    });
    place_index(task.session, *task.current);
    task.session.finish_handled = false;
    if task.quit.load(Ordering::Acquire) {
        return Ok(());
    }
    player.play().map_err(|error| error.to_string())
}

/// Returns whether another track started. `false` means playback stopped at the playlist end.
fn advance<P: Playback, F: Fn() -> Result<P, String>>(
    task: &mut PlaybackTask<'_, P, F>,
    automatic: bool,
) -> Result<bool, String> {
    refresh_order(task.path, task.session)?;
    place_index(task.session, *task.current);
    if automatic && task.session.mode == RepeatMode::Track {
        match task.player.as_mut() {
            Some(player) => player.play().map_err(|error| error.to_string())?,
            None => {
                return Err("no track loaded; press Enter to play the selected track".into());
            }
        }
        task.session.finish_handled = false;
        return Ok(true);
    }
    let Some(index) = task.session.index else {
        return Ok(false);
    };
    let next = index.saturating_add(1);
    let target = if next < task.session.order.len() {
        Some(task.session.order[next])
    } else if task.session.mode == RepeatMode::List && !task.session.order.is_empty() {
        Some(task.session.order[0])
    } else {
        None
    };
    let Some(target) = target else {
        return Ok(false);
    };
    load_track(task, target)?;
    Ok(true)
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
        let worker =
            Worker::start_with::<NativePlayback>(path, || Err("injected device failure".into()))
                .unwrap();
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
        let worker = Worker::start(path).unwrap();
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
                reply: None,
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

    struct Scripted {
        status: Mutex<PlaybackStatus>,
        finish: Arc<AtomicBool>,
        loads: Arc<Mutex<Vec<usize>>>,
        plays: Arc<std::sync::atomic::AtomicUsize>,
        volume: Arc<std::sync::atomic::AtomicU8>,
    }

    impl Playback for Scripted {
        type Error = std::io::Error;

        fn load(
            &mut self,
            mut source: Box<dyn muse_playback::AudioSource>,
        ) -> Result<(), Self::Error> {
            use std::io::Read;
            let mut bytes = Vec::new();
            source.read_to_end(&mut bytes)?;
            self.loads.lock().unwrap().push(bytes.len());
            *self.status.lock().unwrap() = PlaybackStatus {
                state: PlaybackState::Ready,
                position: Duration::ZERO,
                duration: Some(Duration::from_secs(1)),
            };
            Ok(())
        }

        fn play(&mut self) -> Result<(), Self::Error> {
            let state = self.status.lock().unwrap().state;
            if state == PlaybackState::Empty {
                return Err(std::io::Error::other("no track"));
            }
            self.plays.fetch_add(1, Ordering::AcqRel);
            self.status.lock().unwrap().state = PlaybackState::Playing;
            Ok(())
        }

        fn pause(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        fn stop(&mut self) -> Result<(), Self::Error> {
            let status = self.status.get_mut().unwrap();
            if status.state != PlaybackState::Empty {
                status.state = PlaybackState::Ready;
                status.position = Duration::ZERO;
            }
            Ok(())
        }

        fn seek(&mut self, _target: SeekTarget) -> Result<Duration, Self::Error> {
            Ok(Duration::ZERO)
        }

        fn set_volume(&mut self, volume: u8) -> Result<(), Self::Error> {
            self.volume
                .store(volume, std::sync::atomic::Ordering::Release);
            Ok(())
        }

        fn status(&self) -> Result<PlaybackStatus, Self::Error> {
            let mut status = self.status.lock().unwrap();
            if self.finish.swap(false, Ordering::AcqRel) && status.state == PlaybackState::Playing {
                status.state = PlaybackState::Finished;
                status.position = Duration::from_secs(1);
            }
            Ok(*status)
        }
    }

    fn wait_until(worker: &Worker, predicate: impl Fn(&View) -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            let view = worker.view();
            if predicate(&view) {
                return;
            }
            assert!(std::time::Instant::now() < deadline, "{view:?}");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn playlist_advances_in_order_then_stops_loops_or_repeats_one_track() {
        let temporary = tempfile::TempDir::new().unwrap();
        let path = temporary.path().join("library.muse");
        let mut library = Library::open(&path).unwrap();
        let mp3 = temporary.path().join("song.mp3");
        let flac = temporary.path().join("song.flac");
        std::fs::write(
            &mp3,
            include_bytes!("../../muse-library/tests/fixtures/silence.mp3"),
        )
        .unwrap();
        std::fs::write(
            &flac,
            include_bytes!("../../muse-library/tests/fixtures/tone.flac"),
        )
        .unwrap();
        assert_eq!(library.import(&mp3).imported, 1);
        assert_eq!(library.import(&flac).imported, 1);
        let ids: Vec<_> = library
            .list()
            .unwrap()
            .into_iter()
            .map(|track| track.id)
            .collect();
        let playlist = library.all_local().unwrap().id;
        drop(library);
        let finish = Arc::new(AtomicBool::new(false));
        let loads = Arc::new(Mutex::new(Vec::new()));
        let plays = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let applied = Arc::new(std::sync::atomic::AtomicU8::new(100));
        let (finish_player, loads_player, plays_player, applied_player) = (
            Arc::clone(&finish),
            Arc::clone(&loads),
            Arc::clone(&plays),
            Arc::clone(&applied),
        );
        let worker = Worker::start_with(path.clone(), move || {
            Ok(Scripted {
                status: Mutex::new(PlaybackStatus {
                    state: PlaybackState::Empty,
                    position: Duration::ZERO,
                    duration: None,
                }),
                finish: Arc::clone(&finish_player),
                loads: Arc::clone(&loads_player),
                plays: Arc::clone(&plays_player),
                volume: Arc::clone(&applied_player),
            })
        })
        .unwrap();
        let view = worker
            .execute(Action::PlayPlaylist(playlist), None)
            .unwrap();
        assert_eq!(view.current.unwrap().id, ids[0]);
        assert_eq!(view.mode, RepeatMode::Sequential);
        assert_eq!(view.playlist_index, Some(0));
        assert_eq!(view.playlist_len, 2);
        finish.store(true, Ordering::Release);
        wait_until(&worker, |view| {
            view.current.is_some_and(|current| current.id == ids[1])
        });
        assert_eq!(loads.lock().unwrap().len(), 2);
        finish.store(true, Ordering::Release);
        wait_until(&worker, |view| {
            view.current.is_some_and(|current| current.id == ids[1])
                && view
                    .status
                    .is_some_and(|status| status.state == PlaybackState::Finished)
        });
        thread::sleep(Duration::from_millis(80));
        assert_eq!(
            loads.lock().unwrap().len(),
            2,
            "sequential mode stops at the end"
        );
        worker
            .execute(Action::SetMode(RepeatMode::List), None)
            .unwrap();
        worker.execute(Action::Play, None).unwrap();
        finish.store(true, Ordering::Release);
        wait_until(&worker, |view| {
            view.current.is_some_and(|current| current.id == ids[0])
        });
        let plays_before = plays.load(Ordering::Acquire);
        worker
            .execute(Action::SetMode(RepeatMode::Track), None)
            .unwrap();
        finish.store(true, Ordering::Release);
        wait_until(&worker, |_| plays.load(Ordering::Acquire) > plays_before);
        thread::sleep(Duration::from_millis(80));
        assert_eq!(
            loads.lock().unwrap().len(),
            3,
            "single loop does not load another track"
        );
        assert_eq!(worker.view().current.unwrap().id, ids[0]);
        assert_eq!(worker.view().mode, RepeatMode::Track);
        assert_eq!(worker.view().volume, 100);
        worker.execute(Action::SetVolume(50), None).unwrap();
        assert_eq!(worker.view().volume, 50);
        assert_eq!(applied.load(Ordering::Acquire), 50);
        worker.execute(Action::IncVolume(60), None).unwrap();
        assert_eq!(worker.view().volume, 100);
        assert_eq!(applied.load(Ordering::Acquire), 100);
        worker.execute(Action::DecVolume(1_000), None).unwrap();
        assert_eq!(worker.view().volume, 0);
        assert_eq!(applied.load(Ordering::Acquire), 0);
        let settings = crate::service::Runtime::existing(&path)
            .unwrap()
            .unwrap()
            .settings_path();
        assert!(
            std::fs::read_to_string(&settings)
                .unwrap()
                .contains("\"volume\":0")
        );
        drop(worker);
        let reloaded = Worker::start_with(path, || {
            Ok(Scripted {
                status: Mutex::new(PlaybackStatus {
                    state: PlaybackState::Empty,
                    position: Duration::ZERO,
                    duration: None,
                }),
                finish: Arc::new(AtomicBool::new(false)),
                loads: Arc::new(Mutex::new(Vec::new())),
                plays: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                volume: Arc::new(std::sync::atomic::AtomicU8::new(100)),
            })
        })
        .unwrap();
        assert_eq!(reloaded.view().volume, 0);
        drop(reloaded);
        std::fs::remove_file(settings).unwrap();
    }
}
