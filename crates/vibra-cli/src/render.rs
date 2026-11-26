//! Human and JSON rendering of one response.
//!
//! `docs/spec/05-tooling.md`, "Streams": in JSON format the tool writes
//! exactly one envelope to standard output and its logs to standard error. In
//! human format the usage text, a format preview, and the test report go to
//! standard output, and each diagnostic is one line on standard error.

use std::collections::BTreeMap;
use std::io::Write;

use vibra_diagnostics::{Diagnostic, LineIndex};
use vibra_schema::{
    CommandOutcome, DiagnosticDocument, Envelope, Limits, Payload, SchemaViolation,
    Status, TestPayload, TestResult,
};

use vibra_workspace::library_source;

use crate::args::OutputFormat;
use crate::exit_code;

/// The identifier of the limit profile this tool publishes.
pub const LIMIT_PROFILE: &str = "m1Default";

/// The result of one command, before it is rendered.
#[derive(Clone, Debug)]
pub struct Response {
    /// The output format of the response.
    pub format: OutputFormat,
    /// The recognized command name, or the empty string.
    pub command: String,
    /// The command result.
    pub result: CommandOutcome,
    /// The status of the observation.
    pub status: Status,
    /// The workspace revision, `None` when no snapshot was taken.
    pub workspace_revision: Option<String>,
    /// The diagnostics, once each.
    pub diagnostics: Vec<Diagnostic>,
    /// The text of each captured source a diagnostic names, by source
    /// identity, from which display positions are derived. An embedded
    /// library module is not listed: its text is the toolchain's.
    pub sources: BTreeMap<String, String>,
    /// The payload.
    pub payload: Payload,
    /// A line for a person when the response carries no diagnostic to show.
    pub notice: Option<String>,
    /// The lines of the tool's log, written to standard error in either
    /// format (05 "Streams").
    pub log: Vec<String>,
    /// The limit profile, the limits reached, and whether the observation is
    /// complete.
    pub limits: Limits,
}

impl Response {
    /// A response with the status `result` fixes, no snapshot, no
    /// diagnostic, and no notice.
    #[must_use]
    pub fn new(
        format: OutputFormat,
        command: impl Into<String>,
        result: CommandOutcome,
        payload: Payload,
    ) -> Self {
        Self {
            format,
            command: command.into(),
            result,
            status: status_for(result),
            workspace_revision: None,
            diagnostics: Vec::new(),
            sources: BTreeMap::new(),
            payload,
            notice: None,
            log: Vec::new(),
            limits: limits(),
        }
    }
}

/// The status an envelope reports for `result`.
///
/// The specification fixes two pairs: help that succeeds is `passed`, and an
/// unavailable command is `unsupported`. It states no status for the other
/// results; a response that established nothing is reported `unknown`.
#[must_use]
pub const fn status_for(result: CommandOutcome) -> Status {
    match result {
        CommandOutcome::Ok => Status::Passed,
        CommandOutcome::Diagnostics | CommandOutcome::TestFailed => Status::Refuted,
        CommandOutcome::Unavailable => Status::Unsupported,
        CommandOutcome::InvalidInput
        | CommandOutcome::OperationalFailure
        | CommandOutcome::Trap
        | CommandOutcome::ProgramFailed => Status::Unknown,
    }
}

fn limits() -> Limits {
    Limits {
        profile: LIMIT_PROFILE.to_owned(),
        reached: Vec::new(),
        complete: true,
    }
}

/// Builds the envelope of `response`.
///
/// # Errors
///
/// Fails when a diagnostic cannot be rendered.
fn envelope(response: &Response) -> Result<Envelope, String> {
    let mut envelope = Envelope::new(
        env!("CARGO_PKG_VERSION"),
        response.command.clone(),
        response.status,
        response.result,
        response.limits.clone(),
    );
    envelope
        .workspace_revision
        .clone_from(&response.workspace_revision);
    let mut indexes = response
        .sources
        .iter()
        .map(|(source_id, text)| (source_id.clone(), LineIndex::new(text.as_str())))
        .collect::<BTreeMap<_, _>>();
    // A span in an embedded library module is rendered against that module's
    // text, named by its module atom.
    for source_id in response.diagnostics.iter().flat_map(|diagnostic| {
        diagnostic.source_id().into_iter().chain(
            diagnostic
                .related()
                .iter()
                .filter_map(|related| related.source_id.as_deref()),
        )
    }) {
        if !indexes.contains_key(source_id)
            && let Some(text) = library_source(source_id)
        {
            indexes.insert(source_id.to_owned(), LineIndex::new(text));
        }
    }
    // A diagnostic with no location, or naming a source that was never read,
    // is at offset 0 of the empty document.
    let empty = LineIndex::new("");
    for diagnostic in &response.diagnostics {
        let index = diagnostic
            .source_id()
            .and_then(|source_id| indexes.get(source_id))
            .unwrap_or(&empty);
        envelope.diagnostics.push(
            DiagnosticDocument::render_with_source_indexes(diagnostic, index, &indexes)
                .map_err(|error| error.to_string())?,
        );
    }
    envelope.payload = response.payload.clone();
    Ok(envelope)
}

/// The text of the source source_id: a captured project document, or an
/// embedded library module named by its module atom; empty for a source
/// that was never read.
pub(crate) fn source_text<'r>(response: &'r Response, source_id: &str) -> &'r str {
    response
        .sources
        .get(source_id)
        .map(String::as_str)
        .or_else(|| library_source(source_id))
        .unwrap_or("")
}

