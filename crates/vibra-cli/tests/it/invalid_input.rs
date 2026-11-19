//! Invalid input: exit 2, no diagnostics, the recognized command name or an
//! empty one, and the empty payload.

use serde_json::json;

use crate::support::{envelope, run};

/// Asserts the refusal of `arguments` and returns its command name.
fn refused(arguments: &[&str]) -> String {
    let (code, document) = envelope(arguments);
    assert_eq!(code, 2, "{arguments:?}");
    assert_eq!(document["result"], "@command.invalidInput", "{arguments:?}");
    assert_eq!(document["diagnostics"], json!([]), "{arguments:?}");
    assert_eq!(document["payload"], json!({}), "{arguments:?}");
    assert_eq!(document["workspaceRevision"], json!(null), "{arguments:?}");
    document["command"].as_str().expect("command").to_owned()
}

#[test]
fn unknown_command() {
    assert_eq!(refused(&["--format", "json", "frobnicate"]), "");
    assert_eq!(refused(&["--format", "json", "--wirte"]), "");
}

#[test]
fn alias_rejected() {
    for alias in ["init", "show", "find"] {
        assert_eq!(refused(&["--format", "json", alias]), "", "{alias}");
    }
}

#[test]
fn unknown_option() {
    assert_eq!(
        refused(&["--format", "json", "fmt", "a.vib", "--wirte"]),
        "fmt"
    );
    assert_eq!(refused(&["--format", "json", "check", "--write"]), "check");
}

#[test]
fn repeated_global() {
    // The format recognized before the fault is the format of the refusal.
    assert_eq!(
        refused(&["--format", "json", "--format", "human", "help"]),
        ""
    );
    assert_eq!(
        refused(&[
            "--format",
            "json",
            "--workspace",
            "a",
            "--workspace",
            "b",
            "help"
        ]),
        ""
    );
}

#[test]
fn repeated_single_option_after_the_command() {
    assert_eq!(
        refused(&["--format", "json", "fmt", "a.vib", "--write", "--write"]),
        "fmt"
    );
    assert_eq!(
        refused(&[
            "--format", "json", "build", "--target", "a", "--target", "b"
        ]),
        "build"
    );
}

#[test]
fn global_after_command() {
    assert_eq!(run(&["help", "--format", "json"]).code, 2);
    assert_eq!(
        refused(&["--format", "json", "check", "--format", "json"]),
        "check"
    );
    assert_eq!(
        refused(&["--format", "json", "help", "--workspace", "a"]),
        "help"
    );
}

#[test]
fn extra_positional() {
    assert_eq!(
        refused(&["--format", "json", "check", "@app", "@lib"]),
        "check"
    );
    assert_eq!(
        refused(&["--format", "json", "project", "inspect", "x"]),
        "project inspect"
    );
    assert_eq!(
        refused(&["--format", "json", "fmt", "a.vib", "b.vib"]),
        "fmt"
    );
}

#[test]
fn a_missing_operand_or_command_is_invalid_input() {
    assert_eq!(refused(&["--format", "json", "fmt"]), "fmt");
    assert_eq!(refused(&["--format", "json", "eval"]), "eval");
    assert_eq!(
        refused(&["--format", "json", "edit", "rename", "subject"]),
        "edit rename"
    );
    assert_eq!(refused(&["--format", "json"]), "");
    assert_eq!(run(&[]).code, 2);
}

#[test]
fn a_group_needs_a_known_subcommand() {
    assert_eq!(refused(&["--format", "json", "project"]), "");
    assert_eq!(refused(&["--format", "json", "project", "frobnicate"]), "");
    assert_eq!(refused(&["--format", "json", "edit", "init"]), "");
    assert_eq!(
        refused(&["--format", "json", "project", "add", "widget", "x"]),
        "project add"
    );
    assert_eq!(
        refused(&["--format", "json", "project", "add", "target"]),
        "project add"
    );
}

#[test]
fn an_option_needs_its_value() {
    assert_eq!(refused(&["--format", "json", "eval", "f", "--arg"]), "eval");
    assert_eq!(refused(&["--format", "json", "--workspace"]), "");
    assert_eq!(run(&["--format"]).code, 2);
}

#[test]
fn only_run_accepts_the_separator() {
    assert_eq!(refused(&["--format", "json", "build", "--", "x"]), "build");
    assert_eq!(refused(&["--format", "json", "check", "--"]), "check");
}

#[test]
fn recovery_after_bad_global() {
    let outcome = run(&["--format", "xml", "help"]);
    assert_eq!(outcome.code, 2);
    // The format was not recognized, so the refusal is in the default, human
    // format: nothing on standard output, one line on standard error.
    assert!(outcome.stdout.is_empty(), "{:?}", outcome.stdout);
    assert_eq!(outcome.stderr.lines().count(), 1, "{:?}", outcome.stderr);
    assert!(outcome.stderr.starts_with("invalid input:"));
}

#[test]
fn a_valid_line_follows_an_invalid_one() {
    assert_eq!(run(&["--format", "xml", "help"]).code, 2);
    assert_eq!(run(&["help"]).code, 0);
    assert_eq!(run(&["--format", "json", "check", "@app", "@lib"]).code, 2);
    assert_eq!(run(&["--format", "json", "lint", "@app"]).code, 4);
}

#[test]
fn a_global_option_is_accepted_in_either_order() {
    let (code, document) =
        envelope(&["--workspace", "somewhere", "--format", "json", "help"]);
    assert_eq!(code, 0);
    assert_eq!(document["command"], "help");
    let (code, _) = envelope(&["--format", "json", "--workspace", "somewhere", "help"]);
    assert_eq!(code, 0);
}
