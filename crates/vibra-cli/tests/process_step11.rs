#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

//! Process-level command contract tests for M2 Step 11.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use jsonschema::Validator;
use serde_json::Value;
use vibra_syntax::parse_source;
use vibra_types::check_source;
use vibra_workspace::WorkspaceSnapshot;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "vibra-cli-step11-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test root");
        Self(path)
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

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vibra"))
        .args(args)
        .output()
        .expect("run the built vibra binary")
}

fn run_in(args: &[&str], current_dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vibra"))
        .current_dir(current_dir)
        .args(args)
        .output()
        .expect("run the built vibra binary in the requested directory")
}

#[cfg(unix)]
fn directory_identity(path: &Path) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::metadata(path).expect("read workspace directory identity");
    (metadata.dev(), metadata.ino())
}

#[cfg(windows)]
fn directory_identity(path: &Path) -> same_file::Handle {
    same_file::Handle::from_path(path).expect("open workspace directory identity")
}

#[cfg(not(any(unix, windows)))]
fn directory_identity(path: &Path) -> PathBuf {
    path.canonicalize().expect("canonical workspace directory")
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

#[test]
fn init_emits_versioned_json_and_creates_a_decodable_pure_entry_project() {
    let root = TempDir::new("init");
    let workspace_path = root.path().join("hello");
    fs::create_dir(&workspace_path).expect("create hello workspace");
    let workspace = workspace_path.to_string_lossy().into_owned();
    let output = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "project",
        "init",
    ]);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["schemaVersion"], 1);
    assert_eq!(envelope["command"], "init");
    assert_eq!(envelope["result"], "@command.ok");
    assert_eq!(envelope["diagnostics"], serde_json::json!([]));
    assert_eq!(
        envelope["payload"]["created"],
        serde_json::json!([
            "project.vibon",
            "src",
            "src/hello",
            "src/hello/main.vib",
            "tests"
        ])
    );

    let snapshot = WorkspaceSnapshot::load_confined(&workspace_path)
        .expect("generated project decodes and has a confined source snapshot");
    let resolved = snapshot.resolve().expect("resolve generated entry");
    assert!(resolved.accepted(), "{:?}", resolved.diagnostics());
    let entry = snapshot
        .source()
        .documents()
        .find(|document| document.source_id() == "src/hello/main.vib")
        .expect("generated entry is in the target snapshot");
    let text = std::str::from_utf8(entry.bytes()).expect("entry is UTF-8");
    assert!(check_source(entry.source_id(), text).accepted());
    assert!(
        !text.contains("@std."),
        "the pure entry needs no bootstrap import"
    );
}

#[test]
fn init_accepts_an_empty_workspace_relative_destination() {
    let root = TempDir::new("init-destination");
    let destination = root.path().join("hello-app");
    fs::create_dir(&destination).expect("create empty destination");
    let workspace = root.path().to_string_lossy().into_owned();
    let output = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "project",
        "init",
        "hello-app",
    ]);

    assert_eq!(output.status.code(), Some(0));
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    let canonical_destination =
        destination.canonicalize().expect("canonical destination");
    assert_eq!(
        envelope["payload"]["workspace"].as_str(),
        canonical_destination.to_str()
    );
    assert!(destination.join("project.vibon").is_file());
    assert!(destination.join("src/hello-app/main.vib").is_file());
    assert!(destination.join("tests").is_dir());
}

#[test]
fn default_init_populates_the_empty_current_directory_without_replacing_it() {
    let root = TempDir::new("init-current-dir");
    let workspace_path = root.path().join("workspace");
    fs::create_dir(&workspace_path).expect("create empty current directory");
    let original_root = workspace_path.canonicalize().expect("canonical workspace");
    let original_identity = directory_identity(&workspace_path);

    let output = run_in(&["--format", "json", "project", "init"], &workspace_path);

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["result"], "@command.ok");
    assert_eq!(
        envelope["payload"]["workspace"].as_str(),
        original_root.to_str()
    );
    assert_eq!(
        workspace_path
            .canonicalize()
            .expect("workspace remains present"),
        original_root,
        "initialization preserves the current-directory root"
    );
    assert_eq!(
        directory_identity(&workspace_path),
        original_identity,
        "initialization preserves the workspace directory identity"
    );
    assert!(workspace_path.join("project.vibon").is_file());
    assert!(workspace_path.join("src/workspace/main.vib").is_file());
    assert!(workspace_path.join("tests").is_dir());
    assert_eq!(
        fs::read_dir(&workspace_path)
            .expect("read initialized workspace")
            .count(),
        3,
        "successful init removes its in-workspace staging directory"
    );
}

