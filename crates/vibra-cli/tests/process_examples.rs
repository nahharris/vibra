//! The checked-in examples through the real binary.
//!
//! Every project under `examples/` is checked, tested, and held to canonical
//! format by one loop; a new example fails it until its row is added. The
//! examples that run then have their own output asserted: the catalog and the
//! configuration example finish with `@ok`, the console example writes the
//! process's standard streams, and the demo reads the path that
//! `vibra run TARGET -- ARG...` supplies. The demo is also rejected, before it
//! runs, when its function ceiling omits a performed root; the same rejection by
//! the target array is the corpus cases `V1-EFFECT-host-target-array-omits-*`.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use vibra_workspace::WorkspaceSnapshot;

/// Each example and the tests it holds.
const EXAMPLES: [(&str, u64); 4] = [
    ("m3-catalog", 8),
    ("m4-console", 1),
    ("m4-demo", 0),
    ("stage-3a-config", 6),
];

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .canonicalize()
        .expect("the examples")
}

fn example(name: &str) -> PathBuf {
    examples().join(name)
}

/// Runs the binary over `workspace` with `input` on its standard input.
fn vibra_in(workspace: &Path, arguments: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_vibra"))
        .args(["--workspace", &workspace.to_string_lossy()])
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

fn vibra(workspace: &Path, arguments: &[&str]) -> Output {
    vibra_in(workspace, arguments, "")
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("json envelope")
}

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

/// A scratch directory under the system temporary directory, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        // A counter, not the clock, makes the name unique within the process.
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "vibra-examples-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("scratch");
        Self(path)
    }

    /// A copy of the example `name` in this directory.
    fn copy_of(&self, name: &str) -> PathBuf {
        let project = self.0.join(name);
        copy_directory(&example(name), &project);
        project
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

/// Every `.vib` and `.vibon` file below `directory`, relative to `base`.
fn sources(base: &Path, directory: &Path, found: &mut Vec<String>) {
    for entry in std::fs::read_dir(directory).expect("a directory") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            sources(base, &path, found);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "vib" || extension == "vibon")
        {
            found.push(spelled(path.strip_prefix(base).expect("below the base")));
        }
    }
}

