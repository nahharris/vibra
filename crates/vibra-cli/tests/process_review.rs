#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

//! Actual-binary regressions for the M2 review of PR 295: a relocated
//! toolchain, human-mode test reports, host activation exhaustion, `help`, and
//! an initializer cycle through a closure-valued global.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use jsonschema::Validator;
use serde_json::Value;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// A fresh directory under the system temporary root, which is outside the
/// repository checkout the binary was built from.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "vibra-review-{label}-{}-{nonce}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temporary directory");
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A fresh `project init demo` workspace with `src/demo/main.vib` replaced.
struct DemoProject {
    _parent: TempDir,
    root: PathBuf,
}

impl DemoProject {
    fn path(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, content: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().expect("fixture parent"))
            .expect("create fixture directory");
        fs::write(path, content).expect("write fixture");
    }
}

fn demo_project(label: &str, main: &str) -> DemoProject {
    let parent = TempDir::new(label);
    fs::create_dir(parent.path().join("demo")).expect("create destination");
    let output = vibra(parent.path(), &["project", "init", "demo"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let project = DemoProject {
        root: parent.path().join("demo"),
        _parent: parent,
    };
    project.write("src/demo/main.vib", main);
    project
}

fn vibra(root: &Path, arguments: &[&str]) -> Output {
    vibra_at(Path::new(env!("CARGO_BIN_EXE_vibra")), root, arguments)
}

fn vibra_at(binary: &Path, root: &Path, arguments: &[&str]) -> Output {
    Command::new(binary)
        .current_dir(root)
        .args(arguments)
        .output()
        .expect("run the vibra binary")
}

fn json(output: &Output, exit: i32, command: &str, result: &str) -> Value {
    assert_eq!(output.status.code(), Some(exit), "{output:?}");
    let envelope: Value =
        serde_json::from_slice(&output.stdout).expect("stdout is one JSON document");
    let schema: Value = serde_json::from_str(vibra_schema::COMMAND_RESULT_SCHEMA)
        .expect("command-result schema is JSON");
    let validator = Validator::new(&schema).expect("command-result schema compiles");
    assert!(validator.is_valid(&envelope), "{envelope:#}");
    assert_eq!(envelope["command"], command);
    assert_eq!(envelope["result"], result);
    envelope
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("utf-8 output")
}

const ASSERT_SUITE: &str = "(import assert @std.assert)\n(test \"passes\" (assert.true true))\n(test \"strings\" (assert.equal-str \"x\" \"y\"))\n(test \"negation\" (assert.false true))\n";

#[test]
fn a_relocated_binary_verifies_its_embedded_bootstrap() {
    let installation = TempDir::new("installation");
    let binary = installation.path().join(
        Path::new(env!("CARGO_BIN_EXE_vibra"))
            .file_name()
            .expect("binary name"),
    );
    fs::copy(env!("CARGO_BIN_EXE_vibra"), &binary).expect("copy the built binary");
    let project = demo_project(
        "relocated",
        "(import text @std.text)\n(defn main () void (let - (text.length \"A😀\") (do)))\n",
    );
    project.write(
        "tests/math.vib",
        "(import assert @std.assert)\n(test \"works\" (assert.equal-u64 2u64 2u64))\n",
    );

    for arguments in [&["check"][..], &["run", "src/demo"], &["test"]] {
        let output = vibra_at(&binary, project.path(), arguments);
        assert_eq!(output.status.code(), Some(0), "{arguments:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{arguments:?}: {output:?}");
    }
    let mut arguments = vec!["--format", "json", "test"];
    let output = vibra_at(&binary, project.path(), &arguments);
    let envelope = json(&output, 0, "test", "@command.ok");
    assert_eq!(envelope["payload"]["passed"], 1);
    arguments.truncate(2);
    arguments.extend(["run", "src/demo"]);
    let output = vibra_at(&binary, project.path(), &arguments);
    json(&output, 0, "run", "@command.ok");
}

#[test]
fn human_test_reports_each_failure_and_a_summary() {
    let project = demo_project("human-failures", "(defn main () void (do))\n");
    project.write("tests/t.vib", ASSERT_SUITE);

    let output = vibra(project.path(), &["test"]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        text(&output.stdout),
        "FAIL @tests.t::\"strings\" @test.assertion-failed\n  assertion @std.assert.equal-str expected=\"x\" actual=\"y\" at tests/t.vib:3:17\nFAIL @tests.t::\"negation\" @test.assertion-failed\n  assertion @std.assert.false expected=false actual=true at tests/t.vib:4:18\ntest suite @command.test-failed: 1 passed, 2 failed, 3 selected\n"
    );
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn human_test_reports_unavailable_items_and_their_diagnostics() {
    let project = demo_project("human-unavailable", "(defn main () void (do))\n");
    project.write(
        "tests/t.vib",
        "(import assert @std.assert)\n(test \"generic\" (assert.equal 1i32 1i32))\n",
    );

    let output = vibra(project.path(), &["test"]);

    assert_eq!(output.status.code(), Some(4), "{output:?}");
    assert_eq!(
        text(&output.stdout),
        "FAIL @tests.t::\"generic\" @test.unavailable\ntest suite @command.unavailable: 0 passed, 1 failed, 1 selected\n"
    );
    assert!(
        text(&output.stderr).contains("@tool.unavailable"),
        "{output:?}"
    );
}

#[test]
fn human_test_success_keeps_its_single_line() {
    let project = demo_project("human-pass", "(defn main () void (do))\n");
    project.write(
        "tests/t.vib",
        "(import assert @std.assert)\n(test \"passes\" (assert.true true))\n",
    );

    let output = vibra(project.path(), &["test"]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(text(&output.stdout), "test suite passed: 1 test(s)\n");
}

const DEEP_RECURSION: &str =
    "(defn f () i32 (let v (f) v))\n(defn main () void (let - (f) (do)))\n";

#[test]
fn deep_non_tail_recursion_is_an_operational_failure_not_an_abort() {
    let project = demo_project("deep-recursion", DEEP_RECURSION);

    let human = vibra(project.path(), &["run", "src/demo"]);
    assert_eq!(human.status.code(), Some(3), "{human:?}");
    assert!(human.stdout.is_empty(), "{human:?}");
    assert!(
        text(&human.stderr).contains("@runtime.host-stack-exhausted"),
        "{human:?}"
    );

    let output = vibra(project.path(), &["--format", "json", "run", "src/demo"]);
    let envelope = json(&output, 3, "run", "@command.operational-failure");
    assert_eq!(envelope["payload"]["programResult"], Value::Null);
    assert_eq!(envelope["payload"]["trap"], Value::Null);
    assert_eq!(
        envelope["diagnostics"][0]["code"],
        "@runtime.host-stack-exhausted"
    );
    assert_eq!(envelope["diagnostics"][0]["primarySpan"]["start"], 0);
    assert_eq!(envelope["diagnostics"][0]["primarySpan"]["end"], 0);
    assert_eq!(
        envelope["diagnostics"][0]["primarySpan"]["sourceId"],
        Value::Null
    );
}

#[test]
fn deep_non_tail_recursion_in_a_test_stops_the_suite() {
    let project = demo_project("deep-recursion-test", "(defn main () void (do))\n");
    project.write(
        "tests/t.vib",
        "(import assert @std.assert)\n(defn f () i32 (let v (f) v))\n(test \"deep\" (assert.equal-i32 (f) 0i32))\n",
    );

    let output = vibra(project.path(), &["--format", "json", "test"]);

    let envelope = json(&output, 3, "test", "@command.operational-failure");
    assert_eq!(envelope["payload"]["selected"], 0);
    assert_eq!(envelope["payload"]["tests"], serde_json::json!([]));
    assert_eq!(
        envelope["diagnostics"][0]["code"],
        "@runtime.host-stack-exhausted"
    );
}

#[test]
fn help_prints_the_closed_grammar() {
    let directory = TempDir::new("help");
    for spelling in ["help", "--help", "-h"] {
        let output = vibra(directory.path(), &[spelling]);
        assert_eq!(output.status.code(), Some(0), "{spelling}: {output:?}");
        let usage = text(&output.stdout);
        assert!(usage.starts_with("usage:\n"), "{usage}");
        assert!(usage.contains("test [TEST]"), "{usage}");
        assert!(output.stderr.is_empty());
    }
    let output = vibra(directory.path(), &["--format", "json", "--help"]);
    let envelope = json(&output, 0, "help", "@command.ok");
    assert!(
        envelope["payload"]["usage"]
            .as_str()
            .is_some_and(|usage| usage.contains("project init [DEST]"))
    );

    let output = vibra(directory.path(), &["--format", "json", "help", "run"]);
    json(&output, 2, "invalid", "@command.invalid-input");
    let output = vibra(directory.path(), &[]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(text(&output.stderr).contains("vibra help"), "{output:?}");
}

#[test]
fn an_initializer_cycle_through_a_closure_global_is_rejected_before_running() {
    let project = demo_project(
        "closure-cycle",
        "(def h (fn () i32) (lambda () i32 x))\n(def x i32 (h))\n(defn main () void (let - x (do)))\n",
    );

    for command in [&["check"][..], &["run", "src/demo"]] {
        let mut arguments = vec!["--format", "json"];
        arguments.extend_from_slice(command);
        let output = vibra(project.path(), &arguments);
        let envelope = json(&output, 1, command[0], "@command.diagnostics");
        assert_eq!(
            envelope["diagnostics"][0]["code"],
            "@type.initializer-cycle"
        );
        assert_eq!(envelope["diagnostics"][0]["primarySpan"]["start"], 38);
        if command[0] == "run" {
            assert_eq!(envelope["payload"]["programResult"], Value::Null);
        }
    }
}
