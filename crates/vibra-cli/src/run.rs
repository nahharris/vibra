//! `vibra run [TARGET] [-- ARG...]`.
//!
//! `docs/spec/05-tooling.md`, "Command surface", "Program arguments,
//! execution and evaluation", "Versioned envelope and results", and "Command
//! payloads", and `docs/spec/06-runtime.md`, "Entry outcome", are the
//! authority; the work is the workspace engine's [`run_target`].
//!
//! 1. A `TARGET` is an atom naming a local binary target; anything else, a
//!    name no target has, a library, and no or several binary targets with
//!    no selector, are invalid input with no snapshot and no diagnostics.
//! 2. The program's arguments are every token after `--`, kept as
//!    operating-system strings. This module is the only place in the
//!    workspace that launches a target on the process, [`Launch::System`].
//! 3. In JSON format the program's output is captured into the payload, and
//!    one envelope is the whole of standard output; in human format the
//!    program's bytes go straight through, and the tool writes only its
//!    diagnostics, to standard error.
//! 4. The result is the run's: `@command.ok` for the code zero,
//!    `@command.programFailed` (exit 6) for a non-zero code, and the
//!    startup, trap, exhaustion, and unavailable endings as the runtime
//!    chapter keeps them apart. Memory exhaustion reaches the limit
//!    `memory`, and the observation is incomplete.
//! 5. The payload splits the run's trace by phase: the events of phase
//!    `@entry` are its `auditTrace`; the `@startup` events and the startup
//!    reads are the startup record's. Its `drivers` and `policies` are the
//!    trace's.

use std::io::Write;

use serde_json::Value as Json;
use vibra_diagnostics::{ByteSpan, LineIndex};
use vibra_schema::{
    AuditEvent, BindingRecordDocument, BindingStatus, CommandOutcome, DriverRecord,
    Payload, RunPayload, SpanDocument, StartupFailureDocument, StartupRecordDocument,
    StartupStatus, Status, TrapRecord,
};
use vibra_workspace::run::{CommandResult, Launch, RunOutcome, run_target};
use vibra_workspace::{Discovery, MemoryBudget, SourceOrigin, WorkspaceCapture};

use crate::args::{Invocation, OutputFormat};
use crate::check::{is_target_atom, outcome as check_outcome};
use crate::refusal;
use crate::render::{Response, source_text, status_for};

/// The memory budget a run's entry runs under: the memory limit of the
/// published limit profile.
pub const MEMORY_BUDGET: MemoryBudget = MemoryBudget::DEFAULT;

/// The response to a well-formed `run` line.
#[must_use]
pub fn run(invocation: &Invocation) -> Response {
    let format = invocation.globals.format;
    let name = invocation.line.name();
    let target = match invocation.operands.first() {
        None => None,
        Some(operand) => match operand.to_str().filter(|text| is_target_atom(text)) {
            Some(text) => Some(text.to_owned()),
            None => {
                return refusal(
                    format,
                    name,
                    &format!(
                        "`{}` is not a target: a target is an atom such as `@app`",
                        operand.to_string_lossy()
                    ),
                );
            }
        },
    };
    let capture =
        WorkspaceCapture::capture(&invocation.globals.workspace, Discovery::Search);
    if let (Some(target), Some(project)) = (&target, capture.project())
        && !project
            .project()
            .targets()
            .iter()
            .any(|declared| declared.name() == target)
    {
        return refusal(format, name, &format!("`{target}` names no local target"));
    }
    let mut response =
        Response::new(format, name, CommandOutcome::Ok, Payload::Empty {});
    response.workspace_revision = capture.revision().map(str::to_owned);
    if let Some(project) = capture.project() {
        response.sources.insert(
            project.project().source_id().to_owned(),
            String::from_utf8_lossy(project.bytes()).into_owned(),
        );
    }
    let Some(snapshot) = capture.snapshot() else {
        // No project was captured, so no target was selected: the
        // diagnostics, and no run payload.
        let diagnostics = capture.diagnostics().to_vec();
        let (result, status) = check_outcome(&diagnostics, &[], false);
        response.result = result;
        response.status = status;
        response.diagnostics = diagnostics;
        return response;
    };
    for module in snapshot.modules() {
        response.sources.insert(
            module.source_id().to_owned(),
            String::from_utf8_lossy(module.bytes()).into_owned(),
        );
    }
    let project_source = snapshot.project().project().source_id().to_owned();
    let capture_output = format == OutputFormat::Json;
    if !capture_output {
        // The program writes straight to the process's streams: nothing the
        // tool buffered may follow it.
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
    }
    let outcome = run_target(
        snapshot,
        target.as_deref(),
        Launch::System {
            arguments: invocation.program_arguments.clone(),
            capture: capture_output,
        },
        MEMORY_BUDGET,
    );
    respond(response, &outcome, &project_source)
}

/// The command result of a run's result.
const fn command_outcome(result: CommandResult) -> CommandOutcome {
    match result {
        CommandResult::Ok => CommandOutcome::Ok,
        CommandResult::Diagnostics => CommandOutcome::Diagnostics,
        CommandResult::InvalidInput => CommandOutcome::InvalidInput,
        CommandResult::OperationalFailure => CommandOutcome::OperationalFailure,
        CommandResult::Unavailable => CommandOutcome::Unavailable,
        CommandResult::Trap => CommandOutcome::Trap,
        CommandResult::ProgramFailed => CommandOutcome::ProgramFailed,
    }
}

