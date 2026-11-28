//! The run of a binary target: admit, start, enter, map the ending, and
//! release.
//!
//! `docs/spec/06-runtime.md`, "Entry outcome", "Traps", and "Host world"
//! ("Recipes under a world"), and `docs/spec/05-tooling.md`, the result
//! table and "Exit code of a program", are the authority; the observation is
//! the `@targetRun.v2` document of `docs/spec/07-diagnostics-and-conformance.md`,
//! "Persistent observations".
//!
//! [`run_target`] selects the target (the one binary target when none is
//! named), checks the snapshot for it, and ends with `@command.diagnostics`
//! and no startup when the check has an error. It then builds the driver of
//! the [`Launch`], admits the target's validated binding plan against the
//! checked entry and the target's `effects`, starts it, runs the entry with
//! the bound grants as the invocation after the startup reads, maps the
//! ending to a [`CommandResult`], and releases. Every ending after startup
//! begins passes the one release, in reverse acquisition order.
//!
//! | Ending | Result | Record |
//! | --- | --- | --- |
//! | `ExitCode` zero | `@command.ok` | `applicationCode` 0, `programResult` the canonical text |
//! | `ExitCode` non-zero | `@command.programFailed` | `applicationCode` the code, no trap |
//! | a refused required leaf, a failed acquisition | `@command.operationalFailure` | the startup diagnostic, no `programResult` |
//! | a defect trap | `@command.trap` | the trap record with its source origin, when known |
//! | memory exhaustion | `@command.operationalFailure` | `@runtime.memoryExhausted`, the limit `memory` reached |
//! | a module nested past the source nesting limit | `@command.diagnostics` | `@tool.limitExceeded` alone, the limit `sourceNesting` reached, no startup |
//! | no driver for a provider | `@command.unavailable` | `@tool.unavailable` |
//!
//! Under [`Launch::World`] the program's streams are the world's, and the
//! outcome's `stdout` and `stderr` are what the entry wrote. Under
//! [`Launch::System`] the program's input is the process's own; its output
//! goes to the process's own streams, and the outcome holds no copy of it,
//! unless the launch captures it, as `vibra run` does in JSON format: then
//! the outcome's `stdout` and `stderr` are what the entry wrote, and nothing
//! reaches the process's streams.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::rc::Rc;
use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_host::broker::Context;
use vibra_host::driver::Driver;
use vibra_host::recipe::LeafName;
use vibra_host::startup::{
    self, AdmissionRefusal, EntryLeaf, EntryShape, Started, StartupRecord,
};
use vibra_host::system::{
    Output, OutputSink, SystemDriver, SystemInputs, WriteFailure,
};
use vibra_host::trace::{
    self, AuditTrace, Event, FailureCode, NO_REDACTION, StdlibIdentity, TraceHeader,
};
use vibra_host::world::{HostWorld, WorldDriver};
use vibra_interp::{EntryArgument, Execution, MemoryBudget, RuntimeError, run_entry};
use vibra_ir::authority::{GrantToken, HostFault};
use vibra_ir::external::host::HostInterface;
use vibra_ir::{CheckedProgram, SourceOrigin, Trap, Value};
use vibra_syntax::span_record;
use vibra_types::{GrantShape, load_stdlib};

use crate::entry::EntryFacts;
use crate::project::TargetKind;
use crate::revision::workspace_revision;
use crate::semantic::check_snapshot;
use crate::snapshot::SourceSnapshot;

/// The format atom of the observation.
pub const TARGET_RUN_FORMAT: &str = "@targetRun.v2";

/// Where the grants of a run come from.
#[derive(Clone, Debug)]
pub enum Launch<'a> {
    /// A fake world: a fresh copy of the document's world, for conformance
    /// and the tool's tests.
    World(&'a HostWorld),
    /// The process: `vibra run`, with the program's arguments.
    System {
        /// The program's arguments, in order.
        arguments: Vec<OsString>,
        /// Whether the program's standard output and standard error are
        /// kept in the outcome instead of written to the process's own.
        capture: bool,
    },
}

/// A standard output or error kept in memory, shared with the run that
/// reads it once every grant is released.
#[derive(Clone, Debug, Default)]
struct Captured(Rc<RefCell<Vec<u8>>>);

impl Captured {
    fn bytes(&self) -> Vec<u8> {
        self.0.borrow().clone()
    }
}

impl OutputSink for Captured {
    fn write(&mut self, bytes: &[u8]) -> Result<(), WriteFailure> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(())
    }
}

