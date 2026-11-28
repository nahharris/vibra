//! The run of a binary target under a world: selection, the check before
//! admission, startup, the entry's outcome, the streams, and the trace.
//! Expectations come from the runtime chapter, "Entry outcome", "Host world",
//! and "Audit trace", and the tooling chapter's result table. The trap row
//! and the release rows need a driver seam and are unit tests of `run`.

use vibra_workspace::run::{CommandResult, Launch, RunOutcome, run_target};
use vibra_workspace::{HostWorld, MemoryBudget};

use crate::dual_run::{STDOUT, world};
use crate::support::{TempDir, capture};

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
