//! Unit tests of bodies: patterns, coverage, `let`, `try`, fallible values,
//! and `await`. Expectations come from the type and source chapters.

use vibra_diagnostics::DiagnosticCode;

use super::{SOURCE, at, faults, run, value};
use crate::check::check_source;
use crate::check::pattern::COVERAGE_LIMIT;

const SHAPE: &str =
    "type Shape case circle {radius: I32} case square {side: I32} case empty endtype\n";

/// The notes of every `@pattern.nonExhaustive` of `source`.
fn uncovered(source: &str) -> Vec<String> {
    check_source(SOURCE, source)
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.code() == DiagnosticCode::PatternNonExhaustive)
        .flat_map(|diagnostic| diagnostic.notes().to_vec())
        .collect()
}

/// A function matching a `Shape` with `arms`.
fn shape_match(arms: &str) -> String {
    format!(
        "{SHAPE}fn f takes {{s: Shape}} returns r: I32 do \
         return match s {arms} endmatch endfn\n"
    )
}

/// `docs/spec/02-type-system.md`, "Coverage": the note names the first
/// uncovered shape in declaration order, in pattern spelling with `_` for a
/// position only a binder covers.
#[test]
fn first_uncovered_shape_in_declaration_order() {
    assert_eq!(
        uncovered(&shape_match("case Shape.square(side: _) 1i32")),
        ["Shape.circle(radius: _)"]
    );
    assert_eq!(
        uncovered(&shape_match(
            "case Shape.circle(radius: r) r case Shape.empty() 0i32"
        )),
        ["Shape.square(side: _)"]
    );
    assert_eq!(
        uncovered(&shape_match(
            "case Shape.circle(radius: r) r case Shape.square(side: x) x"
        )),
        ["Shape.empty()"]
    );
    assert_eq!(
        uncovered(&shape_match(
            "case Shape.circle(radius: 0i32) 0i32 case Shape.square(side: x) x \
             case Shape.empty() 0i32"
        )),
        ["Shape.circle(radius: _)"]
    );
}

/// A nested position is spelled under its constructors, and a product
/// splits its first component first.
#[test]
fn nested_and_product_shapes_are_spelled() {
    let source = format!(
        "{SHAPE}fn f takes {{o: Option(Shape)}} returns r: I32 do return match o \
         case Option.some(value: Shape.circle(radius: r)) r \
         case Option.some(value: Shape.square(side: s)) s \
         case Option.none() 0i32 endmatch endfn\n"
    );
    assert_eq!(uncovered(&source), ["Option.some(value: Shape.empty())"]);
    let source = "fn f takes {t: {Bool Bool}} returns r: I32 do return match t \
                  case {true _} 1i32 case {false true} 2i32 endmatch endfn\n";
    assert_eq!(uncovered(source), ["{Bool.false() Bool.false()}"]);
    let source = "fn f takes {t: {a: I32 b: Bool}} returns r: I32 do return match t \
                  case {b: true} 1i32 endmatch endfn\n";
    assert_eq!(uncovered(source), ["{a: _ b: Bool.false()}"]);
    let source = "fn f takes {l: List(I32)} returns r: I32 do return match l \
                  case [] 0i32 case [a] a endmatch endfn\n";
    assert_eq!(uncovered(source), ["[_ _]"]);
}

