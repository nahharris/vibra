//! The run of a binary target under a world: selection, the check before
//! admission, startup, the entry's outcome, the streams, and the trace.
//! Expectations come from the runtime chapter, "Entry outcome", "Host world",
//! and "Audit trace", and the tooling chapter's result table. The trap row
//! and the release rows need a driver seam and are unit tests of `run`.

use vibra_diagnostics::DiagnosticCode;
use vibra_host::trace::Event;
use vibra_workspace::run::{CommandResult, Launch, RunOutcome, run_target};
use vibra_workspace::{HostWorld, MemoryBudget, decode_world};

use crate::support::{TempDir, capture};

const STDOUT: &str =
    "{provider: @host.io.stdout resource: {kind: @processStream} after: []}";

/// A project of one binary target `@app` rooted at `src/app`.
fn project(effects: &str, grants: &str) -> String {
    format!(
        "{{format: @project.v2 package: {{name: \"hello\" version: \"0.1.0\"}} \
         targets: [{{name: @app kind: @bin root: \"src/app\" entry: @app.main.main \
         effects: [{effects}] grants: [{grants}]}}] dependencies: [:]}}"
    )
}

/// An entry `main` with the given parameters, row, `use`s, and body.
fn entry(uses: &[&str], takes: &str, performs: &str, body: &str) -> String {
    let mut text = String::new();
    for path in uses.iter().chain(&["@std.process.ExitCode"]) {
        text.push_str(&format!("use {path} enduse\n\n"));
    }
    text.push_str(&format!(
        "proc main\n  takes {{{takes}}}\n  returns code: ExitCode\n  \
         performs {{{performs}}}\n  do\n{body}endproc\n"
    ));
    text
}

/// `H`: writes `hello\n` and returns `ExitCode(code)`.
fn hello(code: &str) -> String {
    entry(
        &["@std.io.StdoutText"],
        "out: StdoutText",
        "StdoutText",
        &format!(
            "    let _ be await StdoutText.write(out \"hello\\n\") endlet\n    \
             return ExitCode({code})\n"
        ),
    )
}

fn hello_project() -> String {
    project("@std.io.StdoutText", &format!("@out: {STDOUT}"))
}

fn world(text: &str) -> HostWorld {
    decode_world(None, text.as_bytes()).expect("the world decodes")
}

/// Runs the one binary target of a tree of `document` and `main` under
/// `world`.
fn run_in(
    document: &str,
    main: &str,
    world: &HostWorld,
    budget: MemoryBudget,
) -> RunOutcome {
    let tree = TempDir::project("run", document, &[("src/app/main.vib", main)]);
    let snapshot = capture(tree.path()).expect("the tree captures");
    run_target(&snapshot, None, Launch::World(world), budget)
}

fn run(document: &str, main: &str, world_text: &str) -> RunOutcome {
    run_in(document, main, &world(world_text), MemoryBudget::DEFAULT)
}

const DEFAULT_WORLD: &str = "{format: @hostWorld.v2}";

fn startup(outcome: &RunOutcome) -> Vec<(String, &'static str)> {
    outcome
        .startup
        .bindings
        .iter()
        .map(|binding| (binding.name.to_string(), binding.status.spelling()))
        .collect()
}

fn invocations(outcome: &RunOutcome) -> Vec<u64> {
    outcome
        .trace
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Host(event) => Some(event.invocation),
            _ => None,
        })
        .collect()
}

fn codes(outcome: &RunOutcome) -> Vec<DiagnosticCode> {
    outcome
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code())
        .collect()
}

fn released(names: &[&str]) -> Vec<(String, &'static str)> {
    names
        .iter()
        .map(|name| ((*name).to_owned(), "released"))
        .collect()
}

#[test]
fn v2_runtime_run_hello() {
    let outcome = run(&hello_project(), &hello("0u8"), DEFAULT_WORLD);
    assert_eq!(outcome.result, CommandResult::Ok);
    assert_eq!(outcome.application_code, Some(0));
    assert!(outcome.program_result.is_some());
    assert_eq!(outcome.stdout, b"hello\n");
    assert!(outcome.stderr.is_empty());
    assert_eq!(startup(&outcome), released(&["@out"]));
    assert_eq!(outcome.trace.target.as_deref(), Some("@app"));
    assert!(outcome.trace.revision.starts_with("sha256:"));
    assert_eq!(invocations(&outcome), [2]);
    assert_eq!(outcome.trap, None);
}

#[test]
fn v2_runtime_run_nonzero() {
    let outcome = run(&hello_project(), &hello("3u8"), DEFAULT_WORLD);
    assert_eq!(outcome.result, CommandResult::ProgramFailed);
    assert_eq!(outcome.application_code, Some(3));
    assert_eq!(outcome.trap, None);
    assert!(!outcome.document().contains("trap:"));
}