/// Fills `response` from how the run ended. `project_source` is the source
/// identity of the project document, which a startup failure's origin is
/// in.
pub(crate) fn respond(
    mut response: Response,
    outcome: &RunOutcome,
    project_source: &str,
) -> Response {
    let result = command_outcome(outcome.result);
    if result == CommandOutcome::InvalidInput {
        return refusal(
            response.format,
            response.command,
            &"the line selects no binary target: name one of the project's binary targets",
        );
    }
    response.result = result;
    response.status = status_for(result);
    response.diagnostics.clone_from(&outcome.diagnostics);
    response.log.clone_from(&outcome.log);
    if !outcome.limits_reached.is_empty() {
        // A reached limit is an unknown outcome and an incomplete
        // observation.
        response.limits.reached.clone_from(&outcome.limits_reached);
        response.limits.complete = false;
        response.status = Status::Unknown;
    }
    match payload(outcome, &response, project_source) {
        Ok(payload) => response.payload = Payload::Run(Box::new(payload)),
        Err(defect) => {
            response.result = CommandOutcome::Trap;
            response.status = status_for(CommandOutcome::Trap);
            response.payload = Payload::Empty {};
            response.notice = Some(format!("internal error: {defect}"));
        }
    }
    response
}

/// The `SpanDocument` of `span` in the source `source_id`, against its
/// captured text.
fn span_document(span: ByteSpan, source_id: &str, response: &Response) -> SpanDocument {
    let text = source_text(response, source_id);
    SpanDocument::render(span, &LineIndex::new(text), Some(source_id))
}

/// The `run` payload of `outcome`.
///
/// # Errors
///
/// A defect of the toolchain: a run that selected no target, an event of a
/// kind no JSON projection is defined for, or a binding the run left
/// acquired.
fn payload(
    outcome: &RunOutcome,
    response: &Response,
    project_source: &str,
) -> Result<RunPayload, String> {
    let target = outcome
        .target
        .clone()
        .ok_or("a run that ended after its selection has no target")?;
    let span = |origin: &SourceOrigin| {
        serde_json::to_value(span_document(origin.span(), origin.source_id(), response))
            .unwrap_or(Json::Null)
    };
    let mut entry = Vec::new();
    let mut startup = Vec::new();
    for event in &outcome.trace.events {
        let json = event.json(&span).ok_or_else(|| {
            format!(
                "the run's trace holds an event of an unknown kind: {}",
                event.canonical_bytes()
            )
        })?;
        let field = |name: &str| json.get(name).and_then(Json::as_str);
        if field("kind") == Some("@host") && field("phase") == Some("@entry") {
            entry.push(AuditEvent(json));
        } else {
            startup.push(AuditEvent(json));
        }
    }
    let record = &outcome.startup;
    let status = match record.status.spelling() {
        "notStarted" => StartupStatus::NotStarted,
        "acquired" => StartupStatus::Acquired,
        "failed" => StartupStatus::Failed,
        other => return Err(format!("a startup record of the status `{other}`")),
    };
    let bindings = record
        .bindings
        .iter()
        .map(|binding| {
            let status = match binding.status.spelling() {
                "refused" => BindingStatus::Refused,
                "failed" => BindingStatus::Failed,
                "released" => BindingStatus::Released,
                other => {
                    return Err(format!(
                        "the binding `{}` is `{other}` after the run",
                        binding.name
                    ));
                }
            };
            Ok(BindingRecordDocument {
                name: binding.name.to_string(),
                recipe_identity: binding.recipe_identity.clone(),
                provider_identity: DriverRecord {
                    provider: binding.provider_identity.provider.clone(),
                    version: binding.provider_identity.version.clone(),
                },
                status,
                grant_projection: binding
                    .grant_projection
                    .as_ref()
                    .map(|grant| grant.json()),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let failure = record
        .failure
        .as_ref()
        .map(|failure| StartupFailureDocument {
            binding: failure.binding.to_string(),
            code: failure.code.atom().to_owned(),
            origin: span_document(failure.span, project_source, response),
        });
    Ok(RunPayload {
        target,
        program_result: outcome.program_result.clone(),
        application_code: outcome.application_code,
        stdout: String::from_utf8_lossy(&outcome.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&outcome.stderr).into_owned(),
        audit_trace: entry,
        drivers: outcome
            .trace
            .drivers
            .iter()
            .map(|driver| DriverRecord {
                provider: driver.provider.clone(),
                version: driver.version.clone(),
            })
            .collect(),
        policies: outcome
            .trace
            .policies
            .iter()
            .map(|(hash, policy)| (hash.clone(), policy.canonical_bytes()))
            .collect(),
        startup: StartupRecordDocument {
            status,
            bindings,
            audit_trace: startup,
            failure,
        },
        trap: outcome.trap.as_ref().map(|trap| TrapRecord {
            trap_code: trap.trap.code().as_atom().to_owned(),
            origin: trap.origin.as_ref().map(|origin| {
                span_document(origin.span(), origin.source_id(), response)
            }),
        }),
    })
}

#[cfg(test)]
mod tests;
