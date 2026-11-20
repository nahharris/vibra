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

// G5: the integer facts are derived under a budget, and running out of it
// is a tool limit.

/// `f` of `count` integers whose nested branches state `a0 ≤ a1`, …, and
/// which calls `leReq(a0 aLast)` inside them.
fn chain(count: usize) -> String {
    let parameters = (0..count)
        .map(|at| format!("a{at}: I32"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut body = format!("return leReq(a0 a{})", count - 1);
    for at in (0..count - 1).rev() {
        body = format!(
            "if Ordered.lessOrEqual(a{at} a{}) then {body} endif",
            at + 1
        );
    }
    format!(
        "fn leReq takes {{a: I32 b: I32}} returns r: I32 requires Ordered.lessOrEqual(a b) do return a endfn\n\
         fn f takes {{{parameters}}} returns r: I32 do {body} return 0i32 endfn\n"
    )
}

#[test]
fn a_chain_of_facts_within_the_budget_is_closed() {
    let source = chain(20);
    decides(&source, &[], &[(REQUIRES, PASSED)]);
    assert_eq!(rules(&source, REQUIRES), ["GUARD-3", "GUARD-6", "INT-7"]);
}

#[test]
fn a_closure_over_the_budget_is_a_tool_limit_alone() {
    let source = chain(80);
    decides(
        &source,
        &[(
            DiagnosticCode::ToolLimitExceeded,
            "Ordered.lessOrEqual(a b)",
        )],
        &[(REQUIRES, UNKNOWN)],
    );
}

// Siblings: each rule's soundness condition, with the program that would
// be wrongly discharged were the condition dropped.

const REFUTED: ObligationStatus = ObligationStatus::Refuted;

/// `pos`, whose bare `requires` states that its argument is positive.
const POS: &str = "fn pos takes {x: I32} returns r: I32 requires Ordered.greater(x 0i32) do return x endfn\n";

#[test]
fn constants_are_substituted_at_the_slots_their_labels_name() {
    // GUARD-1: `less(a b)` at `b: 1 a: 2` is 2 < 1.
    let source = "fn need labelled {a: I32 be 0i32 b: I32 be 0i32} returns r: I32 requires Ordered.less(a b) do return a endfn\n\
         fn run returns r: I32 do return need(b: 1i32 a: 2i32) endfn\n";
    decides(
        source,
        &[(DiagnosticCode::ContractRefuted, "Ordered.less(a b)")],
        &[(REQUIRES, REFUTED)],
    );
    let source = "fn need labelled {a: I32 be 0i32 b: I32 be 0i32} returns r: I32 requires Ordered.less(a b) do return a endfn\n\
         fn run returns r: I32 do return need(b: 2i32 a: 1i32) endfn\n";
    decides(source, &[], &[(REQUIRES, PASSED)]);
}

#[test]
fn an_earlier_arm_with_a_literal_gives_no_negative_case_fact() {
    // GUARD-3: past `pick(value: 0)`, `x` may still be a `pick`.
    let source = "type Pick case pick {value: I32} case empty endtype\n\
         fn isEmpty takes {x: Pick} returns r: Bool do match x case Pick.empty() return true case Pick.pick(value: _) return false endmatch endfn\n\
         fn need takes {x: Pick} returns r: I32 requires isEmpty(x) do return 0i32 endfn\n\
         fn run takes {x: Pick} returns r: I32 do match x case Pick.pick(value: 0i32) return 1i32 case _ return need(x) endmatch endfn\n";
    decides(source, &[(UNPROVED, "need(x)")], &[(REQUIRES, UNKNOWN)]);
    // The arm of the case itself gives it.
    let source = source.replace(
        "case _ return need(x)",
        "case Pick.empty() return need(x) case _ return 2i32",
    );
    decides(&source, &[], &[(REQUIRES, PASSED)]);
}

#[test]
fn a_fact_is_about_a_binding_not_a_name() {
    // GUARD-3 and GUARD-4 compare bindings. A name denotes one binding in
    // its scope: a `let` or a pattern binder of a name in scope is
    // `@name.redeclaration`, so no fact outlives the binding it names.
    let redeclared = |source: &str| {
        faults(source)
            .iter()
            .any(|(code, _)| *code == DiagnosticCode::NameRedeclaration)
    };
    let source = format!(
        "{POS}fn f takes {{x: I32}} returns r: I32 do if Ordered.greater(x 0i32) then let x be I32.sub(x 1i32) endlet return pos(x) endif return 0i32 endfn\n"
    );
    assert!(redeclared(&source), "{source}");
    let source = format!(
        "{POS}fn f takes {{x: I32 o: Option(I32)}} returns r: I32 do if Ordered.greater(x 0i32) then match o case Option.some(value: x) return pos(x) case _ return 0i32 endmatch endif return 0i32 endfn\n"
    );
    assert!(redeclared(&source), "{source}");
    // A binder of another name leaves the fact about `x`.
    // The same name, unshadowed, is the same binding.
    let source = format!(
        "{POS}fn f takes {{x: I32 o: Option(I32)}} returns r: I32 do if Ordered.greater(x 0i32) then match o case Option.some(value: y) return pos(x) case _ return 0i32 endmatch endif return 0i32 endfn\n"
    );
    decides(&source, &[], &[(REQUIRES, PASSED)]);
}

#[test]
fn only_a_bare_proven_ensures_with_a_discharged_requires_is_a_result_fact() {
    let call = "fn f returns r: I32 do return pos(mk()) endfn\n";
    // GUARD-7: a bare `ensures` proven of every exit.
    let source = format!(
        "{POS}fn mk returns r: I32 ensures Ordered.greater(r 0i32) do return 1i32 endfn\n{call}"
    );
    decides(&source, &[], &[(REQUIRES, PASSED), (ENSURES, PASSED)]);
    // An `ensures … else` may return its consequence, which is not checked
    // against the predicate.
    let source = format!(
        "{POS}fn mk takes {{x: I32}} returns r: I32 ensures Ordered.greater(r 0i32) else 0i32 do return x endfn\n\
         fn f takes {{x: I32}} returns r: I32 do return pos(mk(x)) endfn\n"
    );
    decides(&source, &[(UNPROVED, "pos(mk(x))")], &[(REQUIRES, UNKNOWN)]);
    // A `requires … else` may return its consequence, skipping the
    // `ensures`.
    let source = format!(
        "{POS}fn mk takes {{x: I32}} returns r: I32 requires Ordered.greater(x 0i32) else 0i32 ensures Ordered.greater(r 0i32) do return x endfn\n\
         fn f takes {{x: I32}} returns r: I32 do return pos(mk(x)) endfn\n"
    );
    decides(
        &source,
        &[(UNPROVED, "pos(mk(x))")],
        &[(REQUIRES, UNKNOWN), (ENSURES, PASSED)],
    );
    // An `ensures` not established at every exit is no fact.
    let source = format!(
        "{POS}fn mk takes {{x: I32}} returns r: I32 ensures Ordered.greater(r 0i32) do return x endfn\n\
         fn f takes {{x: I32}} returns r: I32 do return pos(mk(x)) endfn\n"
    );
    decides(
        &source,
        &[
            (UNPROVED, "Ordered.greater(r 0i32)"),
            (UNPROVED, "pos(mk(x))"),
        ],
        &[(REQUIRES, UNKNOWN), (ENSURES, UNKNOWN)],
    );
}

#[test]
fn constants_compare_by_the_value_of_their_own_type() {
    // INT-8 over a signed and an unsigned width.
    let gt = |ty: &str, fact: &str, goal: &str| {
        format!(
            "fn gtReq takes {{a: {ty} b: {ty}}} returns r: {ty} requires Ordered.greater(a b) do return a endfn\n\
             fn f takes {{x: {ty}}} returns r: {ty} do if Ordered.greater(x {fact}) then return gtReq(x {goal}) endif return x endfn\n"
        )
    };
    decides(&gt("I8", "-100i8", "-128i8"), &[], &[(REQUIRES, PASSED)]);
    let source = gt("I8", "-100i8", "-50i8");
    decides(
        &source,
        &[(UNPROVED, "gtReq(x -50i8)")],
        &[(REQUIRES, UNKNOWN)],
    );
    decides(&gt("U8", "200u8", "100u8"), &[], &[(REQUIRES, PASSED)]);
    let source = gt("U8", "100u8", "200u8");
    decides(
        &source,
        &[(UNPROVED, "gtReq(x 200u8)")],
        &[(REQUIRES, UNKNOWN)],
    );
}
/// The number of dependencies of each obligation of `source`.
fn dependency_counts(source: &str) -> Vec<usize> {
    check_source(SOURCE, source)
        .obligations()
        .iter()
        .map(|obligation| obligation.dependencies().len())
        .collect()
}

#[test]
fn a_module_value_the_discharge_reads_is_a_dependency() {
    let program = |argument: &str, condition: &str| {
        format!(
            "let limit: I32 be 1i32 endlet\n\
             fn ok takes {{x: I32}} returns r: Bool do return Ordered.greater(x 0i32) endfn\n\
             fn need takes {{x: I32}} returns r: Void requires ok(x) do return void endfn\n\
             fn run takes {{y: I32}} returns r: Void do if {condition} then return need({argument}) endif return void endfn\n"
        )
    };
    // GUARD-1 reads `ok`; the argument `limit` is read for its value.
    assert_eq!(dependency_counts(&program("1i32", "true")), [2]);
    assert_eq!(dependency_counts(&program("limit", "true")), [3]);
    // GUARD-3 reads the condition that names `limit`.
    let source = program("y", "ok(y)");
    assert_eq!(statuses(&source), [(REQUIRES, PASSED)]);
    assert_eq!(dependency_counts(&source), [1]);
    let source = "let limit: I32 be 0i32 endlet\n\
         fn need takes {x: I32} returns r: Void requires Ordered.greater(x 0i32) do return void endfn\n\
         fn run takes {y: I32} returns r: Void do if Ordered.greater(y limit) then return need(y) endif return void endfn\n";
    assert_eq!(statuses(source), [(REQUIRES, PASSED)]);
    assert_eq!(dependency_counts(source), [2]);
}

#[test]
fn fields_written_out_of_order_are_read_by_name() {
    // GUARD-1 over a record and a case whose fields are not written in
    // the type's order: `a` is 2 and `b` is 1, so `a < b` is false.
    let record = "fn need takes {p: {a: I32 b: I32}} returns r: I32 requires Ordered.less(p.a p.b) do return 0i32 endfn\n\
         fn run returns r: I32 do return need({b: 1i32 a: 2i32}) endfn\n";
    decides(
        record,
        &[(DiagnosticCode::ContractRefuted, "Ordered.less(p.a p.b)")],
        &[(REQUIRES, REFUTED)],
    );
    let case = "type Pair case pair {a: I32 b: I32} case none endtype\n\
         fn less takes {p: Pair} returns r: Bool do match p case Pair.pair(a: a b: b) return Ordered.less(a b) case Pair.none() return true endmatch endfn\n\
         fn need takes {p: Pair} returns r: I32 requires less(p) do return 0i32 endfn\n\
         fn run returns r: I32 do return need(Pair.pair(b: 1i32 a: 2i32)) endfn\n";
    decides(
        case,
        &[(DiagnosticCode::ContractRefuted, "less(p)")],
        &[(REQUIRES, REFUTED)],
    );
    // A field of a construction is its operand of that name.
    let symbolic = "fn need takes {p: {a: I32 b: I32}} returns r: I32 requires Ordered.less(p.a p.b) do return 0i32 endfn\n\
         fn run takes {x: I32 y: I32} returns r: I32 do if Ordered.less(y x) then return need({b: x a: y}) endif return 0i32 endfn\n";
    decides(symbolic, &[], &[(REQUIRES, PASSED)]);
    let wrong = symbolic.replace("need({b: x a: y})", "need({b: y a: x})");
    decides(
        &wrong,
        &[(UNPROVED, "need({b: y a: x})")],
        &[(REQUIRES, UNKNOWN)],
    );
}
#[test]
fn integer_facts_are_only_of_the_sized_integer_types() {
    // GUARD-6: a type's own `Ordered` need not be an order. This one
    // calls every pair less, so `less(a b)` does not give `greater(b a)`.
    let source = "type Key of U8 impl Ordered fn compare takes {left: Self right: Self} returns order: Ordering do return Ordering.less() endfn endimpl endtype\n\
         fn need takes {a: Key b: Key} returns r: I32 requires Ordered.greater(b a) do return 0i32 endfn\n\
         fn run takes {a: Key b: Key} returns r: I32 do if Ordered.less(a b) then return need(a b) endif return 0i32 endfn\n";
    // Its predicate is not certified, so no call to `need` is discharged.
    let uncertified = |source: &str| {
        assert!(
            statuses(source).iter().all(|(_, status)| *status != PASSED),
            "{source}"
        );
        assert!(
            faults(source)
                .iter()
                .any(|(code, _)| *code == DiagnosticCode::ContractUnsupportedPredicate),
            "{source}"
        );
    };
    uncertified(source);
    // Nor at an open type.
    uncertified(
        "fn need(T) takes {a: T b: T} returns r: I32 given {T: Ordered} requires Ordered.greater(b a) do return 0i32 endfn\n\
         fn run(T) takes {a: T b: T} returns r: I32 given {T: Ordered} do if Ordered.less(a b) then return need(a b) endif return 0i32 endfn\n",
    );
    // At a sized integer type it is INT-1.
    let source = "fn need takes {a: U8 b: U8} returns r: I32 requires Ordered.greater(b a) do return 0i32 endfn\n\
         fn run takes {a: U8 b: U8} returns r: I32 do if Ordered.less(a b) then return need(a b) endif return 0i32 endfn\n";
    decides(source, &[], &[(REQUIRES, PASSED)]);
}

#[test]
fn a_closure_argument_is_its_own_slot_order() {
    // GUARD-4 through a closure: its parameters are positional, and a
    // proof over the closure's body is not read through.
    let source = format!(
        "{POS}fn f takes {{x: I32}} returns r: I32 do let g be fn takes {{a: I32 b: I32}} returns s: I32 do return a endfn endlet if Ordered.greater(x 0i32) then return pos(g(0i32 x)) endif return 0i32 endfn\n"
    );
    decides(
        &source,
        &[(UNPROVED, "pos(g(0i32 x))")],
        &[(REQUIRES, UNKNOWN)],
    );
}
#[test]
fn a_branch_fact_ends_with_its_branch() {
    // GUARD-3: past `endif` the condition is no fact.
    let source = format!(
        "{POS}fn f takes {{x: I32}} returns r: I32 do let y be if Ordered.greater(x 0i32) then 1i32 else 2i32 endif endlet return pos(x) endfn\n"
    );
    decides(&source, &[(UNPROVED, "pos(x)")], &[(REQUIRES, UNKNOWN)]);
    // Nor past `endmatch`.
    let source = format!(
        "{POS}fn f takes {{x: I32 o: Option(I32)}} returns r: I32 do let y be match o case Option.some(value: v) if Ordered.greater(x 0i32) then v else 0i32 endif case _ 0i32 endmatch endlet return pos(x) endfn\n"
    );
    decides(&source, &[(UNPROVED, "pos(x)")], &[(REQUIRES, UNKNOWN)]);
    // An argument evaluated before the branch is not inside it.
    let source = format!(
        "{POS}fn two takes {{a: I32 b: Bool}} returns r: I32 do return a endfn\n\
         fn f takes {{x: I32}} returns r: I32 do return two(pos(x) Ordered.greater(x 0i32)) endfn\n"
    );
    decides(&source, &[(UNPROVED, "pos(x)")], &[(REQUIRES, UNKNOWN)]);
}

#[test]
fn a_result_fact_needs_the_requires_of_that_call_discharged() {
    // GUARD-7: `mk(x)` has its `requires` unproved, so its `ensures` is no
    // fact of its result.
    let source = format!(
        "{POS}fn mk takes {{x: I32}} returns r: I32 requires Ordered.greater(x 0i32) ensures Ordered.greater(r 0i32) do return x endfn\n\
         fn f takes {{x: I32}} returns r: I32 do return pos(mk(x)) endfn\n"
    );
    decides(
        &source,
        &[(UNPROVED, "pos(mk(x))"), (UNPROVED, "mk(x)")],
        &[(REQUIRES, UNKNOWN), (REQUIRES, UNKNOWN), (ENSURES, PASSED)],
    );
    // With it discharged, the result fact holds.
    let source = source.replace(
        "do return pos(mk(x)) endfn",
        "do if Ordered.greater(x 0i32) then return pos(mk(x)) endif return 0i32 endfn",
    );
    decides(
        &source,
        &[],
        &[(REQUIRES, PASSED), (REQUIRES, PASSED), (ENSURES, PASSED)],
    );
}
#[test]
fn a_contract_call_is_compared_in_its_member_slot_order() {
    // GUARD-1 and GUARD-4 at a member with labels written out of order.
    let member = |call: &str, guard: &str| {
        format!(
            "interface Span fn span takes {{value: Self}} labelled {{a: I32 be 0i32 b: I32 be 0i32}} returns r: I32 requires Ordered.less(a b) endfn endinterface\n\
             type A of I32 impl Span fn span takes {{value: Self}} labelled {{a: I32 be 0i32 b: I32 be 0i32}} returns s: I32 do return a endfn endimpl endtype\n\
             fn run takes {{x: I32 y: I32}} returns r: I32 do if {guard} then return {call} endif return 0i32 endfn\n"
        )
    };
    let source = member("Span.span(A(1i32) b: 1i32 a: 2i32)", "true");
    decides(
        &source,
        &[(DiagnosticCode::ContractRefuted, "Ordered.less(a b)")],
        &[(REQUIRES, REFUTED)],
    );
    let source = member("Span.span(A(1i32) b: 2i32 a: 1i32)", "true");
    decides(&source, &[], &[(REQUIRES, PASSED)]);
    let source = member("Span.span(A(1i32) b: x a: y)", "Ordered.less(y x)");
    decides(&source, &[], &[(REQUIRES, PASSED)]);
    let source = member("Span.span(A(1i32) b: y a: x)", "Ordered.less(y x)");
    decides(
        &source,
        &[(UNPROVED, "Span.span(A(1i32) b: y a: x)")],
        &[(REQUIRES, UNKNOWN)],
    );
}
