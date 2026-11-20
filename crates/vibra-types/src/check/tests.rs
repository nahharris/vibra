//! Unit tests of the checker. Expectations come from the type chapter.

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_interp::MemoryBudget;

use super::{check_source, interpret};

mod bodies;
mod callables;

const SOURCE: &str = "main.vib";

/// The canonical result of running `source`, or the diagnostics.
fn run(source: &str) -> Result<String, String> {
    let outcome = interpret(SOURCE, source, MemoryBudget::DEFAULT);
    match outcome.run {
        Some(Ok(execution)) => Ok(execution.canonical.clone()),
        Some(Err(error)) => Err(format!("{error:?}")),
        None => Err(format!("{:#?}", outcome.check.diagnostics())),
    }
}

/// The canonical `@value.v2` document of a scalar.
fn value(ty: &str, text: &str) -> String {
    format!("{{format: @value.v2 typeInfo: @{ty} value: {text}}}")
}

/// The code and span of every diagnostic of `source`.
fn faults(source: &str) -> Vec<(DiagnosticCode, ByteSpan)> {
    check_source(SOURCE, source)
        .diagnostics()
        .iter()
        .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span()))
        .collect()
}

/// The span of the `nth` occurrence of `needle` in `source`.
fn at(source: &str, needle: &str, nth: usize) -> ByteSpan {
    let start = source
        .match_indices(needle)
        .nth(nth)
        .map_or(usize::MAX, |(start, _)| start);
    ByteSpan::new(start, start.saturating_add(needle.len()))
}

#[test]
fn the_standard_library_checks_clean() {
    let result = check_source(SOURCE, "fn main returns r: U8 do return 7u8 endfn\n");
    assert!(
        !result.has_errors(),
        "{:#?}\nnotes: {:#?}",
        result.diagnostics(),
        result.notes()
    );
    let names = result
        .functions()
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    for member in ["not", "and", "or", "xor"] {
        assert!(
            names.contains(&format!("std.bool.Bool.{member}").as_str()),
            "{names:?}"
        );
    }
    assert!(!result.notes().is_empty());
}

#[test]
fn literal_suffix_and_expected_type() {
    assert_eq!(
        run("fn main returns r: U8 do return 7u8 endfn\n").as_deref(),
        Ok(value("u8", "7u8").as_str())
    );
    assert_eq!(
        run("fn main returns r: I64 do return -5 endfn\n").as_deref(),
        Ok(value("i64", "-5i64").as_str())
    );
}

#[test]
fn ambiguous_literal_has_one_note() {
    let source = "fn main returns r: I32 do let a be 1 endlet return a endfn\n";
    let result = check_source(SOURCE, source);
    let diagnostics = result.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    let Some(diagnostic) = diagnostics.first() else {
        return;
    };
    assert_eq!(diagnostic.code(), DiagnosticCode::TypeAmbiguousInference);
    assert_eq!(diagnostic.primary_span(), at(source, "1", 0));
    assert_eq!(diagnostic.notes().len(), 1);
}

#[test]
fn out_of_range_literals() {
    let source = "fn main returns r: U8 do return -1u8 endfn\n";
    assert_eq!(
        faults(source),
        [(DiagnosticCode::TypeNumericOutOfRange, at(source, "-1u8", 0))]
    );
    let source = "fn main returns r: F32 do return 1e39f32 endfn\n";
    assert_eq!(
        faults(source),
        [(
            DiagnosticCode::TypeNumericOutOfRange,
            at(source, "1e39f32", 0)
        )]
    );
}

#[test]
fn no_promotion() {
    let source =
        "fn add takes {a: I32 b: I64} returns r: I64 do return I64.add(a b) endfn\n";
    assert_eq!(
        faults(source),
        [(DiagnosticCode::TypeArgumentMismatch, at(source, "a", 4))]
    );
}

#[test]
fn function_values_and_callee_expressions() {
    let source = "fn inc takes {x: I32} returns r: I32 do return I32.add(x 1i32) endfn\n\
                  fn apply takes {f: Fn({I32} I32) v: I32} returns r: I32 do return f(v) endfn\n\
                  fn main returns r: I32 do return apply(inc 41i32) endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "42i32").as_str()));
    let source = "fn main returns r: I32 do return pick(true)(1i32) endfn\n\
                  fn inc takes {x: I32} returns r: I32 do return I32.add(x 1i32) endfn\n\
                  fn dec takes {x: I32} returns r: I32 do return I32.sub(x 1i32) endfn\n\
                  fn pick takes {b: Bool} returns f: Fn({I32} I32) do \
                  if b then return inc else return dec endif endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "2i32").as_str()));
}

