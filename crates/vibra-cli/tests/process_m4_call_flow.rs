//! M4 Step 21b: `vibra check` and `vibra run` on a continuation-passing loop.
//!
//! The binary checks on its default main thread. The loop wraps its
//! continuation in a new closure on every round; the analysis used to build a
//! deeper summary for each wrap and the process aborted with a stack overflow,
//! or reported `@tool.unavailable`. It now checks and runs.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const PROJECT: &str = r#"(record format: @project.v1 package: (record name: "cps" version: "0.1.0") targets: (array (record name: @app kind: @bin root: "src/app" entry: @app.main.main effects: (array))) dependencies: (dict))
"#;

const MAIN: &str = r#"(defn main () void
  (let - (walk 100000u64 (lambda (total u64) u64 total)))
  (do))

(defn walk (count u64 next (fn (u64) u64)) u64
  (if (u64.equal count 0u64)
    (next 0u64)
    (walk (lower count) (lambda (total u64) u64 (next (raise total))))))

(defn raise (count u64) u64
  (match (u64.add-checked count 1u64)
    (result.ok value) value
    (result.err -) 0u64))

(defn lower (count u64) u64
  (match (u64.sub-checked count 1u64)
    (result.ok value) value
    (result.err -) 0u64))
"#;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "vibra-callflow21b-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("src/app")).expect("project");
        std::fs::write(path.join("project.vibon"), PROJECT).expect("manifest");
        std::fs::write(path.join("src/app/main.vib"), MAIN).expect("source");
        Self(path)
    }

    fn vibra(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_vibra"))
            .args(["--workspace", self.0.to_str().expect("utf-8")])
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("vibra binary")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
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
    let run = scratch.vibra(&["run", "src/app"]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
}
