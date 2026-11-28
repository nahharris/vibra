//! The length of a sequence costs no host stack.
//!
//! `docs/spec/05-tooling.md`, "Limits", bounds how deeply a document nests;
//! the length of a body, a list, or a `match` is not nesting. Each shape here
//! is one long flat form in the function `f` of a binary project: sequential
//! `let`s, `let … else`, expression statements, the elements of a list
//! literal, and the arms of a `match`. Every stage runs on a thread of
//! 256 KiB. When a stage recursed along a sequence, 1,000 `let`s overflowed
//! a stack of 1 MiB, so about 250 overflow 256 KiB and the sizes here, eight
//! times that, overflow it if one does, which ends the test process. A
//! `match` of literals has no long form: its coverage is decided within a
//! fixed amount of work, which 2,000 arms fit and 2,800 do not. Each stage
//! must also give its verdict: the check has no error, the test passes, the run exits
//! with the code the entry gives when `f` returns the value its first and
//! last statements decide, and the format plan has no error.

use std::fmt::Write as _;

use vibra_workspace::run::{CommandResult, Launch, run_target};
use vibra_workspace::{
    Discovery, HostWorld, MemoryBudget, SuiteResult, WorkspaceCapture, check_snapshot,
    plan_format, run_tests,
};

use crate::stack::on_stack;
use crate::support::TempDir;

/// The stack of every stage.
const STACK: usize = 256 * 1024;
/// The size of each shape: eight times the length at which a recursing stage
/// overflows [`STACK`].
const LETS: usize = 2_000;
const LET_ELSES: usize = 2_000;
const STATEMENTS: usize = 2_000;
const ELEMENTS: usize = 2_000;
const ARMS: usize = 2_000;
/// The exit code of the entry when `f` returns the expected value.
const EXIT: u8 = 7;

const PROJECT: &str = "{format: @project.v2 package: {name: \"probe\" version: \"0.1.0\"} targets: [{name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: [] grants: [:]}] dependencies: [:]}";

/// The module of `shape` at size `n`, and the value `f` returns.
pub(crate) fn module(shape: &str, n: usize) -> (String, i64) {
    let mut text = String::new();
    let expected = match shape {
        // `a0` is 1, each later `a` one more: `f` is `a0 + a(n-1)`.
        "lets" => {
            text.push_str(
                "fn f returns r: I32 visibility @public do\n  let a0 be 1i32 endlet\n",
            );
            for k in 1..n {
                let _ = writeln!(text, "  let a{k} be I32.add(a{} 1i32) endlet", k - 1);
            }
            let _ = writeln!(text, "  return I32.add(a0 a{})\nendfn", n - 1);
            1 + i64::try_from(n).expect("a size")
        }
        // The same, each binding taken out of an `Option`.
        "letElses" => {
            text.push_str(
                "fn f returns r: I32 visibility @public do\n  let Option.some(value: a0) be Option.some(value: 1i32) else return 0i32 endlet\n",
            );
            for k in 1..n {
                let _ = writeln!(
                    text,
                    "  let Option.some(value: a{k}) be Option.some(value: I32.add(a{} 1i32)) else return 0i32 endlet",
                    k - 1
                );
            }
            let _ = writeln!(text, "  return I32.add(a0 a{})\nendfn", n - 1);
            1 + i64::try_from(n).expect("a size")
        }
        // `n` statements that each read the first binding, then the result.
        "statements" => {
            text.push_str(
                "fn f returns r: I32 visibility @public do\n  let a0 be 2i32 endlet\n",
            );
            for _ in 0..n {
                text.push_str("  if Equatable.equal(a0 0i32) then return 0i32 endif\n");
            }
            text.push_str("  return I32.add(a0 1i32)\nendfn\n");
            3
        }
        // `[0 1 … n-1]`: `f` is its first element plus its last.
        "list" => {
            text.push_str("let x: List(I32) visibility @public be [");
            for k in 0..n {
                let _ = write!(text, "{k}i32 ");
            }
            text.push_str("] endlet\n");
            let _ = writeln!(
                text,
                "fn f returns r: I32 visibility @public do\n  let Option.some(value: first) be x(0u64) else return 0i32 endlet\n  let Option.some(value: last) be x({}u64) else return 0i32 endlet\n  return I32.add(first last)\nendfn",
                n - 1
            );
            i64::try_from(n - 1).expect("a size")
        }
        // Arm `k` gives `k`: `f` is the first arm's value plus the last's.
        "match" => {
            text.push_str("fn g takes {v: I32} returns r: I32 do return match v\n");
            for k in 0..n {
                let _ = writeln!(text, "  case {k}i32 {k}i32");
            }
            text.push_str("  case _ 0i32\nendmatch endfn\n");
            let _ = writeln!(
                text,
                "fn f returns r: I32 visibility @public do return I32.add(g(0i32) g({}i32)) endfn",
                n - 1
            );
            i64::try_from(n - 1).expect("a size")
        }
        _ => panic!("no shape {shape}"),
    };
    let entry = format!(
        "use @std.process.ExitCode enduse\n\nproc main returns code: ExitCode do\n  if Equatable.equal(f() {expected}i32) then return ExitCode({EXIT}u8) endif\n  return ExitCode(1u8)\nendproc\n\n"
    );
    (format!("{entry}{text}"), expected)
}

