//! Admission: a descriptor that fails is refused before any code runs, any host
//! operation is performed, or any audit event is recorded.

use vibra_host::{SystemProvider, WorldProvider};
use vibra_nexus::{AdmitError, UNBOUNDED};
use vibra_nexus_bridge::{
    CONSTANT_STR, EXIT_ADMISSION_REFUSED, Finished, Malformed, Refusal, VibraConstant,
    VibraOrigin, VibraTable, execute, launch,
};

use crate::fixture::{self, Fixture, LIMIT, steps};

/// A program that, if admitted, would perform a host operation: the demo.
fn demo() -> Fixture {
    fixture::demo(3, false)
}

/// Launches the fixture and checks everything every refusal shares.
fn refused(fixture: &Fixture) -> Refusal {
    let descriptor = fixture.descriptor();
    let mut host = WorldProvider::new(&vibra_host::HostWorld::default());
    // SAFETY: the fixture owns every table the descriptor addresses.
    let execution = unsafe { execute(&descriptor, &mut host, LIMIT, UNBOUNDED) };
    assert_eq!(steps(), 0, "no step function was called");
    assert!(
        execution.audit.is_empty(),
        "no host operation was performed"
    );
    assert_eq!(execution.live_size, 0);
    let Finished::Refused(refusal) = execution.finished else {
        panic!("expected a refusal, got {:?}", execution.finished);
    };

    // The launcher reports it and exits with the provisional code, having
    // written nothing to standard output.
    let mut system = SystemProvider::new().capturing();
    let mut stderr = Vec::new();
    // SAFETY: as above.
    let code = unsafe { launch(&descriptor, &mut system, LIMIT, &mut stderr) };
    assert_eq!(code, EXIT_ADMISSION_REFUSED);
    let stderr = String::from_utf8(stderr).unwrap();
    assert!(
        stderr.starts_with("@runtime.incompatible-artifact: "),
        "{stderr}"
    );
    assert!(system.captured_stdout().is_empty());
    assert_eq!(steps(), 0);
    refusal
}

#[test]
fn a_program_built_for_another_abi_is_refused_by_the_launcher() {
    let mut program = demo();
    program.abi += 1;
    // The layout of another version may differ: nothing else is read.
    program.functions = vec![];
    assert_eq!(
        refused(&program),
        Refusal::Requirement(AdmitError::Abi {
            runtime: vibra_nexus::ABI_VERSION,
            program: vibra_nexus::ABI_VERSION + 1,
        })
    );
}

#[test]
fn a_program_naming_an_unknown_symbol_is_refused_by_the_launcher() {
    let program = demo().symbols(&["io.stdout-text", "net.connect"]);
    assert_eq!(
        refused(&program),
        Refusal::Requirement(AdmitError::UnknownSymbol("net.connect".to_owned()))
    );
}

#[test]
fn a_registry_minor_above_the_runtimes_is_refused_by_the_launcher() {
    let mut program = demo();
    program.registry_minor = 99;
    assert!(matches!(
        refused(&program),
        Refusal::Requirement(AdmitError::RegistryMinor { program: 99, .. })
    ));
}

fn malformed(fixture: &Fixture) -> Malformed {
    match refused(fixture) {
        Refusal::Malformed(malformed) => malformed,
        other => panic!("expected a malformed descriptor, got {other:?}"),
    }
}

#[test]
fn an_entry_out_of_range_is_refused() {
    let mut program = demo();
    program.entry = 3;
    assert_eq!(malformed(&program), Malformed::EntryOutOfRange);
}

#[test]
fn an_entry_taking_operands_is_refused() {
    let mut program = demo();
    program.entry = 1;
    assert_eq!(malformed(&program), Malformed::EntryTakesOperands);
}

#[test]
fn an_initializer_out_of_range_or_taking_operands_is_refused() {
    let mut program = fixture::module_value();
    program.globals = vec![9];
    assert_eq!(malformed(&program), Malformed::InitializerOutOfRange(0));
    let mut program = demo();
    program.globals = vec![1];
    assert_eq!(malformed(&program), Malformed::InitializerTakesOperands(0));
}

#[test]
fn an_oversized_frame_is_refused() {
    let mut program = demo();
    program.functions[1].slots = vibra_nexus::MAX_FRAME_SLOTS + 1;
    assert_eq!(malformed(&program), Malformed::FrameTooLarge(1));
    let mut program = demo();
    program.functions[1].slots = 1;
    assert_eq!(malformed(&program), Malformed::FrameTooSmall(1));
}

#[test]
fn a_null_or_empty_function_table_is_refused() {
    let program = demo();
    let mut descriptor = program.descriptor();
    descriptor.functions = VibraTable::EMPTY;
    assert_eq!(
        refused_descriptor(&descriptor),
        Malformed::NoFunctions.into()
    );
    descriptor.functions = VibraTable {
        ptr: std::ptr::null(),
        len: 3,
    };
    assert_eq!(
        refused_descriptor(&descriptor),
        Malformed::NullTable("function").into()
    );
    descriptor.functions.len = u64::MAX;
    assert_eq!(
        refused_descriptor(&descriptor),
        Malformed::TableTooLong("function").into()
    );
}

/// As [`refused`], for a descriptor edited after it was built.
fn refused_descriptor(descriptor: &vibra_nexus_bridge::VibraProgram) -> Refusal {
    let mut host = WorldProvider::new(&vibra_host::HostWorld::default());
    // SAFETY: the descriptors this test passes address only tables whose
    // length is zero or whose address is null, which admission refuses
    // before it reads them; the others are the demo fixture's.
    let execution = unsafe { execute(descriptor, &mut host, LIMIT, UNBOUNDED) };
    assert_eq!(steps(), 0);
    assert!(execution.audit.is_empty());
    let Finished::Refused(refusal) = execution.finished else {
        panic!("expected a refusal");
    };
    refusal
}

#[test]
fn a_function_without_a_step_or_with_a_bad_environment_flag_is_refused() {
    let mut program = demo();
    program.functions[2].step = None;
    assert_eq!(malformed(&program), Malformed::NullStep(2));
    let mut program = demo();
    program.functions[2].env = 2;
    assert_eq!(malformed(&program), Malformed::BadEnvFlag(2));
    let mut program = demo();
    program.functions[0].reserved = 1;
    assert_eq!(malformed(&program), Malformed::Reserved);
}

#[test]
fn constants_symbols_and_origins_are_validated() {
    let mut program = demo();
    program.constants = vec![VibraConstant {
        kind: 9,
        reserved: 0,
        data: VibraTable::EMPTY,
    }];
    assert_eq!(malformed(&program), Malformed::BadConstantKind(0));

    static NOT_UTF8: [u8; 2] = [0xFF, 0xFE];
    let mut program = demo();
    program.constants = vec![VibraConstant {
        kind: CONSTANT_STR,
        reserved: 0,
        data: VibraTable::new(&NOT_UTF8),
    }];
    assert_eq!(malformed(&program), Malformed::NotUtf8("a str constant"));

    let mut program = demo();
    program.origins = vec![VibraOrigin {
        end: 1,
        start: 2,
        ..fixture::origin("app/main.vib", 0, 0)
    }];
    assert_eq!(malformed(&program), Malformed::BadOrigin(1));

    let mut program = demo();
    program.symbols = vec![VibraTable::new(&NOT_UTF8)];
    assert_eq!(malformed(&program), Malformed::NotUtf8("a host symbol"));
}
