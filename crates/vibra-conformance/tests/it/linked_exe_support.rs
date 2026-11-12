//! What the linked-executable tests share (package X3-07): the runtime
//! archive, a scratch directory, the toolchain, a process runner, and the
//! reference stepper's expectation of a run.
//!
//! # The archive
//!
//! The runtime archive `vibra_nexus_bridge.lib` is built once per test process
//! and per launcher profile, with the cargo that runs the tests (the `CARGO`
//! variable), into a target directory of its own under the workspace `target/`,
//! so it neither deadlocks with nor invalidates the build that runs the tests.
//!
//! Two profiles are built, because each exposes what the other cannot: the
//! `system` archive (the shipped one) gives the exit code, the real standard
//! output, and the real standard error; the `harness` archive (cargo feature
//! `harness`) reads a host world from standard input and writes the
//! observation document with the outcome, the result kind, the trap, the audit
//! trace, the counters, and the dump of the result. The reference run writes the
//! same document with the test writer of the dump (`dump_text`).

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use vibra_build::{BuildOptions, Built, Target, Toolchain};
use vibra_exec::{OriginId, Program, verify};
use vibra_host::{AuditEvent, HostProvider, HostWorld, SystemProvider, WorldProvider};
use vibra_nexus::{Context, Halt, Outcome, UNBOUNDED, admit, run};
use vibra_nexus_eval::Evaluator;

use crate::support::workspace_root;

/// The memory limit of the launcher's context (`SYSTEM_MEMORY_LIMIT`), which
/// the reference run uses too.
const MEMORY_LIMIT: u64 = 256 * 1024 * 1024;

/// The host world every harness run reads: the default world.
pub(crate) const WORLD: &str = "(record format: @host-world.v1)\n";

/// A launcher profile of the runtime archive.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Profile {
    /// The shipped launcher.
    System,
    /// The launcher with the feature `harness`.
    Harness,
}

impl Profile {
    const fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Harness => "harness",
        }
    }
}

/// The archive of `profile`, built on first use.
pub(crate) fn archive(profile: Profile) -> &'static Path {
    static SYSTEM: OnceLock<PathBuf> = OnceLock::new();
    static HARNESS: OnceLock<PathBuf> = OnceLock::new();
    let cell = match profile {
        Profile::System => &SYSTEM,
        Profile::Harness => &HARNESS,
    };
    cell.get_or_init(|| build_archive(profile))
}

