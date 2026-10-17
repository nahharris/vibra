//! After Step 11 the emitter names no form of a checked program as unlowered
//! (milestone 4 Step 11).
//!
//! `Form` has three kinds, and none stands for a construct a checked program
//! can contain: a program past the 32-bit indices of a module, an entry that
//! takes a parameter, and a checked-IR shape the checker rules out. These
//! tests hold the classifier and the lowering to that:
//!
//! - every registry row has a lowering, inline or as a native import, or is a
//!   native that runs its checked body;
//! - every program the corpus checks lowers, or is refused only for an entry
//!   with parameters, which `check_source` admits and no executable case has;
//! - the corpus reports no case as not lowered.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use std::path::Path;

use vibra_ir::external::CompilerIntrinsic;
use vibra_wasm::{Form, lowers_primitive};

#[test]
fn the_form_kinds_are_the_three_that_no_valid_program_contains() {
    let names = [Form::ModuleSize, Form::Parameters, Form::Malformed].map(Form::name);
    assert_eq!(names, ["module-size", "parameters", "malformed"]);
    // A new kind must be added here on purpose: a `match` without a wildcard.
    let witness = |form: Form| match form {
        Form::ModuleSize | Form::Parameters | Form::Malformed => (),
    };
    witness(Form::Malformed);
}

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
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/cases");
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
            Ok(_) => lowered += 1,
            Err(error) => {
                let names = error
                    .forms()
                    .iter()
                    .map(|used| (used.form(), used.detail()))
                    .collect::<Vec<_>>();
                assert_eq!(
                    names,
                    [(Form::Parameters, Some("entry"))],
                    "a checked program is refused for more than its entry:\n{source}\n{error}"
                );
            }
        }
    }
    assert!(lowered > 100, "{lowered} programs lowered");
}

#[test]
fn the_corpus_reports_no_case_as_not_lowered() {
    let corpus = vibra_conformance::Corpus::discover(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/cases"),
    )
    .expect("the corpus");
    let runner = vibra_conformance::ConformanceRunner::new(
        vibra_conformance::standard_dispatcher(),
    );
    let report = runner.run(&corpus);
    let wasm = report.wasm_counts();
    assert_eq!(wasm.not_lowered, 0);
    assert_eq!(wasm.failed, 0);
    assert!(wasm.matched > 100);
}
