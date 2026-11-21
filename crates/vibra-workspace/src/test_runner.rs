//! The test runner: discovery, selection, the check of the selection, one
//! fresh execution per test, and the `@testRun.v2` observation.
//!
//! `docs/spec/04-programs-and-packages.md`, "Tests" and "Assertions",
//! `docs/spec/05-tooling.md`, "Tests and selection", and
//! `docs/spec/07-diagnostics-and-conformance.md`, `@testRun.v2`, are the
//! authority.
//!
//! Tests are discovered in ascending module path by UTF-8 bytes, then in
//! declaration order. Without a selector every test module is parsed; with
//! one, its module and the test modules it imports are, and no other test
//! module is given to resolution. Parse errors of the parsed test modules
//! are reported first, with no items. Every selected test is then checked with its
//! module's import closure before any runs: an ordinary error makes every
//! item `@test.invalid`, and a test whose declaration closure reaches a form
//! this toolchain does not implement is `@test.unavailable` while the
//! others run. Each test runs in a new interpreter, so module values start
//! fresh and the audit trace is empty.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level};
use vibra_interp::{
    FailureKind, Instrumentation, MemoryBudget, RuntimeError, TestAssertionFailure,
    run_test,
};
use vibra_ir::{CheckedModuleSet, CheckedProgram, SourceOrigin, Trap, encode_type};
use vibra_syntax::source::Declaration;
use vibra_syntax::{canonical_string, decode_string, parse_source};
use vibra_types::{CheckResult, check_workspace};

use crate::resolve::resolve_input_of;
use crate::revision::workspace_revision;
use crate::snapshot::{SourceFile, SourceSnapshot, UnitKind};

/// The format atom of the observation.
pub const TEST_RUN_FORMAT: &str = "@testRun.v2";

/// The format atom of each test's audit trace.
const AUDIT_TRACE_FORMAT: &str = "@auditTrace.v2";

/// The provider of the default fake world each test runs in.
const WORLD_PROVIDER: &str = "@hostWorld.v2";

/// The version of that world's driver.
const WORLD_DRIVER_VERSION: &str = "0.0.2";

/// The unit every test module belongs to, and the prefix of a selector's
/// module.
const TESTS_UNIT: &str = "@tests";

/// One selected test: `@tests.math::"adds one"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestSelector {
    module: String,
    name: String,
}

impl TestSelector {
    /// Reads a selector: a module atom that begins `@tests.`, `::`, and the
    /// test's name as one canonical string literal, with no other
    /// whitespace.
    ///
    /// # Errors
    ///
    /// The text is not a selector: invalid input.
    pub fn parse(text: &str) -> Result<Self, InvalidSelection> {
        let malformed = || InvalidSelection::Malformed(text.to_owned());
        let (module, literal) = text.split_once("::").ok_or_else(malformed)?;
        let segments = module
            .strip_prefix(TESTS_UNIT)
            .and_then(|rest| rest.strip_prefix('.'))
            .ok_or_else(malformed)?;
        if !segments.split('.').all(is_identifier) {
            return Err(malformed());
        }
        let name = decode_string(literal).ok_or_else(malformed)?;
        if canonical_string(&name) != literal {
            return Err(malformed());
        }
        Ok(Self {
            module: module.to_owned(),
            name,
        })
    }

    /// The module path atom.
    #[must_use]
    pub fn module(&self) -> &str {
        &self.module
    }

    /// The decoded test name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for TestSelector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}::{}",
            self.module,
            canonical_string(&self.name)
        )
    }
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|next| next.is_ascii_alphanumeric())
}

/// A selection that is invalid input: it has no diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvalidSelection {
    /// The selector is not well formed.
    Malformed(String),
    /// The selector names no test module, or no test of its module.
    Unknown(String),
}

impl fmt::Display for InvalidSelection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(text) => {
                write!(formatter, "`{text}` is not a test selector")
            }
            Self::Unknown(text) => write!(formatter, "`{text}` names no test"),
        }
    }
}

impl std::error::Error for InvalidSelection {}

