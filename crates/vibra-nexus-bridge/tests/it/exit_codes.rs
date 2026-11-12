//! The `system` launcher maps outcomes to the exit codes of the tooling chapter
//! (`docs/spec/05-tooling.md`, "The process exit mapping"). Each test launches
//! in process on a system provider that captures the program's streams, with
//! standard error captured through an injected writer.

use vibra_host::SystemProvider;
use vibra_ir::boundary::TrapCode;
use vibra_nexus::Trap;
use vibra_nexus_bridge::{
    EXIT_ADMISSION_REFUSED, EXIT_COMPLETED, EXIT_HOST_EVENT, EXIT_PROGRAM_FAILED,
    EXIT_TRAP, Finished, SYSTEM_MEMORY_LIMIT, launch, launch_system,
};

use crate::fixture::{self, Fixture};

fn launched(fixture: &Fixture, limit: u64) -> (i32, String, String) {
    let descriptor = fixture.descriptor();
    let mut host = SystemProvider::with_arguments(Vec::new()).capturing();
    let mut stderr = Vec::new();
    // SAFETY: the fixture owns every table the descriptor addresses.
    let code = unsafe { launch(&descriptor, &mut host, limit, &mut stderr) };
    (
        code,
        String::from_utf8(stderr).unwrap(),
        String::from_utf8(host.captured_stdout().to_vec()).unwrap(),
    )
}

#[test]
fn a_program_that_completes_exits_0() {
    let (code, stderr, _) = launched(&fixture::entry_ok(), SYSTEM_MEMORY_LIMIT);
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert_eq!(EXIT_COMPLETED, 0);
}

#[test]
fn the_system_profile_completes_over_process_arguments() {
    let fixture = fixture::entry_ok();
    let descriptor = fixture.descriptor();
    let mut stderr = Vec::new();
    // SAFETY: the fixture owns every table the descriptor addresses.
    let code = unsafe {
        launch_system(&descriptor, vec!["one".into(), "two".into()], &mut stderr)
    };
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
}

#[test]
fn an_entry_that_returns_err_exits_6_and_writes_the_code_line() {
    let (code, stderr, stdout) = launched(&fixture::entry_err(), SYSTEM_MEMORY_LIMIT);
    assert_eq!(code, 6);
    assert_eq!(EXIT_PROGRAM_FAILED, 6);
    // Only the diagnostic code line: the canonical encoding of the `err`
    // value needs the observation walker of package X3-18.
    assert_eq!(stderr, "@command.program-failed\n");
    assert_eq!(stdout, "");
}

#[test]
fn a_trap_exits_5_and_names_its_code_and_origin() {
    let (code, stderr, _) = launched(&fixture::entry_trap(), SYSTEM_MEMORY_LIMIT);
    assert_eq!(code, 5);
    assert_eq!(EXIT_TRAP, 5);
    assert_eq!(stderr, "@runtime.division-by-zero at app/main.vib:3..9\n");
}

#[test]
fn memory_exhaustion_is_the_host_event_and_exits_3() {
    let (code, stderr, _) = launched(&fixture::deep(100_000), 64 * 1024);
    assert_eq!(code, 3);
    assert_eq!(EXIT_HOST_EVENT, 3);
    assert_eq!(stderr, "@runtime.memory-exhausted\n");
}

#[test]
fn the_provisional_refusal_code_is_the_operational_failure_code() {
    assert_eq!(EXIT_ADMISSION_REFUSED, EXIT_HOST_EVENT);
}

#[test]
fn every_outcome_maps_to_its_code() {
    assert_eq!(Finished::Completed.exit_code(), 0);
    assert_eq!(Finished::MemoryExhausted.exit_code(), 3);
    assert_eq!(
        Finished::Trapped(Trap {
            code: TrapCode::InvalidCheckedProgram,
            origin: 0
        })
        .exit_code(),
        5
    );
    assert_eq!(Finished::ProgramFailed.exit_code(), 6);
}
