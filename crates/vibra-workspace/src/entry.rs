//! The entry and the admission ceiling of a binary target.
//!
//! `docs/spec/04-programs-and-packages.md`, "Targets", and
//! `docs/spec/03-effects.md`, "Target admission". `entry` is a declaration
//! reference whose first part MUST be the target's own name
//! (`@project.entryOutsideTarget` at the `entry` field otherwise); the
//! checker resolves it and holds it to the entry signature, and reads the
//! `effects` list as a row in data form (`vibra_types::check_targets`).
//! This module compares the two:
//!
//! - the entry's written `performs` row MUST be contained in `effects`: an
//!   interface outside it is `@effect.outsideCeiling` at the `effects`
//!   field, related to the entry's `performs` entry, once per interface;
//! - an interface in `effects` that the entry does not perform is the
//!   warning `@contract.unusedEffect` at that entry of the list. A startup
//!   step may perform it too, and only the recipes say which, so the warning
//!   is decided for a target with no grant leaf and no recipe only.
//!
//! The recipes of `grants` are not validated here. A target whose entry
//! takes a grant leaf, or whose `grants` holds a recipe, is
//! `@tool.unavailable` at its `grants` field, so no target is accepted with
//! a recipe nobody checked. The list grants nothing, and naming an entry
//! makes no import edge and changes no visibility.
//!
//! The span of a field (`entry`, `effects`, `grants`) is its value, as the
//! decoder reports "the `format` field".

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{EntityRef, Row};
use vibra_types::{DataReference, EntryLeaf, EntrySignature, TargetCheck, TargetEntry};

use crate::project::{Project, Target};

/// What admission needs of a checked binary target: its entry and its
/// ceiling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryFacts {
    target: String,
    signature: EntrySignature,
    ceiling: Vec<(EntityRef, ByteSpan)>,
}

impl EntryFacts {
    /// The target's unit name, `@` included.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The checked entry.
    #[must_use]
    pub const fn signature(&self) -> &EntrySignature {
        &self.signature
    }

    /// The entry's written row.
    #[must_use]
    pub fn row(&self) -> Row {
        self.signature.row()
    }

    /// The entry's grant leaves, in parameter order.
    #[must_use]
    pub fn leaves(&self) -> Vec<EntryLeaf> {
        self.signature.leaves()
    }

    /// The `effects` list as a row: the static admission ceiling.
    #[must_use]
    pub fn ceiling(&self) -> Row {
        Row::new(self.ceiling.iter().map(|(root, _)| root.clone())).unwrap_or_default()
    }

    /// Each entry of the `effects` list, with where it is written.
    #[must_use]
    pub fn ceiling_entries(&self) -> &[(EntityRef, ByteSpan)] {
        &self.ceiling
    }
}

/// Whether the `entry` atom `entry` of the target `target` starts with the
/// target's own name.
fn inside(target: &Target, entry: &str) -> bool {
    entry
        .strip_prefix(target.name())
        .is_some_and(|rest| rest.starts_with('.'))
}

/// What the checker resolves of the binary target `target` of `project`:
/// its `entry`, unless its first part is not the target's name, and its
/// `effects` entries.
#[must_use]
pub fn target_entry(project: &Project, target: &Target) -> TargetEntry {
    let entry = target
        .entry()
        .filter(|(entry, _)| inside(target, entry))
        .map(|(entry, span)| DataReference::new(entry, span));
    let effects = target
        .effects()
        .iter()
        .map(|(path, span)| DataReference::new(path.clone(), *span))
        .collect();
    TargetEntry::new(project.source_id(), entry, effects)
}

/// The diagnostics of the binary target `target` of `project`, in the
/// order of their spans, with what admission needs of it when its entry and
/// its ceiling check. `found` is what the checker found of its
/// [`target_entry`]; `None` when the check stopped before it, at a fault
/// reported on its own.
#[must_use]
pub fn check_target(
    project: &Project,
    target: &Target,
    found: Option<&TargetCheck>,
) -> (Vec<Diagnostic>, Option<EntryFacts>) {
    let source_id = project.source_id();
    let mut diagnostics = Vec::new();
    if let Some((entry, span)) = target.entry()
        && !inside(target, entry)
    {
        diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::ProjectEntryOutsideTarget,
                span,
                format!(
                    "the entry `{entry}` is outside the target `{}`: its first part is the target's name",
                    target.name()
                ),
            )
            .with_source_id(source_id),
        );
    }
    let Some(found) = found else {
        return (diagnostics, None);
    };
    diagnostics.extend(found.diagnostics().iter().cloned());
    let mut facts = None;
    if let Some(signature) = found.signature() {
        let leaves = signature.leaves();
        if let Some(ceiling) = found.ceiling() {
            let effects = target.effects_span().unwrap_or(target.span());
            let mut contained = true;
            for entry in signature.row_entries() {
                if !ceiling.iter().any(|(root, _)| root == entry.interface()) {
                    contained = false;
                    diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::EffectOutsideCeiling,
                            effects,
                            format!(
                                "the entry performs `@{}`, which the target's `effects` does not list",
                                entry.interface().path()
                            ),
                        )
                        .with_source_id(source_id)
                        .with_related_source(
                            entry.source_id(),
                            entry.span(),
                            "the entry's `performs` entry",
                        ),
                    );
                }
            }
            if leaves.is_empty() && target.grant_keys().is_empty() {
                for (root, span) in ceiling {
                    if !signature
                        .row_entries()
                        .iter()
                        .any(|entry| entry.interface() == root)
                    {
                        diagnostics.push(
                            Diagnostic::new(
                                DiagnosticCode::ContractUnusedEffect,
                                *span,
                                format!(
                                    "neither the entry nor a startup step performs `@{}`",
                                    root.path()
                                ),
                            )
                            .with_source_id(source_id),
                        );
                    }
                }
            }
            facts = contained.then(|| EntryFacts {
                target: target.name().to_owned(),
                signature: signature.clone(),
                ceiling: ceiling.to_vec(),
            });
        }
        if !leaves.is_empty() || !target.grant_keys().is_empty() {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::ToolUnavailable,
                    target.grants_span().unwrap_or(target.span()),
                    "the binding recipes of a target are not supported by this checker yet",
                )
                .with_source_id(source_id),
            );
        }
    }
    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end(),
        )
    });
    (diagnostics, facts)
}