/// How one test ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestOutcome {
    /// `@test.passed`: the body completed with no failed assertion.
    Passed,
    /// `@test.assertionFailed`: an assertion was false, or an operand could
    /// not be observed; the body stopped there.
    AssertionFailed(TestAssertionFailure),
    /// `@test.trap`: a toolchain defect trap ended the test.
    Trap {
        /// The trap.
        trap: Trap,
        /// Where the trapping node was written, when one was.
        origin: Option<SourceOrigin>,
    },
    /// `@test.invalid`: the selection had static errors.
    Invalid,
    /// `@test.unavailable`: the test's declaration closure reaches a form
    /// this toolchain does not implement, the `@tool.unavailable` at it.
    Unavailable(Diagnostic),
    /// `@test.exhausted`: the test exceeded its memory budget, the host
    /// event `@runtime.memoryExhausted`. It is never counted as passed.
    Exhausted,
}

impl TestOutcome {
    /// The `@test.*` result atom.
    #[must_use]
    pub const fn result(&self) -> &'static str {
        match self {
            Self::Passed => "@test.passed",
            Self::AssertionFailed(_) => "@test.assertionFailed",
            Self::Trap { .. } => "@test.trap",
            Self::Invalid => "@test.invalid",
            Self::Unavailable(_) => "@test.unavailable",
            Self::Exhausted => "@test.exhausted",
        }
    }
}

/// One selected test and how it ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestItem {
    selector: TestSelector,
    outcome: TestOutcome,
    instrumentation: Option<Instrumentation>,
}

impl TestItem {
    /// The test's selector.
    #[must_use]
    pub const fn selector(&self) -> &TestSelector {
        &self.selector
    }

    /// How the test ended.
    #[must_use]
    pub const fn outcome(&self) -> &TestOutcome {
        &self.outcome
    }

    /// What the test's execution measured, when it ran to an end of its
    /// own: passed or failed an assertion.
    #[must_use]
    pub const fn instrumentation(&self) -> Option<&Instrumentation> {
        self.instrumentation.as_ref()
    }
}

/// The suite's `@command.*` result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuiteResult {
    /// `@command.ok`.
    Ok,
    /// `@command.diagnostics`: parse or check errors.
    Diagnostics,
    /// `@command.testFailed`: a test failed an assertion.
    TestFailed,
    /// `@command.operationalFailure`: a test exhausted its budget.
    OperationalFailure,
    /// `@command.unavailable`: a test needs an unsupported construct.
    Unavailable,
    /// `@command.trap`: a test trapped.
    Trap,
}

impl SuiteResult {
    /// The atom.
    #[must_use]
    pub const fn as_atom(self) -> &'static str {
        match self {
            Self::Ok => "@command.ok",
            Self::Diagnostics => "@command.diagnostics",
            Self::TestFailed => "@command.testFailed",
            Self::OperationalFailure => "@command.operationalFailure",
            Self::Unavailable => "@command.unavailable",
            Self::Trap => "@command.trap",
        }
    }

    /// The suite result of tests that were checked and ran, by the
    /// precedence trap, operational exhaustion, unavailable, test failed,
    /// ok.
    #[must_use]
    pub fn by_precedence<'a>(
        outcomes: impl IntoIterator<Item = &'a TestOutcome>,
    ) -> Self {
        let rank = |outcome: &TestOutcome| match outcome {
            TestOutcome::Trap { .. } => 4,
            TestOutcome::Exhausted => 3,
            TestOutcome::Unavailable(_) => 2,
            TestOutcome::AssertionFailed(_) => 1,
            TestOutcome::Passed | TestOutcome::Invalid => 0,
        };
        match outcomes.into_iter().map(rank).max() {
            Some(4) => Self::Trap,
            Some(3) => Self::OperationalFailure,
            Some(2) => Self::Unavailable,
            Some(1) => Self::TestFailed,
            _ => Self::Ok,
        }
    }
}

/// One run of the suite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestRun {
    result: SuiteResult,
    items: Vec<TestItem>,
    diagnostics: Vec<Diagnostic>,
    revision: String,
}

impl TestRun {
    /// The suite result, by the precedence trap, operational exhaustion,
    /// unavailable, test failed, ok.
    #[must_use]
    pub const fn result(&self) -> SuiteResult {
        self.result
    }

    /// Every selected test, in discovery order; empty when parse errors
    /// were reported.
    #[must_use]
    pub fn items(&self) -> &[TestItem] {
        &self.items
    }

