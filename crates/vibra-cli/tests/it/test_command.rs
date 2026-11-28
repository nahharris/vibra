//! `vibra test [TEST]` through the command line, each on a confined project
//! tree. Expectations come from the tooling chapter, "Tests and selection",
//! "Versioned envelope and results", and "Command payloads"; spans are byte
//! offsets counted by hand in the sources below.

use serde_json::{Value, json};

use crate::support::{LIBRARY, TempDir, envelope_in, run_in};

/// `src/app/math.vib`: the library function the tests call.
const MATH: &str = "fn add\n  takes {left: I32 right: I32}\n  returns sum: I32\n  \
                    visibility @public\n  do\n    return I32.add(left right)\nendfn\n";

/// The imports of every test module that calls `math.add`: 46 bytes, so its
/// first test starts at byte 46, on line 5.
const PREAMBLE: &str = "use @std.assert enduse\n\nuse @app.math enduse\n\n";

const ADDS: &str = "test \"adds\" do assert.equal(3i32 math.add(1i32 2i32)) endtest\n";

const ADDS_WRONG: &str =
    "test \"adds\" do assert.equal(4i32 math.add(1i32 2i32)) endtest\n";

/// A test whose declaration closure reaches a form this checker does not
/// support: `pick`'s contract call on `T`, whose bound leaves `Item` open.
/// The call `Iter.next(source)` is 128 bytes into the text.
const UNAVAILABLE: &str = "interface Holder(T)\n  given {T: Iter}\n  fn pick\n    \
                           takes {holder: Self source: T}\n    returns found: I32\n    \
                           do\n      let _ be Iter.next(source) endlet\n      \
                           return 1i32\n  endfn\nendinterface\n\n\
                           type Box of I32\n  impl Holder(List(I32))\n  endimpl\nendtype\n\n\
                           test \"open\" do assert.equal(1i32 Holder.pick(Box(1i32) [1i32])) \
                           endtest\n";

/// A test that names the module `math` as a value: `math` is 26 bytes into
/// the test.
const MODULE_VALUE: &str = "test \"static\" do let _ be math endlet endtest\n";

/// A library project with `src/app/math.vib` and `files`.
fn project(label: &str, files: &[(&str, &str)]) -> TempDir {
    let mut all = vec![("src/app/math.vib", MATH)];
    all.extend_from_slice(files);
    TempDir::project(label, LIBRARY, &all)
}

/// The module `tests/math.vib` holding the preamble and `tests`.
fn math_tests(tests: &[&str]) -> String {
    let mut text = PREAMBLE.to_owned();
    text.push_str(&tests.join("\n"));
    text
}

