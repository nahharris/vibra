//! The workspace check: a captured snapshot, resolved and type-checked.
//!
//! `docs/spec/05-tooling.md`, the `check` rule of "Command surface": the
//! check covers a target's units and their import closure, plus every
//! module of the embedded standard library. With no target named, every
//! local target and the `@tests` unit are covered; a named target covers
//! that target only. A target whose kind this checker does not support yet
//! (`@bin`) still decodes: it is reported `@tool.unavailable` at its target
//! record, and the other targets are checked.

use vibra_diagnostics::{Diagnostic, DiagnosticCode, Level};
use vibra_types::{CheckResult, check_workspace};

use crate::project::TargetKind;
use crate::resolve::resolve_input;
use crate::snapshot::{SourceSnapshot, UnitKind};

/// The outcome of [`check_snapshot`].
#[derive(Clone, Debug)]
pub struct WorkspaceCheck {
    diagnostics: Vec<Diagnostic>,
    check: Option<CheckResult>,
}

impl WorkspaceCheck {
    /// Every diagnostic: the targets reported unavailable first, then the
    /// check's.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether any diagnostic is an error.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.level() == Level::Error)
    }

    /// Whether every error is `@tool.unavailable`.
    #[must_use]
    pub fn only_unavailable(&self) -> bool {
        self.has_errors()
            && self
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.level() == Level::Error)
                .all(|diagnostic| diagnostic.code() == DiagnosticCode::ToolUnavailable)
    }

    /// The check of the covered units, when the standard library loaded.
    #[must_use]
    pub const fn check(&self) -> Option<&CheckResult> {
        self.check.as_ref()
    }

    /// The `@types.v2` document of the covered local declarations.
    #[must_use]
    pub fn types_document(&self) -> Option<&str> {
        self.check.as_ref().and_then(CheckResult::types_document)
    }
}

/// Checks `snapshot`: every local target and the `@tests` unit when
/// `target` is `None`, otherwise the target of that unit name only. A name
/// that is no target selects nothing.
#[must_use]
pub fn check_snapshot(
    snapshot: &SourceSnapshot,
    target: Option<&str>,
) -> WorkspaceCheck {
    let project = snapshot.project().project();
    let mut diagnostics = Vec::new();
    let mut units = Vec::new();
    for declared in project.targets() {
        if target.is_some_and(|name| name != declared.name()) {
            continue;
        }
        match declared.kind() {
            TargetKind::Lib => units.push(declared.name().to_owned()),
            TargetKind::Bin => diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::ToolUnavailable,
                    declared.span(),
                    format!(
                        "the binary target `{}` is not supported by this checker yet",
                        declared.name()
                    ),
                )
                .with_source_id(project.source_id()),
            ),
        }
    }
    if target.is_none() {
        units.extend(
            snapshot
                .units()
                .iter()
                .filter(|unit| unit.kind() == UnitKind::Tests)
                .map(|unit| unit.name().to_owned()),
        );
    }
    let input = match resolve_input(snapshot) {
        Ok(input) => input,
        Err(error) => {
            diagnostics.push(error.diagnostic().clone());
            return WorkspaceCheck {
                diagnostics,
                check: None,
            };
        }
    };
    let check = check_workspace(&input, &units);
    diagnostics.extend(check.diagnostics().iter().cloned());
    WorkspaceCheck {
        diagnostics,
        check: Some(check),
    }
}