#[test]
fn init_refuses_a_nonempty_destination_without_overwriting_it() {
    let root = TempDir::new("nonempty");
    let destination = root.path().join("existing");
    fs::create_dir(&destination).expect("create destination");
    fs::write(destination.join("keep.txt"), "original").expect("write sentinel");
    let workspace = root.path().to_string_lossy().into_owned();

    let output = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "project",
        "init",
        "existing",
    ]);

    assert_eq!(output.status.code(), Some(2));
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["result"], "@command.invalid-input");
    assert_eq!(
        fs::read_to_string(destination.join("keep.txt")).expect("read sentinel"),
        "original"
    );
    assert!(!destination.join("project.vibon").exists());
}

#[test]
fn fmt_previews_without_writing_then_writes_canonical_source() {
    let root = TempDir::new("fmt");
    let workspace_path = root.path().join("hello");
    fs::create_dir(&workspace_path).expect("create hello workspace");
    let workspace = workspace_path.to_string_lossy().into_owned();
    let init = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "project",
        "init",
    ]);
    assert_eq!(init.status.code(), Some(0));
    let source_path = workspace_path.join("src/hello/main.vib");
    fs::write(&source_path, "(defn main ( ) void (do))\r\n")
        .expect("write unformatted source");
    let before = fs::read(&source_path).expect("read source before preview");

    let preview = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "fmt",
        "src/hello/main.vib",
    ]);
    assert_eq!(preview.status.code(), Some(0));
    let envelope = json(&preview);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["command"], "fmt");
    assert_eq!(envelope["result"], "@command.ok");
    assert_eq!(envelope["payload"]["path"], "src/hello/main.vib");
    assert_eq!(envelope["payload"]["changed"], true);
    assert_eq!(envelope["payload"]["written"], false);
    let formatted = envelope["payload"]["text"]
        .as_str()
        .expect("preview contains formatted text");
    assert_eq!(fs::read(&source_path).expect("read after preview"), before);

    let write = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "fmt",
        "src/hello/main.vib",
        "--write",
    ]);
    assert_eq!(write.status.code(), Some(0));
    let envelope = json(&write);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["payload"]["changed"], true);
    assert_eq!(envelope["payload"]["written"], true);
    assert_eq!(
        fs::read_to_string(&source_path).expect("read after write"),
        formatted
    );
    let document = parse_source("src/hello/main.vib", formatted)
        .expect("formatted source selects source grammar");
    assert!(!document.recovered());
}

#[test]
fn fmt_write_reports_checker_errors_and_preserves_ill_typed_source() {
    let root = TempDir::new("fmt-ill-typed");
    let workspace_path = root.path().join("hello");
    fs::create_dir(&workspace_path).expect("create hello workspace");
    let workspace = workspace_path.to_string_lossy().into_owned();
    let init = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "project",
        "init",
    ]);
    assert_eq!(init.status.code(), Some(0));

    let source_path = workspace_path.join("src/hello/main.vib");
    let original = "(defn main () str  1i32)\n";
    fs::write(&source_path, original).expect("write ill-typed source");
    let output = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "fmt",
        "src/hello/main.vib",
        "--write",
    ]);

    assert_eq!(output.status.code(), Some(1));
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["result"], "@command.diagnostics");
    assert_eq!(envelope["payload"]["written"], false);
    assert!(!envelope["diagnostics"].as_array().unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(source_path).expect("read source after fmt"),
        original
    );
}

#[test]
fn fmt_selects_vibon_from_the_exact_extension_and_writes_only_with_flag() {
    let root = TempDir::new("fmt-vibon");
    let workspace_path = root.path().join("hello");
    fs::create_dir(&workspace_path).expect("create hello workspace");
    let workspace = workspace_path.to_string_lossy().into_owned();
    let init = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "project",
        "init",
    ]);
    assert_eq!(init.status.code(), Some(0));
    let project_path = workspace_path.join("project.vibon");
    let canonical = fs::read_to_string(&project_path).expect("read generated project");
    let unformatted = format!("\n\n{canonical}");
    fs::write(&project_path, &unformatted).expect("write unformatted project");

    let preview = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "fmt",
        "project.vibon",
    ]);
    assert_eq!(preview.status.code(), Some(0));
    let envelope = json(&preview);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["payload"]["changed"], true);
    assert_eq!(envelope["payload"]["written"], false);
    assert_eq!(
        fs::read_to_string(&project_path).expect("read after preview"),
        unformatted
    );
    let formatted = envelope["payload"]["text"]
        .as_str()
        .expect("preview contains canonical VIBON");

    let write = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "fmt",
        "project.vibon",
        "--write",
    ]);
    assert_eq!(write.status.code(), Some(0));
    let envelope = json(&write);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["payload"]["written"], true);
    assert_eq!(
        fs::read_to_string(&project_path).expect("read written VIBON"),
        formatted
    );
}

#[test]
fn process_exit_codes_and_json_envelopes_are_stable_for_invalid_input() {
    let root = TempDir::new("invalid");
    let workspace = root.path().to_string_lossy().into_owned();
    let output = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "fmt",
        "file.txt",
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["schemaVersion"], 1);
    assert_eq!(envelope["command"], "fmt");
    assert_eq!(envelope["result"], "@command.invalid-input");
    assert!(envelope["diagnostics"].is_array());
    assert!(envelope["payload"]["text"].is_null());
}

