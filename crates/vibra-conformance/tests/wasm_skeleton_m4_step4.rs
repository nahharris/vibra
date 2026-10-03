//! The WebAssembly skeleton end to end (milestone 4 Step 4): the checked empty
//! entry is emitted, validated under the v1 baseline, and run, and emission is
//! deterministic from the first module (ledger D4.4).

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::fmt::Write as _;
use std::process::Command;

use vibra_ir::CheckedProgram;
use vibra_ir::boundary::{
    ENTRY_EXPORT, LIVE_SIZE_EXPORT, MEMORY_EXPORT, ORIGIN_EXPORT, RESULT_EXPORT,
    STATUS_EXPORT, TRAP_CODE_EXPORT,
};
use vibra_wasm_run::{MemoryLimit, Outcome, Runner, validate};

/// The empty entry, as a source document.
const EMPTY_ENTRY: &str = "(defn done () void)\n";
/// What `(do)` checks to: a sequence holding the `void` literal.
const EMPTY_DO_ENTRY: &str = "(defn main () void (do))\n";

fn checked(source: &str) -> CheckedProgram {
    let result = vibra_types::check_source("input.vib", source);
    assert!(result.accepted(), "{:?}", result.diagnostics());
    result.program().expect("an accepted program").clone()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}

fn emitted(source: &str) -> Vec<u8> {
    vibra_wasm::emit(&checked(source))
        .expect("the empty entry lowers")
        .into_bytes()
}

#[test]
fn the_checked_empty_entry_is_emitted_validated_and_run() {
    for source in [EMPTY_ENTRY, EMPTY_DO_ENTRY] {
        let bytes = emitted(source);
        validate(&bytes).expect("the module validates under the baseline");
        let runner = Runner::new(MemoryLimit::new(64 * 1024 * 1024)).expect("engine");
        assert_eq!(
            runner.run_entry(&bytes).expect("runs"),
            Outcome::Completed {
                result: 0,
                live_size: 0
            },
            "{source}"
        );
    }
}

#[test]
fn the_module_declares_exactly_the_boundary_a_void_entry_needs() {
    let summary = validate(&emitted(EMPTY_ENTRY)).expect("valid");
    // The empty entry uses no native, so the module imports nothing.
    assert!(summary.imports.is_empty(), "{:?}", summary.imports);
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
        ]
    );
    assert_eq!(summary.initial_pages, 1);
}

#[test]
fn the_module_has_no_custom_section() {
    // Section ids follow the 8-byte header; 0 is a custom section. Walk the
    // sections by their LEB128 sizes.
    let bytes = emitted(EMPTY_ENTRY);
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
fn emission_is_byte_identical_across_checks_in_one_process() {
    let first = emitted(EMPTY_ENTRY);
    let second = emitted(EMPTY_ENTRY);
    assert_eq!(first, second);
}

/// Prints the module of the empty entry, for the parent test to compare.
#[test]
#[ignore = "run only as the child of `emission_is_byte_identical_across_processes`"]
fn emit_in_child_process() {
    // The harness prints its own text on this line, so start a fresh one.
    println!(
        "
EMITTED:{}",
        hex(&emitted(EMPTY_ENTRY))
    );
}

#[test]
fn emission_is_byte_identical_across_processes() {
    let own = hex(&emitted(EMPTY_ENTRY));
    let mut seen = Vec::new();
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
        seen.push(line.to_owned());
    }
    assert_eq!(seen[0], own, "a second process emitted other bytes");
    assert_eq!(seen[1], own);
}

#[test]
fn a_program_the_emitter_cannot_lower_has_no_module() {
    let program = checked("(defn answer () u64 7u64)\n");
    let error =
        vibra_wasm::emit(&program).expect_err("a literal result is not lowered yet");
    let names = error
        .forms()
        .iter()
        .map(|used| used.form().name())
        .collect::<Vec<_>>();
    assert_eq!(names, ["non-void-result", "literal"]);
}
