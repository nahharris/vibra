//! Every tool limit a command reaches is named in `limits.reached`
//! (`docs/spec/05-tooling.md`, "Limits"): the reduction limit of one
//! compile-time evaluation is `compileTimeReductions`, and the derivation
//! limit of the discharge of one obligation is `proofDerivations`. Each is
//! reported by `@tool.limitExceeded`, the outcome is unknown, and the
//! observation incomplete.

use std::fmt::Write as _;

use serde_json::{Value, json};

use crate::support::{LIBRARY, TempDir, envelope_in};

/// A binary project whose entry is `@app.main.main`.
const BINARY: &str = "{format: @project.v2 package: {name: \"hello\" version: \"0.1.0\"} targets: [{name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: [] grants: [:]}] dependencies: [:]}";

/// The entry of [`BINARY`].
const ENTRY: &str = "use @std.process.ExitCode enduse\n\nproc main returns code: ExitCode do return ExitCode(0u8) endproc\n";

/// A test module that reaches `@app.main`.
const TESTS: &str = "use @std.assert enduse\n\nuse @app.main enduse\n\ntest \"t\" do assert.equal(1i32 1i32) endtest\n";

/// A constant construction of `Big` whose `where` predicate takes 2^30
/// calls to evaluate: past the reduction limit of one compile-time
/// evaluation.
fn reductions() -> String {
    let mut source = String::from(
        "type Big of I32 where fn takes {raw: I32} returns ok: Bool do return f30(raw) endfn endtype\n\
         fn f0 takes {x: I32} returns r: Bool do return Ordered.greater(x 0i32) endfn\n",
    );
    for level in 1..=30 {
        let below = level - 1;
        let _ = writeln!(
            source,
            "fn f{level} takes {{x: I32}} returns r: Bool do return Bool.and(f{below}(x) f{below}(x)) endfn"
        );
    }
    source.push_str(
        "fn big returns r: I32 do let b be Big(1i32) endlet return 0i32 endfn\n",
    );
    source
}

/// A call whose `requires` follows from a closure of 80 ordered facts:
/// past the derivation limit of the discharge of one obligation.
fn derivations() -> String {
    let parameters = (0..80)
        .map(|k| format!("a{k}: I32"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut body = String::new();
    for k in 0..79 {
        let _ = write!(body, "if Ordered.lessOrEqual(a{k} a{}) then ", k + 1);
    }
    body.push_str("return leReq(a0 a79)");
    body.push_str(&" endif".repeat(79));
    format!(
        "fn leReq takes {{a: I32 b: I32}} returns r: I32\n  requires Ordered.lessOrEqual(a b)\n  do return a\nendfn\n\
         fn f takes {{{parameters}}} returns r: I32\n  do\n    {body}\n    return 0i32\nendfn\n"
    )
}

fn codes(value: &Value) -> Vec<String> {
    value["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().expect("a code").to_owned())
        .collect()
}

/// Asserts that `value` reports `limit` alone, reached, by
/// `@tool.limitExceeded`, with the status `unknown`.
fn assert_reached(value: &Value, limit: &str) {
    assert!(
        codes(value).contains(&"@tool.limitExceeded".to_owned()),
        "{value}"
    );
    assert_eq!(value["status"], "unknown", "{value}");
    assert_eq!(value["limits"]["reached"], json!([limit]), "{value}");
    assert_eq!(value["limits"]["complete"], false, "{value}");
}

fn check(label: &str, module: &str) -> Value {
    let directory = TempDir::project(label, LIBRARY, &[("src/app/main.vib", module)]);
    envelope_in(directory.path(), &["--format", "json", "check"]).1
}

fn test(label: &str, module: &str) -> Value {
    let directory = TempDir::project(
        label,
        LIBRARY,
        &[("src/app/main.vib", module), ("tests/main.vib", TESTS)],
    );
    envelope_in(directory.path(), &["--format", "json", "test"]).1
}

fn run(label: &str, module: &str) -> Value {
    let directory = TempDir::project(
        label,
        BINARY,
        &[("src/app/main.vib", &format!("{ENTRY}{module}"))],
    );
    envelope_in(directory.path(), &["--format", "json", "run"]).1
}

#[test]
fn check_names_the_compile_time_reduction_limit() {
    assert_reached(
        &check("limit-check-reductions", &reductions()),
        "compileTimeReductions",
    );
}

#[test]
fn check_names_the_proof_derivation_limit() {
    assert_reached(
        &check("limit-check-derivations", &derivations()),
        "proofDerivations",
    );
}

#[test]
fn test_names_the_compile_time_reduction_limit() {
    assert_reached(
        &test("limit-test-reductions", &reductions()),
        "compileTimeReductions",
    );
}

#[test]
fn run_names_the_compile_time_reduction_limit() {
    assert_reached(
        &run("limit-run-reductions", &reductions()),
        "compileTimeReductions",
    );
}
