//! The `windows` column of the native parity inventory: a case's linked
//! executable, run as a process.
//!
//! For an accepted executable case whose stepper run completed, the executor
//! builds the checked program with `vibra_build::build_executable` against the
//! runtime archive the caller names, which must be the `harness` archive, and
//! launches the executable twice: unbounded, and with quota 1. The harness
//! launcher takes the quota as its first argument and the case's `@host-world.v1`
//! document on standard input, and writes one observation document
//! (`vibra_nexus_bridge::harness::observation`). Each run is turned into a
//! [`WasmObservation`] and held to the case's one expectation by
//! `runner::executor_status`, the function the Wasm backend and the stepper are
//! held to.
//!
//! This is the one module of the runner that starts a process.
//!
//! # What the launcher cannot report
//!
//! The observation document carries the outcome, the kind of result
//! (`void-or-ok`, `err`, or none), the trap code and its origin ordinal, and the
//! audit trace. It does not carry the entry's result value (package X3-18), the
//! origin as a span, or the standard output bytes (the audit trace carries each
//! `io.stdout-text` event). The result of an entry of type `void` has one
//! inhabitant, so a run that completed has observed it, and the executor
//! supplies it. For any other completed run the executor lends the stepper's
//! result of the same quota so that every other field is still compared, and
//! reports the run as not having verified the result: a case with a result
//! snapshot is then `pending X3-09` and never `matched`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use vibra_build::{BuildOptions, Target, Toolchain, build_executable};
use vibra_diagnostics::DiagnosticCode;
use vibra_ir::{ObservedValue, Type, Value};

use crate::native::{EntryJob, StepperRun};
use crate::runner::{BackendObservation, ExecutionObservation, WasmObservation};

/// The package that owns the observation of the entry's result value; a case
/// the executable matches except for that value waits there.
pub const RESULT_PACKAGE: &str = "X3-09";

/// How long a launched executable may run.
const TIMEOUT: Duration = Duration::from_secs(120);

/// One launch of a case's executable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WindowsRun {
    /// The quota of the run: [`u64::MAX`] for unbounded.
    pub(crate) quota: u64,
    /// What the executable did, in the terms the Wasm backend uses.
    pub(crate) observation: BackendObservation<WasmObservation>,
    /// The origin ordinal the launcher reported for a trap.
    pub(crate) trap_origin: Option<u32>,
    /// Whether the run observed the entry's result: false when the result was
    /// lent by the stepper.
    pub(crate) result_observed: bool,
}

/// Why the `windows` column cannot run at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsUnavailable(pub String);

impl std::fmt::Display for WindowsUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Builds and launches the executables of the `windows` column.
#[derive(Clone, Debug)]
pub struct WindowsExecutor {
    toolchain: Toolchain,
    archive: PathBuf,
    work_dir: PathBuf,
}

impl WindowsExecutor {
    /// An executor over the `harness` runtime `archive`, building under
    /// `work_dir`. The runner never builds or searches for the archive.
    ///
    /// # Errors
    ///
    /// [`WindowsUnavailable`]: the host is not Windows, the archive is not a
    /// file, or no MSVC toolchain is found.
    pub fn new(
        archive: PathBuf,
        work_dir: PathBuf,
    ) -> Result<Self, WindowsUnavailable> {
        if !cfg!(windows) {
            return Err(WindowsUnavailable("this host is not Windows".to_owned()));
        }
        if !archive.is_file() {
            return Err(WindowsUnavailable(format!(
                "the runtime archive {} is not a file",
                archive.display()
            )));
        }
        let toolchain = Toolchain::from_machine()
            .map_err(|error| WindowsUnavailable(format!("no toolchain: {error}")))?;
        Ok(Self {
            toolchain,
            archive,
            work_dir,
        })
    }

