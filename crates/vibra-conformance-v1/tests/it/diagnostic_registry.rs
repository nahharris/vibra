//! Checks the internal consistency of the v1 registry.
//!
//! The specification's table describes the registry of `vibra-diagnostics`,
//! which `vibra-conformance` checks against it. The v1 registry keeps the
//! checks that need no table: domain, fix capability, and level population.

// A test asserts by panicking.
use vibra_diagnostics_v1::{DiagnosticCode, FixCapability, Level};

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
