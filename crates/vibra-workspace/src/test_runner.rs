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
//! others run. Each test runs in a new interpreter over a fresh copy of the
//! host world, a case's `@hostWorld.v2` or the default one, with a new
//! broker context, so module values start fresh and the audit trace is
//! empty. A test that takes its world receives the context's test root and
//! derives every grant from it; one without `takes` receives nothing. The
//! world is the fake world's driver only, so no test reaches a real host
//! resource. Each item carries its test's `@auditTrace.v2`, with no
//! `target`.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level};
use vibra_host::broker::Context;
use vibra_host::driver::DriverIdentity;
use vibra_host::trace::{self, AuditTrace, StdlibIdentity, TraceHeader};
use vibra_host::world::{DRIVER_VERSION, HostWorld, WORLD_FORMAT, WorldDriver};
use vibra_interp::{
    FailureKind, Instrumentation, MemoryBudget, RuntimeError, TestAssertionFailure,
    run_test,
};
use vibra_ir::{
    CheckedModuleSet, CheckedProgram, SourceOrigin, Trap, Type, encode_type,
};
use vibra_syntax::source::Declaration;
use vibra_syntax::{canonical_string, decode_string, parse_source};
use vibra_types::{CheckResult, check_workspace, load_stdlib};

use crate::resolve::resolve_input_of;
use crate::revision::workspace_revision;
use crate::snapshot::{SourceFile, SourceSnapshot, UnitKind};

/// The format atom of the observation.
pub const TEST_RUN_FORMAT: &str = "@testRun.v2";

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
    /// `@test.invalid`: the selection had static errors (`None`), or the
    /// test's `TestWorld.directory` fixture conflicts, the
    /// `@test.conflictingFixture` at the `await` of the call.
    Invalid(Option<Diagnostic>),
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
            Self::Invalid(_) => "@test.invalid",
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
    trace: AuditTrace,
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

    /// The test's audit trace: every event its body's host calls
    /// committed, in completion order; no event for a test that did not
    /// run.
    #[must_use]
    pub const fn trace(&self) -> &AuditTrace {
        &self.trace
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
    /// ok. A test whose fixture conflicts failed.
    #[must_use]
    pub fn by_precedence<'a>(
        outcomes: impl IntoIterator<Item = &'a TestOutcome>,
    ) -> Self {
        let rank = |outcome: &TestOutcome| match outcome {
            TestOutcome::Trap { .. } => 4,
            TestOutcome::Exhausted => 3,
            TestOutcome::Unavailable(_) => 2,
            TestOutcome::AssertionFailed(_) | TestOutcome::Invalid(Some(_)) => 1,
            TestOutcome::Passed | TestOutcome::Invalid(None) => 0,
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
        fields.push(format!("auditTrace: {}", trace::encode(&item.trace)));
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
/// selector, each under `budget` in a fresh copy of the default host world.
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
    run_tests_in(snapshot, selector, &HostWorld::default(), budget)
}

/// [`run_tests`], each test in a fresh copy of `world`.
///
/// # Errors
///
/// As [`run_tests`].
pub fn run_tests_in(
    snapshot: &SourceSnapshot,
    selector: Option<&TestSelector>,
    world: &HostWorld,
    budget: MemoryBudget,
) -> Result<TestRun, InvalidSelection> {
    let revision = workspace_revision(&snapshot.revision_records()).unwrap_or_default();
    if !snapshot.limit_faults().is_empty() {
        // A module nested past the source nesting limit stops the run before
        // any test module is read, as a parse error would.
        return Ok(TestRun {
            result: SuiteResult::Diagnostics,
            items: Vec::new(),
            diagnostics: snapshot.limit_faults().to_vec(),
            revision,
        });
    }
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
    let library = match load_stdlib() {
        Ok(stdlib) => StdlibIdentity {
            package: stdlib.package_name().to_owned(),
            version: stdlib.package_version().to_owned(),
        },
        Err(error) => {
            return Ok(TestRun {
                result: SuiteResult::Diagnostics,
                items: Vec::new(),
                diagnostics: vec![error.diagnostic().clone()],
                revision,
            });
        }
    };
    let header = TraceHeader {
        revision: revision.clone(),
        target: None,
        library,
    };
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
                    outcome: TestOutcome::Invalid(None),
                    instrumentation: None,
                    trace: world_trace(&header),
                };
            };
            let (outcome, instrumentation, trace) = run_one(
                check,
                &test.selector,
                index,
                &World {
                    world,
                    header: &header,
                },
                budget,
            );
            TestItem {
                selector: test.selector,
                outcome,
                instrumentation,
                trace,
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

/// What every test of a run starts from: the world it copies and the
/// header of its trace.
struct World<'w> {
    world: &'w HostWorld,
    header: &'w TraceHeader,
}

