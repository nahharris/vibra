#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

//! Actual-binary M2 demo evidence. Repeat offline with:
//! `cargo test --locked --offline -p vibra-cli --test process_step14`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use jsonschema::Validator;
use serde_json::Value;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempHello {
    parent: PathBuf,
    root: PathBuf,
}

impl TempHello {
    fn new(label: &str) -> Self {
        for _ in 0..1024 {
            let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let parent = std::env::temp_dir().join(format!(
                "vibra-step14-{label}-{}-{nonce}",
                std::process::id()
            ));
            match fs::create_dir(&parent) {
                Ok(()) => {
                    let root = parent.join("hello");
                    if let Err(error) = fs::create_dir(&root) {
                        let _ = fs::remove_dir(&parent);
                        panic!("create a fresh hello workspace: {error}");
                    }
                    return Self { parent, root };
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create a unique temporary parent: {error}"),
            }
        }
        panic!("could not create a unique temporary hello workspace");
    }

    fn write(&self, relative: &str, content: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().expect("fixture file has a parent"))
            .expect("create fixture directory");
        fs::write(path, content).expect("write fixture file");
    }
}

impl Drop for TempHello {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.parent);
    }
}

fn run(root: &Path, arguments: &[&str]) -> Output {
    command(root, arguments)
        .output()
        .expect("run the actual vibra binary")
}

fn command(root: &Path, arguments: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vibra"));
    command
        .current_dir(root)
        .args(["--format", "json"])
        .args(arguments);
    command
}

fn run_with_timeout(root: &Path, arguments: &[&str], timeout: Duration) -> Output {
    let mut child = command(root, arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start the actual vibra binary");
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .expect("collect completed command output");
            }
            Ok(None) if started.elapsed() < timeout => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait_with_output();
                panic!(
                    "run started a nonterminating program before reporting diagnostics"
                );
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait_with_output();
                panic!("wait for the actual vibra binary: {error}");
            }
        }
    }
}