/// One line per diagnostic on standard error, with its location when it has
/// one.
fn write_diagnostics<E: Write>(response: &Response, stderr: &mut E) {
    for diagnostic in &response.diagnostics {
        let location = diagnostic.source_id().map(|source_id| {
            let text = source_text(response, source_id);
            let position =
                LineIndex::new(text).position(diagnostic.primary_span().start());
            format!("{source_id}:{}:{}: ", position.line, position.column)
        });
        let _ = writeln!(
            stderr,
            "{}{} {}: {}",
            location.unwrap_or_default(),
            diagnostic.level().as_atom(),
            diagnostic.code().as_atom(),
            diagnostic.message()
        );
    }
}

/// The human test report: one line per item with its selector and result
/// atom; under each item that did not pass, its diagnostics, its failure, or
/// its trap; and the passed, failed, and selected counts on the last line.
fn write_test_report<W: Write>(test: &TestPayload, stdout: &mut W) {
    for item in &test.tests {
        let _ = writeln!(stdout, "{} {}", item.name, item.result.as_atom());
        if item.result == TestResult::Passed {
            continue;
        }
        for code in &item.diagnostics {
            let _ = writeln!(stdout, "  diagnostic: {code}");
        }
        if let Some(failure) = &item.failure {
            let span = &failure.primary_span;
            let location = format!(
                "{}:{}:{}",
                span.source_id.as_deref().unwrap_or("<unknown>"),
                span.start_position.line,
                span.start_position.column
            );
            let assertion = failure.assertion.as_deref().unwrap_or("an assertion");
            let _ = match (
                &failure.observation_error,
                &failure.expected,
                &failure.actual,
            ) {
                (Some(_), _, _) => writeln!(
                    stdout,
                    "  failure: {assertion} could not observe an operand at {location}"
                ),
                (None, Some(expected), Some(actual)) => writeln!(
                    stdout,
                    "  failure: {assertion} expected {expected}, actual {actual} at {location}"
                ),
                (None, _, _) => {
                    writeln!(stdout, "  failure: {assertion} is false at {location}")
                }
            };
        }
        if let Some(trap) = &item.trap {
            let _ = writeln!(stdout, "  trap: {}", trap.trap_code);
        }
    }
    let _ = writeln!(
        stdout,
        "passed {}, failed {}, selected {}",
        test.passed, test.failed, test.selected
    );
}

/// Writes `response` and returns the exit code.
///
/// `serialize` turns the envelope into its validated JSON text; a failure
/// there is a toolchain defect, so the response becomes `@command.trap`
/// (exit 5), and a trap that cannot itself be serialized writes nothing to
/// standard output.
pub fn emit<W: Write, E: Write>(
    response: &Response,
    stdout: &mut W,
    stderr: &mut E,
    serialize: &dyn Fn(&Envelope) -> Result<String, SchemaViolation>,
) -> i32 {
    for line in &response.log {
        let _ = writeln!(stderr, "{line}");
    }
    match response.format {
        OutputFormat::Json => {
            let built = envelope(response).and_then(|built| {
                serialize(&built).map_err(|violation| violation.to_string())
            });
            match built {
                Ok(json) => {
                    let _ = writeln!(stdout, "{json}");
                    exit_code(response.result)
                }
                Err(message) => trap(response, &message, stdout, stderr, serialize),
            }
        }
        OutputFormat::Human => {
            match &response.payload {
                Payload::Help(help) => {
                    let _ = write!(stdout, "{}", help.usage);
                }
                Payload::Fmt(fmt)
                    if !fmt.written && response.diagnostics.is_empty() =>
                {
                    if let Some(text) = &fmt.text {
                        let _ = write!(stdout, "{text}");
                    }
                }
                Payload::Test(test) => write_test_report(test, stdout),
                _ => {}
            }
            write_diagnostics(response, stderr);
            if let Some(notice) = &response.notice {
                let _ = writeln!(stderr, "{notice}");
            }
            exit_code(response.result)
        }
    }
}

fn trap<W: Write, E: Write>(
    response: &Response,
    message: &str,
    stdout: &mut W,
    stderr: &mut E,
    serialize: &dyn Fn(&Envelope) -> Result<String, SchemaViolation>,
) -> i32 {
    let _ = writeln!(stderr, "internal error: {message}");
    let mut trapped = Envelope::new(
        env!("CARGO_PKG_VERSION"),
        response.command.clone(),
        status_for(CommandOutcome::Trap),
        CommandOutcome::Trap,
        limits(),
    );
    trapped
        .workspace_revision
        .clone_from(&response.workspace_revision);
    if let Ok(json) = serialize(&trapped) {
        let _ = writeln!(stdout, "{json}");
    }
    exit_code(CommandOutcome::Trap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response() -> Response {
        Response::new(
            OutputFormat::Json,
            "help",
            CommandOutcome::Ok,
            Payload::Empty {},
        )
    }

    #[test]
    fn a_document_that_fails_its_schema_is_a_trap_with_exit_5() {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let code = emit(&response(), &mut stdout, &mut stderr, &|envelope| {
            envelope.to_validated_json()
        });
        // An empty payload for a successful help violates the schema.
        assert_eq!(code, 5);
        let output = String::from_utf8(stdout).expect("UTF-8");
        let value: serde_json::Value =
            serde_json::from_str(&output).expect("one JSON value");
        assert_eq!(value["result"], "@command.trap");
        assert_eq!(output.lines().count(), 1);
        assert!(!stderr.is_empty());
    }

    #[test]
    fn a_trap_that_cannot_be_serialized_writes_nothing_to_standard_output() {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let code = emit(&response(), &mut stdout, &mut stderr, &|_| {
            Err(SchemaViolation::new(
                "command-result",
                vec!["always".to_owned()],
            ))
        });
        assert_eq!(code, 5);
        assert!(stdout.is_empty());
    }
}
