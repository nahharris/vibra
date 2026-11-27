//! Checks the registry of `vibra-diagnostics` against the specification's
//! table.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Diagnostics are a language
//! surface", says the table is the single authority and every other mention
//! of a code or its level must agree with it. These tests are that check for
//! the compiler's registry: membership, level, domain, and fix capability.

use crate::support::workspace_root;

use std::collections::{BTreeMap, BTreeSet};

use vibra_diagnostics::{DiagnosticCode, Domain, FixCapability, Level};

const CHAPTER: &str = "docs/spec/07-diagnostics-and-conformance.md";

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

    let different: Vec<String> = specified
        .iter()
        .filter_map(|(code, level)| {
            implemented
                .get(code)
                .filter(|implemented| *implemented != level)
                .map(|implemented| format!("{code}: {implemented}, specified {level}"))
        })
        .collect();
    assert!(
        different.is_empty(),
        "levels that differ from the specification table: {different:?}"
    );
}

#[test]
fn the_registry_is_in_specification_table_order() {
    // The registry order breaks ties when diagnostics sort, so it is the
    // table's order.
    let path = workspace_root().join(CHAPTER);
    let text = std::fs::read_to_string(&path).expect("the chapter is readable");
    let specified: Vec<&str> = text
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let [_, code, level, _] = cells.as_slice() else {
                return None;
            };
            let code = code.strip_prefix('`')?.strip_suffix('`')?;
            let level = level.strip_prefix('`')?.strip_suffix('`')?;
            (code.starts_with('@') && matches!(level, "@error" | "@warning"))
                .then_some(code)
        })
        .collect();
    let implemented: Vec<&str> = DiagnosticCode::ALL
        .iter()
        .map(|code| code.as_atom())
        .collect();
    assert_eq!(implemented, specified);
}

#[test]
fn the_registry_has_117_codes() {
    assert_eq!(DiagnosticCode::ALL.len(), 117);
    let unique = DiagnosticCode::ALL
        .iter()
        .map(|code| code.as_atom())
        .collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), 117);
}

#[test]
fn exactly_three_codes_are_warnings() {
    for code in DiagnosticCode::ALL {
        let expected = if matches!(
            code.as_atom(),
            "@type.suspiciousRow" | "@style.casing" | "@contract.unusedEffect"
        ) {
            Level::Warning
        } else {
            Level::Error
        };
        assert_eq!(code.level(), expected, "for {code}");
    }
}

#[test]
fn every_domain_is_the_first_component_of_its_code_with_the_specified_counts() {
    let mut counts = BTreeMap::<&str, usize>::new();
    for code in DiagnosticCode::ALL {
        let atom = code.as_atom();
        let spelled = atom
            .strip_prefix('@')
            .and_then(|path| path.split('.').next())
            .unwrap_or_else(|| panic!("{atom} has no domain component"));
        assert_eq!(code.domain().as_str(), spelled, "for {atom}");
        *counts.entry(code.domain().as_str()).or_default() += 1;
    }
    let expected = [
        ("syntax", 11),
        ("name", 10),
        ("module", 11),
        ("type", 30),
        ("pattern", 6),
        ("effect", 4),
        ("external", 3),
        ("data", 5),
        ("project", 10),
        ("runtime", 3),
        ("style", 1),
        ("contract", 7),
        ("grant", 6),
        ("tool", 9),
        ("test", 1),
    ];
    assert_eq!(
        Domain::ALL
            .iter()
            .map(|domain| domain.as_str())
            .collect::<Vec<_>>(),
        expected
            .iter()
            .map(|(domain, _)| *domain)
            .collect::<Vec<_>>(),
        "the fifteen domains"
    );
    assert_eq!(counts, BTreeMap::from(expected));
}

#[test]
fn every_code_has_fix_capability_none() {
    for code in DiagnosticCode::ALL {
        assert_eq!(code.fix_capability(), FixCapability::None, "for {code}");
    }
}

#[test]
fn no_code_is_spelled_with_a_hyphen() {
    for code in DiagnosticCode::ALL {
        assert!(!code.as_atom().contains('-'), "{code} has a hyphen");
    }
}