/// A fixed-length list pattern never covers its list type, so the note names
/// the shortest length no arm covers in list spelling, never a `_` that
/// would include a covered length; a length an arm covers only in part comes
/// first, and a list of an uninhabited element has only the empty length.
#[test]
fn a_list_witness_names_an_uncovered_length() {
    let list = |arms: &str| {
        format!(
            "fn f takes {{xs: List(U8)}} returns r: U8 do return match xs {arms} endmatch endfn\n"
        )
    };
    assert_eq!(uncovered(&list("case [] 0u8")), ["[_]"]);
    assert_eq!(uncovered(&list("case [] 0u8 case [x] x")), ["[_ _]"]);
    assert_eq!(uncovered(&list("case [x] x")), ["[]"]);
    assert_eq!(uncovered(&list("case [] 0u8 case [1u8] 1u8")), ["[_]"]);
    assert_eq!(uncovered(&list("case [] 0u8 case [x _ _] x")), ["[_]"]);
    let source = "fn f takes {xs: List(List(U8))} returns r: U8 do return match xs \
                  case [] 0u8 case [[]] 1u8 endmatch endfn\n";
    assert_eq!(uncovered(source), ["[[_]]"]);
    let source = "fn f takes {xs: List(Never)} returns r: U8 do return match xs \
                  case [] 0u8 endmatch endfn\n";
    assert_eq!(uncovered(source), Vec::<String>::new());
}

/// An arm is related to the earliest arm by which the arms before it cover
/// it; an arm of an uninhabited case has no value at all and no relation.
#[test]
fn unreachable_arms_relate_the_earliest_covering_arm() {
    let source = shape_match(
        "case Shape.circle(radius: _) 1i32 case Shape.square(side: _) 2i32 \
         case Shape.empty() 3i32 case Shape.square(side: 4i32) 4i32",
    );
    let result = check_source(SOURCE, &source);
    let found = result
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            let related = diagnostic
                .related()
                .iter()
                .map(|related| related.span)
                .collect::<Vec<_>>();
            (diagnostic.code(), diagnostic.primary_span(), related)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        found,
        [(
            DiagnosticCode::PatternUnreachableArm,
            at(&source, "Shape.square(side: 4i32)", 0),
            vec![at(&source, "Shape.square(side: _)", 0)]
        )]
    );
    let source = "fn f takes {x: Result(I32 Never)} returns r: I32 do return match x \
                  case Result.ok(value: v) v case Result.err(error: _) 0i32 endmatch endfn\n";
    let result = check_source(SOURCE, source);
    let found = result
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code(),
                diagnostic.primary_span(),
                diagnostic.related().len(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        found,
        [(
            DiagnosticCode::PatternUnreachableArm,
            at(source, "Result.err(error: _)", 0),
            0
        )]
    );
}