/// The `@command.*` result of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandResult {
    /// `@command.ok`: the entry returned `ExitCode` zero.
    Ok,
    /// `@command.diagnostics`: static diagnostics prevent admission.
    Diagnostics,
    /// `@command.invalidInput`: no target, or a target that is no binary
    /// target, was selected.
    InvalidInput,
    /// `@command.operationalFailure`: a startup failure, memory exhaustion,
    /// or a tool failure.
    OperationalFailure,
    /// `@command.unavailable`: a form or a driver this toolchain does not
    /// have.
    Unavailable,
    /// `@command.trap`: a defect trap.
    Trap,
    /// `@command.programFailed`: the entry returned a non-zero `ExitCode`.
    ProgramFailed,
}

impl CommandResult {
    /// The atom.
    #[must_use]
    pub const fn as_atom(self) -> &'static str {
        match self {
            Self::Ok => "@command.ok",
            Self::Diagnostics => "@command.diagnostics",
            Self::InvalidInput => "@command.invalidInput",
            Self::OperationalFailure => "@command.operationalFailure",
            Self::Unavailable => "@command.unavailable",
            Self::Trap => "@command.trap",
            Self::ProgramFailed => "@command.programFailed",
        }
    }
}

/// The defect trap that ended a run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrapRecord {
    /// The trap.
    pub trap: Trap,
    /// The source origin of the trapping operation, when one is known.
    pub origin: Option<SourceOrigin>,
}

/// How one run of a target ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunOutcome {
    /// The selected binary target's atom; `None` when none was selected.
    pub target: Option<String>,
    /// The `@command.*` result.
    pub result: CommandResult,
    /// The code the entry returned; `None` when it did not return.
    pub application_code: Option<u8>,
    /// The canonical text of the entry's result; `None` when it did not
    /// return.
    pub program_result: Option<String>,
    /// The startup record, every acquired binding `released`.
    pub startup: StartupRecord,
    /// The run's whole trace, its startup events first.
    pub trace: AuditTrace,
    /// The defect trap that ended the run, when one did.
    pub trap: Option<TrapRecord>,
    /// Every diagnostic: the check's, then that of the ending, if any.
    pub diagnostics: Vec<Diagnostic>,
    /// The limits the run reached, in order: `memory` after exhaustion.
    pub limits_reached: Vec<String>,
    /// Every byte the program wrote to its standard output.
    pub stdout: Vec<u8>,
    /// Every byte the program wrote to its standard error.
    pub stderr: Vec<u8>,
    /// The lines of the tool's log: each release the driver reported as
    /// failed, which changes neither the result nor the observation.
    pub log: Vec<String>,
}

impl RunOutcome {
    /// The canonical text of the `@targetRun.v2` observation.
    #[must_use]
    pub fn document(&self) -> String {
        let mut fields = vec![
            format!("format: {TARGET_RUN_FORMAT}"),
            format!("result: {}", self.result.as_atom()),
        ];
        if let Some(code) = self.application_code {
            fields.push(format!("applicationCode: {code}u8"));
        }
        fields.push(format!("stdout: {}", bytes_record(&self.stdout)));
        fields.push(format!("stderr: {}", bytes_record(&self.stderr)));
        let failed = self
            .startup
            .failure
            .as_ref()
            .map(|failure| (&failure.binding, failure.code));
        let startup = self
            .startup
            .bindings
            .iter()
            .map(|binding| {
                let failure = failed
                    .filter(|(leaf, _)| **leaf == binding.name)
                    .map(|(_, code)| format!(" failure: {}", code.atom()))
                    .unwrap_or_default();
                format!(
                    "{{binding: {} status: @{}{failure}}}",
                    binding.name,
                    binding.status.spelling()
                )
            })
            .collect::<Vec<_>>();
        fields.push(format!("startup: [{}]", startup.join(" ")));
        if let Some(trap) = &self.trap {
            let origin = trap
                .origin
                .as_ref()
                .map(|origin| {
                    format!(
                        " origin: {}",
                        span_record(origin.source_id(), origin.span())
                    )
                })
                .unwrap_or_default();
            fields.push(format!(
                "trap: {{trapCode: {}{origin}}}",
                trap.trap.code().as_atom()
            ));
        }
        fields.push(format!("auditTrace: {}", trace::encode(&self.trace)));
        format!("{{{}}}", fields.join(" "))
    }
}

/// A bytes record: `{kind: @bytes values: [...]}`.
fn bytes_record(bytes: &[u8]) -> String {
    let values = bytes
        .iter()
        .map(|byte| format!("{byte}u8"))
        .collect::<Vec<_>>();
    format!("{{kind: @bytes values: [{}]}}", values.join(" "))
}

