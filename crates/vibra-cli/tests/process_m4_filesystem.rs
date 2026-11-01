//! M4 Step 17: the demo shape through the real binary and the system provider.
//! A program reads a path from standard input, reads the file it names, writes a
//! transformed copy beside it, and prints the transformation. The files are in a
//! fresh directory under the system temporary directory, removed afterwards.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const PROJECT: &str = r#"(record format: @project.v1 package: (record name: "fs-demo" version: "0.1.0") targets: (array (record name: @app kind: @bin root: "src/app" entry: @app.main.main effects: (array @std.fs.read @std.fs.write @std.io.stdin @std.io.stdout))) dependencies: (dict))
"#;

const MAIN: &str = r#"(import fs @std.fs)
(import io @std.io)
(import text @std.text)

(defn main () (result void fs.fs-error)
  effects: (fs.read fs.write io.stdin io.stdout)
  (let name (match (io.stdin.text)
    (result.ok line) (text.trim line)
    (result.err -) ""))
  (let contents (try (fs.read.text (fs.path name))))
  (let shout (text.concat "> " contents))
  (let - (try (fs.write.text (fs.path (text.concat name ".out")) shout)))
  (match (io.stdout.text shout)
    (result.ok) (result.ok)
    (result.err -) (result.ok)))
"#;

/// A fresh directory that is removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        // A counter, not the clock, makes the name unique within the process: the
        // clock of some platforms is too coarse to tell two parallel tests apart.
        static NEXT: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "vibra-fs17-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("project/src/app")).expect("project");
        std::fs::create_dir_all(path.join("data")).expect("data");
        std::fs::write(path.join("project/project.vibon"), PROJECT).expect("manifest");
        std::fs::write(path.join("project/src/app/main.vib"), MAIN).expect("source");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn vibra(scratch: &Scratch, arguments: &[&str], input: &str) -> Output {
    let workspace = scratch.0.join("project").to_string_lossy().into_owned();
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

fn spelled(path: &std::path::Path) -> String {
    path.to_str().expect("utf-8 path").replace('\\', "/")
}

#[test]
fn a_program_reads_the_file_named_on_stdin_and_writes_beside_it() {
    let scratch = Scratch::new();
    let input = scratch.0.join("data/note.txt");
    std::fs::write(&input, "héllo\n").expect("input file");
    let at = spelled(&input);
    let run = vibra(
        &scratch,
        &["--format", "json", "run", "src/app"],
        &format!("{at}\n"),
    );
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    let envelope: serde_json::Value =
        serde_json::from_slice(&run.stdout).expect("json");
    assert_eq!(envelope["result"], "@command.ok");
    assert_eq!(envelope["payload"]["stdout"], "> héllo\n");
    // The write reached the real file, beside the input.
    assert_eq!(
        std::fs::read_to_string(scratch.0.join("data/note.txt.out")).expect("output"),
        "> héllo\n"
    );
    let ops = envelope["payload"]["auditTrace"]
        .as_array()
        .expect("trace")
        .iter()
        .map(|event| event["op"].as_str().expect("op").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        ops,
        [
            "io.stdin-text",
            "fs.read-text",
            "fs.write-text",
            "io.stdout-text"
        ]
    );
    assert_eq!(
        envelope["payload"]["auditTrace"][1]["args"][0],
        format!("(record kind: @wrapper type: @std.fs.path value: \"{at}\")")
    );
}

#[test]
fn a_missing_file_is_the_err_of_its_call_the_entry_returns_and_no_trap() {
    let scratch = Scratch::new();
    let missing = spelled(&scratch.0.join("data/absent.txt"));
    let run = vibra(
        &scratch,
        &["--format", "json", "run", "src/app"],
        &format!("{missing}\n"),
    );
    // The outcome code of an `err` entry is Step 19; here the run has no trap.
    let envelope: serde_json::Value =
        serde_json::from_slice(&run.stdout).expect("json");
    assert!(
        envelope["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .is_empty()
    );
    assert!(envelope["payload"]["trap"].is_null());
    assert!(
        envelope["payload"]["programResult"]
            .as_str()
            .expect("result")
            .contains("variant: @not-found"),
        "{envelope}"
    );
    assert!(!scratch.0.join("data/absent.txt.out").exists());
}
