//! Guards with `else`, type invariants, and bare guards: the type chapter,
//! "Type invariants" and "Guards", and the runtime chapter, "Callable
//! contract execution".

use vibra_diagnostics::DiagnosticCode;
use vibra_interp::MemoryBudget;

use super::{SOURCE, at, faults, run, value};
use crate::check::interpret;

/// A function that counts down from `n`: its cost shows whether it ran.
const SPIN: &str = "fn spin takes {n: I32} returns r: I32 do if Equatable.equal(n 0i32) \
                    then return 0i32 else return spin(I32.sub(n 1i32)) endif endfn\n";

/// The reductions a run of `source` took.
fn reductions(source: &str) -> Result<u64, String> {
    let outcome = interpret(SOURCE, source, MemoryBudget::DEFAULT);
    match outcome.run {
        Some(Ok(execution)) => Ok(execution.instrumentation.reduction_count),
        Some(Err(error)) => Err(format!("{error:?}")),
        None => Err(format!("{:#?}", outcome.check.diagnostics())),
    }
}

const SPLIT: &str = "type SplitError case noParts endtype\n\
    fn split takes {total: I32 parts: I32} returns shares: Result(I32 SplitError) \
    requires Ordered.greaterOrEqual(parts 1i32) else Result.err(error: SplitError.noParts()) \
    do return Result.ok(value: total) endfn\n\
    fn code takes {x: Result(I32 SplitError)} returns r: I32 do match x \
    case Result.ok(value: v) return v case Result.err(error: SplitError.noParts()) return -1i32 \
    endmatch endfn\n";

#[test]
fn requires_else_returns_the_consequence_when_false() {
    let source = format!(
        "fn main returns r: {{I32 I32}} do return {{code(split(10i32 2i32)) \
         code(split(10i32 0i32))}} endfn\n{SPLIT}"
    );
    assert_eq!(
        run(&source),
        Ok(
            "{format: @value.v2 typeInfo: {kind: @tuple arguments: [@i32 @i32]} \
            value: {kind: @tuple values: [10i32 -1i32]}}"
                .to_owned()
        )
    );
}

#[test]
fn a_false_requires_skips_the_body() -> Result<(), String> {
    let program = |parts: &str| {
        format!(
            "fn main returns r: I32 do return f({parts}) endfn\n\
             fn f takes {{parts: I32}} returns r: I32 requires Ordered.greater(parts 0i32) \
             else 0i32 do return spin(1000i32) endfn\n{SPIN}"
        )
    };
    let skipped = reductions(&program("0i32"))?;
    let ran = reductions(&program("1i32"))?;
    assert!(skipped * 10 < ran, "skipped {skipped}, ran {ran}");
    Ok(())
}

#[test]
fn a_false_requires_skips_the_ensures() -> Result<(), String> {
    let program = |parts: &str| {
        format!(
            "fn main returns r: I32 do return f({parts}) endfn\n\
             fn f takes {{parts: I32}} returns r: I32 requires Ordered.greater(parts 0i32) \
             else 0i32 ensures Equatable.equal(r 5i32) else heavy(r) do return 5i32 endfn\n\
             fn heavy takes {{r: I32}} returns s: I32 do return r endfn\n"
        )
    };
    assert_eq!(run(&program("0i32")), Ok(value("i32", "0i32")));
    assert_eq!(run(&program("1i32")), Ok(value("i32", "5i32")));
    Ok(())
}

#[test]
fn guard_faults_are_reported_at_their_operand() {
    let wrong = "fn f takes {c: Bool} returns r: I32 requires c else \"no\" do return 1i32 endfn\n";
    assert_eq!(
        faults(wrong),
        vec![(
            DiagnosticCode::ContractInvalidConsequence,
            at(wrong, "\"no\"", 0)
        )]
    );
    let not_bool = "fn f takes {c: Bool} returns r: I32 requires 1i32 else 0i32 do return 1i32 endfn\n";
    assert_eq!(
        faults(not_bool),
        vec![(
            DiagnosticCode::ContractInvalidPredicate,
            at(not_bool, "1i32", 0)
        )]
    );
    let lookup = "fn f takes {x: List(I32)} returns r: I32 requires check(x) else 0i32 \
        do return 1i32 endfn\n\
        fn check takes {x: List(I32)} returns b: Bool do match x(0u64) \
        case Option.some(value: _) return true case Option.none() return false endmatch endfn\n";
    assert_eq!(
        faults(lookup),
        vec![(
            DiagnosticCode::ContractUnsupportedPredicate,
            at(lookup, "check(x)", 0)
        )]
    );
    let recursive = "fn f takes {x: I32} returns r: I32 requires even(x) else 0i32 do return x endfn\n\
        fn even takes {x: I32} returns b: Bool do match x case 0i32 return true \
        case _ return odd(I32.sub(x 1i32)) endmatch endfn\n\
        fn odd takes {x: I32} returns b: Bool do match x case 0i32 return false \
        case _ return even(I32.sub(x 1i32)) endmatch endfn\n";
    assert_eq!(
        faults(recursive),
        vec![(
            DiagnosticCode::ContractUnsupportedPredicate,
            at(recursive, "even(x)", 0)
        )]
    );
    let certified = "fn f takes {x: I32} returns r: I32 requires positive(x) else 0i32 do return x endfn\n\
        fn positive takes {x: I32} returns r: Bool do return Ordered.greater(x 0i32) endfn\n";
    assert_eq!(faults(certified), Vec::new());
}

