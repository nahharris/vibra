//! `vibra check [TARGET]`.
//!
//! `docs/spec/05-tooling.md`, "Command surface": without a target, `check`
//! covers every local target and the `@tests` unit; with one, that target
//! only. A target is an atom, never a path, and one that names no local
//! target is invalid input. A target whose kind the profile does not support
//! is `@tool.unavailable` at its target record, and the other targets are
//! still checked. The work is the workspace engine's [`check_snapshot`].
//!
//! "Versioned envelope and results" fixes the status: the result fixes it,
//! except that a reached limit or a bare guard that is not established make
//! it `unknown`, and a disproved bare guard makes it `refuted`.

use vibra_diagnostics::{Diagnostic, DiagnosticCode};
use vibra_schema::{CheckPayload, CommandOutcome, ObligationRecord, Payload, Status};
use vibra_workspace::{Discovery, PROJECT_FILE_NAME, WorkspaceCapture, check_snapshot};

use crate::args::Invocation;
use crate::refusal;
use crate::render::{Response, status_for};

/// Whether `text` is an atom of one identifier part, such as `@app`: the
/// spelling of a target name.
fn is_target_atom(text: &str) -> bool {
    let Some(name) = text.strip_prefix('@') else {
        return false;
    };
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| character.is_ascii_alphanumeric())
}

/// The result and status of a check.
///
/// The result follows the diagnostics: none that is an error is
/// `@command.ok`; errors that are all `@tool.unavailable` are
/// `@command.unavailable`; any other error is `@command.diagnostics`. The
/// status is the result's, except that a refuted obligation makes it
/// `refuted`, and an obligation that is not established or a reached limit
/// makes it `unknown`. An unavailable result stays `unsupported`.
#[must_use]
pub fn outcome(
    diagnostics: &[Diagnostic],
    obligations: &[ObligationRecord],
    limit_reached: bool,
) -> (CommandOutcome, Status) {
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.is_error())
        .collect::<Vec<_>>();
    let result = if errors.is_empty() {
        CommandOutcome::Ok
    } else if errors
        .iter()
        .all(|diagnostic| diagnostic.code() == DiagnosticCode::ToolUnavailable)
    {
        CommandOutcome::Unavailable
    } else {
        CommandOutcome::Diagnostics
    };
    let has = |status: Status| obligations.iter().any(|record| record.status == status);
    let status = if result == CommandOutcome::Unavailable {
        Status::Unsupported
    } else if has(Status::Refuted) {
        Status::Refuted
    } else if has(Status::Unknown) || limit_reached {
        Status::Unknown
    } else {
        status_for(result)
    };
    (result, status)
}

fn checked(
    response: &mut Response,
    diagnostics: Vec<Diagnostic>,
    obligations: Vec<ObligationRecord>,
) {
    let (result, status) = outcome(&diagnostics, &obligations, false);
    response.result = result;
    response.status = status;
    response.diagnostics = diagnostics;
    response.payload = Payload::Check(CheckPayload {
        accepted: result == CommandOutcome::Ok,
        obligations,
    });
}

/// The response to a well-formed `check` line.
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
            PROJECT_FILE_NAME.to_owned(),
            String::from_utf8_lossy(project.bytes()).into_owned(),
        );
    }
    let Some(snapshot) = capture.snapshot() else {
        checked(&mut response, capture.diagnostics().to_vec(), Vec::new());
        return response;
    };
    for module in snapshot.modules() {
        response.sources.insert(
            module.source_id().to_owned(),
            String::from_utf8_lossy(module.bytes()).into_owned(),
        );
    }
    let check = check_snapshot(snapshot, target.as_deref());
    // The checker records no obligation yet: the bare guard steps add them
    // to the check result, and this payload carries them.
    checked(&mut response, check.diagnostics().to_vec(), Vec::new());
    if response.result == CommandOutcome::Ok {
        response.notice = Some("check passed".to_owned());
    }
    response
}

#[cfg(test)]
mod tests {
    use vibra_diagnostics::{ByteSpan, LineIndex};
    use vibra_schema::{ObligationKind, SpanDocument};

    use super::*;

    fn obligation(status: Status) -> ObligationRecord {
        ObligationRecord {
            kind: ObligationKind::Requires,
            declaration: "sha256:00".to_owned(),
            predicate_identity: "sha256:01".to_owned(),
            source: SpanDocument::render(
                ByteSpan::new(0, 1),
                &LineIndex::new("x"),
                Some("src/app/main.vib"),
            ),
            status,
            rule_ids: Vec::new(),
            dependencies: Vec::new(),
            counterexample: None,
        }
    }

    fn error(code: DiagnosticCode) -> Diagnostic {
        Diagnostic::new(code, ByteSpan::new(0, 1), "fault")
    }

    #[test]
    fn the_status_mapping_covers_the_four_statuses() {
        assert_eq!(
            outcome(&[], &[obligation(Status::Passed)], false),
            (CommandOutcome::Ok, Status::Passed)
        );
        assert_eq!(
            outcome(
                &[error(DiagnosticCode::TypeMismatch)],
                &[obligation(Status::Refuted), obligation(Status::Unknown)],
                false
            ),
            (CommandOutcome::Diagnostics, Status::Refuted)
        );
        assert_eq!(
            outcome(
                &[error(DiagnosticCode::TypeMismatch)],
                &[obligation(Status::Passed), obligation(Status::Unknown)],
                false
            ),
            (CommandOutcome::Diagnostics, Status::Unknown)
        );
        assert_eq!(
            outcome(&[error(DiagnosticCode::ToolLimitExceeded)], &[], true),
            (CommandOutcome::Diagnostics, Status::Unknown)
        );
        assert_eq!(
            outcome(&[error(DiagnosticCode::ToolUnavailable)], &[], false),
            (CommandOutcome::Unavailable, Status::Unsupported)
        );
        assert_eq!(
            outcome(&[error(DiagnosticCode::TypeMismatch)], &[], false),
            (CommandOutcome::Diagnostics, Status::Refuted)
        );
        assert_eq!(
            outcome(
                &[
                    error(DiagnosticCode::ToolUnavailable),
                    error(DiagnosticCode::TypeMismatch)
                ],
                &[],
                false
            ),
            (CommandOutcome::Diagnostics, Status::Refuted)
        );
    }

    #[test]
    fn a_target_is_an_atom_of_one_part() {
        assert!(is_target_atom("@app"));
        assert!(is_target_atom("@myApp2"));
        assert!(!is_target_atom("app"));
        assert!(!is_target_atom("src/app"));
        assert!(!is_target_atom("@app.main"));
        assert!(!is_target_atom("@"));
        assert!(!is_target_atom("@2app"));
    }
}
