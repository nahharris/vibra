//! The emitter lowers every form of a checked program (milestone 4 Step 11).
//!
//! `EmitError` has three kinds, and none stands for a construct a checked
//! program can contain: a program past the 32-bit indices of a module, an entry
//! that takes a parameter, and a checked-IR shape the checker rules out. These
//! tests hold the lowering to that:
//!
//! - every registry row has a lowering, inline or as a native import, or is a
//!   native that runs its checked body;
//! - every program the corpus checks emits, or is refused only for an entry
//!   with parameters, which `check_source` admits and no executable case has;
//! - emission of each of those programs is byte-identical.
//!
//! That the corpus runs without a failing case in the Wasm backend is held by
//! the conformance run itself, which fails a case whose module is not emitted.

use crate::support::corpus_root;
use std::path::Path;

use vibra_ir::external::CompilerIntrinsic;
use vibra_wasm::{EmitError, lowers_primitive};

#[test]
fn every_registry_row_has_a_lowering() {
    let mut unlowered = Vec::new();
    for intrinsic in CompilerIntrinsic::all() {
        if !lowers_primitive(intrinsic) && intrinsic.native_import().is_none() {
            unlowered.push(intrinsic.symbol());
        }
    }
    // `array.fold` is the one native with no import and no inline row: the
    // module runs the checked body of the standard library's function.
    assert_eq!(
        unlowered,
        ["array.fold"],
        "rows with no lowering: {unlowered:?}"
    );
}

fn sources(directory: &Path, found: &mut Vec<String>) {
    for entry in std::fs::read_dir(directory).expect("a directory") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "vib")
            && path.file_name().is_some_and(|name| name == "input.vib")
        {
            found.push(std::fs::read_to_string(&path).expect("a source"));
        }
    }
}

#[test]
fn every_program_the_corpus_checks_lowers() {
    let root = corpus_root();
    let mut found = Vec::new();
    sources(&root, &mut found);
    assert!(found.len() > 100, "{} sources", found.len());
    let mut lowered = 0_usize;
    for source in &found {
        let result = vibra_types::check_source("input.vib", source);
        let Some(program) = result.program().filter(|_| result.accepted()) else {
            continue;
        };
        match vibra_wasm::emit(program) {
            Ok(module) => {
                let again = vibra_wasm::emit(program).expect("a second emission");
                assert_eq!(
                    module.bytes(),
                    again.bytes(),
                    "emission differs for:\n{source}"
                );
                lowered += 1;
            }
            Err(error) => assert!(
                matches!(error, EmitError::EntryParameters { .. }),
                "a checked program is refused for more than its entry:\n{source}\n{error}"
            ),
        }
    }
    assert!(lowered > 100, "{lowered} programs lowered");
}