#[test]
fn control_flow_values() {
    let source = "fn main returns r: I32 do return f(false) endfn\n\
                  fn f takes {b: Bool} returns r: I32 do if b then return 1i32 endif return 2i32 endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "2i32").as_str()));
    let source = "fn main returns r: I32 do return f(false) endfn\n\
                  fn f takes {b: Bool} returns r: I32 do \
                  return match b case true 1i32 case false 2i32 endmatch endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "2i32").as_str()));
    let source = "fn main returns r: I32 do \
                  let a be do let x be 1i32 endlet I32.add(x x) enddo endlet return a endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "2i32").as_str()));
    let source = "fn main returns r: Void do let v be do let x be 1i32 endlet enddo endlet return v endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("void", "void").as_str()));
    let source = "let a: U8 be b endlet\nlet b: U8 be 1u8 endlet\n\
                  fn main returns r: U8 do return a endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("u8", "1u8").as_str()));
}

#[test]
fn control_flow_faults() {
    let source =
        "fn f takes {b: Bool} returns r: I32 do if b then return 1i32 endif endfn\n";
    assert_eq!(
        faults(source).first().map(|fault| fault.0),
        Some(DiagnosticCode::TypeMissingReturn)
    );
    let source = "fn f returns r: I32 do return 1i32 return 2i32 endfn\n";
    assert_eq!(
        faults(source),
        [(
            DiagnosticCode::TypeUnreachableCode,
            at(source, "return 2i32", 0)
        )]
    );
    let source = "fn f returns r: I32 do let a be return 1i32 endlet return a endfn\n";
    assert_eq!(
        faults(source),
        [(
            DiagnosticCode::TypeUnreachableCode,
            at(source, "return 1i32", 0)
        )]
    );
    let source = "fn f takes {b: Bool} returns r: I32 do let a be if b then 1i32 endif endlet return a endfn\n";
    assert_eq!(
        faults(source),
        [(
            DiagnosticCode::TypeMismatch,
            at(source, "if b then 1i32 endif", 0)
        )]
    );
    let source = "fn f returns r: I32 do return \"x\" endfn\n";
    assert_eq!(
        faults(source),
        [(DiagnosticCode::TypeMismatch, at(source, "\"x\"", 0))]
    );
    let source =
        "let a: U8 be helper() endlet\nfn helper returns r: U8 do return a endfn\n";
    assert_eq!(
        faults(source),
        [(
            DiagnosticCode::TypeInitializerCycle,
            at(source, "let a: U8 be helper() endlet", 0)
        )]
    );
    let source = "let a: U8 be return 1u8 endlet\n";
    assert_eq!(
        faults(source),
        [(
            DiagnosticCode::TypeInvalidReturn,
            at(source, "return 1u8", 0)
        )]
    );
    let source = "fn f returns r: I32 do let n be 1i32 endlet return n(2i32) endfn\n";
    assert_eq!(
        faults(source),
        [(
            DiagnosticCode::TypeNotApplicable,
            ByteSpan::new(at(source, "n(", 0).start(), at(source, "n(", 0).start() + 1)
        )]
    );
}

#[test]
fn coverage() {
    let source = "fn f takes {o: Ordering} returns r: I32 do \
                  return match o case Ordering.less() 1i32 case Ordering.equal() 2i32 endmatch endfn\n";
    let result = check_source(SOURCE, source);
    let diagnostics = result.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    let Some(diagnostic) = diagnostics.first() else {
        return;
    };
    assert_eq!(diagnostic.code(), DiagnosticCode::PatternNonExhaustive);
    assert_eq!(diagnostic.notes(), ["Ordering.greater()"]);
    let source = "fn f takes {o: Ordering} returns r: I32 do \
                  return match o case _ 0i32 case Ordering.less() 1i32 endmatch endfn\n";
    assert_eq!(
        faults(source),
        [(
            DiagnosticCode::PatternUnreachableArm,
            at(source, "Ordering.less()", 0)
        )]
    );
}

