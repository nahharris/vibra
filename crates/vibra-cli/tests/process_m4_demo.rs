//! M4 Step 22: the demo gate through the real binary and the system provider.
//!
//! `examples/m4-demo` declares `@std.env.read`, `@std.fs.read`, and
//! `@std.io.stdout`, reads the path that `vibra run TARGET -- ARG...` supplies,
//! quotes each line, and writes the result to standard output. The same source
//! is rejected when its function ceiling or its target array omits a performed
//! root, and no grant, prompt, or runtime effect check takes part.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

/// A copy of the demo project under the system temporary directory, removed when
/// dropped, beside a directory for the files the demo reads.
struct Scratch(PathBuf);

fn copy_directory(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("directory");
    for entry in std::fs::read_dir(from).expect("a directory") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a type").is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy");
        }
    }
}

impl Scratch {
    fn new() -> Self {
        // A counter, not the clock, makes the name unique within the process.
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "vibra-demo22-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        copy_directory(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/m4-demo"),
            &path.join("project"),
        );
        std::fs::create_dir_all(path.join("data")).expect("data");
        Self(path)
    }

    fn project(&self) -> String {
        self.0.join("project").to_string_lossy().into_owned()
    }

    fn vibra(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_vibra"))
            .args(["--workspace", &self.project()])
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("vibra binary")
    }

    fn edit(&self, file: &str, from: &str, to: &str) {
        let path = self.0.join("project").join(file);
        let text = std::fs::read_to_string(&path).expect("file");
        assert!(text.contains(from), "{file} lacks {from:?}");
        std::fs::write(path, text.replacen(from, to, 1)).expect("edit");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn spelled(path: &Path) -> String {
    path.to_str().expect("utf-8 path").replace('\\', "/")
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("json")
}

#[test]
fn the_demo_reads_the_path_it_is_given_and_writes_the_quoted_content() {
    let scratch = Scratch::new();
    let input = scratch.0.join("data/note.txt");
    std::fs::write(&input, "alpha\nbeta\n").expect("input file");
    let at = spelled(&input);

    let human = scratch.vibra(&["run", "src/app", "--", &at]);
    assert_eq!(human.status.code(), Some(0), "{human:?}");
    assert_eq!(
        String::from_utf8(human.stdout).expect("utf-8"),
        "> alpha\n> beta\n"
    );

    let machine = scratch.vibra(&["--format", "json", "run", "src/app", "--", &at]);
    assert_eq!(machine.status.code(), Some(0), "{machine:?}");
    let envelope = json(&machine);
    assert_eq!(envelope["result"], "@command.ok");
    assert_eq!(envelope["payload"]["stdout"], "> alpha\n> beta\n");
    let trace = envelope["payload"]["auditTrace"].as_array().expect("trace");
    let ops = trace
        .iter()
        .map(|event| event["op"].as_str().expect("op").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(ops, ["env.read-args", "fs.read-text", "io.stdout-text"]);
    // The path the audit shows is the path the argument named, read at run time.
    assert_eq!(
        trace[1]["args"][0],
        format!("(record kind: @wrapper type: @std.fs.path value: \"{at}\")")
    );
}

#[test]
fn a_different_argument_reads_a_different_file_of_the_same_binary_source() {
    let scratch = Scratch::new();
    let other = scratch.0.join("data/other.txt");
    std::fs::write(&other, "one\n").expect("input file");
    let run = scratch.vibra(&["run", "src/app", "--", &spelled(&other)]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(String::from_utf8(run.stdout).expect("utf-8"), "> one\n");
}

#[test]
fn a_function_ceiling_that_omits_a_performed_root_is_rejected_before_running() {
    let scratch = Scratch::new();
    scratch.edit(
        "src/app/main.vib",
        "(env.read fs.read io.stdout)",
        "(env.read io.stdout)",
    );
    let check = scratch.vibra(&["--format", "json", "check"]);
    assert_ne!(check.status.code(), Some(0), "{check:?}");
    let envelope = json(&check);
    let diagnostics = envelope["diagnostics"].as_array().expect("diagnostics");
    assert_eq!(diagnostics.len(), 1, "{envelope}");
    assert_eq!(diagnostics[0]["code"], "@effect.outside-ceiling");
    assert!(
        diagnostics[0]["notes"]
            .to_string()
            .contains("performs @std.fs.read via @std.fs.read.text"),
        "{envelope}"
    );
    // `run` refuses the same source and performs nothing.
    let run = scratch.vibra(&["--format", "json", "run", "src/app", "--", "unused"]);
    assert_ne!(run.status.code(), Some(0), "{run:?}");
    let performed = json(&run);
    assert_eq!(performed["payload"]["auditTrace"], serde_json::json!([]));
    assert_eq!(performed["payload"]["stdout"], "");
}

#[test]
fn a_target_array_that_omits_a_performed_root_is_rejected_before_running() {
    for (omitted, removed) in [
        ("@std.env.read", "@std.env.read "),
        ("@std.fs.read", "@std.fs.read "),
        ("@std.io.stdout", " @std.io.stdout"),
    ] {
        let scratch = Scratch::new();
        scratch.edit("project.vibon", removed, "");
        let check = scratch.vibra(&["--format", "json", "check"]);
        assert_ne!(check.status.code(), Some(0), "{omitted}: {check:?}");
        let envelope = json(&check);
        let diagnostics = envelope["diagnostics"].as_array().expect("diagnostics");
        assert_eq!(diagnostics.len(), 1, "{omitted}: {envelope}");
        assert_eq!(diagnostics[0]["code"], "@effect.outside-ceiling");
        assert_eq!(
            diagnostics[0]["primarySpan"]["sourceId"], "project.vibon",
            "{omitted}"
        );
        assert!(
            diagnostics[0]["related"]
                .to_string()
                .contains(&format!("performs {omitted} via")),
            "{omitted}: {envelope}"
        );
    }
}

#[test]
fn the_demo_sources_are_in_canonical_format() {
    let scratch = Scratch::new();
    for file in ["src/app/main.vib", "project.vibon"] {
        let printed = scratch.vibra(&["fmt", file]);
        assert_eq!(printed.status.code(), Some(0), "{file}: {printed:?}");
        let stored = std::fs::read_to_string(scratch.0.join("project").join(file))
            .expect("file");
        assert_eq!(
            String::from_utf8(printed.stdout).expect("utf-8"),
            stored,
            "{file}"
        );
    }
}