/// A tuple of `width` `Bool` components and a `match` on it whose arms are
/// every combination but the last, in counting order.
fn bool_product(width: usize) -> String {
    let combinations = 1usize << width;
    let ty = vec!["Bool"; width].join(" ");
    let mut arms = String::new();
    for combination in 0..combinations.saturating_sub(1) {
        let components = (0..width)
            .map(|bit| {
                if combination >> (width - 1 - bit) & 1 == 1 {
                    "true"
                } else {
                    "false"
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        arms.push_str(&format!(" case {{{components}}} 1i32"));
    }
    format!(
        "fn f takes {{t: {{{ty}}}}} returns r: I32 do return match t{arms} \
         case _ 0i32 endmatch endfn\n"
    )
}

/// The checker gives one coverage question [`COVERAGE_LIMIT`] units of
/// work: a `match` over eight `Bool` components with 256 arms is decided,
/// and one over twelve with 4,096 is `@pattern.unsupportedCoverage` at the
/// `match` and never accepted.
#[test]
fn coverage_has_a_fixed_limit() {
    assert_eq!(COVERAGE_LIMIT, 1 << 22);
    assert_eq!(faults(&bool_product(8)), []);
    let source = bool_product(12);
    let start = source.find("match").unwrap_or(usize::MAX);
    let end = source
        .find("endmatch")
        .map_or(usize::MAX, |end| end + "endmatch".len());
    assert_eq!(
        faults(&source),
        [(
            DiagnosticCode::PatternUnsupportedCoverage,
            vibra_diagnostics::ByteSpan::new(start, end)
        )]
    );
}

/// Patterns of every kind take their value apart at run time.
#[test]
fn patterns_bind_at_run_time() {
    let source = "fn main returns r: I32 do let {a b} be {1i32 2i32} endlet \
                  return I32.add(a b) endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "3i32").as_str()));
    let source = "fn main returns r: I32 do return f({3i32 4i32}) endfn\n\
                  fn f takes {{a b}: {I32 I32}} returns r: I32 do return I32.add(a b) endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "7i32").as_str()));
    let source = "fn main returns r: I32 do return I32.add(f([5i32]) f([])) endfn\n\
                  fn f takes {l: List(I32)} returns r: I32 do \
                  let [head] be l else return 10i32 endlet return head endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "15i32").as_str()));
    let source = "let zero: I32 be 0i32 endlet\n\
                  fn main returns r: I32 do return I32.add(f(0i32) f(5i32)) endfn\n\
                  fn f takes {n: I32} returns r: I32 do \
                  return match n case zero 100i32 case 5i32 1i32 case _ 2i32 endmatch endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "101i32").as_str()));
    let source = "type Celsius of F64 endtype\n\
                  type User of {name: Str id: I32} endtype\n\
                  fn main returns r: I32 do let Celsius(c) be Celsius(1.5f64) endlet \
                  let {x: x} be {x: 2i32 y: 3i32} endlet \
                  return match User(name: \"Ada\" id: 4i32) case User(id: i) I32.add(i x) endmatch endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "6i32").as_str()));
}

/// `try` leaves the callable with the absence and otherwise yields the
/// payload.
#[test]
fn try_propagates() {
    let source = "fn main returns r: I32 do return I32.add(g(Option.some(value: 1i32)) \
                  g(Option.none())) endfn\n\
                  fn g takes {o: Option(I32)} returns r: I32 do \
                  return match f(o) case Option.some(value: v) v case Option.none() 10i32 endmatch endfn\n\
                  fn f takes {o: Option(I32)} returns r: Option(I32) do \
                  let v be try o endlet return Option.some(value: I32.add(v 1i32)) endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "12i32").as_str()));
}

/// Outside a callable there is no result to leave with, so `try` is
/// invalid; the same rule makes it invalid directly in a `test` body.
#[test]
fn try_without_an_enclosing_result_is_invalid() {
    let source = "let o: Option(I32) be Option.none() endlet\n\
                  let x: I32 be try o endlet\n";
    assert_eq!(
        faults(source),
        [(DiagnosticCode::TypeInvalidTry, at(source, "try o", 0))]
    );
}

/// A bare `where` is read and its predicate checked; each construction of
/// the type states an obligation, which no discharge rule establishes yet.
#[test]
fn bare_where_types_are_read() {
    let source = "type Small of I32 where fn takes {v: I32} returns r: Bool do return true endfn endtype\n\
                  fn f takes {s: Small} returns r: I32 do return match s case Small(n) n endmatch endfn\n";
    assert_eq!(faults(source), []);
    let source = format!(
        "{}fn g returns r: Small do return Small(1i32) endfn\n",
        source
            .lines()
            .next()
            .map_or(String::new(), |line| format!("{line}\n"))
    );
    assert_eq!(
        faults(&source),
        [(
            DiagnosticCode::ContractUnproved,
            at(&source, "Small(1i32)", 0)
        )]
    );
}

/// Patterns nest without bound: one nested far deeper than any type is
/// typed without recursion on the host stack, and the outermost level that
/// does not fit is the one fault.
#[test]
fn deeply_nested_patterns_are_checked_iteratively() {
    let depth = 20_000;
    let pattern = format!("{}x{}", "[".repeat(depth), "]".repeat(depth));
    let source = format!(
        "fn f returns r: I32 do let {pattern} be 1i32 endlet return 0i32 endfn\n"
    );
    assert_eq!(
        faults(&source),
        [(DiagnosticCode::TypeMismatch, at(&source, &pattern, 0))]
    );
}
