//! The workspace check: a captured snapshot, resolved and type-checked.
//!
//! `docs/spec/05-tooling.md`, the `check` rule of "Command surface": the
//! check covers a target's units and their import closure, plus every
//! module of the embedded standard library. With no target named, every
//! local target and the `@tests` unit are covered; a named target covers
//! that target only. A binary target's entry and admission ceiling are
//! checked with its unit ([`crate::entry`]); its recipes are not supported
//! yet, so a target with a grant leaf is `@tool.unavailable` at its
//! `grants` field.

use vibra_diagnostics::{Diagnostic, DiagnosticCode, Level};
use vibra_types::{CheckResult, check_targets};

use crate::entry::{EntryFacts, check_target, target_entry};
use crate::project::{Target, TargetKind};
use crate::resolve::resolve_input;
use crate::snapshot::{SourceSnapshot, UnitKind};

/// The outcome of [`check_snapshot`].
#[derive(Clone, Debug)]
pub struct WorkspaceCheck {
    diagnostics: Vec<Diagnostic>,
    check: Option<CheckResult>,
    entries: Vec<EntryFacts>,
}

impl WorkspaceCheck {
    /// Every diagnostic: those of the project file first, in the order of
    /// their spans, then the check's.
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

    /// The entry and the ceiling of each covered binary target whose entry
    /// and `effects` list check, in project order.
    #[must_use]
    pub fn entries(&self) -> &[EntryFacts] {
        &self.entries
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
    let covered = project
        .targets()
        .iter()
        .filter(|declared| target.is_none_or(|name| name == declared.name()))
        .collect::<Vec<_>>();
    let mut units = covered
        .iter()
        .map(|declared| declared.name().to_owned())
        .collect::<Vec<_>>();
    let binaries = covered
        .iter()
        .copied()
        .filter(|declared| declared.kind() == TargetKind::Bin)
        .collect::<Vec<&Target>>();
    if target.is_none() {
        units.extend(
            snapshot
                .units()
                .iter()
                .filter(|unit| unit.kind() == UnitKind::Tests)
                .map(|unit| unit.name().to_owned()),
        );
    }
    let mut diagnostics = Vec::new();
    let input = match resolve_input(snapshot) {
        Ok(input) => input,
        Err(error) => {
            for binary in &binaries {
                diagnostics.extend(check_target(project, binary, None).0);
            }
            diagnostics.push(error.diagnostic().clone());
            return WorkspaceCheck {
                diagnostics,
                check: None,
                entries: Vec::new(),
            };
        }
    };
    let requests = binaries
        .iter()
        .map(|binary| target_entry(project, binary))
        .collect::<Vec<_>>();
    let check = check_targets(&input, &units, &requests);
    let mut entries = Vec::new();
    for (index, binary) in binaries.iter().enumerate() {
        let (found, facts) = check_target(project, binary, check.targets().get(index));
        diagnostics.extend(found);
        entries.extend(facts);
    }
    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end(),
        )
    });
    diagnostics.extend(check.diagnostics().iter().cloned());
    WorkspaceCheck {
        diagnostics,
        check: Some(check),
        entries,
    }
}
