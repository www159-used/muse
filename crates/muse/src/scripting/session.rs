// SPDX-License-Identifier: AGPL-3.0-only

//! The serialized script session.
//!
//! One session owns an engine, the session scope and the compiled definitions of
//! every accepted submission. Submissions are evaluated one at a time on the
//! worker thread, so the render loop never touches the engine.
//!
//! The session follows the same retention rule as the reference Rhai REPL: each
//! submission is compiled on its own, merged into the accumulated definitions and
//! then evaluated; afterwards its statements are dropped again so only function
//! definitions survive into later submissions. Variables live in the session
//! scope, which is why a function defined in one submission stays callable in a
//! later one while a statement never runs twice.

use rhai::{AST, Dynamic, Engine, Scope};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use super::host::Host;
use crate::scripting::api::{self, Unit};

/// How many Rhai operations one submission may perform before it is stopped.
const MAX_OPERATIONS: u64 = 1_000_000;

/// How deeply one submission may nest function calls before it is stopped.
const MAX_CALL_LEVELS: usize = 64;

/// Result of evaluating one submission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Evaluation {
    /// The script produced a value, already formatted for display.
    ///
    /// An empty value means the script only performed statements.
    Value(String),
    /// The script failed to compile or run.
    ///
    /// The message is in English and mentions the failure without promising that
    /// earlier effects were undone.
    Error(String),
}

/// One serialized scripting session.
pub struct Session {
    engine: Engine,
    scope: Scope<'static>,
    definitions: AST,
    output: Arc<Mutex<String>>,
    cancelled: Arc<AtomicBool>,
}

impl Session {
    /// Create a session with every application API registered.
    pub fn new() -> Self {
        Self::with_host(Host::default())
    }

    pub fn with_host(host: Host) -> Self {
        Self::with_host_and_cancel(host, Arc::new(AtomicBool::new(false)))
    }

    pub fn with_host_and_cancel(host: Host, cancelled: Arc<AtomicBool>) -> Self {
        let mut engine = Engine::new();
        // A runaway script must not be able to hang the worker forever; the UI
        // stays responsive because it never evaluates scripts itself.
        engine.set_max_operations(MAX_OPERATIONS);
        engine.set_max_call_levels(MAX_CALL_LEVELS);
        let progress_cancelled = cancelled.clone();
        engine.on_progress(move |_| {
            progress_cancelled
                .load(Ordering::Acquire)
                .then(|| Dynamic::from("Script cancelled"))
        });
        let output = Arc::new(Mutex::new(String::new()));
        let printed = Arc::clone(&output);
        engine.on_print(move |line| append_output(&printed, line));
        let debugged = Arc::clone(&output);
        engine.on_debug(move |line, _, _| append_output(&debugged, line));
        let catalog = api::install_with_host(&mut engine, &host);
        debug_assert_eq!(
            api::verify_parity(&engine, &catalog),
            Ok(()),
            "the scripting catalog and the engine registration must agree"
        );
        Self {
            engine,
            scope: Scope::new(),
            definitions: AST::empty(),
            output,
            cancelled,
        }
    }