#[test]
fn tail_calls_run_in_constant_depth() {
    let source = "fn count takes {n: I32 acc: I32} returns r: I32 do \
                  if Equatable.equal(n 0i32) then return acc \
                  else return count(I32.sub(n 1i32) I32.add(acc 1i32)) endif endfn\n\
                  fn main returns r: I32 do return count(100000i32 0i32) endfn\n";
    let outcome = interpret(SOURCE, source, MemoryBudget::DEFAULT);
    assert!(
        matches!(outcome.run, Some(Ok(_))),
        "{:?} {:#?}",
        outcome.run,
        outcome.check.diagnostics()
    );
    let Some(Ok(execution)) = outcome.run else {
        return;
    };
    assert_eq!(execution.canonical, value("i32", "100000i32"));
    assert_eq!(execution.instrumentation.max_activation_depth, 1);
}

/// The member functions of `@std.builtin`'s `Dict` and `List` check clean:
/// each is lowered and none is skipped with a note.
#[test]
fn dict_and_list_members_check_clean() {
    let result = check_source(SOURCE, "fn main returns r: U8 do return 7u8 endfn\n");
    assert!(!result.has_errors(), "{:#?}", result.diagnostics());
    let names = result
        .functions()
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    for member in [
        "std.builtin.Dict.entries",
        "std.builtin.List.length",
        "std.builtin.List.append",
        "std.builtin.List.concat",
        "std.builtin.List.slice",
    ] {
        assert!(names.contains(&member), "{member} in {names:?}");
    }
    for note in result.notes() {
        assert!(
            !note.contains("`std.builtin.Dict") && !note.contains("`std.builtin.List"),
            "{note}"
        );
    }
    assert_eq!(
        run("fn main returns r: U64 do return List.length([1i32 2i32]) endfn\n")
            .as_deref(),
        Ok(value("u64", "2u64").as_str())
    );
}

/// The lowered body of the function `name` of an accepted `source`, as its
/// debug text.
fn lowered(source: &str, name: &str) -> String {
    let result = check_source(SOURCE, source);
    assert!(!result.has_errors(), "{:#?}", result.diagnostics());
    let set = result
        .module_set
        .as_ref()
        .map(|set| set.functions().to_vec());
    let function = set
        .into_iter()
        .flatten()
        .find(|function| function.name.ends_with(name));
    format!("{:?}", function.map(|function| function.body))
}

/// A record value and a constructor are lowered with their operands in
/// written order, which the interpreter then runs in that order
/// (`vibra-interp`'s `products_evaluate_operands_in_written_order`): the
/// field `b`, written first, comes first.
#[test]
fn products_lower_operands_in_written_order() {
    let source =
        "fn main returns r: {a: I32 b: I32} do return {b: 2i32 a: 1i32} endfn\n";
    let body = lowered(source, "main");
    assert!(body.contains("Record([(\"b\", "), "{body}");
    let source = "type P of {a: I32 b: I32} endtype\n\
                  fn main returns r: P do return P(b: 2i32 a: 1i32) endfn\n";
    let body = lowered(source, "main");
    assert!(body.contains("Record([(\"b\", "), "{body}");
    let source = "type S case c {a: I32 b: I32} endtype\n\
                  fn main returns r: S do return S.c(b: 2i32 a: 1i32) endfn\n";
    let body = lowered(source, "main");
    assert!(body.contains("fields: [(\"b\", "), "{body}");
}

/// The step 15 row: the operands of a tuple of calls are lowered in
/// written order, and the value lists them in that order.
#[test]
fn tuple_of_calls_runs_in_written_order() {
    let source = "fn main returns r: {I32 I32} do return {first() second()} endfn\n\
                  fn first returns r: I32 do return 1i32 endfn\n\
                  fn second returns r: I32 do return 2i32 endfn\n";
    assert_eq!(
        run(source).as_deref(),
        Ok(
            "{format: @value.v2 typeInfo: {kind: @tuple arguments: [@i32 @i32]} \
            value: {kind: @tuple values: [1i32 2i32]}}"
        )
    );
}