fn build_archive(profile: Profile) -> PathBuf {
    let root = workspace_root();
    let target = root
        .join("target")
        .join(format!("x3-07-archive-{}", profile.name()));
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(cargo);
    command
        .current_dir(&root)
        .args([
            "build",
            "--locked",
            "--offline",
            "-p",
            "vibra-nexus-bridge",
            "--release",
        ])
        .env("CARGO_TARGET_DIR", &target);
    if matches!(profile, Profile::Harness) {
        command.args(["--features", "harness"]);
    }
    let output = command.output().expect("cargo starts");
    assert!(
        output.status.success(),
        "building the {} archive failed:\n{}\n{}",
        profile.name(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let library = target.join("release").join("vibra_nexus_bridge.lib");
    assert!(library.is_file(), "{} was not built", library.display());
    library
}

/// The toolchain of this machine, discovered once.
pub(crate) fn toolchain() -> &'static Toolchain {
    static TOOLCHAIN: OnceLock<Toolchain> = OnceLock::new();
    TOOLCHAIN.get_or_init(|| match Toolchain::from_machine() {
        Ok(toolchain) => toolchain,
        Err(error) => panic!("no toolchain on this machine: {error}"),
    })
}

/// A scratch directory under the workspace `target/`, removed with the value.
#[derive(Debug)]
pub(crate) struct Scratch {
    /// The directory this value owns and removes.
    root: PathBuf,
    /// Where the test works: `root`, or the directory named `inner` inside it.
    path: PathBuf,
}

impl Scratch {
    /// A new empty directory named for `label`, this process, and a serial; the
    /// working directory is `inner` inside it when `inner` is not empty (a name
    /// with spaces and non-ASCII characters, for example).
    pub(crate) fn new(label: &str, inner: &str) -> Self {
        static SERIAL: AtomicUsize = AtomicUsize::new(0);
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let root = workspace_root()
            .join("target")
            .join("x3-07-linked")
            .join(format!("{label}-{}-{serial}", std::process::id()));
        let path = if inner.is_empty() {
            root.clone()
        } else {
            root.join(inner)
        };
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Self { root, path }
    }

    /// The directory the test works in.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The options to build into `scratch` with the archive of `profile`.
pub(crate) fn options(
    scratch: &Scratch,
    profile: Profile,
    executable: &str,
) -> BuildOptions {
    BuildOptions {
        target: Target::WINDOWS_X64,
        toolchain: toolchain().clone(),
        archive: archive(profile).to_path_buf(),
        work_dir: scratch.path().join("work"),
        output: scratch.path().join(executable),
    }
}

/// A finished process.
#[derive(Debug)]
pub(crate) struct Ran {
    /// The exit code.
    pub(crate) code: Option<i32>,
    /// Standard output.
    pub(crate) stdout: Vec<u8>,
    /// Standard error.
    pub(crate) stderr: Vec<u8>,
}

/// Runs `executable` with `arguments` and `stdin`, in an environment of
/// `SystemRoot` only, and waits at most a minute.
pub(crate) fn run_exe(executable: &Path, arguments: &[&str], stdin: &[u8]) -> Ran {
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", root);
    }
    let mut child = command.spawn().expect("the executable starts");
    let mut input = child.stdin.take().expect("a piped stdin");
    let bytes = stdin.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = input.write_all(&bytes);
    });
    let mut out = child.stdout.take().expect("a piped stdout");
    let mut err = child.stderr.take().expect("a piped stderr");
    let out_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = out.read_to_end(&mut bytes);
        bytes
    });
    let err_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = err.read_to_end(&mut bytes);
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait().expect("the process can be polled") {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "{} ran for a minute and was stopped",
            executable.display()
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    let _ = writer.join();
    Ran {
        code: status.code(),
        stdout: out_reader.join().expect("stdout is read"),
        stderr: err_reader.join().expect("stderr is read"),
    }
}

// -- the reference stepper ---------------------------------------------------

