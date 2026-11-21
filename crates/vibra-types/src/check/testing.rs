//! The checks of `test` declarations and the set of tests that can run.
//!
//! `docs/spec/04-programs-and-packages.md`, "Tests" and "Assertions": a
//! `test` outside `@tests` is `@module.testOutsideTests`; a module of
//! `@tests` that declares a test contains an import that resolves to the
//! module `@std.assert` (`@module.missingRequiredImport` at its first test's
//! name). [`effects::procs::test_header`] decides whether the header of a
//! test that takes a world or performs operations is supported.
//!
//! When every error is `@tool.unavailable`, a test whose declaration
//! closure holds an unsupported form is unavailable and the other tests
//! still run: [`runnable`] drops every declaration that has, or reaches, a
//! fault and lowers the rest.

use std::collections::BTreeMap;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use super::declare::{Entity, Program};
use super::effects;
use super::expr::{Reference, Target};

/// The module path the required import resolves to.
const ASSERT_MODULE: &str = "@std.assert";

/// What [`declaration_faults`] found.
pub(crate) struct TestFaults {
    /// Every fault, in written order per module.
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// The `@tool.unavailable` of each test that takes a world or performs
    /// operations, by test index.
    pub(crate) unavailable: BTreeMap<usize, Diagnostic>,
}

/// Whether the test `index` has a body this checker checks: a test of a
/// module of `@tests` in scope whose header [`effects::procs::test_header`]
/// supports.
pub(crate) fn checked_test(program: &Program<'_>, index: usize) -> bool {
    program.tests.get(index).is_some_and(|test| {
        effects::procs::test_header(test).is_none()
            && program
                .modules
                .get(test.module)
                .is_some_and(|module| module.in_scope && module.tests)
    })
}

/// The declaration faults of the tests of the local modules in scope.
pub(crate) fn declaration_faults(program: &Program<'_>) -> TestFaults {
    let mut faults = TestFaults {
        diagnostics: Vec::new(),
        unavailable: BTreeMap::new(),
    };
    for (module_index, module) in program.modules.iter().enumerate() {
        if !module.in_scope || module.embedded {
            continue;
        }
        let source = module.source_id.as_ref();
        let tests = program
            .tests
            .iter()
            .enumerate()
            .filter(|(_, test)| test.module == module_index)
            .collect::<Vec<_>>();
        let Some((_, first)) = tests.first() else {
            continue;
        };
        if !module.tests {
            for (_, test) in &tests {
                faults.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::ModuleTestOutsideTests,
                        test.view.construct().span(),
                        "a test is declared only in a module of `@tests`",
                    )
                    .with_source_id(source),
                );
            }
            continue;
        }
        let imports_assert = module.scope.values().any(|entity| {
            matches!(entity, Entity::Module(target)
                if program
                    .modules
                    .get(*target)
                    .is_some_and(|imported| imported.path == ASSERT_MODULE))
        });
        if !imports_assert {
            faults.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::ModuleMissingRequiredImport,
                    first.name_span,
                    format!(
                        "a module that declares a test imports `{ASSERT_MODULE}`, under any name"
                    ),
                )
                .with_source_id(source),
            );
        }
        for (index, test) in tests {
            let Some(diagnostic) = effects::procs::test_header(test) else {
                continue;
            };
            let diagnostic = diagnostic.with_source_id(source);
            faults.diagnostics.push(diagnostic.clone());
            faults.unavailable.insert(index, diagnostic);
        }
    }
    faults
}

/// Why each declaration of a program whose every error is
/// `@tool.unavailable` cannot be lowered: its own first fault, or the fault
/// of a declaration it refers to, transitively. `reported` holds the
/// faults reported over declarations rather than inside bodies.
pub(crate) fn unavailable_closure(
    program: &Program<'_>,
    reported: &[Diagnostic],
    faults: &BTreeMap<Target, Vec<Diagnostic>>,
    references: &BTreeMap<Target, Vec<Reference>>,
    lowered: &dyn Fn(Target) -> bool,
    unavailable_tests: &BTreeMap<usize, Diagnostic>,
) -> BTreeMap<Target, Diagnostic> {
    let mut bad: BTreeMap<Target, Diagnostic> = BTreeMap::new();
    for (index, diagnostic) in unavailable_tests {
        bad.insert(Target::Test(*index), diagnostic.clone());
    }
    for (target, found) in faults {
        if let Some(first) = found.first() {
            bad.entry(*target).or_insert_with(|| first.clone());
        } else if !lowered(*target) {
            bad.entry(*target)
                .or_insert_with(|| unlowered(program, reported, *target));
        }
    }
    loop {
        let mut grew = false;
        for (target, refers) in references {
            if bad.contains_key(target) {
                continue;
            }
            let reason = refers.iter().find_map(|reference| {
                bad.get(&reference.target).cloned().or_else(|| {
                    (!faults.contains_key(&reference.target)
                        && !lowered(reference.target))
                    .then(|| unlowered(program, reported, reference.target))
                })
            });
            if let Some(reason) = reason {
                bad.insert(*target, reason);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    bad
}

/// The `@tool.unavailable` of a declaration that was not lowered: the one
/// reported over its declaration, or else one at the declaration.
fn unlowered(
    program: &Program<'_>,
    reported: &[Diagnostic],
    target: Target,
) -> Diagnostic {
    let (module, span, what) = match target {
        Target::Function(index) => match program.functions.get(index) {
            Some(data) => (
                data.module,
                data.view.construct().span(),
                format!("the function `{}`", data.qualified),
            ),
            None => return fallback(),
        },
        Target::Value(index) => match program.values.get(index) {
            Some(data) => (
                data.module,
                data.view.construct().span(),
                format!("the module value `{}`", data.qualified),
            ),
            None => return fallback(),
        },
        Target::Test(index) => match program.tests.get(index) {
            Some(data) => (
                data.module,
                data.view.construct().span(),
                format!("the test `{}`", data.qualified),
            ),
            None => return fallback(),
        },
    };
    let source = program
        .modules
        .get(module)
        .map(|data| data.source_id.as_ref());
    let found = reported.iter().find(|diagnostic| {
        diagnostic.code() == DiagnosticCode::ToolUnavailable
            && diagnostic.source_id() == source
            && diagnostic.primary_span().start() >= span.start()
            && diagnostic.primary_span().end() <= span.end()
    });
    found.cloned().unwrap_or_else(|| {
        let diagnostic = Diagnostic::new(
            DiagnosticCode::ToolUnavailable,
            span,
            format!("{what} uses a form this checker does not support yet"),
        );
        match source {
            Some(source) => diagnostic.with_source_id(source),
            None => diagnostic,
        }
    })
}

fn fallback() -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::ToolUnavailable,
        ByteSpan::empty_at(0),
        "a declaration uses a form this checker does not support yet",
    )
}
