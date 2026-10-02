//! M3 review fixes that only a real process shows.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn project(label: &str, source: &str) -> PathBuf {
    let root = std::env::temp_dir()
        .join(format!("vibra-cli-review-{label}-{}", std::process::id()));
    fs::create_dir_all(root.join("src/app")).expect("source root");
    fs::write(
        root.join("project.vibon"),
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array))) dependencies: (dict))\n",
    )
    .expect("project marker");
    fs::write(root.join("src/app/main.vib"), source).expect("entry module");
    root
}

/// The entry's result is encoded and released on the interpreter thread: a
/// deeply nested value must not overflow the stack of the command's own
/// thread (`docs/spec/06-runtime.md`, "Tail calls": exhaustion never aborts
/// the process).
#[test]
fn a_deeply_nested_entry_result_does_not_abort_the_process() {
    let root = project(
        "deep-result",
        "(deftype nat (record prev (array nat)))\n\n(deftype failure (record n nat))\n\n(defn lower (count i32) i32\n  (match (i32.sub-checked count 1i32)\n    (result.ok value) value\n    (result.err -) 0i32))\n\n(defn build (count i32 inner nat) nat\n  (if (i32.equal count 0i32)\n    inner\n    (build (lower count) (nat prev: (array.of inner)))))\n\n(defn execute () (result void failure)\n  (result.err (failure n: (build 2000i32 (nat prev: (array.of))))))\n",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_vibra"))
        .args(["--format", "json", "--workspace"])
        .arg(&root)
        .args(["run", "src/app"])
        .output()
        .expect("vibra binary");
    let _ = fs::remove_dir_all(&root);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("json envelope");
    assert_eq!(envelope["result"], "@command.ok");
    assert!(
        envelope["payload"]["programResult"]
            .as_str()
            .is_some_and(|result| result.contains("variant: @err")),
    );
}

/// A function hidden behind `any` in the entry's result has no encoding to
/// report: `run` ends with a coded trap, not an interpreter invariant
/// failure (`docs/spec/06-runtime.md`, "Traps").
#[test]
fn an_entry_result_hiding_a_function_is_a_coded_trap() {
    let root = project(
        "hidden-function",
        "(defn inc (x i32) i32 x)\n\n(defn execute () (result void any) (result.err inc))\n",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_vibra"))
        .args(["--format", "json", "--workspace"])
        .arg(&root)
        .args(["run", "src/app"])
        .output()
        .expect("vibra binary");
    let _ = fs::remove_dir_all(&root);
    assert_eq!(output.status.code(), Some(5), "{output:?}");
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("invariant"),
        "{output:?}"
    );
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("json envelope");
    assert_eq!(envelope["result"], "@command.trap");
    assert_eq!(
        envelope["diagnostics"][0]["code"],
        "@runtime.unobservable-function"
    );
    assert_eq!(
        envelope["payload"]["trap"]["trapCode"],
        "@runtime.unobservable-function"
    );
    assert!(envelope["payload"]["trap"]["origin"].is_null());
}