#[test]
fn every_example_checks_passes_its_tests_and_is_in_canonical_format() {
    let mut found = std::fs::read_dir(examples())
        .expect("the examples")
        .map(|entry| entry.expect("an entry").path())
        .filter(|path| path.join("project.vibon").is_file())
        .map(|path| {
            path.file_name()
                .expect("a name")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    found.sort();
    let listed = EXAMPLES.map(|(name, _)| name);
    assert_eq!(found, listed, "add a row to EXAMPLES for a new example");

    for (name, tests) in EXAMPLES {
        let project = example(name);
        let check = vibra(&project, &["--format", "json", "check"]);
        assert_eq!(check.status.code(), Some(0), "{name}: {check:?}");
        assert_eq!(json(&check)["payload"]["accepted"], true, "{name}");

        let test = vibra(&project, &["--format", "json", "test"]);
        assert_eq!(test.status.code(), Some(0), "{name}: {test:?}");
        let test = json(&test);
        assert_eq!(test["result"], "@command.ok", "{name}");
        assert_eq!(test["payload"]["selected"], tests, "{name}");
        assert_eq!(test["payload"]["passed"], tests, "{name}");

        let mut files = Vec::new();
        sources(&project, &project, &mut files);
        assert!(!files.is_empty(), "{name} has no sources");
        for file in files {
            let written = std::fs::read_to_string(project.join(&file)).expect("source");
            let printed = vibra(&project, &["fmt", &file]);
            assert_eq!(printed.status.code(), Some(0), "{name}/{file}: {printed:?}");
            // Compare without the host's line endings: a checkout may rewrite them.
            let formatted = String::from_utf8(printed.stdout).expect("utf-8");
            assert_eq!(
                formatted.replace("\r\n", "\n"),
                written.replace("\r\n", "\n"),
                "{name}/{file}"
            );
        }
    }
}

#[test]
fn the_catalog_and_the_configuration_example_run_to_ok() {
    for name in ["m3-catalog", "stage-3a-config"] {
        let run = vibra(&example(name), &["--format", "json", "run", "src/app"]);
        assert_eq!(run.status.code(), Some(0), "{name}: {run:?}");
        let run = json(&run);
        assert_eq!(run["result"], "@command.ok", "{name}");
        assert!(
            run["payload"]["programResult"]
                .as_str()
                .is_some_and(|result| result.contains("variant: @ok")),
            "{name}: {run}"
        );
    }
}

#[test]
fn the_catalog_index_names_every_implementation() {
    let workspace =
        WorkspaceSnapshot::load_confined(example("m3-catalog")).expect("workspace");
    let index =
        vibra_workspace::index::index_with_embedded_stdlib(&workspace).expect("index");
    let again =
        vibra_workspace::index::index_with_embedded_stdlib(&workspace).expect("index");
    assert_eq!(index.canonical_vibon(), again.canonical_vibon());

    let blocks = index
        .implementations()
        .iter()
        .map(|block| (block.receiver(), block.interface()))
        .collect::<Vec<_>>();
    for (receiver, interface) in [
        ("@catalog.stock.sku", "@std.core.ordered"),
        (
            "@catalog.stock.sku",
            "(record type: @std.core.from arguments: (array @u32))",
        ),
        ("@catalog.stock.part", "@catalog.stock.priced"),
        ("@catalog.stock.service", "@catalog.stock.priced"),
        (
            "(record type: @catalog.stock.stack arguments: (array (record type: @param name: @t)))",
            "(record type: @std.iter.iter arguments: (array (record type: @param name: @t)))",
        ),
    ] {
        assert!(
            blocks.contains(&(receiver, interface)),
            "{receiver} {interface}: {blocks:?}"
        );
    }

    let total = index
        .declarations()
        .iter()
        .find(|declaration| declaration.id() == "catalog.stock.total")
        .expect("total");
    assert_eq!(total.module(), "catalog.stock");
}

#[test]
fn a_position_query_in_the_catalog_reports_the_selected_conversion() {
    const SOURCE: &str = "src/catalog/stock.vib";
    let workspace =
        WorkspaceSnapshot::load_confined(example("m3-catalog")).expect("workspace");
    let source = std::fs::read_to_string(example("m3-catalog").join(SOURCE))
        .expect("source")
        .replace("\r\n", "\n");
    let query = workspace
        .query_position_with_embedded_stdlib(
            SOURCE,
            source.find("(from.convert number)").expect("conversion"),
        )
        .expect("query");
    let application = query.application().value().expect("application");
    let dispatch = application.dispatch().expect("dispatch");
    assert_eq!(dispatch.interface(), "std.core.from");
    assert_eq!(dispatch.receiver().name(), "catalog.stock.sku");
    assert!(dispatch.destination());
}

#[test]
fn the_console_test_writes_to_its_audit_trace_and_not_the_process() {
    let test = vibra(&example("m4-console"), &["--format", "json", "test"]);
    let test = json(&test);
    let events = test["payload"]["tests"][0]["auditTrace"]
        .as_array()
        .expect("trace");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["op"], "io.stdout-text");
    assert_eq!(events[0]["args"][0], "\"a\"");
}

#[test]
fn json_run_captures_the_streams_and_reports_the_events_as_objects() {
    let run = vibra_in(
        &example("m4-console"),
        &["--format", "json", "run", "src/app"],
        "hello\n",
    );
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    let envelope = json(&run);
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
    let run = vibra_in(&example("m4-console"), &["run", "src/app"], "hello\n");
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(
        String::from_utf8(run.stdout).expect("utf-8"),
        "hello\none\ntwo\nthree\n"
    );
}

#[test]
fn the_demo_reads_the_path_it_is_given_and_writes_the_quoted_content() {
    let scratch = Scratch::new();
    let input = scratch.0.join("note.txt");
    std::fs::write(&input, "alpha\nbeta\n").expect("input file");
    let at = spelled(&input);
    let demo = example("m4-demo");

    let human = vibra(&demo, &["run", "src/app", "--", &at]);
    assert_eq!(human.status.code(), Some(0), "{human:?}");
    assert_eq!(
        String::from_utf8(human.stdout).expect("utf-8"),
        "> alpha\n> beta\n"
    );

    let machine = vibra(&demo, &["--format", "json", "run", "src/app", "--", &at]);
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

    // Another argument reads another file with the same binary source.
    let other = scratch.0.join("other.txt");
    std::fs::write(&other, "one\n").expect("input file");
    let run = vibra(&demo, &["run", "src/app", "--", &spelled(&other)]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(String::from_utf8(run.stdout).expect("utf-8"), "> one\n");
}

#[test]
fn a_function_ceiling_that_omits_a_performed_root_is_rejected_before_running() {
    for (omitted, ceiling, witness) in [
        (
            "@std.fs.read",
            "(env.read io.stdout)",
            "performs @std.fs.read via @std.fs.read.text",
        ),
        (
            "@std.env.read",
            "(fs.read io.stdout)",
            "performs @std.env.read via @std.env.read.args",
        ),
        (
            "@std.io.stdout",
            "(env.read fs.read)",
            "performs @std.io.stdout via @std.io.stdout.text",
        ),
    ] {
        let scratch = Scratch::new();
        let project = scratch.copy_of("m4-demo");
        let path = project.join("src/app/main.vib");
        let text = std::fs::read_to_string(&path).expect("file");
        let from = "(env.read fs.read io.stdout)";
        assert!(text.contains(from), "main.vib lacks {from:?}");
        std::fs::write(&path, text.replacen(from, ceiling, 1)).expect("edit");

        let check = vibra(&project, &["--format", "json", "check"]);
        assert_ne!(check.status.code(), Some(0), "{omitted}: {check:?}");
        let envelope = json(&check);
        let diagnostics = envelope["diagnostics"].as_array().expect("diagnostics");
        assert_eq!(diagnostics.len(), 1, "{omitted}: {envelope}");
        assert_eq!(diagnostics[0]["code"], "@effect.outside-ceiling");
        assert_eq!(
            diagnostics[0]["primarySpan"]["sourceId"], "src/app/main.vib",
            "{omitted}"
        );
        assert_eq!(
            diagnostics[0]["notes"],
            serde_json::json!([witness]),
            "{omitted}"
        );
        // `run` refuses the same source and performs nothing.
        let run = vibra(
            &project,
            &["--format", "json", "run", "src/app", "--", "unused"],
        );
        assert_ne!(run.status.code(), Some(0), "{omitted}: {run:?}");
        let performed = json(&run);
        assert_eq!(performed["payload"]["auditTrace"], serde_json::json!([]));
        assert_eq!(performed["payload"]["stdout"], "");
    }
}
