//! M4 Step 16b-2: the checked-in console example checks, passes its test, and
//! runs with the actual binary over the real standard streams. Human `run` lets
//! the program write the process's streams; JSON `run` captures the same bytes
//! in the envelope, and the audit trace is an array of event objects either way.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn demo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/m4-console")
        .canonicalize()
        .expect("demo project")
}

/// Runs the binary with `input` on its standard input.
fn vibra(arguments: &[&str], input: &str) -> Output {
    let workspace = demo().to_string_lossy().into_owned();
    let mut child = Command::new(env!("CARGO_BIN_EXE_vibra"))
        .args(["--workspace", &workspace])
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("vibra binary");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write input");
    child.wait_with_output().expect("output")
}

#[test]
fn the_console_example_checks_and_passes_its_test() {
    let check = vibra(&["--format", "json", "check"], "");
    assert_eq!(check.status.code(), Some(0), "{check:?}");
    let test = vibra(&["--format", "json", "test"], "");
    assert_eq!(test.status.code(), Some(0), "{test:?}");
    let test: serde_json::Value = serde_json::from_slice(&test.stdout).expect("json");
    assert_eq!(test["payload"]["passed"], 1);
    // The test's standard output went to its audit trace and not the process.
    let events = test["payload"]["tests"][0]["auditTrace"]
        .as_array()
        .expect("trace");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["op"], "io.stdout-text");
    assert_eq!(events[0]["args"][0], "\"a\"");
}

#[test]
fn the_console_example_is_in_canonical_format() {
    for source in ["src/console/walk.vib", "src/app/main.vib", "tests/walk.vib"] {
        let written = std::fs::read_to_string(demo().join(source)).expect("source");
        let output = vibra(&["fmt", source], "");
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        // Compare without the host's line endings: a checkout may rewrite them.
        let formatted = String::from_utf8(output.stdout).expect("utf-8");
        assert_eq!(
            formatted.replace("\r\n", "\n"),
            written.replace("\r\n", "\n"),
            "{source}"
        );
    }
}

#[test]
fn json_run_captures_the_streams_and_reports_the_events_as_objects() {
    let run = vibra(&["--format", "json", "run", "src/app"], "hello\n");
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    let envelope: serde_json::Value =
        serde_json::from_slice(&run.stdout).expect("json");
    assert_eq!(envelope["result"], "@command.ok");
    assert_eq!(envelope["payload"]["stdout"], "hello\none\ntwo\nthree\n");
    assert_eq!(envelope["payload"]["stderr"], "");
    let events = envelope["payload"]["auditTrace"].as_array().expect("trace");
    let ops = events
        .iter()
        .map(|event| event["op"].as_str().expect("op"))
        .collect::<Vec<_>>();
    assert_eq!(
        ops,
        [
            "io.stdin-text",
            "io.stdout-text",
            "io.stdout-text",
            "io.stdout-text",
            "io.stdout-text"
        ]
    );
    assert_eq!(events[0]["kind"], "@host");
    assert_eq!(
        events[0]["result"],
        "(record kind: @enum type: @std.result.result variant: @ok payload: \"hello\\n\")"
    );
}

#[test]
fn human_run_writes_the_programs_bytes_to_the_process() {
    let run = vibra(&["run", "src/app"], "hello\n");
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(
        String::from_utf8(run.stdout).expect("utf-8"),
        "hello\none\ntwo\nthree\n"
    );
}
