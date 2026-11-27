//! One case run twice: under `Launch::World` with its world document, and
//! under `Launch::System` with that world written into a new temporary
//! directory (architecture, section 3). The two outcomes are compared on the
//! `@command.*` result, the application code, both captured streams, each
//! binding's status, and the trace's events with their source origins, the
//! drivers and the results of clock and random operations removed.
//!
//! Agreement on a case is sampled evidence over that case's domain (03
//! "Tests and model profiles": "A comparison of a real driver against its
//! model is sampled evidence with a stated domain"); it proves nothing about
//! the drivers in general. The system half needs the directory driver, which
//! exists on Windows only: elsewhere each row runs the world half and prints
//! `NOT RUN`.
//!
//! The attack runs of `attacks_run.rs` use [`materialize`] and
//! [`run_system`] from here.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use vibra_host::trace::Event;
use vibra_ir::external::host::HostOp;
use vibra_syntax::canonical_string;
use vibra_workspace::run::{CommandResult, Launch, RunOutcome, run_target};
use vibra_workspace::{HostWorld, MemoryBudget, decode_world};

use crate::support::{TempDir, capture};

// ---------------------------------------------------------------------------
// The harness.
// ---------------------------------------------------------------------------

/// A world written to disk, and the arguments of the system run over it.
pub(crate) struct Materialized {
    /// The directory that stands for the world's `/`.
    pub(crate) root: PathBuf,
    /// The world's `args`, each argument that is a virtual path re-rooted
    /// under [`Self::root`].
    pub(crate) arguments: Vec<OsString>,
    /// The canonical text of each re-rooted argument and of the virtual path
    /// it stands for.
    pub(crate) rerooted: Vec<(String, String)>,
}

/// The path under `root` of the virtual names `names`.
fn under(root: &Path, names: &[String]) -> PathBuf {
    let mut path = root.to_path_buf();
    for name in names {
        path.push(name);
    }
    path
}

