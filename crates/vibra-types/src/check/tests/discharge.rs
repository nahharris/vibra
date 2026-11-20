//! The discharge of bare guards: `docs/spec/02-type-system.md`, "Bare
//! guards", and `docs/spec/07-diagnostics-and-conformance.md`, "Integer
//! facts". Each rule has a program it establishes and the nearest one it
//! does not.

use vibra_diagnostics::DiagnosticCode;

use super::{SOURCE, at, faults};
use crate::check::{ObligationKind, ObligationStatus, check_source};

/// `pos`, whose bare `requires` states that its argument is positive.
const POS: &str = "fn pos takes {x: I32} returns r: I32 requires Ordered.greater(x 0i32) do return x endfn\n";

/// The kind, status, and rules of every obligation of `source`.
fn obligations(source: &str) -> Vec<(ObligationKind, ObligationStatus, Vec<String>)> {
    check_source(SOURCE, source)
        .obligations()
        .iter()
        .map(|obligation| {
            (
                obligation.kind(),
                obligation.status(),
                obligation.rules().to_vec(),
            )
        })
        .collect()
}

fn passed(
    kind: ObligationKind,
    rules: &[&str],
) -> (ObligationKind, ObligationStatus, Vec<String>) {
    (
        kind,
        ObligationStatus::Passed,
        rules.iter().map(|rule| (*rule).to_owned()).collect(),
    )
}

fn unknown(kind: ObligationKind) -> (ObligationKind, ObligationStatus, Vec<String>) {
    (kind, ObligationStatus::Unknown, Vec::new())
}

const REQUIRES: ObligationKind = ObligationKind::Requires;

#[test]
fn a_constant_argument_is_decided_by_evaluation() {
    let source = format!("{POS}fn main returns r: I32 do return pos(5i32) endfn\n");
    assert_eq!(faults(&source), []);
    assert_eq!(obligations(&source), [passed(REQUIRES, &["GUARD-1"])]);
    let source = format!("{POS}fn main returns r: I32 do return pos(0i32) endfn\n");
    assert_eq!(
        faults(&source),
        [(
            DiagnosticCode::ContractRefuted,
            at(&source, "Ordered.greater(x 0i32)", 0)
        )]
    );
    let result = check_source(SOURCE, &source);
    let found = result
        .obligations()
        .iter()
        .map(|obligation| (obligation.status(), obligation.counterexample()))
        .collect::<Vec<_>>();
    assert_eq!(
        found,
        [(
            ObligationStatus::Refuted,
            Some("{kind: @record fields: {x: 0i32}}")
        )]
    );
}

#[test]
fn a_dominating_branch_holds_only_inside_it() {
    let branch = |then: &str, otherwise: &str| {
        format!(
            "{POS}fn f takes {{y: I32}} returns r: I32 do if Ordered.greater(y 0i32) then return {then} else return {otherwise} endif endfn\n"
        )
    };
    let source = branch("pos(y)", "0i32");
    assert_eq!(faults(&source), []);
    assert_eq!(obligations(&source), [passed(REQUIRES, &["GUARD-3"])]);
    let source = branch("0i32", "pos(y)");
    assert_eq!(
        faults(&source),
        [(DiagnosticCode::ContractUnproved, at(&source, "pos(y)", 0))]
    );
    assert_eq!(obligations(&source), [unknown(REQUIRES)]);
}

