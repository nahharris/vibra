//! `vibra check [TARGET]` through the command line.

use serde_json::{Value, json};

use crate::support::{LIBRARY, TempDir, envelope_in};

const VALID: &str = "fn answer returns r: I32 do return 42i32 endfn";
const MISMATCH: &str =
    "fn answer returns r: I32 do let x: I32 be \"a\" endlet return 42i32 endfn";

const BINARY_AND_LIBRARY: &str = "{format: @project.v2 package: {name: \"hello\" version: \"0.1.0\"} targets: [{name: @app kind: @lib root: \"src/app\"} {name: @hello kind: @bin root: \"src/hello\" entry: @hello.main.main effects: [] grants: [:]}] dependencies: [:]}";

fn check(root: &TempDir, arguments: &[&str]) -> (i32, Value) {
    let mut line = vec!["--format", "json", "check"];
    line.extend_from_slice(arguments);
    envelope_in(root.path(), &line)
}

fn codes(document: &Value) -> Vec<String> {
    document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().expect("code").to_owned())
        .collect()
}

#[test]
fn check_passed() {
    let root =
        TempDir::project("check-passed", LIBRARY, &[("src/app/main.vib", VALID)]);
    let (code, document) = check(&root, &[]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["command"], "check");
    assert_eq!(document["status"], "passed");
    assert_eq!(document["result"], "@command.ok");
    assert_eq!(
        document["payload"],
        json!({"accepted": true, "obligations": []})
    );
    assert_eq!(document["diagnostics"], json!([]));
    assert_eq!(
        document["limits"],
        json!({"profile": "m1Default", "reached": [], "complete": true})
    );
    let revision = document["workspaceRevision"].as_str().expect("a revision");
    assert!(revision.starts_with("sha256:") && revision.len() == 71);
    let (code, named) = check(&root, &["@app"]);
    assert_eq!(code, 0);
    assert_eq!(named["payload"]["accepted"], true);
}

#[test]
fn check_diagnostics() {
    let root = TempDir::project(
        "check-diagnostics",
        LIBRARY,
        &[("src/app/main.vib", MISMATCH)],
    );
    let (code, document) = check(&root, &[]);
    assert_eq!(code, 1);
    assert_eq!(document["result"], "@command.diagnostics");
    assert_eq!(document["status"], "refuted");
    assert_eq!(
        document["payload"],
        json!({"accepted": false, "obligations": []})
    );
    assert_eq!(codes(&document), ["@type.mismatch"]);
    // `"a"` at bytes 42..45 of the module, all on line 1.
    assert_eq!(
        document["diagnostics"][0]["primarySpan"],
        json!({"sourceId": "src/app/main.vib", "start": 42, "end": 45,
            "startPosition": {"line": 1, "column": 43},
            "endPosition": {"line": 1, "column": 46}})
    );
}

#[test]
fn check_unknown_target() {
    let root =
        TempDir::project("check-unknown", LIBRARY, &[("src/app/main.vib", VALID)]);
    let (code, document) = check(&root, &["@nope"]);
    assert_eq!(code, 2);
    assert_eq!(document["result"], "@command.invalidInput");
    assert_eq!(document["diagnostics"], json!([]));
    assert_eq!(document["payload"], json!({}));
}

#[test]
fn check_target_is_path() {
    let root = TempDir::project("check-path", LIBRARY, &[("src/app/main.vib", VALID)]);
    let (code, document) = check(&root, &["src/app"]);
    assert_eq!(code, 2);
    assert_eq!(document["result"], "@command.invalidInput");
    assert_eq!(document["workspaceRevision"], json!(null));
}

#[test]
fn check_no_project() {
    let root = TempDir::new("check-no-project");
    let (code, document) = check(&root, &[]);
    assert_eq!(code, 1);
    assert_eq!(document["result"], "@command.diagnostics");
    assert_eq!(codes(&document), ["@project.notFound"]);
    assert_eq!(document["workspaceRevision"], json!(null));
    assert_eq!(document["payload"]["accepted"], false);
}

