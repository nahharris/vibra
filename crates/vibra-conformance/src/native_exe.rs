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
//! # What the launcher reports
//!
//! The observation document carries the outcome, the kind of result
//! (`void-or-ok`, `err`, or none), the trap code and its origin ordinal, the
//! audit trace, the run's counters, and, for a run that completed, the entry's
//! result as an untyped structural dump (`vibra_nexus_bridge::harness`). The
//! executor reads the dump by the entry's result type with the reader the
//! stepper uses (`native_read::read`), over a different source, so a completed
//! run's result is encoded by `ObservedValue` and held to the case's result
//! snapshot like every other executor's. A dump that was cut off by a bound of
//! the launcher is not observable: the run reports no result and the case is
//! pending at the package that owns the observation of values, never
//! `matched`. The origin as a span and the standard output bytes are not in the
//! document (the audit trace carries each `io.stdout-text` event).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use vibra_build::{BuildOptions, Target, Toolchain, build_executable};
use vibra_diagnostics::DiagnosticCode;
use vibra_ir::{CheckedProgram, Type};
use vibra_syntax::{DataNode, DataValue, Literal, parse_data};

use crate::native::EntryJob;
use crate::native_read::{Definitions, DumpSource, Stop, decode_dump, read};
use crate::runner::{BackendObservation, ExecutionObservation, WasmObservation};

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
    /// Whether the run observed the entry's result: false when the run
    /// completed and the dump of its result was cut off or holds a value the
    /// reader has no row for.
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
    /// 1.
    pub(crate) fn observe(
        &self,
        case_id: &str,
        job: &EntryJob,
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
            [vibra_nexus::UNBOUNDED, 1]
                .into_iter()
                .map(|quota| {
                    launch(&built.executable, &job.world_text, quota, &job.program)
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
    program: &CheckedProgram,
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
    let definitions: Definitions<'_> = program
        .types()
        .iter()
        .map(|definition| (definition.id(), definition))
        .collect();
    parse(
        &text,
        quota,
        &program.entry().signature().result(),
        &definitions,
    )
}

/// The field `label` of a record.
fn field<'a>(
    fields: &'a [vibra_syntax::DataField],
    label: &str,
) -> Option<&'a DataNode> {
    fields
        .iter()
        .find(|field| field.label().value() == label)
        .map(vibra_syntax::DataField::value)
}

