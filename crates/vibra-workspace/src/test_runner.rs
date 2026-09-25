//! Pure test selection and execution over a captured project graph.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Domain, Level};
use vibra_ir::{SourceOrigin, Value};
use vibra_resolve::{DeclarationId, EntityKind, ResolvedSnapshot};
use vibra_syntax::{Declaration, Literal};

use crate::WorkspaceSnapshot;

/// An exact canonical module/name test selector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestSelector {
    module: String,
    name: String,
}

impl TestSelector {
    /// Parses the canonical `@tests.module::"name"` spelling.
    pub fn parse(value: &str) -> Option<Self> {
        let (module, literal) = value.split_once("::")?;
        if !canonical_test_module(module) {
            return None;
        }
        let source = format!("(test {literal} void)");
        let document =
            vibra_syntax::parse_source(Path::new("selector.vib"), &source).ok()?;
        if !document.accepted()
            || document.recovered()
            || !document.diagnostics().is_empty()
        {
            return None;
        }
        let ast = document.ast()?;
        let [Declaration::Test(test)] = ast.declarations() else {
            return None;
        };
        let Literal::String(name) = test.name() else {
            return None;
        };
        if canonical_string_literal(name.value()) != literal {
            return None;
        }
        Some(Self {
            module: module.to_owned(),
            name: name.value().to_owned(),
        })
    }

    /// Canonical module identity including its `@tests` unit.
    #[must_use]
    pub fn module(&self) -> &str {
        &self.module
    }

    /// Exact decoded test name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Full canonical selector spelling.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!("{}::{}", self.module, canonical_string_literal(&self.name))
    }
}

fn canonical_test_module(module: &str) -> bool {
    let Some(path) = module.strip_prefix("@tests.") else {
        return false;
    };
    let source = format!("(import module {module})");
    let Ok(document) = vibra_syntax::parse_source(Path::new("selector.vib"), &source)
    else {
        return false;
    };
    if !document.accepted()
        || document.recovered()
        || !document.diagnostics().is_empty()
    {
        return false;
    }
    let Some(ast) = document.ast() else {
        return false;
    };
    let [Declaration::Import(import)] = ast.declarations() else {
        return false;
    };
    import.target().raw() == module
        && import
            .target()
            .segments()
            .first()
            .is_some_and(|unit| unit == "tests")
        && !path.is_empty()
}

fn canonical_string_literal(value: &str) -> String {
    let mut output = String::with_capacity(value.len().saturating_add(2));
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            control if control <= '\u{1f}' || control == '\u{7f}' => {
                output.push_str(&format!("\\u{{{:x}}}", control as u32));
            }
            other => output.push(other),
        }
    }
    output.push('"');
    output
}

/// Outcome of one selected test item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestItemStatus {
    /// The body completed and every assertion passed.
    Passed,
    /// A supported assertion evaluated to false.
    AssertionFailed,
    /// Suite-wide static errors prevented all selected tests from running.
    Invalid,
    /// A valid but M2-unavailable form lies in this test's dependency closure.
    Unavailable,
    /// The interpreter encountered a runtime invariant trap.
    Trap,
}

impl TestItemStatus {
    /// Stable language outcome atom.
    #[must_use]
    pub const fn as_atom(self) -> &'static str {
        match self {
            Self::Passed => "@test.passed",
            Self::AssertionFailed => "@test.assertion-failed",
            Self::Invalid => "@test.invalid",
            Self::Unavailable => "@test.unavailable",
            Self::Trap => "@test.trap",
        }
    }
}

/// One structured pure-assertion failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestFailure {
    assertion: String,
    expected: String,
    actual: String,
    source_id: String,
    primary_span: ByteSpan,
}

impl TestFailure {
    /// Canonical assertion member atom.
    #[must_use]
    pub fn assertion(&self) -> &str {
        &self.assertion
    }

    /// Canonical expected literal.
    #[must_use]
    pub fn expected(&self) -> &str {
        &self.expected
    }

    /// Canonical actual literal.
    #[must_use]
    pub fn actual(&self) -> &str {
        &self.actual
    }

    /// Source identity owning the failed assertion call.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Primary span of the failed assertion call.
    #[must_use]
    pub const fn primary_span(&self) -> ByteSpan {
        self.primary_span
    }
}

