//! Probes of the recursions that source nesting does not bound: long chains
//! of declarations, calls, imports, and packages, and long flat forms.
//!
//! `docs/spec/05-tooling.md`, "Limits", bounds how deeply one document
//! nests; nothing bounds how long a chain of declared types, calls, module
//! values, imports, or dependencies is, or how many statements, elements,
//! or arms one form holds. [`recursion_probes`] (ignored: it takes minutes)
//! runs the stages of [`crate::stack`] over a generated input of each kind,
//! each in a child process of this test binary, on a 1 MiB stack (the
//! default main thread of a Windows process) and on [`COMMAND_STACK`], and
//! prints whether each finished, overflowed, or ran past its time.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use vibra_diagnostics::DiagnosticCode;
use vibra_workspace::{
    COMMAND_STACK, Discovery, MemoryBudget, SuiteResult, WorkspaceCapture,
    check_snapshot, run_tests,
};

use crate::probes::{CHAINS, LINKS, PROBES, chain, probe_files};
use crate::stack::{on_stack, run_stage};
use crate::support::TempDir;

/// Writes the probe `name` of size `n` into `directory`.
fn write_probe(directory: &TempDir, name: &str, n: usize) {
    for (path, text) in probe_files(name, n) {
        directory.write(&path, &text);
    }
}

/// Each chain of 100,000 links is `@tool.limitExceeded` alone, at the token
/// the definition of a level names, and every stage answers it on a quarter
/// of the command stack.
#[test]
fn chains_past_the_limit_are_the_limit_alone() {
    for name in CHAINS {
        let directory = TempDir::new(&format!("recursion-chain-{name}"));
        write_probe(&directory, name, LINKS);
        let (_, offset) = chain(name, LINKS);
        let capture = WorkspaceCapture::capture(directory.path(), Discovery::Exact);
        let Some(snapshot) = capture.snapshot() else {
            panic!("{name}: no snapshot: {:?}", capture.diagnostics());
        };
        let check = check_snapshot(snapshot, None);
        let found = check
            .diagnostics()
            .iter()
            .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span().start()))
            .collect::<Vec<_>>();
        assert_eq!(
            found,
            [(DiagnosticCode::ToolLimitExceeded, offset)],
            "{name}"
        );
        for stage in ["check", "test", "run", "fmt"] {
            on_stack(COMMAND_STACK / 4, || run_stage(stage, directory.path()));
        }
    }
}

/// Every probe, at a size of 20, checks with no error and its test passes:
/// the probes reach every stage.
#[test]
fn probes_check_and_pass_when_small() {
    for (name, _) in PROBES {
        let directory = TempDir::new(&format!("recursion-small-{name}"));
        write_probe(&directory, name, 20);
        let capture = WorkspaceCapture::capture(directory.path(), Discovery::Exact);
        let Some(snapshot) = capture.snapshot() else {
            panic!("{name}: no snapshot: {:?}", capture.diagnostics());
        };
        let check = check_snapshot(snapshot, None);
        let errors = check
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.is_error())
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "{name}: {errors:?}");
        let run =
            run_tests(snapshot, None, MemoryBudget::DEFAULT).expect("no selection");
        assert_eq!(
            run.result(),
            SuiteResult::Ok,
            "{name}: {:?}",
            run.diagnostics()
        );
    }
}

/// The variable a child reads: `stage probe size stack`.
const CHILD: &str = "VIBRA_RECURSION_PROBE";

/// A child: runs one stage of one probe on the stack it is given.
#[test]
#[ignore = "a child of `recursion_probes`"]
fn child() {
    let Ok(request) = std::env::var(CHILD) else {
        return;
    };
    let parts = request.split(' ').collect::<Vec<_>>();
    let [stage, name, size, stack] = parts.as_slice() else {
        panic!("a malformed probe {request:?}");
    };
    let size = size.parse::<usize>().expect("a size");
    let stack = stack.parse::<usize>().expect("a stack size");
    let directory = TempDir::new(&format!("recursion-{name}"));
    write_probe(&directory, name, size);
    on_stack(stack, || run_stage(stage, directory.path()));
}

/// How one child ended.
fn outcome(stage: &str, name: &str, size: usize, stack: usize) -> String {
    const TIME: Duration = Duration::from_secs(180);
    let started = Instant::now();
    let mut child = Command::new(std::env::current_exe().expect("this test binary"))
        .args([
            "recursion::child",
            "--exact",
            "--ignored",
            "--test-threads=1",
        ])
        .env(CHILD, format!("{stage} {name} {size} {stack}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("a child");
    loop {
        if let Some(status) = child.try_wait().expect("a status") {
            let seconds = started.elapsed().as_secs_f64();
            return match status.code() {
                Some(0) => format!("ok {seconds:.1}s"),
                // STATUS_STACK_OVERFLOW on Windows.
                Some(code) if code as u32 == 0xC000_00FD => {
                    format!("overflow {seconds:.1}s")
                }
                Some(code) => format!("exit {code:#x} {seconds:.1}s"),
                None => format!("signal {seconds:.1}s"),
            };
        }
        if started.elapsed() > TIME {
            let _ = child.kill();
            let _ = child.wait();
            return format!("time > {}s", TIME.as_secs());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[test]
#[ignore = "minutes; prints the probe table of the report"]
fn recursion_probes() {
    const DEFAULT: usize = 1024 * 1024;
    let only = std::env::var("VIBRA_PROBES").ok();
    let wanted = |name: &str| {
        only.as_deref()
            .is_none_or(|only| only.split(',').any(|item| item == name))
    };
    println!("probe\tstage\tstack\toutcomes by size");
    let chains = CHAINS.iter().map(|name| (*name, &[LINKS][..]));
    for (name, sizes) in PROBES.iter().copied().chain(chains) {
        if !wanted(name) {
            continue;
        }
        for stage in ["capture", "check", "test", "run"] {
            for (label, stack) in [("1 MiB", DEFAULT), ("COMMAND_STACK", COMMAND_STACK)]
            {
                let mut cells = Vec::new();
                for size in sizes {
                    let ended = outcome(stage, name, *size, stack);
                    let stop = !ended.starts_with("ok");
                    cells.push(format!("{size}: {ended}"));
                    if stop {
                        break;
                    }
                }
                println!("{name}\t{stage}\t{label}\t{}", cells.join("; "));
            }
        }
    }
}
