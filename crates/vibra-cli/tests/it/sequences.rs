//! A long sequence of `let`s through the built binary: `check`, `test`, and
//! `run` each answer with their document, never a stack overflow, and `run`
//! reports the code the program returns.
//!
//! Each run is a child process, so an overflow ends that child with its
//! exception code and fails the assertion, not the test process. The
//! default size runs in seconds; the ignored test runs 100,000 `let`s.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::support::{TempDir, single_document};

/// The built `vibra`.
const VIBRA: &str = env!("CARGO_BIN_EXE_vibra");

/// The number of `let`s of the default test.
const LETS: usize = 20_000;

/// The code the entry returns when `f` returns the expected value.
const EXIT: u8 = 7;

/// A binary project whose entry is `@app.main.main`.
const BINARY: &str = "{format: @project.v2 package: {name: \"hello\" version: \"0.1.0\"} targets: [{name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: [] grants: [:]}] dependencies: [:]}";

/// The project whose `f` binds `n` sequential `let`s, `a0` to 1 and each
/// later one more, and returns `a0 + a(n-1)`; the entry returns [`EXIT`]
/// when that is `n + 1`, and a test asserts it.
fn project(n: usize) -> TempDir {
    let expected = n + 1;
    let mut main = format!(
        "use @std.process.ExitCode enduse\n\nproc main returns code: ExitCode do\n  if Equatable.equal(f() {expected}i32) then return ExitCode({EXIT}u8) endif\n  return ExitCode(1u8)\nendproc\n\nfn f returns r: I32 visibility @public do\n  let a0 be 1i32 endlet\n"
    );
    for k in 1..n {
        let _ = writeln!(main, "  let a{k} be I32.add(a{} 1i32) endlet", k - 1);
    }
    let _ = writeln!(main, "  return I32.add(a0 a{})\nendfn", n - 1);
    let tests = format!(
        "use @std.assert enduse\n\nuse @app.main enduse\n\ntest \"f\" do assert.equal(main.f() {expected}i32) endtest\n"
    );
    TempDir::project(
        &format!("sequence-lets-{n}"),
        BINARY,
        &[("src/app/main.vib", &main), ("tests/main.vib", &tests)],
    )
}

/// The one envelope of `vibra --format json` with `arguments` in
/// `directory`, checked against its schema, and the exit code.
fn json(directory: &Path, arguments: &[&str]) -> (i32, Value) {
    let output = Command::new(VIBRA)
        .args(["--format", "json"])
        .args(arguments)
        .current_dir(directory)
        .output()
        .expect("the binary runs");
    let code = output.status.code().expect("an exit status");
    assert!(
        (0..=6).contains(&code),
        "`vibra {}` ended with {code:#x}, not a result's exit code",
        arguments.join(" ")
    );
    let value = single_document(&String::from_utf8_lossy(&output.stdout));
    vibra_schema::validate_command_result(&value)
        .unwrap_or_else(|violation| panic!("{arguments:?}: {violation}"));
    (code, value)
}

/// `check`, `test`, and `run` of [`project`] at `n` each give their verdict.
fn commands(n: usize) {
    let directory = project(n);
    let path = directory.path();

    let (code, value) = json(path, &["check"]);
    assert_eq!(value["result"], "@command.ok", "{value}");
    assert_eq!(code, 0);

    let (code, value) = json(path, &["test"]);
    assert_eq!(value["result"], "@command.ok", "{value}");
    assert_eq!(
        value["payload"]["tests"][0]["result"], "@test.passed",
        "{value}"
    );
    assert_eq!(code, 0);

    let (code, value) = json(path, &["run"]);
    assert_eq!(value["result"], "@command.programFailed", "{value}");
    assert_eq!(value["payload"]["applicationCode"], EXIT, "{value}");
    assert_eq!(value["payload"]["trap"], Value::Null, "{value}");
    assert_eq!(code, 6);
}

#[test]
fn a_long_sequence_of_lets_gives_every_verdict() {
    commands(LETS);
}

#[test]
#[ignore = "minutes in a debug build; the default size proves the property"]
fn a_hundred_thousand_lets_give_every_verdict() {
    commands(100_000);
}
