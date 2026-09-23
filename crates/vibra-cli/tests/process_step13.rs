#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

//! Process-level command contract tests for M2 Step 13.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use jsonschema::Validator;
use serde_json::Value;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempProject(PathBuf);

impl TempProject {
    fn new(label: &str, tests: &[(&str, &str)]) -> Self {
        let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "vibra-cli-step13-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src/app")).expect("create target source root");
        fs::write(
            root.join("project.vibon"),
            "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array))) dependencies: (map))\n",
        )
        .expect("write project marker");
        fs::write(
            root.join("src/app/main.vib"),
            "(defn execute () void void)\n",
        )
        .expect("write target source");
        for (relative, source) in tests {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().expect("test source parent"))
                .expect("create test module directory");
            fs::write(path, source).expect("write test source");
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

fn run(project: &TempProject, arguments: &[&str]) -> Output {
    let workspace = project.path().to_string_lossy().into_owned();
    Command::new(env!("CARGO_BIN_EXE_vibra"))
        .args(["--format", "json", "--workspace", &workspace, "test"])
        .args(arguments)
        .output()
        .expect("run the built vibra binary")
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
fn test_runs_a_passing_test_and_emits_the_closed_json_item() {
    let project = TempProject::new(
        "passing",
        &[(
            "tests/math.vib",
            "(import assert @std.assert)\n(test \"works\" (assert.true true))\n",
        )],
    );

    let output = run(&project, &[]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty());
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["command"], "test");
    assert_eq!(envelope["result"], "@command.ok");
    assert_eq!(envelope["diagnostics"], serde_json::json!([]));
    assert_eq!(envelope["payload"]["selected"], 1);
    assert_eq!(envelope["payload"]["passed"], 1);
    assert_eq!(envelope["payload"]["failed"], 0);
    assert_eq!(
        envelope["payload"]["tests"][0],
        serde_json::json!({
            "name": "@tests.math::\"works\"",
            "result": "@test.passed",
            "failure": null,
            "trap": null,
            "auditTrace": [],
            "diagnostics": []
        })
    );
}

#[test]
fn failed_assertion_is_a_structured_test_failure_and_exit_one() {
    let project = TempProject::new(
        "assertion-failure",
        &[(
            "tests/math.vib",
            "(import assert @std.assert)\n(test \"fails\" (assert.equal-i32 4i32 5i32))\n",
        )],
    );

    let output = run(&project, &[]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["result"], "@command.test-failed");
    assert_eq!(envelope["payload"]["passed"], 0);
    assert_eq!(envelope["payload"]["failed"], 1);
    let item = &envelope["payload"]["tests"][0];
    assert_eq!(item["result"], "@test.assertion-failed");
    assert_eq!(item["failure"]["assertion"], "@std.assert.equal-i32");
    assert_eq!(item["failure"]["expected"], "4i32");
    assert_eq!(item["failure"]["actual"], "5i32");
    assert_eq!(item["failure"]["primarySpan"]["sourceId"], "tests/math.vib");
    assert_eq!(item["trap"], Value::Null);
    assert_eq!(item["auditTrace"], serde_json::json!([]));
}

#[test]
fn missing_assertion_import_is_static_diagnostics_not_unavailable() {
    let project = TempProject::new(
        "missing-import",
        &[("tests/math.vib", "(test \"missing-import\" void)\n")],
    );

    let output = run(&project, &[]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["result"], "@command.diagnostics");
    assert_eq!(envelope["payload"]["tests"][0]["result"], "@test.invalid");
    assert!(
        envelope["diagnostics"]
            .as_array()
            .is_some_and(|diagnostics| {
                diagnostics.iter().any(|diagnostic| {
                    diagnostic["code"] == "@module.missing-required-import"
                })
            })
    );
}

#[test]
fn unavailable_assertion_member_stays_distinct_from_failure() {
    let project = TempProject::new(
        "unavailable-assertion",
        &[(
            "tests/math.vib",
            "(import assert @std.assert)\n(test \"generic\" (assert.equal 1i32 1i32))\n",
        )],
    );

    let output = run(&project, &[]);

    assert_eq!(output.status.code(), Some(4), "{output:?}");
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["result"], "@command.unavailable");
    assert_eq!(
        envelope["payload"]["tests"][0]["result"],
        "@test.unavailable"
    );
}

#[test]
fn selector_must_be_canonical_and_unknown_selector_is_invalid_input() {
    let project = TempProject::new(
        "selector",
        &[(
            "tests/math.vib",
            "(import assert @std.assert)\n(test \"works\" (assert.true true))\n",
        )],
    );

    for selector in ["@tests.math::works", "@tests.math::\"missing\""] {
        let output = run(&project, &[selector]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "selector {selector}: {output:?}"
        );
        let envelope = json(&output);
        assert_envelope_schema(&envelope);
        assert_eq!(envelope["result"], "@command.invalid-input");
        assert_eq!(envelope["diagnostics"], serde_json::json!([]));
    }
}

