//! The test runner: selectors, discovery order, the check before any test
//! runs, a fresh interpreter per test, isolation of an exhausted test, and
//! the `@testRun.v2` observation. Expectations come from the programs
//! chapter, "Tests" and "Assertions", and the tooling chapter, "Tests and
//! selection".

use vibra_interp::MemoryBudget;
use vibra_workspace::{
    InvalidSelection, SuiteResult, TestOutcome, TestRun, TestSelector, run_tests,
};

use crate::support::{LIBRARY, TempDir, capture};

const MATH: &str = "fn add\n  takes {left: I32 right: I32}\n  returns sum: I32\n  \
                    visibility @public\n  do\n    return I32.add(left right)\nendfn\n";

/// Runs the tests of a library project holding `src/app/math.vib` and
/// `files`.
fn run(
    files: &[(&str, &str)],
    selector: Option<&str>,
) -> Result<TestRun, InvalidSelection> {
    run_with(files, selector, MemoryBudget::DEFAULT)
}

fn run_with(
    files: &[(&str, &str)],
    selector: Option<&str>,
    budget: MemoryBudget,
) -> Result<TestRun, InvalidSelection> {
    let mut all = vec![("src/app/math.vib", MATH)];
    all.extend_from_slice(files);
    let tree = TempDir::project("tests", LIBRARY, &all);
    let snapshot = capture(tree.path()).expect("the project captures");
    let selector = selector.map(|text| TestSelector::parse(text).expect("a selector"));
    run_tests(&snapshot, selector.as_ref(), budget)
}

fn names(run: &TestRun) -> Vec<String> {
    run.items()
        .iter()
        .map(|item| item.selector().to_string())
        .collect()
}

#[test]
fn a_selector_is_a_tests_module_and_a_canonical_string() {
    let selector =
        TestSelector::parse("@tests.math::\"adds one\"").expect("a selector");
    assert_eq!(selector.module(), "@tests.math");
    assert_eq!(selector.name(), "adds one");
    assert_eq!(selector.to_string(), "@tests.math::\"adds one\"");
    for malformed in [
        "@app.math::\"adds\"",
        "@tests::\"adds\"",
        "@tests.math:: \"adds\"",
        "@tests.math::adds",
        "@tests.math::\"\\u{61}dds\"",
        "@tests.math",
        "@tests.Math-x::\"adds\"",
    ] {
        assert_eq!(
            TestSelector::parse(malformed),
            Err(InvalidSelection::Malformed(malformed.to_owned())),
            "{malformed}"
        );
    }
}

#[test]
fn tests_run_in_module_order_then_declaration_order() {
    let b = "use @std.assert enduse\n\ntest \"second\" do assert.true(true) endtest\n\n\
             test \"first\" do assert.true(true) endtest\n";
    let a = "use @std.assert enduse\n\ntest \"z\" do assert.true(true) endtest\n\n\
             test \"y\" do assert.true(true) endtest\n";
    let run = run(&[("tests/b.vib", b), ("tests/a.vib", a)], None).expect("a run");
    assert_eq!(
        names(&run),
        [
            "@tests.a::\"z\"",
            "@tests.a::\"y\"",
            "@tests.b::\"second\"",
            "@tests.b::\"first\"",
        ]
    );
    assert_eq!(run.result(), SuiteResult::Ok);
    assert_eq!((run.selected(), run.passed(), run.failed()), (4, 4, 0));
}

#[test]
fn every_test_initializes_the_module_values_it_reads_afresh() {
    let tests = "use @std.assert enduse\n\nlet base: I32 be I32.add(1i32 2i32) endlet\n\n\
                 test \"once\" do assert.equal(3i32 base) endtest\n\n\
                 test \"again\" do assert.equal(3i32 base) endtest\n";
    let run = run(&[("tests/state.vib", tests)], None).expect("a run");
    assert_eq!(run.result(), SuiteResult::Ok);
    let initializations = run
        .items()
        .iter()
        .map(|item| {
            assert_eq!(item.outcome(), &TestOutcome::Passed);
            item.instrumentation()
                .map(|measured| measured.module_value_initializations)
        })
        .collect::<Vec<_>>();
    assert_eq!(initializations, [Some(1), Some(1)]);
}

#[test]
fn a_selector_selects_one_test() {
    let tests = "use @std.assert enduse\n\nuse @app.math enduse\n\n\
                 test \"adds\" do assert.equal(3i32 math.add(1i32 2i32)) endtest\n\n\
                 test \"other\" do assert.true(false) endtest\n";
    let files = [("tests/math.vib", tests)];
    let run = run(&files, Some("@tests.math::\"adds\"")).expect("a run");
    assert_eq!(names(&run), ["@tests.math::\"adds\""]);
    assert_eq!(run.result(), SuiteResult::Ok);
    for unknown in ["@tests.math::\"missing\"", "@tests.other::\"adds\""] {
        assert_eq!(
            self::run(&files, Some(unknown)),
            Err(InvalidSelection::Unknown(unknown.to_owned()))
        );
    }
}

#[test]
fn an_exhausted_test_ends_only_itself() {
    let tests = "use @std.assert enduse\n\n\
                 fn grow\n  takes {items: List(I32)}\n  returns count: U64\n  do\n    \
                 return grow(List.concat(items items))\nendfn\n\n\
                 test \"a exhausts\" do assert.equal(0u64 grow([1i32])) endtest\n\n\
                 test \"b passes\" do assert.true(true) endtest\n";
    let run = run_with(
        &[("tests/memory.vib", tests)],
        None,
        MemoryBudget::new(4 * 1024 * 1024),
    )
    .expect("a run");
    let outcomes = run
        .items()
        .iter()
        .map(|item| item.outcome().clone())
        .collect::<Vec<_>>();
    assert_eq!(outcomes, [TestOutcome::Exhausted, TestOutcome::Passed]);
    assert_eq!(run.result(), SuiteResult::OperationalFailure);
    assert_eq!((run.selected(), run.passed(), run.failed()), (2, 1, 1));
    assert_eq!(run.document(), None);
}

#[test]
fn a_static_error_invalidates_the_whole_selection() {
    let tests = "use @std.assert enduse\n\n\
                 test \"bad\" do assert.true(1i32) endtest\n\n\
                 test \"good\" do assert.true(true) endtest\n";
    let run = run(&[("tests/math.vib", tests)], None).expect("a run");
    assert_eq!(run.result(), SuiteResult::Diagnostics);
    assert!(
        run.items()
            .iter()
            .all(|item| item.outcome() == &TestOutcome::Invalid)
    );
    assert_eq!(run.items().len(), 2);
    assert!(!run.diagnostics().is_empty());
}

#[test]
fn a_parse_error_reports_no_items() {
    let run = run(&[("tests/math.vib", "test \"broken\" do\n")], None).expect("a run");
    assert_eq!(run.result(), SuiteResult::Diagnostics);
    assert!(run.items().is_empty());
    assert!(!run.diagnostics().is_empty());
}

#[test]
fn a_project_without_tests_is_an_empty_passing_suite() {
    let run = run(&[], None).expect("a run");
    assert_eq!(run.result(), SuiteResult::Ok);
    assert!(run.items().is_empty());
    assert_eq!(
        run.document().as_deref(),
        Some("{format: @testRun.v2 result: @command.ok tests: []}")
    );
}
