#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

//! Process-level command contract tests for M2 Step 12.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use jsonschema::Validator;
use serde_json::Value;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempProject(PathBuf);

impl TempProject {
    fn new(label: &str, targets: &str, sources: &[(&str, &str)]) -> Self {
        let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "vibra-cli-step12-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src/app")).expect("create project source root");
        fs::write(
            root.join("project.vibon"),
            format!("(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array {targets}) dependencies: (map))\n"),
        )
        .expect("write project marker");
        for (path, source) in sources {
            let path = root.join(path);
            fs::create_dir_all(path.parent().expect("source parent"))
                .expect("create module directory");
            fs::write(path, source).expect("write module source");
        }
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vibra"))
        .args(args)
        .output()
        .expect("run the built vibra binary")
}

fn run_from(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vibra"))
        .current_dir(directory)
        .args(args)
        .output()
        .expect("run the built vibra binary from the selected directory")
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout is one JSON document")
}

fn assert_envelope_schema(envelope: &Value) {
    let schema: Value = serde_json::from_str(vibra_schema::COMMAND_RESULT_SCHEMA)
        .expect("checked-in command result schema is valid JSON");
    let validator = Validator::new(&schema).expect("command result schema compiles");
    assert!(
        validator.is_valid(envelope),
        "envelope violates command-result schema: {}",
        serde_json::to_string_pretty(envelope).expect("envelope serializes")
    );
}

fn binary_target() -> &'static str {
    "(record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array))"
}

#[test]
fn check_accepts_a_selected_target_by_its_project_relative_root() {
    let project = TempProject::new(
        "check-selected",
        binary_target(),
        &[("src/app/main.vib", "(defn execute () void (do))\n")],
    );
    let workspace = project.path().to_string_lossy().into_owned();

    let output = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "check",
        "src/app",
    ]);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["command"], "check");
    assert_eq!(envelope["result"], "@command.ok");
    assert_eq!(envelope["diagnostics"], serde_json::json!([]));
    assert_eq!(envelope["payload"], serde_json::json!({ "accepted": true }));
}

#[test]
fn run_emits_one_json_envelope_and_keeps_program_output_inside_it() {
    let project = TempProject::new(
        "run-json",
        binary_target(),
        &[("src/app/main.vib", "(defn execute () void (do))\n")],
    );
    let workspace = project.path().to_string_lossy().into_owned();

    let output = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "run",
        "src/app",
    ]);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["command"], "run");
    assert_eq!(envelope["result"], "@command.ok");
    assert_eq!(envelope["diagnostics"], serde_json::json!([]));
    assert_eq!(envelope["payload"]["target"], "src/app");
    assert_eq!(envelope["payload"]["stdout"], "");
    assert_eq!(envelope["payload"]["stderr"], "");
    assert_eq!(envelope["payload"]["auditTrace"], serde_json::json!([]));
    assert_eq!(envelope["payload"]["trap"], Value::Null);
}

#[test]
fn explicit_check_does_not_fail_on_an_unrelated_target() {
    let project = TempProject::new(
        "unrelated-error",
        &format!(
            "{} (record name: @other kind: @lib root: \"src/other\")",
            binary_target()
        ),
        &[
            ("src/app/main.vib", "(defn execute () void (do))\n"),
            (
                "src/other/broken.vib",
                "(defn invalid () i32 \"wrong type\")\n",
            ),
        ],
    );
    let workspace = project.path().to_string_lossy().into_owned();

    let selected = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "check",
        "src/app",
    ]);
    let all = run(&["--format", "json", "--workspace", &workspace, "check"]);

    assert_eq!(selected.status.code(), Some(0));
    let selected_envelope = json(&selected);
    assert_envelope_schema(&selected_envelope);
    assert_eq!(selected_envelope["payload"]["accepted"], true);
    assert_eq!(all.status.code(), Some(1));
    let all_envelope = json(&all);
    assert_envelope_schema(&all_envelope);
    assert_eq!(all_envelope["result"], "@command.diagnostics");
    assert_eq!(all_envelope["payload"]["accepted"], false);
}

#[test]
fn unknown_target_is_invalid_input_with_the_documented_exit() {
    let project = TempProject::new(
        "unknown-target",
        binary_target(),
        &[("src/app/main.vib", "(defn execute () void (do))\n")],
    );
    let workspace = project.path().to_string_lossy().into_owned();

    let output = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "check",
        "src/missing",
    ]);

    assert_eq!(output.status.code(), Some(2));
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["result"], "@command.invalid-input");
    assert_eq!(envelope["diagnostics"], serde_json::json!([]));
    assert_eq!(envelope["payload"]["accepted"], false);
}

