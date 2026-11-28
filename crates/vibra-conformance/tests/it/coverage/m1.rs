//! The coverage audit of the M1 exit.
//!
//! An M1 form is an owner row whose step column names an M1 step before any
//! later milestone's name. M1 owns the grammar of chapter 01, the codes whose
//! owner cell names its steps, and the discharge rules `GUARD-1` to `GUARD-7`
//! with the integer facts `INT-1` to `INT-8` of `docs/spec/02-type-system.md`,
//! "Bare guards". A row that says it names host tests holds what the corpus
//! does not show of it.

use super::{Milestone, assert_covered};

/// M1's data.
const M1: Milestone = Milestone {
    name: "M1",
    // `07 / 23; M2 07, 21` is M1's; `M2 21 / M2 22` and `M3` are not.
    owns_step: |step| {
        step.split('M')
            .next()
            .is_some_and(|m1| m1.chars().any(|c| c.is_ascii_digit()))
    },
    owns_code: |owner| owner.starts_with("steps"),
    documents: &["types", "result", "testRun"],
    host_rows_hold_gaps: true,
    chapters: true,
    discharge_rules: &[
        "GUARD-1", "GUARD-2", "GUARD-3", "GUARD-4", "GUARD-5", "GUARD-6", "GUARD-7",
        "INT-1", "INT-2", "INT-3", "INT-4", "INT-5", "INT-6", "INT-7", "INT-8",
    ],
    host_only: &[],
    host_codes: &[],
    authority: None,
    coverage: &[],
    unavailable: None,
};

#[test]
fn m1_coverage() {
    assert_covered(&M1);
}
