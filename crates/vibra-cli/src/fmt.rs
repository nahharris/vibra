//! `vibra fmt PATH [--write]`.
//!
//! `docs/spec/05-tooling.md`, "Formatting": the extension selects the
//! grammar, the path is confined beneath the workspace root, a preview is
//! the default, and `--write` replaces the file only when no error
//! diagnostic is present. The work is the workspace engine's
//! [`plan_format`]; this module maps the plan to the envelope.

use vibra_schema::{CommandOutcome, FmtPayload, Payload};
use vibra_workspace::{Discovery, WorkspaceCapture, format_mode, plan_format};

use crate::args::Invocation;
use crate::refusal;
use crate::render::Response;

/// The response to a well-formed `fmt` line.
#[must_use]
pub fn run(invocation: &Invocation) -> Response {
    let format = invocation.globals.format;
    let name = invocation.line.name();
    let Some(path) = invocation.operands.first().and_then(|path| path.to_str()) else {
        return refusal(format, name, &"`PATH` is a Unicode path");
    };
    if let Err(invalid) = format_mode(path) {
        return refusal(format, name, &invalid);
    }
    let write = invocation
        .options
        .iter()
        .any(|(option, _)| *option == "--write");
    let workspace = &invocation.globals.workspace;
    let capture = WorkspaceCapture::capture(workspace, Discovery::Search);
    let plan = match plan_format(workspace, &capture, path) {
        Ok(plan) => plan,
        Err(invalid) => return refusal(format, name, &invalid),
    };
    let mut diagnostics = plan.diagnostics().to_vec();
    let mut written = false;
    if write {
        match plan.write() {
            Ok(wrote) => written = wrote,
            Err(diagnostic) => diagnostics.push(*diagnostic),
        }
    }
    let has_errors = diagnostics
        .iter()
        .any(vibra_diagnostics::Diagnostic::is_error);
    let result = if has_errors {
        CommandOutcome::Diagnostics
    } else {
        CommandOutcome::Ok
    };
    let mut response = Response::new(
        format,
        name,
        result,
        Payload::Fmt(FmtPayload {
            path: plan.path().to_owned(),
            changed: plan.changed(),
            written,
            text: if written {
                None
            } else {
                plan.text().map(str::to_owned)
            },
        }),
    );
    response.workspace_revision = plan.revision().map(str::to_owned);
    if let Some(text) = plan.text() {
        response
            .sources
            .insert(plan.path().to_owned(), text.to_owned());
    }
    response.diagnostics = diagnostics;
    if written {
        response.notice = Some(format!("formatted {}", plan.path()));
    }
    response
}