    /// Take the output captured during an evaluation, if any.
    fn take_output(&self) -> String {
        std::mem::take(
            &mut *self
                .output
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    /// Evaluate one submission and report its outcome.
    ///
    /// A compile failure publishes nothing: the accumulated definitions and the
    /// session scope are left untouched, so a rejected submission can never
    /// change what a later submission can call. A runtime failure keeps the
    /// effects that already happened and the definitions the submission
    /// contributed before it failed, because earlier statements are never
    /// replayed on a later submission.
    pub fn evaluate(&mut self, input: &str) -> Evaluation {
        self.take_output();
        let compiled = match self.engine.compile_with_scope(&self.scope, input) {
            Ok(compiled) => compiled,
            Err(error) => return Evaluation::Error(error_message(&error)),
        };
        self.definitions += compiled.clone();
        let evaluated = self
            .engine
            .eval_ast_with_scope::<Dynamic>(&mut self.scope, &self.definitions);
        // Keep the definitions, drop the statements: later submissions must not
        // run this one again.
        self.definitions.clear_statements();
        let result = match evaluated {
            Ok(value) if value.is::<()>() => Evaluation::Value(String::new()),
            Ok(value) => Evaluation::Value(display_value(&value)),
            Err(_) if self.cancelled.load(Ordering::Acquire) => {
                Evaluation::Error("Script error: Script cancelled".into())
            }
            Err(error) => Evaluation::Error(error_message(&error)),
        };
        let output = self.take_output();
        if output.is_empty() {
            return result;
        }
        match result {
            Evaluation::Value(value) if value.is_empty() => Evaluation::Value(output),
            Evaluation::Value(value) => Evaluation::Value(format!("{output}\n{value}")),
            Evaluation::Error(error) => Evaluation::Error(format!("{output}\n{error}")),
        }
    }
}

/// Append one captured output line, keeping the reported text bounded.
fn append_output(output: &Mutex<String>, line: &str) {
    const MAX_OUTPUT: usize = 8192;
    let mut output = output
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if output.len() >= MAX_OUTPUT {
        return;
    }
    if !output.is_empty() {
        output.push('\n');
    }
    let available = MAX_OUTPUT.saturating_sub(output.len());
    let end = line
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= available)
        .last()
        .unwrap_or(0);
    let end = if line.len() <= available {
        line.len()
    } else {
        end
    };
    output.push_str(&line[..end]);
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// Format an evaluated value for the status line.
fn display_value(value: &Dynamic) -> String {
    value.clone().try_cast::<Unit>().map_or_else(
        || value.to_string(),
        |unit| format!("unit::{}", unit.label()),
    )
}

/// Render a Rhai compile or runtime error as an English, user-facing message.
fn error_message(error: impl std::fmt::Display) -> String {
    format!("Script error: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session::new()
    }

    #[test]
    fn print_and_debug_are_returned_with_the_evaluation() {
        let mut session = session();
        let result = value(session.evaluate("print(\"hello\"); debug(42);"));
        assert!(result.contains("hello"));
        assert!(result.contains("42"));
        assert_eq!(value(session.evaluate("1 + 1")), "2");
    }

    #[test]
    fn cancellation_stops_a_running_script() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = cancelled.clone();
        let mut session = Session::with_host_and_cancel(Host::default(), cancelled);
        signal.store(true, Ordering::Release);
        let result = session.evaluate("while true {}");
        assert!(
            matches!(&result, Evaluation::Error(error) if error.contains("Script cancelled")),
            "{result:?}"
        );
    }

    #[test]
    fn host_calls_without_a_service_report_an_error() {
        let mut session = session();
        assert!(
            matches!(session.evaluate("play()"), Evaluation::Error(error) if error.contains("playback service is unavailable"))
        );
    }

    fn value(evaluation: Evaluation) -> String {
        match evaluation {
            Evaluation::Value(value) => value,
            Evaluation::Error(error) => panic!("expected a value, got error: {error}"),
        }
    }

    fn error(evaluation: Evaluation) -> String {
        match evaluation {
            Evaluation::Error(error) => error,
            Evaluation::Value(value) => panic!("expected an error, got value: {value}"),
        }
    }

    fn is_error(evaluation: Evaluation) -> bool {
        matches!(evaluation, Evaluation::Error(_))
    }

    #[test]
    fn expressions_report_their_value() {
        let mut session = session();
        assert_eq!(value(session.evaluate("1 + 1")), "2");
        assert_eq!(value(session.evaluate("2 * 21")), "42");
    }

    #[test]
    fn defined_functions_stay_callable_in_later_submissions() {
        let mut session = session();
        assert_eq!(value(session.evaluate("fn twice(x) { x * 2 }")), "");
        assert_eq!(value(session.evaluate("twice(21)")), "42");
    }

    #[test]
    fn session_variables_survive_between_submissions() {
        let mut session = session();
        assert_eq!(value(session.evaluate("let base = 41;")), "");
        assert_eq!(value(session.evaluate("base + 1")), "42");
    }

    #[test]
    fn statements_do_not_run_again_in_later_submissions() {
        let mut session = session();
        assert_eq!(value(session.evaluate("let count = 0; count += 1;")), "");
        assert_eq!(value(session.evaluate("count")), "1");
        assert_eq!(value(session.evaluate("count += 10;")), "");
        assert_eq!(value(session.evaluate("count")), "11");
    }

    #[test]
    fn a_parse_failure_publishes_no_definitions() {
        let mut session = session();
        // A parse failure leaves the session exactly as it was.
        let failure = error(session.evaluate("fn broken( {"));
        assert!(
            failure.starts_with("Script error:"),
            "unexpected message: {failure}"
        );
        assert!(is_error(session.evaluate("broken()")));
        assert!(error(session.evaluate("missing_variable")).starts_with("Script error:"));
        // Earlier submissions still work after a rejected one.
        assert_eq!(value(session.evaluate("fn keep() { 7 }")), "");
        assert!(is_error(session.evaluate("broken( {")));
        assert_eq!(value(session.evaluate("keep()")), "7");
    }

    #[test]
    fn a_runtime_failure_keeps_definitions_made_before_it() {
        let mut session = session();
        assert_eq!(value(session.evaluate("fn keep() { 7 }")), "");
        let failure = error(session.evaluate("keep(); unknown_operation();"));
        assert!(
            failure.starts_with("Script error:"),
            "unexpected message: {failure}"
        );
        assert_eq!(value(session.evaluate("keep()")), "7");
    }

    #[test]
    fn unit_values_are_reported_with_their_qualified_name() {
        let mut session = session();
        assert_eq!(value(session.evaluate("unit::SEC")), "unit::SEC");
    }

    #[test]
    fn registered_functions_are_available_without_a_language_prefix() {
        let mut session = session();
        assert_eq!(
            value(session.evaluate("version()")),
            format!("muse {}", env!("CARGO_PKG_VERSION"))
        );
    }
}
