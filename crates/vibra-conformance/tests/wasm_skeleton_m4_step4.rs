//! The WebAssembly skeleton end to end (milestone 4 Step 4): the empty entry is
//! emitted, validated under the v1 baseline, and run, and emission is
//! deterministic from the first module (ledger D4.4).
//!
//! The empty entry is built from IR constructors, not checked from source: every
//! checked program now carries the prelude's `true` and `false` module values
//! (Step 2b), which need the arena of Step 5a, so no source program lowers yet.
//! A test below pins that fact, so the first step that lowers module values
//! updates it deliberately.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::fmt::Write as _;
use std::process::Command;

use vibra_diagnostics::ByteSpan;
use vibra_ir::boundary::{
    ENTRY_EXPORT, LENGTH_EXPORT, LIVE_SIZE_EXPORT, MEMORY_EXPORT, ORIGIN_EXPORT,
    READ_F32_EXPORT, READ_F64_EXPORT, READ_I32_EXPORT, READ_I64_EXPORT, READ_ID_EXPORT,
    RELEASE_EXPORT, RESULT_EXPORT, STATUS_EXPORT, TRAP_CODE_EXPORT, VARIANT_EXPORT,
};
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, SourceOrigin, Type, Value,
};
use vibra_wasm_run::{MemoryLimit, Outcome, ResultSlot, Runner, validate};

fn origin() -> SourceOrigin {
    SourceOrigin::new("input.vib", ByteSpan::new(0, 1))
}

/// `(defn done () void)` as the checker would build it, without the prelude.
fn empty_entry() -> CheckedProgram {
    let done = CheckedFunction::new(
        "done",
        FunctionSignature::new(Vec::new(), Type::Void),
        Expr::literal(Value::Void, origin()),
        origin(),
    )
    .expect("a checked function");
    CheckedProgram::try_new(vec![done], 0).expect("a checked program")
}

/// `(defn main () void (do))`: a sequence holding the `void` literal.
fn empty_do_entry() -> CheckedProgram {
    let body = Expr::Sequence {
        expressions: vec![Expr::literal(Value::Void, origin())],
        origin: origin(),
    };
    let main = CheckedFunction::new(
        "main",
        FunctionSignature::new(Vec::new(), Type::Void),
        body,
        origin(),
    )
    .expect("a checked function");
    CheckedProgram::try_new(vec![main], 0).expect("a checked program")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}

fn emitted(program: &CheckedProgram) -> Vec<u8> {
    vibra_wasm::emit(program)
        .expect("the empty entry lowers")
        .into_bytes()
}

#[test]
fn the_empty_entry_is_emitted_validated_and_run() {
    for program in [empty_entry(), empty_do_entry()] {
        let bytes = emitted(&program);
        validate(&bytes).expect("the module validates under the baseline");
        let runner = Runner::new(MemoryLimit::new(64 * 1024 * 1024)).expect("engine");
        assert_eq!(
            runner.run_entry(&bytes).expect("runs"),
            Outcome::Completed {
                result: ResultSlot::from_bits(0),
                live_size: 0
            }
        );
    }
}

#[test]
fn the_module_declares_the_boundary_a_void_entry_needs() {
    let summary = validate(&emitted(&empty_entry())).expect("valid");
    // The empty entry uses no native, so the module imports nothing.
    assert!(summary.imports.is_empty(), "{:?}", summary.imports);
    // Every accessor of the boundary table that no test needs, in the order of
    // the specification's table (Step 5a). Step 11 adds the test and failure
    // exports.
    assert_eq!(
        summary.exports,
        [
            MEMORY_EXPORT,
            ENTRY_EXPORT,
            STATUS_EXPORT,
            TRAP_CODE_EXPORT,
            ORIGIN_EXPORT,
            RESULT_EXPORT,
            LIVE_SIZE_EXPORT,
            RELEASE_EXPORT,
            VARIANT_EXPORT,
            LENGTH_EXPORT,
            READ_I32_EXPORT,
            READ_I64_EXPORT,
            READ_F32_EXPORT,
            READ_F64_EXPORT,
            READ_ID_EXPORT,
        ]
    );
    assert_eq!(summary.initial_pages, 1);
}