/// One runtime invariant trap belonging to a test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestTrap {
    trap_code: String,
    origin: Option<SourceOrigin>,
}

impl TestTrap {
    /// Stable trap code.
    #[must_use]
    pub fn trap_code(&self) -> &str {
        &self.trap_code
    }

    /// Source origin, when the runtime invariant has one.
    #[must_use]
    pub const fn origin(&self) -> Option<&SourceOrigin> {
        self.origin.as_ref()
    }
}

/// One selected test and its isolated result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestItem {
    name: String,
    status: TestItemStatus,
    failure: Option<TestFailure>,
    trap: Option<TestTrap>,
    audit_trace: Vec<String>,
    diagnostics: Vec<Diagnostic>,
}

impl TestItem {
    /// Canonical selector spelling.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Outcome of this test.
    #[must_use]
    pub const fn status(&self) -> TestItemStatus {
        self.status
    }

    /// Structured false-assertion detail.
    #[must_use]
    pub const fn failure(&self) -> Option<&TestFailure> {
        self.failure.as_ref()
    }

    /// Structured trap detail.
    #[must_use]
    pub const fn trap(&self) -> Option<&TestTrap> {
        self.trap.as_ref()
    }

    /// Isolated ordered host trace, empty for M2.
    #[must_use]
    pub fn audit_trace(&self) -> &[String] {
        &self.audit_trace
    }

    /// Diagnostics attributed to this test.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// Overall workspace test-suite result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestSuiteStatus {
    /// The empty or fully passing suite.
    Ok,
    /// Required parsing or static diagnostics were found.
    Diagnostics,
    /// One or more assertion-failed tests remain after precedence.
    TestFailed,
    /// The selector is malformed at the CLI boundary or names no test.
    InvalidInput,
    /// At least one test depends on an unavailable M2 form.
    Unavailable,
    /// At least one selected test trapped, which takes precedence.
    Trap,
    /// The host stopped execution (for example, the interpreter's activation
    /// budget was exhausted). No test result is reported.
    OperationalFailure,
}

impl TestSuiteStatus {
    /// Stable command result atom.
    #[must_use]
    pub const fn as_atom(self) -> &'static str {
        match self {
            Self::Ok => "@command.ok",
            Self::Diagnostics => "@command.diagnostics",
            Self::TestFailed => "@command.test-failed",
            Self::InvalidInput => "@command.invalid-input",
            Self::Unavailable => "@command.unavailable",
            Self::Trap => "@command.trap",
            Self::OperationalFailure => "@command.operational-failure",
        }
    }
}

/// Result of selecting, checking, and running tests in one immutable snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceTestResult {
    status: TestSuiteStatus,
    diagnostics: Vec<Diagnostic>,
    items: Vec<TestItem>,
}

impl WorkspaceTestResult {
    /// Overall result.
    #[must_use]
    pub const fn status(&self) -> TestSuiteStatus {
        self.status
    }

    /// Suite diagnostics in canonical order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Selected test records in canonical discovery order.
    #[must_use]
    pub fn items(&self) -> &[TestItem] {
        &self.items
    }

    /// Number of selected records.
    #[must_use]
    pub const fn selected(&self) -> usize {
        self.items.len()
    }

    /// Number of passing tests.
    #[must_use]
    pub fn passed(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == TestItemStatus::Passed)
            .count()
    }

    /// Selected count minus the passing count.
    #[must_use]
    pub fn failed(&self) -> usize {
        self.selected().saturating_sub(self.passed())
    }
}

struct TestRef<'a> {
    module: &'a vibra_resolve::ModuleRecord,
    declaration: &'a vibra_syntax::TestDeclaration,
    id: DeclarationId,
}

/// Selects and runs the tests admitted by one workspace snapshot.
pub fn run_tests(
    workspace: &WorkspaceSnapshot,
    selector: Option<&TestSelector>,
    verification: Option<&vibra_types::BootstrapVerification>,
) -> WorkspaceTestResult {
    let graph = match workspace.source_graph() {
        Ok(graph) => graph,
        Err(error) => return diagnostics_result(error.diagnostics().to_vec()),
    };
    if !graph.diagnostics().is_empty() {
        return diagnostics_result(graph.diagnostics().to_vec());
    }
    let resolved = match workspace.resolve_graph(&graph, verification) {
        Ok(resolved) => resolved,
        Err(error) => return diagnostics_result(error.diagnostics().to_vec()),
    };
    select_tests(&resolved, selector, verification)
}

