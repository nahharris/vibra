//! Human and JSON rendering of one response.
//!
//! `docs/spec/05-tooling.md`, "Streams": in JSON format the tool writes
//! exactly one envelope to standard output and its logs to standard error. In
//! human format the usage text goes to standard output and each diagnostic is
//! one line on standard error.

use std::io::Write;

use vibra_diagnostics::{Diagnostic, LineIndex};
use vibra_schema::{
    CommandOutcome, DiagnosticDocument, Envelope, Limits, Payload, SchemaViolation,
    Status,
};

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
    /// The diagnostics, once each.
    pub diagnostics: Vec<Diagnostic>,
    /// The payload.
    pub payload: Payload,
    /// A line for a person when the response carries no diagnostic to show.
    pub notice: Option<String>,
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
        status_for(response.result),
        response.result,
        limits(),
    );
    // Every diagnostic of this command line has no location, so the index is
    // the empty document's.
    let index = LineIndex::new("");
    for diagnostic in &response.diagnostics {
        envelope.diagnostics.push(
            DiagnosticDocument::render(diagnostic, &index)
                .map_err(|error| error.to_string())?,
        );
    }
    envelope.payload = response.payload.clone();
    Ok(envelope)
}

/// One line per diagnostic on standard error.
fn write_diagnostics<E: Write>(diagnostics: &[Diagnostic], stderr: &mut E) {
    for diagnostic in diagnostics {
        let _ = writeln!(
            stderr,
            "{} {}: {}",
            diagnostic.level().as_atom(),
            diagnostic.code().as_atom(),
            diagnostic.message()
        );
    }
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
            if let Payload::Help(help) = &response.payload {
                let _ = write!(stdout, "{}", help.usage);
            }
            write_diagnostics(&response.diagnostics, stderr);
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
    let trapped = Envelope::new(
        env!("CARGO_PKG_VERSION"),
        response.command.clone(),
        status_for(CommandOutcome::Trap),
        CommandOutcome::Trap,
        limits(),
    );
    if let Ok(json) = serialize(&trapped) {
        let _ = writeln!(stdout, "{json}");
    }
    exit_code(CommandOutcome::Trap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response() -> Response {
        Response {
            format: OutputFormat::Json,
            command: "help".to_owned(),
            result: CommandOutcome::Ok,
            diagnostics: Vec::new(),
            payload: Payload::Empty {},
            notice: None,
        }
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