/// The identity of the fake world's driver.
fn world_driver() -> DriverIdentity {
    DriverIdentity {
        provider: WORLD_FORMAT.to_owned(),
        version: DRIVER_VERSION.to_owned(),
    }
}

/// The trace of a test with no event: the world's driver is bound to every
/// test, run or not.
fn world_trace(header: &TraceHeader) -> AuditTrace {
    let mut trace = AuditTrace::new(header.clone());
    trace.bind_driver(world_driver());
    trace
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

/// Runs the checked test `selector`, the `index`-th selected, in a fresh
/// copy of the run's world.
fn run_one(
    check: &CheckResult,
    selector: &TestSelector,
    index: usize,
    world: &World<'_>,
    budget: MemoryBudget,
) -> (TestOutcome, Option<Instrumentation>, AuditTrace) {
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
            world_trace(world.header),
        );
    };
    if let Some(diagnostic) = checked.unavailable() {
        return (
            TestOutcome::Unavailable(diagnostic.clone()),
            None,
            world_trace(world.header),
        );
    }
    let (Some(function), Some(set)) = (checked.function(), check.test_set()) else {
        return (
            TestOutcome::Unavailable(Diagnostic::new(
                DiagnosticCode::ToolUnavailable,
                ByteSpan::empty_at(0),
                format!("the test `{selector}` was not lowered"),
            )),
            None,
            world_trace(world.header),
        );
    };
    let (ended, trace) = execute(set, function, index, world, budget);
    let (outcome, instrumentation) = outcome(ended);
    (outcome, instrumentation, trace)
}

/// How a test whose execution ended as `ended` ends. A form the
/// interpreter does not run makes the test `@test.unavailable`.
fn outcome(
    ended: Result<vibra_interp::TestExecution, RuntimeError>,
) -> (TestOutcome, Option<Instrumentation>) {
    match ended {
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
        Err(error @ RuntimeError::ConflictingFixture { .. }) => {
            (TestOutcome::Invalid(Some(error.diagnostic())), None)
        }
        Err(
            error @ (RuntimeError::UnsupportedValue { .. }
            | RuntimeError::Unavailable { .. }),
        ) => (TestOutcome::Unavailable(error.diagnostic()), None),
    }
}

/// Whether the test function `function` takes its world: its one
/// parameter is the `TestWorld`.
fn takes_world(set: &CheckedModuleSet, function: usize) -> bool {
    set.functions().get(function).is_some_and(|function| {
        matches!(
            function.signature.type_info(),
            Type::Fn(signature) if !signature.parameters.is_empty()
        )
    })
}