#[test]
fn check_unsupported() {
    let source = "fn answer returns r: I32 do return 42i32 endfn\n";
    // A test that takes a world: 51 bytes of imports and a type before it.
    let tests = "use @std.assert enduse\n\ntype World of I32 endtype\n\n\
                 test \"world\" takes {world: World} do endtest\n";
    let root = TempDir::project(
        "check-world",
        LIBRARY,
        &[("src/app/main.vib", source), ("tests/main.vib", tests)],
    );
    let (code, document) = check(&root, &[]);
    assert_eq!(code, 4, "{document}");
    assert_eq!(document["result"], "@command.unavailable");
    assert_eq!(document["status"], "unsupported");
    assert_eq!(codes(&document), ["@tool.unavailable"]);
    let span = &document["diagnostics"][0]["primarySpan"];
    assert_eq!(span["sourceId"], "tests/main.vib");
    // The whole test, until step 21.
    assert_eq!(span["start"], 51);
    assert_eq!(span["end"], 95);
    assert_eq!(document["payload"]["accepted"], false);
}
#[test]
fn check_bin_target_unsupported() {
    let root = TempDir::project(
        "check-bin",
        BINARY_AND_LIBRARY,
        &[("src/app/main.vib", VALID), ("src/hello/main.vib", VALID)],
    );
    let (code, document) = check(&root, &[]);
    assert_eq!(code, 4, "{document}");
    assert_eq!(document["status"], "unsupported");
    assert_eq!(codes(&document), ["@tool.unavailable"]);
    let span = &document["diagnostics"][0]["primarySpan"];
    assert_eq!(span["sourceId"], "project.vibon");
    let record = "{name: @hello kind: @bin root: \"src/hello\" entry: @hello.main.main effects: [] grants: [:]}";
    let start = BINARY_AND_LIBRARY.find(record).expect("the record");
    assert_eq!(span["start"], start);
    assert_eq!(span["end"], start + record.len());
}

#[test]
fn check_covers_tests() {
    // The one fault is in the body of a `test`: `"a"` at bytes 50..53, on
    // line 3 from column 27.
    let test =
        "use @std.assert enduse\n\ntest \"a\" do let x: I32 be \"a\" endlet endtest";
    let root = TempDir::project(
        "check-tests",
        LIBRARY,
        &[("src/app/main.vib", VALID), ("tests/a.vib", test)],
    );
    let (code, document) = check(&root, &[]);
    assert_eq!(code, 1, "{document}");
    assert_eq!(codes(&document), ["@type.mismatch"]);
    assert_eq!(
        document["diagnostics"][0]["primarySpan"],
        json!({"sourceId": "tests/a.vib", "start": 50, "end": 53,
            "startPosition": {"line": 3, "column": 27},
            "endPosition": {"line": 3, "column": 30}})
    );
    let (code, named) = check(&root, &["@app"]);
    assert_eq!(code, 0, "{named}");
    assert_eq!(named["diagnostics"], json!([]));
}

#[test]
fn check_revision_stable() {
    let root =
        TempDir::project("check-revision", LIBRARY, &[("src/app/main.vib", VALID)]);
    let (_, first) = check(&root, &[]);
    let (_, second) = check(&root, &[]);
    assert_eq!(first["workspaceRevision"], second["workspaceRevision"]);
    root.write(
        "src/app/main.vib",
        "fn answer returns r: I32 do return 43i32 endfn",
    );
    let (_, third) = check(&root, &[]);
    assert_ne!(first["workspaceRevision"], third["workspaceRevision"]);
}