/// `source` has the faults `faults`, each a code and the text of its
/// primary span, and the obligations `expected`, each a kind, a status, and
/// the rules used.
fn discharges(
    source: &str,
    faults_at: &[(DiagnosticCode, &str)],
    expected: &[(ObligationKind, ObligationStatus, &[&str])],
) {
    let found = faults(source);
    let wanted = faults_at
        .iter()
        .map(|(code, text)| (*code, at(source, text, 0)))
        .collect::<Vec<_>>();
    assert_eq!(found, wanted, "{source}");
    let wanted = expected
        .iter()
        .map(|(kind, status, rules)| {
            (
                *kind,
                *status,
                rules
                    .iter()
                    .map(|rule| (*rule).to_owned())
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(obligations(source), wanted, "{source}");
}

const PASSED: ObligationStatus = ObligationStatus::Passed;
const UNKNOWN: ObligationStatus = ObligationStatus::Unknown;
const UNPROVED: DiagnosticCode = DiagnosticCode::ContractUnproved;

/// A function `name` of `a` and `b` whose bare `requires` is `predicate`.
fn requiring(name: &str, predicate: &str) -> String {
    format!(
        "fn {name} takes {{a: I32 b: I32}} returns r: I32 requires {predicate} do return a endfn\n"
    )
}

/// `f` of `x` and `y` returning `call` when `condition` holds.
fn when(condition: &str, call: &str) -> String {
    format!(
        "fn f takes {{x: I32 y: I32 z: I32}} returns r: I32 do if {condition} then return {call} else return 0i32 endif endfn\n"
    )
}

/// `Positive`, whose bare `where` is the named predicate `isPositive`.
const POSITIVE: &str = "type Positive of I32 where isPositive endtype\n\
     fn isPositive takes {v: I32} returns r: Bool do return Ordered.greater(v 0i32) endfn\n";

#[test]
fn a_type_invariant_supplies_its_own_predicate() {
    let body = |ty: &str| {
        format!(
            "{POS}fn f takes {{p: {ty}}} returns r: I32 do let {ty}(raw) be p endlet return pos(raw) endfn\n"
        )
    };
    discharges(
        &format!("{POSITIVE}{}", body("Positive")),
        &[],
        &[(REQUIRES, PASSED, &["GUARD-2"])],
    );
    let non_negative = "type NonNegative of I32 where isNonNegative endtype\n\
         fn isNonNegative takes {v: I32} returns r: Bool do return Ordered.greaterOrEqual(v 0i32) endfn\n";
    discharges(
        &format!("{non_negative}{}", body("NonNegative")),
        &[(UNPROVED, "pos(raw)")],
        &[(REQUIRES, UNKNOWN, &[])],
    );
}

#[test]
fn the_negation_of_a_condition_is_read_as_its_defining_relation() {
    let source = "fn nonpos takes {x: I32} returns r: I32 requires Ordered.lessOrEqual(x 0i32) do return x endfn\n\
         fn f takes {y: I32} returns r: I32 do if Ordered.greater(y 0i32) then return 0i32 else return nonpos(y) endif endfn\n";
    discharges(source, &[], &[(REQUIRES, PASSED, &["GUARD-3", "GUARD-6"])]);
    let source = format!(
        "{POS}fn f takes {{y: I32}} returns r: I32 do if Ordered.greater(y 0i32) then return 1i32 endif return pos(y) endfn\n"
    );
    discharges(
        &source,
        &[(UNPROVED, "pos(y)")],
        &[(REQUIRES, UNKNOWN, &[])],
    );
}

#[test]
fn a_match_arm_supplies_its_case() {
    let source = "fn isSome takes {o: Option(I32)} returns r: Bool do match o case Option.some(value: v) return true case _ return false endmatch endfn\n\
         fn posOpt takes {o: Option(I32)} returns r: I32 requires isSome(o) do return 1i32 endfn\n";
    discharges(
        &format!(
            "{source}fn f takes {{o: Option(I32)}} returns r: I32 do match o case Option.some(value: v) return posOpt(o) case _ return 0i32 endmatch endfn\n"
        ),
        &[],
        &[(REQUIRES, PASSED, &["GUARD-3"])],
    );
    let source = format!(
        "{source}fn f takes {{o: Option(I32)}} returns r: I32 do match o case Option.some(value: v) return 0i32 case _ return posOpt(o) endmatch endfn\n"
    );
    discharges(
        &source,
        &[(UNPROVED, "posOpt(o)")],
        &[(REQUIRES, UNKNOWN, &[])],
    );
}

#[test]
fn substitution_renames_aliases_and_arguments() {
    discharges(
        &format!(
            "{POS}fn f takes {{y: I32}} returns r: I32 do let z be y endlet if Ordered.greater(y 0i32) then return pos(z) else return 0i32 endif endfn\n"
        ),
        &[],
        &[(REQUIRES, PASSED, &["GUARD-4", "GUARD-3"])],
    );
    discharges(
        &format!(
            "{POS}fn g takes {{a: I32}} returns r: I32 requires Ordered.greater(a 0i32) do return pos(a) endfn\n"
        ),
        &[],
        &[(REQUIRES, PASSED, &["GUARD-4"])],
    );
}

#[test]
fn the_boolean_structure_projects_and_introduces_and() {
    let h = requiring(
        "h",
        "Bool.and(Ordered.greater(a 0i32) Ordered.greater(b 0i32))",
    );
    discharges(
        &format!(
            "{POS}fn k takes {{a: I32 b: I32}} returns r: I32 requires Bool.and(Ordered.greater(a 0i32) Ordered.greater(b 0i32)) do return pos(a) endfn\n"
        ),
        &[],
        &[(REQUIRES, PASSED, &["GUARD-4", "GUARD-5"])],
    );
    discharges(
        &format!(
            "{h}fn f takes {{a: I32 b: I32}} returns r: I32 do if Ordered.greater(a 0i32) then if Ordered.greater(b 0i32) then return h(a b) else return 0i32 endif else return 0i32 endif endfn\n"
        ),
        &[],
        &[(REQUIRES, PASSED, &["GUARD-3", "GUARD-5"])],
    );
    discharges(
        &format!(
            "{POS}fn f takes {{a: I32 b: I32}} returns r: I32 do if Bool.or(Ordered.greater(a 0i32) Ordered.greater(b 0i32)) then return pos(a) else return 0i32 endif endfn\n"
        ),
        &[(UNPROVED, "pos(a)")],
        &[(REQUIRES, UNKNOWN, &[])],
    );
}

#[test]
fn each_integer_fact_has_a_passing_and_a_nearest_failing_program() {
    let not_equal = requiring("notEqualReq", "Bool.not(Equatable.equal(a b))");
    let ge = requiring("geReq", "Ordered.greaterOrEqual(a b)");
    let le = requiring("leReq", "Ordered.lessOrEqual(a b)");
    let eq = requiring("eqReq", "Equatable.equal(a b)");
    let lt = requiring("ltReq", "Ordered.less(a b)");
    let gt = requiring("gtReq", "Ordered.greater(a b)");
    let passing: [(String, &[&str]); 8] = [
        (
            format!(
                "{not_equal}{}",
                when("Ordered.less(x y)", "notEqualReq(x y)")
            ),
            &["GUARD-3", "GUARD-6", "INT-1", "GUARD-5"],
        ),
        (
            format!("{ge}{}", when("Ordered.greater(x y)", "geReq(x y)")),
            &["GUARD-3", "GUARD-6", "INT-2"],
        ),
        (
            format!("{le}{}", when("Equatable.equal(x y)", "leReq(y x)")),
            &["GUARD-3", "GUARD-6", "INT-3"],
        ),
        (
            format!(
                "{eq}{}",
                when(
                    "Ordered.lessOrEqual(x y)",
                    "if Ordered.lessOrEqual(y x) then eqReq(x y) else 0i32 endif"
                )
            ),
            &["GUARD-3", "GUARD-6", "INT-4"],
        ),
        (
            format!(
                "{lt}{}",
                when(
                    "Ordered.lessOrEqual(x y)",
                    "if Bool.not(Equatable.equal(x y)) then ltReq(x y) else 0i32 endif"
                )
            ),
            &["GUARD-3", "GUARD-5", "GUARD-6", "INT-5"],
        ),
        (
            format!(
                "{lt}{}",
                when(
                    "Ordered.less(x y)",
                    "if Ordered.lessOrEqual(y z) then ltReq(x z) else 0i32 endif"
                )
            ),
            &["GUARD-3", "GUARD-6", "INT-6"],
        ),
        (
            format!(
                "{le}{}",
                when(
                    "Ordered.lessOrEqual(x y)",
                    "if Ordered.lessOrEqual(y z) then leReq(x z) else 0i32 endif"
                )
            ),
            &["GUARD-3", "GUARD-6", "INT-7"],
        ),
        (
            format!("{gt}{}", when("Ordered.greater(x 5i32)", "gtReq(x 3i32)")),
            &["GUARD-6", "INT-8", "INT-2", "INT-6", "INT-1", "GUARD-3"],
        ),
    ];
    for (source, rules) in &passing {
        discharges(source, &[], &[(REQUIRES, PASSED, rules)]);
    }
    let failing = [
        (
            format!(
                "{not_equal}{}",
                when("Ordered.lessOrEqual(x y)", "notEqualReq(x y)")
            ),
            "notEqualReq(x y)",
        ),
        (
            format!("{ge}{}", when("Ordered.less(x y)", "geReq(x y)")),
            "geReq(x y)",
        ),
        (
            format!(
                "{le}{}",
                when("Bool.not(Equatable.equal(x y))", "leReq(y x)")
            ),
            "leReq(y x)",
        ),
        (
            format!("{eq}{}", when("Ordered.lessOrEqual(x y)", "eqReq(x y)")),
            "eqReq(x y)",
        ),
        (
            format!("{lt}{}", when("Ordered.lessOrEqual(x y)", "ltReq(x y)")),
            "ltReq(x y)",
        ),
        (
            format!(
                "{lt}{}",
                when(
                    "Ordered.lessOrEqual(x y)",
                    "if Ordered.lessOrEqual(y z) then ltReq(x z) else 0i32 endif"
                )
            ),
            "ltReq(x z)",
        ),
        (
            format!(
                "{le}{}",
                when(
                    "Ordered.lessOrEqual(x y)",
                    "if Ordered.lessOrEqual(z y) then leReq(x z) else 0i32 endif"
                )
            ),
            "leReq(x z)",
        ),
        (
            format!("{gt}{}", when("Ordered.greater(x 3i32)", "gtReq(x 5i32)")),
            "gtReq(x 5i32)",
        ),
    ];
    for (source, call) in &failing {
        discharges(source, &[(UNPROVED, call)], &[(REQUIRES, UNKNOWN, &[])]);
    }
}

#[test]
fn only_the_listed_relations_carry_facts() {
    let ge = requiring("geReq", "Ordered.greaterOrEqual(a b)");
    let le = requiring("leReq", "Ordered.lessOrEqual(a b)");
    discharges(
        &format!(
            "{ge}{}",
            when("Ordered.inRange(x 1i32 9i32)", "geReq(x 1i32)")
        ),
        &[],
        &[(REQUIRES, PASSED, &["GUARD-3", "GUARD-6"])],
    );
    discharges(
        &format!(
            "{le}{}",
            when("Equatable.equal(Ordered.min(x y) x)", "leReq(x y)")
        ),
        &[(UNPROVED, "leReq(x y)")],
        &[(REQUIRES, UNKNOWN, &[])],
    );
    discharges(
        &format!(
            "{POS}{}",
            when("Ordered.greater(x 0i32)", "pos(I32.add(x 1i32))")
        ),
        &[(UNPROVED, "pos(I32.add(x 1i32))")],
        &[(REQUIRES, UNKNOWN, &[])],
    );
}

#[test]
fn a_proven_ensures_holds_for_a_result_whose_requires_was_discharged() {
    discharges(
        &format!(
            "{POS}fn mk returns r: I32 ensures Ordered.greater(r 0i32) do return 1i32 endfn\nfn f returns r: I32 do return pos(mk()) endfn\n"
        ),
        &[],
        &[
            (ObligationKind::Ensures, PASSED, &["GUARD-1"]),
            (REQUIRES, PASSED, &["GUARD-7"]),
        ],
    );
    discharges(
        &format!(
            "{POS}fn mk takes {{a: I32}} returns r: I32 requires Ordered.greater(a 0i32) else 1i32 ensures Ordered.greater(r 0i32) do return a endfn\nfn f takes {{y: I32}} returns r: I32 do return pos(mk(y)) endfn\n"
        ),
        &[(UNPROVED, "pos(mk(y))")],
        &[
            (ObligationKind::Ensures, PASSED, &["GUARD-4"]),
            (REQUIRES, UNKNOWN, &[]),
        ],
    );
}

#[test]
fn an_ensures_holds_at_every_exit() {
    discharges(
        "fn bad returns r: I32 ensures Ordered.greater(r 0i32) do return 0i32 endfn\n",
        &[(DiagnosticCode::ContractRefuted, "Ordered.greater(r 0i32)")],
        &[(
            ObligationKind::Ensures,
            ObligationStatus::Refuted,
            &["GUARD-1"],
        )],
    );
    discharges(
        "fn two takes {y: I32} returns r: I32 ensures Ordered.greater(r 0i32) do if Ordered.greater(y 0i32) then return y else return y endif endfn\n",
        &[(UNPROVED, "Ordered.greater(r 0i32)")],
        &[(ObligationKind::Ensures, UNKNOWN, &[])],
    );
    discharges(
        "fn isSome takes {o: Option(I32)} returns r: Bool do match o case Option.some(value: v) return true case _ return false endmatch endfn\n\
         fn t takes {o: Option(I32)} returns r: Option(I32) ensures isSome(r) do let v be try o endlet return Option.some(value: v) endfn\n",
        &[(UNPROVED, "isSome(r)")],
        &[(ObligationKind::Ensures, UNKNOWN, &[])],
    );
}

#[test]
fn a_bare_where_holds_at_every_construction() {
    let construct = |body: &str| {
        format!("{POSITIVE}fn f takes {{x: I32}} returns r: Positive do {body} endfn\n")
    };
    discharges(
        &construct("return Positive(3i32)"),
        &[],
        &[(ObligationKind::Where, PASSED, &["GUARD-1"])],
    );
    discharges(
        &construct("return Positive(x)"),
        &[(UNPROVED, "Positive(x)")],
        &[(ObligationKind::Where, UNKNOWN, &[])],
    );
    discharges(
        &construct(
            "if isPositive(x) then return Positive(x) else return Positive(1i32) endif",
        ),
        &[],
        &[
            (ObligationKind::Where, PASSED, &["GUARD-3"]),
            (ObligationKind::Where, PASSED, &["GUARD-1"]),
        ],
    );
}

#[test]
fn a_function_value_keeps_no_obligation() {
    let source = format!(
        "{POS}fn main returns r: I32 do let f: Fn({{I32}} I32) be pos endlet return f(1i32) endfn\n"
    );
    assert_eq!(
        faults(&source),
        [(
            DiagnosticCode::ContractObligationLost,
            at(&source, "pos", 1)
        )]
    );
    let source = format!(
        "{POSITIVE}fn posP takes {{Positive(raw): Positive}} returns r: I32 requires isPositive(raw) do return raw endfn\n\
         fn main returns r: I32 do let f: Fn({{Positive}} I32) be posP endlet return f(Positive(1i32)) endfn\n"
    );
    assert_eq!(faults(&source), []);
    let source = "fn posElse takes {x: I32} returns r: I32 requires Ordered.greater(x 0i32) else -1i32 do return x endfn\n\
         fn main returns r: I32 do let f: Fn({I32} I32) be posElse endlet return f(0i32) endfn\n";
    assert_eq!(
        super::run(source).as_deref(),
        Ok(super::value("i32", "-1i32").as_str())
    );
}

/// A discharged guard leaves no code: the lowered program of `pos(5i32)` is
/// the program of the same source with the `requires` written as spaces, so
/// that every span is the same.
#[test]
fn a_discharged_guard_leaves_no_code() {
    let guard = "requires Ordered.greater(x 0i32) ";
    let main = "fn main returns r: I32 do return pos(5i32) endfn\n";
    let guarded = format!("{POS}{main}");
    let plain = guarded.replacen(guard, &" ".repeat(guard.len()), 1);
    assert_ne!(guarded, plain);
    let guarded = check_source(SOURCE, &guarded);
    let plain = check_source(SOURCE, &plain);
    assert_eq!(guarded.diagnostics(), []);
    assert_eq!(plain.diagnostics(), []);
    assert!(guarded.module_set().is_some());
    assert_eq!(guarded.module_set(), plain.module_set());
}