fn select_tests(
    resolved: &ResolvedSnapshot,
    selector: Option<&TestSelector>,
    verification: Option<&vibra_types::BootstrapVerification>,
) -> WorkspaceTestResult {
    let local = resolved.package();
    let modules = resolved
        .modules()
        .iter()
        .filter(|module| module.package() == local && module.unit() == "tests")
        .collect::<Vec<_>>();
    let selected_modules = if let Some(selector) = selector {
        let Some(module) = modules
            .iter()
            .find(|module| module_atom(module) == selector.module)
        else {
            return empty_result(TestSuiteStatus::InvalidInput);
        };
        vec![*module]
    } else {
        modules
    };
    let parse_sources = selected_modules
        .iter()
        .map(|module| module.source_id().to_owned())
        .collect::<BTreeSet<_>>();
    let parse_diagnostics = resolved
        .diagnostics()
        .iter()
        .filter(|diagnostic| {
            diagnostic
                .source_id()
                .is_some_and(|source| parse_sources.contains(source))
                && (diagnostic.code().domain() == Domain::Syntax
                    || diagnostic.code() == DiagnosticCode::ModuleIoError)
        })
        .cloned()
        .collect::<Vec<_>>();
    if !parse_diagnostics.is_empty() {
        return diagnostics_result(parse_diagnostics);
    }
    let mut tests = Vec::new();
    for module in &selected_modules {
        let Some(ast) = module.ast() else {
            continue;
        };
        for declaration in ast.declarations() {
            let Declaration::Test(test) = declaration else {
                continue;
            };
            let Literal::String(name) = test.name() else {
                continue;
            };
            let id = resolved
                .declarations()
                .iter()
                .find(|resolved| {
                    resolved.source_id() == module.source_id()
                        && resolved.span() == test.span()
                        && resolved.id().kind() == EntityKind::Test
                })
                .map(|resolved| resolved.id().clone())
                .unwrap_or_else(|| {
                    DeclarationId::new(
                        local.name(),
                        local.version(),
                        "tests",
                        module.segments().iter().cloned(),
                        ["$test".to_owned(), name.value().to_owned()],
                        EntityKind::Test,
                    )
                });
            if selector.is_none_or(|selector| {
                module_atom(module) == selector.module && name.value() == selector.name
            }) {
                tests.push(TestRef {
                    module,
                    declaration: test,
                    id,
                });
            }
        }
    }
    tests.sort_by(|left, right| {
        module_atom(left.module)
            .as_bytes()
            .cmp(module_atom(right.module).as_bytes())
            .then_with(|| {
                left.declaration
                    .span()
                    .start()
                    .cmp(&right.declaration.span().start())
            })
    });
    if tests.is_empty() {
        return if selector.is_some() {
            empty_result(TestSuiteStatus::InvalidInput)
        } else {
            empty_result(TestSuiteStatus::Ok)
        };
    }
    let module_closures = tests
        .iter()
        .map(|test| {
            (
                test.module.source_id().to_owned(),
                module_import_closure(resolved, test.module),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let checking_sources = module_closures
        .values()
        .flat_map(|sources| sources.iter().cloned())
        .collect::<BTreeSet<_>>();

    let mut diagnostics = duplicate_test_names(&selected_modules);
    diagnostics.extend(
        resolved
            .diagnostics()
            .iter()
            .filter(|diagnostic| {
                diagnostic
                    .source_id()
                    .is_some_and(|source_id| checking_sources.contains(source_id))
            })
            .filter(|diagnostic| {
                !is_missing_assertion_bootstrap_diagnostic(resolved, diagnostic)
            })
            .cloned(),
    );

    for module in selected_modules.iter().filter(|module| {
        tests
            .iter()
            .any(|test| test.module.source_id() == module.source_id())
    }) {
        let has_exact_import = module.ast().is_some_and(module_imports_assert);
        if !has_exact_import {
            if let Some(first_test) = module.ast().and_then(|ast| {
                ast.declarations().iter().find_map(|declaration| {
                    if let Declaration::Test(test) = declaration {
                        Some(test)
                    } else {
                        None
                    }
                })
            }) {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::ModuleMissingRequiredImport,
                        first_test.name_span(),
                        "test modules must import @std.assert",
                    )
                    .with_source_id(module.source_id()),
                );
            }
        } else if !module_has_verified_assertion_import(resolved, module, verification)
        {
            for test in tests
                .iter()
                .filter(|test| test.module.source_id() == module.source_id())
            {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::ToolUnavailable,
                        test.declaration.span(),
                        "the @std.assert module requires verified bootstrap provenance",
                    )
                    .with_source_id(module.source_id()),
                );
            }
        }
    }

    let checked = vibra_types::check_resolved(
        resolved,
        &checking_sources.iter().cloned().collect::<Vec<_>>(),
        verification,
    );
    diagnostics.extend(
        checked
            .diagnostics()
            .iter()
            .filter(|diagnostic| {
                diagnostic
                    .source_id()
                    .is_some_and(|source_id| checking_sources.contains(source_id))
            })
            .cloned(),
    );
    sort_and_deduplicate(&mut diagnostics);

    let module_diagnostics = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code() != DiagnosticCode::ToolUnavailable)
        .cloned()
        .collect::<Vec<_>>();
    let static_errors = module_diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.level() == Level::Error)
        .cloned()
        .collect::<Vec<_>>();
    if !static_errors.is_empty() {
        let mut envelope_diagnostics = module_diagnostics.clone();
        let items = tests
            .iter()
            .map(|test| {
                let closure = module_closures
                    .get(test.module.source_id())
                    .cloned()
                    .unwrap_or_default();
                let mut item_diagnostics = module_diagnostics
                    .iter()
                    .filter(|diagnostic| {
                        diagnostic_in_source_closure(diagnostic, &closure)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                item_diagnostics.extend(item_unavailable_diagnostics(
                    resolved,
                    test,
                    &diagnostics,
                ));
                envelope_diagnostics.extend(item_diagnostics.iter().cloned());
                sort_and_deduplicate(&mut item_diagnostics);
                invalid_item(test, &item_diagnostics)
            })
            .collect();
        sort_and_deduplicate(&mut envelope_diagnostics);
        return WorkspaceTestResult {
            status: TestSuiteStatus::Diagnostics,
            diagnostics: envelope_diagnostics,
            items,
        };
    }

    let mut items = Vec::with_capacity(tests.len());
    for test in &tests {
        let closure = module_closures
            .get(test.module.source_id())
            .cloned()
            .unwrap_or_default();
        let mut item_diagnostics = module_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic_in_source_closure(diagnostic, &closure))
            .cloned()
            .collect::<Vec<_>>();
        let unavailable_diagnostics =
            item_unavailable_diagnostics(resolved, test, &diagnostics);
        if !unavailable_diagnostics.is_empty() {
            item_diagnostics.extend(unavailable_diagnostics);
            sort_and_deduplicate(&mut item_diagnostics);
            items.push(TestItem {
                name: test_name(test),
                status: TestItemStatus::Unavailable,
                failure: None,
                trap: None,
                audit_trace: Vec::new(),
                diagnostics: item_diagnostics,
            });
            continue;
        }

        let Some(program) = checked.program_for_function(&test.id) else {
            let mut item =
                trap_item(test_name(test), vibra_interp::RuntimeError::NoEntry);
            item.diagnostics.extend(item_diagnostics);
            sort_and_deduplicate(&mut item.diagnostics);
            items.push(item);
            continue;
        };
        match vibra_interp::Interpreter::run_test(program) {
            Ok(execution) => {
                if let Some(failure) = execution.assertion_failure() {
                    items.push(TestItem {
                        name: test_name(test),
                        status: TestItemStatus::AssertionFailed,
                        failure: Some(TestFailure {
                            assertion: failure.assertion().to_owned(),
                            expected: canonical_assertion_value(failure.expected()),
                            actual: canonical_assertion_value(failure.actual()),
                            source_id: failure.origin().source_id().to_owned(),
                            primary_span: failure.origin().span(),
                        }),
                        trap: None,
                        audit_trace: execution.audit_trace().to_vec(),
                        diagnostics: item_diagnostics.clone(),
                    });
                } else {
                    items.push(TestItem {
                        name: test_name(test),
                        status: TestItemStatus::Passed,
                        failure: None,
                        trap: None,
                        audit_trace: execution.audit_trace().to_vec(),
                        diagnostics: item_diagnostics.clone(),
                    });
                }
            }
            Err(error) if error.is_host_event() => {
                let diagnostic = error.host_diagnostic().unwrap_or_else(|| {
                    Diagnostic::new(
                        DiagnosticCode::ProjectIoError,
                        ByteSpan::empty_at(0),
                        error.to_string(),
                    )
                });
                return WorkspaceTestResult {
                    status: TestSuiteStatus::OperationalFailure,
                    diagnostics: vec![diagnostic],
                    items: Vec::new(),
                };
            }
            Err(error) => {
                let mut item = trap_item(test_name(test), error);
                item.diagnostics.extend(item_diagnostics);
                sort_and_deduplicate(&mut item.diagnostics);
                items.push(item);
            }
        }
    }

    let mut envelope_diagnostics = module_diagnostics;
    envelope_diagnostics.extend(
        items
            .iter()
            .flat_map(|item| item.diagnostics.iter().cloned()),
    );
    sort_and_deduplicate(&mut envelope_diagnostics);
    let status = if items.iter().any(|item| item.status == TestItemStatus::Trap) {
        TestSuiteStatus::Trap
    } else if items
        .iter()
        .any(|item| item.status == TestItemStatus::Unavailable)
    {
        TestSuiteStatus::Unavailable
    } else if items
        .iter()
        .any(|item| item.status == TestItemStatus::AssertionFailed)
    {
        TestSuiteStatus::TestFailed
    } else {
        TestSuiteStatus::Ok
    };
    WorkspaceTestResult {
        status,
        diagnostics: envelope_diagnostics,
        items,
    }
}