#[test]
fn canonical_selector_runs_only_its_exact_test() {
    let project = TempProject::new(
        "selector-exact",
        &[(
            "tests/math.vib",
            "(import assert @std.assert)\n(test \"selected\" (assert.true true))\n(test \"not-selected\" (assert.false true))\n",
        )],
    );

    let output = run(&project, &["@tests.math::\"selected\""]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["result"], "@command.ok");
    assert_eq!(envelope["payload"]["selected"], 1);
    assert_eq!(envelope["payload"]["passed"], 1);
    assert_eq!(
        envelope["payload"]["tests"][0]["name"],
        "@tests.math::\"selected\""
    );
}

#[test]
fn project_diagnostics_precede_an_unknown_selector_lookup() {
    let project = TempProject::new("selector-precedence", &[]);
    fs::write(
        project.path().join("project.vibon"),
        "(record format: @project.v1 package: (record name: \"demo\") targets: (array) dependencies: (map))\n",
    )
    .expect("write a project with a schema diagnostic");

    let output = run(&project, &["@tests.math::\"missing\""]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["result"], "@command.diagnostics");
    assert_eq!(envelope["payload"]["selected"], 0);
    assert_eq!(envelope["payload"]["tests"], serde_json::json!([]));
    assert!(
        !envelope["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .is_empty()
    );
}

#[test]
fn discovered_modules_and_tests_use_canonical_process_order() {
    let project = TempProject::new(
        "deterministic-order",
        &[
            (
                "tests/z.vib",
                "(import assert @std.assert)\n(test \"z\" (assert.true true))\n",
            ),
            (
                "tests/a.vib",
                "(import assert @std.assert)\n(test \"a-first\" (assert.true true))\n(test \"a-second\" (assert.true true))\n",
            ),
        ],
    );

    let output = run(&project, &[]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    let names = envelope["payload"]["tests"]
        .as_array()
        .expect("test records")
        .iter()
        .map(|item| item["name"].as_str().expect("canonical selector"))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "@tests.a::\"a-first\"",
            "@tests.a::\"a-second\"",
            "@tests.z::\"z\"",
        ]
    );
}

#[test]
fn omitted_selector_accepts_an_empty_test_suite() {
    let project = TempProject::new("empty-suite", &[]);

    let output = run(&project, &[]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["result"], "@command.ok");
    assert_eq!(
        envelope["payload"],
        serde_json::json!({
            "selected": 0,
            "passed": 0,
            "failed": 0,
            "tests": []
        })
    );
}

#[test]
fn warnings_are_attributed_to_passing_items_without_blocking_execution() {
    let project = TempProject::new(
        "warning-attribution",
        &[(
            "tests/math.vib",
            "(import assert @std.assert)\n(defn choose (fallback i32) i32 labelled: (first i32 7i32 second i32 8i32) first)\n(test \"warning-still-runs\" (assert.equal-i32 (choose 3i32 second: 11i32 first: 9i32) 9i32))\n",
        )],
    );

    let output = run(&project, &[]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["result"], "@command.ok");
    assert!(
        envelope["diagnostics"]
            .as_array()
            .is_some_and(|diagnostics| {
                diagnostics.iter().any(|diagnostic| {
                    diagnostic["code"] == "@style.argument-order"
                        && diagnostic["level"] == "@warning"
                })
            })
    );
    assert!(
        envelope["payload"]["tests"][0]["diagnostics"]
            .as_array()
            .is_some_and(|diagnostics| diagnostics.iter().any(|diagnostic| {
                diagnostic["code"] == "@style.argument-order"
                    && diagnostic["level"] == "@warning"
            }))
    );
    assert_eq!(
        envelope["payload"]["tests"][0]["auditTrace"],
        serde_json::json!([])
    );
}

#[test]
fn unavailability_takes_precedence_over_assertion_failure() {
    let project = TempProject::new(
        "unavailable-precedence",
        &[(
            "tests/math.vib",
            "(import assert @std.assert)\n(test \"fails\" (assert.false true))\n(test \"unavailable\" (assert.equal 1i32 1i32))\n",
        )],
    );

    let output = run(&project, &[]);

    assert_eq!(output.status.code(), Some(4), "{output:?}");
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    assert_eq!(envelope["result"], "@command.unavailable");
    assert_eq!(envelope["payload"]["selected"], 2);
    assert_eq!(envelope["payload"]["passed"], 0);
    assert_eq!(envelope["payload"]["failed"], 2);
    assert_eq!(
        envelope["payload"]["tests"][0]["result"],
        "@test.assertion-failed"
    );
    assert_eq!(
        envelope["payload"]["tests"][1]["result"],
        "@test.unavailable"
    );
}