    /// Every diagnostic, once.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The number of selected tests.
    #[must_use]
    pub fn selected(&self) -> usize {
        self.items.len()
    }

    /// The number of `@test.passed` items.
    #[must_use]
    pub fn passed(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.outcome == TestOutcome::Passed)
            .count()
    }

    /// The selected tests that did not pass.
    #[must_use]
    pub fn failed(&self) -> usize {
        self.selected() - self.passed()
    }

    /// The workspace revision the run was taken from.
    #[must_use]
    pub fn revision(&self) -> &str {
        &self.revision
    }

    /// The canonical text of the `@testRun.v2` observation, with every item,
    /// an exhausted one included.
    #[must_use]
    pub fn document(&self) -> String {
        let tests = self
            .items
            .iter()
            .map(|item| self.record(item))
            .collect::<Vec<_>>();
        format!(
            "{{format: {TEST_RUN_FORMAT} result: {} tests: [{}]}}",
            self.result.as_atom(),
            tests.join(" ")
        )
    }

    fn record(&self, item: &TestItem) -> String {
        let mut fields = vec![
            format!("name: {}", canonical_string(&item.selector.to_string())),
            format!("result: {}", item.outcome.result()),
        ];
        match &item.outcome {
            TestOutcome::AssertionFailed(failure) => {
                fields.push(format!("failure: {}", failure_record(failure)));
            }
            TestOutcome::Trap { trap, origin } => {
                let origin = origin
                    .as_ref()
                    .map(|origin| format!(" origin: {}", span_record(origin)))
                    .unwrap_or_default();
                fields.push(format!(
                    "trap: {{trapCode: {}{origin}}}",
                    trap.code().as_atom()
                ));
            }
            _ => {}
        }
        fields.push(format!(
            "auditTrace: {{format: {AUDIT_TRACE_FORMAT} revision: {} drivers: \
             [{{provider: {WORLD_PROVIDER} version: {}}}] policies: [:] redaction: @none \
             events: []}}",
            canonical_string(&self.revision),
            canonical_string(WORLD_DRIVER_VERSION)
        ));
        format!("{{{}}}", fields.join(" "))
    }
}

fn failure_record(failure: &TestAssertionFailure) -> String {
    let kind = match failure.kind {
        FailureKind::Assertion => "@assertion",
        FailureKind::Observation { .. } => "@observation",
    };
    let mut fields = vec![
        format!("kind: {kind}"),
        format!("assertion: {}", canonical_string(&failure.assertion)),
    ];
    if let Some(expected) = &failure.expected {
        fields.push(format!("expected: {}", canonical_string(expected)));
    }
    if let Some(actual) = &failure.actual {
        fields.push(format!("actual: {}", canonical_string(actual)));
    }
    fields.push(format!("primarySpan: {}", span_record(&failure.origin)));
    if let FailureKind::Observation { code, .. } = &failure.kind {
        let type_info = observation_type(failure)
            .map(|encoded| format!(" typeInfo: {encoded}"))
            .unwrap_or_default();
        fields.push(format!(
            "observationError: {{code: {}{type_info}}}",
            code.as_atom()
        ));
    }
    format!("{{{}}}", fields.join(" "))
}

/// The canonical `@type.v2` text of the operand an observation failure
/// could not observe; `None` for a false assertion, or when the type has no
/// encoding.
#[must_use]
pub fn observation_type(failure: &TestAssertionFailure) -> Option<String> {
    match &failure.kind {
        FailureKind::Observation { type_info, .. } => encode_type(type_info).ok(),
        FailureKind::Assertion => None,
    }
}

fn span_record(origin: &SourceOrigin) -> String {
    let span = origin.span();
    format!(
        "{{sourceId: {} start: {}u64 end: {}u64}}",
        canonical_string(origin.source_id()),
        span.start(),
        span.end()
    )
}

/// One discovered test.
struct Discovered {
    selector: TestSelector,
}