fn module_imports_assert(ast: &vibra_syntax::SourceAst) -> bool {
    ast.declarations().iter().any(|declaration| {
        matches!(declaration, Declaration::Import(import)
            if import.target().segments().len() == 2
                && import.target().segments().first().is_some_and(|segment| segment == "std")
                && import.target().segments().get(1).is_some_and(|segment| segment == "assert"))
    })
}

fn module_has_verified_assertion_import(
    resolved: &ResolvedSnapshot,
    module: &vibra_resolve::ModuleRecord,
    verification: Option<&vibra_types::BootstrapVerification>,
) -> bool {
    let Some(verification) = verification else {
        return false;
    };
    let (package, overlay) = verification.resolver_overlay();
    if &package != verification.package() {
        return false;
    }
    let Some(assertion_source) = overlay
        .iter()
        .find(|source| source.unit() == "std" && source.segments() == ["assert"])
    else {
        return false;
    };
    resolved.imports().iter().any(|import| {
        if import.source_id() != module.source_id()
            || import.written().trim_start_matches('@') != "std.assert"
        {
            return false;
        }
        let Some(target) = import.module() else {
            return false;
        };
        target.package() == &package
            && target.unit() == assertion_source.unit()
            && target.segments() == assertion_source.segments()
            && resolved.modules().iter().any(|resolved_module| {
                resolved_module.package() == &package
                    && resolved_module.unit() == assertion_source.unit()
                    && resolved_module.segments() == assertion_source.segments()
                    && resolved_module.source_id() == assertion_source.source_id()
                    && resolved_module.bytes() == assertion_source.bytes()
            })
    })
}