    /// Builds the case's executable and launches it unbounded and with quota
    /// 1. `stepper` is the stepper's runs of the same case.
    pub(crate) fn observe(
        &self,
        case_id: &str,
        job: &EntryJob,
        stepper: &[StepperRun],
    ) -> Result<Vec<WindowsRun>, String> {
        let directory = self.work_dir.join(case_id);
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("{}: {error}", directory.display()))?;
        let options = BuildOptions {
            target: Target::WINDOWS_X64,
            toolchain: self.toolchain.clone(),
            archive: self.archive.clone(),
            work_dir: directory.join("work"),
            output: directory.join("case.exe"),
        };
        let built = build_executable(&job.program, &options)
            .map_err(|error| format!("the build failed: {error}"));
        let outcome = built.and_then(|built| {
            let void = job.program.entry().signature().result() == Type::Void;
            [vibra_nexus::UNBOUNDED, 1]
                .into_iter()
                .map(|quota| {
                    let lent = stepper.iter().find(|run| run.quota == quota);
                    launch(&built.executable, &job.world_text, quota, void, lent)
                        .map_err(|error| format!("{error}\nlink: {}", built.command))
                })
                .collect()
        });
        let _ = std::fs::remove_dir_all(&directory);
        outcome
    }
}

fn launch(
    executable: &Path,
    world_text: &str,
    quota: u64,
    void_entry: bool,
    stepper: Option<&StepperRun>,
) -> Result<WindowsRun, String> {
    let mut command = Command::new(executable);
    if quota != vibra_nexus::UNBOUNDED {
        command.arg(quota.to_string());
    }
    command
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", root);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("the executable does not start: {error}"))?;
    let mut input = child.stdin.take().ok_or("no stdin")?;
    let world = world_text.as_bytes().to_vec();
    let writer = std::thread::spawn(move || {
        let _ = input.write_all(&world);
    });
    let mut out = child.stdout.take().ok_or("no stdout")?;
    let mut err = child.stderr.take().ok_or("no stderr")?;
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
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("the process cannot be polled: {error}"))?
        {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "the executable ran for {TIMEOUT:?} and was stopped"
            ));
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    let _ = writer.join();
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    if status.code() != Some(0) {
        return Err(format!(
            "the harness launcher exited with {:?}: {}",
            status.code(),
            String::from_utf8_lossy(&stderr)
        ));
    }
    let text = String::from_utf8_lossy(&stdout).into_owned();
    parse(&text, quota, void_entry, stepper)
}

/// Decodes the observation document of the harness launcher.
fn parse(
    text: &str,
    quota: u64,
    void_entry: bool,
    stepper: Option<&StepperRun>,
) -> Result<WindowsRun, String> {
    let malformed = || format!("the observation document is malformed: {text}");
    let (head, audit_text) = text.split_once(" audit: ").ok_or_else(malformed)?;
    let audit_text = audit_text
        .trim_end()
        .strip_suffix(')')
        .ok_or_else(malformed)?;
    let field = |label: &str| {
        head.split_whitespace()
            .skip_while(|word| *word != label)
            .nth(1)
            .map(str::to_owned)
            .ok_or_else(malformed)
    };
    let outcome = field("outcome:")?;
    let trap = field("trap:")?;
    let origin = field("origin:")?.parse::<u32>().map_err(|_| malformed())?;
    let audit = vibra_host::decode_trace("observation.vibon", audit_text)
        .map_err(|errors| format!("the audit trace does not decode: {errors:?}"))?;
    let lent = stepper.and_then(|run| match &run.observation.outcome {
        WasmObservation::Completed(execution) => execution.result.clone(),
        _ => None,
    });
    let completed = |failed: bool| {
        let (result, observed) = if void_entry {
            (
                Some(
                    ObservedValue::Primitive(Value::Void)
                        .canonical_observation(&Type::Void),
                ),
                true,
            )
        } else {
            (lent.clone(), false)
        };
        (
            BackendObservation {
                outcome: WasmObservation::Completed(ExecutionObservation {
                    result,
                    audit_trace: audit.clone(),
                }),
                program_failed: Some(failed),
            },
            observed,
            None,
        )
    };
    let (observation, result_observed, trap_origin) = match outcome.as_str() {
        "@completed" => completed(false),
        "@program-failed" => completed(true),
        "@trapped" => (
            BackendObservation::new(WasmObservation::Trapped(trap)),
            true,
            Some(origin),
        ),
        "@memory-exhausted" => (
            BackendObservation::new(WasmObservation::HostEvent(
                DiagnosticCode::RuntimeMemoryExhausted.as_atom().to_owned(),
            )),
            true,
            None,
        ),
        other => {
            return Err(format!("the launcher ended in `{other}`: {text}"));
        }
    };
    Ok(WindowsRun {
        quota,
        observation,
        trap_origin,
        result_observed,
    })
}
