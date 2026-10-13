// SPDX-License-Identifier: AGPL-3.0-only

use super::{Command, Remote};
use crate::playback::{Current, View};
use muse_playback::SeekTarget;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::Duration,
};

type Completion = Arc<Mutex<Option<Result<(), String>>>>;

pub(crate) struct Client {
    requests: SyncSender<(Command, Option<Current>)>,
    view: Arc<Mutex<View>>,
    exit: Arc<AtomicU8>,
    finished: Completion,
}

fn lock<T>(value: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    value.lock().unwrap_or_else(|e| e.into_inner())
}

impl Client {
    pub fn start(remote: Remote) -> Result<Self, String> {
        let view = Arc::new(Mutex::new(remote.call(Command::Status, None)?));
        let exit = Arc::new(AtomicU8::new(0));
        let finished = Arc::new(Mutex::new(None));
        let (requests, receiver) = mpsc::sync_channel(16);
        let (thread_view, thread_exit, thread_finished) =
            (view.clone(), exit.clone(), finished.clone());
        thread::spawn(move || {
            loop {
                if thread_exit.load(Ordering::Acquire) == 2 {
                    let result = remote
                        .call(Command::Stop, None)
                        .map(|view| *lock(&thread_view) = view);
                    *lock(&thread_finished) = Some(result);
                    break;
                }
                let request = receiver.recv_timeout(Duration::from_millis(100));
                if thread_exit.load(Ordering::Acquire) == 2 {
                    continue;
                }
                let (command, expected) = match request {
                    Ok(request) => request,
                    Err(_) if thread_exit.load(Ordering::Acquire) != 0 => {
                        *lock(&thread_finished) = Some(Ok(()));
                        break;
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => (Command::Status, None),
                };
                match remote.call(command, expected) {
                    Ok(snapshot) => *lock(&thread_view) = snapshot,
                    Err(error) => {
                        let mut view = lock(&thread_view);
                        view.pending = None;
                        view.error = Some(error);
                    }
                }
            }
        });
        Ok(Self {
            requests,
            view,
            exit,
            finished,
        })
    }

    pub fn view(&self) -> View {
        lock(&self.view).clone()
    }

    fn send(&self, command: Command) -> Result<(), String> {
        if self.exit.load(Ordering::Acquire) != 0 {
            return Err("terminal client is exiting".into());
        }
        let mut view = lock(&self.view);
        let loads = command.loads();
        if loads {
            if view.pending.is_some() {
                return Err("a track is loading; please wait".into());
            }
            view.pending = Some(match command {
                Command::Load(id) | Command::PlayPlaylist(id) => id,
                _ => view.current.map(|current| current.id).unwrap_or(0),
            });
        }
        if let Err(error) = self.requests.try_send((command, view.current)) {
            if loads {
                view.pending = None;
            }
            return Err(format!("cannot submit playback operation: {error}"));
        }
        Ok(())
    }

    pub fn load(&self, id: i64) -> Result<(), String> {
        self.send(Command::Load(id))
    }
    pub fn play_playlist(&self, id: i64) -> Result<(), String> {
        self.send(Command::PlayPlaylist(id))
    }
    pub fn use_playlist(&self, id: i64) -> Result<(), String> {
        self.send(Command::UsePlaylist(id))
    }
    pub fn cycle_mode(&self) -> Result<(), String> {
        self.send(Command::CycleMode)
    }
    pub fn play_next_once(&self, id: i64) -> Result<(), String> {
        self.send(Command::PlayNextOnce(id))
    }
    pub fn toggle(&self) -> Result<(), String> {
        self.send(Command::Toggle)
    }
    pub fn stop(&self) -> Result<(), String> {
        self.send(Command::Stop)
    }
    pub fn seek(&self, target: SeekTarget) -> Result<(), String> {
        let (direction, duration) = match target {
            SeekTarget::Absolute(d) => (0, d),
            SeekTarget::Backward(d) => (-1, d),
            SeekTarget::Forward(d) => (1, d),
        };
        self.send(Command::Seek {
            direction,
            millis: duration.as_millis().min(u64::MAX as u128) as u64,
        })
    }
    pub fn exit(&self, detach: bool) {
        self.exit
            .store(if detach { 1 } else { 2 }, Ordering::Release);
    }
    pub fn completion(&self) -> Option<Result<(), String>> {
        lock(&self.finished).clone()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // Dropping a transport must never implicitly stop a shared playback session.
        let _ = self
            .exit
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
    }
}
