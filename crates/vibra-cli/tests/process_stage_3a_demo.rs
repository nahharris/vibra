//! M3 Step 9: the checked-in Stage 3A demo checks, passes its tests, and runs
//! with the actual binary.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::process::{Command, Output};

fn demo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/stage-3a-config")
        .canonicalize()
        .expect("demo project")
}

fn vibra(arguments: &[&str]) -> Output {
    let workspace = demo().to_string_lossy().into_owned();
    Command::new(env!("CARGO_BIN_EXE_vibra"))
        .args(["--format", "json", "--workspace", &workspace])
        .args(arguments)
        .output()
        .expect("vibra binary")
}

fn envelope(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("json envelope")
}

#[test]
fn the_stage_3a_demo_checks_tests_and_runs() {
    let check = vibra(&["check"]);
    assert_eq!(check.status.code(), Some(0), "{check:?}");
    assert_eq!(envelope(&check)["payload"]["accepted"], true);

    let test = vibra(&["test"]);
    assert_eq!(test.status.code(), Some(0), "{test:?}");
    let test = envelope(&test);
    assert_eq!(test["result"], "@command.ok");
    assert_eq!(test["payload"]["selected"], 6);
    assert_eq!(test["payload"]["passed"], 6);

    let run = vibra(&["run", "src/app"]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    let run = envelope(&run);
    assert_eq!(run["result"], "@command.ok");
    assert!(
        run["payload"]["programResult"]
            .as_str()
            .is_some_and(|result| result.contains("variant: @ok")),
        "{run}"
    );
}
