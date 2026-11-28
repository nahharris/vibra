//! Unit tests of the runner that need its fault-injection hook.

use vibra_diagnostics::ByteSpan;
use vibra_interp::MemoryBudget;
use vibra_ir::{SourceOrigin, Trap};

use super::{SuiteResult, TRAP_FIRST, TestOutcome, outcome, run_tests};
use crate::snapshot::SourceSnapshot;
use crate::test_tree::Tree;

const PROJECT: &str = "{format: @project.v2 package: {name: \"hello\" version: \"0.1.0\"} \
                       targets: [{name: @app kind: @lib root: \"src/app\"}] dependencies: [:]}";

const TESTS: &str = "use @std.assert enduse\n\n\
                     test \"first\" do assert.true(true) endtest\n\n\
                     test \"second\" do assert.true(true) endtest\n";

/// The snapshot of the project of `PROJECT` with the tests `TESTS`, and
/// the tree that holds it.
fn snapshot() -> Result<(Tree, SourceSnapshot), String> {
    let tree = Tree::new(&[
        ("project.vibon", PROJECT),
        ("src/app/main.vib", ""),
        ("tests/math.vib", TESTS),
    ])?;
    let snapshot = tree.snapshot()?;
    Ok((tree, snapshot))
}

#[test]
fn the_suite_result_follows_the_precedence_over_all_five_outcomes() {
    let trap = TestOutcome::Trap {
        trap: Trap::InvalidCheckedProgram,
        origin: None,
    };
    let exhausted = TestOutcome::Exhausted;
    let unavailable = TestOutcome::Unavailable(vibra_diagnostics::Diagnostic::new(
        vibra_diagnostics::DiagnosticCode::ToolUnavailable,
        vibra_diagnostics::ByteSpan::empty_at(0),
        "unsupported",
    ));
    let failed = TestOutcome::AssertionFailed(vibra_interp::TestAssertionFailure {
        kind: vibra_interp::FailureKind::Assertion,
        assertion: "assert.true".to_owned(),
        expected: None,
        actual: None,
        origin: vibra_ir::SourceOrigin::new(
            "tests/math.vib",
            vibra_diagnostics::ByteSpan::new(0, 1),
        ),
    });
    let passed = TestOutcome::Passed;
    let table: [(&[&TestOutcome], SuiteResult); 9] = [
        (&[], SuiteResult::Ok),
        (&[&passed, &passed], SuiteResult::Ok),
        (&[&passed, &failed], SuiteResult::TestFailed),
        (&[&failed, &unavailable, &passed], SuiteResult::Unavailable),
        (
            &[&unavailable, &exhausted, &failed],
            SuiteResult::OperationalFailure,
        ),
        (&[&exhausted, &trap, &unavailable], SuiteResult::Trap),
        (&[&failed, &passed, &trap], SuiteResult::Trap),
        (&[&exhausted], SuiteResult::OperationalFailure),
        (&[&unavailable], SuiteResult::Unavailable),
    ];
    for (outcomes, expected) in table {
        assert_eq!(
            SuiteResult::by_precedence(outcomes.iter().copied()),
            expected,
            "{outcomes:?}"
        );
    }
}

#[test]
fn a_trap_ends_only_its_own_test() -> Result<(), String> {
    let (_tree, snapshot) = snapshot()?;
    TRAP_FIRST.set(true);
    let run = run_tests(&snapshot, None, MemoryBudget::DEFAULT);
    TRAP_FIRST.set(false);
    let run = run.map_err(|error| error.to_string())?;
    let outcomes = run
        .items()
        .iter()
        .map(|item| item.outcome().clone())
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        [
            TestOutcome::Trap {
                trap: Trap::InvalidCheckedProgram,
                origin: None,
            },
            TestOutcome::Passed,
        ]
    );
    assert_eq!(run.result(), SuiteResult::Trap);
    let document = run.document();
    assert!(
        document.contains(
            "result: @test.trap trap: {trapCode: @runtime.invalidCheckedProgram} auditTrace:"
        ),
        "{document}"
    );
    assert!(
        document.starts_with("{format: @testRun.v2 result: @command.trap tests: [")
    );
    Ok(())
}

/// 06 Origins and 07 `@testRun.v2`: a trap record holds the span record of
/// the trapping operation when one is known, and omits `origin` otherwise.
#[test]
fn a_trap_record_writes_its_origin_when_known() -> Result<(), String> {
    let (_tree, snapshot) = snapshot()?;
    TRAP_FIRST.set(true);
    let run = run_tests(&snapshot, None, MemoryBudget::DEFAULT);
    TRAP_FIRST.set(false);
    let mut run = run.map_err(|error| error.to_string())?;
    // `assert.true(true)` of the first test: line 3 of `TESTS` starts at
    // byte 24 (`use @std.assert enduse` is 22 bytes, then two line feeds),
    // and `test "first" do ` is 16 bytes, so the call spans 40..57.
    let first = run.items.first_mut().ok_or("no selected test")?;
    first.outcome = TestOutcome::Trap {
        trap: Trap::InvalidCheckedProgram,
        origin: Some(SourceOrigin::new("tests/math.vib", ByteSpan::new(40, 57))),
    };
    let document = run.document();
    assert!(
        document.contains(
            "result: @test.trap trap: {trapCode: @runtime.invalidCheckedProgram \
             origin: {sourceId: \"tests/math.vib\" start: 40u64 end: 57u64}} auditTrace:"
        ),
        "{document}"
    );
    Ok(())
}

/// Row "a test that reaches a host body": the interpreter answers a
/// host-bound member entered with no host context with `@tool.unavailable`
/// (`vibra-interp`'s `a_host_body_without_a_context_is_unavailable`), and
/// the runner, which passes none, makes that test `@test.unavailable` with
/// the diagnostic, not a trap or a failure.
#[test]
fn a_test_that_reaches_a_host_body_is_unavailable() -> Result<(), String> {
    let (ended, instrumentation) =
        outcome(Err(vibra_interp::RuntimeError::Unavailable {
            message: "the host-bound member `testWorld.random` needs a host context"
                .to_owned(),
        }));
    assert_eq!(ended.result(), "@test.unavailable");
    let TestOutcome::Unavailable(diagnostic) = ended else {
        return Err(format!("not unavailable: {ended:?}"));
    };
    assert_eq!(
        diagnostic.code(),
        vibra_diagnostics::DiagnosticCode::ToolUnavailable
    );
    assert!(diagnostic.message().contains("testWorld.random"));
    assert_eq!(instrumentation, None);
    Ok(())
}