/// One execution of the test function `function` in a new interpreter,
/// over a new `WorldDriver` copying the run's world and a new broker
/// context, and the test's trace.
///
/// The body runs in the procedure context its checked function states. A
/// test that takes its world receives the context's test root; every grant
/// it uses descends from it.
fn execute(
    set: &Arc<CheckedModuleSet>,
    function: usize,
    index: usize,
    world: &World<'_>,
    budget: MemoryBudget,
) -> (
    Result<vibra_interp::TestExecution, RuntimeError>,
    AuditTrace,
) {
    if let Some(error) = injected(index) {
        return (Err(error), world_trace(world.header));
    }
    let program = match CheckedProgram::for_entry(Arc::clone(set), function) {
        Ok(program) => program,
        Err(error) => {
            return (
                Err(RuntimeError::Trap {
                    trap: Trap::InvalidCheckedProgram,
                    origin: None,
                    message: format!("a test is not a valid entry: {error}"),
                }),
                world_trace(world.header),
            );
        }
    };
    let mut driver = WorldDriver::new(world.world);
    let mut context = Context::for_world(&mut driver, world.header.clone());
    let root = if takes_world(set, function) {
        match context.test_root() {
            Ok(root) => Some(root),
            Err(fault) => {
                return (
                    Err(RuntimeError::Trap {
                        trap: Trap::InvalidHostValue,
                        origin: None,
                        message: format!("the test root was not issued: {fault:?}"),
                    }),
                    world_trace(world.header),
                );
            }
        }
    } else {
        None
    };
    let ended = {
        #[cfg(test)]
        let mut faulting = isolation::Faulting::new(&mut context, index);
        #[cfg(test)]
        let host: &mut dyn vibra_ir::authority::HostContext = &mut faulting;
        #[cfg(not(test))]
        let host: &mut dyn vibra_ir::authority::HostContext = &mut context;
        run_test(&program, root, Some(host), budget)
    };
    let mut trace = context.trace().clone();
    trace.bind_driver(world_driver());
    (ended, trace)
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

/// The fault-injection seam of the isolation tests, and the tests that need
/// the broker behind an interpreter entry.
#[cfg(test)]
mod isolation {
    use std::cell::Cell;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;

    use vibra_host::broker::Context;
    use vibra_host::trace::{
        AuditTrace, Event, HostEvent, StdlibIdentity, TraceHeader,
    };
    use vibra_host::world::{HostWorld, WorldDriver};
    use vibra_interp::{EntryArgument, MemoryBudget, RuntimeError, run_entry};
    use vibra_ir::authority::{
        GrantToken, HostCompletion, HostContext, HostFault, HostRequest, Issued,
        NarrowRequest, Narrowed, Receipt,
    };
    use vibra_ir::external::host::{HostOp, TestWorldOp};
    use vibra_ir::{CheckedProgram, Trap, Value};
    use vibra_types::load_stdlib;

    use super::{TestOutcome, check_selection, run_tests, test_modules};
    use crate::discovery::discover_project_at;
    use crate::snapshot::SourceSnapshot;

    thread_local! {
        /// The selected test, by index, and its host call, counted from one,
        /// that the seam answers with a driver fault instead of the broker.
        static FAULT_AT: Cell<Option<(usize, u64)>> = const { Cell::new(None) };
    }

    /// The host context of one test: its broker context, with the host
    /// call [`FAULT_AT`] names, if any, faulting.
    pub(super) struct Faulting<'c, 'd> {
        context: &'c mut Context<'d>,
        fault_at: Option<u64>,
        calls: u64,
    }

    impl<'c, 'd> Faulting<'c, 'd> {
        /// The seam over `context` for the `index`-th selected test.
        pub(super) fn new(context: &'c mut Context<'d>, index: usize) -> Self {
            let fault_at = FAULT_AT
                .get()
                .filter(|(test, _)| *test == index)
                .map(|(_, call)| call);
            Self {
                context,
                fault_at,
                calls: 0,
            }
        }
    }

    impl HostContext for Faulting<'_, '_> {
        fn host(
            &mut self,
            request: HostRequest<'_>,
        ) -> Result<HostCompletion, HostFault> {
            self.calls += 1;
            if self.fault_at == Some(self.calls) {
                return Err(HostFault::InvalidHostValue(
                    "an injected driver fault".to_owned(),
                ));
            }
            self.context.host(request)
        }

        fn commit(&mut self, receipt: Receipt) -> Result<(), HostFault> {
            self.context.commit(receipt)
        }

        fn narrow(
            &mut self,
            request: NarrowRequest<'_>,
        ) -> Result<Narrowed, HostFault> {
            self.context.narrow(request)
        }

        fn test_world(
            &mut self,
            op: TestWorldOp,
            world: GrantToken,
            arguments: &[Value],
            invocation: u64,
        ) -> Result<Issued, HostFault> {
            self.context.test_world(op, world, arguments, invocation)
        }
    }

    const PROJECT: &str = "{format: @project.v2 package: {name: \"hello\" version: \
                           \"0.1.0\"} targets: [{name: @app kind: @lib root: \
                           \"src/app\"}] dependencies: [:]}";

    const MAIN: &str = "use @std.io.StdoutText enduse\n\n\
                        use @std.io.IoError enduse\n\n\
                        proc emit\n  takes {grant: StdoutText}\n  \
                        returns outcome: Result(Void IoError)\n  \
                        performs {StdoutText}\n  visibility @public\n  do\n    \
                        return await StdoutText.write(grant \"x\")\nendproc\n";

    const TESTS: &str = "use @std.assert enduse\n\n\
                         use @std.test.TestWorld enduse\n\n\
                         use @std.io.StdoutText enduse\n\n\
                         test \"traps\"\n  takes {world: TestWorld}\n  \
                         performs {TestWorld StdoutText}\n  do\n    \
                         let {stdoutText: out} be await TestWorld.streams(world) endlet\n    \
                         let _ be await StdoutText.write(out \"a\") endlet\n    \
                         let _ be await StdoutText.write(out \"b\") endlet\n\
                         endtest\n\n\
                         test \"writes\"\n  takes {world: TestWorld}\n  \
                         performs {TestWorld StdoutText}\n  do\n    \
                         let {stdoutText: out} be await TestWorld.streams(world) endlet\n    \
                         let _ be await StdoutText.write(out \"c\") endlet\n\
                         endtest\n";

    /// The tests of the entry tree: one test reaches `emit`, so that it is
    /// lowered.
    const ENTRY_TESTS: &str = "use @std.assert enduse\n\n\
                               use @std.test.TestWorld enduse\n\n\
                               use @std.io.StdoutText enduse\n\n\
                               use @app.main enduse\n\n\
                               test \"emits\"\n  takes {world: TestWorld}\n  \
                               performs {TestWorld StdoutText}\n  do\n    \
                               let {stdoutText: out} be await TestWorld.streams(world) endlet\n    \
                               let _ be await main.emit(out) endlet\n\
                               endtest\n";

    /// A project tree in its own temporary directory, removed on drop.
    struct Tree(PathBuf);

    impl Tree {
        fn new(prefix: &str, tests: &str) -> Result<Self, String> {
            let root =
                std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            let write = |relative: &str, text: &str| {
                let path = root.join(relative);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                fs::write(path, text).map_err(|error| error.to_string())
            };
            write("project.vibon", PROJECT)?;
            write("src/app/main.vib", MAIN)?;
            write("tests/run.vib", tests)?;
            Ok(Self(root))
        }

        fn snapshot(&self) -> Result<SourceSnapshot, String> {
            let root = fs::canonicalize(&self.0).map_err(|error| error.to_string())?;
            let project =
                discover_project_at(&root).map_err(|faults| format!("{faults:?}"))?;
            SourceSnapshot::capture(project).map_err(|faults| format!("{faults:?}"))
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// The host events of `trace`.
    fn host_events(trace: &AuditTrace) -> Vec<&HostEvent> {
        trace
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Host(event) => Some(event),
                _ => None,
            })
            .collect()
    }

    /// `(invocation, parent, root)` of each host event of `trace`.
    fn ordinals(trace: &AuditTrace) -> Vec<(u64, Option<u64>, u64)> {
        host_events(trace)
            .iter()
            .map(|event| (event.invocation, event.parent, event.grant.root))
            .collect()
    }

    /// F7 of the review of grants: a defect trap ends only its own test,
    /// after host calls, and the next test runs in a fresh world. The first
    /// test writes `a` to its standard output and then meets a driver fault
    /// on its second write: `@runtime.invalidHostValue` (06 "Driver
    /// faults"), and its trace keeps the one completed write, invocation 3
    /// (the test is 1, `TestWorld.streams` is 2), parent 1, through
    /// `stdoutText`, the sixth root `streams` issues. The second test starts
    /// over: its write is again invocation 3, parent 1, root 6, and it
    /// passes (04 "Test worlds and grants": each test has its own world).
    #[test]
    fn a_defect_trap_after_host_calls_ends_only_its_own_test() -> Result<(), String> {
        let tree = Tree::new("vibra-runner-defect-isolation", TESTS)?;
        let snapshot = tree.snapshot()?;
        let control = run_tests(&snapshot, None, MemoryBudget::DEFAULT)
            .map_err(|error| error.to_string())?;
        FAULT_AT.set(Some((0, 2)));
        let run = run_tests(&snapshot, None, MemoryBudget::DEFAULT);
        FAULT_AT.set(None);
        let run = run.map_err(|error| error.to_string())?;
        let [first, second] = run.items() else {
            return Err(format!("two tests: {}", run.document()));
        };
        assert!(
            matches!(
                first.outcome(),
                TestOutcome::Trap {
                    trap: Trap::InvalidHostValue,
                    ..
                }
            ),
            "{:?}",
            first.outcome()
        );
        assert_eq!(ordinals(first.trace()), [(3, Some(1), 6)]);
        assert_eq!(
            host_events(first.trace())
                .iter()
                .map(|event| event.op)
                .collect::<Vec<_>>(),
            [HostOp::IoStdoutText]
        );
        assert_eq!(second.outcome(), &TestOutcome::Passed);
        assert_eq!(ordinals(second.trace()), [(3, Some(1), 6)]);
        // Without the fault, the first test makes both writes, and the
        // second test's trace is the same as with it.
        let [control_first, control_second] = control.items() else {
            return Err(format!("two tests: {}", control.document()));
        };
        assert_eq!(control_first.outcome(), &TestOutcome::Passed);
        assert_eq!(
            ordinals(control_first.trace()),
            [(3, Some(1), 6), (4, Some(1), 6)]
        );
        assert_eq!(control_second.trace(), second.trace());
        Ok(())
    }

    /// The header of a trace of the default world.
    fn header() -> Result<TraceHeader, String> {
        let stdlib = load_stdlib().map_err(|error| format!("{error:?}"))?;
        Ok(TraceHeader {
            revision: String::new(),
            target: None,
            library: StdlibIdentity {
                package: stdlib.package_name().to_owned(),
                version: stdlib.package_version().to_owned(),
            },
        })
    }

    /// The test root of `context`, issued once, and the `stdoutText` grant
    /// of the streams it issues from that root.
    fn stdout_of(
        context: &mut Context<'_>,
    ) -> Result<(GrantToken, GrantToken), String> {
        let root = context.test_root().map_err(|fault| format!("{fault:?}"))?;
        match context.test_world(
            TestWorldOp::Streams,
            root,
            &[Value::Str(String::new())],
            1,
        ) {
            Ok(Issued::Bundle(fields)) => fields
                .iter()
                .find(|(name, _)| *name == "stdoutText")
                .map(|(_, token)| (root, *token))
                .ok_or_else(|| format!("no stdoutText in {fields:?}")),
            other => Err(format!("streams issued {other:?}")),
        }
    }

    /// The bytes the world's standard output holds.
    fn stdout_bytes(
        context: &mut Context<'_>,
        root: GrantToken,
    ) -> Result<Vec<u8>, String> {
        match context.test_world(TestWorldOp::Output, root, &[], 9) {
            Ok(Issued::Output { stdout, .. }) => Ok(stdout),
            other => Err(format!("output issued {other:?}")),
        }
    }

    /// F7 of the review of grants: `run_entry` with a token its context
    /// never issued, and with a token another context issued. The entry
    /// `emit(grant: StdoutText)` writes `x` through its grant; the broker
    /// validates the token at use (06 "Host calls", step 1), so each run is
    /// the defect trap `@runtime.invalidHostValue`, with no event and
    /// nothing written. The same entry with the context's own `stdoutText`
    /// writes `x` and appends one event.
    #[test]
    fn run_entry_with_a_token_the_context_never_issued() -> Result<(), String> {
        let tree = Tree::new("vibra-runner-entry-tokens", ENTRY_TESTS)?;
        let snapshot = tree.snapshot()?;
        let modules =
            test_modules(&snapshot, None).map_err(|error| error.to_string())?;
        let check = check_selection(&snapshot, None, &modules)
            .map_err(|error| format!("{error:?}"))?;
        let set = check
            .module_set()
            .ok_or_else(|| format!("checked: {:?}", check.diagnostics()))?;
        let emit = check
            .functions()
            .iter()
            .find(|(name, _)| name.ends_with("emit"))
            .map(|(_, index)| *index)
            .ok_or_else(|| format!("no emit in {:?}", check.functions()))?;
        let program = CheckedProgram::for_entry(Arc::clone(set), emit)
            .map_err(|error| error.to_string())?;
        let world = HostWorld::default();
        let header = header()?;
        // `token` makes the grant the entry receives from the run's own
        // `stdoutText`: every run has a new context.
        let run = |token: &dyn Fn(GrantToken) -> GrantToken| -> Result<_, String> {
            let mut driver = WorldDriver::new(&world);
            let mut context = Context::for_world(&mut driver, header.clone());
            let (root, own) = stdout_of(&mut context)?;
            let given = token(own);
            let ended = run_entry(
                &program,
                vec![EntryArgument::Grant(given)],
                2,
                &mut context,
                MemoryBudget::DEFAULT,
            );
            let events = host_events(context.trace()).len();
            let written = stdout_bytes(&mut context, root)?;
            Ok((ended, events, written, own, given))
        };
        let (ended, events, written, _, _) = run(&|own| own)?;
        assert!(ended.is_ok(), "{ended:?}");
        assert_eq!((events, written.as_slice()), (1, b"x".as_slice()));
        let mut other_driver = WorldDriver::new(&world);
        let mut other = Context::for_world(&mut other_driver, header.clone());
        let (_, foreign) = stdout_of(&mut other)?;
        let unissued =
            |own: GrantToken| GrantToken::new(own.context(), own.slot() + 1_000, 0);
        let tokens: [(&str, &dyn Fn(GrantToken) -> GrantToken); 2] =
            [("unissued", &unissued), ("foreign", &|_| foreign)];
        for (what, token) in tokens {
            let (ended, events, written, own, given) = run(token)?;
            assert_eq!(
                given.context() == own.context(),
                what == "unissued",
                "{what}: the token's context"
            );
            assert!(
                matches!(
                    ended,
                    Err(RuntimeError::Trap {
                        trap: Trap::InvalidHostValue,
                        ..
                    })
                ),
                "{what}: {ended:?}"
            );
            assert_eq!((events, written.len()), (0, 0), "{what}");
        }
        Ok(())
    }
}
