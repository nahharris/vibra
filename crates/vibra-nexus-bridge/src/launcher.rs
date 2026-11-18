//! The launcher: admits a program, runs it on a host provider, and maps the
//! outcome to the exit code of the tooling chapter.
//!
//! `docs/spec/05-tooling.md`, "The process exit mapping" (the paragraph that
//! starts "The process exit mapping is fixed"), fixes `0` for `@command.ok`,
//! `3` for `@command.operational-failure`, `5` for `@command.trap`, and `6`
//! for `@command.program-failed`. "An entry that returns `err`" (the paragraph
//! before it) is `program-failed`, "neither a trap nor a diagnostic", and
//! memory exhaustion "stays the host event, `@command.operational-failure`".

use std::ffi::OsString;
use std::io::Write;

use vibra_host::{AuditEvent, HostProvider, SystemProvider};
use vibra_ir::boundary::TrapCode;
use vibra_nexus::{Context, Outcome, Trap, UNBOUNDED, run};

use crate::abi::VibraProgram;
use crate::admission::{Admitted, Refusal, admit};
use crate::generated::Generated;

/// Exit code of a program that completed (`@command.ok`).
pub const EXIT_COMPLETED: i32 = 0;
/// Exit code of a host event (`@command.operational-failure`).
pub const EXIT_HOST_EVENT: i32 = 3;
/// Exit code of a trap (`@command.trap`).
pub const EXIT_TRAP: i32 = 5;
/// Exit code of an entry that returned `err` (`@command.program-failed`).
pub const EXIT_PROGRAM_FAILED: i32 = 6;
/// Exit code of a program the runtime refuses at admission.
///
/// PROVISIONAL: pending the maintainer's ruling on the admission-refusal exit code
/// The draft runtime chapter names the diagnostic `@runtime.incompatible-artifact`
/// and no exit code; this is the code the tooling chapter assigns to an
/// operational failure.
pub const EXIT_ADMISSION_REFUSED: i32 = EXIT_HOST_EVENT;

/// The memory a native program's context may hold: the budget `vibra run`
/// gives the interpreter (`MEMORY_BUDGET` of `vibra-cli`). The limit belongs to
/// the embedding host.
pub const SYSTEM_MEMORY_LIMIT: u64 = 256 * 1024 * 1024;

/// How a launch ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finished {
    /// The entry returned `void` or `ok`.
    Completed,
    /// The entry returned `err`.
    ProgramFailed,
    /// A trap stopped the program.
    Trapped(Trap),
    /// The host event `@runtime.memory-exhausted`.
    MemoryExhausted,
    /// Admission refused the program; nothing ran.
    Refused(Refusal),
}

impl Finished {
    /// The process exit code of this outcome.
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        match self {
            Self::Completed => EXIT_COMPLETED,
            Self::ProgramFailed => EXIT_PROGRAM_FAILED,
            Self::Trapped(_) => EXIT_TRAP,
            Self::MemoryExhausted => EXIT_HOST_EVENT,
            Self::Refused(_) => EXIT_ADMISSION_REFUSED,
        }
    }
}

/// A launch: its outcome and what the context observed.
#[derive(Clone, Debug)]
pub struct Execution {
    /// How it ended.
    pub finished: Finished,
    /// The audit trace, empty when the program was refused.
    pub audit: Vec<AuditEvent>,
    /// The bytes of values and frames live at the end.
    pub live_size: u64,
    /// The source span of a trap's origin, as `source:start..end`.
    pub trap_origin: Option<String>,
    /// Whether an entry result existed to observe: a program that completed.
    pub result_observed: bool,
}

/// Admits `program` and runs it on `host` under `limit` bytes, continuing after
/// every yield of `quota`.
///
/// # Safety
///
/// As [`admit`]: the descriptor's tables are valid, unchanging memory. The
/// step functions it names must honor the contract of the routines.
pub unsafe fn execute(
    program: &VibraProgram,
    host: &mut dyn HostProvider,
    limit: u64,
    quota: u64,
) -> Execution {
    // SAFETY: forwarded; the caller's contract is that of `admit`.
    match unsafe { admit(program) } {
        Ok(admitted) => run_admitted(admitted, host, limit, quota, |_, _| ()).0,
        Err(refusal) => refused(refusal),
    }
}

/// The execution of a program that admission refused: nothing ran.
pub(crate) fn refused(refusal: Refusal) -> Execution {
    Execution {
        finished: Finished::Refused(refusal),
        audit: Vec::new(),
        live_size: 0,
        trap_origin: None,
        result_observed: false,
    }
}