#[test]
fn v2_runtime_run_code_255() {
    let outcome = run(&hello_project(), &hello("255u8"), DEFAULT_WORLD);
    assert_eq!(outcome.application_code, Some(255));
    assert!(outcome.document().contains("applicationCode: 255u8"));
}

#[test]
fn v2_runtime_run_static_error() {
    let outcome = run(&hello_project(), &hello("\"a\""), DEFAULT_WORLD);
    assert_eq!(outcome.result, CommandResult::Diagnostics);
    assert_eq!(codes(&outcome), [DiagnosticCode::TypeArgumentMismatch]);
    assert!(outcome.startup.bindings.is_empty());
    assert!(outcome.trace.events.is_empty());
    assert!(outcome.trace.drivers.is_empty());
    assert!(outcome.stdout.is_empty());
}

/// `M` of step 26 and an entry that reads `a.txt` and writes it.
fn m_project() -> String {
    project(
        "@std.env.EnvReadArgs @std.fs.DirReadText @std.io.StdoutText",
        &format!(
            "@args: {{provider: @host.env.args resource: {{kind: @processArguments}} after: []}} \
             @notes: {{provider: @host.fs.directory resource: {{kind: @argumentPath index: 0u64 \
             source: @args}} policy: {{format: @scopePolicy.v1 family: @filesystem expression: \
             {{kind: @fileRule root: @notes prefix: [] depth: @tree actions: [@read] names: \
             {{kind: @suffix value: \".txt\"}}}}}} after: [@args]}} @out: {STDOUT}"
        ),
    )
}

fn m_entry(name: &str, body: &str) -> String {
    entry(
        &[
            "@std.env.EnvReadArgs",
            "@std.fs.DirReadText",
            "@std.fs.FsError",
            "@std.fs.RelativePath",
            "@std.io.StdoutText",
        ],
        "args: EnvReadArgs notes: DirReadText out: Option(StdoutText)",
        "EnvReadArgs DirReadText StdoutText",
        &format!(
            "    let Result.ok(value: path) be RelativePath([\"{name}\"]) endlet\n{body}"
        ),
    )
}

fn m_reads() -> String {
    m_entry(
        "a.txt",
        "    let Result.ok(value: text) be await DirReadText.read(notes path) else\n      \
         return ExitCode(1u8)\n    endlet\n    \
         let Option.some(value: stdout) be out else\n      return ExitCode(2u8)\n    endlet\n    \
         let _ be await StdoutText.write(stdout text) endlet\n    return ExitCode(0u8)\n",
    )
}

const NOTES_WORLD: &str = "{format: @hostWorld.v2 directories: [\"/notes\"] files: \
    [\"/notes/a.txt\": \"hello\" \"/notes/raw.bin\": {kind: @bytes values: [0u8 255u8]}] \
    args: [\"/notes\"]}";

#[test]
fn v2_runtime_run_required_refused() {
    let outcome = run(
        &m_project(),
        &m_reads(),
        "{format: @hostWorld.v2 args: [\"/missing\"]}",
    );
    assert_eq!(outcome.result, CommandResult::OperationalFailure);
    assert_eq!(outcome.program_result, None);
    assert_eq!(outcome.application_code, None);
    assert_eq!(
        startup(&outcome),
        [
            ("@args".to_owned(), "released"),
            ("@notes".to_owned(), "refused")
        ]
    );
    // The @args events that completed are kept: its acquisition and the
    // read of @notes's path, invocation 1.
    assert_eq!(invocations(&outcome), [1]);
    assert_eq!(outcome.trace.events.len(), 3);
    let failure = outcome
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code() == DiagnosticCode::GrantRequiredUnavailable
        })
        .expect("the startup diagnostic");
    assert_eq!(failure.source_id(), Some("project.vibon"));
    assert!(outcome.document().contains(
        "{binding: @notes status: @refused failure: @grant.requiredUnavailable}"
    ));
}

#[test]
fn v2_runtime_run_optional_absent() {
    let document = project(
        "@std.env.EnvReadVar @std.fs.DirWriteText",
        "@metrics: {provider: @host.fs.directory resource: {kind: @variablePath name: \
         \"METRICS\" source: @vars} policy: {format: @scopePolicy.v1 family: @filesystem \
         expression: {kind: @fileRule root: @metrics prefix: [] depth: @tree actions: \
         [@create @replace] names: {kind: @anyName}}} after: [@vars]} @vars: {provider: \
         @host.env.vars resource: {kind: @variableSet names: [\"METRICS\"]} after: []}",
    );
    let main = entry(
        &["@std.env.EnvReadVar", "@std.fs.DirWriteText"],
        "metrics: Option(DirWriteText) vars: EnvReadVar",
        "DirWriteText EnvReadVar",
        "    match metrics\n      case Option.none()\n        return ExitCode(0u8)\n      \
         case _\n        return ExitCode(1u8)\n    endmatch\n",
    );
    let outcome = run(&document, &main, DEFAULT_WORLD);
    assert_eq!(outcome.result, CommandResult::Ok);
    assert_eq!(
        startup(&outcome),
        [
            ("@vars".to_owned(), "released"),
            ("@metrics".to_owned(), "refused")
        ]
    );
    assert_eq!(outcome.startup.failure, None);
}

