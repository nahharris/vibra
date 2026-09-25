//! Workspace checking and pure execution over a captured project snapshot.

use std::collections::{BTreeMap, BTreeSet};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level};
use vibra_interp::{Execution, RuntimeError};
use vibra_ir::CheckedProgram;
use vibra_resolve::{EntityKind, ResolvedSnapshot};
use vibra_syntax::{Declaration, NameKind, TypeExpr};

use crate::{
    WorkspaceSnapshot,
    project::{Target, TargetKind},
};

pub use crate::test_runner::{
    TestFailure, TestItem, TestItemStatus, TestSelector, TestSuiteStatus, TestTrap,
    WorkspaceTestResult, run_tests,
};

/// Overall result of checking a workspace scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStatus {
    /// The checked scope contains no errors or unavailable features.
    Accepted,
    /// The scope contains source or project diagnostics.
    Diagnostics,
    /// The scope requires functionality unavailable in the current profile.
    Unavailable,
}

/// The result of a successful pure interpreter run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    /// The program returned a language value.
    Program(Execution),
    /// The interpreter rejected an invariant at the checked-program boundary.
    InterpreterFailure(RuntimeError),
}

/// A workspace check result with checked entry programs when the entire scope
/// is accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceCheckResult {
    status: CheckStatus,
    diagnostics: Vec<Diagnostic>,
    programs: BTreeMap<String, CheckedProgram>,
}

impl WorkspaceCheckResult {
    /// Classification of the complete checking scope.
    #[must_use]
    pub const fn status(&self) -> CheckStatus {
        self.status
    }

    /// Diagnostics in stable source/span order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Checked executable program for a target owned by the workspace.
    #[must_use]
    pub fn program_for_target(&self, target: &Target) -> Option<&CheckedProgram> {
        self.programs.get(target.name().atom().value())
    }
}

/// The result of a target check and its optional pure execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceRunResult {
    check: WorkspaceCheckResult,
    outcome: Option<RunOutcome>,
}

impl WorkspaceRunResult {
    /// The complete check performed before execution.
    #[must_use]
    pub const fn check(&self) -> &WorkspaceCheckResult {
        &self.check
    }

    /// The program result or interpreter trap, absent when checking blocked
    /// execution.
    #[must_use]
    pub const fn outcome(&self) -> Option<&RunOutcome> {
        self.outcome.as_ref()
    }
}

/// Checks every local target and each verified bootstrap module imported by
/// the workspace.
pub fn check_all(snapshot: &WorkspaceSnapshot) -> WorkspaceCheckResult {
    check_all_with_bootstrap(snapshot, None)
}

/// Checks every local target with an optional already verified bootstrap
/// package. No filesystem access occurs in this function.
pub fn check_all_with_bootstrap(
    snapshot: &WorkspaceSnapshot,
    verification: Option<&vibra_types::BootstrapVerification>,
) -> WorkspaceCheckResult {
    check_scope(snapshot, None, verification)
}

/// Checks one already selected local target and all local target units in its
/// transitive import closure. Every declaration in those units is checked.
pub fn check_target(
    snapshot: &WorkspaceSnapshot,
    target: &Target,
) -> WorkspaceCheckResult {
    check_target_with_bootstrap(snapshot, target, None)
}

/// Checks one already selected local target and its complete import closure
/// with an optional verified bootstrap package.
pub fn check_target_with_bootstrap(
    snapshot: &WorkspaceSnapshot,
    target: &Target,
    verification: Option<&vibra_types::BootstrapVerification>,
) -> WorkspaceCheckResult {
    check_scope(snapshot, Some(target), verification)
}

/// Checks a selected binary target and runs its accepted checked IR through
/// the pure reference interpreter.
pub fn run_target(snapshot: &WorkspaceSnapshot, target: &Target) -> WorkspaceRunResult {
    run_target_with_bootstrap(snapshot, target, None)
}

/// Checks a selected binary target with an optional verified bootstrap
/// package, then invokes the interpreter only after the complete scope is
/// accepted.
pub fn run_target_with_bootstrap(
    snapshot: &WorkspaceSnapshot,
    target: &Target,
    verification: Option<&vibra_types::BootstrapVerification>,
) -> WorkspaceRunResult {
    let check = check_target_with_bootstrap(snapshot, target, verification);
    let outcome = if check.status() != CheckStatus::Accepted
        || target.kind() != TargetKind::Bin
    {
        None
    } else {
        check.program_for_target(target).map(|program| {
            match vibra_interp::run(program) {
                Ok(execution) => RunOutcome::Program(execution),
                Err(error) => RunOutcome::InterpreterFailure(error),
            }
        })
    };
    WorkspaceRunResult { check, outcome }
}