/// Runs the tests of `snapshot` that `selector` selects, every test with no
/// selector, each under `budget`.
///
/// # Errors
///
/// The selector names no test module or no test of its module: invalid
/// input, with no diagnostics.
pub fn run_tests(
    snapshot: &SourceSnapshot,
    selector: Option<&TestSelector>,
    budget: MemoryBudget,
) -> Result<TestRun, InvalidSelection> {
    let revision = workspace_revision(&snapshot.revision_records()).unwrap_or_default();
    let modules = test_modules(snapshot, selector)?;
    let mut parse_errors = Vec::new();
    let mut discovered = Vec::new();
    for module in &modules {
        let document = parse_source(Some(module.source_id()), module.bytes());
        parse_errors.extend(
            document
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.level() == Level::Error)
                .cloned(),
        );
        let Some(view) = vibra_syntax::module(&document) else {
            continue;
        };
        for declaration in view.declarations() {
            if let Declaration::Test(test) = declaration
                && let Some(name) = test.name_value()
            {
                discovered.push(Discovered {
                    selector: TestSelector {
                        module: module.path().to_owned(),
                        name,
                    },
                });
            }
        }
    }
    if !parse_errors.is_empty() {
        return Ok(TestRun {
            result: SuiteResult::Diagnostics,
            items: Vec::new(),
            diagnostics: parse_errors,
            revision,
        });
    }
    if let Some(selector) = selector {
        discovered.retain(|test| test.selector == *selector);
        if discovered.is_empty() {
            return Err(InvalidSelection::Unknown(selector.to_string()));
        }
    }
    let (check, diagnostics) = match check_selection(snapshot, selector, &modules) {
        Ok(check) => {
            let diagnostics = check.diagnostics().to_vec();
            (Some(check), diagnostics)
        }
        Err(defect) => (None, vec![*defect]),
    };
    let check = check.filter(|check| !check.has_errors() || check.only_unavailable());
    let invalid = check.is_none();
    let items = discovered
        .into_iter()
        .enumerate()
        .map(|(index, test)| {
            let Some(check) = &check else {
                return TestItem {
                    selector: test.selector,
                    outcome: TestOutcome::Invalid,
                    instrumentation: None,
                };
            };
            let (outcome, instrumentation) =
                run_one(check, &test.selector, index, budget);
            TestItem {
                selector: test.selector,
                outcome,
                instrumentation,
            }
        })
        .collect::<Vec<_>>();
    let result = if invalid {
        SuiteResult::Diagnostics
    } else {
        SuiteResult::by_precedence(items.iter().map(TestItem::outcome))
    };
    Ok(TestRun {
        result,
        items,
        diagnostics,
        revision,
    })
}

/// The test modules a run needs, sorted by module path bytes: every module
/// of `@tests`, or the selector's module and the test modules it reaches
/// through `use`.
fn test_modules<'s>(
    snapshot: &'s SourceSnapshot,
    selector: Option<&TestSelector>,
) -> Result<Vec<&'s SourceFile>, InvalidSelection> {
    let mut modules = snapshot
        .units()
        .iter()
        .filter(|unit| unit.kind() == UnitKind::Tests)
        .flat_map(|unit| unit.modules())
        .collect::<Vec<_>>();
    modules.sort_by(|left, right| left.path().as_bytes().cmp(right.path().as_bytes()));
    let Some(selector) = selector else {
        return Ok(modules);
    };
    let Some(selected) = modules
        .iter()
        .copied()
        .find(|module| module.path() == selector.module)
    else {
        return Err(InvalidSelection::Unknown(selector.to_string()));
    };
    let mut reached = BTreeSet::from([selected.path()]);
    let mut pending = vec![selected];
    while let Some(module) = pending.pop() {
        let document = parse_source(Some(module.source_id()), module.bytes());
        let Some(view) = vibra_syntax::module(&document) else {
            continue;
        };
        for declaration in view.declarations() {
            let Declaration::Use(import) = declaration else {
                continue;
            };
            let Some(locator) = import.locator() else {
                continue;
            };
            for &candidate in &modules {
                if names_module(locator.text(), candidate.path())
                    && reached.insert(candidate.path())
                {
                    pending.push(candidate);
                }
            }
        }
    }
    modules.retain(|module| reached.contains(module.path()));
    Ok(modules)
}

/// Whether the `use` locator `locator` names the module `path` or a
/// declaration of it.
fn names_module(locator: &str, path: &str) -> bool {
    locator
        .strip_prefix(path)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}

