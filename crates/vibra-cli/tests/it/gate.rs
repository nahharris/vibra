//! The release program of v0.0.2, `examples/gate/`, through the built
//! binary: check, format, and test it; run it under its grant bindings and
//! show the four behaviours of release gate item 2; reject the compile-time
//! constructions of the milestone's exit evidence on variants of it that
//! each test writes; and run it under a world. Expectations come from the
//! release gate, the effects chapter ("Effect ceilings", "Capability
//! values", "Target admission"), the programs chapter ("Targets", "Startup
//! order"), and the runtime chapter ("Recipes under a world").
//!
//! A run needs the directory driver, which exists on Windows only: elsewhere
//! each run row prints `NOT RUN: no directory driver` and the check and test
//! rows still pass.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use vibra_workspace::run::{CommandResult, Launch, run_target};
use vibra_workspace::{
    MemoryBudget, SourceSnapshot, decode_world, discover_project_at,
};

use crate::support::{TempDir, single_document};

/// The built `vibra`.
const VIBRA: &str = env!("CARGO_BIN_EXE_vibra");

/// The variable the `@metrics` recipe reads.
const METRICS: &str = "GATE_METRICS";

/// What the program shows for `a.txt` (2 lines) and `b.txt` (3 lines).
const TWO_NOTES_SHOWN: &str = "a: 2 lines\nb: 3 lines\n2 notes, 5 lines\n";

/// The notes of the run rows: `a.txt` with two lines, `b.txt` with three.
const TWO_NOTES: &[(&str, &str)] = &[("a.txt", "one\ntwo\n"), ("b.txt", "1\n2\n3")];

/// `examples/gate/`.
fn gate() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/gate")
}

/// The world case of the corpus that runs the release program.
fn world_case() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/cases/GRANT/V2-GRANT-release-optional-absent")
}

