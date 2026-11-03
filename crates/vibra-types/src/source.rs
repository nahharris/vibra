//! Checking one source document: the only module of a one-module workspace.
//!
//! There is one checker. A source a tool checks on its own is resolved with
//! the embedded standard library as a workspace of one module, under the
//! source ID it was given, and checked by [`check_resolved`] as any module of a
//! project is. Nothing here reads a file, and the only modules the source can
//! import are the standard library's.

use std::collections::BTreeSet;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level};
use vibra_ir::{CheckedProgram, IrError};
use vibra_resolve::{
    ResolveInput, ResolvedSnapshot, Resolver, SourceModule, SourceUnit,
};
use vibra_syntax::{ApplicationBinding, EffectRowOrder};

use crate::{ResolvedCheckResult, Stdlib, check_resolved, load_stdlib};

/// The package, unit, and module a source checked on its own is given. They
/// name nothing the source can write; the source ID is its identity.
const PACKAGE_NAME: &str = "source";
const PACKAGE_VERSION: &str = "0.0.0";
const UNIT: &str = "app";
const MODULE: &str = "main";

/// The result of checking one source document as a one-module workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckResult {
    program: Option<CheckedProgram>,
    diagnostics: Vec<Diagnostic>,
    bindings: Vec<ApplicationBinding>,
    effect_rows: Vec<EffectRowOrder>,
}

impl CheckResult {
    /// The checked program of an accepted source, whose entry is the first
    /// function the source declares. Absent when the source declares no
    /// function, so callers must use [`Self::accepted`] separately from this
    /// accessor.
    #[must_use]
    pub const fn program(&self) -> Option<&CheckedProgram> {
        self.program.as_ref()
    }

    /// Diagnostics in deterministic source order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Authoritative call binding facts for accepted applications.
    ///
    /// The facts are intentionally returned by the checker rather than
    /// inferred by the syntax or formatting crates. A formatter may use these
    /// entries to normalize labelled operand order only after the
    /// corresponding call has passed semantic checking.
    #[must_use]
    pub fn application_bindings(&self) -> &[ApplicationBinding] {
        &self.bindings
    }

    /// The canonical order of every effect row the source writes, sorted by
    /// resolved root identity. The formatter reorders a row by it and keeps
    /// each lexical reference the row's imports require.
    #[must_use]
    pub fn effect_row_orders(&self) -> &[EffectRowOrder] {
        &self.effect_rows
    }

    /// Whether checking accepted this source document.
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|diagnostic| diagnostic.level() != Level::Error)
    }
}

/// The resolver input every check of the toolchain starts from: the project's
/// `units`, the closed names of the language core, and, when the standard
/// library is loaded, its modules as the verified overlay.
#[must_use]
pub fn standard_resolve_input(
    package_name: &str,
    package_version: &str,
    units: Vec<SourceUnit>,
    verification: Option<&Stdlib>,
) -> ResolveInput {
    let mut input = ResolveInput::new(package_name, package_version, units)
        .with_reserved_import_paths(
            [
                "text", "option", "result", "core", "bool", "char", "bytes", "assert",
            ]
            .map(|module| ("std".to_owned(), vec![module.to_owned()])),
        )
        .with_builtin_members(crate::builtin_member_names())
        .with_assertion_members(crate::assertion_member_names())
        .with_role_types(crate::role_type_names().into_iter().map(|(_, name)| name));
    if let Some(verification) = verification {
        let (overlay_package, modules) = verification.resolver_overlay();
        input = input.with_verified_overlay(
            overlay_package.name(),
            overlay_package.version(),
            modules,
        );
    }
    input
}

