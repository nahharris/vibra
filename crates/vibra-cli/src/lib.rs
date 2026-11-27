//! The `vibra` command line: the argument grammar of the whole command
//! surface, `help`, `fmt`, `check`, `test`, `run`, the support inventory,
//! invalid input, and the unavailable outcome of every command no step has
//! landed yet.
//!
//! `docs/spec/05-tooling.md`, "Command surface" and "Versioned envelope and
//! results", are the normative definitions. [`run()`] is a pure function of its
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
pub mod check;
pub mod fmt;
pub mod help;
pub mod inventory;
pub mod render;
pub mod run;
pub mod test;

use std::ffi::OsString;
use std::io::Write;
use std::path::Path;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_schema::{CommandOutcome, HelpPayload, Payload};
use vibra_workspace::COMMAND_STACK;

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
        Ok(invocation) => on_command_stack(&invocation),
        Err(rejection) => invalid_input(&rejection),
    };
    emit(&response, &mut stdout, &mut stderr, &|envelope| {
        envelope.to_validated_json()
    })
}

/// Runs `invocation` on a thread with [`COMMAND_STACK`] bytes of stack, the
/// size the workspace engine's stages are measured to need at the source
/// nesting limit.
///
/// A thread the platform cannot start is an operational failure; a panic
/// of the toolchain is a trap. Neither ends the process without a
/// document.
fn on_command_stack(invocation: &Invocation) -> Response {
    let format = invocation.globals.format;
    let name = invocation.line.name();
    #[cfg(test)]
    let hook = test::carry_hook();
    let ended = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("vibra-command".to_owned())
            .stack_size(COMMAND_STACK)
            .spawn_scoped(scope, move || {
                #[cfg(test)]
                hook();
                execute(invocation)
            })
            .map(|handle| handle.join())
    });
    match ended {
        Ok(Ok(response)) => response,
        Ok(Err(_)) => {
            let mut response =
                Response::new(format, name, CommandOutcome::Trap, Payload::Empty {});
            response.notice = Some("internal error: the command panicked".to_owned());
            response
        }
        Err(error) => {
            let mut response = Response::new(
                format,
                name,
                CommandOutcome::OperationalFailure,
                Payload::Empty {},
            );
            response.notice =
                Some(format!("the command's thread cannot be started: {error}"));
            response
        }
    }
}

/// The response to a well-formed command line.
fn execute(invocation: &Invocation) -> Response {
    let line = invocation.line;
    let format = invocation.globals.format;
    match line.words {
        ["help"] => Response::new(
            format,
            line.name(),
            CommandOutcome::Ok,
            Payload::Help(HelpPayload {
                usage: help::usage(),
                capabilities: inventory::capabilities(),
            }),
        ),
        ["fmt"] => fmt::run(invocation),
        ["check"] => check::run(invocation),
        ["test"] => test::run(invocation),
        ["run"] => run::run(invocation),
        _ => unavailable(format, line.name()),
    }
}

/// The response to a known command this tool does not support.
fn unavailable(format: OutputFormat, name: String) -> Response {
    let diagnostic = Diagnostic::new(
        DiagnosticCode::ToolUnavailable,
        ByteSpan::empty_at(0),
        format!("`{name}` is not available in this release"),
    )
    .with_note(format!("the command `{name}` is known but not supported"));
    let mut response =
        Response::new(format, name, CommandOutcome::Unavailable, Payload::Empty {});
    response.diagnostics.push(diagnostic);
    response
}

/// The response to a command line that is invalid input: no diagnostics, the
/// recognized command name or an empty one, and the empty payload.
fn invalid_input(rejection: &Rejection) -> Response {
    refusal(
        rejection.format,
        rejection.command.clone(),
        &rejection.reason,
    )
}

/// Invalid input for `command`: no snapshot, no diagnostics, the empty
/// payload, and one line for a person.
pub(crate) fn refusal(
    format: OutputFormat,
    command: impl Into<String>,
    reason: &dyn std::fmt::Display,
) -> Response {
    let mut response = Response::new(
        format,
        command,
        CommandOutcome::InvalidInput,
        Payload::Empty {},
    );
    response.notice = Some(format!(
        "invalid input: {reason}; run `vibra help` for the usage"
    ));
    response
}