fn envelope(output: &Output, exit: i32, command: &str, result: &str) -> Value {
    assert_eq!(output.status.code(), Some(exit), "{output:?}");

    let mut documents =
        serde_json::Deserializer::from_slice(&output.stdout).into_iter::<Value>();
    let document = documents
        .next()
        .expect("one JSON envelope on stdout")
        .expect("stdout contains valid JSON");
    assert!(
        documents.next().is_none(),
        "stdout has one JSON envelope only"
    );

    let schema: Value = serde_json::from_str(vibra_schema::COMMAND_RESULT_SCHEMA)
        .expect("checked-in command-result schema is valid JSON");
    let validator = Validator::new(&schema).expect("command-result schema compiles");
    assert!(
        validator.is_valid(&document),
        "envelope violates schema: {}",
        serde_json::to_string_pretty(&document).expect("envelope serializes")
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr_lines = stderr.lines().collect::<Vec<_>>();
    let mut next_line = 0;
    for diagnostic in document["diagnostics"]
        .as_array()
        .expect("diagnostics is an array")
        .iter()
    {
        let expected = format!(
            "{}: {}",
            diagnostic["code"]
                .as_str()
                .expect("diagnostic code is a string"),
            diagnostic["message"]
                .as_str()
                .expect("diagnostic message is a string")
        );
        let position = stderr_lines
            .get(next_line..)
            .and_then(|lines| lines.iter().position(|line| *line == expected))
            .unwrap_or_else(|| {
                panic!(
                    "JSON-mode stderr is missing diagnostic `{expected}` in order: {stderr}"
                )
            });
        next_line += position + 1;
    }
    assert_eq!(document["command"], command);
    assert_eq!(document["result"], result);
    document
}

fn binary_project(dependencies: &str) -> String {
    format!(
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @hello kind: @bin root: \"src/hello\" entry: @hello.main.main effects: (array))) dependencies: {dependencies})\n"
    )
}

fn assert_nonexecuting_run(
    workspace: &TempHello,
    expected_result: &str,
    expected_exit: i32,
    diagnostic_code: &str,
) {
    let output = run(&workspace.root, &["run", "src/hello"]);
    assert_nonexecuting_output(
        &output,
        expected_result,
        expected_exit,
        diagnostic_code,
    );
}

fn assert_nonexecuting_output(
    output: &Output,
    expected_result: &str,
    expected_exit: i32,
    diagnostic_code: &str,
) {
    let document = envelope(output, expected_exit, "run", expected_result);
    assert!(
        document["diagnostics"]
            .as_array()
            .expect("diagnostics is an array")
            .iter()
            .any(|diagnostic| diagnostic["code"] == diagnostic_code),
        "expected {diagnostic_code}: {}",
        document["diagnostics"]
    );
    assert_eq!(document["payload"]["programResult"], Value::Null);
    assert_eq!(document["payload"]["stdout"], "");
    assert_eq!(document["payload"]["stderr"], "");
    assert_eq!(document["payload"]["auditTrace"], serde_json::json!([]));
    assert_eq!(document["payload"]["trap"], Value::Null);
}

#[test]
fn actual_binary_positive_demo_repeats_in_two_fresh_hello_workspaces() {
    for repetition in 0..2 {
        let workspace = TempHello::new(&format!("positive-{repetition}"));

        let initialized = run(&workspace.root, &["project", "init"]);
        let init_document = envelope(&initialized, 0, "init", "@command.ok");
        assert_eq!(
            init_document["payload"]["created"],
            serde_json::json!([
                "project.vibon",
                "src",
                "src/hello",
                "src/hello/main.vib",
                "tests"
            ])
        );

        workspace.write(
            "src/hello/helper.vib",
            "(defn answer () i32 visibility: @public\n  42i32)\n",
        );
        workspace.write(
            "src/hello/main.vib",
            "(import helper @hello.helper)\n(defn main () void (let - (helper.answer) (do)))\n",
        );

        let preview = run(&workspace.root, &["fmt", "src/hello/main.vib"]);
        let preview_document = envelope(&preview, 0, "fmt", "@command.ok");
        assert!(
            preview_document["payload"]["changed"]
                .as_bool()
                .unwrap_or(false)
        );
        assert!(
            !preview_document["payload"]["written"]
                .as_bool()
                .unwrap_or(true)
        );
        let preview_text = preview_document["payload"]["text"]
            .as_str()
            .expect("preview returns formatted source text");
        let main_path = workspace.root.join("src/hello/main.vib");
        let unformatted = "(import helper @hello.helper)\n(defn main () void (let - (helper.answer) (do)))\n";
        assert_eq!(
            fs::read(&main_path).expect("read unmodified input"),
            unformatted.as_bytes()
        );

        let applied = run(&workspace.root, &["fmt", "src/hello/main.vib", "--write"]);
        let applied_document = envelope(&applied, 0, "fmt", "@command.ok");
        assert!(
            applied_document["payload"]["changed"]
                .as_bool()
                .unwrap_or(false)
        );
        assert!(
            applied_document["payload"]["written"]
                .as_bool()
                .unwrap_or(false)
        );
        assert_eq!(applied_document["payload"]["text"], Value::Null);
        assert_eq!(
            fs::read(&main_path).expect("read formatted source"),
            preview_text.as_bytes(),
            "--write installs exactly the text returned by preview"
        );

        workspace.write(
            "tests/math.vib",
            "(import assert @std.assert)\n(import helper @hello.helper)\n(test \"answer\" (assert.equal-i32 (helper.answer) 42i32))\n",
        );

        let checked = run(&workspace.root, &["check", "src/hello"]);
        let check_document = envelope(&checked, 0, "check", "@command.ok");
        assert!(
            check_document["payload"]["accepted"]
                .as_bool()
                .unwrap_or(false)
        );
        assert_eq!(check_document["diagnostics"], serde_json::json!([]));

        let executed = run(&workspace.root, &["run", "src/hello"]);
        let run_document = envelope(&executed, 0, "run", "@command.ok");
        assert_eq!(run_document["payload"]["target"], "src/hello");
        assert_eq!(
            run_document["payload"]["programResult"],
            "(record type: @void value: void)\n"
        );
        assert_eq!(run_document["payload"]["stdout"], "");
        assert_eq!(run_document["payload"]["stderr"], "");
        assert_eq!(run_document["payload"]["auditTrace"], serde_json::json!([]));
        assert_eq!(run_document["payload"]["trap"], Value::Null);

        let tested = run(&workspace.root, &["test", "@tests.math::\"answer\""]);
        let test_document = envelope(&tested, 0, "test", "@command.ok");
        assert_eq!(test_document["diagnostics"], serde_json::json!([]));
        assert_eq!(test_document["payload"]["selected"], 1);
        assert_eq!(test_document["payload"]["passed"], 1);
        assert_eq!(test_document["payload"]["failed"], 0);
        assert_eq!(
            test_document["payload"]["tests"][0]["result"],
            "@test.passed"
        );
        assert_eq!(
            test_document["payload"]["tests"][0]["auditTrace"],
            serde_json::json!([])
        );
    }
}

#[test]
fn actual_binary_run_preflight_failures_never_produce_a_program_result() {
    let mismatch = TempHello::new("negative-mismatch");
    mismatch.write("project.vibon", &binary_project("(map)"));
    mismatch.write(
        "src/hello/main.vib",
        "(defn spin () void (spin))\n(defn value (number i32) i32 number)\n(defn main () void (let - (spin) (let - (value true) (do))))\n",
    );
    let mismatch_output = run_with_timeout(
        &mismatch.root,
        &["run", "src/hello"],
        Duration::from_secs(10),
    );
    assert_nonexecuting_output(
        &mismatch_output,
        "@command.diagnostics",
        1,
        "@type.argument-mismatch",
    );

    let private = TempHello::new("negative-private");
    private.write("project.vibon", &binary_project("(map)"));
    private.write(
        "src/hello/main.vib",
        "(import secret @hello.secret)\n(defn main () void (let - (secret.answer) (do)))\n",
    );
    private.write("src/hello/secret.vib", "(defn answer () i32 42i32)\n");
    assert_nonexecuting_run(
        &private,
        "@command.diagnostics",
        1,
        "@name.private-access",
    );

    let host = TempHello::new("negative-host");
    host.write("project.vibon", &binary_project("(map)"));
    host.write(
        "src/hello/main.vib",
        "(deffect audit (defn record (message str) void external: @host symbol: \"audit.record\"))\n(defn main () void (do))\n",
    );
    assert_nonexecuting_run(&host, "@command.unavailable", 4, "@tool.unavailable");

    let dependency = TempHello::new("negative-path-dependency");
    dependency.write(
        "project.vibon",
        &binary_project(
            "(map @remote (record kind: @path path: \"../outside\" target: @core))",
        ),
    );
    dependency.write("src/hello/main.vib", "(defn main () void (do))\n");
    assert_nonexecuting_run(
        &dependency,
        "@command.unavailable",
        4,
        "@tool.unavailable",
    );
}

#[test]
fn actual_binary_failing_assertion_is_a_structured_test_failure() {
    let workspace = TempHello::new("negative-assertion");
    workspace.write("project.vibon", &binary_project("(map)"));
    workspace.write("src/hello/main.vib", "(defn main () void (do))\n");
    workspace.write(
        "tests/math.vib",
        "(import assert @std.assert)\n(test \"fails\" (assert.false true))\n",
    );

    let output = run(&workspace.root, &["test", "@tests.math::\"fails\""]);
    let document = envelope(&output, 1, "test", "@command.test-failed");
    assert_eq!(document["payload"]["selected"], 1);
    assert_eq!(document["payload"]["passed"], 0);
    assert_eq!(document["payload"]["failed"], 1);
    let item = &document["payload"]["tests"][0];
    assert_eq!(item["result"], "@test.assertion-failed");
    assert_eq!(item["failure"]["assertion"], "@std.assert.false");
    assert_eq!(item["auditTrace"], serde_json::json!([]));
    assert_eq!(item["trap"], Value::Null);
}