fn check_scope(
    snapshot: &WorkspaceSnapshot,
    selected_target: Option<&Target>,
    verification: Option<&vibra_types::BootstrapVerification>,
) -> WorkspaceCheckResult {
    let graph = match snapshot.source_graph() {
        Ok(graph) => graph,
        Err(error) => {
            return finish_result(vec![
                Diagnostic::new(
                    DiagnosticCode::ProjectIoError,
                    ByteSpan::empty_at(0),
                    error.to_string(),
                )
                .with_source_id("project.vibon"),
            ]);
        }
    };
    let resolved = match snapshot.resolve_graph(&graph, verification) {
        Ok(resolved) => resolved,
        Err(error) => {
            return finish_result(vec![
                Diagnostic::new(
                    DiagnosticCode::ProjectIoError,
                    ByteSpan::empty_at(0),
                    error.to_string(),
                )
                .with_source_id("project.vibon"),
            ]);
        }
    };
    let mut diagnostics = graph.diagnostics().to_vec();
    let Some(target_unit) = selected_target.and_then(|target| {
        snapshot
            .project()
            .project()
            .targets()
            .iter()
            .find(|candidate| *candidate == target)
            .map(|candidate| candidate.name().atom().value().to_owned())
    }) else {
        if selected_target.is_some() {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::ProjectInvalidTargetRoot,
                    ByteSpan::empty_at(0),
                    "selected target does not belong to this workspace snapshot",
                )
                .with_source_id("project.vibon"),
            );
            return finish_result(diagnostics);
        }
        let units = graph
            .units()
            .iter()
            .filter(|unit| unit.name() != "tests")
            .map(|unit| unit.name().to_owned())
            .collect::<BTreeSet<_>>();
        return check_resolved_scope(
            snapshot,
            &resolved,
            &units,
            None,
            verification,
            diagnostics,
        );
    };
    let units = BTreeSet::from([target_unit.clone()]);
    check_resolved_scope(
        snapshot,
        &resolved,
        &units,
        Some(&target_unit),
        verification,
        diagnostics,
    )
}

fn check_resolved_scope(
    snapshot: &WorkspaceSnapshot,
    resolved: &ResolvedSnapshot,
    initial_units: &BTreeSet<String>,
    selected_unit: Option<&str>,
    verification: Option<&vibra_types::BootstrapVerification>,
    mut diagnostics: Vec<Diagnostic>,
) -> WorkspaceCheckResult {
    let (units, source_ids) = import_closure(resolved, initial_units, verification);
    let entry_spans = resolved
        .entries()
        .iter()
        .filter(|entry| units.contains(entry.unit()))
        .map(|entry| {
            (
                entry.source_id().to_owned(),
                entry.span().start(),
                entry.span().end(),
            )
        })
        .collect::<BTreeSet<_>>();
    let missing_bootstrap_spans = if verification.is_none() {
        resolved
            .imports()
            .iter()
            .filter(|import| {
                is_bootstrap_import_path(import.written())
                    && import.module().is_none_or(|module| {
                        module.package() != resolved.package()
                    })
            })
            .map(|import| {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::ToolUnavailable,
                        import.span(),
                        "the standard-library import requires verified bootstrap provenance",
                    )
                    .with_source_id(import.source_id()),
                );
                (import.source_id().to_owned(), import.span())
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    diagnostics.extend(
        resolved
            .diagnostics()
            .iter()
            .filter(|diagnostic| {
                let missing_bootstrap_path = diagnostic.code()
                    == DiagnosticCode::ModuleUnknownPath
                    && diagnostic.source_id().is_some_and(|source_id| {
                        missing_bootstrap_spans.iter().any(|(import_source, span)| {
                            import_source == source_id
                                && span.start() <= diagnostic.primary_span().start()
                                && diagnostic.primary_span().end() <= span.end()
                        })
                    });
                if missing_bootstrap_path {
                    return false;
                }
                diagnostic.source_id().is_some_and(|source_id| {
                    source_ids.contains(source_id)
                        || entry_spans.contains(&(
                            source_id.to_owned(),
                            diagnostic.primary_span().start(),
                            diagnostic.primary_span().end(),
                        ))
                })
            })
            .cloned(),
    );
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code() == DiagnosticCode::ModuleSourceIdCollision)
    {
        return finish_result(diagnostics);
    }
    diagnostics.extend(validate_entries(resolved, &units));

    let selected_sources = source_ids.iter().cloned().collect::<Vec<_>>();
    let checked =
        vibra_types::check_resolved(resolved, &selected_sources, verification);
    diagnostics.extend(checked.diagnostics().iter().cloned());
    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.source_id().unwrap_or_default().to_owned(),
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end(),
            diagnostic.code(),
        )
    });
    let status = classify(&diagnostics);
    let mut programs = BTreeMap::new();
    if status == CheckStatus::Accepted {
        let targets = snapshot.project().project().targets();
        for target in targets.iter().filter(|target| {
            units.contains(target.name().atom().value())
                && selected_unit
                    .is_none_or(|selected| target.name().atom().value() == selected)
        }) {
            if target.kind() != TargetKind::Bin {
                continue;
            }
            let Some(entry) = resolved
                .entries()
                .iter()
                .find(|entry| entry.unit() == target.name().atom().value())
            else {
                continue;
            };
            let Some(declaration) = entry.declaration() else {
                continue;
            };
            let Some(program) = checked.program_for_function(declaration) else {
                continue;
            };
            programs.insert(target.name().atom().value().to_owned(), program.clone());
        }
    }
    WorkspaceCheckResult {
        status,
        diagnostics,
        programs,
    }
}