/// Runs an admitted program as [`execute`] does, and lets `observe` read the
/// context once the run has ended and before the context is dropped. The
/// observer must leave the context's balance as it found it: the execution's
/// live size is taken after it.
pub(crate) fn run_admitted<T>(
    admitted: Admitted,
    host: &mut dyn HostProvider,
    limit: u64,
    quota: u64,
    observe: impl FnOnce(&mut Context<'_>, &Finished) -> T,
) -> (Execution, T) {
    let generated = Generated::new(admitted);
    let mut ctx = Context::new(limit, generated.globals(), host);
    let finished = if generated.start(&mut ctx).is_err() {
        Finished::MemoryExhausted
    } else {
        loop {
            match run(&mut ctx, &generated, quota) {
                Outcome::Yielded => {}
                Outcome::Completed => {
                    let failed = generated
                        .admitted()
                        .entry_err_variant()
                        .is_some_and(|variant| ctx.result_is_variant(variant));
                    break if failed {
                        Finished::ProgramFailed
                    } else {
                        Finished::Completed
                    };
                }
                Outcome::Trapped(trap) => break Finished::Trapped(trap),
                Outcome::MemoryExhausted => break Finished::MemoryExhausted,
                // `Outcome` is `non_exhaustive`: an outcome this launcher does
                // not know is a host-side failure.
                _ => break Finished::MemoryExhausted,
            }
        }
    };
    let trap_origin = match &finished {
        Finished::Trapped(trap) => describe_origin(generated.admitted(), trap.origin),
        _ => None,
    };
    let observed = observe(&mut ctx, &finished);
    let execution = Execution {
        result_observed: matches!(
            finished,
            Finished::Completed | Finished::ProgramFailed
        ),
        audit: ctx.audit().to_vec(),
        live_size: ctx.live_size(),
        trap_origin,
        finished,
    };
    (execution, observed)
}

fn describe_origin(admitted: &Admitted, ordinal: u32) -> Option<String> {
    admitted
        .origin(ordinal)
        .map(|origin| format!("{}:{}..{}", origin.source, origin.start, origin.end))
}

/// The registered atom of a trap code.
#[must_use]
pub const fn trap_atom(code: TrapCode) -> &'static str {
    match code {
        TrapCode::InvalidCheckedProgram => "@runtime.invalid-checked-program",
        TrapCode::UnobservableFunction => "@runtime.unobservable-function",
        TrapCode::InvalidHostValue => "@runtime.invalid-host-value",
        TrapCode::DivisionByZero => "@runtime.division-by-zero",
    }
}

/// Writes what human `run` writes to standard error for this outcome, as far
/// as the runtime can observe it: the diagnostic code line.
///
/// For `program-failed` human `run` writes the canonical encoding of the
/// entry's `err` value. Observing that value in a native program needs the
/// observation walker of package X3-18, which does not exist yet, so only the
/// code line is written.
///
/// # Errors
///
/// The error of `stderr`.
pub fn report(execution: &Execution, stderr: &mut dyn Write) -> std::io::Result<()> {
    match &execution.finished {
        Finished::Completed => Ok(()),
        Finished::ProgramFailed => writeln!(stderr, "@command.program-failed"),
        Finished::Trapped(trap) => match &execution.trap_origin {
            Some(origin) => {
                writeln!(stderr, "{} at {origin}", trap_atom(trap.code))
            }
            None => writeln!(stderr, "{}", trap_atom(trap.code)),
        },
        Finished::MemoryExhausted => writeln!(stderr, "@runtime.memory-exhausted"),
        Finished::Refused(refusal) => {
            writeln!(stderr, "@runtime.incompatible-artifact: {refusal}")
        }
    }
}

/// The `system` profile: the system provider over the process `arguments`, the
/// real standard streams, no quota. Returns the process exit code.
///
/// # Safety
///
/// As [`execute`].
pub unsafe fn launch_system(
    program: &VibraProgram,
    arguments: Vec<OsString>,
    stderr: &mut dyn Write,
) -> i32 {
    let mut host = SystemProvider::with_arguments(arguments);
    // SAFETY: forwarded; the caller's contract is that of `execute`.
    unsafe { launch(program, &mut host, SYSTEM_MEMORY_LIMIT, stderr) }
}

/// Runs `program` unbounded on `host`, reports to `stderr`, and returns the
/// exit code. [`launch_system`] is this over the system provider.
///
/// # Safety
///
/// As [`execute`].
pub unsafe fn launch(
    program: &VibraProgram,
    host: &mut dyn HostProvider,
    limit: u64,
    stderr: &mut dyn Write,
) -> i32 {
    // SAFETY: forwarded; the caller's contract is that of `execute`.
    let execution = unsafe { execute(program, host, limit, UNBOUNDED) };
    // A closed standard error cannot change how the program ended.
    let _ = report(&execution, stderr);
    execution.finished.exit_code()
}