fn test(root: &TempDir, arguments: &[&str]) -> (i32, Value) {
    let mut line = vec!["--format", "json", "test"];
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

fn counts(document: &Value) -> (u64, u64, u64) {
    let payload = &document["payload"];
    let count = |field: &str| payload[field].as_u64().expect("a count");
    (count("selected"), count("passed"), count("failed"))
}

fn names(document: &Value) -> Vec<String> {
    document["payload"]["tests"]
        .as_array()
        .expect("tests")
        .iter()
        .map(|item| item["name"].as_str().expect("name").to_owned())
        .collect()
}

fn results(document: &Value) -> Vec<Value> {
    document["payload"]["tests"]
        .as_array()
        .expect("tests")
        .iter()
        .map(|item| item["result"].clone())
        .collect()
}

#[test]
fn test_all_pass() {
    let root = project("test-pass", &[("tests/math.vib", &math_tests(&[ADDS]))]);
    let (code, document) = test(&root, &[]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["command"], "test");
    assert_eq!(document["result"], "@command.ok");
    assert_eq!(document["status"], "passed");
    assert_eq!(document["diagnostics"], json!([]));
    assert_eq!(
        document["limits"],
        json!({"profile": "m1Default", "reached": [], "complete": true})
    );
    assert!(document["workspaceRevision"].is_string());
    assert_eq!(counts(&document), (1, 1, 0));
    assert_eq!(
        document["payload"]["tests"],
        json!([{
            "name": "@tests.math::\"adds\"",
            "result": "@test.passed",
            "failure": null,
            "trap": null,
            "auditTrace": [],
            "diagnostics": [],
            "status": "passed"
        }])
    );
}

#[test]
fn test_failure() {
    let root = project(
        "test-failure",
        &[("tests/math.vib", &math_tests(&[ADDS_WRONG]))],
    );
    let (code, document) = test(&root, &[]);
    assert_eq!(code, 1, "{document}");
    assert_eq!(document["result"], "@command.testFailed");
    assert_eq!(document["status"], "refuted");
    assert_eq!(document["diagnostics"], json!([]));
    assert_eq!(counts(&document), (1, 0, 1));
    let item = &document["payload"]["tests"][0];
    assert_eq!(item["result"], "@test.assertionFailed");
    assert_eq!(item["status"], "refuted");
    assert_eq!(item["trap"], Value::Null);
    assert_eq!(item["diagnostics"], json!([]));
    // The call `assert.equal(4i32 math.add(1i32 2i32))` is bytes 61..99, on
    // line 5 from column 16 to column 54.
    assert_eq!(
        item["failure"],
        json!({
            "kind": "assertion",
            "assertion": "assert.equal",
            "expected": "4i32",
            "actual": "3i32",
            "primarySpan": {
                "sourceId": "tests/math.vib", "start": 61, "end": 99,
                "startPosition": {"line": 5, "column": 16},
                "endPosition": {"line": 5, "column": 54}
            },
            "observationError": null
        })
    );
}

/// `tests/observe.vib`: `assert.equal` on two `Any` values holding closures.
/// Its imports take 24 bytes, so the test is line 3 from byte 24.
const OBSERVE: &str = "use @std.assert enduse\n\n\
                       test \"closures\" do \
                       let a: Any be fn takes {n: I32} returns m: I32 do return n endfn endlet \
                       let b: Any be fn takes {n: I32} returns m: I32 do return n endfn endlet \
                       assert.equal(a b) endtest\n";

#[test]
fn test_observation_error() {
    let root = project("test-observation", &[("tests/observe.vib", OBSERVE)]);
    let (code, document) = test(&root, &[]);
    assert_eq!(code, 1, "{document}");
    assert_eq!(document["result"], "@command.testFailed");
    assert_eq!(document["status"], "refuted");
    assert_eq!(counts(&document), (1, 0, 1));
    // The call `assert.equal(a b)` is bytes 187..204, on line 3 from column
    // 164 to column 181. Its operands are observed at `Any`, encoded `@any`.
    assert_eq!(codes(&document), ["@tool.unsupportedValue"]);
    let span = &document["diagnostics"][0]["primarySpan"];
    assert_eq!(span["sourceId"], "tests/observe.vib");
    assert_eq!((&span["start"], &span["end"]), (&json!(187), &json!(204)));
    let item = &document["payload"]["tests"][0];
    assert_eq!(item["name"], "@tests.observe::\"closures\"");
    assert_eq!(item["result"], "@test.assertionFailed");
    assert_eq!(item["status"], "refuted");
    assert_eq!(item["trap"], Value::Null);
    assert_eq!(item["diagnostics"], json!(["@tool.unsupportedValue"]));
    assert_eq!(
        item["failure"],
        json!({
            "kind": "observation",
            "assertion": "assert.equal",
            "expected": null,
            "actual": null,
            "primarySpan": {
                "sourceId": "tests/observe.vib", "start": 187, "end": 204,
                "startPosition": {"line": 3, "column": 164},
                "endPosition": {"line": 3, "column": 181}
            },
            "observationError": {
                "code": "@tool.unsupportedValue",
                "type": "@any",
                "origin": null
            }
        })
    );
}

#[test]
fn test_selector() {
    let other = "test \"other\" do assert.true(false) endtest\n";
    let root = project(
        "test-selector",
        &[("tests/math.vib", &math_tests(&[ADDS, other]))],
    );
    let (code, document) = test(&root, &["@tests.math::\"adds\""]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(counts(&document), (1, 1, 0));
    assert_eq!(names(&document), ["@tests.math::\"adds\""]);
    let (code, all) = test(&root, &[]);
    assert_eq!(code, 1);
    assert_eq!(counts(&all), (2, 1, 1));
}

#[test]
fn test_selector_escaped_name() {
    let escaped = "test \"a \\\"b\\\"\" do assert.true(true) endtest\n";
    let root = project(
        "test-escaped",
        &[("tests/math.vib", &math_tests(&[ADDS, escaped]))],
    );
    let selector = "@tests.math::\"a \\\"b\\\"\"";
    let (code, document) = test(&root, &[selector]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(counts(&document), (1, 1, 0));
    assert_eq!(names(&document), [selector]);
}

/// Invalid input: no snapshot, no diagnostics, the empty payload.
fn assert_invalid_input(code: i32, document: &Value) {
    assert_eq!(code, 2, "{document}");
    assert_eq!(document["result"], "@command.invalidInput");
    assert_eq!(document["status"], "unknown");
    assert_eq!(document["diagnostics"], json!([]));
    assert_eq!(document["workspaceRevision"], Value::Null);
    assert_eq!(document["payload"], json!({}));
}

#[test]
fn test_selector_malformed() {
    let root = project(
        "test-malformed",
        &[("tests/math.vib", &math_tests(&[ADDS]))],
    );
    for selector in [
        "@tests.math:\"adds\"",
        "@tests.math:: \"adds\"",
        "@app.math::\"adds\"",
        "@tests.math::adds",
    ] {
        let (code, document) = test(&root, &[selector]);
        assert_invalid_input(code, &document);
    }
}

#[test]
fn test_selector_no_module() {
    let root = project(
        "test-no-module",
        &[("tests/math.vib", &math_tests(&[ADDS]))],
    );
    let (code, document) = test(&root, &["@tests.none::\"x\""]);
    assert_invalid_input(code, &document);
}

#[test]
fn test_selector_no_test() {
    let root = project("test-no-test", &[("tests/math.vib", &math_tests(&[ADDS]))]);
    let (code, document) = test(&root, &["@tests.math::\"missing\""]);
    assert_invalid_input(code, &document);
}

#[test]
fn test_parse_error_first() {
    // Chapter 07, "Recovery": no `fn` is open, so the `endfn` at bytes
    // 12..17 closes nothing and is reported at itself; the `test` opened at
    // bytes 0..4 is never closed and is reported at its opener.
    let root = project("test-parse", &[("tests/math.vib", "test \"x\" do endfn")]);
    let (code, document) = test(&root, &[]);
    assert_eq!(code, 1, "{document}");
    assert_eq!(document["result"], "@command.diagnostics");
    assert_eq!(document["status"], "refuted");
    assert_eq!(
        codes(&document),
        ["@syntax.unmatchedDelimiter", "@syntax.unmatchedDelimiter"]
    );
    let spans = document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|diagnostic| {
            let span = &diagnostic["primarySpan"];
            (
                span["sourceId"].clone(),
                span["start"].clone(),
                span["end"].clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        spans,
        [
            (json!("tests/math.vib"), json!(0), json!(4)),
            (json!("tests/math.vib"), json!(12), json!(17)),
        ]
    );
    assert_eq!(counts(&document), (0, 0, 0));
    assert_eq!(document["payload"]["tests"], json!([]));
}

#[test]
fn test_selector_parses_one_module() {
    let one = "use @std.assert enduse\n\ntest \"one\" do assert.true(true) endtest\n";
    let root = project(
        "test-one-module",
        &[("tests/a.vib", one), ("tests/b.vib", "test \"x\" do endfn")],
    );
    let (code, document) = test(&root, &["@tests.a::\"one\""]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["diagnostics"], json!([]));
    assert_eq!(counts(&document), (1, 1, 0));
    assert_eq!(names(&document), ["@tests.a::\"one\""]);
    let (code, all) = test(&root, &[]);
    assert_eq!(code, 1, "without a selector every test module is parsed");
    assert_eq!(
        codes(&all),
        ["@syntax.unmatchedDelimiter", "@syntax.unmatchedDelimiter"]
    );
    assert!(names(&all).is_empty());
}

#[test]
fn test_invalid_selection() {
    let tests = "use @std.assert enduse\n\n\
                 test \"bad\" do let x: I32 be \"a\" endlet endtest\n\n\
                 test \"good\" do assert.true(true) endtest\n";
    let root = project("test-invalid", &[("tests/math.vib", tests)]);
    let (code, document) = test(&root, &[]);
    assert_eq!(code, 1, "{document}");
    assert_eq!(document["result"], "@command.diagnostics");
    assert_eq!(document["status"], "refuted");
    // `"a"` at bytes 52..55, on line 3 from column 29: reported once.
    assert_eq!(codes(&document), ["@type.mismatch"]);
    assert_eq!(
        document["diagnostics"][0]["primarySpan"],
        json!({"sourceId": "tests/math.vib", "start": 52, "end": 55,
            "startPosition": {"line": 3, "column": 29},
            "endPosition": {"line": 3, "column": 32}})
    );
    assert_eq!(counts(&document), (2, 0, 2));
    for item in document["payload"]["tests"].as_array().expect("tests") {
        assert_eq!(item["result"], "@test.invalid");
        assert_eq!(item["status"], "refuted");
        assert_eq!(item["diagnostics"], json!(["@type.mismatch"]));
    }
}

#[test]
fn test_unavailable_item() {
    let root = project(
        "test-unavailable",
        &[("tests/math.vib", &math_tests(&[UNAVAILABLE, ADDS]))],
    );
    let (code, document) = test(&root, &[]);
    assert_eq!(code, 4, "{document}");
    assert_eq!(document["result"], "@command.unavailable");
    assert_eq!(document["status"], "unsupported");
    // `Iter.next(source)` in `pick`, bytes 174..191.
    assert_eq!(codes(&document), ["@tool.unavailable"]);
    let span = &document["diagnostics"][0]["primarySpan"];
    assert_eq!(span["sourceId"], "tests/math.vib");
    assert_eq!((&span["start"], &span["end"]), (&json!(174), &json!(191)));
    assert_eq!(counts(&document), (2, 1, 1));
    let items = &document["payload"]["tests"];
    assert_eq!(items[0]["name"], "@tests.math::\"open\"");
    assert_eq!(items[0]["result"], "@test.unavailable");
    assert_eq!(items[0]["status"], "unsupported");
    assert_eq!(items[0]["diagnostics"], json!(["@tool.unavailable"]));
    assert_eq!(items[1]["result"], "@test.passed");
    assert_eq!(items[1]["diagnostics"], json!([]));
}

#[test]
fn test_module_as_value_invalid() {
    // 02 "Namespaces and resolution": a module in value position is the wrong
    // kind of entity, an ordinary error, so the whole selection is invalid.
    let root = project(
        "test-module-value",
        &[("tests/math.vib", &math_tests(&[MODULE_VALUE, ADDS]))],
    );
    let (code, document) = test(&root, &[]);
    assert_eq!(code, 1, "{document}");
    assert_eq!(document["result"], "@command.diagnostics");
    // `math` in the first test, bytes 72..76.
    assert_eq!(codes(&document), ["@name.wrongEntityKind"]);
    let span = &document["diagnostics"][0]["primarySpan"];
    assert_eq!(span["sourceId"], "tests/math.vib");
    assert_eq!((&span["start"], &span["end"]), (&json!(72), &json!(76)));
    assert_eq!(
        results(&document),
        [json!("@test.invalid"), json!("@test.invalid")]
    );
}

#[test]
fn test_precedence_unavailable_over_failed() {
    let root = project(
        "test-precedence",
        &[("tests/math.vib", &math_tests(&[UNAVAILABLE, ADDS_WRONG]))],
    );
    let (code, document) = test(&root, &[]);
    assert_eq!(code, 4, "{document}");
    assert_eq!(document["result"], "@command.unavailable");
    assert_eq!(document["status"], "unsupported");
    assert_eq!(
        results(&document),
        [json!("@test.unavailable"), json!("@test.assertionFailed")]
    );
}

#[test]
fn test_memory_exhausted() {
    // `grow` doubles its list until the published budget is exhausted.
    let tests = "use @std.assert enduse\n\n\
                 fn grow\n  takes {items: List(I32)}\n  returns count: U64\n  do\n    \
                 return grow(List.concat(items items))\nendfn\n\n\
                 test \"a exhausts\" do assert.equal(0u64 grow([1i32])) endtest\n\n\
                 test \"b passes\" do assert.true(true) endtest\n";
    let root = project("test-memory", &[("tests/memory.vib", tests)]);
    let (code, document) = test(&root, &[]);
    assert_eq!(code, 3, "{document}");
    assert_eq!(document["result"], "@command.operationalFailure");
    assert_eq!(document["status"], "unknown");
    assert_eq!(
        document["limits"],
        json!({"profile": "m1Default", "reached": ["memory"], "complete": false})
    );
    assert_eq!(codes(&document), ["@runtime.memoryExhausted"]);
    assert_eq!(
        document["diagnostics"][0]["primarySpan"],
        json!({"sourceId": null, "start": 0, "end": 0,
            "startPosition": {"line": 1, "column": 1},
            "endPosition": {"line": 1, "column": 1}})
    );
    assert_eq!(counts(&document), (2, 1, 1));
    let items = &document["payload"]["tests"];
    assert_eq!(items[0]["name"], "@tests.memory::\"a exhausts\"");
    assert_eq!(items[0]["result"], "@test.exhausted");
    assert_eq!(items[0]["status"], "unknown");
    assert_eq!(items[0]["trap"], Value::Null);
    assert_eq!(items[0]["diagnostics"], json!(["@runtime.memoryExhausted"]));
    assert_eq!(items[1]["result"], "@test.passed");
}

#[test]
fn test_empty_suite() {
    let root = project("test-empty", &[]);
    let (code, document) = test(&root, &[]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["result"], "@command.ok");
    assert_eq!(document["status"], "passed");
    assert_eq!(counts(&document), (0, 0, 0));
    assert_eq!(document["payload"]["tests"], json!([]));
}

#[test]
fn test_order() {
    let b = "use @std.assert enduse\n\ntest \"second\" do assert.true(true) endtest\n\n\
             test \"first\" do assert.true(true) endtest\n";
    let a = "use @std.assert enduse\n\ntest \"z\" do assert.true(true) endtest\n\n\
             test \"y\" do assert.true(true) endtest\n";
    let root = project("test-order", &[("tests/b.vib", b), ("tests/a.vib", a)]);
    let (code, document) = test(&root, &[]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(
        names(&document),
        [
            "@tests.a::\"z\"",
            "@tests.a::\"y\"",
            "@tests.b::\"second\"",
            "@tests.b::\"first\"",
        ]
    );
}

#[test]
fn test_human_report() {
    let root = project(
        "test-human",
        &[("tests/math.vib", &math_tests(&[ADDS_WRONG]))],
    );
    let outcome = run_in(root.path(), &["test"]);
    assert_eq!(outcome.code, 1);
    let lines = outcome.stdout.lines().collect::<Vec<_>>();
    assert_eq!(
        lines.first(),
        Some(&"@tests.math::\"adds\" @test.assertionFailed"),
        "{}",
        outcome.stdout
    );
    assert!(
        lines.iter().any(|line| line.contains("assert.equal")
            && line.contains("4i32")
            && line.contains("3i32")
            && line.contains("tests/math.vib:5:16")),
        "{}",
        outcome.stdout
    );
    assert_eq!(lines.last(), Some(&"passed 0, failed 1, selected 1"));
    let passing = project(
        "test-human-pass",
        &[("tests/math.vib", &math_tests(&[ADDS]))],
    );
    let outcome = run_in(passing.path(), &["test"]);
    assert_eq!(outcome.code, 0);
    assert_eq!(
        outcome.stdout,
        "@tests.math::\"adds\" @test.passed\npassed 1, failed 0, selected 1\n"
    );
}

#[test]
fn test_without_a_project_is_not_found() {
    let missing = TempDir::new("test-json-no-project");
    let (code, document) = envelope_in(missing.path(), &["--format", "json", "test"]);
    assert_eq!(code, 1);
    assert_eq!(codes(&document), ["@project.notFound"]);
    assert_eq!(counts(&document), (0, 0, 0));
}

/// `tests/world.vib`: a test whose `TestWorld.directory` fixture holds `["a"]`
/// as a file and as the parent of `["a" "b"]`. The `await` of the call is
/// bytes 293 to 348, on line 13.
const CONFLICTING: &str = "use @std.assert enduse\n\nuse @std.test.TestWorld enduse\n\n\
    use @std.fs.RelativePath enduse\n\n\
    test \"conflict\"\n  takes {world: TestWorld}\n  performs {TestWorld}\n  do\n    \
    let Result.ok(value: a) be RelativePath([\"a\"]) endlet\n    \
    let Result.ok(value: b) be RelativePath([\"a\" \"b\"]) endlet\n    \
    let _ be await TestWorld.directory(world files: [a: \"1\" b: \"2\"]) endlet\nendtest\n";

/// 04 "Test worlds and grants" and 05 "Tests and selection": a conflicting
/// fixture ends its test `@test.invalid` with `@test.conflictingFixture` at
/// the `await`, attributed to that item; it counts as failed, so the suite is
/// `@command.testFailed` (exit 1), and the other test still passes.
#[test]
fn test_conflicting_fixture() {
    let root = project(
        "conflicting-fixture",
        &[
            ("tests/math.vib", &math_tests(&[ADDS])),
            ("tests/world.vib", CONFLICTING),
        ],
    );
    let (code, document) = test(&root, &[]);
    assert_eq!(code, 1, "{document}");
    assert_eq!(document["result"], "@command.testFailed");
    assert_eq!(document["status"], "refuted");
    assert_eq!(counts(&document), (2, 1, 1));
    assert_eq!(
        results(&document),
        [json!("@test.passed"), json!("@test.invalid")]
    );
    let item = &document["payload"]["tests"][1];
    assert_eq!(item["name"], "@tests.world::\"conflict\"");
    assert_eq!(item["status"], "refuted");
    assert_eq!(item["diagnostics"], json!(["@test.conflictingFixture"]));
    assert_eq!(item["auditTrace"], json!([]));
    assert_eq!(codes(&document), ["@test.conflictingFixture"]);
    let span = &document["diagnostics"][0]["primarySpan"];
    assert_eq!(span["sourceId"], "tests/world.vib");
    assert_eq!(
        (span["start"].as_u64(), span["end"].as_u64()),
        (Some(293), Some(348))
    );
    assert_eq!(span["startPosition"]["line"], 13);
}
/// `tests/stdin.vib`: one test reading its stdin to the end through
/// `io.readAllText`, whose host calls are awaited inside `@std.io`.
const READ_ALL: &str = "use @std.assert enduse\n\nuse @std.test.TestWorld enduse\n\n\
                        use @std.io enduse\n\nuse @std.io.StdinText enduse\n\n\
                        test \"reads all\"\n  takes {world: TestWorld}\n  \
                        performs {TestWorld StdinText}\n  do\n    \
                        let {stdinText: input} be await TestWorld.streams(world stdin: \"hi\") endlet\n    \
                        assert.equal(Result.ok(value: \"hi\") await io.readAllText(input))\nendtest\n";

/// 06 "Audit trace": an event's origin is the `await` that ran the host
/// call, here `await StdinText.read(input 65536u64)` at bytes 3565 to 3601
/// of `stdlib/src/std/io.vib` (146:22 to 146:58), named by the module atom
/// `@std.io` (04 "Discovery and the source snapshot"); its positions come
/// from the library module's text. Two reads: "hi", then the empty end.
#[test]
fn test_trace_origin_in_a_library_module() {
    let root = TempDir::project(
        "test-library-origin",
        LIBRARY,
        &[("src/app/math.vib", MATH), ("tests/stdin.vib", READ_ALL)],
    );
    let (code, document) = envelope_in(root.path(), &["--format", "json", "test"]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["result"], "@command.ok");
    let trace = document["payload"]["tests"][0]["auditTrace"]
        .as_array()
        .expect("an audit trace");
    assert_eq!(trace.len(), 2);
    for event in trace {
        assert_eq!(event["op"], "io.stdinText");
        assert_eq!(
            event["origin"],
            json!({
                "sourceId": "@std.io",
                "start": 3565,
                "end": 3601,
                "startPosition": {"line": 146, "column": 22},
                "endPosition": {"line": 146, "column": 58},
            })
        );
    }
}