/// Checks one `.vib` source document as the only module of a one-module
/// workspace with the embedded standard library, and lowers it to checked IR.
///
/// The document keeps `source_id` as its identity, so every diagnostic and
/// span is the one the workspace check reports for the same module. The
/// program, when the source is accepted, is entered at the first function the
/// source declares.
pub fn check_source(source_id: impl AsRef<str>, source: &str) -> CheckResult {
    let source_id = source_id.as_ref();
    let stdlib = match load_stdlib() {
        Ok(stdlib) => stdlib,
        Err(error) => {
            return rejected(vec![
                Diagnostic::new(
                    DiagnosticCode::ToolUnavailable,
                    ByteSpan::empty_at(0),
                    format!("the embedded standard library does not verify: {error}"),
                )
                .with_source_id(source_id),
            ]);
        }
    };
    let unit = SourceUnit::lib(
        UNIT,
        vec![SourceModule::new(
            UNIT,
            [MODULE],
            source_id,
            source.as_bytes(),
        )],
    );
    let resolved = Resolver::resolve(standard_resolve_input(
        PACKAGE_NAME,
        PACKAGE_VERSION,
        vec![unit],
        Some(&stdlib),
    ));
    let scope = scope_of(&resolved, &stdlib, source_id);
    let selected = scope.iter().cloned().collect::<Vec<_>>();
    let checked = check_resolved(&resolved, &selected, Some(&stdlib));
    let mut diagnostics = resolved
        .diagnostics()
        .iter()
        .filter(|diagnostic| {
            diagnostic
                .source_id()
                .is_some_and(|diagnostic_source| scope.contains(diagnostic_source))
        })
        .cloned()
        .chain(checked.diagnostics().iter().cloned())
        .collect::<Vec<_>>();
    let program = if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.level() == Level::Error)
    {
        None
    } else {
        entry_program(&checked, source_id, &mut diagnostics)
    };
    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.source_id().unwrap_or_default().to_owned(),
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end(),
            diagnostic.code(),
        )
    });
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.level() == Level::Error)
    {
        return rejected(diagnostics);
    }
    CheckResult {
        program,
        diagnostics,
        bindings: checked.application_bindings(source_id).to_vec(),
        effect_rows: checked.effect_row_orders(source_id).to_vec(),
    }
}

fn rejected(diagnostics: Vec<Diagnostic>) -> CheckResult {
    CheckResult {
        program: None,
        diagnostics,
        bindings: Vec::new(),
        effect_rows: Vec::new(),
    }
}

/// The source and every standard-library module it imports, transitively:
/// the modules the workspace check of a project selects for the same source.
fn scope_of(
    resolved: &ResolvedSnapshot,
    stdlib: &Stdlib,
    source_id: &str,
) -> BTreeSet<String> {
    let mut scope = BTreeSet::from([source_id.to_owned()]);
    loop {
        let imported = resolved
            .imports()
            .iter()
            .filter(|import| scope.contains(import.source_id()))
            .filter_map(|import| import.module())
            .filter(|target| target.package() == stdlib.package())
            .filter_map(|target| {
                resolved.modules().iter().find(|module| {
                    module.package() == target.package()
                        && module.unit() == target.unit()
                        && module.segments() == target.segments()
                })
            })
            .map(|module| module.source_id().to_owned())
            .collect::<Vec<_>>();
        let before = scope.len();
        scope.extend(imported);
        if scope.len() == before {
            return scope;
        }
    }
}

/// The program whose entry is the first function checked from `source_id`.
fn entry_program(
    checked: &ResolvedCheckResult,
    source_id: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CheckedProgram> {
    let set = checked.module_set()?;
    let entry = set
        .functions()
        .iter()
        .position(|function| function.origin().source_id() == source_id)?;
    match CheckedProgram::for_entry(std::sync::Arc::clone(set), entry) {
        Ok(program) => Some(program),
        Err(IrError::GlobalInitializerCycle(index)) => {
            if let Some(global) = set.globals().get(index) {
                diagnostics.push(crate::initializer_cycle_diagnostic(global.origin()));
            }
            None
        }
        Err(error) => {
            crate::unavailable(
                diagnostics,
                source_id,
                ByteSpan::empty_at(0),
                format!("checked IR construction failed: {error}"),
            );
            None
        }
    }
}
