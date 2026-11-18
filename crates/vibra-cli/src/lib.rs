//! The `vibra` command line: the argument grammar of the whole command
//! surface, `help`, the support inventory, invalid input, and the unavailable
//! outcome of every command no step has landed yet.
//!
//! `docs/spec/05-tooling.md`, "Command surface" and "Versioned envelope and
//! results", are the normative definitions. [`run`] is a pure function of its
//! arguments, working directory, and two writers; the process entry point in
//! `main.rs` only supplies the process's own.

// Tests assert by panicking and index fixtures they just built. The library
// itself is still checked against these lints, because clippy builds the lib
// target without `cfg(test)` as well.
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

pub mod args;
pub mod help;
pub mod inventory;
pub mod render;

use std::ffi::OsString;
use std::io::Write;
use std::path::Path;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_schema::{CommandOutcome, HelpPayload, Payload};

pub use args::OutputFormat;
use args::{Invocation, Rejection, parse};
use render::{Response, emit};

/// The process exit code of a command result.
///
/// `docs/spec/05-tooling.md`, "Versioned envelope and results": 0, 1, 1, 2,
/// 3, 4, 5, 6 for the eight results in the order of the result table.
#[must_use]
pub const fn exit_code(result: CommandOutcome) -> i32 {
    match result {
        CommandOutcome::Ok => 0,
        CommandOutcome::Diagnostics | CommandOutcome::TestFailed => 1,
        CommandOutcome::InvalidInput => 2,
        CommandOutcome::OperationalFailure => 3,
        CommandOutcome::Unavailable => 4,
        CommandOutcome::Trap => 5,
        CommandOutcome::ProgramFailed => 6,
    }
}

/// Runs one command line and returns the process exit code.
///
/// `arguments` excludes the program name. `current_directory` is the
/// workspace when no `--workspace` is given. In JSON format exactly one
/// envelope is written to `stdout`, followed by one newline; logs go to
/// `stderr`.
pub fn run<W: Write, E: Write>(
    arguments: impl IntoIterator<Item = OsString>,
    current_directory: &Path,
    mut stdout: W,
    mut stderr: E,
) -> i32 {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let response = match parse(&arguments, current_directory) {
        Ok(invocation) => execute(&invocation),
        Err(rejection) => invalid_input(&rejection),
    };
    emit(&response, &mut stdout, &mut stderr, &|envelope| {
        envelope.to_validated_json()
    })
}

/// The response to a well-formed command line.
fn execute(invocation: &Invocation) -> Response {
    let line = invocation.line;
    let format = invocation.globals.format;
    if line.supported {
        let usage = help::usage();
        return Response {
            format,
            command: line.name(),
            result: CommandOutcome::Ok,
            diagnostics: Vec::new(),
            payload: Payload::Help(HelpPayload {
                usage,
                capabilities: inventory::capabilities(),
            }),
            notice: None,
        };
    }
    let name = line.name();
    let diagnostic = Diagnostic::new(
        DiagnosticCode::ToolUnavailable,
        ByteSpan::empty_at(0),
        format!("`{name}` is not available in this release"),
    )
    .with_note(format!("the command `{name}` is known but not supported"));
    Response {
        format,
        command: name,
        result: CommandOutcome::Unavailable,
        diagnostics: vec![diagnostic],
        payload: Payload::Empty {},
        notice: None,
    }
}

/// The response to a command line that is invalid input: no diagnostics, the
/// recognized command name or an empty one, and the empty payload.
fn invalid_input(rejection: &Rejection) -> Response {
    Response {
        format: rejection.format,
        command: rejection.command.clone(),
        result: CommandOutcome::InvalidInput,
        diagnostics: Vec::new(),
        payload: Payload::Empty {},
        notice: Some(format!(
            "invalid input: {}; run `vibra help` for the usage",
            rejection.reason
        )),
    }
}
