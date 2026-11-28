//! The host stack the stages need, by nesting shape: the measurement behind
//! [`COMMAND_STACK`], and the guard that keeps it true.
//!
//! Every shape writes one form nested `n` levels (`docs/spec/05-tooling.md`,
//! "Limits") into the module value `x` of a binary project, which a test
//! and the entry read. The stages are those a command runs after the
//! reader: the capture with its measurement, resolution, the check (which
//! resolves, checks, lowers, and validates the checked program), the test
//! run, the run of the entry in the default world, and the format plan.
//!
//! [`stages_fit_a_quarter_of_the_command_stack`] runs every stage on every
//! shape at the limit on a thread of a quarter of [`COMMAND_STACK`]: an
//! overflow there ends the test process, which fails it.
//! [`stack_per_level`] (ignored: about ten minutes) finds by bisection the
//! smallest stack each stage needs at 16 and at 1,024 levels, each probe in
//! a child process of this test binary, and prints the table that the
//! documentation of [`COMMAND_STACK`] cites.

use std::path::Path;
use std::process::Command;

use vibra_workspace::run::{Launch, run_target};
use vibra_workspace::{
    COMMAND_STACK, Discovery, HostWorld, MemoryBudget, WorkspaceCapture, plan_format,
    resolve_input, run_tests, source_nesting_limit,
};

use crate::support::TempDir;

/// The project of every shape: one binary target over `src/app`.
pub(crate) const PROJECT: &str = "{format: @project.v2 package: {name: \"probe\" version: \"0.1.0\"} targets: [{name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: [] grants: [:]}] dependencies: [:]}";

/// The test that reads the value.
const TEST: &str =
    "use @app.main enduse\n\ntest \"reads x\" do let _ be main.x endlet endtest\n";

/// The shapes, by name.
const SHAPES: &[&str] = &[
    "do", "list", "call", "apply", "type", "pattern", "let", "if", "field", "prefix",
];

/// The stages, by name.
const STAGES: &[&str] = &["capture", "resolve", "check", "test", "run", "fmt"];

/// The module of `shape` with its form nested `n` levels deep.
fn module(shape: &str, n: usize) -> String {
    let entry = "use @std.process.ExitCode enduse\n\nproc main returns code: ExitCode do let _ be x endlet return ExitCode(0u8) endproc\n";
    let value = |ty: &str, init: String| {
        format!("{entry}\nlet x: {ty} visibility @public be {init} endlet\n")
    };
    match shape {
        "do" => value(
            "I32",
            format!("{}1i32{}", "do ".repeat(n), " enddo".repeat(n)),
        ),
        "list" => value("I32", format!("{}1i32{}", "[".repeat(n), "]".repeat(n))),
        "call" => format!(
            "fn id takes {{v: I32}} returns r: I32 do v endfn\n{}",
            value("I32", format!("{}1i32{}", "id(".repeat(n), ")".repeat(n)))
        ),
        // `f()()…`: each application encloses the form before it.
        "apply" => format!(
            "fn f returns r: I32 do 1i32 endfn\n{}",
            value("I32", format!("f{}", "()".repeat(n)))
        ),
        "type" => value(
            &format!("{}I32{}", "List(".repeat(n), ")".repeat(n)),
            "[]".to_owned(),
        ),
        "pattern" => value(
            "I32",
            format!(
                "match 1i32 case {}v{} 1i32 case _ 0i32 endmatch",
                "{".repeat(n.saturating_sub(1)),
                "}".repeat(n.saturating_sub(1))
            ),
        ),
        // Each level of `do let a be … endlet a enddo` is two forms.
        "let" => value(
            "I32",
            format!(
                "{}1i32{}",
                "do let a be ".repeat(n / 2),
                " endlet a enddo".repeat(n / 2)
            ),
        ),
        // The condition's `Ordered` is nested two levels in its `if`: its
        // field read and its application.
        "if" => value(
            "I32",
            format!(
                "{}1i32{}",
                "if Ordered.greater(1i32 0i32) then ".repeat(n - 2),
                " else 0i32 endif".repeat(n - 2)
            ),
        ),
        "field" => value("I32", format!("y{}", ".a".repeat(n))),
        "prefix" => format!(
            "fn f returns r: I32 do {}1i32 endfn\n{}",
            "return ".repeat(n),
            value("I32", "f()".to_owned())
        ),
        _ => panic!("no shape {shape}"),
    }
}

fn project(shape: &str, n: usize) -> TempDir {
    let directory = TempDir::new(&format!("stack-{shape}-{n}"));
    directory.write("project.vibon", PROJECT);
    directory.write("src/app/main.vib", &module(shape, n));
    directory.write("tests/main.vib", TEST);
    directory
}