#[test]
fn selected_check_includes_every_declaration_in_the_transitive_import_closure() {
    let project = TempProject::new(
        "import-closure-error",
        &format!(
            "{} (record name: @util kind: @lib root: \"src/util\")",
            binary_target()
        ),
        &[
            (
                "src/app/main.vib",
                "(import util @util.api)\n(defn execute () void (util.call))\n",
            ),
            (
                "src/util/api.vib",
                "(defn call () void (do) visibility: @public)\n(defn unused () i32 \"wrong type\")\n",
            ),
        ],
    );
    let workspace = project.path().to_string_lossy().into_owned();

    let check = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "check",
        "src/app",
    ]);
    let run_target = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "run",
        "src/app",
    ]);

    assert_eq!(check.status.code(), Some(1));
    let check_envelope = json(&check);
    assert_envelope_schema(&check_envelope);
    assert_eq!(check_envelope["result"], "@command.diagnostics");
    assert_eq!(check_envelope["payload"]["accepted"], false);
    assert!(
        check_envelope["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .iter()
            .any(|diagnostic| {
                diagnostic["primarySpan"]["sourceId"] == "src/util/api.vib"
            })
    );

    assert_eq!(run_target.status.code(), Some(1));
    let run_envelope = json(&run_target);
    assert_envelope_schema(&run_envelope);
    assert_eq!(run_envelope["result"], "@command.diagnostics");
    assert_eq!(run_envelope["payload"]["target"], "src/app");
    assert_eq!(run_envelope["payload"]["programResult"], Value::Null);
}

#[test]
fn run_rejects_a_library_target_as_invalid_input() {
    let project = TempProject::new(
        "run-library",
        &format!(
            "{} (record name: @util kind: @lib root: \"src/util\")",
            binary_target()
        ),
        &[
            ("src/app/main.vib", "(defn execute () void (do))\n"),
            (
                "src/util/api.vib",
                "(defn call () void (do) visibility: @public)\n",
            ),
        ],
    );
    let workspace = project.path().to_string_lossy().into_owned();

    let output = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "run",
        "src/util",
    ]);

    assert_eq!(output.status.code(), Some(2));
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["result"], "@command.invalid-input");
    assert_eq!(envelope["diagnostics"], serde_json::json!([]));
}

#[test]
fn check_and_run_select_project_relative_targets_from_a_nested_cwd() {
    let project = TempProject::new(
        "nested-cwd-target",
        binary_target(),
        &[("src/app/main.vib", "(defn execute () void (do))\n")],
    );
    let nested = project.path().join("src/nested");
    fs::create_dir_all(&nested).expect("create nested working directory");

    for command in ["check", "run"] {
        let output = run_from(&nested, &["--format", "json", command, "src/app"]);

        assert_eq!(output.status.code(), Some(0), "{command}: {:?}", output);
        assert!(output.stderr.is_empty(), "{command}: {:?}", output.stderr);
        let envelope = json(&output);
        assert_envelope_schema(&envelope);
        assert_eq!(envelope["command"], command);
        assert_eq!(envelope["result"], "@command.ok");
        if command == "run" {
            assert_eq!(envelope["payload"]["target"], "src/app");
        }
    }
}

#[test]
fn signed_stdlib_imports_cannot_be_rebound_by_a_local_std_target() {
    let project = TempProject::new(
        "reserved-stdlib-overlay",
        &format!(
            "{} (record name: @std kind: @lib root: \"src/std\")",
            binary_target()
        ),
        &[
            (
                "src/app/main.vib",
                "(import text @std.text)\n(defn execute () void (let - (text.length \"local\") (do)))\n",
            ),
            (
                "src/std/text.vib",
                "(defn length (value str) u64 visibility: @public 999u64)\n",
            ),
        ],
    );
    let workspace = project.path().to_string_lossy().into_owned();

    for command in ["check", "run"] {
        let output = run(&[
            "--format",
            "json",
            "--workspace",
            &workspace,
            command,
            "src/app",
        ]);

        assert_eq!(output.status.code(), Some(0), "{command}: {:?}", output);
        let envelope = json(&output);
        assert_envelope_schema(&envelope);
        assert_eq!(envelope["result"], "@command.ok");
        assert_eq!(envelope["diagnostics"], serde_json::json!([]));
    }
}

#[test]
fn check_returns_without_interpreting_a_nonterminating_pure_entry() {
    let project = TempProject::new(
        "check-does-not-run",
        binary_target(),
        &[(
            "src/app/main.vib",
            "(defn spin () void (spin))\n(defn execute () void (spin))\n",
        )],
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_vibra"))
        .current_dir(project.path())
        .args(["--format", "json", "check", "src/app"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start check process");
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if child.try_wait().expect("poll check process").is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("check executed a nonterminating entry function");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output = child
        .wait_with_output()
        .expect("collect completed check output");

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["result"], "@command.ok");
    assert_eq!(envelope["payload"]["accepted"], true);
}

#[test]
fn project_load_diagnostics_keep_their_source_line_and_column() {
    let project = TempProject::new(
        "project-load-location",
        binary_target(),
        &[("src/app/main.vib", "(defn execute () void (do))\n")],
    );
    fs::write(
        project.path().join("project.vibon"),
        "(record format: @project.v1\n package: (record name: \"demo\" version: \"bad\")\n targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array)))\n dependencies: (map))\n",
    )
    .expect("replace project marker with a line-two schema error");
    let workspace = project.path().to_string_lossy().into_owned();

    let output = run(&["--format", "json", "--workspace", &workspace, "check"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["result"], "@command.diagnostics");
    let diagnostic = &envelope["diagnostics"][0];
    assert_eq!(diagnostic["primarySpan"]["sourceId"], "project.vibon");
    assert_eq!(diagnostic["primarySpan"]["startPosition"]["line"], 2);
    assert!(
        diagnostic["primarySpan"]["startPosition"]["column"]
            .as_u64()
            .is_some_and(|column| column > 1)
    );
}
