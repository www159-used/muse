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
type Reply = SyncSender<Completed>;

struct Completed {
    expected: Option<Current>,
    result: Result<View, String>,
}

struct Request {
    command: Command,
    expected: Option<Current>,
    reply: Option<Reply>,
}

pub(crate) struct Client {
    requests: SyncSender<Request>,
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
                let Request {
                    command,
                    expected,
                    reply,
                } = match request {
                    Ok(request) => request,
                    Err(_) if thread_exit.load(Ordering::Acquire) != 0 => {
                        *lock(&thread_finished) = Some(Ok(()));
                        break;
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => Request {
                        command: Command::Status,
                        expected: None,
                        reply: None,
                    },
                };
                let result = remote.call(command, expected);
                match &result {
                    Ok(snapshot) => *lock(&thread_view) = snapshot.clone(),
                    Err(error) => {
                        let mut view = lock(&thread_view);
                        view.pending = None;
                        view.error = Some(error.clone());
                    }
                }
                if let Some(reply) = reply {
                    let _ = reply.try_send(Completed { expected, result });
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

    /// Adopt a fresher snapshot (e.g. from a synchronous colon transport call)
    /// so the next `view()` poll does not roll the UI back to a stale state.
    pub fn apply(&self, view: View) {
        *lock(&self.view) = view;
    }

    fn submit(&self, command: Command, reply: Option<Reply>) -> Result<Option<Current>, String> {
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
        let expected = view.current;
        if let Err(error) = self.requests.try_send(Request {
            command,
            expected,
            reply,
        }) {
            if loads {
                view.pending = None;
            }
            return Err(format!("cannot submit playback operation: {error}"));
        }
        Ok(expected)
    }

    fn send(&self, command: Command) -> Result<(), String> {
        self.submit(command, None).map(|_| ())
    }

    /// Wait for the matching service response, not merely queue acceptance.
    pub fn call(&self, command: Command) -> Result<View, String> {
        self.call_with_timeout(command, Duration::from_secs(15))
    }

    fn call_with_timeout(&self, command: Command, timeout: Duration) -> Result<View, String> {
        let (reply, result) = mpsc::sync_channel(1);
        let expected_for_debug = self.submit(command, Some(reply))?;
        let completed = result.recv_timeout(timeout).map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => {
                "playback outcome unconfirmed after timeout; inspect status before retrying"
                    .to_owned()
            }
            mpsc::RecvTimeoutError::Disconnected => {
                "playback operation cancelled before completion".to_owned()
            }
        })?;
        if completed.expected != expected_for_debug {
            return Err("playback completion identity mismatch".into());
        }
        completed.result
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn fake_client() -> (Arc<Client>, mpsc::Receiver<Request>) {
        let (requests, received) = mpsc::sync_channel(4);
        let view = View {
            current: Some(Current {
                id: 7,
                generation: 3,
            }),
            ..View::default()
        };
        (
            Arc::new(Client {
                requests,
                view: Arc::new(Mutex::new(view)),
                exit: Arc::new(AtomicU8::new(0)),
                finished: Arc::new(Mutex::new(None)),
            }),
            received,
        )
    }

    #[test]
    fn call_waits_for_matching_completion_and_carries_identity() {
        let (client, received) = fake_client();
        let responder = thread::spawn(move || {
            let request = received.recv().unwrap();
            assert!(matches!(request.command, Command::Pause));
            assert_eq!(
                request.expected,
                Some(Current {
                    id: 7,
                    generation: 3
                })
            );
            thread::sleep(Duration::from_millis(20));
            request
                .reply
                .unwrap()
                .send(Completed {
                    expected: request.expected,
                    result: Err("stale playback request".into()),
                })
                .unwrap();
        });
        let start = Instant::now();
        assert_eq!(
            client.call(Command::Pause).unwrap_err(),
            "stale playback request"
        );
        assert!(start.elapsed() >= Duration::from_millis(20));
        responder.join().unwrap();
    }

    #[test]
    fn mismatched_completion_identity_is_rejected() {
        let (client, received) = fake_client();
        let responder = thread::spawn(move || {
            let request = received.recv().unwrap();
            request
                .reply
                .unwrap()
                .send(Completed {
                    expected: Some(Current {
                        id: 8,
                        generation: 4,
                    }),
                    result: Ok(View::default()),
                })
                .unwrap();
        });
        assert!(
            client
                .call(Command::Pause)
                .unwrap_err()
                .contains("identity mismatch")
        );
        responder.join().unwrap();
    }

    #[test]
    fn exiting_client_rejects_new_host_calls_without_queueing() {
        let (client, received) = fake_client();
        client.exit(true);
        assert!(client.call(Command::Pause).unwrap_err().contains("exiting"));
        assert!(received.try_recv().is_err());
    }

    #[test]
    fn unavailable_completion_returns_a_bounded_unconfirmed_error() {
        let (client, received) = fake_client();
        let _held_request = thread::spawn(move || {
            let request = received.recv().unwrap();
            thread::sleep(Duration::from_millis(50));
            drop(request);
        });
        let start = Instant::now();
        let error = client
            .call_with_timeout(Command::Pause, Duration::from_millis(10))
            .unwrap_err();
        assert!(error.contains("outcome unconfirmed"), "{error}");
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