fn import_closure(
    resolved: &ResolvedSnapshot,
    initial_units: &BTreeSet<String>,
    verification: Option<&vibra_types::BootstrapVerification>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut units = initial_units.clone();
    let project_package = resolved.package();
    let bootstrap_package =
        verification.map(vibra_types::BootstrapVerification::package);
    let mut source_ids = resolved
        .modules()
        .iter()
        .filter(|module| {
            module.package() == project_package && units.contains(module.unit())
        })
        .map(|module| module.source_id().to_owned())
        .collect::<BTreeSet<_>>();
    loop {
        let before_units = units.len();
        let before_sources = source_ids.len();
        let current_sources = source_ids.clone();
        for import in resolved
            .imports()
            .iter()
            .filter(|import| current_sources.contains(import.source_id()))
        {
            let Some(target) = import.module() else {
                continue;
            };
            if target.package() == project_package {
                units.insert(target.unit().to_owned());
                source_ids.extend(
                    resolved
                        .modules()
                        .iter()
                        .filter(|module| {
                            module.package() == project_package
                                && module.unit() == target.unit()
                        })
                        .map(|module| module.source_id().to_owned()),
                );
            } else if bootstrap_package
                .is_some_and(|package| target.package() == package)
            {
                let target_source = resolved.modules().iter().find(|module| {
                    module.package() == target.package()
                        && module.unit() == target.unit()
                        && module.segments() == target.segments()
                });
                if let Some(module) = target_source {
                    source_ids.insert(module.source_id().to_owned());
                }
            }
        }
        if units.len() == before_units && source_ids.len() == before_sources {
            break;
        }
    }
    (units, source_ids)
}

fn validate_entries(
    resolved: &ResolvedSnapshot,
    units: &BTreeSet<String>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for entry in resolved
        .entries()
        .iter()
        .filter(|entry| units.contains(entry.unit()))
    {
        let Some(id) = entry.declaration() else {
            continue;
        };
        if id.kind() != EntityKind::Function || id.path().len() != 1 {
            continue;
        }
        let Some(header) = resolved
            .declarations()
            .iter()
            .find(|declaration| declaration.id() == id)
        else {
            continue;
        };
        let Some(module) = resolved
            .modules()
            .iter()
            .find(|module| module.source_id() == header.source_id())
        else {
            continue;
        };
        let Some(function) = module.ast().and_then(|ast| {
            ast.declarations()
                .iter()
                .find_map(|declaration| match declaration {
                    Declaration::Defn(function) if function.span() == header.span() => {
                        Some(function)
                    }
                    _ => None,
                })
        }) else {
            continue;
        };
        let has_labelled_parameters =
            function.attributes().items().iter().any(|attribute| {
                matches!(
                    attribute,
                    vibra_syntax::Attribute::Labelled(entries) if !entries.is_empty()
                )
            });
        let has_variadic_parameter =
            function.attributes().items().iter().any(|attribute| {
                matches!(attribute, vibra_syntax::Attribute::Variadic(_))
            });
        let result_supported = matches!(function.result(), TypeExpr::Void)
            || is_deferred_result_type(function.result());
        if !function.parameters().is_empty()
            || has_variadic_parameter
            || has_labelled_parameters
            || !result_supported
        {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::ProjectInvalidEntrySignature,
                    entry.span(),
                    "entry function must have no parameters and return void or result void e",
                )
                .with_source_id(entry.source_id()),
            );
        }
    }
    diagnostics
}

pub(crate) fn is_bootstrap_import_path(written: &str) -> bool {
    ["std.text", "std.assert"].iter().any(|module| {
        written == *module
            || written
                .strip_prefix(module)
                .is_some_and(|suffix| suffix.starts_with('.'))
    })
}

fn is_deferred_result_type(ty: &TypeExpr) -> bool {
    let TypeExpr::Applied { head, arguments } = ty else {
        return false;
    };
    let [TypeExpr::Void, TypeExpr::Name(error_type)] = arguments.as_slice() else {
        return false;
    };
    head.kind() == NameKind::Symbol
        && error_type.kind() == NameKind::Symbol
        && head.value() == "result"
        && !matches!(
            error_type.value(),
            "bool"
                | "void"
                | "char"
                | "str"
                | "bytes"
                | "atom"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "f32"
                | "f64"
        )
}

fn classify(diagnostics: &[Diagnostic]) -> CheckStatus {
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code() == DiagnosticCode::ToolUnavailable)
    {
        CheckStatus::Unavailable
    } else if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.level() == Level::Error)
    {
        CheckStatus::Diagnostics
    } else {
        CheckStatus::Accepted
    }
}

fn finish_result(diagnostics: Vec<Diagnostic>) -> WorkspaceCheckResult {
    let mut diagnostics = diagnostics;
    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.source_id().unwrap_or_default().to_owned(),
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end(),
            diagnostic.code(),
        )
    });
    WorkspaceCheckResult {
        status: classify(&diagnostics),
        diagnostics,
        programs: BTreeMap::new(),
    }
}