#[test]
fn check_json_validates() {
    // Every test of this file and of `fmt.rs` reads its envelope through
    // `envelope_in`, which validates it against the published schema. This
    // test runs each shape once more in one place.
    let root =
        TempDir::project("check-json", LIBRARY, &[("src/app/main.vib", MISMATCH)]);
    let empty = TempDir::new("check-json-empty");
    for (directory, line) in [
        (&root, &["--format", "json", "check"][..]),
        (&root, &["--format", "json", "check", "@app"]),
        (&root, &["--format", "json", "check", "@nope"]),
        (&root, &["--format", "json", "check", "src/app"]),
        (&empty, &["--format", "json", "check"]),
        (&root, &["--format", "json", "fmt", "src/app/main.vib"]),
        (
            &root,
            &["--format", "json", "fmt", "src/app/main.vib", "--write"],
        ),
        (&root, &["--format", "json", "fmt", "src/app/none.vib"]),
        (&root, &["--format", "json", "fmt", "../a.vib"]),
        (&root, &["--format", "json", "fmt", "README.md"]),
        (&root, &["--format", "json", "fmt", "project.vibon"]),
    ] {
        let (code, document) = envelope_in(directory.path(), line);
        assert!(
            vibra_schema::validate_command_result(&document).is_ok() && code >= 0,
            "{line:?}"
        );
    }
}

const POS: &str = "fn pos takes {x: I32} returns r: I32 requires Ordered.greater(x 0i32) do return x endfn\n";

/// The status, kind, and rules of each obligation the payload carries.
fn obligations(document: &Value) -> Vec<(String, String, Vec<String>)> {
    document["payload"]["obligations"]
        .as_array()
        .expect("obligations")
        .iter()
        .map(|record| {
            let text =
                |field: &str| record[field].as_str().expect("a string").to_owned();
            let rules = record["ruleIds"]
                .as_array()
                .expect("rules")
                .iter()
                .map(|rule| rule.as_str().expect("a rule").to_owned())
                .collect();
            (text("status"), text("kind"), rules)
        })
        .collect()
}

#[test]
fn check_sends_the_obligations_and_an_unestablished_one_is_unknown() {
    let passed = format!("{POS}fn main returns r: I32 do return pos(5i32) endfn\n");
    let root = TempDir::project(
        "check-obligation-passed",
        LIBRARY,
        &[("src/app/main.vib", passed.as_str())],
    );
    let (code, document) = check(&root, &[]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["status"], "passed");
    assert_eq!(
        obligations(&document),
        [(
            "passed".to_owned(),
            "requires".to_owned(),
            vec!["GUARD-1".to_owned()]
        )]
    );
    let unproved =
        format!("{POS}fn f takes {{y: I32}} returns r: I32 do return pos(y) endfn\n");
    let root = TempDir::project(
        "check-obligation-unknown",
        LIBRARY,
        &[("src/app/main.vib", unproved.as_str())],
    );
    let (code, document) = check(&root, &[]);
    assert_eq!(code, 1, "{document}");
    assert_eq!(document["result"], "@command.diagnostics");
    assert_eq!(document["status"], "unknown");
    assert_eq!(codes(&document), ["@contract.unproved"]);
    assert_eq!(
        obligations(&document),
        [("unknown".to_owned(), "requires".to_owned(), Vec::new())]
    );
    let source = &document["payload"]["obligations"][0]["source"];
    assert_eq!(source["sourceId"], "src/app/main.vib");
}

/// The example project of the repository, `examples/hello`.
fn hello_example() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello")
}

#[test]
fn examples_check() {
    let (code, document) =
        envelope_in(&hello_example(), &["--format", "json", "check"]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["status"], "passed");
    assert_eq!(document["result"], "@command.ok");
    assert_eq!(document["diagnostics"], json!([]));
    let (code, tests) = envelope_in(&hello_example(), &["--format", "json", "test"]);
    assert_eq!(code, 0, "{tests}");
    assert_eq!(tests["status"], "passed");
}

#[test]
fn the_readme_shows_the_checked_example() {
    let readme = std::fs::read_to_string(hello_example().join("../../README.md"))
        .expect("the repository README");
    for file in [
        "src/hello/greeting.vib",
        "tests/greeting.vib",
        "project.vibon",
    ] {
        let text = std::fs::read_to_string(hello_example().join(file))
            .expect("an example file");
        assert!(
            readme.contains(&text),
            "README.md shows `examples/hello/{file}` as it is"
        );
    }
}