#[test]
fn v2_runtime_run_exhausted() {
    let main = hello("0u8")
        .replace(
            "proc main",
            "fn loop\n  takes {n: I32}\n  returns r: I32\n  do\n    \
             return I32.add(1i32 loop(n))\nendfn\n\nproc main",
        )
        .replace(
            "    return ExitCode(0u8)\n",
            "    let _ be loop(0i32) endlet\n    return ExitCode(0u8)\n",
        );
    let outcome = run_in(
        &hello_project(),
        &main,
        &world(DEFAULT_WORLD),
        MemoryBudget::new(1 << 20),
    );
    assert_eq!(outcome.result, CommandResult::OperationalFailure);
    assert_eq!(codes(&outcome), [DiagnosticCode::RuntimeMemoryExhausted]);
    assert_eq!(outcome.application_code, None);
    assert_eq!(outcome.limits_reached, ["memory"]);
    assert_eq!(outcome.stdout, b"hello\n", "the write before it is kept");
    assert_eq!(startup(&outcome), released(&["@out"]));
}

#[test]
fn v2_runtime_run_reads_note() {
    let outcome = run(&m_project(), &m_reads(), NOTES_WORLD);
    assert_eq!(outcome.result, CommandResult::Ok);
    assert_eq!(outcome.stdout, b"hello");
    // 1 the startup read, 2 the entry, 3 and 4 the body's awaits.
    assert_eq!(invocations(&outcome), [1, 3, 4]);
    assert_eq!(startup(&outcome), released(&["@args", "@notes", "@out"]));
}

#[test]
fn v2_runtime_run_denied_by_recipe_policy() {
    let main = m_entry(
        "raw.bin",
        "    match await DirReadText.read(notes path)\n      \
         case Result.err(error: FsError.denied())\n        return ExitCode(4u8)\n      \
         case _\n        return ExitCode(0u8)\n    endmatch\n",
    );
    let outcome = run(&m_project(), &main, NOTES_WORLD);
    assert_eq!(outcome.application_code, Some(4));
    assert_eq!(invocations(&outcome), [1], "no event for the denied read");
}

#[test]
fn v2_runtime_run_recipe_limit() {
    let document = project(
        "@std.io.StdoutText",
        "@out: {provider: @host.io.stdout resource: {kind: @processStream} \
         limits: {outputBytes: 3u64} after: []}",
    );
    let main = entry(
        &["@std.io.IoError", "@std.io.StdoutText"],
        "out: StdoutText",
        "StdoutText",
        "    match await StdoutText.write(out \"hello\")\n      \
         case Result.err(error: IoError.limitExceeded())\n        return ExitCode(5u8)\n      \
         case _\n        return ExitCode(0u8)\n    endmatch\n",
    );
    let outcome = run(&document, &main, DEFAULT_WORLD);
    assert_eq!(outcome.application_code, Some(5));
    assert!(outcome.stdout.is_empty());
    assert!(invocations(&outcome).is_empty());
}

#[test]
fn v2_grant_env_name_denied() {
    let document = project(
        "@std.env.EnvReadVar",
        "@vars: {provider: @host.env.vars resource: {kind: @variableSet names: [\"HOME\"]} \
         after: []}",
    );
    let main = entry(
        &["@std.env.EnvError", "@std.env.EnvReadVar"],
        "vars: EnvReadVar",
        "EnvReadVar",
        "    match await EnvReadVar.read(vars \"PATH\")\n      \
         case Result.err(error: EnvError.denied())\n        return ExitCode(6u8)\n      \
         case _\n        return ExitCode(0u8)\n    endmatch\n",
    );
    let outcome = run(
        &document,
        &main,
        "{format: @hostWorld.v2 env: [\"HOME\": \"/home/vibra\" \"PATH\": \"/bin\"]}",
    );
    assert_eq!(outcome.application_code, Some(6));
    assert!(invocations(&outcome).is_empty(), "Q12: no event");
}

