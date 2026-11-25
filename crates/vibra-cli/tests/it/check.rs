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
fn check_test_takes_another_type() {
    let source = "fn answer returns r: I32 do return 42i32 endfn\n";
    // A test that takes another type than `TestWorld`: 51 bytes of imports
    // and a type before it, and `takes {world: World}` 13 bytes into it.
    let tests = "use @std.assert enduse\n\ntype World of I32 endtype\n\n\
                 test \"world\" takes {world: World} do endtest\n";
    let root = TempDir::project(
        "check-world",
        LIBRARY,
        &[("src/app/main.vib", source), ("tests/main.vib", tests)],
    );
    let (code, document) = check(&root, &[]);
    assert_eq!(code, 1, "{document}");
    assert_eq!(document["result"], "@command.diagnostics");
    assert_eq!(codes(&document), ["@syntax.invalidForm"]);
    let span = &document["diagnostics"][0]["primarySpan"];
    assert_eq!(span["sourceId"], "tests/main.vib");
    assert_eq!(span["start"], 64);
    assert_eq!(span["end"], 84);
    assert_eq!(document["payload"]["accepted"], false);
}

#[test]
fn check_bin_target_passed() {
    // An entry that takes no grant needs no recipe, so its target checks.
    let entry = "use @std.process.ExitCode enduse\n\n\
                 proc main returns code: ExitCode do return ExitCode(0u8) endproc\n";
    let root = TempDir::project(
        "check-bin-passed",
        BINARY_AND_LIBRARY,
        &[("src/app/main.vib", VALID), ("src/hello/main.vib", entry)],
    );
    let (code, document) = check(&root, &[]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["status"], "passed");
    assert!(codes(&document).is_empty());
}

#[test]
fn check_bin_target_unsupported() {
    // The entry takes a grant leaf, and recipes are not checked yet: the
    // target is unavailable at its `grants` field.
    let entry = "use @std.io.StdoutText enduse\n\nuse @std.process.ExitCode enduse\n\n\
                 proc main takes {out: StdoutText} returns code: ExitCode do return ExitCode(0u8) endproc\n";
    let root = TempDir::project(
        "check-bin",
        BINARY_AND_LIBRARY,
        &[("src/app/main.vib", VALID), ("src/hello/main.vib", entry)],
    );
    let (code, document) = check(&root, &[]);
    assert_eq!(code, 4, "{document}");
    assert_eq!(document["status"], "unsupported");
    assert_eq!(codes(&document), ["@tool.unavailable"]);
    let span = &document["diagnostics"][0]["primarySpan"];
    assert_eq!(span["sourceId"], "project.vibon");
    let start = BINARY_AND_LIBRARY.find("grants: [:]").expect("the grants") + 8;
    assert_eq!(span["start"], start);
    assert_eq!(span["end"], start + "[:]".len());
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

/// The tree of the conformance case `group/id`, checked in place: `check`
/// reads it and writes nothing.
fn corpus_tree(group: &str, id: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/cases")
        .join(group)
        .join(id)
        .join("tree")
}

/// A span document's source, offsets, and display positions.
fn span_of(span: &Value) -> (String, u64, u64, (u64, u64), (u64, u64)) {
    let number = |value: &Value| value.as_u64().expect("a number");
    let position = |value: &Value| (number(&value["line"]), number(&value["column"]));
    (
        span["sourceId"].as_str().expect("a source").to_owned(),
        number(&span["start"]),
        number(&span["end"]),
        position(&span["startPosition"]),
        position(&span["endPosition"]),
    )
}

/// 07 "Diagnostic documents and spans": a related span may be in another
/// document, here the embedded `@std.fs`, whose source identity is its module
/// atom (04 "Discovery and the source snapshot"); its positions come from the
/// library module's text. Case `V2-GRANT-view-does-not-narrow`: the receiver
/// `dir.readText` at bytes 2352 to 2364 (106:39 to 106:51), related to the
/// name `DirWriteText` of its interface at bytes 5403 to 5415 of
/// `stdlib/src/std/fs.vib` (234:11 to 234:23).
#[test]
fn check_json_related_span_in_a_library_module() {
    let tree = corpus_tree("GRANT", "V2-GRANT-view-does-not-narrow");
    let (code, document) = envelope_in(&tree, &["--format", "json", "check"]);
    assert_eq!(code, 1, "{document}");
    assert_eq!(document["result"], "@command.diagnostics");
    assert_eq!(codes(&document), ["@type.unsatisfiedBound"]);
    let diagnostic = &document["diagnostics"][0];
    assert_eq!(
        span_of(&diagnostic["primarySpan"]),
        (
            "tests/grants.vib".to_owned(),
            2352,
            2364,
            (106, 39),
            (106, 51)
        )
    );
    let related = diagnostic["related"].as_array().expect("related spans");
    assert_eq!(related.len(), 1);
    assert_eq!(
        span_of(&related[0]["span"]),
        ("@std.fs".to_owned(), 5403, 5415, (234, 11), (234, 23))
    );
}

/// The same with a primary span in a project file and related spans in the
/// project file and twice in `@std.io`, the shape of case
/// `V2-EFFECT-std-read-all-row`: the witness chain `io.readAllText(input)`
/// (161 to 182), `readTextAfter(input "")` at bytes 2990 to 3013 of
/// `stdlib/src/std/io.vib` (126:18 to 126:41), and
/// `StdinText.read(input 65536u64)` at 3571 to 3601 (146:28 to 146:58).
#[test]
fn check_json_related_spans_in_a_project_file_and_a_library_module() {
    let tree = corpus_tree("EFFECT", "V2-EFFECT-std-read-all-row");
    let (code, document) = envelope_in(&tree, &["--format", "json", "check"]);
    assert_eq!(code, 1, "{document}");
    assert_eq!(codes(&document), ["@effect.outsideCeiling"]);
    let diagnostic = &document["diagnostics"][0];
    assert_eq!(
        span_of(&diagnostic["primarySpan"]).0,
        "src/app/main.vib".to_owned()
    );
    let related = diagnostic["related"]
        .as_array()
        .expect("related spans")
        .iter()
        .map(|related| span_of(&related["span"]))
        .collect::<Vec<_>>();
    assert_eq!(related.len(), 3);
    assert_eq!(
        (related[0].0.as_str(), related[0].1, related[0].2),
        ("src/app/main.vib", 161, 182)
    );
    assert_eq!(
        related[1..],
        [
            ("@std.io".to_owned(), 2990, 3013, (126, 18), (126, 41)),
            ("@std.io".to_owned(), 3571, 3601, (146, 28), (146, 58)),
        ]
    );
}