/// How the reference run ended.
#[derive(Debug, PartialEq, Eq)]
enum Ending {
    Completed,
    ProgramFailed,
    Trapped(&'static str, u32),
    MemoryExhausted,
}

/// What a launcher is expected to do, computed from a run of the reference
/// stepper and from the tooling chapter's exit mapping, never from the native
/// launcher.
#[derive(Debug)]
pub(crate) struct Reference {
    /// The process exit code of the `system` launcher.
    pub(crate) exit_code: i32,
    /// Standard output of the `system` launcher.
    pub(crate) stdout: Vec<u8>,
    /// Standard error of the `system` launcher.
    pub(crate) stderr: String,
    /// The line the `harness` launcher writes to standard output, without its
    /// line feed.
    pub(crate) observation: String,
}

/// What a run of the reference stepper leaves for the harness document: the
/// audit trace, the counters, and the dump of the result.
struct Seen {
    audit: Vec<AuditEvent>,
    counters: Option<String>,
    value: Option<String>,
}

fn drive(
    code: &Evaluator<'_>,
    host: &mut dyn HostProvider,
    err_variant: Option<u32>,
    quota: u64,
) -> (Ending, Seen) {
    let mut ctx = Context::new(MEMORY_LIMIT, code.globals(), host);
    if let Err(halt) = code.start(&mut ctx) {
        assert_eq!(halt, Halt::Exhausted, "the entry frame could not be pushed");
        return (
            Ending::MemoryExhausted,
            Seen {
                audit: Vec::new(),
                counters: None,
                value: None,
            },
        );
    }
    let outcome = loop {
        match run(&mut ctx, code, quota) {
            Outcome::Yielded => {}
            other => break other,
        }
    };
    let ending = match outcome {
        Outcome::Completed => {
            if err_variant.is_some_and(|variant| ctx.result_is_variant(variant)) {
                Ending::ProgramFailed
            } else {
                Ending::Completed
            }
        }
        Outcome::Trapped(trap) => {
            Ending::Trapped(trap.code.diagnostic_code().as_atom(), trap.origin)
        }
        Outcome::MemoryExhausted => Ending::MemoryExhausted,
        other => panic!("the stepper ended in {other:?}"),
    };
    let stats = ctx.stats();
    let result = ctx.result();
    let value = matches!(ending, Ending::Completed | Ending::ProgramFailed)
        .then(|| crate::dump_text::dump(ctx.store(), result));
    let counters = format!(
        "(record yields: {} peak-frames: {} tail-transfers: {} peak-live: {} live: {})",
        stats.yields,
        stats.max_depth,
        stats.tail_transfers,
        ctx.peak_size(),
        ctx.live_size()
    );
    let seen = Seen {
        audit: ctx.audit().to_vec(),
        counters: Some(counters),
        value,
    };
    (ending, seen)
}

fn origin_text(program: &Program, ordinal: u32) -> Option<String> {
    program.origin(OriginId(ordinal)).map(|origin| {
        format!(
            "{}:{}..{}",
            origin.source_id(),
            origin.span().start(),
            origin.span().end()
        )
    })
}

/// What the launchers are expected to do with `program` under `quota`.
/// `err_variant` is the variant index of `err` when the entry returns a
/// `result`.
///
/// `capture_stdout` also runs the program on the system provider with its
/// output captured; without it `stdout` is empty and not to be compared.
pub(crate) fn reference(
    program: &Program,
    err_variant: Option<u32>,
    quota: u64,
    capture_stdout: bool,
) -> Reference {
    verify(program).expect("the program verifies");
    let code = Evaluator::new(program).expect("the stepper takes the program");
    admit(&code.requirements()).expect("an admissible program");

    // The system launcher's world: the real streams, captured.
    let mut system = SystemProvider::new().capturing();
    let captured = capture_stdout.then(|| {
        let (ending, _) = drive(&code, &mut system, err_variant, quota);
        (ending, system.captured_stdout().to_vec())
    });

    // The harness launcher's world: the document it reads from standard input.
    let world = HostWorld::decode("host-world.vibon", WORLD).expect("a host world");
    let mut provider = WorldProvider::new(&world);
    let (ending, seen) = drive(&code, &mut provider, err_variant, quota);
    let stdout = captured.map_or_else(Vec::new, |(system_ending, stdout)| {
        assert_eq!(system_ending, ending, "the two providers end alike");
        stdout
    });

    let (exit_code, stderr) = match &ending {
        Ending::Completed => (0, String::new()),
        Ending::ProgramFailed => (6, "@command.program-failed\n".to_owned()),
        Ending::Trapped(atom, ordinal) => (
            5,
            match origin_text(program, *ordinal) {
                Some(origin) => format!("{atom} at {origin}\n"),
                None => format!("{atom}\n"),
            },
        ),
        Ending::MemoryExhausted => (3, "@runtime.memory-exhausted\n".to_owned()),
    };
    let (outcome, result, trap, origin) = match &ending {
        Ending::Completed => ("completed", "void-or-ok", "@none".to_owned(), 0),
        Ending::ProgramFailed => ("program-failed", "err", "@none".to_owned(), 0),
        Ending::Trapped(atom, ordinal) => {
            ("trapped", "none", (*atom).to_owned(), *ordinal)
        }
        Ending::MemoryExhausted => ("memory-exhausted", "none", "@none".to_owned(), 0),
    };
    let observation = format!(
        "(record outcome: @{outcome} result: @{result} trap: {trap} origin: {origin} audit: {} counters: {} value: {})",
        vibra_host::trace_to_vibon(&seen.audit),
        seen.counters.as_deref().unwrap_or("@none"),
        seen.value.as_deref().unwrap_or("@none")
    );
    Reference {
        exit_code,
        stdout,
        stderr,
        observation,
    }
}

// -- the comparison -----------------------------------------------------------

/// What a program is expected to do, written by hand from the specification.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Expected {
    /// The process exit code of the tooling chapter.
    pub(crate) exit_code: i32,
    /// The bytes written to standard output.
    pub(crate) stdout: &'static str,
}

