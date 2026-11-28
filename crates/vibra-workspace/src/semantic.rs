//! The workspace check: a captured snapshot, resolved and type-checked.
//!
//! `docs/spec/05-tooling.md`, the `check` rule of "Command surface": the
//! check covers a target's units and their import closure, plus every
//! module of the embedded standard library. With no target named, every
//! local target and the `@tests` unit are covered; a named target covers
//! that target only. A binary target's entry and admission ceiling are
//! checked with its unit ([`crate::entry`]), and its recipes against the
//! entry's leaves ([`crate::binding`]).

use vibra_diagnostics::{Diagnostic, Level};
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
    limits: Vec<String>,
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
        vibra_diagnostics::only_unavailable(&self.diagnostics)
    }

    /// The check of the covered units, when the standard library loaded.
    #[must_use]
    pub const fn check(&self) -> Option<&CheckResult> {
        self.check.as_ref()
    }

    /// The limits checking reached, by the name `limits.reached` gives them,
    /// in the order first reached: those of the check of the covered units
    /// ([`CheckResult::limits_reached`]), and `sourceNesting` when a module
    /// is nested past the source nesting limit.
    #[must_use]
    pub fn limits_reached(&self) -> &[String] {
        &self.limits
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
/// that is no target selects nothing. A snapshot with a module nested past
/// the source nesting limit is not checked: its diagnostics are the limit's
/// alone ([`SourceSnapshot::limit_faults`]).
#[must_use]
pub fn check_snapshot(
    snapshot: &SourceSnapshot,
    target: Option<&str>,
) -> WorkspaceCheck {
    if !snapshot.limit_faults().is_empty() {
        return WorkspaceCheck {
            diagnostics: snapshot.limit_faults().to_vec(),
            check: None,
            entries: Vec::new(),
            limits: snapshot.limits_reached(),
        };
    }
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
    let mut diagnostics = snapshot.unavailable().to_vec();
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
                limits: snapshot.limits_reached(),
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
    let mut limits = snapshot.limits_reached();
    limits.extend(
        check
            .limits_reached()
            .iter()
            .map(|limit| (*limit).to_owned()),
    );
    WorkspaceCheck {
        diagnostics,
        check: Some(check),
        entries,
        limits,
    }
}
