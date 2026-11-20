//! The soundness of the discharge: `docs/spec/02-type-system.md`, "Bare
//! guards". A wrong `passed` removes a check the author wrote, so each test
//! here pairs a program the rules must not discharge with its nearest
//! neighbour they do discharge.

use vibra_diagnostics::DiagnosticCode;

use super::{SOURCE, at, faults};
use crate::check::{ObligationKind, ObligationStatus, check_source};

const REQUIRES: ObligationKind = ObligationKind::Requires;
const ENSURES: ObligationKind = ObligationKind::Ensures;
const PASSED: ObligationStatus = ObligationStatus::Passed;
const UNKNOWN: ObligationStatus = ObligationStatus::Unknown;
const UNPROVED: DiagnosticCode = DiagnosticCode::ContractUnproved;

/// The kind and status of every obligation of `source`, sorted.
fn statuses(source: &str) -> Vec<(ObligationKind, ObligationStatus)> {
    let mut found = check_source(SOURCE, source)
        .obligations()
        .iter()
        .map(|obligation| (obligation.kind(), obligation.status()))
        .collect::<Vec<_>>();
    found.sort_by_key(|(kind, status)| (format!("{kind:?}"), format!("{status:?}")));
    found
}

/// `source` has the faults `faults_at`, each a code and the text of its
/// primary span, and obligations of the kinds and statuses `expected`, in
/// any order.
fn decides(
    source: &str,
    faults_at: &[(DiagnosticCode, &str)],
    expected: &[(ObligationKind, ObligationStatus)],
) {
    let wanted = faults_at
        .iter()
        .map(|(code, text)| (*code, at(source, text, 0)))
        .collect::<Vec<_>>();
    assert_eq!(faults(source), wanted, "{source}");
    let mut expected = expected.to_vec();
    expected.sort_by_key(|(kind, status)| (format!("{kind:?}"), format!("{status:?}")));
    assert_eq!(statuses(source), expected, "{source}");
}

/// The rules of the one obligation of `source` of kind `kind`.
fn rules(source: &str, kind: ObligationKind) -> Vec<String> {
    let result = check_source(SOURCE, source);
    let found = result
        .obligations()
        .iter()
        .filter(|obligation| obligation.kind() == kind)
        .map(|obligation| obligation.rules().to_vec())
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "{source}");
    found.into_iter().next().unwrap_or_default()
}

// G1: a call's operands are in the callee's parameter-slot order.

/// `lt`, a predicate of two labels with defaults, and `need`, whose bare
/// `requires` is `lt` with the labels `requires` gives.
fn labelled(requires: &str) -> String {
    format!(
        "fn lt labelled {{a: I32 be 0i32 b: I32 be 0i32}} returns r: Bool do return Ordered.less(a b) endfn\n\
         fn need takes {{x: I32 y: I32}} returns r: Void requires {requires} do return void endfn\n"
    )
}

#[test]
fn labelled_operands_are_compared_by_label_not_by_written_place() {
    // `lt(a: x b: y)` is x < y; the obligation is `lt(b: x a: y)`, y < x.
    let source = format!(
        "{}fn run takes {{x: I32 y: I32}} returns r: Void do if lt(a: x b: y) then return need(x y) endif return void endfn\n",
        labelled("lt(b: x a: y)")
    );
    decides(&source, &[(UNPROVED, "need(x y)")], &[(REQUIRES, UNKNOWN)]);
    // The same relation with the labels written in the other order.
    let source = format!(
        "{}fn run takes {{x: I32 y: I32}} returns r: Void do if lt(a: y b: x) then return need(x y) endif return void endfn\n",
        labelled("lt(b: x a: y)")
    );
    decides(&source, &[], &[(REQUIRES, PASSED)]);
    assert_eq!(rules(&source, REQUIRES), ["GUARD-3"]);
}

#[test]
fn an_omitted_label_is_its_default_in_its_own_slot() {
    // The obligation `lt(b: x)` is 0 < x, its `a` the default.
    let run = |condition: &str| {
        format!(
            "{}fn run takes {{x: I32 y: I32}} returns r: Void do if {condition} then return need(x y) endif return void endfn\n",
            labelled("lt(b: x)")
        )
    };
    let source = run("lt(b: x a: 0i32)");
    decides(&source, &[], &[(REQUIRES, PASSED)]);
    let source = run("lt(b: x a: 1i32)");
    decides(&source, &[(UNPROVED, "need(x y)")], &[(REQUIRES, UNKNOWN)]);
    // The default on the branch's side, written on the obligation's.
    let source = format!(
        "{}fn run takes {{x: I32 y: I32}} returns r: Void do if lt(b: x) then return need(x y) endif return void endfn\n",
        labelled("lt(a: 0i32 b: x)")
    );
    decides(&source, &[], &[(REQUIRES, PASSED)]);
}

/// `mk`, whose proven `ensures` relates its result to the label `lo`, and
/// `above`, whose bare `requires` is that its first argument exceeds its
/// second.
const MK: &str = "fn mk labelled {lo: I32 be 0i32 hi: I32 be 0i32} returns r: I32 requires Ordered.less(lo hi) ensures Ordered.greater(r lo) do return hi endfn\n\
     fn above takes {v: I32 w: I32} returns r: I32 requires Ordered.greater(v w) do return v endfn\n";

#[test]
fn a_certified_result_binds_each_label_to_its_own_argument() {
    // `mk(lo: x hi: y)` is y, greater than x and not greater than y.
    let run = |second: &str| {
        format!(
            "{MK}fn run takes {{x: I32 y: I32}} returns r: I32 do if Ordered.less(x y) then return above(mk(lo: x hi: y) {second}) endif return 0i32 endfn\n"
        )
    };
    let source = run("x");
    decides(
        &source,
        &[],
        &[(REQUIRES, PASSED), (REQUIRES, PASSED), (ENSURES, PASSED)],
    );
    let source = run("y");
    decides(
        &source,
        &[(UNPROVED, "above(mk(lo: x hi: y) y)")],
        &[(REQUIRES, UNKNOWN), (REQUIRES, PASSED), (ENSURES, PASSED)],
    );
}