/// The two executables of one program, one for each launcher profile.
#[derive(Debug)]
pub(crate) struct Linked {
    /// The `system` executable and how it was built.
    pub(crate) system: Built,
    /// The `harness` executable and how it was built.
    pub(crate) harness: Built,
}

/// How much of the stepper a comparison runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    /// Unbounded and quota 1, each on the system provider (for its output) and
    /// on a world.
    Full,
    /// Unbounded, on a world only: the stepper is unoptimized in the test
    /// profile and a 100,000-round loop takes it seconds. The standard output
    /// of the process is held to the hand-written expectation only.
    Deep,
}

/// Compares everything the two launchers expose today with the reference run:
/// for the `system` launcher the exit code, the standard output bytes, and the
/// standard error; for the `harness` launcher the observation document, under
/// the unbounded run and, for [`Scope::Full`], quota 1. Each is also held to
/// `expected`.
///
/// Every failure message carries the link command of the executable involved.
pub(crate) fn assert_agrees(
    label: &str,
    linked: &Linked,
    program: &Program,
    err_variant: Option<u32>,
    expected: Expected,
    scope: Scope,
) {
    let system_context = format!("{label} (system)\nlink: {}", linked.system.command);
    let full = scope == Scope::Full;
    let unbounded = reference(program, err_variant, UNBOUNDED, full);
    assert_eq!(
        unbounded.exit_code, expected.exit_code,
        "{label}: the stepper's exit code is not the one written by hand"
    );
    let want_stdout = if full {
        assert_eq!(
            String::from_utf8_lossy(&unbounded.stdout),
            expected.stdout,
            "{label}: the stepper's standard output is not the one written by hand"
        );
        unbounded.stdout.clone()
    } else {
        expected.stdout.as_bytes().to_vec()
    };

    let ran = run_exe(&linked.system.executable, &[], b"");
    assert_eq!(
        ran.code,
        Some(expected.exit_code),
        "{system_context}\nstderr: {}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&want_stdout),
        "{system_context}: standard output"
    );
    assert_eq!(ran.stdout, want_stdout, "{system_context}: stdout bytes");
    assert_eq!(
        String::from_utf8_lossy(&ran.stderr),
        unbounded.stderr,
        "{system_context}: standard error"
    );

    let quotas: &[u64] = if full { &[UNBOUNDED, 1] } else { &[UNBOUNDED] };
    for &quota in quotas {
        let later;
        let reference_run = if quota == UNBOUNDED {
            &unbounded
        } else {
            later = reference(program, err_variant, quota, false);
            &later
        };
        let harness_context = format!(
            "{label} (harness, quota {quota})\nlink: {}",
            linked.harness.command
        );
        let argument = quota.to_string();
        let arguments: &[&str] = if quota == UNBOUNDED {
            &[]
        } else {
            &[&argument]
        };
        let ran = run_exe(&linked.harness.executable, arguments, WORLD.as_bytes());
        assert_eq!(ran.code, Some(0), "{harness_context}");
        assert_eq!(
            String::from_utf8_lossy(&ran.stdout),
            format!("{}\n", reference_run.observation),
            "{harness_context}: the observation document"
        );
        assert!(
            ran.stderr.is_empty(),
            "{harness_context}: standard error: {}",
            String::from_utf8_lossy(&ran.stderr)
        );
    }
}