/// Runs `stage` over the project in `directory`.
pub(crate) fn run_stage(stage: &str, directory: &Path) {
    let capture = WorkspaceCapture::capture(directory, Discovery::Exact);
    let Some(snapshot) = capture.snapshot() else {
        panic!("no snapshot: {:?}", capture.diagnostics());
    };
    match stage {
        "capture" => {}
        "resolve" => {
            let input = resolve_input(snapshot).expect("the library loads");
            let _ = vibra_resolve::resolve(&input);
        }
        "check" => {
            let _ = vibra_workspace::check_snapshot(snapshot, None);
        }
        "test" => {
            let _ = run_tests(snapshot, None, MemoryBudget::DEFAULT);
        }
        "run" => {
            let world = HostWorld::default();
            let _ = run_target(
                snapshot,
                None,
                Launch::World(&world),
                MemoryBudget::DEFAULT,
            );
        }
        "fmt" => {
            let _ = plan_format(directory, &capture, "src/app/main.vib");
        }
        _ => panic!("no stage {stage}"),
    }
}

/// Runs `body` on a thread with `stack` bytes; an overflow ends the process.
pub(crate) fn on_stack(stack: usize, body: impl FnOnce() + Send) {
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .stack_size(stack)
            .spawn_scoped(scope, body)
            .expect("a thread");
        assert!(handle.join().is_ok(), "the stage finished");
    });
}

#[test]
fn stages_fit_a_quarter_of_the_command_stack() {
    let limit = source_nesting_limit();
    for shape in SHAPES {
        let directory = project(shape, limit);
        let capture = WorkspaceCapture::capture(directory.path(), Discovery::Exact);
        let faults = capture.snapshot().expect("a snapshot").limit_faults();
        assert!(faults.is_empty(), "{shape} is within the limit: {faults:?}");
        for stage in STAGES {
            on_stack(COMMAND_STACK / 4, || run_stage(stage, directory.path()));
        }
    }
}

/// The variable a child probe reads: `stage shape levels stack`.
const PROBE: &str = "VIBRA_STACK_PROBE";

/// A child probe: runs one stage of one shape on the stack it is given. It
/// does nothing when it is not a child.
#[test]
#[ignore = "a child of `stack_per_level`"]
fn probe() {
    let Ok(request) = std::env::var(PROBE) else {
        return;
    };
    let parts = request.split(' ').collect::<Vec<_>>();
    let [stage, shape, levels, stack] = parts.as_slice() else {
        panic!("a malformed probe {request:?}");
    };
    let levels = levels.parse::<usize>().expect("levels");
    let stack = stack.parse::<usize>().expect("a stack size");
    let directory = project(shape, levels);
    on_stack(stack, || run_stage(stage, directory.path()));
}

/// Whether `stage` of `shape` at `levels` finishes on `stack` bytes, in a
/// child process.
fn fits(stage: &str, shape: &str, levels: usize, stack: usize) -> bool {
    let status = Command::new(std::env::current_exe().expect("this test binary"))
        .args(["stack::probe", "--exact", "--ignored", "--test-threads=1"])
        .env(PROBE, format!("{stage} {shape} {levels} {stack}"))
        .output()
        .expect("a child probe");
    status.status.success()
}

/// The smallest stack, to 4 KiB, on which `stage` of `shape` at `levels`
/// finishes.
fn smallest(stage: &str, shape: &str, levels: usize) -> usize {
    const STEP: usize = 4 * 1024;
    let mut high = 256 * 1024;
    while !fits(stage, shape, levels, high) {
        high *= 2;
        assert!(
            high <= 4 * 1024 * 1024 * 1024,
            "{stage} {shape}: over 4 GiB"
        );
    }
    let mut low = high / 2;
    while high - low > STEP {
        let middle = (low + high) / 2 / STEP * STEP;
        if fits(stage, shape, levels, middle) {
            high = middle;
        } else {
            low = middle;
        }
    }
    high
}

#[test]
#[ignore = "about ten minutes; prints the table of COMMAND_STACK"]
fn stack_per_level() {
    let limit = source_nesting_limit();
    let base = 16;
    let mut deepest = (0, "", "");
    println!("stage\tshape\tat {base} KiB\tat {limit} KiB\tbytes per level");
    let only = std::env::var("VIBRA_SHAPES").ok();
    for shape in SHAPES {
        if only
            .as_deref()
            .is_some_and(|only| !only.split(',').any(|item| item == *shape))
        {
            continue;
        }
        for stage in STAGES {
            let small = smallest(stage, shape, base);
            let large = smallest(stage, shape, limit);
            let per_level = large.saturating_sub(small) / (limit - base);
            println!(
                "{stage}\t{shape}\t{}\t{}\t{per_level}",
                small / 1024,
                large / 1024
            );
            if large > deepest.0 {
                deepest = (large, stage, shape);
            }
        }
    }
    println!(
        "deepest: {} {} at {limit} levels needs {} KiB; COMMAND_STACK is {} KiB",
        deepest.1,
        deepest.2,
        deepest.0 / 1024,
        COMMAND_STACK / 1024
    );
    assert!(COMMAND_STACK >= 4 * deepest.0);
}
