//! M4 Step 21b: `vibra check` and `vibra run` on a continuation-passing loop.
//!
//! The binary checks on its default main thread. The loop wraps its
//! continuation in a new closure on every round; the analysis used to build a
//! deeper summary for each wrap and the process aborted with a stack overflow,
//! or reported `@tool.unavailable`. It now checks and runs.

use std::process::Output;

use crate::support::{TempDir, app_target, manifest, run_workspace};

const MAIN: &str = r#"(defn main () void
  (let - (walk 100000u64 (lambda (total u64) u64 total)))
  (do))

(defn walk (count u64 next (fn (u64) u64)) u64
  (if (equatable.equal count 0u64)
    (next 0u64)
    (walk (lower count) (lambda (total u64) u64 (next (raise total))))))

(defn raise (count u64) u64
  (match (u64.add count 1u64)
    (result.ok value) value
    (result.err -) 0u64))

(defn lower (count u64) u64
  (match (u64.sub count 1u64)
    (result.ok value) value
    (result.err -) 0u64))
"#;

struct Scratch(TempDir);

impl Scratch {
    fn new() -> Self {
        let manifest = manifest("cps", &app_target("main", ""), "(dict)");
        Self(TempDir::project(
            "callflow21b",
            &manifest,
            &[("src/app/main.vib", MAIN)],
        ))
    }

    fn vibra(&self, arguments: &[&str]) -> Output {
        run_workspace(self.0.path(), arguments, None)
    }
}

#[test]
fn check_accepts_a_continuation_passing_loop() {
    let scratch = Scratch::new();
    let check = scratch.vibra(&["check"]);
    assert_eq!(check.status.code(), Some(0), "{check:?}");
    assert_eq!(
        String::from_utf8(check.stdout).expect("utf-8").trim(),
        "check accepted"
    );
}

#[test]
fn run_completes_a_continuation_passing_loop() {
    let scratch = Scratch::new();
    let run = scratch.vibra(&["run", "@app"]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
}
