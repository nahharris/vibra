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

// G2: a `match` is read through only past arms known not to match.

/// `Pick`, `ok` with the arms `arms`, `need`, whose bare `requires` is
/// `ok(x)`, and `run`, which calls `need(x)` in its `pick` arm.
fn picked(field: &str, arms: &str, guard: &str) -> String {
    format!(
        "type Pick case pick {{value: {field}}} case empty endtype\n\
         fn ok takes {{x: Pick}} returns r: Bool {guard}do match x {arms} endmatch endfn\n\
         fn need takes {{x: Pick}} returns r: Void requires ok(x) do return void endfn\n\
         fn run takes {{x: Pick}} returns r: Void do match x case Pick.pick(value: _) return need(x) case Pick.empty() return void endmatch endfn\n"
    )
}

#[test]
fn a_possibly_matching_earlier_arm_stops_the_reading() {
    let source = picked(
        "I32",
        "case Pick.pick(value: 0i32) return false case Pick.pick(value: _) return true case Pick.empty() return false",
        "",
    );
    decides(&source, &[(UNPROVED, "need(x)")], &[(REQUIRES, UNKNOWN)]);
    // Without the literal arm the case fact selects the `true` arm.
    let source = picked(
        "I32",
        "case Pick.pick(value: _) return true case Pick.empty() return false",
        "",
    );
    decides(&source, &[], &[(REQUIRES, PASSED)]);
    assert_eq!(rules(&source, REQUIRES), ["GUARD-3"]);
    // An arm of another case is disjoint and is passed over.
    let source = picked(
        "I32",
        "case Pick.empty() return false case Pick.pick(value: _) return true",
        "",
    );
    decides(&source, &[], &[(REQUIRES, PASSED)]);
}

#[test]
fn a_nested_literal_makes_an_arm_possibly_matching() {
    let tuple = picked(
        "{I32 I32}",
        "case Pick.pick(value: {0i32 _}) return false case Pick.pick(value: _) return true case Pick.empty() return false",
        "",
    );
    decides(&tuple, &[(UNPROVED, "need(x)")], &[(REQUIRES, UNKNOWN)]);
    let list = picked(
        "List(I32)",
        "case Pick.pick(value: [0i32]) return false case Pick.pick(value: _) return true case Pick.empty() return false",
        "",
    );
    decides(&list, &[(UNPROVED, "need(x)")], &[(REQUIRES, UNKNOWN)]);
}

#[test]
fn a_known_field_decides_a_literal_arm() {
    let ok = "type Pair case pair {value: I32 other: I32} case none endtype\n\
         fn ok takes {x: Pair} returns r: Bool do match x case Pair.pair(value: 0i32 other: _) return false case Pair.pair(value: _ other: _) return true case Pair.none() return false endmatch endfn\n\
         fn need takes {x: Pair} returns r: Void requires ok(x) do return void endfn\n";
    // `value` is 1: the literal arm is disjoint.
    let source = format!(
        "{ok}fn run takes {{y: I32}} returns r: Void do return need(Pair.pair(value: 1i32 other: y)) endfn\n"
    );
    decides(&source, &[], &[(REQUIRES, PASSED)]);
    // `value` is unknown: the literal arm may match.
    let source = format!(
        "{ok}fn run takes {{y: I32}} returns r: Void do return need(Pair.pair(value: y other: 1i32)) endfn\n"
    );
    decides(
        &source,
        &[(UNPROVED, "need(Pair.pair(value: y other: 1i32))")],
        &[(REQUIRES, UNKNOWN)],
    );
}

// G3: a callable with a guard is not read through.

#[test]
fn a_guarded_callable_is_not_read_through() {
    let arms = "case Pick.pick(value: _) return true case Pick.empty() return false";
    // `ok` always returns its entry consequence, `false`.
    let source = picked("I32", arms, "requires false else false ");
    decides(&source, &[(UNPROVED, "need(x)")], &[(REQUIRES, UNKNOWN)]);
    // Its exit consequence replaces every candidate that fails the guard.
    let source = picked("I32", arms, "ensures Bool.not(r) else false ");
    decides(&source, &[(UNPROVED, "need(x)")], &[(REQUIRES, UNKNOWN)]);
    // The unguarded twin is read through.
    let source = picked("I32", arms, "");
    decides(&source, &[], &[(REQUIRES, PASSED)]);
}

#[test]
fn a_guarded_named_invariant_is_not_unfolded() {
    // `isPositive(v)` is `true` for every `v` at most 100: its entry
    // consequence. A `Positive` may hold -5, so `pos(raw)` is not known.
    let source = "type Positive of I32 where isPositive endtype\n\
         fn isPositive takes {v: I32} returns r: Bool requires Ordered.greater(v 100i32) else true do return Ordered.greater(v 0i32) endfn\n\
         fn pos takes {x: I32} returns r: I32 requires Ordered.greater(x 0i32) do return x endfn\n\
         fn f takes {p: Positive} returns r: I32 do let Positive(raw) be p endlet return pos(raw) endfn\n";
    decides(source, &[(UNPROVED, "pos(raw)")], &[(REQUIRES, UNKNOWN)]);
}
