//! `vibra run [TARGET] [-- ARG...]` through the built binary, each in a
//! temporary workspace. The trees are those of step 29's corpus cases, read
//! from `conformance/cases`, and a few written here; expectations come from
//! the tooling chapter ("Program arguments, execution and evaluation",
//! "Versioned envelope and results", "Command payloads") and the runtime
//! chapter ("Entry outcome"). A row that needs the directory driver runs on
//! Windows only and prints `NOT RUN` elsewhere.
//!
//! The trap row is the unit test `run::tests::trap_is_exit_5_with_its_trap_record`:
//! no trap follows from user data, and the built binary has no driver to
//! fault.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use crate::support::{TempDir, single_document};

/// The built `vibra`.
const VIBRA: &str = env!("CARGO_BIN_EXE_vibra");

/// The corpus of the repository.
fn cases() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/cases")
}

/// Copies every file under `from` into `to`, keeping relative paths.
fn copy_tree(from: &Path, to: &Path) {
    let mut pending = vec![from.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).expect("a readable case tree") {
            let path = entry.expect("an entry").path();
            let relative = path.strip_prefix(from).expect("beneath the tree");
            if path.is_dir() {
                fs::create_dir_all(to.join(relative)).expect("a directory");
                pending.push(path);
            } else {
                fs::copy(&path, to.join(relative)).expect("a copied file");
            }
        }
    }
}

/// A temporary workspace holding the tree of the corpus case `id`.
fn case_tree(id: &str) -> TempDir {
    let prefix = id
        .strip_prefix("V2-")
        .and_then(|rest| rest.split('-').next())
        .expect("a case id");
    let directory = TempDir::new(&id.to_lowercase());
    copy_tree(
        &cases().join(prefix).join(id).join("tree"),
        directory.path(),
    );
    directory
}