const POSITIVE: &str = "type PositiveError case notPositive endtype\n\
    type Positive of I32 where fn takes {raw: I32} returns ok: Bool \
    do return Ordered.greater(raw 0i32) endfn else PositiveError.notPositive() endtype\n";

#[test]
fn where_else_constructs_a_result() {
    let source = format!(
        "fn main returns r: {{I32 I32}} do return {{code(Positive(1i32)) code(Positive(0i32))}} endfn\n\
         fn code takes {{p: Result(Positive PositiveError)}} returns r: I32 do match p \
         case Result.ok(value: Positive(raw)) return raw \
         case Result.err(error: PositiveError.notPositive()) return -1i32 endmatch endfn\n{POSITIVE}"
    );
    assert_eq!(
        run(&source),
        Ok(
            "{format: @value.v2 typeInfo: {kind: @tuple arguments: [@i32 @i32]} \
            value: {kind: @tuple values: [1i32 -1i32]}}"
                .to_owned()
        )
    );
}

#[test]
fn where_else_types_and_constants() {
    let typed = format!(
        "fn f takes {{x: I32}} returns r: Result(Positive PositiveError) do \
         let p: Result(Positive PositiveError) be Positive(x) endlet return p endfn\n{POSITIVE}"
    );
    assert_eq!(faults(&typed), Vec::new());
    let plain = format!(
        "fn f takes {{x: I32}} returns r: I32 do let p: Positive be Positive(x) endlet \
         return 0i32 endfn\n{POSITIVE}"
    );
    assert_eq!(
        faults(&plain),
        vec![(DiagnosticCode::TypeMismatch, at(&plain, "Positive(x)", 0))]
    );
    let nonconstant = "type E case bad {raw: I32} endtype\n\
        type P of I32 where fn takes {raw: I32} returns ok: Bool do return true endfn \
        else E.bad(raw: 0i32) endtype\n\
        type Q of I32 where fn takes {raw: I32} returns ok: Bool do return true endfn \
        else makeError(0i32) endtype\n\
        fn makeError takes {raw: I32} returns e: E do return E.bad(raw: raw) endfn\n";
    assert_eq!(
        faults(nonconstant),
        vec![(
            DiagnosticCode::ContractInvalidConsequence,
            at(nonconstant, "makeError(0i32)", 0)
        )]
    );
    let named = "type E case bad endtype\ntype P of I32 where isPositive else E.bad() endtype\n\
        fn isPositive takes {raw: I32} returns ok: Bool do return Ordered.greater(raw 0i32) endfn\n";
    assert_eq!(faults(named), Vec::new());
    let not_predicate = "type E case bad endtype\ntype P of I32 where Ordered.greater(1i32 0i32) else E.bad() endtype\n";
    assert_eq!(
        faults(not_predicate),
        vec![(
            DiagnosticCode::ContractInvalidPredicate,
            at(not_predicate, "Ordered.greater(1i32 0i32)", 0)
        )]
    );
    let variant = "type S case a case b where fn takes {s: S} returns ok: Bool do return true endfn endtype\n";
    let start = variant.find("where").unwrap_or_default();
    let end = variant.find(" endtype").unwrap_or_default();
    assert_eq!(
        faults(variant),
        vec![(
            DiagnosticCode::TypeInvalidInvariant,
            vibra_diagnostics::ByteSpan::new(start, end)
        )]
    );
}

#[test]
fn constant_constructions_are_evaluated() {
    let ok = format!(
        "fn main returns r: I32 do let Result.ok(value: Positive(raw)) be Positive(5i32) endlet \
         return raw endfn\n{POSITIVE}"
    );
    assert_eq!(run(&ok), Ok(value("i32", "5i32")));
    let err = format!(
        "fn main returns r: I32 do let Result.ok(value: Positive(raw)) be Positive(0i32) endlet \
         return raw endfn\n{POSITIVE}"
    );
    assert_eq!(
        faults(&err),
        vec![(
            DiagnosticCode::PatternRefutableBinding,
            at(&err, "Result.ok(value: Positive(raw))", 0)
        )]
    );
    let matched = format!(
        "fn main returns r: I32 do match Positive(0i32) case Result.ok(value: Positive(raw)) \
         return raw case Result.err(error: PositiveError.notPositive()) return -1i32 endmatch endfn\n{POSITIVE}"
    );
    assert_eq!(run(&matched), Ok(value("i32", "-1i32")));
    let parameter = format!(
        "fn f takes {{x: I32}} returns r: I32 do let Result.ok(value: Positive(raw)) be Positive(x) \
         endlet return raw endfn\n{POSITIVE}"
    );
    assert_eq!(
        faults(&parameter),
        vec![(
            DiagnosticCode::PatternRefutableBinding,
            at(&parameter, "Result.ok(value: Positive(raw))", 0)
        )]
    );
}

