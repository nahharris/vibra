//! The host test of `@test.trap`, which needs the fault-injection hook
//! compiled only for this crate's tests.

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use super::TRAP_FIRST;

const PROJECT: &str = "{format: @project.v2 package: {name: \"hello\" version: \"0.1.0\"} \
                       targets: [{name: @app kind: @lib root: \"src/app\"}] dependencies: [:]}";

const TESTS: &str = "use @std.assert enduse\n\n\
                     test \"first\" do assert.true(true) endtest\n\n\
                     test \"second\" do assert.true(true) endtest\n";

/// A project tree in a fresh temporary directory, removed on drop.
struct Tree(PathBuf);

impl Tree {
    fn new(label: &str) -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .expect("temporary directory")
            .join(format!("vibra-cli-test-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (relative, text) in [
            ("project.vibon", PROJECT),
            ("src/app/main.vib", ""),
            ("tests/math.vib", TESTS),
        ] {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().expect("a parent")).expect("directory");
            fs::write(path, text).expect("fixture");
        }
        Self(root)
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(tree: &Tree, arguments: &[&str]) -> (i32, String) {
    let mut stdout = Vec::new();
    let code = crate::run(
        arguments.iter().map(OsString::from),
        &tree.0,
        &mut stdout,
        std::io::sink(),
    );
    (code, String::from_utf8(stdout).expect("UTF-8"))
}

#[test]
fn a_trapped_test_is_a_trap_record_and_the_suite_traps() {
    let tree = Tree::new("trap");
    TRAP_FIRST.set(true);
    let (code, output) = run(&tree, &["--format", "json", "test"]);
    let (human_code, human) = run(&tree, &["test"]);
    TRAP_FIRST.set(false);
    assert_eq!(code, 5, "{output}");
    let document: Value = serde_json::from_str(&output).expect("one JSON value");
    vibra_schema::validate_command_result(&document).expect("the envelope validates");
    assert_eq!(document["result"], "@command.trap");
    assert_eq!(document["status"], "unknown");
    let payload = &document["payload"];
    assert_eq!(
        (&payload["selected"], &payload["passed"], &payload["failed"]),
        (&Value::from(2), &Value::from(1), &Value::from(1))
    );
    let first = &payload["tests"][0];
    assert_eq!(first["name"], "@tests.math::\"first\"");
    assert_eq!(first["result"], "@test.trap");
    assert_eq!(first["status"], "unknown");
    assert_eq!(
        first["trap"],
        serde_json::json!({"trapCode": "@runtime.invalidCheckedProgram", "origin": null})
    );
    assert_eq!(first["failure"], Value::Null);
    assert_eq!(
        first["diagnostics"],
        serde_json::json!(["@runtime.invalidCheckedProgram"])
    );
    assert_eq!(payload["tests"][1]["result"], "@test.passed");
    let codes = document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|diagnostic| diagnostic["code"].clone())
        .collect::<Vec<_>>();
    assert_eq!(codes, ["@runtime.invalidCheckedProgram"]);
    assert_eq!(
        document["diagnostics"][0]["primarySpan"]["sourceId"],
        Value::Null
    );
    assert_eq!(human_code, 5);
    assert!(
        human.contains("@tests.math::\"first\" @test.trap"),
        "{human}"
    );
    assert!(
        human.contains("trap: @runtime.invalidCheckedProgram"),
        "{human}"
    );
    assert_eq!(
        human.lines().last(),
        Some("passed 1, failed 1, selected 2"),
        "{human}"
    );
}

#[test]
fn without_the_hook_the_same_suite_passes() {
    let tree = Tree::new("plain");
    let (code, output) = run(&tree, &["--format", "json", "test"]);
    assert_eq!(code, 0, "{output}");
}