/// What one run of the binary produced.
struct Ran {
    code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Runs the binary in `directory` with `arguments`, feeding `stdin`.
fn vibra(directory: &Path, arguments: &[&str], stdin: &[u8]) -> Ran {
    let mut child = Command::new(VIBRA)
        .args(arguments)
        .current_dir(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary starts");
    let mut input = child.stdin.take().expect("a piped standard input");
    input.write_all(stdin).expect("standard input is written");
    drop(input);
    let output = child.wait_with_output().expect("the binary ends");
    Ran {
        code: output.status.code().expect("an exit status"),
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

/// The one envelope of `vibra --format json run` with `arguments`, checked
/// against the published schema, and the exit status.
fn run_json(directory: &Path, arguments: &[&str]) -> (i32, Value) {
    let mut line = vec!["--format", "json", "run"];
    line.extend_from_slice(arguments);
    let ran = vibra(directory, &line, b"");
    let text = String::from_utf8(ran.stdout).expect("the envelope is UTF-8");
    let document = single_document(&text);
    vibra_schema::validate_command_result(&document).unwrap_or_else(|violation| {
        panic!("the envelope of {line:?} violates its schema: {violation}")
    });
    (ran.code, document)
}

fn codes(document: &Value) -> Vec<String> {
    document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().expect("a code").to_owned())
        .collect()
}

fn assert_invalid_input(code: i32, document: &Value) {
    assert_eq!(code, 2, "{document}");
    assert_eq!(document["result"], "@command.invalidInput");
    assert_eq!(document["status"], "unknown");
    assert_eq!(document["workspaceRevision"], Value::Null);
    assert_eq!(document["diagnostics"], json!([]));
    assert_eq!(document["payload"], json!({}));
}

/// An entry that writes each of its arguments followed by a line feed.
const ECHO: &str = "use @std.env.EnvReadArgs enduse\n\nuse @std.io.StdoutText enduse\n\n\
    use @std.process.ExitCode enduse\n\n\
    fn lines\n  takes {items: List(Str)}\n  returns text: Str\n  do\n    \
    return Iter.fold(\n      items\n      \"\"\n      fn\n        \
    takes {text: Str item: Str}\n        returns joined: Str\n        do\n          \
    return Str.concat(Str.concat(text item) \"\\n\")\n      endfn\n    )\nendfn\n\n\
    proc main\n  takes {args: EnvReadArgs out: StdoutText}\n  returns code: ExitCode\n  \
    performs {EnvReadArgs StdoutText}\n  do\n    \
    let Result.ok(value: items) be await EnvReadArgs.read(args) else\n      \
    return ExitCode(1u8)\n    endlet\n    \
    let _ be await StdoutText.write(out lines(items)) endlet\n    \
    return ExitCode(0u8)\nendproc\n";

/// The binary target `name` rooted at `src/{name}` running [`ECHO`].
fn echo_target(name: &str) -> String {
    format!(
        "{{name: @{name} kind: @bin root: \"src/{name}\" entry: @{name}.main.main \
         effects: [@std.env.EnvReadArgs @std.io.StdoutText] grants: [\
         @args: {{provider: @host.env.args resource: {{kind: @processArguments}} after: []}} \
         @out: {{provider: @host.io.stdout resource: {{kind: @processStream}} after: []}}]}}"
    )
}

fn echo_project(targets: &[&str]) -> TempDir {
    let records = targets
        .iter()
        .map(|name| echo_target(name))
        .collect::<Vec<_>>()
        .join(" ");
    let document = format!(
        "{{format: @project.v2 package: {{name: \"echo\" version: \"0.1.0\"}} \
         targets: [{records}] dependencies: [:]}}"
    );
    let directory = TempDir::project("run-echo", &document, &[]);
    for name in targets {
        directory.write(&format!("src/{name}/main.vib"), ECHO);
    }
    directory
}

#[test]
fn hello_human() {
    let tree = case_tree("V2-RUNTIME-run-hello");
    let ran = vibra(tree.path(), &["run"], b"");
    assert_eq!(ran.code, 0, "{}", String::from_utf8_lossy(&ran.stderr));
    assert_eq!(ran.stdout, b"hello\n");
    assert_eq!(ran.stderr, b"");
}

#[test]
fn hello_json() {
    let tree = case_tree("V2-RUNTIME-run-hello");
    let (code, document) = run_json(tree.path(), &[]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["command"], "run");
    assert_eq!(document["result"], "@command.ok");
    assert_eq!(document["status"], "passed");
    assert_eq!(document["diagnostics"], json!([]));
    let revision = document["workspaceRevision"].as_str().expect("a revision");
    let payload = &document["payload"];
    assert_eq!(payload["target"], "@hello");
    assert_eq!(payload["stdout"], "hello\n");
    assert_eq!(payload["stderr"], "");
    assert_eq!(payload["applicationCode"], 0);
    assert_eq!(payload["trap"], Value::Null);
    assert!(payload["programResult"].is_string(), "{payload}");
    let events = payload["auditTrace"].as_array().expect("events");
    assert_eq!(events.len(), 1, "{payload}");
    assert_eq!(events[0]["kind"], "@host");
    assert_eq!(events[0]["phase"], "@entry");
    assert_eq!(events[0]["op"], "io.stdoutText");
    assert_eq!(events[0]["invocation"], 2);
    assert_eq!(events[0]["parent"], 1);
    assert_eq!(events[0]["args"], json!(["\"hello\\n\""]));
    // The system driver of the stream, not a world's.
    let driver = json!({"provider": "@host.io.stdout", "version": "0.0.2"});
    assert_eq!(events[0]["driver"], driver);
    assert_eq!(payload["drivers"], json!([driver]));
    assert_eq!(payload["policies"], json!({}));
    let startup = &payload["startup"];
    assert_eq!(startup["status"], "acquired");
    assert_eq!(startup["failure"], Value::Null);
    let bindings = startup["bindings"].as_array().expect("bindings");
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0]["name"], "@out");
    assert_eq!(bindings[0]["status"], "released");
    assert_eq!(bindings[0]["providerIdentity"], driver);
    let startup_events = startup["auditTrace"].as_array().expect("startup events");
    assert_eq!(startup_events.len(), 1);
    assert_eq!(startup_events[0]["kind"], "@startup");
    assert_eq!(startup_events[0]["status"], "@acquired");
    // The revision is the snapshot's, the one the trace of the case names.
    let expected = fs::read_to_string(
        cases().join("RUNTIME/V2-RUNTIME-run-hello/target-run.vibon"),
    )
    .expect("the case's observation");
    assert!(expected.contains(revision), "{revision}");
    println!("{document}");
}

#[test]
fn non_zero_code() {
    let tree = case_tree("V2-RUNTIME-run-nonzero");
    let (code, document) = run_json(tree.path(), &[]);
    assert_eq!(code, 6, "{document}");
    assert_eq!(document["result"], "@command.programFailed");
    assert_eq!(document["status"], "unknown");
    assert_eq!(document["payload"]["applicationCode"], 3);
    assert_eq!(document["payload"]["trap"], Value::Null);
    assert_eq!(document["diagnostics"], json!([]));
    let human = vibra(tree.path(), &["run"], b"");
    assert_eq!(human.code, 6);
}

#[test]
fn static_error() {
    let tree = case_tree("V2-RUNTIME-run-static-error");
    let (code, document) = run_json(tree.path(), &[]);
    assert_eq!(code, 1, "{document}");
    assert_eq!(document["result"], "@command.diagnostics");
    assert_eq!(document["status"], "refuted");
    assert_eq!(codes(&document), ["@type.argumentMismatch"]);
    let payload = &document["payload"];
    assert_eq!(payload["startup"]["status"], "notStarted");
    assert_eq!(payload["startup"]["bindings"], json!([]));
    assert_eq!(payload["programResult"], Value::Null);
    assert_eq!(payload["applicationCode"], Value::Null);
    assert_eq!(payload["auditTrace"], json!([]));
    // Human format: the diagnostic on standard error, nothing on standard
    // output.
    let human = vibra(tree.path(), &["run"], b"");
    assert_eq!(human.code, 1);
    assert_eq!(human.stdout, b"");
    let stderr = String::from_utf8(human.stderr).expect("UTF-8");
    assert!(stderr.contains("@type.argumentMismatch"), "{stderr}");
}

#[test]
fn required_refused() {
    if !cfg!(windows) {
        println!("NOT RUN: the directory driver exists on Windows only");
        return;
    }
    let tree = case_tree("V2-RUNTIME-run-reads-note");
    let missing = tree.path().join("missing");
    let missing = missing.to_str().expect("a Unicode path");
    let (code, document) = run_json(tree.path(), &["--", missing]);
    assert_eq!(code, 3, "{document}");
    assert_eq!(document["result"], "@command.operationalFailure");
    assert_eq!(document["status"], "unknown");
    let payload = &document["payload"];
    assert_eq!(payload["programResult"], Value::Null);
    assert_eq!(payload["applicationCode"], Value::Null);
    let startup = &payload["startup"];
    assert_eq!(startup["status"], "failed");
    assert_eq!(startup["failure"]["binding"], "@notes");
    assert_eq!(startup["failure"]["code"], "@grant.requiredUnavailable");
    assert_eq!(startup["failure"]["origin"]["sourceId"], "project.vibon");
    // The read of `@args` completed before `@notes` was refused, and is kept.
    let kinds = startup["auditTrace"]
        .as_array()
        .expect("startup events")
        .iter()
        .map(|event| (event["kind"].clone(), event["phase"].clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            (json!("@startup"), Value::Null),
            (json!("@host"), json!("@startup")),
            (json!("@startup"), Value::Null),
        ]
    );
    assert!(codes(&document).contains(&"@grant.requiredUnavailable".to_owned()));
}

#[test]
fn exhaustion() {
    let tree = case_tree("V2-RUNTIME-run-exhausted");
    let (code, document) = run_json(tree.path(), &[]);
    assert_eq!(code, 3, "{document}");
    assert_eq!(document["result"], "@command.operationalFailure");
    assert_eq!(document["status"], "unknown");
    assert_eq!(
        document["limits"],
        json!({"profile": "m1Default", "reached": ["memory"], "complete": false})
    );
    assert_eq!(codes(&document), ["@runtime.memoryExhausted"]);
    let payload = &document["payload"];
    assert_eq!(payload["applicationCode"], Value::Null);
    assert_eq!(payload["stdout"], "hello\n");
    assert_eq!(payload["startup"]["bindings"][0]["status"], "released");
}

fn echoed(ran: &Ran) -> String {
    assert_eq!(ran.code, 0, "{}", String::from_utf8_lossy(&ran.stderr));
    String::from_utf8(ran.stdout.clone()).expect("UTF-8")
}

#[test]
fn arguments_after_the_separator() {
    let tree = echo_project(&["app"]);
    let ran = vibra(tree.path(), &["run", "--", "a", "--", "-b"], b"");
    assert_eq!(echoed(&ran), "a\n--\n-b\n");
}

#[test]
fn separator_with_nothing_after_it() {
    let tree = echo_project(&["app"]);
    assert_eq!(echoed(&vibra(tree.path(), &["run", "--"], b"")), "");
    // The read itself: an empty list, not an absent one.
    let (code, document) = run_json(tree.path(), &["--"]);
    assert_eq!(code, 0, "{document}");
    let read = &document["payload"]["auditTrace"][0];
    assert_eq!(read["op"], "env.readArgs");
    assert!(
        read["result"]
            .as_str()
            .expect("the result's text")
            .contains("payload: {value: {kind: @list values: []}}"),
        "{read}"
    );
}

#[test]
fn target_then_separator() {
    let tree = echo_project(&["app", "tool"]);
    let ran = vibra(tree.path(), &["run", "@tool", "--", "x"], b"");
    assert_eq!(echoed(&ran), "x\n");
    let (code, document) = run_json(tree.path(), &["@tool", "--", "x"]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["payload"]["target"], "@tool");
    // Two binary targets and no selector select nothing.
    let (code, document) = run_json(tree.path(), &["--", "x"]);
    assert_invalid_input(code, &document);
}

#[test]
fn separator_on_another_command() {
    let tree = echo_project(&["app"]);
    let ran = vibra(tree.path(), &["--format", "json", "check", "--", "x"], b"");
    let document = single_document(&String::from_utf8(ran.stdout).expect("UTF-8"));
    vibra_schema::validate_command_result(&document).expect("a valid envelope");
    assert_eq!(document["command"], "check");
    assert_invalid_input(ran.code, &document);
}

#[test]
fn unknown_target() {
    let tree = echo_project(&["app"]);
    for target in ["@nope", "nope"] {
        let (code, document) = run_json(tree.path(), &[target]);
        assert_invalid_input(code, &document);
        assert_eq!(document["command"], "run");
    }
    // A library is not a target `run` can select.
    let library = TempDir::project("run-library", crate::support::LIBRARY, &[]);
    library.write(
        "src/app/main.vib",
        "fn f\n  returns r: I32\n  do\n    return 1i32\nendfn\n",
    );
    let (code, document) = run_json(library.path(), &["@app"]);
    assert_invalid_input(code, &document);
}

#[test]
fn stdin_passes_through() {
    let tree = case_tree("V2-RUNTIME-run-stdin");
    let ran = vibra(tree.path(), &["run"], b"x\n");
    assert_eq!(ran.code, 0, "{}", String::from_utf8_lossy(&ran.stderr));
    assert_eq!(ran.stdout, b"x\n");
}

const BYTE_255_PROJECT: &str = "{format: @project.v2 package: {name: \"bytes\" version: \"0.1.0\"} \
    targets: [{name: @app kind: @bin root: \"src/app\" entry: @app.main.main \
    effects: [@std.io.StdoutBytes] grants: [@out: {provider: @host.io.stdout \
    resource: {kind: @processStream} after: []}]}] dependencies: [:]}";

const BYTE_255: &str = "use @std.io.StdoutBytes enduse\n\nuse @std.process.ExitCode enduse\n\n\
    proc main\n  takes {out: StdoutBytes}\n  returns code: ExitCode\n  \
    performs {StdoutBytes}\n  do\n    \
    let _ be await StdoutBytes.write(out Bytes([255u8])) endlet\n    \
    return ExitCode(0u8)\nendproc\n";

#[test]
fn invalid_utf8_output_json() {
    let tree = TempDir::project(
        "run-byte",
        BYTE_255_PROJECT,
        &[("src/app/main.vib", BYTE_255)],
    );
    let (code, document) = run_json(tree.path(), &[]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["payload"]["stdout"], "\u{fffd}");
    let write = &document["payload"]["auditTrace"][0];
    assert_eq!(write["op"], "io.stdoutBytes");
    assert_eq!(write["args"], json!(["{kind: @bytes values: [255u8]}"]));
    // In human format the byte itself goes through.
    let human = vibra(tree.path(), &["run"], b"");
    assert_eq!(human.code, 0);
    assert_eq!(human.stdout, [255]);
}

#[test]
fn directory_target() {
    let tree = case_tree("V2-RUNTIME-run-reads-note");
    let notes = TempDir::new("run-notes");
    notes.write("a.txt", "hello");
    let directory = notes.path().to_str().expect("a Unicode path");
    if cfg!(windows) {
        let ran = vibra(tree.path(), &["run", "--", directory], b"");
        assert_eq!(ran.code, 0, "{}", String::from_utf8_lossy(&ran.stderr));
        assert_eq!(ran.stdout, b"hello");
        let (code, document) = run_json(tree.path(), &["--", directory]);
        assert_eq!(code, 0, "{document}");
        assert_eq!(document["payload"]["stdout"], "hello");
        let providers = document["payload"]["drivers"]
            .as_array()
            .expect("drivers")
            .iter()
            .map(|driver| driver["provider"].as_str().expect("a provider").to_owned())
            .collect::<Vec<_>>();
        assert!(
            providers.contains(&"@host.fs.directory".to_owned()),
            "{providers:?}"
        );
        let policies = document["payload"]["policies"]
            .as_object()
            .expect("policies");
        assert_eq!(policies.len(), 1, "the `.txt` policy of `@notes`");
    } else {
        println!(
            "NOT RUN: directory target, Windows (the directory driver exists on Windows only)"
        );
    }
}

#[test]
fn directory_target_other_platforms() {
    if cfg!(windows) {
        println!("NOT RUN: directory target, other platforms (this is Windows)");
        return;
    }
    let tree = case_tree("V2-RUNTIME-run-reads-note");
    let notes = TempDir::new("run-notes-elsewhere");
    notes.write("a.txt", "hello");
    let directory = notes.path().to_str().expect("a Unicode path");
    let (code, document) = run_json(tree.path(), &["--", directory]);
    assert_eq!(code, 4, "{document}");
    assert_eq!(document["result"], "@command.unavailable");
    assert_eq!(document["status"], "unsupported");
    assert!(codes(&document).contains(&"@tool.unavailable".to_owned()));
    assert_eq!(document["payload"]["startup"]["status"], "notStarted");
    assert_eq!(document["payload"]["startup"]["bindings"], json!([]));
}

#[test]
fn no_permission_flag() {
    let tree = echo_project(&["app"]);
    let (code, document) = run_json(tree.path(), &["--allow-all"]);
    assert_invalid_input(code, &document);
    let human = vibra(tree.path(), &["run", "--allow-all"], b"");
    assert_eq!(human.code, 2);
    assert_eq!(human.stdout, b"", "nothing ran");
}

#[test]
fn no_world_switch() {
    let tree = echo_project(&["app"]);
    tree.write("w.vibon", "{format: @hostWorld.v2}");
    let (code, document) = run_json(tree.path(), &["--world", "w.vibon"]);
    assert_invalid_input(code, &document);
    let (code, document) = run_json(tree.path(), &["@app", "--world", "w.vibon"]);
    assert_invalid_input(code, &document);
}

#[test]
fn observation_error_code_in_json() {
    let tree = case_tree("V2-PROJECT-test-unobservable");
    let ran = vibra(tree.path(), &["--format", "json", "test"], b"");
    let document = single_document(&String::from_utf8(ran.stdout).expect("UTF-8"));
    vibra_schema::validate_command_result(&document).expect("a valid envelope");
    assert_eq!(ran.code, 1, "{document}");
    let failure = &document["payload"]["tests"][0]["failure"];
    assert_eq!(
        failure["observationError"]["code"],
        "@tool.unsupportedValue"
    );
}

/// Whether the text after an occurrence of `Launch::System` makes it a
/// pattern: a record pattern followed by `=>` or by a single `=`.
fn is_pattern(after: &str) -> bool {
    let rest = after.trim_start();
    let rest = if let Some(body) = rest.strip_prefix('{') {
        let mut depth = 1usize;
        let mut end = None;
        for (index, character) in body.char_indices() {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(index + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        match end {
            Some(end) => body[end..].trim_start(),
            None => return false,
        }
    } else {
        rest
    };
    rest.starts_with("=>") || (rest.starts_with('=') && !rest.starts_with("=="))
}

/// Every Rust file under `directory`, skipping build output.
fn rust_files(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("a readable directory") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            rust_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// The workspace-relative files that construct `Launch::System`: an
/// occurrence that is not in a comment, a string, or a documentation link,
/// and not a pattern.
fn system_launchers() -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    let mut launchers = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).expect("a readable source");
        for (index, _) in text.match_indices("Launch::System") {
            let line_start = text[..index].rfind('\n').map_or(0, |at| at + 1);
            let before = &text[line_start..index];
            if before.contains("//") || before.ends_with(['"', '`', '[']) {
                continue;
            }
            if is_pattern(&text[index + "Launch::System".len()..]) {
                continue;
            }
            let relative = file
                .strip_prefix(&root)
                .expect("beneath the root")
                .to_string_lossy()
                .replace('\\', "/");
            if !launchers.contains(&relative) {
                launchers.push(relative);
            }
        }
    }
    launchers.sort();
    launchers
}

#[test]
fn only_caller_of_the_system_launch() {
    let allowed = [
        "crates/vibra-cli/src/run.rs",
        "crates/vibra-workspace/tests/it/attacks_run.rs",
        "crates/vibra-workspace/tests/it/dual_run.rs",
    ];
    let launchers = system_launchers();
    assert!(
        launchers.contains(&"crates/vibra-cli/src/run.rs".to_owned()),
        "`vibra run` launches on the process: {launchers:?}"
    );
    let others = launchers
        .iter()
        .filter(|file| !allowed.contains(&file.as_str()))
        .collect::<Vec<_>>();
    assert!(
        others.is_empty(),
        "other constructors of `Launch::System`: {others:?}"
    );
}

#[test]
fn the_launch_scanner_tells_a_construction_from_a_pattern() {
    assert!(is_pattern(" { arguments, capture } => prepared"));
    assert!(is_pattern(" { .. } = launch else"));
    assert!(!is_pattern(
        " {\n    arguments: Vec::new(),\n    capture: true,\n},"
    ));
    assert!(!is_pattern(" { arguments, capture: false } == other"));
}