/// Runs the binary target `target` of `snapshot`, or its one binary target
/// when `target` is `None`, under `launch` and `budget`.
#[must_use]
pub fn run_target(
    snapshot: &SourceSnapshot,
    target: Option<&str>,
    launch: Launch<'_>,
    budget: MemoryBudget,
) -> RunOutcome {
    let revision = workspace_revision(&snapshot.revision_records()).unwrap_or_default();
    match prepare(snapshot, target, &revision) {
        Err(ended) => *ended,
        Ok(prepared) => match launch {
            Launch::World(world) => {
                prepared.run(&mut WorldDriver::new(world), budget, |driver| {
                    let output = driver.output();
                    (output.stdout.to_vec(), output.stderr.to_vec())
                })
            }
            Launch::System { arguments, capture } => {
                let mut inputs = SystemInputs::process(arguments);
                let captured = capture.then(|| {
                    let streams = (Captured::default(), Captured::default());
                    inputs.stdout = Output::Replaced(Box::new(streams.0.clone()));
                    inputs.stderr = Output::Replaced(Box::new(streams.1.clone()));
                    streams
                });
                prepared.run(&mut SystemDriver::new(inputs), budget, |_| {
                    captured.map_or_else(
                        || (Vec::new(), Vec::new()),
                        |(stdout, stderr)| (stdout.bytes(), stderr.bytes()),
                    )
                })
            }
        },
    }
}

/// The trace of a run that recorded nothing.
fn empty_trace(revision: &str, target: Option<&str>) -> AuditTrace {
    AuditTrace {
        revision: revision.to_owned(),
        target: target.map(str::to_owned),
        drivers: Vec::new(),
        policies: BTreeMap::new(),
        redaction: NO_REDACTION.to_owned(),
        events: Vec::new(),
    }
}

/// An outcome that ended before startup began.
fn before_startup(
    revision: &str,
    target: Option<&str>,
    result: CommandResult,
    diagnostics: Vec<Diagnostic>,
    trap: Option<TrapRecord>,
) -> Box<RunOutcome> {
    Box::new(RunOutcome {
        target: target.map(str::to_owned),
        result,
        application_code: None,
        program_result: None,
        startup: StartupRecord::not_started(),
        trace: empty_trace(revision, target),
        trap,
        diagnostics,
        limits_reached: Vec::new(),
        stdout: Vec::new(),
        stderr: Vec::new(),
        log: Vec::new(),
    })
}

/// A defect of the toolchain found before startup: the trap
/// `@runtime.invalidCheckedProgram`, with no origin.
fn defect(
    revision: &str,
    target: &str,
    mut diagnostics: Vec<Diagnostic>,
    message: String,
) -> Box<RunOutcome> {
    diagnostics.push(Diagnostic::new(
        DiagnosticCode::RuntimeInvalidCheckedProgram,
        ByteSpan::empty_at(0),
        message,
    ));
    before_startup(
        revision,
        Some(target),
        CommandResult::Trap,
        diagnostics,
        Some(TrapRecord {
            trap: Trap::InvalidCheckedProgram,
            origin: None,
        }),
    )
}