/// The names of a virtual path: `/in/a.txt` is `["in", "a.txt"]`; a relative
/// one is taken under `/`.
fn virtual_names(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Whether the entries of the world directory `names` collide when case is
/// ignored, as they do on a case-insensitive volume.
fn needs_case_sensitivity(world: &HostWorld, names: &[String]) -> bool {
    let entries = world.filesystem().list(names).unwrap_or_default();
    let mut folded = entries
        .iter()
        .map(|name| name.to_lowercase())
        .collect::<Vec<_>>();
    folded.sort();
    folded.windows(2).any(|pair| pair[0] == pair[1])
}

/// Makes `directory`, which must be empty, case-sensitive. The volumes of
/// every other platform are case-sensitive already.
fn make_case_sensitive(directory: &Path) -> Result<(), String> {
    if !cfg!(windows) {
        return Ok(());
    }
    let plain = directory.display().to_string();
    let plain = plain.strip_prefix(r"\\?\").unwrap_or(&plain).to_owned();
    let output = std::process::Command::new("fsutil.exe")
        .args(["file", "setCaseSensitiveInfo"])
        .arg(&plain)
        .arg("enable")
        .output()
        .map_err(|error| format!("fsutil did not start: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "this volume cannot make {plain} case-sensitive: {}",
            String::from_utf8_lossy(&output.stdout).trim()
        ))
    }
}

/// Writes the world's `directories` and `files` under the new directory
/// `dir`, and returns the world's `args`, each argument that is a virtual
/// path (it starts with `/`) re-rooted there. Nothing outside `dir` is
/// written: every path written is `dir` joined with names the world
/// validated as plain.
///
/// `Launch::System { arguments, capture }` gives the program the process's
/// own standard input and environment, so a world that sets `stdin` or
/// `env` is outside the harness's domain and is refused.
///
/// # Errors
///
/// The reason the machine cannot hold this world (a directory whose entries
/// differ only in case, on a volume that cannot make it case-sensitive).
pub(crate) fn materialize(
    world: &HostWorld,
    dir: &Path,
) -> Result<Materialized, String> {
    assert!(
        world.stdin().is_empty() && world.env().is_empty(),
        "the system half takes the process's own stdin and environment"
    );
    fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let root = fs::canonicalize(dir).map_err(|error| error.to_string())?;
    let filesystem = world.filesystem();
    if needs_case_sensitivity(world, &[]) {
        make_case_sensitive(&root)?;
    }
    // Path order puts every directory before its descendants.
    for names in filesystem.directories() {
        let path = under(&root, names);
        assert!(path.starts_with(&root), "{} escapes", path.display());
        fs::create_dir(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if needs_case_sensitivity(world, names) {
            make_case_sensitive(&path)?;
        }
    }
    for (names, contents) in filesystem.files() {
        let path = under(&root, names);
        assert!(path.starts_with(&root), "{} escapes", path.display());
        fs::write(&path, contents)
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    let mut rerooted = Vec::new();
    let arguments = world
        .args()
        .iter()
        .map(|argument| {
            if argument.starts_with('/') {
                let path = under(&root, &virtual_names(argument));
                let text = path.display().to_string();
                rerooted.push((canonical_string(&text), canonical_string(argument)));
                OsString::from(text)
            } else {
                OsString::from(argument)
            }
        })
        .collect();
    Ok(Materialized {
        root,
        arguments,
        rerooted,
    })
}

/// `document` with every `@fixedPath` re-rooted under `root`: the path is a
/// virtual path of the world, relative ones taken under `/`.
pub(crate) fn reroot_fixed_paths(document: &str, root: &Path) -> String {
    const OPEN: &str = "{kind: @fixedPath path: \"";
    let mut out = String::new();
    let mut rest = document;
    while let Some(at) = rest.find(OPEN) {
        let (before, after) = rest.split_at(at + OPEN.len());
        let Some(end) = after.find('"') else { break };
        out.push_str(&before[..at]);
        out.push_str("{kind: @fixedPath path: ");
        let path = under(root, &virtual_names(&after[..end]));
        out.push_str(&canonical_string(&path.display().to_string()));
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// The source file of every case: the one binary target `@app`.
pub(crate) const MAIN: &str = "src/app/main.vib";

/// Writes the tree of `document` and `main` into `dir`.
fn write_tree(dir: &Path, document: &str, main: &str) {
    fs::create_dir_all(dir.join("src/app")).expect("the tree's directories");
    fs::write(dir.join("project.vibon"), document).expect("the project file");
    fs::write(dir.join(MAIN), main).expect("the entry");
}

/// The world a case's text decodes to.
pub(crate) fn world(text: &str) -> HostWorld {
    decode_world(None, text.as_bytes()).expect("the world decodes")
}

/// A system run of the tree of `document` (its `@fixedPath`s re-rooted under
/// `materialized.root`) and `main`, written into `tree`.
pub(crate) fn run_system(
    tree: &Path,
    document: &str,
    main: &str,
    materialized: Materialized,
) -> (RunOutcome, Observed) {
    write_tree(
        tree,
        &reroot_fixed_paths(document, &materialized.root),
        main,
    );
    let snapshot = capture(tree).expect("the system tree captures");
    let launch = Launch::System {
        arguments: materialized.arguments,
        capture: true,
    };
    let outcome = run_target(&snapshot, None, launch, MemoryBudget::DEFAULT);
    let mut observed = Observed::of(&outcome);
    observed.unroot(&materialized.rerooted);
    (outcome, observed)
}

/// Both halves of a case.
pub(crate) struct Both {
    /// The world half.
    pub(crate) world: RunOutcome,
    /// The system half, or why it did not run: the platform has no
    /// directory driver, or the machine cannot hold the world.
    pub(crate) system: Result<(RunOutcome, Observed), String>,
    /// The temporary directory of the system half: the world under
    /// `world/`, the tree under `tree/`.
    pub(crate) directory: TempDir,
}

impl Both {
    /// The disk directory of the world's `/`.
    pub(crate) fn root(&self) -> PathBuf {
        fs::canonicalize(self.directory.join("world")).expect("the world root")
    }
}

/// Runs the one binary target of `document` and `main` under the world of
/// `world_text`, then, on Windows, under the system driver over that world
/// written to disk, with every `@fixedPath` re-rooted there.
pub(crate) fn run_both(document: &str, main: &str, world_text: &str) -> Both {
    let world = world(world_text);
    let tree = TempDir::project("dual-world", document, &[(MAIN, main)]);
    let snapshot = capture(tree.path()).expect("the world tree captures");
    let in_world = run_target(
        &snapshot,
        None,
        Launch::World(&world),
        MemoryBudget::DEFAULT,
    );
    let directory = TempDir::new("dual-system");
    let system = if cfg!(windows) {
        materialize(&world, &directory.join("world")).map(|materialized| {
            run_system(&directory.join("tree"), document, main, materialized)
        })
    } else {
        Err("no directory driver".to_owned())
    };
    Both {
        world: in_world,
        system,
        directory,
    }
}

/// What the comparison reads of one outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Observed {
    /// The `@command.*` atom.
    pub(crate) result: &'static str,
    /// The application code.
    pub(crate) application_code: Option<u8>,
    /// The captured standard output.
    pub(crate) stdout: Vec<u8>,
    /// The captured standard error.
    pub(crate) stderr: Vec<u8>,
    /// Each binding and its status.
    pub(crate) bindings: Vec<(String, &'static str)>,
    /// Each event, as its compared fields in order.
    pub(crate) events: Vec<Vec<(&'static str, String)>>,
}

/// Whether the result of `op` is the reading of a clock or random bytes,
/// which the comparison removes.
fn varies(op: HostOp) -> bool {
    matches!(
        op,
        HostOp::TimeNowWall | HostOp::TimeNowMonotonic | HostOp::RandomGenerateBytes
    )
}

/// The compared fields of `event`. A startup failure is compared by its code
/// only: its message is unspecified text.
fn fields(event: &Event) -> Vec<(&'static str, String)> {
    match event {
        Event::Startup(event) => vec![
            ("kind", "@startup".to_owned()),
            ("binding", event.binding.to_string()),
            ("provider", event.provider.atom().to_owned()),
            ("status", event.status.atom().to_owned()),
            (
                "grant",
                event
                    .grant
                    .as_ref()
                    .map(|grant| grant.canonical_bytes())
                    .unwrap_or_default(),
            ),
            (
                "failure",
                event
                    .failure
                    .as_ref()
                    .map(|failure| failure.code.atom().to_owned())
                    .unwrap_or_default(),
            ),
        ],
        Event::Host(event) => {
            let mut fields = vec![
                ("kind", "@host".to_owned()),
                ("invocation", event.invocation.to_string()),
                ("parent", format!("{:?}", event.parent)),
                ("phase", event.phase.atom().to_owned()),
                ("op", event.op.symbol().to_owned()),
                (
                    "origin",
                    event
                        .origin
                        .as_ref()
                        .map(|origin| {
                            format!(
                                "{} {}..{}",
                                origin.source_id(),
                                origin.span().start(),
                                origin.span().end()
                            )
                        })
                        .unwrap_or_default(),
                ),
                ("grant", event.grant.canonical_bytes()),
                ("args", event.args.join(" ")),
            ];
            if !varies(event.op) {
                fields.push(("result", event.result.clone()));
            }
            fields
        }
        Event::Opaque(event) => vec![
            ("kind", event.kind().to_owned()),
            ("bytes", event.canonical_bytes().to_owned()),
        ],
    }
}

impl Observed {
    /// What the comparison reads of `outcome`: its streams are the world's,
    /// or those the system launch captured.
    pub(crate) fn of(outcome: &RunOutcome) -> Self {
        Self {
            result: outcome.result.as_atom(),
            application_code: outcome.application_code,
            stdout: outcome.stdout.clone(),
            stderr: outcome.stderr.clone(),
            bindings: outcome
                .startup
                .bindings
                .iter()
                .map(|binding| (binding.name.to_string(), binding.status.spelling()))
                .collect(),
            events: outcome.trace.events.iter().map(fields).collect(),
        }
    }

    /// The world half's observation: its streams are in the outcome.
    /// Maps each re-rooted argument back to the virtual path it stands for,
    /// wherever an argument or a result shows it: the reads of `args`.
    fn unroot(&mut self, rerooted: &[(String, String)]) {
        for event in &mut self.events {
            for (name, text) in event.iter_mut() {
                if *name == "args" || *name == "result" {
                    for (real, virtual_path) in rerooted {
                        *text = text.replace(real.as_str(), virtual_path);
                    }
                }
            }
        }
    }

    /// Whether some event is a host call of `op`.
    pub(crate) fn calls(&self, symbol: &str) -> usize {
        self.events
            .iter()
            .filter(|event| event.contains(&("op", symbol.to_owned())))
            .count()
    }
}

/// The first difference between the two halves, naming what differs; a
/// difference prints both sides whole.
pub(crate) fn compare(world: &Observed, system: &Observed) -> Result<(), String> {
    let mut difference = None;
    let mut note = |what: String| {
        difference.get_or_insert(what);
    };
    if world.result != system.result {
        note(format!("result: {} / {}", world.result, system.result));
    }
    if world.application_code != system.application_code {
        note(format!(
            "applicationCode: {:?} / {:?}",
            world.application_code, system.application_code
        ));
    }
    for (name, left, right) in [
        ("stdout", &world.stdout, &system.stdout),
        ("stderr", &world.stderr, &system.stderr),
    ] {
        if left != right {
            note(format!(
                "{name}: {:?} / {:?}",
                String::from_utf8_lossy(left),
                String::from_utf8_lossy(right)
            ));
        }
    }
    if world.bindings != system.bindings {
        note(format!(
            "bindings: {:?} / {:?}",
            world.bindings, system.bindings
        ));
    }
    if world.events.len() != system.events.len() {
        note(format!(
            "events: {} / {}",
            world.events.len(),
            system.events.len()
        ));
    }
    for (index, (left, right)) in world.events.iter().zip(&system.events).enumerate() {
        if left == right {
            continue;
        }
        let field = left
            .iter()
            .zip(right)
            .find(|(a, b)| a != b)
            .map_or("fields", |((name, _), _)| *name);
        let value = |event: &[(&str, String)]| {
            event
                .iter()
                .find(|(name, _)| *name == field)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        note(format!(
            "event {index} field `{field}`: {} / {}",
            value(left),
            value(right)
        ));
        break;
    }
    match difference {
        None => Ok(()),
        Some(what) => {
            let mut report = format!("the halves disagree: {what} (world / system)\n");
            let _ = writeln!(report, "world:  {world:?}");
            let _ = writeln!(report, "system: {system:?}");
            Err(report)
        }
    }
}

/// Runs a case, compares the halves, and prints the row's line; returns the
/// world half's observation and the system half's outcome, when it ran.
pub(crate) fn agree(
    row: &str,
    document: &str,
    main: &str,
    world_text: &str,
) -> (Observed, Both) {
    let both = run_both(document, main, world_text);
    let world = Observed::of(&both.world);
    match &both.system {
        Err(reason) => println!("NOT RUN {row}: {reason}"),
        Ok((_, system)) => {
            if let Err(report) = compare(&world, system) {
                panic!("{row}: {report}");
            }
            println!(
                "ROW {row}: {}, both halves agree: {} code {:?}, {} events",
                std::env::consts::OS,
                world.result,
                world.application_code,
                world.events.len()
            );
        }
    }
    (world, both)
}

// ---------------------------------------------------------------------------
// The programs.
// ---------------------------------------------------------------------------

/// The stdout recipe.
pub(crate) const STDOUT: &str =
    "{provider: @host.io.stdout resource: {kind: @processStream} after: []}";

/// A project of one binary target `@app` rooted at `src/app`.
pub(crate) fn project(effects: &str, grants: &str) -> String {
    format!(
        "{{format: @project.v2 package: {{name: \"dual\" version: \"0.1.0\"}} \
         targets: [{{name: @app kind: @bin root: \"src/app\" entry: @app.main.main \
         effects: [{effects}] grants: [{grants}]}}] dependencies: [:]}}"
    )
}

/// A directory recipe of `leaf` over `resource` with one file rule.
pub(crate) fn file_rule(
    leaf: &str,
    resource: &str,
    prefix: &str,
    actions: &str,
    names: &str,
    tail: &str,
) -> String {
    format!(
        "{leaf}: {{provider: @host.fs.directory resource: {resource} policy: \
         {{format: @scopePolicy.v1 family: @filesystem expression: {{kind: @fileRule \
         root: {leaf} prefix: [{prefix}] depth: @tree actions: [{actions}] names: \
         {names}}}}} {tail}}}"
    )
}

/// The suffix name test `.txt`.
pub(crate) const TXT: &str = "{kind: @suffix value: \".txt\"}";

/// The name test that accepts every name.
pub(crate) const ANY_NAME: &str = "{kind: @anyName}";

/// The project of `C`: `@args`; `@source` an `@argumentPath` of argument 0
/// with a read rule of `source_prefix` and `source_names`, and
/// `source_limits` (a `limits: …` field or nothing); `@dest` the `@fixedPath`
/// `dest_path` with `dest_actions`; `@out`.
pub(crate) fn copy_project(
    source_prefix: &str,
    source_names: &str,
    source_limits: &str,
    dest_path: &str,
    dest_actions: &str,
) -> String {
    let args = "@args: {provider: @host.env.args resource: {kind: @processArguments} after: []}";
    let source = file_rule(
        "@source",
        "{kind: @argumentPath index: 0u64 source: @args}",
        source_prefix,
        "@read",
        source_names,
        &format!("{source_limits} after: [@args]"),
    );
    let dest = file_rule(
        "@dest",
        &format!("{{kind: @fixedPath path: \"{dest_path}\"}}"),
        "",
        dest_actions,
        ANY_NAME,
        "after: []",
    );
    project(
        "@std.env.EnvReadArgs @std.fs.DirReadText @std.fs.DirWriteText @std.io.StdoutText",
        &format!("{args} {dest} @out: {STDOUT} {source}"),
    )
}

/// The project of `C` as the matrix gives it: suffix `.txt`, `dest` the
/// `@fixedPath` `out` with create and replace.
pub(crate) fn copy_default() -> String {
    copy_project("", TXT, "", "out", "@create @replace")
}

/// The function mapping an `FsError` to the codes of the case programs:
/// `notFound` 31, `denied` 32, `limitExceeded` 33, `invalidText` 34,
/// `invalidPath` 35, any other 39. A write adds 10.
const READ_CODE: &str = "fn readCode
  takes {error: FsError}
  returns code: U8
  do
    match error
      case FsError.notFound()
        return 31u8
      case FsError.denied()
        return 32u8
      case FsError.limitExceeded()
        return 33u8
      case FsError.invalidText()
        return 34u8
      case FsError.invalidPath()
        return 35u8
      case _
        return 39u8
    endmatch
endfn
";

/// `C`, the copy program: it reads its arguments; the names after the first
/// are the path (one or two names); it reads that file through `source`,
/// writes it under `dest`, and writes it to `out`. Codes: 0 copied; 20 an
/// argument list of another length; 21 the path is no `RelativePath`; 22 the
/// arguments could not be read; a read error its `readCode`, a write error
/// 10 more.
pub(crate) fn copy_program() -> String {
    format!(
        "use @std.env.EnvReadArgs enduse

use @std.fs.DirReadText enduse

use @std.fs.DirWriteText enduse

use @std.fs.FsError enduse

use @std.fs.RelativePath enduse

use @std.io.StdoutText enduse

use @std.process.ExitCode enduse

{READ_CODE}
proc deliver
  takes {{dest: DirWriteText out: StdoutText path: RelativePath text: Str}}
  returns code: ExitCode
  performs {{DirWriteText StdoutText}}
  do
    match await DirWriteText.write(dest path text)
      case Result.err(error: error)
        return ExitCode(U8.add(10u8 readCode(error)))
      case _
        let _ be await StdoutText.write(out text) endlet
        return ExitCode(0u8)
    endmatch
endproc

proc copy
  takes {{source: DirReadText dest: DirWriteText out: StdoutText names: List(Str)}}
  returns code: ExitCode
  performs {{DirReadText DirWriteText StdoutText}}
  do
    let Result.ok(value: path) be RelativePath(names) else
      return ExitCode(21u8)
    endlet
    match await DirReadText.read(source path)
      case Result.err(error: error)
        return ExitCode(readCode(error))
      case Result.ok(value: text)
        return await deliver(dest out path text)
    endmatch
endproc

proc main
  takes {{args: EnvReadArgs source: DirReadText dest: DirWriteText out: StdoutText}}
  returns code: ExitCode
  performs {{EnvReadArgs DirReadText DirWriteText StdoutText}}
  do
    let Result.ok(value: arguments) be await EnvReadArgs.read(args) else
      return ExitCode(22u8)
    endlet
    match arguments
      case [_ first]
        return await copy(source dest out [first])
      case [_ first second]
        return await copy(source dest out [first second])
      case _
        return ExitCode(20u8)
    endmatch
endproc
"
    )
}

/// The uses every other case program starts with.
pub(crate) fn uses(paths: &[&str]) -> String {
    let mut text = String::new();
    for path in paths.iter().chain(&["@std.process.ExitCode"]) {
        let _ = write!(text, "use {path} enduse\n\n");
    }
    text
}

/// The read-error mapping, for the other case programs.
#[cfg(windows)]
pub(crate) fn read_code() -> &'static str {
    READ_CODE
}

// ---------------------------------------------------------------------------
// The rows.
// ---------------------------------------------------------------------------

/// The world of the copy rows: `/in/a.txt`, `/in/a.bin`, the bytes file
/// `/in/bad.txt`, the 5-byte `/in/five.txt`, the directory `/out`, and the
/// arguments `args`.
fn copy_world(args: &str) -> String {
    format!(
        "{{format: @hostWorld.v2 directories: [\"/out\"] files: [\"/in/a.bin\": \
         \"binary\" \"/in/a.txt\": \"hello from a\" \"/in/bad.txt\": {{kind: @bytes \
         values: [255u8]}} \"/in/five.txt\": \"12345\"] args: [{args}]}}"
    )
}

#[test]
fn dual_copy() {
    let (world, both) = agree(
        "dual: copy",
        &copy_default(),
        &copy_program(),
        &copy_world("\"/in\" \"a.txt\""),
    );
    assert_eq!(world.result, CommandResult::Ok.as_atom());
    assert_eq!(world.application_code, Some(0));
    assert_eq!(world.stdout, b"hello from a");
    if both.system.is_ok() {
        let copied = fs::read(both.root().join("out").join("a.txt")).expect("the copy");
        assert_eq!(copied, b"hello from a", "the destination holds the text");
    }
}

#[test]
fn dual_denied_by_policy() {
    let (world, _) = agree(
        "dual: denied by policy",
        &copy_default(),
        &copy_program(),
        &copy_world("\"/in\" \"a.bin\""),
    );
    assert_eq!(
        world.application_code,
        Some(32),
        "the entry's code for denied"
    );
    assert_eq!(world.calls("fs.readText"), 0, "no read event");
}

#[test]
fn dual_not_found() {
    let (world, _) = agree(
        "dual: not found",
        &copy_default(),
        &copy_program(),
        &copy_world("\"/in\" \"no.txt\""),
    );
    assert_eq!(world.application_code, Some(31), "notFound");
    assert_eq!(world.calls("fs.readText"), 1, "one completed event");
}

#[test]
fn dual_required_refused() {
    let (world, both) = agree(
        "dual: required refused",
        &copy_default(),
        &copy_program(),
        &copy_world("\"/missing\" \"a.txt\""),
    );
    assert_eq!(world.result, CommandResult::OperationalFailure.as_atom());
    assert_eq!(world.application_code, None);
    let failure = both
        .world
        .startup
        .failure
        .as_ref()
        .expect("a startup failure");
    assert_eq!(failure.binding.to_string(), "@source");
    assert_eq!(failure.code.atom(), "@grant.requiredUnavailable");
    assert!(
        world.bindings.contains(&("@source".to_owned(), "refused")),
        "{:?}",
        world.bindings
    );
}

#[test]
fn dual_listing() {
    let document = project(
        "@std.fs.DirList",
        "@list: {provider: @host.fs.directory resource: {kind: @fixedPath path: \"in\"} \
         policy: {format: @scopePolicy.v1 family: @filesystem expression: {kind: \
         @directoryRule root: @list prefix: [\"d\"] depth: @self actions: [@list]}} \
         after: []}",
    );
    let main = format!(
        "{}proc main
  takes {{list: DirList}}
  returns code: ExitCode
  performs {{DirList}}
  do
    let Result.ok(value: path) be RelativePath([\"d\"]) endlet
    match await DirList.entries(list path)
      case Result.ok(value: [\"A\" \"a\" \"b\"])
        return ExitCode(0u8)
      case Result.ok(value: _)
        return ExitCode(1u8)
      case Result.err(error: FsError.denied())
        return ExitCode(32u8)
      case _
        return ExitCode(39u8)
    endmatch
endproc
",
        uses(&["@std.fs.DirList", "@std.fs.FsError", "@std.fs.RelativePath"])
    );
    let (world, _) = agree(
        "dual: listing",
        &document,
        &main,
        "{format: @hostWorld.v2 files: [\"/in/d/A\": \"1\" \"/in/d/a\": \"2\" \"/in/d/b\": \"3\"]}",
    );
    assert_eq!(world.application_code, Some(0), "[\"A\" \"a\" \"b\"]");
}

#[test]
fn dual_limits() {
    let (world, _) = agree(
        "dual: limits",
        &copy_project(
            "",
            TXT,
            "limits: {inputBytes: 3u64}",
            "out",
            "@create @replace",
        ),
        &copy_program(),
        &copy_world("\"/in\" \"five.txt\""),
    );
    assert_eq!(world.application_code, Some(33), "limitExceeded");
}

#[test]
fn dual_replace_then_read() {
    let out = "{kind: @fixedPath path: \"out\"}";
    let document = project(
        "@std.fs.DirReadText @std.fs.DirWriteText @std.io.StdoutText",
        &format!(
            "@out: {STDOUT} {} {}",
            file_rule("@reader", out, "", "@read", ANY_NAME, "after: []"),
            file_rule(
                "@writer",
                out,
                "",
                "@create @replace",
                ANY_NAME,
                "after: []"
            ),
        ),
    );
    let main = format!(
        "{}proc main
  takes {{reader: DirReadText writer: DirWriteText out: StdoutText}}
  returns code: ExitCode
  performs {{DirReadText DirWriteText StdoutText}}
  do
    let Result.ok(value: path) be RelativePath([\"n.txt\"]) endlet
    let Result.ok(value: _) be await DirWriteText.write(writer path \"one\") else
      return ExitCode(41u8)
    endlet
    let Result.ok(value: _) be await DirWriteText.write(writer path \"two\") else
      return ExitCode(42u8)
    endlet
    let Result.ok(value: text) be await DirReadText.read(reader path) else
      return ExitCode(43u8)
    endlet
    let _ be await StdoutText.write(out text) endlet
    return ExitCode(0u8)
endproc
",
        uses(&[
            "@std.fs.DirReadText",
            "@std.fs.DirWriteText",
            "@std.fs.RelativePath",
            "@std.io.StdoutText",
        ])
    );
    let (world, both) = agree(
        "dual: replace then read",
        &document,
        &main,
        "{format: @hostWorld.v2 directories: [\"/out\"]}",
    );
    assert_eq!(world.application_code, Some(0));
    assert_eq!(world.stdout, b"two");
    if both.system.is_ok() {
        let written =
            fs::read(both.root().join("out").join("n.txt")).expect("the file");
        assert_eq!(written, b"two");
    }
}

#[test]
fn dual_invalid_text() {
    let (world, _) = agree(
        "dual: invalid text",
        &copy_default(),
        &copy_program(),
        &copy_world("\"/in\" \"bad.txt\""),
    );
    assert_eq!(world.application_code, Some(34), "invalidText");
}

/// `compare` given two outcomes that differ in one field of one event fails
/// and names the event's index and the field.
#[test]
fn dual_disagreement_is_reported() {
    let both = run_both(
        &copy_default(),
        &copy_program(),
        &copy_world("\"/in\" \"no.txt\""),
    );
    let world = Observed::of(&both.world);
    assert_eq!(compare(&world, &world), Ok(()));
    let index = world
        .events
        .iter()
        .position(|event| event.contains(&("op", "fs.readText".to_owned())))
        .expect("the read event");
    let mut other = world.clone();
    for (name, value) in &mut other.events[index] {
        if *name == "result" {
            value.push_str(" changed");
        }
    }
    let report = compare(&world, &other).expect_err("the halves differ");
    assert!(
        report.contains(&format!("event {index} field `result`")),
        "{report}"
    );
    assert!(
        report.contains("world:") && report.contains("system:"),
        "{report}"
    );
    println!(
        "ROW dual: disagreement is reported: {}, reported as `event {index} field `result``",
        std::env::consts::OS
    );
}