/// Decodes the observation document of the harness launcher. `result_type` is
/// the entry's result type and `definitions` the declarations it refers to.
fn parse(
    text: &str,
    quota: u64,
    result_type: &Type,
    definitions: &Definitions<'_>,
) -> Result<WindowsRun, String> {
    let malformed =
        |what: &str| format!("the observation document is malformed ({what}): {text}");
    let parsed = parse_data(Path::new("observation.vibon"), text)
        .map_err(|error| malformed(&error.to_string()))?;
    if !parsed.accepted() || parsed.recovered() {
        return Err(malformed("it is not a VIBON value"));
    }
    let root = parsed.data().ok_or_else(|| malformed("it has no value"))?;
    let DataValue::Record(fields) = root.value() else {
        return Err(malformed("it is not a record"));
    };
    let atom = |label: &str| match field(fields, label).map(DataNode::value) {
        Some(DataValue::Atom(atom)) => Ok(format!("@{}", atom.value())),
        _ => Err(malformed(&format!("`{label}` is not an atom"))),
    };
    let outcome = atom("outcome")?;
    let trap = atom("trap")?;
    let origin = match field(fields, "origin").map(DataNode::value) {
        Some(DataValue::Literal(Literal::Integer(literal))) => literal
            .digits()
            .parse::<u32>()
            .map_err(|_| malformed("`origin` is an ordinal"))?,
        _ => return Err(malformed("`origin` is not an integer")),
    };
    let audit_node = field(fields, "audit").ok_or_else(|| malformed("no `audit`"))?;
    let audit = vibra_host::decode_trace("observation.vibon", audit_node.raw())
        .map_err(|errors| format!("the audit trace does not decode: {errors:?}"))?;
    let value = field(fields, "value").ok_or_else(|| malformed("no `value`"))?;
    let completed = |failed: bool| -> Result<_, String> {
        let read_result = decode_dump(value).and_then(|tree| {
            read(&DumpSource::default(), definitions, &&tree, result_type, 0)
        });
        let (result, observed, outcome) = match read_result {
            Ok(observed) => (
                Some(observed.canonical_observation(result_type)),
                true,
                None,
            ),
            Err(Stop::Unobservable(_)) => (None, false, None),
            Err(Stop::Fault(cause)) => (
                None,
                true,
                Some(WasmObservation::Failed {
                    reason: format!(
                        "{}: the executable's result is unreadable: {cause}",
                        vibra_ir::boundary::TrapCode::InvalidCheckedProgram
                            .diagnostic_code()
                            .as_atom()
                    ),
                }),
            ),
        };
        let outcome =
            outcome.unwrap_or(WasmObservation::Completed(ExecutionObservation {
                result,
                audit_trace: audit.clone(),
            }));
        Ok((
            BackendObservation {
                outcome,
                program_failed: Some(failed),
            },
            observed,
            None,
        ))
    };
    let (observation, result_observed, trap_origin) = match outcome.as_str() {
        "@completed" => completed(false)?,
        "@program-failed" => completed(true)?,
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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const AUDIT: &str = "audit: (record format: @audit-trace.v1 events: (array))";
    const COUNTERS: &str = "counters: (record yields: 0 peak-frames: 1 tail-transfers: 0 peak-live: 96 live: 64)";

    fn document(outcome: &str, value: &str) -> String {
        format!(
            "(record outcome: @{outcome} result: @void-or-ok trap: @none origin: 0 {AUDIT} {COUNTERS} value: {value})"
        )
    }

    fn run(outcome: &str, value: &str, ty: &Type) -> Result<WindowsRun, String> {
        parse(&document(outcome, value), 1, ty, &Definitions::new())
    }

    #[test]
    fn a_completed_run_is_read_from_its_dump() {
        let scalar = r#"(record node: @scalar class: @i32 bits: "4294967289")"#;
        let run = run("completed", scalar, &Type::I32).expect("a document");
        assert!(run.result_observed);
        let WasmObservation::Completed(execution) = &run.observation.outcome else {
            panic!("{run:?}");
        };
        assert_eq!(
            execution.result.as_deref(),
            Some("(record type: @i32 value: -7i32)\n")
        );
        assert_eq!(run.observation.program_failed, Some(false));
        // The `err` outcome is the launcher's word, not the value's shape.
        let failed =
            self::run("program-failed", scalar, &Type::I32).expect("a document");
        assert_eq!(failed.observation.program_failed, Some(true));
    }

    #[test]
    fn a_truncated_dump_is_unobserved_never_matched() {
        let cut = "(record node: @truncated)";
        let nested = r#"(record node: @object kind: @tuple variant: 0 cells: (array (record node: @truncated)))"#;
        for value in [cut, nested] {
            let ty = if value == cut {
                Type::I32
            } else {
                Type::Tuple(vec![Type::I32])
            };
            let run = run("completed", value, &ty).expect("a document");
            assert!(!run.result_observed, "{value}");
            let WasmObservation::Completed(execution) = &run.observation.outcome else {
                panic!("{run:?}");
            };
            assert_eq!(execution.result, None, "no result is invented for {value}");
        }
        // A dump the reader cannot type is a failure of the run, not a result.
        let run = run("completed", r#"(record node: @str value: "x")"#, &Type::I32)
            .expect("a document");
        assert!(matches!(
            run.observation.outcome,
            WasmObservation::Failed { .. }
        ));
        // And a document that is not the launcher's is an error of the launch.
        assert!(parse("(((", 1, &Type::Void, &Definitions::new()).is_err());
        assert!(run_text("(record outcome: @completed)").is_err());
    }

    fn run_text(text: &str) -> Result<WindowsRun, String> {
        parse(text, 1, &Type::Void, &Definitions::new())
    }
}