/// Every file under `directory`, as a path relative to it with `/`.
fn files(directory: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).expect("a readable directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let relative = path.strip_prefix(directory).expect("beneath it");
                found.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

/// A copy of `examples/gate/` with each `(file, old, new)` edit applied;
/// each `old` occurs exactly once in its file.
fn variant(label: &str, edits: &[(&str, &str, &str)]) -> TempDir {
    let directory = TempDir::new(&format!("gate-{label}"));
    for file in files(&gate()) {
        let text = fs::read_to_string(gate().join(&file)).expect("an example file");
        directory.write(&file, &text);
    }
    for (file, old, new) in edits {
        let text = directory.read(file);
        assert_eq!(
            text.matches(old).count(),
            1,
            "{label}: `{old}` occurs once in {file}"
        );
        directory.write(file, &text.replacen(old, new, 1));
    }
    directory
}

/// What one run of the binary produced.
struct Ran {
    code: i32,
    stdout: Vec<u8>,
}

/// Runs the binary in `directory` with `arguments`, with `GATE_METRICS` set
/// to `metrics` or absent, and an empty standard input.
fn vibra(directory: &Path, arguments: &[&str], metrics: Option<&Path>) -> Ran {
    let mut command = Command::new(VIBRA);
    command
        .args(arguments)
        .current_dir(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match metrics {
        Some(path) => command.env(METRICS, path),
        None => command.env_remove(METRICS),
    };
    let output = command.output().expect("the binary runs");
    Ran {
        code: output.status.code().expect("an exit status"),
        stdout: output.stdout,
    }
}

/// The one envelope of `vibra --workspace W --format json COMMAND...`, run
/// in `directory`, checked against the published schema, and the exit
/// status.
fn envelope(
    directory: &Path,
    workspace: &Path,
    command: &[&str],
    metrics: Option<&Path>,
) -> (i32, Value) {
    let workspace = workspace.to_str().expect("a Unicode workspace path");
    let mut line = vec!["--workspace", workspace, "--format", "json"];
    line.extend_from_slice(command);
    let ran = vibra(directory, &line, metrics);
    let text = String::from_utf8(ran.stdout).expect("the envelope is UTF-8");
    let document = single_document(&text);
    vibra_schema::validate_command_result(&document).unwrap_or_else(|violation| {
        panic!("the envelope of {line:?} violates its schema: {violation}")
    });
    (ran.code, document)
}

/// `vibra check` of `workspace`.
fn check(workspace: &Path) -> (i32, Value) {
    envelope(workspace, workspace, &["check"], None)
}

/// The errors of `document`: code, source, and primary span.
fn errors(document: &Value) -> Vec<(String, String, u64, u64)> {
    document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter(|diagnostic| diagnostic["level"] == "@error")
        .map(|diagnostic| {
            let span = &diagnostic["primarySpan"];
            (
                diagnostic["code"].as_str().expect("a code").to_owned(),
                span["sourceId"].as_str().expect("a source").to_owned(),
                span["start"].as_u64().expect("a start"),
                span["end"].as_u64().expect("an end"),
            )
        })
        .collect()
}

/// The one error of a rejected variant: `check` exits 1 with exactly one
/// error, which is returned.
fn one_error(workspace: &Path) -> (String, String, u64, u64) {
    let (code, document) = check(workspace);
    assert_eq!(code, 1, "{document}");
    assert_eq!(document["result"], "@command.diagnostics");
    let mut found = errors(&document);
    assert_eq!(found.len(), 1, "{document}");
    found.remove(0)
}

/// The byte range of the first `needle` in `text`, from `from`.
fn range_of(text: &str, needle: &str, from: usize) -> (u64, u64) {
    let at = text[from..]
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` is in the text"))
        + from;
    (at as u64, (at + needle.len()) as u64)
}

/// The byte range of the bracketed value that starts at the first `[` or
/// `{` after `field` in `text`.
fn value_of(text: &str, field: &str) -> (u64, u64) {
    let (_, after) = range_of(text, field, 0);
    let start = text[after as usize..]
        .find(['[', '{'])
        .expect("a bracketed value")
        + after as usize;
    let mut depth = 0usize;
    for (index, character) in text[start..].char_indices() {
        match character {
            '[' | '{' => depth += 1,
            ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return (start as u64, (start + index + 1) as u64);
                }
            }
            _ => {}
        }
    }
    panic!("`{field}` has a closed value")
}

/// A working directory for a run: `logs/` and `metrics/` empty, and
/// `notes/` holding `notes`.
fn scene(label: &str, notes: &[(&str, &str)]) -> TempDir {
    let directory = TempDir::new(&format!("gate-run-{label}"));
    fs::create_dir_all(directory.path().join("logs")).expect("the log directory");
    fs::create_dir_all(directory.path().join("metrics")).expect("the metric directory");
    fs::create_dir_all(directory.path().join("notes")).expect("the notes directory");
    for (name, text) in notes {
        directory.write(&format!("notes/{name}"), text);
    }
    directory
}

/// `scene/notes` as an argument.
fn notes_argument(scene: &TempDir) -> String {
    scene
        .path()
        .join("notes")
        .to_str()
        .expect("a Unicode path")
        .to_owned()
}

/// Whether the directory driver exists; prints `NOT RUN` for `row` when not.
fn has_directory_driver(row: &str) -> bool {
    if cfg!(windows) {
        return true;
    }
    println!("NOT RUN: {row}: no directory driver");
    false
}

/// The status of each processed leaf of a run payload's startup record.
fn statuses(document: &Value) -> Vec<(String, String)> {
    document["payload"]["startup"]["bindings"]
        .as_array()
        .expect("bindings")
        .iter()
        .map(|binding| {
            (
                binding["name"].as_str().expect("a name").to_owned(),
                binding["status"].as_str().expect("a status").to_owned(),
            )
        })
        .collect()
}

/// The leaves of the release program, in startup order, with `metrics` the
/// status of `@metrics`.
fn processed(metrics: &str) -> Vec<(String, String)> {
    [
        ("@args", "released"),
        ("@logs", "released"),
        ("@notes.list", "released"),
        ("@notes.reader", "released"),
        ("@out", "released"),
        ("@vars", "released"),
        ("@metrics", metrics),
    ]
    .iter()
    .map(|(name, status)| ((*name).to_owned(), (*status).to_owned()))
    .collect()
}

/// The operation and the arguments of each `@entry` event of a run payload.
fn entry_events(document: &Value) -> Vec<(String, Value)> {
    document["payload"]["auditTrace"]
        .as_array()
        .expect("entry events")
        .iter()
        .map(|event| {
            (
                event["op"].as_str().expect("an operation").to_owned(),
                event["args"].clone(),
            )
        })
        .collect()
}

#[test]
fn check_format_and_test() {
    let (code, document) = check(&gate());
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["status"], "passed");
    assert_eq!(document["diagnostics"], json!([]));
    for file in files(&gate()) {
        let (code, document) = envelope(&gate(), &gate(), &["fmt", &file], None);
        assert_eq!(code, 0, "{file}: {document}");
        assert_eq!(document["payload"]["changed"], false, "{file} is canonical");
    }
    let (code, document) = envelope(&gate(), &gate(), &["test"], None);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["status"], "passed");
    let tests = document["payload"]["tests"].as_array().expect("tests");
    assert!(!tests.is_empty());
    for item in tests {
        assert_eq!(item["result"], "@test.passed", "{item}");
    }
}

/// Release gate 1: "Its tests obtain every grant from `TestWorld`". A test
/// that takes anything takes exactly `{world: TestWorld}`.
#[test]
fn tests_take_only_the_world() {
    let mut seen = 0;
    for file in files(&gate().join("tests")) {
        let text =
            fs::read_to_string(gate().join("tests").join(&file)).expect("a test");
        let mut lines = text.lines();
        while let Some(line) = lines.next() {
            if !line.starts_with("test ") {
                continue;
            }
            seen += 1;
            for header in lines.by_ref() {
                let header = header.trim();
                if header == "do" {
                    break;
                }
                if header.starts_with("takes") {
                    assert_eq!(header, "takes {world: TestWorld}", "{file}: {line}");
                }
            }
        }
    }
    assert!(seen > 0, "the release program has tests");
}

/// Release gate 2: "Run it with `vibra run` under the target's grant
/// bindings".
#[test]
fn run() {
    if !has_directory_driver("run") {
        return;
    }
    let scene = scene("run", TWO_NOTES);
    let notes = notes_argument(&scene);
    let workspace = gate();
    let workspace = workspace.to_str().expect("a Unicode path");
    let ran = vibra(
        scene.path(),
        &["--workspace", workspace, "run", "--", &notes],
        None,
    );
    assert_eq!(ran.code, 0);
    assert_eq!(
        String::from_utf8(ran.stdout).expect("UTF-8"),
        TWO_NOTES_SHOWN
    );
    assert_eq!(
        scene.read("logs/gate.log"),
        "a.txt: 2 lines\nb.txt: 3 lines\n2 notes, 5 lines\nmetric: absent\n"
    );
}

/// Release gate 2: "a callback limited to the log grant cannot reach the
/// reader". The log callback awaits a read of the reader it closes over.
#[test]
fn showing_the_callback_cannot_read() {
    let old = "          return await DirWriteText.write(logs at text)";
    let new = "          let _ be await DirReadText.read(reader at) endlet\n\
               \x20         return await DirWriteText.write(logs at text)";
    let tree = variant("callback-reads", &[("src/gate/main.vib", old, new)]);
    let (code, source, start, end) = one_error(tree.path());
    assert_eq!(code, "@effect.outsideCeiling");
    assert_eq!(source, "src/gate/main.vib");
    let text = tree.read("src/gate/main.vib");
    let (callback, _) = range_of(&text, "      proc\n        takes {text: Str}", 0);
    let (_, closed) = range_of(&text, "      endproc", callback as usize);
    assert!(
        callback <= start && end <= closed,
        "in the callback: {start}..{end}"
    );
    assert_eq!(
        (start, end),
        range_of(&text, "await DirReadText.read(reader at)", 0)
    );
}

/// The exit evidence: "a callback whose row exceeds its parameter's row".
#[test]
fn showing_the_callback_row() {
    let old = "        performs {DirWriteText}\n        do\n          let Result.ok(value: at) be RelativePath([\"gate.log\"])";
    let new = "        performs {DirWriteText DirReadText}\n        do\n          let Result.ok(value: at) be RelativePath([\"gate.log\"])";
    let tree = variant("callback-row", &[("src/gate/main.vib", old, new)]);
    let (code, source, start, end) = one_error(tree.path());
    assert_eq!(code, "@type.argumentMismatch");
    assert_eq!(source, "src/gate/main.vib");
    let text = tree.read("src/gate/main.vib");
    let (call, _) =
        range_of(&text, "tally.record(found summary metricLine writeLog)", 0);
    assert_eq!((start, end), range_of(&text, "writeLog", call as usize));
}

/// `V2-GRANT-release-optional-absent`, through the binary: with
/// `GATE_METRICS` unset the `@metrics` leaf is refused, the entry runs, no
/// metric file is written, and standard output is the same as with a
/// metric directory (Q22).
#[test]
fn optional_absent() {
    if !has_directory_driver("optional absent") {
        return;
    }
    let scene = scene("absent", TWO_NOTES);
    let notes = notes_argument(&scene);
    let (code, document) =
        envelope(scene.path(), &gate(), &["run", "--", &notes], None);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["result"], "@command.ok");
    assert_eq!(statuses(&document), processed("refused"));
    assert_eq!(document["payload"]["stdout"], TWO_NOTES_SHOWN);
    assert!(files(&scene.path().join("metrics")).is_empty());
    let metrics = scene.path().join("metrics");
    let (code, present) = envelope(
        scene.path(),
        &gate(),
        &["run", "--", &notes],
        Some(&metrics),
    );
    assert_eq!(code, 0, "{present}");
    assert_eq!(present["payload"]["stdout"], document["payload"]["stdout"]);
}

/// 03: "An optional grant is an `Option`": with `GATE_METRICS` naming a
/// directory, the program writes its metric file there.
#[test]
fn showing_optional_present() {
    if !has_directory_driver("optional present") {
        return;
    }
    let scene = scene("present", TWO_NOTES);
    let notes = notes_argument(&scene);
    let metrics = scene.path().join("metrics");
    let (code, document) = envelope(
        scene.path(),
        &gate(),
        &["run", "--", &notes],
        Some(&metrics),
    );
    assert_eq!(code, 0, "{document}");
    assert_eq!(statuses(&document), processed("released"));
    assert_eq!(
        scene.read("metrics/gate.metric"),
        "mean lines per note: 2\n"
    );
    assert!(
        scene
            .read("logs/gate.log")
            .ends_with("metric: gate.metric\n")
    );
}

/// Release gate 2: "a required grant that cannot be acquired fails startup
/// before the entry runs".
#[test]
fn showing_required_fails_startup() {
    if !has_directory_driver("required fails startup") {
        return;
    }
    let scene = scene("required", &[]);
    let (code, document) = envelope(
        scene.path(),
        &gate(),
        &["run", "--", r"C:\no\such\dir"],
        None,
    );
    assert_eq!(code, 3, "{document}");
    assert_eq!(document["result"], "@command.operationalFailure");
    let startup = &document["payload"]["startup"];
    assert_eq!(startup["status"], "failed");
    assert_eq!(startup["failure"]["binding"], "@notes.list");
    assert_eq!(startup["failure"]["code"], "@grant.requiredUnavailable");
    let project =
        fs::read_to_string(gate().join("project.vibon")).expect("the project");
    let (start, end) = value_of(&project, "@notes.list:");
    let origin = &startup["failure"]["origin"];
    assert_eq!(origin["sourceId"], "project.vibon");
    assert_eq!(
        (origin["start"].as_u64(), origin["end"].as_u64()),
        (Some(start), Some(end))
    );
    let diagnostics = errors(&document);
    assert_eq!(
        diagnostics,
        [(
            "@grant.requiredUnavailable".to_owned(),
            "project.vibon".to_owned(),
            start,
            end
        )]
    );
    assert_eq!(document["payload"]["applicationCode"], Value::Null);
    assert_eq!(document["payload"]["stdout"], "");
    assert_eq!(document["payload"]["auditTrace"], json!([]));
    assert!(files(&scene.path().join("logs")).is_empty());
}

/// Release gate 2: "a request outside a grant's policy returns a typed
/// denial". The program tries every listed name; `c.md` fails the reader's
/// `.txt` rule before the driver, so it has no event.
#[test]
fn showing_typed_denial() {
    if !has_directory_driver("typed denial") {
        return;
    }
    let mut notes = TWO_NOTES.to_vec();
    notes.push(("c.md", "x\n"));
    let scene = scene("denial", &notes);
    let argument = notes_argument(&scene);
    let (code, document) =
        envelope(scene.path(), &gate(), &["run", "--", &argument], None);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["payload"]["stdout"], TWO_NOTES_SHOWN);
    assert_eq!(
        scene.read("logs/gate.log"),
        "a.txt: 2 lines\nb.txt: 3 lines\nc.md: denied\n2 notes, 5 lines\nmetric: absent\n"
    );
    let reads = entry_events(&document)
        .into_iter()
        .filter(|(op, _)| op == "fs.readText")
        .map(|(_, arguments)| arguments.to_string())
        .collect::<Vec<_>>();
    assert_eq!(reads.len(), 2, "{reads:?}");
    assert!(
        reads[0].contains("a.txt") && reads[1].contains("b.txt"),
        "{reads:?}"
    );
    assert!(!reads.iter().any(|read| read.contains("c.md")), "{reads:?}");
}

/// The exit evidence: "an exhausted shared limit". The reader is bounded at
/// two invocations; the third note's read is `limitExceeded`, a value the
/// program logs.
#[test]
fn showing_exhausted_shared_limit() {
    if !has_directory_driver("exhausted shared limit") {
        return;
    }
    let old = "              names: {kind: @suffix value: \".txt\"}\n            }\n          }\n          after: [@args]";
    let new = "              names: {kind: @suffix value: \".txt\"}\n            }\n          }\n          limits: {invocations: 2u64}\n          after: [@args]";
    let tree = variant("limit", &[("project.vibon", old, new)]);
    let mut notes = TWO_NOTES.to_vec();
    notes.push(("c.txt", "x\n"));
    let scene = scene("limit", &notes);
    let argument = notes_argument(&scene);
    let (code, document) =
        envelope(scene.path(), tree.path(), &["run", "--", &argument], None);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["payload"]["stdout"], TWO_NOTES_SHOWN);
    assert_eq!(
        scene.read("logs/gate.log"),
        "a.txt: 2 lines\nb.txt: 3 lines\nc.txt: limit exceeded\n2 notes, 5 lines\nmetric: absent\n"
    );
    let reads = entry_events(&document)
        .into_iter()
        .filter(|(op, _)| op == "fs.readText")
        .count();
    assert_eq!(reads, 2);
}

/// The exit evidence: "an operation outside a ceiling". `listNotes` awaits
/// `DirList.entries` without `DirList` in its row.
#[test]
fn rejection_operation_outside_a_ceiling() {
    let old = "  returns names: Result(List(Str) FsError)\n  performs {DirList}\n";
    let new = "  returns names: Result(List(Str) FsError)\n";
    let tree = variant("outside-ceiling", &[("src/gate/tally.vib", old, new)]);
    let (code, source, start, end) = one_error(tree.path());
    assert_eq!(code, "@effect.outsideCeiling");
    assert_eq!(source, "src/gate/tally.vib");
    let text = tree.read("src/gate/tally.vib");
    assert_eq!(
        (start, end),
        range_of(&text, "await DirList.entries(list here)", 0)
    );
}

/// The exit evidence: "an entry row outside the target's admission
/// ceiling". `effects` omits `StdoutText`, which the entry performs.
#[test]
fn rejection_entry_row_outside_the_admission_ceiling() {
    let old = "        @std.io.StdoutText\n      ]";
    let new = "      ]";
    let tree = variant("admission", &[("project.vibon", old, new)]);
    let (code, source, start, end) = one_error(tree.path());
    assert_eq!(code, "@effect.outsideCeiling");
    assert_eq!(source, "project.vibon");
    assert_eq!(
        (start, end),
        value_of(&tree.read("project.vibon"), "effects:")
    );
}

/// The exit evidence: "a missing binding". The `@logs` recipe is removed.
#[test]
fn rejection_missing_binding() {
    let project =
        fs::read_to_string(gate().join("project.vibon")).expect("the project");
    let (start, end) = value_of(&project, "@logs:");
    let (key, _) = range_of(&project, "        @logs:", 0);
    let recipe = &project[key as usize..end as usize + 1];
    assert!(start > key);
    let tree = variant("missing-binding", &[("project.vibon", recipe, "")]);
    let (code, source, start, end) = one_error(tree.path());
    assert_eq!(code, "@grant.invalidBinding");
    assert_eq!(source, "project.vibon");
    assert_eq!(
        (start, end),
        value_of(&tree.read("project.vibon"), "grants:")
    );
}

/// The exit evidence: "a cyclic binding". `@logs` is after `@metrics` and
/// `@metrics` after `@logs`.
#[test]
fn rejection_cyclic_binding() {
    let logs_old = "names: {kind: @exact names: [\"gate.log\"]}\n            }\n          }\n          after: []";
    let logs_new = "names: {kind: @exact names: [\"gate.log\"]}\n            }\n          }\n          after: [@metrics]";
    let tree = variant(
        "cycle",
        &[
            ("project.vibon", logs_old, logs_new),
            ("project.vibon", "after: [@vars]", "after: [@vars @logs]"),
        ],
    );
    let (code, source, start, end) = one_error(tree.path());
    assert_eq!(code, "@grant.bindingCycle");
    assert_eq!(source, "project.vibon");
    let text = tree.read("project.vibon");
    let (list, _) = range_of(&text, "after: [@vars @logs]", 0);
    assert_eq!((start, end), range_of(&text, "@logs", list as usize));
}

/// The exit evidence: "No grant is obtainable from data". A test builds a
/// record of the fields of a grant's projection and passes it as `logs`.
#[test]
fn no_grant_from_data() {
    let test = "use @std.assert enduse\n\n\
        use @std.test.TestWorld enduse\n\n\
        use @std.env.EnvReadArgs enduse\n\n\
        use @std.fs.DirList enduse\n\n\
        use @std.fs.DirReadText enduse\n\n\
        use @std.fs.DirWriteText enduse\n\n\
        use @std.io.StdoutText enduse\n\n\
        use @gate.main enduse\n\n\
        test \"a record shaped like a grant\"\n  \
        takes {world: TestWorld}\n  \
        performs {TestWorld EnvReadArgs DirList DirReadText DirWriteText StdoutText}\n  \
        do\n    \
        let {list: list readText: reader} be await TestWorld.directory(world) endlet\n    \
        let {variables: vars arguments: args} be await TestWorld.environment(world) endlet\n    \
        let {stdoutText: out} be await TestWorld.streams(world) endlet\n    \
        let forged be {issuedBy: {kind: @binding leaf: @logs} policy: \"\" root: 2u64} endlet\n    \
        let _ be await main.main(args vars {list: list reader: reader} forged Option.none() out) endlet\n\
        endtest\n";
    let tree = variant("grant-from-data", &[]);
    tree.write("tests/forged.vib", test);
    let (code, source, start, end) = one_error(tree.path());
    assert_eq!(code, "@type.argumentMismatch");
    assert_eq!(source, "tests/forged.vib");
    let (argument, _) = range_of(test, "forged Option.none()", 0);
    assert_eq!((start, end), (argument, argument + 6));
}

/// The exit evidence: no grant is obtainable "from a row alone". A test
/// whose row names `DirReadText` and that takes no world has no reader to
/// name.
#[test]
fn no_grant_from_a_row() {
    let test = "use @std.fs.DirReadText enduse\n\n\
        use @std.fs.RelativePath enduse\n\n\
        test \"a row alone\"\n  \
        performs {DirReadText}\n  \
        do\n    \
        let Result.ok(value: at) be RelativePath([\"a.txt\"]) endlet\n    \
        let _ be await DirReadText.read(reader at) endlet\n\
        endtest\n";
    let tree = variant("grant-from-row", &[]);
    tree.write("tests/row.vib", test);
    let (code, source, start, end) = one_error(tree.path());
    assert_eq!(code, "@name.unknownSymbol");
    assert_eq!(source, "tests/row.vib");
    assert_eq!((start, end), range_of(test, "reader", 0));
}

/// The world of the world case: the notes of the typed-denial showing, a
/// log directory, and no `GATE_METRICS`.
fn world_text() -> String {
    fs::read_to_string(world_case().join("world.vibon")).expect("the case's world")
}

/// 06 "Recipes under a world" and Q17: the release program run under a
/// world holding the same files has the same standard output, application
/// code, and startup statuses as the real run.
#[test]
fn the_same_program_under_a_world() {
    let world = decode_world(None, world_text().as_bytes()).expect("the world decodes");
    let project = discover_project_at(gate()).expect("the release program");
    let snapshot = SourceSnapshot::capture(project).expect("its snapshot");
    let in_world = run_target(
        &snapshot,
        None,
        Launch::World(&world),
        MemoryBudget::DEFAULT,
    );
    assert_eq!(
        in_world.result,
        CommandResult::Ok,
        "{:?}",
        in_world.diagnostics
    );
    assert_eq!(in_world.application_code, Some(0));
    assert_eq!(String::from_utf8_lossy(&in_world.stdout), TWO_NOTES_SHOWN);
    if !has_directory_driver("the same program under a world, system half") {
        return;
    }
    let mut notes = TWO_NOTES.to_vec();
    notes.push(("c.md", "x\n"));
    let scene = scene("world", &notes);
    let argument = notes_argument(&scene);
    let (code, document) =
        envelope(scene.path(), &gate(), &["run", "--", &argument], None);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["payload"]["stdout"], TWO_NOTES_SHOWN);
    assert_eq!(statuses(&document), processed("refused"));
    let world_statuses = in_world
        .startup
        .bindings
        .iter()
        .map(|binding| {
            (
                binding.name.to_string(),
                binding.status.spelling().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(world_statuses, processed("refused"));
}

/// The world case's tree is the release program: each of its files is the
/// file of `examples/gate/` at the same path, and it holds every file of the
/// program outside the two `tests/` directories.
#[test]
fn the_world_case_runs_the_release_program() {
    let tree = world_case().join("tree");
    let held = files(&tree);
    let program = files(&gate())
        .into_iter()
        .filter(|file| !file.starts_with("tests/") && !file.contains("/tests/"))
        .collect::<Vec<_>>();
    assert_eq!(held, program);
    for file in held {
        assert_eq!(
            fs::read(tree.join(&file)).expect("a case file"),
            fs::read(gate().join(&file)).expect("a program file"),
            "{file}"
        );
    }
}
