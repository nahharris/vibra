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
fn a_selector_parses_its_module_and_the_test_modules_it_imports() {
    let helpers = "fn three\n  returns r: I32\n  visibility @public\n  do\n    return 3i32\nendfn\n";
    let uses = "use @std.assert enduse\n\nuse @tests.helpers enduse\n\n\
                test \"uses\" do assert.equal(3i32 helpers.three()) endtest\n";
    let broken = "test \"x\" do endfn";
    let files = [
        ("tests/helpers.vib", helpers),
        ("tests/uses.vib", uses),
        ("tests/broken.vib", broken),
    ];
    let run = run(&files, Some("@tests.uses::\"uses\"")).expect("a run");
    assert_eq!(names(&run), ["@tests.uses::\"uses\""]);
    assert_eq!(run.result(), SuiteResult::Ok);
    assert!(run.diagnostics().is_empty(), "{:?}", run.diagnostics());
    // An imported test module is required: its parse errors come first.
    let files = [("tests/helpers.vib", broken), ("tests/uses.vib", uses)];
    let run = self::run(&files, Some("@tests.uses::\"uses\"")).expect("a run");
    assert_eq!(run.result(), SuiteResult::Diagnostics);
    assert!(run.items().is_empty());
    assert!(
        run.diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.source_id() == Some("tests/helpers.vib"))
    );
}

/// ATK-test-runner-no-system-driver: the runner builds each test's world
/// from the fake world's driver only, so its source names no real driver.
#[test]
fn atk_test_runner_no_system_driver() {
    let source = include_str!("../../src/test_runner.rs");
    for forbidden in ["SystemDriver", "Launch::System", "vibra_host::system"] {
        assert!(
            !source.contains(forbidden),
            "the test runner names `{forbidden}`"
        );
    }
    assert!(
        source.contains("WorldDriver::new"),
        "the runner builds a fake world"
    );
}