#[test]
fn v2_runtime_run_stdin() {
    let document = project(
        "@std.io.StdinText @std.io.StdoutText",
        &format!(
            "@input: {{provider: @host.io.stdin resource: {{kind: @processStream}} after: []}} \
             @out: {STDOUT}"
        ),
    );
    let main = entry(
        &["@std.io", "@std.io.StdinText", "@std.io.StdoutText"],
        "input: StdinText out: StdoutText",
        "StdinText StdoutText",
        "    let Result.ok(value: text) be await io.readAllText(input) else\n      \
         return ExitCode(1u8)\n    endlet\n    \
         let _ be await StdoutText.write(out text) endlet\n    return ExitCode(0u8)\n",
    );
    let outcome = run(&document, &main, "{format: @hostWorld.v2 stdin: \"x\\ny\"}");
    assert_eq!(outcome.result, CommandResult::Ok);
    assert_eq!(outcome.stdout, b"x\ny");
    // Q10: the read of the input, then the read of its empty end.
    assert_eq!(invocations(&outcome), [4, 6, 7]);
}

#[test]
fn v2_runtime_run_fresh_world() {
    let rule = |leaf: &str, actions: &str| {
        format!(
            "{{provider: @host.fs.directory resource: {{kind: @fixedPath path: \"data\"}} \
             policy: {{format: @scopePolicy.v1 family: @filesystem expression: {{kind: \
             @fileRule root: {leaf} prefix: [] depth: @tree actions: [{actions}] names: \
             {{kind: @anyName}}}}}} after: []}}"
        )
    };
    let document = project(
        "@std.fs.DirReadText @std.fs.DirWriteText",
        &format!(
            "@reader: {} @writer: {}",
            rule("@reader", "@read"),
            rule("@writer", "@create @replace")
        ),
    );
    let main = entry(
        &[
            "@std.fs.DirReadText",
            "@std.fs.DirWriteText",
            "@std.fs.FsError",
            "@std.fs.RelativePath",
        ],
        "reader: DirReadText writer: DirWriteText",
        "DirReadText DirWriteText",
        "    let Result.ok(value: path) be RelativePath([\"f.txt\"]) endlet\n    \
         let before be await DirReadText.read(reader path) endlet\n    \
         let _ be await DirWriteText.write(writer path \"x\") endlet\n    \
         match before\n      case Result.err(error: FsError.notFound())\n        \
         return ExitCode(0u8)\n      case _\n        return ExitCode(1u8)\n    endmatch\n",
    );
    let shared = world("{format: @hostWorld.v2 directories: [\"/data\"]}");
    for _ in 0..2 {
        let outcome = run_in(&document, &main, &shared, MemoryBudget::DEFAULT);
        assert_eq!(
            outcome.application_code,
            Some(0),
            "the file is absent at start"
        );
    }
}

#[test]
fn v2_runtime_run_entry_requires_else() {
    let main = hello("0u8").replace(
        "  performs {StdoutText}\n",
        "  performs {StdoutText}\n  requires false else ExitCode(9u8)\n",
    );
    let outcome = run(&hello_project(), &main, DEFAULT_WORLD);
    assert_eq!(outcome.application_code, Some(9));
    assert!(outcome.stdout.is_empty(), "the body's write is absent");
    assert!(invocations(&outcome).is_empty());
}

#[test]
fn v2_runtime_run_target_selection() {
    let target = |name: &str, kind: &str| {
        let entry = if kind == "@bin" {
            format!(
                " entry: {name}.main.main effects: [@std.io.StdoutText] grants: [@out: {STDOUT}]"
            )
        } else {
            String::new()
        };
        format!(
            "{{name: {name} kind: {kind} root: \"src/{}\"{entry}}}",
            name.trim_start_matches('@')
        )
    };
    let document = format!(
        "{{format: @project.v2 package: {{name: \"hello\" version: \"0.1.0\"}} targets: [{} {} {}] \
         dependencies: [:]}}",
        target("@a", "@bin"),
        target("@b", "@bin"),
        target("@c", "@lib")
    );
    let text = hello("0u8");
    let tree = TempDir::project(
        "run",
        &document,
        &[
            ("src/a/main.vib", text.as_str()),
            ("src/b/main.vib", text.as_str()),
            (
                "src/c/lib.vib",
                "fn one returns r: I32 do return 1i32 endfn\n",
            ),
        ],
    );
    let snapshot = capture(tree.path()).expect("the tree captures");
    let world = HostWorld::default();
    let select = |name: Option<&str>| {
        run_target(
            &snapshot,
            name,
            Launch::World(&world),
            MemoryBudget::DEFAULT,
        )
    };
    for name in [None, Some("@c"), Some("@d")] {
        let outcome = select(name);
        assert_eq!(outcome.result, CommandResult::InvalidInput, "{name:?}");
        assert_eq!(outcome.target, None);
        assert!(outcome.diagnostics.is_empty());
        assert!(outcome.trace.events.is_empty());
    }
    let named = select(Some("@b"));
    assert_eq!(named.result, CommandResult::Ok);
    assert_eq!(named.trace.target.as_deref(), Some("@b"));
}