/// The project of `shape` at size `n`, whose test asserts the value of `f`.
fn project(shape: &str, n: usize) -> TempDir {
    let (main, expected) = module(shape, n);
    let directory = TempDir::new(&format!("sequence-{shape}"));
    directory.write("project.vibon", PROJECT);
    directory.write("src/app/main.vib", &main);
    directory.write(
        "tests/main.vib",
        &format!(
            "use @std.assert enduse\n\nuse @app.main enduse\n\ntest \"f\" do assert.equal(main.f() {expected}i32) endtest\n"
        ),
    );
    directory
}

/// Runs every stage of `shape` at size `n` on [`STACK`] and checks each
/// verdict.
fn stages(shape: &str, n: usize) {
    let directory = project(shape, n);
    let path = directory.path();
    on_stack(STACK, || {
        let capture = WorkspaceCapture::capture(path, Discovery::Exact);
        let snapshot = capture.snapshot().expect("a snapshot");
        assert!(
            snapshot.limit_faults().is_empty(),
            "{shape}: within the limit"
        );

        let check = check_snapshot(snapshot, None);
        let errors = check
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.is_error())
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "{shape}: {errors:?}");
        drop(check);

        let run =
            run_tests(snapshot, None, MemoryBudget::DEFAULT).expect("no selection");
        assert_eq!(
            run.result(),
            SuiteResult::Ok,
            "{shape}: {:?}",
            run.diagnostics()
        );
        drop(run);

        let world = HostWorld::default();
        let outcome =
            run_target(snapshot, None, Launch::World(&world), MemoryBudget::DEFAULT);
        assert_eq!(
            (outcome.result, outcome.application_code),
            (CommandResult::ProgramFailed, Some(EXIT)),
            "{shape}: {:?}",
            outcome.diagnostics
        );
        drop(outcome);

        let plan =
            plan_format(path, &capture, "src/app/main.vib").expect("a module path");
        assert!(!plan.has_errors(), "{shape}: {:?}", plan.diagnostics());
    });
}

/// Every shape at a small size reaches every stage with its verdict.
#[test]
fn short_sequences_give_their_verdicts() {
    for shape in ["lets", "letElses", "statements", "list", "match"] {
        stages(shape, 20);
    }
}

#[test]
fn lets_cost_no_stack() {
    stages("lets", LETS);
}

#[test]
fn let_elses_cost_no_stack() {
    stages("letElses", LET_ELSES);
}

#[test]
fn statements_cost_no_stack() {
    stages("statements", STATEMENTS);
}

#[test]
fn elements_cost_no_stack() {
    stages("list", ELEMENTS);
}

#[test]
fn arms_cost_no_stack() {
    stages("match", ARMS);
}