#[test]
fn step12_commands_run_while_the_later_test_command_remains_unavailable() {
    let root = TempDir::new("unavailable");
    fs::create_dir_all(root.path().join("src/app")).expect("create source root");
    fs::write(
        root.path().join("project.vibon"),
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array))) dependencies: (map))\n",
    )
    .expect("write project marker");
    fs::write(
        root.path().join("src/app/main.vib"),
        "(defn execute () void (do))\n",
    )
    .expect("write entry source");
    let workspace = root.path().to_string_lossy().into_owned();
    let check = run(&["--format", "json", "--workspace", &workspace, "check"]);

    assert_eq!(check.status.code(), Some(0));
    let check_envelope = json(&check);
    assert_envelope_schema(&check_envelope);
    assert_eq!(check_envelope["command"], "check");
    assert_eq!(check_envelope["result"], "@command.ok");
    assert_eq!(check_envelope["payload"]["accepted"], true);
    assert_eq!(check_envelope["diagnostics"], serde_json::json!([]));

    let run_output = run(&[
        "--format",
        "json",
        "--workspace",
        &workspace,
        "run",
        "src/app",
    ]);
    assert_eq!(run_output.status.code(), Some(0));
    let run_envelope = json(&run_output);
    assert_envelope_schema(&run_envelope);
    assert_eq!(run_envelope["command"], "run");
    assert_eq!(run_envelope["result"], "@command.ok");

    let test = run(&["--format", "json", "--workspace", &workspace, "test"]);
    assert_eq!(test.status.code(), Some(4));
    let test_envelope = json(&test);
    assert_envelope_schema(&test_envelope);
    assert_eq!(test_envelope["command"], "test");
    assert_eq!(test_envelope["result"], "@command.unavailable");
    assert_eq!(test_envelope["diagnostics"][0]["code"], "@tool.unavailable");
}

#[test]
fn valid_or_unavailable_commands_validate_the_frozen_argument_grammar_first() {
    let root = TempDir::new("unavailable-grammar");
    fs::create_dir_all(root.path().join("src/app")).expect("create source root");
    fs::write(
        root.path().join("project.vibon"),
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array))) dependencies: (map))\n",
    )
    .expect("write project marker");
    fs::write(
        root.path().join("src/app/main.vib"),
        "(defn execute () void (do))\n",
    )
    .expect("write entry source");
    let workspace = root.path().to_string_lossy().into_owned();
    let invalid: &[&[&str]] = &[
        &["check", "one", "two"],
        &["check", "--all"],
        &["check", "../outside"],
        &["run"],
        &["run", "app", "extra"],
        &["run", "../outside"],
        &["test", "module.case", "extra"],
        &["test", "--all"],
        &["test", "../outside"],
    ];
    for arguments in invalid {
        let mut command = vec!["--format", "json", "--workspace", workspace.as_str()];
        command.extend_from_slice(arguments);
        let output = run(&command);

        assert_eq!(output.status.code(), Some(2), "arguments: {arguments:?}");
        let envelope = json(&output);
        assert_envelope_schema(&envelope);
        assert_eq!(envelope["command"], arguments[0]);
        assert_eq!(envelope["result"], "@command.invalid-input");
    }

    for command_name in ["check", "run", "test"] {
        let mut command = vec!["--format", "json", "--workspace", workspace.as_str()];
        command.push(command_name);
        command.push(workspace.as_str());
        let output = run(&command);

        assert_eq!(
            output.status.code(),
            Some(2),
            "absolute {command_name} argument"
        );
        let envelope = json(&output);
        assert_envelope_schema(&envelope);
        assert_eq!(envelope["command"], command_name);
        assert_eq!(envelope["result"], "@command.invalid-input");
    }

    let valid: &[&[&str]] = &[
        &["check"],
        &["check", "src/app"],
        &["run", "src/app"],
        &["test"],
        &["test", "app.main.case"],
    ];
    for arguments in valid {
        let mut command = vec!["--format", "json", "--workspace", workspace.as_str()];
        command.extend_from_slice(arguments);
        let output = run(&command);

        let envelope = json(&output);
        assert_envelope_schema(&envelope);
        assert_eq!(envelope["command"], arguments[0]);
        if arguments[0] == "test" {
            assert_eq!(output.status.code(), Some(4), "arguments: {arguments:?}");
            assert_eq!(envelope["result"], "@command.unavailable");
            assert_eq!(envelope["diagnostics"][0]["code"], "@tool.unavailable");
        } else {
            assert_eq!(output.status.code(), Some(0), "arguments: {arguments:?}");
            assert_eq!(envelope["result"], "@command.ok");
            assert_eq!(envelope["diagnostics"], serde_json::json!([]));
        }
    }
}