#[test]
fn the_module_has_no_custom_section() {
    // Section ids follow the 8-byte header; 0 is a custom section. Walk the
    // sections by their LEB128 sizes.
    let bytes = emitted(&empty_entry());
    let mut offset = 8;
    let mut ids = Vec::new();
    while offset < bytes.len() {
        ids.push(bytes[offset]);
        offset += 1;
        let mut size = 0_usize;
        let mut shift = 0;
        loop {
            let byte = bytes[offset];
            offset += 1;
            size |= usize::from(byte & 0x7f) << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                break;
            }
        }
        offset += size;
    }
    assert_eq!(offset, bytes.len(), "the sections tile the module");
    assert!(!ids.contains(&0), "a custom section is present: {ids:?}");
}

#[test]
fn emission_is_byte_identical_across_builds_in_one_process() {
    assert_eq!(emitted(&empty_entry()), emitted(&empty_entry()));
}

/// A program whose entry builds arena values: data segments, the constructor,
/// and a handle-table registration, so what the arena adds is emitted in order.
fn literal_entry() -> CheckedProgram {
    let body = Expr::Sequence {
        expressions: vec![
            Expr::literal(Value::Str("h\u{e9}llo".to_owned()), origin()),
            Expr::literal(Value::Atom("ok".to_owned()), origin()),
            Expr::literal(Value::Bytes(vec![1, 2, 3]), origin()),
            Expr::literal(Value::Bool(true), origin()),
        ],
        origin: origin(),
    };
    let main = CheckedFunction::new(
        "main",
        FunctionSignature::new(Vec::new(), Type::Bool),
        body,
        origin(),
    )
    .expect("a checked function");
    CheckedProgram::try_new(vec![main], 0).expect("a checked program")
}

/// Prints the modules of the empty entry and of an arena program, for the
/// parent test to compare.
#[test]
#[ignore = "run only as the child of `emission_is_byte_identical_across_processes`"]
fn emit_in_child_process() {
    // The harness prints its own text on this line, so start a fresh one.
    println!(
        "\nEMITTED:{}{}",
        hex(&emitted(&empty_entry())),
        hex(&emitted(&literal_entry()))
    );
}

#[test]
fn emission_is_byte_identical_across_processes() {
    let own = format!(
        "{}{}",
        hex(&emitted(&empty_entry())),
        hex(&emitted(&literal_entry()))
    );
    for _ in 0..2 {
        let output = Command::new(std::env::current_exe().expect("this test binary"))
            .args([
                "--ignored",
                "--exact",
                "emit_in_child_process",
                "--nocapture",
                "--test-threads=1",
            ])
            .output()
            .expect("the child process runs");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("utf-8");
        let line = stdout
            .lines()
            .find_map(|line| line.strip_prefix("EMITTED:"))
            .unwrap_or_else(|| panic!("the child printed no module:\n{stdout}"));
        assert_eq!(line, own, "another process emitted other bytes");
    }
}

#[test]
fn a_checked_program_carries_the_prelude_values_and_does_not_lower_yet() {
    // `true` and `false` are module values of every checked program (Step 2b),
    // and a module value needs the arena of Step 5a.
    let result = vibra_types::check_source("input.vib", "(defn done () void)\n");
    let program = result.program().expect("an accepted program");
    let error =
        vibra_wasm::emit(program).expect_err("module values are not lowered yet");
    let names = error
        .forms()
        .iter()
        .map(|used| used.form().name())
        .collect::<Vec<_>>();
    assert!(names.contains(&"module-value"), "{names:?}");
}

#[test]
fn a_program_the_emitter_cannot_lower_has_no_module() {
    let result = vibra_types::check_source("input.vib", "(defn answer () u64 7u64)\n");
    let error = vibra_wasm::emit(result.program().expect("accepted"))
        .expect_err("the prelude's module values are not lowered yet");
    let names = error
        .forms()
        .iter()
        .map(|used| used.form().name())
        .collect::<Vec<_>>();
    assert!(names.contains(&"module-value"), "{names:?}");
}
