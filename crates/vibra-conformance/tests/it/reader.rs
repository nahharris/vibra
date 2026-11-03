//! The `vibra-conformance` binary exits nonzero when a case fails and when the
//! corpus is empty. A passing reader-v1 case through the runner is every
//! `V1-SRC-FMT-*` and `V1-SRC-READER-*` corpus case.

use std::process::Command;

use crate::support::TempDir;

#[test]
fn internal_entrypoint_returns_nonzero_when_a_case_fails() {
    let root = TempDir::new("reader-failing");
    let case = root.join("V1-SRC-READER-entrypoint-failure");
    std::fs::create_dir_all(&case).expect("create temporary case");
    std::fs::write(
        case.join("case.toml"),
        r#"id = "V1-SRC-READER-entrypoint-failure"
rule = "V1-SRC-READER"
profile = "reader-v1"

[inputs]
source = "input.vib"

[expect]
accepted = false

[[expect.diagnostics]]
code = "@syntax.unmatched-delimiter"
level = "@error"

[expect.diagnostics.span]
start = 0
end = 1
"#,
    )
    .expect("write temporary manifest");
    std::fs::write(case.join("input.vib"), "(valid)\n")
        .expect("write temporary source");

    let status = Command::new(env!("CARGO_BIN_EXE_vibra-conformance"))
        .args(["--root"])
        .arg(root.path())
        .status()
        .expect("run internal conformance entrypoint");
    assert!(!status.success(), "a failed conformance case must fail CI");
}

#[test]
fn internal_entrypoint_returns_nonzero_for_an_empty_corpus() {
    let root = TempDir::new("reader-empty");

    let status = Command::new(env!("CARGO_BIN_EXE_vibra-conformance"))
        .args(["--root"])
        .arg(root.path())
        .status()
        .expect("run internal conformance entrypoint");
    assert!(!status.success(), "an empty corpus must fail CI");
}