/// What admission needs of the checked entry of `facts`.
fn entry_shape(facts: &EntryFacts) -> Option<EntryShape> {
    let leaves = facts
        .leaves()
        .iter()
        .map(|leaf| {
            Some(EntryLeaf {
                name: LeafName::new(leaf.name()),
                interface: HostInterface::by_owner_path(&format!(
                    "@{}",
                    leaf.leaf().interface().path()
                ))?,
                optional: leaf.leaf().optional(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(EntryShape {
        row: facts.row(),
        leaves,
    })
}

/// Everything a run needs before it builds its driver.
struct Prepared {
    target: String,
    revision: String,
    project_source: String,
    target_span: ByteSpan,
    header: TraceHeader,
    facts: EntryFacts,
    shape: EntryShape,
    program: CheckedProgram,
    diagnostics: Vec<Diagnostic>,
}

/// Selects and checks the target, and lowers its entry; an outcome when the
/// run ends before startup.
fn prepare(
    snapshot: &SourceSnapshot,
    target: Option<&str>,
    revision: &str,
) -> Result<Prepared, Box<RunOutcome>> {
    let project = snapshot.project().project();
    let invalid = || {
        before_startup(
            revision,
            None,
            CommandResult::InvalidInput,
            Vec::new(),
            None,
        )
    };
    let selected = match target {
        Some(name) => project
            .targets()
            .iter()
            .find(|declared| declared.name() == name)
            .filter(|declared| declared.kind() == TargetKind::Bin),
        None => {
            let mut binaries = project
                .targets()
                .iter()
                .filter(|declared| declared.kind() == TargetKind::Bin);
            binaries.next().filter(|_| binaries.next().is_none())
        }
    };
    let selected = selected.ok_or_else(invalid)?;
    let name = selected.name();
    let check = check_snapshot(snapshot, Some(name));
    let diagnostics = check.diagnostics().to_vec();
    if check.has_errors() {
        let result = if check.only_unavailable() {
            CommandResult::Unavailable
        } else {
            CommandResult::Diagnostics
        };
        let mut outcome =
            before_startup(revision, Some(name), result, diagnostics, None);
        // A reached limit, a module nested past the source nesting limit
        // among them, is an unknown, incomplete observation.
        outcome
            .limits_reached
            .extend(check.limits_reached().iter().cloned());
        return Err(outcome);
    }
    let library = match load_stdlib() {
        Ok(stdlib) => StdlibIdentity {
            package: stdlib.package_name().to_owned(),
            version: stdlib.package_version().to_owned(),
        },
        Err(error) => {
            let mut diagnostics = diagnostics;
            diagnostics.push(error.diagnostic().clone());
            return Err(before_startup(
                revision,
                Some(name),
                CommandResult::Diagnostics,
                diagnostics,
                None,
            ));
        }
    };
    let Some(facts) = check.entries().iter().find(|facts| facts.target() == name)
    else {
        return Err(defect(
            revision,
            name,
            diagnostics,
            format!("the checked target `{name}` has no entry facts"),
        ));
    };
    let entry = check.check().and_then(|checked| {
        let set = checked.module_set()?;
        let index = checked
            .functions()
            .iter()
            .find(|(function, _)| function == facts.signature().function())
            .map(|(_, index)| *index)?;
        Some(CheckedProgram::for_entry(Arc::clone(set), index))
    });
    let program = match entry {
        Some(Ok(program)) => program,
        Some(Err(error)) => {
            return Err(defect(
                revision,
                name,
                diagnostics,
                format!("the entry of `{name}` is not a valid entry: {error}"),
            ));
        }
        None => {
            return Err(defect(
                revision,
                name,
                diagnostics,
                format!("the entry of `{name}` was not lowered"),
            ));
        }
    };
    let Some(shape) = entry_shape(facts) else {
        return Err(defect(
            revision,
            name,
            diagnostics,
            format!("a grant leaf of `{name}` has no host interface"),
        ));
    };
    Ok(Prepared {
        target: name.to_owned(),
        revision: revision.to_owned(),
        project_source: project.source_id().to_owned(),
        target_span: selected.name_span(),
        header: TraceHeader {
            revision: revision.to_owned(),
            target: Some(name.to_owned()),
            library,
        },
        facts: facts.clone(),
        shape,
        program,
        diagnostics,
    })
}

/// How the part of a run after admission ended.
enum Ending {
    /// Admission refused the plan; nothing was acquired.
    Refused(AdmissionRefusal),
    /// A fault of the broker ended startup.
    StartupFault(HostFault),
    /// A leaf stopped startup.
    StartupFailed,
    /// The bindings are not the entry's parameters.
    Arguments(String),
    /// The entry ran.
    Entered(Box<Result<Execution, RuntimeError>>),
}

impl Prepared {
    /// Admits, starts, enters, releases, and maps, on `driver`; `output`
    /// reads what the program wrote once every grant is released.
    fn run<D: Driver>(
        self,
        driver: &mut D,
        budget: MemoryBudget,
        output: impl FnOnce(&D) -> (Vec<u8>, Vec<u8>),
    ) -> RunOutcome {
        let ceiling = self.facts.ceiling();
        let admitted =
            startup::admit(self.facts.plan(), &self.shape, &ceiling, &*driver);
        let (ending, record, trace, log) = match admitted {
            Err(refusal) => (
                Ending::Refused(refusal),
                StartupRecord::not_started(),
                empty_trace(&self.revision, Some(&self.target)),
                Vec::new(),
            ),
            Ok(admission) => {
                let mut context = Context::new(&mut *driver, self.header.clone());
                let mut started = startup::start(admission, &mut context);
                let ending = self.enter(&mut started, budget);
                // The one release of every acquired grant, in reverse order.
                let released = startup::release(started);
                let log = released.failures.iter().map(ToString::to_string).collect();
                (ending, released.record, context.trace().clone(), log)
            }
        };
        let (stdout, stderr) = output(driver);
        let mut outcome = self.outcome(ending, record, trace, stdout, stderr);
        outcome.log = log;
        outcome
    }

    /// Runs the entry when startup bound every leaf.
    fn enter(&self, started: &mut Started<'_, '_>, budget: MemoryBudget) -> Ending {
        if let Some(fault) = &started.fault {
            return Ending::StartupFault(fault.clone());
        }
        let Some(bindings) = started.bindings.clone() else {
            return Ending::StartupFailed;
        };
        let bound = bindings
            .iter()
            .map(|(leaf, token)| (leaf.as_str(), *token))
            .collect::<BTreeMap<_, _>>();
        match arguments(self.facts.signature().parameters(), &bound) {
            Err(why) => Ending::Arguments(why),
            Ok(grants) => {
                let first = started.next_invocation;
                Ending::Entered(Box::new(run_entry(
                    &self.program,
                    grants,
                    first,
                    started.context(),
                    budget,
                )))
            }
        }
    }

    /// The diagnostic of the leaf that stopped startup, at its recipe.
    fn startup_failure(&self, record: &StartupRecord) -> Option<Diagnostic> {
        let failure = record.failure.as_ref()?;
        let code = match failure.code {
            FailureCode::RequiredUnavailable => {
                DiagnosticCode::GrantRequiredUnavailable
            }
            FailureCode::AcquisitionFailed => DiagnosticCode::GrantAcquisitionFailed,
        };
        let reason = record
            .events
            .iter()
            .find_map(|event| match event {
                Event::Startup(event) if event.binding == failure.binding => event
                    .failure
                    .as_ref()
                    .map(|failure| failure.message.clone()),
                _ => None,
            })
            .unwrap_or_default();
        Some(
            Diagnostic::new(
                code,
                failure.span,
                format!("the leaf `{}` stopped startup: {reason}", failure.binding),
            )
            .with_source_id(&self.project_source),
        )
    }

    /// Maps `ending` to the run's outcome.
    fn outcome(
        self,
        ending: Ending,
        startup: StartupRecord,
        trace: AuditTrace,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    ) -> RunOutcome {
        let at_start = ByteSpan::empty_at(0);
        let invalid_host_value = |message: String| {
            (
                CommandResult::Trap,
                Some(Diagnostic::new(
                    DiagnosticCode::RuntimeInvalidHostValue,
                    at_start,
                    message,
                )),
                Some(TrapRecord {
                    trap: Trap::InvalidHostValue,
                    origin: None,
                }),
            )
        };
        let mut application_code = None;
        let mut program_result = None;
        let mut limits_reached = Vec::new();
        let (result, diagnostic, trap) = match ending {
            Ending::Refused(AdmissionRefusal::Unavailable(providers)) => {
                let providers = providers
                    .iter()
                    .map(|provider| provider.atom())
                    .collect::<Vec<_>>();
                (
                    CommandResult::Unavailable,
                    Some(
                        Diagnostic::new(
                            DiagnosticCode::ToolUnavailable,
                            self.target_span,
                            format!(
                                "no driver is available for {}",
                                providers.join(", ")
                            ),
                        )
                        .with_source_id(&self.project_source),
                    ),
                    None,
                )
            }
            // The static check admitted what admission refuses: a defect of
            // the check, which ends the run as a tool failure.
            Ending::Refused(refusal) => (
                CommandResult::OperationalFailure,
                Some(
                    Diagnostic::new(
                        refusal.code(),
                        self.target_span,
                        format!("admission refused the checked target: {refusal:?}"),
                    )
                    .with_source_id(&self.project_source),
                ),
                None,
            ),
            Ending::StartupFault(HostFault::MemoryExhausted) => {
                limits_reached.push("memory".to_owned());
                (
                    CommandResult::OperationalFailure,
                    Some(Diagnostic::new(
                        DiagnosticCode::RuntimeMemoryExhausted,
                        at_start,
                        "startup exhausted the memory of the broker",
                    )),
                    None,
                )
            }
            Ending::StartupFault(fault) => {
                invalid_host_value(format!("a fault ended startup: {fault:?}"))
            }
            Ending::StartupFailed => (
                CommandResult::OperationalFailure,
                self.startup_failure(&startup),
                None,
            ),
            Ending::Arguments(why) => invalid_host_value(why),
            Ending::Entered(entered) => match *entered {
                Ok(execution) => match exit_code(&execution) {
                    Some(code) => {
                        application_code = Some(code);
                        program_result = Some(execution.canonical);
                        let result = if code == 0 {
                            CommandResult::Ok
                        } else {
                            CommandResult::ProgramFailed
                        };
                        (result, None, None)
                    }
                    None => (
                        CommandResult::Trap,
                        Some(Diagnostic::new(
                            DiagnosticCode::RuntimeInvalidCheckedProgram,
                            at_start,
                            "the entry returned a value that is no `ExitCode`",
                        )),
                        Some(TrapRecord {
                            trap: Trap::InvalidCheckedProgram,
                            origin: None,
                        }),
                    ),
                },
                Err(error) => {
                    let diagnostic = Some(error.diagnostic());
                    match error {
                        RuntimeError::Trap { trap, origin, .. } => (
                            CommandResult::Trap,
                            diagnostic,
                            Some(TrapRecord { trap, origin }),
                        ),
                        RuntimeError::MemoryExhausted { .. } => {
                            limits_reached.push("memory".to_owned());
                            (CommandResult::OperationalFailure, diagnostic, None)
                        }
                        RuntimeError::Unavailable { .. } => {
                            (CommandResult::Unavailable, diagnostic, None)
                        }
                        RuntimeError::UnsupportedValue { .. } => {
                            (CommandResult::OperationalFailure, diagnostic, None)
                        }
                        // No target obtains a `TestWorld`, so no fixture of one can
                        // conflict: a boundary value of the wrong kind.
                        RuntimeError::ConflictingFixture { origin, .. } => (
                            CommandResult::Trap,
                            diagnostic,
                            Some(TrapRecord {
                                trap: Trap::InvalidHostValue,
                                origin,
                            }),
                        ),
                    }
                }
            },
        };
        let mut diagnostics = self.diagnostics;
        diagnostics.extend(diagnostic);
        RunOutcome {
            target: Some(self.target),
            result,
            application_code,
            program_result,
            startup,
            trace,
            trap,
            diagnostics,
            limits_reached,
            stdout,
            stderr,
            log: Vec::new(),
        }
    }
}

/// The `U8` of the `ExitCode` an entry returned.
fn exit_code(execution: &Execution) -> Option<u8> {
    match execution.value.value() {
        Value::Wrapper(inner) => match **inner {
            Value::U8(code) => Some(code),
            _ => None,
        },
        _ => None,
    }
}

/// One record parameter being built: the leaf name prefix of its fields,
/// the fields still to build, the fields built, and its own field name in
/// its parent.
struct Frame<'s> {
    prefix: String,
    pending: std::slice::Iter<'s, (String, GrantShape)>,
    built: Vec<(String, EntryArgument)>,
    label: String,
}

/// The entry's arguments, parameter by parameter, from what each leaf is
/// bound to: a grant, or an absent optional one. A record parameter is
/// built with an explicit stack, never by recursion.
///
/// # Errors
///
/// A leaf of a parameter that startup did not bind.
fn arguments(
    parameters: &[(String, GrantShape)],
    bound: &BTreeMap<&str, Option<GrantToken>>,
) -> Result<Vec<EntryArgument>, String> {
    let leaf = |name: &str| match bound.get(name) {
        Some(Some(token)) => Ok(EntryArgument::Grant(*token)),
        Some(None) => Ok(EntryArgument::Absent),
        None => Err(format!("startup bound no grant for the leaf `{name}`")),
    };
    let mut arguments = Vec::with_capacity(parameters.len());
    for (name, shape) in parameters {
        let name = format!("@{name}");
        let fields = match shape {
            GrantShape::Leaf(_) => {
                arguments.push(leaf(&name)?);
                continue;
            }
            GrantShape::Record(fields) => fields,
        };
        let mut stack = vec![Frame {
            prefix: name,
            pending: fields.iter(),
            built: Vec::new(),
            label: String::new(),
        }];
        while let Some(top) = stack.last_mut() {
            if let Some((field, shape)) = top.pending.next() {
                let full = format!("{}.{field}", top.prefix);
                match shape {
                    GrantShape::Leaf(_) => {
                        top.built.push((field.clone(), leaf(&full)?))
                    }
                    GrantShape::Record(inner) => stack.push(Frame {
                        prefix: full,
                        pending: inner.iter(),
                        built: Vec::new(),
                        label: field.clone(),
                    }),
                }
                continue;
            }
            let Some(done) = stack.pop() else { break };
            let value = EntryArgument::Record(done.built);
            match stack.last_mut() {
                Some(parent) => parent.built.push((done.label, value)),
                None => arguments.push(value),
            }
        }
    }
    Ok(arguments)
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "tests assert by panicking on the fixtures they build"
)]
mod tests {
    //! The rows that need a driver seam: a wrapper of the world driver
    //! counts and orders releases, and can make one host call report a
    //! refusal its gate did not make, a driver fault that traps.

    use vibra_diagnostics::DiagnosticCode;
    use vibra_host::driver::{
        Acquire, Action, Completion, DriverIdentity, Gate, Progress, Resolved,
        ResourceId,
    };
    use vibra_host::recipe::Provider;
    use vibra_host::trace::Event;
    use vibra_host::world::{HostWorld, WorldDriver};
    use vibra_interp::MemoryBudget;
    use vibra_ir::Trap;

    use super::{CommandResult, Driver, RunOutcome, prepare};
    use crate::revision::workspace_revision;
    use crate::test_tree::Tree;

    /// The world driver, recording acquisitions and releases, refusing
    /// without a gate refusal on its `fault`-th call, counted from 1, and
    /// failing its first release for the reason `fail_release`.
    struct Counting {
        inner: WorldDriver,
        acquired: Vec<ResourceId>,
        released: Vec<ResourceId>,
        calls: usize,
        fault: Option<usize>,
        fail_release: Option<&'static str>,
        /// The call, counted from 1, at which the driver reports memory
        /// exhaustion.
        exhausted: Option<usize>,
    }

    impl Driver for Counting {
        fn identity(&self, provider: Provider) -> Option<DriverIdentity> {
            self.inner.identity(provider)
        }

        fn acquire(&mut self, provider: Provider, resource: &Resolved) -> Acquire {
            let acquired = self.inner.acquire(provider, resource);
            if let Acquire::Acquired(id) = &acquired {
                self.acquired.push(*id);
            }
            acquired
        }

        fn release(&mut self, resource: ResourceId) -> Result<(), String> {
            self.released.push(resource);
            self.inner.release(resource)?;
            self.fail_release
                .take()
                .map_or(Ok(()), |why| Err(why.to_owned()))
        }

        fn call(
            &mut self,
            resource: ResourceId,
            action: &Action<'_>,
            gate: &mut dyn Gate,
        ) -> Completion {
            self.calls += 1;
            if self.fault == Some(self.calls) {
                return Completion::Refused;
            }
            if self.exhausted == Some(self.calls) {
                return Completion::MemoryExhausted(Progress::NONE);
            }
            self.inner.call(resource, action, gate)
        }
    }

    const PROJECT: &str = "{format: @project.v2 package: {name: \"hello\" version: \"0.1.0\"} \
        targets: [{name: @hello kind: @bin root: \"src/hello\" entry: @hello.main.main \
        effects: [@std.io.StderrText @std.io.StdoutText] grants: [\
        @err: {provider: @host.io.stderr resource: {kind: @processStream} after: []} \
        @out: {provider: @host.io.stdout resource: {kind: @processStream} after: []}]}] \
        dependencies: [:]}";

    /// Writes once to each stream, then `tail`.
    fn source(tail: &str) -> String {
        format!(
            "use @std.io.StderrText enduse\n\nuse @std.io.StdoutText enduse\n\n\
             use @std.process.ExitCode enduse\n\n\
             fn loop\n  takes {{n: I32}}\n  returns r: I32\n  do\n    \
             return I32.add(1i32 loop(n))\nendfn\n\n\
             proc main\n  takes {{err: StderrText out: StdoutText}}\n  \
             returns code: ExitCode\n  performs {{StderrText StdoutText}}\n  do\n    \
             let _ be await StdoutText.write(out \"hello\\n\") endlet\n    \
             let _ be await StderrText.write(err \"x\") endlet\n{tail}    \
             return ExitCode(0u8)\nendproc\n"
        )
    }

    const MAIN: &str = "src/hello/main.vib";

    fn run(
        tail: &str,
        fault: Option<usize>,
        budget: MemoryBudget,
    ) -> (RunOutcome, Counting) {
        run_failing_release(tail, fault, budget, None)
    }

    /// [`run`], with the driver's first release failing for `fail_release`.
    fn run_failing_release(
        tail: &str,
        fault: Option<usize>,
        budget: MemoryBudget,
        fail_release: Option<&'static str>,
    ) -> (RunOutcome, Counting) {
        run_with(tail, fault, budget, fail_release, None)
    }

    /// [`run`], with each of the driver's faults.
    fn run_with(
        tail: &str,
        fault: Option<usize>,
        budget: MemoryBudget,
        fail_release: Option<&'static str>,
        exhausted: Option<usize>,
    ) -> (RunOutcome, Counting) {
        let text = source(tail);
        let tree =
            Tree::new(&[("project.vibon", PROJECT), (MAIN, &text)]).expect("the tree");
        let snapshot = tree.snapshot().expect("the tree captures");
        let revision =
            workspace_revision(&snapshot.revision_records()).unwrap_or_default();
        let Ok(prepared) = prepare(&snapshot, None, &revision) else {
            panic!("the target prepares");
        };
        let world = HostWorld::default();
        let mut driver = Counting {
            inner: WorldDriver::new(&world),
            acquired: Vec::new(),
            released: Vec::new(),
            calls: 0,
            fault,
            fail_release,
            exhausted,
        };
        let outcome = prepared.run(&mut driver, budget, |driver| {
            let output = driver.inner.output();
            (output.stdout.to_vec(), output.stderr.to_vec())
        });
        (outcome, driver)
    }

    /// Every acquired leaf released once, in reverse acquisition order, and
    /// the record says so.
    fn assert_released(outcome: &RunOutcome, driver: &Counting) {
        assert_eq!(driver.acquired.len(), 2, "both leaves are acquired");
        let reverse = driver.acquired.iter().rev().copied().collect::<Vec<_>>();
        assert_eq!(driver.released, reverse, "released once each, in reverse");
        let statuses = outcome
            .startup
            .bindings
            .iter()
            .map(|binding| (binding.name.to_string(), binding.status.spelling()))
            .collect::<Vec<_>>();
        assert_eq!(
            statuses,
            [
                ("@err".to_owned(), "released"),
                ("@out".to_owned(), "released")
            ]
        );
    }

    fn host_events(outcome: &RunOutcome) -> usize {
        outcome
            .trace
            .events
            .iter()
            .filter(|event| matches!(event, Event::Host(_)))
            .count()
    }

    /// A driver that reports memory exhaustion,
    /// for an allocation the system refused, ends the run as memory
    /// exhausted: the memory limit reached, no event for the call, and every
    /// leaf released.
    #[test]
    fn a_driver_out_of_memory_ends_the_run_as_memory_exhausted() {
        let (outcome, driver) =
            run_with("", None, MemoryBudget::DEFAULT, None, Some(2));
        assert_eq!(outcome.result, CommandResult::OperationalFailure);
        assert_eq!(outcome.limits_reached, ["memory"]);
        assert_eq!(outcome.application_code, None);
        assert_eq!(host_events(&outcome), 1, "only the completed write");
        assert_eq!(
            outcome
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code())
                .collect::<Vec<_>>(),
            [DiagnosticCode::RuntimeMemoryExhausted]
        );
        assert_released(&outcome, &driver);
    }

    /// A release the driver reports as failed
    /// leaves the result and the record as they were, and one line for the
    /// tool's log.
    #[test]
    fn a_failed_release_leaves_the_result_and_one_log_line() {
        let (outcome, driver) = run_failing_release(
            "",
            None,
            MemoryBudget::DEFAULT,
            Some("the handle could not be closed"),
        );
        assert_eq!(outcome.result, CommandResult::Ok);
        assert_eq!(outcome.application_code, Some(0));
        assert_released(&outcome, &driver);
        assert_eq!(outcome.log.len(), 1, "{:?}", outcome.log);
        assert!(
            outcome
                .log
                .iter()
                .all(|line| line.starts_with("releasing @")
                    && line.ends_with(" failed: the handle could not be closed")),
            "{:?}",
            outcome.log
        );
        let (clean, _) = run("", None, MemoryBudget::DEFAULT);
        assert!(clean.log.is_empty());
        assert_eq!(
            clean.document(),
            outcome.document(),
            "the observation is the same"
        );
    }

    /// `V2-RUNTIME-run-trap`: the second host call faults after one
    /// completed write. 06 "Traps": no trap follows from user data, so the
    /// fault is the driver's.
    #[test]
    fn v2_runtime_run_trap() {
        let (outcome, _) = run("", Some(2), MemoryBudget::DEFAULT);
        assert_eq!(outcome.result, CommandResult::Trap);
        assert_eq!(outcome.application_code, None);
        assert_eq!(outcome.program_result, None);
        assert_eq!(outcome.stdout, b"hello\n");
        assert_eq!(host_events(&outcome), 1, "only the completed write");
        let Some(trap) = &outcome.trap else {
            panic!("a trap record");
        };
        assert_eq!(trap.trap, Trap::InvalidHostValue);
        assert_eq!(trap.trap.code(), DiagnosticCode::RuntimeInvalidHostValue);
        // The span record of the trapping `await`, by hand from the source.
        let text = source("");
        let needle = "await StderrText.write(err \"x\")";
        let start = text.find(needle).expect("the await");
        let origin = trap.origin.as_ref().expect("a source origin");
        assert_eq!(origin.source_id(), MAIN);
        assert_eq!(
            format!("{}..{}", origin.span().start(), origin.span().end()),
            format!("{start}..{}", start + needle.len())
        );
        assert!(
            outcome
                .document()
                .contains("trap: {trapCode: @runtime.invalidHostValue origin: {sourceId: \"src/hello/main.vib\""),
            "{}",
            outcome.document()
        );
    }

    #[test]
    fn atk_release_on_trap() {
        let (outcome, driver) = run("", Some(2), MemoryBudget::DEFAULT);
        assert_eq!(outcome.result, CommandResult::Trap);
        assert_released(&outcome, &driver);
    }

    #[test]
    fn atk_release_on_exhaustion() {
        let (outcome, driver) = run(
            "    let _ be loop(0i32) endlet\n",
            None,
            MemoryBudget::new(1 << 20),
        );
        assert_eq!(outcome.result, CommandResult::OperationalFailure);
        assert_eq!(outcome.limits_reached, ["memory"]);
        assert_released(&outcome, &driver);
    }

    #[test]
    fn a_finished_run_releases_too() {
        let (outcome, driver) = run("", None, MemoryBudget::DEFAULT);
        assert_eq!(outcome.result, CommandResult::Ok);
        assert_eq!(outcome.stderr, b"x");
        assert_released(&outcome, &driver);
    }
}