#[test]
fn a_constant_where_else_construction_is_no_constant_expression() {
    let source = format!(
        "fn f labelled {{p: Result(Positive PositiveError) be Positive(1i32)}} returns r: I32 do return 0i32 endfn\n{POSITIVE}"
    );
    assert_eq!(
        faults(&source),
        vec![(
            DiagnosticCode::TypeNotConstant,
            at(&source, "Positive(1i32)", 0)
        )]
    );
}

#[test]
fn bare_guards_are_unproved() {
    let requires = "fn f takes {x: I32} returns r: I32 requires Ordered.greater(x 0i32) do return x endfn\n\
        fn g takes {y: I32} returns r: I32 do return f(y) endfn\n";
    assert_eq!(
        faults(requires),
        vec![(DiagnosticCode::ContractUnproved, at(requires, "f(y)", 0))]
    );
    let uncalled = "fn f takes {x: I32} returns r: I32 requires Ordered.greater(x 0i32) do return x endfn\n";
    assert_eq!(faults(uncalled), Vec::new());
    let ensures = "fn f takes {x: I32} returns r: I32 ensures Ordered.greater(r 0i32) do return x endfn\n";
    assert_eq!(
        faults(ensures),
        vec![(
            DiagnosticCode::ContractUnproved,
            at(ensures, "Ordered.greater(r 0i32)", 0)
        )]
    );
}

#[test]
fn a_folded_guard_still_evaluates_the_arguments() -> Result<(), String> {
    let program = |guard: &str| {
        format!(
            "fn main returns r: I32 do return f(spin(1000i32)) endfn\n\
             fn f takes {{x: I32}} returns r: I32 {guard} do return x endfn\n{SPIN}"
        )
    };
    assert_eq!(
        run(&program("requires true else 7i32")),
        Ok(value("i32", "0i32"))
    );
    assert_eq!(
        run(&program("requires false else 7i32")),
        Ok(value("i32", "7i32"))
    );
    let folded = reductions(&program("requires true else 7i32"))?;
    let unguarded = reductions(&program(""))?;
    assert!(
        folded >= unguarded,
        "folded {folded}, unguarded {unguarded}"
    );
    assert!(
        folded < unguarded + 10,
        "folded {folded}, unguarded {unguarded}"
    );
    Ok(())
}

/// `chain{depth}`, a certified predicate whose cost doubles with each level.
fn chain(depth: usize) -> String {
    let mut source = String::from(
        "fn c0 takes {x: I32} returns r: Bool do return Ordered.less(x 100i32) endfn\n",
    );
    for level in 1..=depth {
        let below = level - 1;
        source.push_str(&format!(
            "fn c{level} takes {{x: I32}} returns r: Bool do return Bool.and(c{below}(x) c{below}(x)) endfn\n"
        ));
    }
    source
}

#[test]
fn the_ensures_predicate_runs_once_on_the_candidate() -> Result<(), String> {
    let program = |guard: &str, body: &str| {
        format!(
            "fn main returns r: I32 do return f(4i32) endfn\n\
             fn f takes {{x: I32}} returns r: I32 {guard} do {body} return x endfn\n{}",
            chain(6)
        )
    };
    let base = reductions(&program("", ""))?;
    let once = reductions(&program("", "let _ be c6(x) endlet"))? - base;
    let ensures = reductions(&program("ensures c6(r) else 0i32", ""))? - base;
    assert!(once > 100, "one evaluation costs {once}");
    assert!(
        ensures + 20 > once && ensures < once + 20,
        "ensures {ensures}, once {once}"
    );
    Ok(())
}

#[test]
fn a_pattern_on_an_invariant_type_runs_no_predicate() -> Result<(), String> {
    let program = |reads: usize| {
        let sum = (0..reads).fold(String::from("0i32"), |sum, _| {
            format!("I32.add(unwrap(p) {sum})")
        });
        format!(
            "fn main returns r: I32 do match Heavy(7i32) case Result.ok(value: p) return {sum} \
             case Result.err(error: _) return 0i32 endmatch endfn\n\
             fn unwrap takes {{p: Heavy}} returns raw: I32 do let Heavy(raw) be p endlet return raw endfn\n\
             type HeavyError case no endtype\n\
             type Heavy of I32 where c6 else HeavyError.no() endtype\n{}",
            chain(6)
        )
    };
    let one = reductions(&program(1))?;
    let four = reductions(&program(4))?;
    let construction = reductions(&program(0))?;
    let predicate = construction.saturating_sub(10);
    assert!(predicate > 100, "the predicate costs about {predicate}");
    assert!(
        four - one < predicate / 2,
        "three more reads cost {}",
        four - one
    );
    Ok(())
}