fn module_import_closure(
    resolved: &ResolvedSnapshot,
    start: &vibra_resolve::ModuleRecord,
) -> BTreeSet<String> {
    let mut sources = BTreeSet::from([start.source_id().to_owned()]);
    loop {
        let before = sources.len();
        let current = sources.clone();
        for import in resolved
            .imports()
            .iter()
            .filter(|import| current.contains(import.source_id()))
        {
            let Some(target) = import.module() else {
                continue;
            };
            if let Some(module) = resolved.modules().iter().find(|candidate| {
                candidate.package() == target.package()
                    && candidate.unit() == target.unit()
                    && candidate.segments() == target.segments()
            }) {
                sources.insert(module.source_id().to_owned());
            }
        }
        if sources.len() == before {
            break;
        }
    }
    sources
}

fn is_missing_assertion_bootstrap_diagnostic(
    resolved: &ResolvedSnapshot,
    diagnostic: &Diagnostic,
) -> bool {
    let missing_imports = resolved
        .imports()
        .iter()
        .filter(|import| {
            import.written().trim_start_matches('@') == "std.assert"
                && import.module().is_none()
                && diagnostic.source_id() == Some(import.source_id())
        })
        .collect::<Vec<_>>();
    missing_imports.iter().any(|import| {
        (diagnostic.code() == DiagnosticCode::ModuleUnknownPath
            && diagnostic.primary_span() == import.span())
            || (diagnostic.code() == DiagnosticCode::NameUnknownSymbol
                && resolved.references().iter().any(|reference| {
                    reference.source_id() == import.source_id()
                        && reference.target().is_none()
                        && reference.span() == diagnostic.primary_span()
                        && reference
                            .written()
                            .strip_prefix(import.alias())
                            .is_some_and(|suffix| suffix.starts_with('.'))
                }))
    })
}