/// Checks the selected test modules with their import closure: every
/// module of `@tests`, or the selector's module. Only the test modules of
/// `modules` are given to resolution, so a test module the selection does
/// not reach is neither parsed nor checked.
///
/// # Errors
///
/// The embedded standard library is defective.
fn check_selection(
    snapshot: &SourceSnapshot,
    selector: Option<&TestSelector>,
    modules: &[&SourceFile],
) -> Result<CheckResult, Box<Diagnostic>> {
    let units = vec![
        selector
            .map_or_else(|| TESTS_UNIT.to_owned(), |selected| selected.module.clone()),
    ];
    let tests = snapshot
        .units()
        .iter()
        .filter(|unit| unit.kind() == UnitKind::Tests)
        .flat_map(|unit| unit.modules())
        .map(SourceFile::path)
        .collect::<BTreeSet<_>>();
    let needed = modules
        .iter()
        .map(|module| module.path())
        .collect::<BTreeSet<_>>();
    let input = resolve_input_of(snapshot, &|module| {
        !tests.contains(module.path()) || needed.contains(module.path())
    })
    .map_err(|error| Box::new(error.diagnostic().clone()))?;
    Ok(check_workspace(&input, &units))
}

/// Runs the checked test `selector`, the `index`-th selected.
fn run_one(
    check: &CheckResult,
    selector: &TestSelector,
    index: usize,
    budget: MemoryBudget,
) -> (TestOutcome, Option<Instrumentation>) {
    let checked = check
        .tests()
        .iter()
        .find(|test| test.module() == selector.module && test.name() == selector.name);
    let Some(checked) = checked else {
        return (
            TestOutcome::Unavailable(Diagnostic::new(
                DiagnosticCode::ToolUnavailable,
                ByteSpan::empty_at(0),
                format!("the test `{selector}` was not checked"),
            )),
            None,
        );
    };
    if let Some(diagnostic) = checked.unavailable() {
        return (TestOutcome::Unavailable(diagnostic.clone()), None);
    }
    let (Some(function), Some(set)) = (checked.function(), check.test_set()) else {
        return (
            TestOutcome::Unavailable(Diagnostic::new(
                DiagnosticCode::ToolUnavailable,
                ByteSpan::empty_at(0),
                format!("the test `{selector}` was not lowered"),
            )),
            None,
        );
    };
    match execute(set, function, index, budget) {
        Ok(execution) => {
            let outcome = match execution.failure {
                None => TestOutcome::Passed,
                Some(failure) => TestOutcome::AssertionFailed(failure),
            };
            (outcome, Some(execution.instrumentation))
        }
        Err(RuntimeError::MemoryExhausted { .. }) => (TestOutcome::Exhausted, None),
        Err(RuntimeError::Trap { trap, origin, .. }) => {
            (TestOutcome::Trap { trap, origin }, None)
        }
        Err(
            error @ (RuntimeError::UnsupportedValue { .. }
            | RuntimeError::Unavailable { .. }),
        ) => (TestOutcome::Unavailable(error.diagnostic()), None),
    }
}

/// One execution of the test function `function` in a new interpreter.
fn execute(
    set: &Arc<CheckedModuleSet>,
    function: usize,
    index: usize,
    budget: MemoryBudget,
) -> Result<vibra_interp::TestExecution, RuntimeError> {
    if let Some(error) = injected(index) {
        return Err(error);
    }
    let program =
        CheckedProgram::for_entry(Arc::clone(set), function).map_err(|error| {
            RuntimeError::Trap {
                trap: Trap::InvalidCheckedProgram,
                origin: None,
                message: format!("a test is not a valid entry: {error}"),
            }
        })?;
    run_test(&program, budget)
}

#[cfg(test)]
thread_local! {
    /// The fault-injection hook of the trap-isolation test: the first
    /// selected test raises `@runtime.invalidCheckedProgram` before it runs.
    static TRAP_FIRST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The fault the test hook injects into the `index`-th selected test.
#[cfg(test)]
fn injected(index: usize) -> Option<RuntimeError> {
    (index == 0 && TRAP_FIRST.get()).then(|| RuntimeError::Trap {
        trap: Trap::InvalidCheckedProgram,
        origin: None,
        message: "an injected fault".to_owned(),
    })
}

#[cfg(not(test))]
const fn injected(_index: usize) -> Option<RuntimeError> {
    None
}

#[cfg(test)]
mod tests;
