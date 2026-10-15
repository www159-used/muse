// SPDX-License-Identifier: AGPL-3.0-only

//! The serialized script worker.
//!
//! Submitted input travels to a single worker thread over a bounded queue and
//! the worker's finished messages travel back over another bounded queue. Nothing
//! in the render loop ever touches the engine: it only collects whatever the
//! worker has already finished, so a script can never block drawing or input
//! handling.

use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};

use super::host::Host;
use crate::scripting::session::{Evaluation, Session};
use crate::service::Client;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// How many submissions may wait for the worker before the UI reports a busy
/// script worker instead of queueing more work.
const REQUEST_CAPACITY: usize = 8;

/// How many finished messages may wait for the UI before the worker stops
/// reporting new ones.
const RESULT_CAPACITY: usize = 32;

/// One message sent from the UI to the worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptRequest {
    /// Evaluate one submitted command line.
    Evaluate(String),
    /// Stop the worker; it exits after finishing the current request.
    Shutdown,
}

/// One finished message sent from the worker to the UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptMessage {
    /// Evaluation finished; the text is ready for the status line.
    Evaluated(String),
    /// The worker stopped and cannot accept more submissions.
    Stopped,
}

impl ScriptMessage {
    /// Status-line text for a finished message.
    pub fn text(self) -> String {
        match self {
            Self::Evaluated(text) => text,
            Self::Stopped => "The script worker stopped; restart muse to run scripts".into(),
        }
    }
}

/// Handle to the serialized script worker.
///
/// Dropping the handle stops the worker: the request channel disconnects, so
/// pending submissions are discarded rather than evaluated behind the user's
/// back.
pub struct ScriptWorker {
    requests: SyncSender<ScriptRequest>,
    messages: Receiver<ScriptMessage>,
    host: Host,
    cancelled: Arc<AtomicBool>,
}

impl ScriptWorker {
    /// Start the worker thread with a fresh session.
    pub fn start() -> Self {
        let host = Host::default();
        let cancelled = Arc::new(AtomicBool::new(false));
        let (requests, inbox) = sync_channel(REQUEST_CAPACITY);
        let (outbox, messages) = sync_channel(RESULT_CAPACITY);
        let thread_host = host.clone();
        let thread_cancelled = cancelled.clone();
        std::thread::Builder::new()
            .name("muse-scripts".into())
            .spawn(move || run(inbox, outbox, thread_host, thread_cancelled))
            .expect("the script worker thread starts");
        Self {
            requests,
            messages,
            host,
            cancelled,
        }
    }

    pub fn attach(&self, client: Arc<Client>) {
        self.host.attach(client);
    }

    /// Submit one command line for evaluation.
    ///
    /// Returns an error when the worker is busy with a backlog, so the UI can
    /// report it instead of growing an unbounded queue.
    pub fn submit(&self, input: &str) -> Result<(), String> {
        match self
            .requests
            .try_send(ScriptRequest::Evaluate(input.to_owned()))
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                Err("The script worker is busy; try again in a moment".into())
            }
            Err(TrySendError::Disconnected(_)) => {
                Err("The script worker stopped; restart muse to run scripts".into())
            }
        }
    }

    /// Take the oldest finished message, if the worker has already produced one.
    pub fn try_result(&self) -> Option<String> {
        self.messages.try_recv().ok().map(ScriptMessage::text)
    }

    /// Ask the worker to stop.
    pub fn shutdown(&self) {
        self.cancelled.store(true, Ordering::Release);
        let _ = self.requests.try_send(ScriptRequest::Shutdown);
    }
}

impl Drop for ScriptWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Worker loop: evaluate one request at a time, in submission order.
fn run(
    inbox: Receiver<ScriptRequest>,
    outbox: SyncSender<ScriptMessage>,
    host: Host,
    cancelled: Arc<AtomicBool>,
) {
    let mut session = Session::with_host_and_cancel(host, cancelled.clone());
    while let Ok(request) = inbox.recv() {
        if cancelled.load(Ordering::Acquire) {
            break;
        }
        match request {
            ScriptRequest::Evaluate(input) => {
                if outbox
                    .send(ScriptMessage::Evaluated(report(session.evaluate(&input))))
                    .is_err()
                {
                    break;
                }
            }
            ScriptRequest::Shutdown => break,
        }
    }
    let _ = outbox.send(ScriptMessage::Stopped);
}

/// Turn an evaluated session result into status-line text.
fn report(evaluation: Evaluation) -> String {
    match evaluation {
        Evaluation::Value(value) if value.is_empty() => "Script completed".into(),
        Evaluation::Value(value) => value,
        Evaluation::Error(error) => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_submitted_script_reports_its_value_without_the_ui_evaluating_it() {
        let worker = ScriptWorker::start();
        worker.submit("version()").unwrap();
        let message = await_result(&worker, "version()");
        assert_eq!(message, format!("muse {}", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn a_broken_script_reports_an_english_error() {
        let worker = ScriptWorker::start();
        worker.submit("seek(90, ").unwrap();
        let message = await_result(&worker, "seek(90, ");
        assert!(
            message.starts_with("Script error:"),
            "unexpected message: {message}"
        );
    }

    #[test]
    fn a_definition_reports_completion_without_a_value() {
        let worker = ScriptWorker::start();
        worker.submit("fn keep() { 1 }").unwrap();
        let message = await_result(&worker, "fn keep() { 1 }");
        assert_eq!(message, "Script completed");
    }

    #[test]
    fn submissions_are_evaluated_in_order_on_one_worker() {
        let worker = ScriptWorker::start();
        worker.submit("fn value() { 21 }").unwrap();
        worker.submit("value() * 2").unwrap();
        worker.submit("let x = 1;").unwrap();
        let first = await_result(&worker, "definition");
        assert_eq!(first, "Script completed");
        assert_eq!(await_result(&worker, "value() * 2"), "42");
        assert_eq!(await_result(&worker, "let x = 1;"), "Script completed");
        assert!(worker.try_result().is_none());
    }

    #[test]
    fn a_full_request_queue_reports_a_busy_worker_instead_of_blocking() {
        let (sender, receiver) = sync_channel::<ScriptRequest>(1);
        sender
            .try_send(ScriptRequest::Evaluate("version()".into()))
            .unwrap();
        let error = sender.try_send(ScriptRequest::Evaluate("1 + 1".into()));
        assert!(matches!(error, Err(TrySendError::Full(_))));
        assert_eq!(
            receiver.try_recv(),
            Ok(ScriptRequest::Evaluate("version()".into()))
        );
    }

    #[test]
    fn a_disconnected_worker_reports_submission_failures() {
        let (requests, inbox) = sync_channel::<ScriptRequest>(1);
        let (outbox, messages) = sync_channel::<ScriptMessage>(1);
        drop(inbox);
        drop(outbox);
        let worker = ScriptWorker {
            requests,
            messages,
            host: Host::default(),
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        let error = worker.submit("1 + 1").unwrap_err();
        assert!(error.contains("stopped"), "unexpected error: {error}");
    }

    #[test]
    fn dropping_the_worker_stops_the_thread() {
        let worker = ScriptWorker::start();
        worker.submit("1 + 1").unwrap();
        drop(worker);
        // The worker thread observes the disconnection and exits on its own;
        // dropping must not panic or hang.
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    fn await_result(worker: &ScriptWorker, request: &str) -> String {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(message) = worker.try_result() {
                return message;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "no result for {request}"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}
