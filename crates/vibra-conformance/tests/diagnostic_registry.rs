//! Checks the implemented registry against the specification's canonical
//! table.
//!
//! `docs/spec/07-diagnostics-and-conformance.md` carries the canonical table
//! of codes and levels and requires, under `V1-DIAG`, that the queryable
//! registry and that table agree on membership, level, domain, and fix
//! capability. This test is that check. Without it the two could drift, and
//! the release gate's requirement that a code's registered level match what it
//! emits would rest on nobody having made a typo.

// A test asserts by panicking.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vibra_diagnostics::{DiagnosticCode, FixCapability, Level};

const CHAPTER: &str = "docs/spec/07-diagnostics-and-conformance.md";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lives two levels below the workspace root")
        .to_path_buf()
}

/// The code-to-level table in the specification chapter.
///
/// Rows are `| `@code` | `@level` |`. Nothing else in the chapter has that
/// shape, and an empty result fails the test rather than passing vacuously.
fn specification_table() -> BTreeMap<String, String> {
    let path = workspace_root().join(CHAPTER);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));

    let mut table = BTreeMap::new();
    for line in text.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        // A well-formed row splits into "", code, level, "".
        if cells.len() != 4 {
            continue;
        }
        let (Some(code), Some(level)) = (
            cells[1].strip_prefix('`').and_then(|c| c.strip_suffix('`')),
            cells[2].strip_prefix('`').and_then(|c| c.strip_suffix('`')),
        ) else {
            continue;
        };
        if !code.starts_with('@') || !matches!(level, "@error" | "@warning") {
            continue;
        }

        let previous = table.insert(code.to_owned(), level.to_owned());
        assert!(previous.is_none(), "{code} appears twice in the table");
    }

    assert!(
        !table.is_empty(),
        "found no code table in {CHAPTER}; the parser or the chapter changed"
    );
    table
}

fn implemented_registry() -> BTreeMap<String, String> {
    DiagnosticCode::ALL
        .iter()
        .map(|code| (code.as_atom().to_owned(), code.level().as_atom().to_owned()))
        .collect()
}

#[test]
fn the_registry_and_the_specification_table_have_the_same_codes() {
    let specified = specification_table();
    let implemented = implemented_registry();

    let missing: Vec<&String> = specified
        .keys()
        .filter(|code| !implemented.contains_key(*code))
        .collect();
    assert!(
        missing.is_empty(),
        "specified but not implemented: {missing:?}"
    );

    let extra: Vec<&String> = implemented
        .keys()
        .filter(|code| !specified.contains_key(*code))
        .collect();
    assert!(
        extra.is_empty(),
        "implemented but not in the specification table: {extra:?}"
    );
}

#[test]
fn every_registered_level_matches_the_specification_table() {
    let specified = specification_table();

    for code in DiagnosticCode::ALL {
        let Some(expected) = specified.get(code.as_atom()) else {
            continue; // Membership is the other test's failure to report.
        };
        assert_eq!(
            code.level().as_atom(),
            expected,
            "{} has the wrong level; the specification table governs",
            code.as_atom()
        );
    }
}

#[test]
fn every_registered_domain_is_the_first_component_of_its_code() {
    for code in DiagnosticCode::ALL {
        let atom = code.as_atom();
        let spelled = atom
            .strip_prefix('@')
            .and_then(|path| path.split('.').next())
            .unwrap_or_else(|| panic!("{atom} has no domain component"));
        assert_eq!(code.domain().as_str(), spelled, "for {atom}");
    }
}

#[test]
fn every_fix_capability_is_one_of_the_two_specified_atoms() {
    for code in DiagnosticCode::ALL {
        let capability = code.fix_capability().as_atom();
        assert!(
            matches!(capability, "@safe" | "@none"),
            "{} has fix capability {capability}, which the specification does not define",
            code.as_atom()
        );
    }
}

#[test]
fn both_levels_are_populated() {
    // If one level had no members the two-level design would carry no
    // information, which is the reasoning recorded for the two warnings.
    for level in [Level::Error, Level::Warning] {
        assert!(
            DiagnosticCode::ALL.iter().any(|code| code.level() == level),
            "no code is registered at {level}"
        );
    }
}

#[test]
fn a_fix_capability_is_only_claimed_for_a_code_the_chapter_calls_fixable() {
    // The chapter says presentation with one unambiguous semantic binding may
    // parse with a style diagnostic and a safe formatter fix. A code claiming
    // `@safe` outside that reading needs the chapter changed first.
    for code in DiagnosticCode::ALL {
        if code.fix_capability() == FixCapability::Safe {
            assert_eq!(
                code.level(),
                Level::Warning,
                "{} claims a safe fix; v1 only offers fixes for normalized \
                 presentation, which is never an error",
                code.as_atom()
            );
        }
    }
}
