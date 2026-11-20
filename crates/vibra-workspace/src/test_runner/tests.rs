//! Unit tests of the runner that need its fault-injection hook.

use std::fs;
use std::path::PathBuf;

use vibra_interp::MemoryBudget;
use vibra_ir::Trap;

use super::{SuiteResult, TRAP_FIRST, TestOutcome, run_tests};
use crate::discovery::discover_project_at;
use crate::snapshot::SourceSnapshot;

const PROJECT: &str = "{format: @project.v2 package: {name: \"hello\" version: \"0.1.0\"} \
                       targets: [{name: @app kind: @lib root: \"src/app\"}] dependencies: [:]}";

const TESTS: &str = "use @std.assert enduse\n\n\
                     test \"first\" do assert.true(true) endtest\n\n\
                     test \"second\" do assert.true(true) endtest\n";

/// A project tree in a fresh temporary directory, removed on drop.
struct Tree(PathBuf);

impl Tree {
    fn new() -> Result<Self, String> {
        let root = std::env::temp_dir()
            .join(format!("vibra-runner-trap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let write = |relative: &str, text: &str| {
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::write(path, text).map_err(|error| error.to_string())
        };
        write("project.vibon", PROJECT)?;
        write("src/app/main.vib", "")?;
        write("tests/math.vib", TESTS)?;
        Ok(Self(root))
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
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
    let tree = Tree::new()?;
    let root = fs::canonicalize(&tree.0).map_err(|error| error.to_string())?;
    let project = discover_project_at(&root).map_err(|faults| format!("{faults:?}"))?;
    let snapshot =
        SourceSnapshot::capture(project).map_err(|faults| format!("{faults:?}"))?;
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