fn declaration_dependency_closure(
    resolved: &ResolvedSnapshot,
    start: &DeclarationId,
) -> BTreeSet<DeclarationId> {
    let mut closure = BTreeSet::from([start.clone()]);
    loop {
        let before = closure.len();
        let current = closure.clone();
        for reference in resolved
            .references()
            .iter()
            .filter(|reference| current.contains(reference.from()))
        {
            if let Some(target) = reference.target() {
                closure.insert(target.clone());
            }
        }
        if closure.len() == before {
            break;
        }
    }
    closure
}

fn item_unavailable_diagnostics(
    resolved: &ResolvedSnapshot,
    test: &TestRef<'_>,
    diagnostics: &[Diagnostic],
) -> Vec<Diagnostic> {
    let dependency_closure = declaration_dependency_closure(resolved, &test.id);
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code() == DiagnosticCode::ToolUnavailable)
        .filter(|diagnostic| {
            diagnostic_owner(resolved, diagnostic)
                .is_some_and(|owner| dependency_closure.contains(owner))
        })
        .cloned()
        .collect()
}

fn diagnostic_owner<'a>(
    resolved: &'a ResolvedSnapshot,
    diagnostic: &Diagnostic,
) -> Option<&'a DeclarationId> {
    let source = diagnostic.source_id()?;
    let span = diagnostic.primary_span();
    resolved
        .declarations()
        .iter()
        .filter(|declaration| declaration.source_id() == source)
        .filter(|declaration| {
            declaration.span().start() <= span.start()
                && span.end() <= declaration.span().end()
        })
        .min_by_key(|declaration| declaration.span().len())
        .map(|declaration| declaration.id())
}

fn diagnostic_in_source_closure(
    diagnostic: &Diagnostic,
    source_ids: &BTreeSet<String>,
) -> bool {
    diagnostic
        .source_id()
        .is_some_and(|source_id| source_ids.contains(source_id))
}

fn canonical_assertion_value(value: &Value) -> String {
    match value {
        Value::Str(value) => canonical_string_literal(value),
        _ => value.canonical_vibon(),
    }
}

#[cfg(test)]
mod provenance_tests {

    use vibra_resolve::{ResolveInput, Resolver, SourceModule, SourceUnit};

    use super::{TestSuiteStatus, select_tests};

    #[test]
    fn local_assertion_import_cannot_satisfy_verified_assertion_contract() {
        let test_source = "(import assert @std.assert)\n(test \"spoofed\" (assert.equal-bool false true))\n";
        let fake_assertions = "(defn equal-bool (expected bool actual bool) void visibility: @public void)\n";
        let resolved = Resolver::resolve(ResolveInput::new(
            "demo",
            "0.1.0",
            vec![
                SourceUnit::lib(
                    "std",
                    vec![SourceModule::new(
                        "std",
                        ["assert"],
                        "src/std/assert.vib",
                        fake_assertions,
                    )],
                ),
                SourceUnit::lib(
                    "tests",
                    vec![SourceModule::new(
                        "tests",
                        ["math"],
                        "tests/math.vib",
                        test_source,
                    )],
                ),
            ],
        ));
        let verification =
            vibra_types::verify_bootstrap().expect("signed bootstrap verification");

        let result = select_tests(&resolved, None, Some(&verification));

        assert_eq!(
            result.status(),
            TestSuiteStatus::Unavailable,
            "{:?}",
            result.diagnostics()
        );
        assert!(result.items()[0].diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == vibra_diagnostics::DiagnosticCode::ToolUnavailable
        }));
    }
}

