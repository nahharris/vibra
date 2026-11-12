//! The `harness` launcher profile (decision N18), compiled only with the cargo
//! feature `harness`.
//!
//! It takes a `@host-world.v1` document and a quota, runs the program, and
//! writes one observation document. The document is internal to the
//! conformance runner and unversioned. The shipped archive does not contain
//! this module, so a shipped launcher has no way to select a host world.

use std::io::Write;

use vibra_host::{HostWorld, WorldProvider, trace_to_vibon};

use crate::abi::VibraProgram;
use crate::launcher::{Execution, Finished, SYSTEM_MEMORY_LIMIT, execute, trap_atom};

/// Exit code of a harness launch whose input was not a host world.
pub const EXIT_INVALID_INPUT: i32 = 2;

/// The observation document of an execution:
///
/// ```text
/// (record outcome: @completed|@program-failed|@trapped|@memory-exhausted|@refused
///         result: @void-or-ok|@err|@none
///         trap: @runtime.<code>|@none  origin: <ordinal>
///         audit: (record format: @audit-trace.v1 events: (array ...)))
/// ```
#[must_use]
pub fn observation(execution: &Execution) -> String {
    let (outcome, result, trap, origin) = match &execution.finished {
        Finished::Completed => ("completed", "void-or-ok", "@none".to_owned(), 0),
        Finished::ProgramFailed => ("program-failed", "err", "@none".to_owned(), 0),
        Finished::Trapped(trap) => (
            "trapped",
            "none",
            trap_atom(trap.code).to_owned(),
            trap.origin,
        ),
        Finished::MemoryExhausted => {
            ("memory-exhausted", "none", "@none".to_owned(), 0)
        }
        Finished::Refused(_) => ("refused", "none", "@none".to_owned(), 0),
    };
    format!(
        "(record outcome: @{outcome} result: @{result} trap: {trap} origin: {origin} audit: {})",
        trace_to_vibon(&execution.audit)
    )
}

/// Runs `program` in the host world `world_text` under `quota`, writes the
/// observation document to `out`, and returns the process exit code: `0` when
/// the document was written, whatever the program did.
///
/// # Safety
///
/// As [`execute`].
pub unsafe fn launch_harness(
    program: &VibraProgram,
    world_text: &str,
    quota: u64,
    out: &mut dyn Write,
    diagnostics: &mut dyn Write,
) -> i32 {
    let world = match HostWorld::decode("host-world.vibon", world_text) {
        Ok(world) => world,
        Err(errors) => {
            let _ =
                writeln!(diagnostics, "invalid host world: {} error(s)", errors.len());
            return EXIT_INVALID_INPUT;
        }
    };
    let mut host = WorldProvider::new(&world);
    // SAFETY: forwarded; the caller's contract is that of `execute`.
    let execution = unsafe { execute(program, &mut host, SYSTEM_MEMORY_LIMIT, quota) };
    let _ = writeln!(out, "{}", observation(&execution));
    0
}
