//! No value index, offset, or instance identity appears in typed IR
//! (ledger D2.2, `docs/spec/06-runtime.md`, "The value arena").
//!
//! The canonical form of a checked program is what a snapshot records and what
//! the Wasm backend consumes. It names positions in the program itself, such as
//! an activation slot, a module value or function by its place in the program,
//! a union member, and a tuple component, because those are structure and not
//! references to values. It must never name a place in an instance: an arena
//! index, a linear-memory offset, an address, a handle, or an instance. Two
//! properties enforce that over every source document of the corpus that
//! reaches the checker: no field of the canonical form has a name that denotes
//! such a reference, and the form is byte-identical across independent checks
//! and processes, which an address or an instance identity would break.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use vibra_conformance::{ConformanceOperation, Corpus};

/// A field name that contains one of these denotes a place in an instance.
const INSTANCE_WORDS: &[&str] = &[
    "offset", "address", "pointer", "ptr", "handle", "instance", "arena", "heap",
    "memory",
];

/// A field name that is one of these denotes a value ID.
const VALUE_ID_NAMES: &[&str] = &["id", "value-id", "value_id", "ref", "reference"];

fn corpus() -> Corpus {
    Corpus::discover(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/cases"),
    )
    .expect("the corpus loads")
}

/// The canonical forms of every checked program reachable from a single source
/// document of an `interpret` or `type-check` case, in corpus order.
fn canonical_forms() -> Vec<(String, String)> {
    let mut forms = Vec::new();
    for case in corpus().cases() {
        let operation = case.manifest().operation();
        if !matches!(
            operation,
            ConformanceOperation::Interpret | ConformanceOperation::TypeCheck
        ) {
            continue;
        }
        let Some(source_id) = case.manifest().inputs.source.as_deref() else {
            continue;
        };
        let source = case.read_file(source_id).expect("the source reads");
        let result = vibra_types::check_source(source_id, &source);
        if let Some(program) = result.program() {
            forms.push((case.manifest().id.clone(), program.canonical_vibon()));
        }
    }
    forms
}

/// Every field name in a canonical form, outside string literals.
fn field_names(canonical: &str) -> BTreeSet<String> {
    let mut outside = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for character in canonical.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
        } else {
            outside.push(character);
        }
    }
    let mut names = BTreeSet::new();
    let mut word = String::new();
    for character in outside.chars() {
        if character.is_alphanumeric() || character == '-' || character == '_' {
            word.push(character);
        } else {
            if character == ':' && !word.is_empty() {
                names.insert(word.clone());
            }
            word.clear();
        }
    }
    names
}

/// The identifiers a source document writes, which are the only user-chosen
/// field names its canonical form can carry.
fn written_names(source: &str) -> BTreeSet<String> {
    source
        .split(|character: char| {
            !(character.is_alphanumeric() || character == '-' || character == '_')
        })
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

#[test]
fn no_field_of_the_canonical_form_denotes_a_place_in_an_instance() {
    let mut programs = 0_usize;
    let mut structural = BTreeSet::new();
    for case in corpus().cases() {
        if !matches!(
            case.manifest().operation(),
            ConformanceOperation::Interpret | ConformanceOperation::TypeCheck
        ) {
            continue;
        }
        let Some(source_id) = case.manifest().inputs.source.as_deref() else {
            continue;
        };
        let source = case.read_file(source_id).expect("the source reads");
        let result = vibra_types::check_source(source_id, &source);
        let Some(program) = result.program() else {
            continue;
        };
        programs += 1;
        let written = written_names(&source);
        let case_id = &case.manifest().id;
        for name in field_names(&program.canonical_vibon()) {
            // A name the program's own source writes is a record field or a
            // similar user choice, and may be anything.
            if written.contains(&name) {
                continue;
            }
            for word in INSTANCE_WORDS {
                assert!(
                    !name.contains(word),
                    "{case_id}: field `{name}` names a place in an instance"
                );
            }
            assert!(
                !VALUE_ID_NAMES.contains(&name.as_str()),
                "{case_id}: field `{name}` names a value ID"
            );
            structural.insert(name);
        }
    }
    assert!(
        programs > 100,
        "the corpus yields many programs: {programs}"
    );
    // The scan saw the structure of the form: positions in the program, which
    // are present by design (`slot`, `index`, `function`, `member`), and none
    // of them is a reference into an instance.
    for position in ["kind", "slot", "index", "function"] {
        assert!(
            structural.contains(position),
            "`{position}` is a structural field: {structural:?}"
        );
    }
}

#[test]
fn the_field_scanner_ignores_string_contents_and_finds_real_fields() {
    let names = field_names(
        "(record kind: @literal value: \"offset: 4 and address: 5 \\\" handle:\" slot: 3u64)",
    );
    assert_eq!(
        names,
        BTreeSet::from(["kind".to_owned(), "value".to_owned(), "slot".to_owned()])
    );
    assert!(field_names("(record arena-offset: 1u64)").contains("arena-offset"));
}

#[test]
fn the_canonical_form_is_identical_across_independent_checks() {
    assert_eq!(canonical_forms(), canonical_forms());
}

/// Prints a digest of every canonical form, for the parent test to compare.
#[test]
#[ignore = "run only as the child of `the_canonical_form_is_identical_across_processes`"]
fn canonical_forms_in_child_process() {
    let forms = canonical_forms();
    let total = forms
        .iter()
        .map(|(case_id, canonical)| format!("{case_id}\n{canonical}"))
        .collect::<Vec<_>>()
        .join("\u{1}");
    // A 64-bit FNV-1a digest, which needs no dependency and covers every byte.
    let mut digest = 0xcbf2_9ce4_8422_2325_u64;
    for byte in total.bytes() {
        digest ^= u64::from(byte);
        digest = digest.wrapping_mul(0x0100_0000_01b3);
    }
    println!("\nDIGEST:{digest:016x}:{}", total.len());
}

#[test]
fn the_canonical_form_is_identical_across_processes() {
    let output = Command::new(std::env::current_exe().expect("this test binary"))
        .args([
            "--ignored",
            "--exact",
            "canonical_forms_in_child_process",
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .expect("the child process runs");
    assert!(output.status.success());
    let first = String::from_utf8(output.stdout).expect("utf-8");
    let digest_of = |text: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix("DIGEST:"))
            .unwrap_or_else(|| panic!("no digest in:\n{text}"))
            .to_owned()
    };

    let forms = canonical_forms();
    let total = forms
        .iter()
        .map(|(case_id, canonical)| format!("{case_id}\n{canonical}"))
        .collect::<Vec<_>>()
        .join("\u{1}");
    let mut digest = 0xcbf2_9ce4_8422_2325_u64;
    for byte in total.bytes() {
        digest ^= u64::from(byte);
        digest = digest.wrapping_mul(0x0100_0000_01b3);
    }
    assert_eq!(
        digest_of(&first),
        format!("{digest:016x}:{}", total.len()),
        "another process produced another canonical form"
    );
}