fn sort_and_deduplicate(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.sort_by_key(diagnostic_key);
    diagnostics.dedup_by(|left, right| {
        left.code() == right.code()
            && left.source_id() == right.source_id()
            && left.primary_span() == right.primary_span()
    });
}

fn module_atom(module: &vibra_resolve::ModuleRecord) -> String {
    if module.segments().is_empty() {
        "@tests".to_owned()
    } else {
        format!("@tests.{}", module.segments().join("."))
    }
}

fn test_name(test: &TestRef<'_>) -> String {
    let Literal::String(name) = test.declaration.name() else {
        return String::new();
    };
    format!(
        "{}::{}",
        module_atom(test.module),
        canonical_string_literal(name.value())
    )
}

fn duplicate_test_names(modules: &[&vibra_resolve::ModuleRecord]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for module in modules {
        let Some(ast) = module.ast() else {
            continue;
        };
        let mut names = BTreeMap::<String, ByteSpan>::new();
        for declaration in ast.declarations() {
            let Declaration::Test(test) = declaration else {
                continue;
            };
            let Literal::String(name) = test.name() else {
                continue;
            };
            if let Some(earlier) = names.get(name.value()).copied() {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::NameRedeclaration,
                        test.name_span(),
                        "a test name is repeated within one module",
                    )
                    .with_source_id(module.source_id())
                    .with_related_source(
                        module.source_id(),
                        earlier,
                        "the earlier test name is here",
                    ),
                );
            } else {
                names.insert(name.value().to_owned(), test.name_span());
            }
        }
    }
    diagnostics.sort_by_key(diagnostic_key);
    diagnostics
}

fn invalid_item(test: &TestRef<'_>, diagnostics: &[Diagnostic]) -> TestItem {
    TestItem {
        name: test_name(test),
        status: TestItemStatus::Invalid,
        failure: None,
        trap: None,
        audit_trace: Vec::new(),
        diagnostics: diagnostics.to_vec(),
    }
}

fn trap_item(name: String, _error: vibra_interp::RuntimeError) -> TestItem {
    let diagnostic = Diagnostic::new(
        DiagnosticCode::RuntimeInvalidCheckedProgram,
        ByteSpan::empty_at(0),
        "checked program violated M2 runtime invariants",
    );
    TestItem {
        name,
        status: TestItemStatus::Trap,
        failure: None,
        trap: Some(TestTrap {
            trap_code: "@runtime.invalid-checked-program".to_owned(),
            origin: None,
        }),
        audit_trace: Vec::new(),
        diagnostics: vec![diagnostic],
    }
}

fn diagnostics_result(mut diagnostics: Vec<Diagnostic>) -> WorkspaceTestResult {
    diagnostics.sort_by_key(diagnostic_key);
    WorkspaceTestResult {
        status: TestSuiteStatus::Diagnostics,
        diagnostics,
        items: Vec::new(),
    }
}

fn empty_result(status: TestSuiteStatus) -> WorkspaceTestResult {
    WorkspaceTestResult {
        status,
        diagnostics: Vec::new(),
        items: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{TestItemStatus, trap_item};
    use vibra_diagnostics::{ByteSpan, DiagnosticCode};

    #[test]
    fn runtime_checked_program_trap_is_unlocated_and_has_the_closed_code() {
        let item = trap_item(
            "@tests.math::\"traps\"".to_owned(),
            vibra_interp::RuntimeError::InvalidBody {
                function: "test".to_owned(),
            },
        );

        assert_eq!(item.status(), TestItemStatus::Trap);
        let trap = item.trap().expect("structured trap");
        assert_eq!(trap.trap_code(), "@runtime.invalid-checked-program");
        assert!(trap.origin().is_none());
        let [diagnostic] = item.diagnostics() else {
            panic!("one trap diagnostic");
        };
        assert_eq!(
            diagnostic.code(),
            DiagnosticCode::RuntimeInvalidCheckedProgram
        );
        assert_eq!(diagnostic.primary_span(), ByteSpan::empty_at(0));
        assert!(diagnostic.source_id().is_none());
    }
}

fn diagnostic_key(diagnostic: &Diagnostic) -> (String, usize, usize, DiagnosticCode) {
    (
        diagnostic.source_id().unwrap_or_default().to_owned(),
        diagnostic.primary_span().start(),
        diagnostic.primary_span().end(),
        diagnostic.code(),
    )
}
