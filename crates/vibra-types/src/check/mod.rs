//! The checker: a resolved graph in, diagnostics and checked IR out.
//!
//! `docs/spec/02-type-system.md` is the authority for what is well typed,
//! `docs/spec/06-runtime.md` "Module-value initialization" for the
//! initializer-cycle analysis, `docs/spec/05-tooling.md` "One source alone"
//! for [`check_source`], and `docs/spec/07-diagnostics-and-conformance.md`
//! for every diagnostic and for the `@types.v2` document.
//!
//! The checker resolves the graph first; a graph with resolution errors is
//! reported as resolution reports it and is not type-checked. It then reads
//! every declaration's header ([`declare`]), checks and lowers the bodies of
//! the modules in scope ([`expr`]), deciding the coverage of their patterns
//! ([`pattern`]), runs the initializer-cycle analysis
//! ([`cycle`]), and builds the [`CheckedModuleSet`] ([`lower`]).

mod callables;
mod cycle;
mod declare;
mod expr;
mod generics;
mod literal;
mod lower;
mod nominal;
mod pattern;

use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level};
use vibra_interp::{Execution, MemoryBudget, RuntimeError};
use vibra_ir::{CheckedModuleSet, CheckedProgram};
use vibra_resolve::{
    PackageId, ResolveInput, SourceModule, SourceUnit, UnitKind, resolve,
};

use crate::stdlib::{PRELUDE, Stdlib, load_stdlib};
use declare::{BuildInputs, PackageName, Program, Source, parse_all};

/// The package of a source checked alone.
const SOURCE_PACKAGE: &str = "source";
/// Its version.
const SOURCE_VERSION: &str = "0.0.0";
/// The one unit of a source checked alone.
const SOURCE_UNIT: &str = "@app";
/// The one module of a source checked alone.
const SOURCE_MODULE: &str = "@app.main";

/// What one check found: its diagnostics, the notes recording the
/// standard-library declarations it skipped, and, when nothing was rejected,
/// the checked module set and the `@types.v2` document.
#[derive(Clone, Debug)]
pub struct CheckResult {
    diagnostics: Vec<Diagnostic>,
    notes: Vec<String>,
    module_set: Option<Arc<CheckedModuleSet>>,
    entry: Option<usize>,
    types: Option<String>,
    functions: Vec<(String, usize)>,
}

impl CheckResult {
    fn rejected(diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            diagnostics,
            notes: Vec::new(),
            module_set: None,
            entry: None,
            types: None,
            functions: Vec::new(),
        }
    }

    /// Every diagnostic, resolution's first, then the checker's in source
    /// order of their modules.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// One note for each standard-library declaration this checker does not
    /// support yet and skipped.
    #[must_use]
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// Whether any diagnostic is an error.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.level() == Level::Error)
    }

    /// Whether every error is `@tool.unavailable`: the input uses something
    /// this checker does not support yet, and nothing it does support is
    /// wrong.
    #[must_use]
    pub fn only_unavailable(&self) -> bool {
        self.has_errors()
            && self
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.level() == Level::Error)
                .all(|diagnostic| diagnostic.code() == DiagnosticCode::ToolUnavailable)
    }

    /// The checked module set, when no error was found.
    #[must_use]
    pub const fn module_set(&self) -> Option<&Arc<CheckedModuleSet>> {
        self.module_set.as_ref()
    }

    /// The `@types.v2` document of the local declarations checked, when no
    /// error other than `@tool.unavailable` was found.
    #[must_use]
    pub fn types_document(&self) -> Option<&str> {
        self.types.as_deref()
    }

    /// The qualified name and module-set index of every function lowered.
    #[must_use]
    pub fn functions(&self) -> &[(String, usize)] {
        &self.functions
    }

    /// The module-set index of the first function with no parameter that
    /// the first local module declares.
    #[must_use]
    pub const fn entry(&self) -> Option<usize> {
        self.entry
    }
}

