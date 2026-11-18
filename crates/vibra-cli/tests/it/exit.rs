//! The result-to-exit mapping and the streams of the process.

use std::process::Command;

use serde_json::Value;
use vibra_cli::exit_code;
use vibra_schema::CommandOutcome;

use crate::support::{run, single_document};

#[test]
fn exit_table() {
    let exits = CommandOutcome::ALL.map(exit_code);
    assert_eq!(exits, [0, 1, 1, 2, 3, 4, 5, 6]);
    assert_eq!(exit_code(CommandOutcome::Ok), 0);
    assert_eq!(exit_code(CommandOutcome::Diagnostics), 1);
    assert_eq!(exit_code(CommandOutcome::TestFailed), 1);
    assert_eq!(exit_code(CommandOutcome::InvalidInput), 2);
    assert_eq!(exit_code(CommandOutcome::OperationalFailure), 3);
    assert_eq!(exit_code(CommandOutcome::Unavailable), 4);
    assert_eq!(exit_code(CommandOutcome::Trap), 5);
    assert_eq!(exit_code(CommandOutcome::ProgramFailed), 6);
}

#[test]
fn json_single_document() {
    for arguments in [
        &["--format", "json", "help"][..],
        &["--format", "json", "check"],
        &["--format", "json", "fmt", "a.vib", "--write"],
        &["--format", "json", "frobnicate"],
        &["--format", "json", "help", "extra"],
    ] {
        let outcome = run(arguments);
        let document = single_document(&outcome.stdout);
        assert!(document.is_object(), "{arguments:?}");
        assert!(
            outcome.stderr.is_empty(),
            "nothing but the envelope: {:?}",
            outcome.stderr
        );
    }
}

#[test]
fn the_binary_writes_one_envelope_and_exits_with_its_result() {
    let binary = env!("CARGO_BIN_EXE_vibra");
    for (arguments, code, result) in [
        (&["--format", "json", "help"][..], 0, "@command.ok"),
        (&["--format", "json", "check"], 4, "@command.unavailable"),
        (
            &["--format", "json", "frobnicate"],
            2,
            "@command.invalidInput",
        ),
    ] {
        let output = Command::new(binary)
            .args(arguments)
            .output()
            .expect("run the built vibra binary");
        assert_eq!(output.status.code(), Some(code), "{arguments:?}");
        let stdout = String::from_utf8(output.stdout).expect("UTF-8");
        let document: Value = single_document(&stdout);
        assert_eq!(document["result"], result);
        assert!(output.stderr.is_empty());
    }
    let human = Command::new(binary)
        .arg("help")
        .output()
        .expect("run the built vibra binary");
    assert_eq!(human.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&human.stdout).contains("usage: vibra"));
}