/// A constructor binds labels exactly when its shape has field names
/// (`docs/spec/02-type-system.md`, "Application").
#[test]
fn constructor_labels_follow_the_shape() {
    let user = "type User of {name: Str id: U64} endtype\n";
    let source =
        format!("{user}fn f returns r: User do return User(\"Ada\" 1u64) endfn\n");
    assert_eq!(
        faults(&source),
        [(
            DiagnosticCode::TypeArgumentMismatch,
            at(&source, "User(\"Ada\" 1u64)", 0)
        )]
    );
    let source = "type Pair of {I32 Str} endtype\n\
                  fn f returns r: Pair do return Pair(a: 1i32 b: \"x\") endfn\n";
    assert_eq!(
        faults(source),
        [
            (
                DiagnosticCode::TypeArgumentMismatch,
                at(source, "a: 1i32", 0)
            ),
            (
                DiagnosticCode::TypeArgumentMismatch,
                at(source, "b: \"x\"", 0)
            ),
        ]
    );
}

/// A literal argument that does not fit its slot is an argument mismatch,
/// like any other argument.
#[test]
fn literal_argument_is_an_argument_mismatch() {
    let source = "fn inc takes {x: I32} returns r: I32 do return x endfn\n\
                  fn f returns r: I32 do return inc(\"x\") endfn\n";
    assert_eq!(
        faults(source),
        [(DiagnosticCode::TypeArgumentMismatch, at(source, "\"x\"", 0))]
    );
}

/// A type argument is inferred from the expected type when no operand
/// fixes it, and a generic function value takes its arguments from the
/// written type it is bound to.
#[test]
fn type_arguments_from_the_expected_type() {
    let source = "fn main returns r: Option(I32) do return none() endfn\n\
                  fn none(T) returns r: Option(T) do return Option.none() endfn\n";
    assert!(run(source).is_ok(), "{:?}", run(source));
    let source = "fn main returns r: I32 do let f: Fn({I32} I32) be id endlet return f(4i32) endfn\n\
                  fn id(T) takes {x: T} returns r: T do return x endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "4i32").as_str()));
    let source = "fn id(T) takes {x: T} returns r: T do return x endfn\n\
                  fn main returns r: I32 do let f be id endlet return 1i32 endfn\n";
    assert_eq!(
        faults(source),
        [(DiagnosticCode::TypeAmbiguousInference, at(source, "id", 1))]
    );
}

/// The representation is hidden from other modules only: the declaring
/// module builds and reads a private representation.
#[test]
fn private_representation_is_visible_in_its_module() {
    let source = "type Token of {id: U64} visibility @public representation @private endtype\n\
                  fn main returns r: U64 do let t be Token(id: 3u64) endlet return t.id endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("u64", "3u64").as_str()));
}

/// Recursion through a `Dict` or a callable is finite; a parameter held in
/// place is expanded, so holding a type in a `Box` of itself repeats.
#[test]
fn finite_size_through_collections_and_callables() {
    for source in [
        "type T of {kids: Dict(Str T)} endtype\n",
        "type T of {next: Fn({} T)} endtype\n",
        "type Box(V) of {item: V} endtype\ntype T of {kids: List(Box(T))} endtype\n",
    ] {
        assert_eq!(faults(source), [], "{source}");
    }
    let source = "type Box(V) of {item: V} endtype\ntype T of {b: Box(T)} endtype\n";
    assert_eq!(
        faults(source),
        [(
            DiagnosticCode::TypeInfiniteSize,
            at(source, "type T of {b: Box(T)} endtype", 0)
        )]
    );
}

/// A callable's own type is one level more than its deepest parameter, so a
/// parameter type of depth 64 makes the callable 65 deep; the same type is
/// accepted on a local.
#[test]
fn callable_depth_counts_its_parameters() {
    let deepest = format!("{}I32{}", "List(".repeat(63), ")".repeat(63));
    let source =
        format!("fn f takes {{x: {deepest}}} returns r: I32 do return 1i32 endfn\n");
    assert_eq!(
        faults(&source),
        [(DiagnosticCode::TypeDepthExceeded, at(&source, &deepest, 0))]
    );
    let source = format!(
        "fn f returns r: I32 do let x: {deepest} be [] endlet return 1i32 endfn\n"
    );
    assert_eq!(faults(&source), []);
}
