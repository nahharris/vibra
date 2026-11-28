//! The rendering of a trapped suite: the outcome is built by hand, as the
//! runner's own tests show a trap ends only its own test and makes the suite
//! `SuiteResult::Trap`.

use serde_json::Value;
use vibra_schema::{CommandOutcome, Payload};
use vibra_workspace::{SuiteResult, TestOutcome, Trap};

use super::finish;
use crate::args::OutputFormat;
use crate::render::{Response, emit};

/// The exit code and standard output of the response to a suite whose first
/// test trapped and whose second passed, in `format`.
fn trapped(format: OutputFormat) -> (i32, String) {
    let items = vec![
        (
            "@tests.math::\"first\"".to_owned(),
            TestOutcome::Trap {
                trap: Trap::InvalidCheckedProgram,
                origin: None,
            },
        ),
        ("@tests.math::\"second\"".to_owned(), TestOutcome::Passed),
    ];
    let mut response =
        Response::new(format, "test", CommandOutcome::Ok, Payload::Empty {});
    finish(
        &mut response,
        SuiteResult::Trap,
        items,
        vec![Vec::new(), Vec::new()],
        &[],
    );
    let mut stdout = Vec::new();
    let code = emit(&response, &mut stdout, &mut std::io::sink(), &|envelope| {
        envelope.to_validated_json()
    });
    (code, String::from_utf8(stdout).expect("UTF-8"))
}

#[test]
fn a_trapped_test_is_a_trap_record_and_the_suite_traps() {
    let (code, output) = trapped(OutputFormat::Json);
    let (human_code, human) = trapped(OutputFormat::Human);
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