/// Checks the units named in `units` (`@app`, `@tests`) of a resolved
/// workspace, every module they import, and every module of the embedded
/// standard library.
#[must_use]
pub fn check_workspace(input: &ResolveInput, units: &[String]) -> CheckResult {
    let stdlib = match load_stdlib() {
        Ok(stdlib) => stdlib,
        Err(error) => return CheckResult::rejected(vec![error.diagnostic().clone()]),
    };
    let resolution = resolve(input);
    let mut diagnostics = resolution.diagnostics().to_vec();
    if resolution.has_errors() {
        return CheckResult::rejected(diagnostics);
    }
    let mut sources = Vec::new();
    for unit in input.units() {
        for module in unit.modules() {
            sources.push(Source {
                path: module.path().to_owned(),
                source_id: module.source_id().to_owned(),
                bytes: module.bytes().to_vec(),
                embedded: false,
            });
        }
    }
    for module in input.embedded().modules() {
        sources.push(Source {
            path: module.path().to_owned(),
            source_id: module.source_id().to_owned(),
            bytes: module.bytes().to_vec(),
            embedded: true,
        });
    }
    let documents = parse_all(&sources);
    let inputs = build_inputs(input, &stdlib, units);
    let program = Program::build(&sources, &documents, &inputs);
    let mut result = lower::check_program(program, &documents);
    diagnostics.append(&mut result.diagnostics);
    result.diagnostics = diagnostics;
    if result.has_errors() {
        result.module_set = None;
        if !result.only_unavailable() {
            result.types = None;
        }
        result.entry = None;
        result.functions.clear();
    }
    result
}

/// Checks one `.vib` source as the only module of a one-module library
/// workspace with the embedded standard library.
///
/// The source keeps `source_id` as its identity, so its diagnostics and
/// spans are those the workspace check reports for the same module. The
/// entry is the first function with no parameter it declares.
#[must_use]
pub fn check_source(source_id: &str, source: &str) -> CheckResult {
    let stdlib = match load_stdlib() {
        Ok(stdlib) => stdlib,
        Err(error) => return CheckResult::rejected(vec![error.diagnostic().clone()]),
    };
    let input = ResolveInput::new(
        PackageId::new(SOURCE_PACKAGE, SOURCE_VERSION),
        vec![SourceUnit::new(
            SOURCE_UNIT,
            UnitKind::Lib,
            vec![SourceModule::new(
                SOURCE_MODULE,
                source_id,
                source.as_bytes().to_vec(),
            )],
        )],
        stdlib.overlay().embedded_unit(),
    );
    check_workspace(&input, &[SOURCE_UNIT.to_owned()])
}

/// The outcome of [`interpret`].
#[derive(Clone, Debug)]
pub struct Interpretation {
    /// The check of the source.
    pub check: CheckResult,
    /// The run, when the source was accepted and has an entry.
    pub run: Option<Result<Execution, RuntimeError>>,
}

/// Checks one source as [`check_source`] does and, when it is accepted, runs
/// its entry in the reference interpreter under `budget`.
#[must_use]
pub fn interpret(
    source_id: &str,
    source: &str,
    budget: MemoryBudget,
) -> Interpretation {
    let mut check = check_source(source_id, source);
    let (Some(set), Some(entry)) = (check.module_set.clone(), check.entry) else {
        if !check.has_errors() {
            check.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::ToolUnavailable,
                    ByteSpan::empty_at(0),
                    "the source declares no function without parameters to enter",
                )
                .with_source_id(source_id),
            );
        }
        return Interpretation { check, run: None };
    };
    match CheckedProgram::for_entry(set, entry) {
        Ok(program) => Interpretation {
            check,
            run: Some(vibra_interp::run(&program, budget)),
        },
        Err(error) => {
            check.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::RuntimeInvalidCheckedProgram,
                    ByteSpan::empty_at(0),
                    format!("the checked program is not valid: {error}"),
                )
                .with_source_id(source_id),
            );
            Interpretation { check, run: None }
        }
    }
}

fn build_inputs<'s>(
    input: &ResolveInput,
    stdlib: &Stdlib,
    units: &'s [String],
) -> BuildInputs<'s> {
    let builtin = stdlib
        .modules()
        .iter()
        .find(|module| module.path().ends_with(".builtin"))
        .map(|module| module.path().to_owned());
    let prelude = PRELUDE
        .iter()
        .filter_map(|name| match stdlib.prelude().get(*name) {
            Some(declaration) => Some((
                (*name).to_owned(),
                declaration.module().to_owned(),
                declaration.name().to_owned(),
            )),
            None => builtin
                .as_ref()
                .map(|module| ((*name).to_owned(), module.clone(), (*name).to_owned())),
        })
        .collect();
    let role = |role: crate::Role| {
        stdlib.role(role).map(|declaration| {
            (
                declaration.module().to_owned(),
                declaration.name().to_owned(),
            )
        })
    };
    BuildInputs {
        local: PackageName {
            name: input.package().name().to_owned(),
            version: input.package().version().to_owned(),
        },
        embedded: PackageName {
            name: stdlib.package_name().to_owned(),
            version: stdlib.package_version().to_owned(),
        },
        units,
        prelude,
        str_role: role(crate::Role::Str),
        bool_role: role(crate::Role::Bool),
        library: stdlib
            .library()
            .iter()
            .map(|(library, declaration)| {
                (
                    *library,
                    declaration.module().to_owned(),
                    declaration.name().to_owned(),
                )
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests;
