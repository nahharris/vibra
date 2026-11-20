//! Callables: labelled parameters and their constant defaults, anonymous
//! `fn` and its captures, `Fn` types with labels, function values,
//! constructors that are not values, joint generic inference, generic
//! anonymous callables, and tail calls. Expectations come from the source
//! and type chapters.

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_ir::{CallTarget, Expr, FunctionBody};

use super::{SOURCE, at, faults, run, value};
use crate::check::check_source;

const PAD: &str = "fn pad takes {s: Str} labelled {width: U64 be 4u64} returns r: U64 \
                   do return width endfn\n";

#[test]
fn a_missing_labelled_argument_takes_its_default() {
    let source = format!("fn main returns r: U64 do return pad(\"x\") endfn\n{PAD}");
    assert_eq!(run(&source).as_deref(), Ok(value("u64", "4u64").as_str()));
    let source =
        format!("fn main returns r: U64 do return pad(\"x\" width: 2u64) endfn\n{PAD}");
    assert_eq!(run(&source).as_deref(), Ok(value("u64", "2u64").as_str()));
}

#[test]
fn a_labelled_argument_binds_in_any_position() {
    let source =
        format!("fn main returns r: U64 do return pad(width: 2u64 \"x\") endfn\n{PAD}");
    assert_eq!(run(&source).as_deref(), Ok(value("u64", "2u64").as_str()));
}

#[test]
fn a_labelled_argument_written_first_keeps_its_parameter() {
    // The arguments are bound in written order before the call, and each
    // still reaches its own parameter.
    let source = "fn main returns r: I32 do return f(n: 5i32 2i32) endfn\n\
                  fn f takes {m: I32} labelled {n: I32 be 0i32} returns r: I32 \
                  do return I32.sub(m n) endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "-3i32").as_str()));
}

/// The functions `body` calls directly, in the order an execution reaches
/// them: a `let` runs its value before its body, a sequence runs in order,
/// and a call runs its callee, then its positional arguments, then its
/// labelled arguments in written order, then the callee itself, in a tail
/// position or not
/// (`vibra-interp`'s `evaluation_order_is_callee_positional_then_labelled_as_written`).
fn calls_in_order(body: &Expr, names: &[String], order: &mut Vec<String>) {
    match body {
        Expr::Let { value, body, .. } => {
            calls_in_order(&value.expr, names, order);
            calls_in_order(&body.expr, names, order);
        }
        Expr::Return(value) => calls_in_order(&value.expr, names, order),
        Expr::Sequence(statements) => {
            for statement in statements {
                calls_in_order(&statement.expr, names, order);
            }
        }
        Expr::Call { target, arguments } | Expr::TailCall { target, arguments } => {
            if let CallTarget::Value(callee) = target {
                calls_in_order(&callee.expr, names, order);
            }
            for argument in &arguments.positional {
                calls_in_order(&argument.expr, names, order);
            }
            for (_, argument) in &arguments.labelled {
                calls_in_order(&argument.expr, names, order);
            }
            if let CallTarget::Direct { function, .. } = target {
                order.extend(names.get(*function).cloned());
            }
        }
        _ => {}
    }
}

/// A call through a computed callee whose labelled argument is written
/// before its positional one: the callee runs first, then each operand in
/// written order (`docs/spec/06-runtime.md`, "Evaluation"), and each still
/// reaches its own parameter.
#[test]
fn a_computed_callee_runs_before_reordered_operands() {
    let source = "fn main returns r: U8 do return make()(b: arg() pos()) endfn\n\
                  fn make returns f: Fn({U8 b: U8} U8) do \
                  return fn takes {a: U8} labelled {b: U8 be 0u8} returns r: U8 \
                  do return U8.sub(a b) endfn endfn\n\
                  fn arg returns r: U8 do return 1u8 endfn\n\
                  fn pos returns r: U8 do return 10u8 endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("u8", "9u8").as_str()));
    let result = check_source(SOURCE, source);
    let functions = result
        .module_set
        .as_ref()
        .map(|set| set.functions().to_vec())
        .unwrap_or_default();
    let names = functions
        .iter()
        .map(|function| function.name.rsplit('.').next().unwrap_or("").to_owned())
        .collect::<Vec<_>>();
    let main = functions
        .iter()
        .find(|function| function.name.ends_with(".main"));
    let mut order = Vec::new();
    if let Some(FunctionBody::Expr(body)) = main.map(|main| &main.body) {
        calls_in_order(&body.expr, &names, &mut order);
    }
    assert_eq!(order, ["make", "arg", "pos"]);
}

#[test]
fn a_duplicate_or_unknown_label_does_not_fit() {
    let source = format!(
        "fn main returns r: U64 do return pad(\"x\" width: 1u64 width: 2u64) endfn\n{PAD}"
    );
    assert_eq!(
        faults(&source),
        [(
            DiagnosticCode::TypeArgumentMismatch,
            at(&source, "width: 2u64", 0)
        )]
    );
    let source =
        format!("fn main returns r: U64 do return pad(\"x\" depth: 1u64) endfn\n{PAD}");
    assert_eq!(
        faults(&source),
        [(
            DiagnosticCode::TypeArgumentMismatch,
            at(&source, "depth: 1u64", 0)
        )]
    );
}

#[test]
fn a_default_is_a_constant_expression() {
    let source = "fn main returns r: U64 do return pad(\"x\") endfn\n\
                  fn compute returns r: U64 do return 4u64 endfn\n\
                  fn pad takes {s: Str} labelled {width: U64 be compute()} returns r: U64 \
                  do return width endfn\n";
    assert_eq!(
        faults(source),
        [(DiagnosticCode::TypeNotConstant, at(source, "compute()", 0))]
    );
    let source = "fn main returns r: U64 do return pad(\"x\") endfn\n\
                  let four: U64 be 4u64 endlet\n\
                  fn pad takes {s: Str} labelled {width: U64 be four} returns r: U64 \
                  do return width endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("u64", "4u64").as_str()));
    let source = "fn main returns r: I32 do return size() endfn\n\
                  type Shape case empty case dot {x: I32} endtype\n\
                  fn size labelled {mode: Shape be Shape.dot(x: 3i32)} returns r: I32 do \
                  return match mode case Shape.empty() 0i32 \
                  case Shape.dot(x: x) x endmatch endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "3i32").as_str()));
}

#[test]
fn a_fn_type_has_its_labels() {
    let source = format!(
        "fn main returns r: U64 do let f: Fn({{Str width: U64}} U64) be pad endlet \
         return f(\"x\") endfn\n{PAD}"
    );
    assert_eq!(run(&source).as_deref(), Ok(value("u64", "4u64").as_str()));
    let source = format!(
        "fn main returns r: U64 do let f: Fn({{Str}} U64) be pad endlet \
         return f(\"x\") endfn\n{PAD}"
    );
    assert_eq!(
        faults(&source),
        [(DiagnosticCode::TypeMismatch, at(&source, "pad", 0))]
    );
}

#[test]
fn an_anonymous_fn_captures_by_value() {
    let source = "fn main returns r: I32 do let base be 10i32 endlet \
                  let add be fn takes {x: I32} returns r: I32 do return I32.add(x base) endfn \
                  endlet return add(1i32) endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "11i32").as_str()));
    // A capture through two anonymous callables.
    let source = "fn main returns r: I32 do let base be 10i32 endlet \
                  let outer be fn returns g: Fn({I32} I32) do \
                  return fn takes {x: I32} returns r: I32 do return I32.add(x base) endfn \
                  endfn endlet return outer()(2i32) endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "12i32").as_str()));
}

#[test]
fn an_anonymous_fn_cannot_name_itself() {
    let source = "fn main returns r: I32 do \
                  let f be fn takes {x: I32} returns r: I32 do return f(x) endfn endlet \
                  return f(1i32) endfn\n";
    let start = at(source, "f(x)", 0).start();
    assert_eq!(
        faults(source),
        [(
            DiagnosticCode::NameUnknownSymbol,
            ByteSpan::new(start, start + 1)
        )]
    );
}

#[test]
fn a_generic_anonymous_fn_bound_by_let_stays_generic() {
    let source = "fn main returns r: {I32 Str} do \
                  let id be fn(T) takes {x: T} returns r: T do return x endfn endlet \
                  return {id(1i32) id(\"x\")} endfn\n";
    let str_type = "{kind: @declared path: @std.str.Str seed: \
                    \"sha256:29dc059689d38a6e1e8b843c7c518b7bfe2d16d9f596ef242d758b9b970b1d8a\" \
                    arguments: []}";
    let expected = format!(
        "{{format: @value.v2 typeInfo: {{kind: @tuple arguments: [@i32 {str_type}]}} \
         value: {{kind: @tuple values: [1i32 \"x\"]}}}}"
    );
    assert_eq!(run(source).as_deref(), Ok(expected.as_str()));
}

#[test]
fn a_generic_anonymous_fn_elsewhere_takes_its_expected_type() {
    let source = "fn main returns r: I32 do \
                  return apply(fn(T) takes {x: T} returns r: T do return x endfn 1i32) endfn\n\
                  fn apply takes {f: Fn({I32} I32) v: I32} returns r: I32 do return f(v) endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "1i32").as_str()));
    let source = "fn main returns r: I32 do \
                  let _ be fn(T) takes {x: T} returns r: T do return x endfn endlet \
                  return 1i32 endfn\n";
    let start = source.find("fn(T)").unwrap_or(0);
    let end = source
        .find("endfn endlet")
        .map_or(0, |end| end + "endfn".len());
    assert_eq!(
        faults(source),
        [(
            DiagnosticCode::TypeAmbiguousInference,
            ByteSpan::new(start, end)
        )]
    );
}

#[test]
fn a_constructor_is_not_a_function_value() {
    let source = "type Celsius of F64 endtype\n\
                  fn main returns r: F64 do let f: Fn({F64} Celsius) be Celsius endlet \
                  return 1.0f64 endfn\n";
    assert_eq!(
        faults(source),
        [(DiagnosticCode::TypeMismatch, at(source, "Celsius", 2))]
    );
    let source = "type Celsius of F64 endtype\n\
                  fn main returns r: Celsius do \
                  let f: Fn({F64} Celsius) be fn takes {v: F64} returns c: Celsius do \
                  return Celsius(v) endfn endlet return f(1.0f64) endfn\n";
    assert!(run(source).is_ok(), "{:?}", run(source));
}

#[test]
fn a_member_is_a_function_value() {
    let source = "type U of {n: List(I32)} \
                  fn len takes {u: Self} returns r: U64 do return List.length(u.n) endfn endtype\n\
                  fn main returns r: U64 do let f be U.len endlet \
                  return f(U(n: [1i32 2i32])) endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("u64", "2u64").as_str()));
}

#[test]
fn inference_is_joint_over_every_operand() {
    let pick = "fn pick(T) takes {a: T b: T} returns r: T do return a endfn\n";
    let source = format!(
        "fn main returns r: I64 do let n: I64 be pick(1 2i64) endlet return n endfn\n{pick}"
    );
    assert_eq!(run(&source).as_deref(), Ok(value("i64", "1i64").as_str()));
    let source = format!("fn main returns r: I64 do return pick(1 2i64) endfn\n{pick}");
    assert_eq!(run(&source).as_deref(), Ok(value("i64", "1i64").as_str()));
    let source = format!(
        "fn main returns r: I32 do let n be pick(1i32 \"x\") endlet return 1i32 endfn\n{pick}"
    );
    assert_eq!(
        faults(&source),
        [(
            DiagnosticCode::TypeArgumentMismatch,
            at(&source, "\"x\"", 0)
        )]
    );
}

#[test]
fn a_tail_call_through_a_value_or_a_closure() {
    let source = "fn main returns r: U64 do return count(100000u64) endfn\n\
                  fn count takes {n: U64} returns r: U64 do \
                  let step be fn takes {m: U64} returns r: U64 do \
                  if Equatable.equal(m 0u64) then return 0u64 endif return count(U64.sub(m 1u64)) endfn \
                  endlet return step(n) endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("u64", "0u64").as_str()));
}

#[test]
fn an_anonymous_fn_has_labelled_defaults() {
    let source = "fn main returns r: I32 do \
                  let f be fn takes {x: I32} labelled {d: I32 be 1i32} returns r: I32 do \
                  return I32.add(x d) endfn endlet \
                  return I32.add(f(1i32) f(1i32 d: 5i32)) endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "8i32").as_str()));
}

#[test]
fn a_generic_anonymous_fn_captures_and_nests() {
    let source = "fn main returns r: I32 do let base be 1i32 endlet \
                  let k be fn(T) takes {x: T} returns r: {T I32} do return {x base} endfn \
                  endlet let {_ n} be k(\"a\") endlet return n endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "1i32").as_str()));
    let source = "fn main returns r: I32 do return wrap(3i32) endfn\n\
                  fn wrap(T) takes {v: T} returns r: T do \
                  let id be fn(U) takes {x: U} returns r: U do return x endfn endlet \
                  return id(v) endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "3i32").as_str()));
}

#[test]
fn a_result_type_argument_comes_from_the_expected_type() {
    let source = "fn main returns r: I32 do let o: Option(I32) be none() endlet \
                  return match o case Option.some(value: v) v case Option.none() 7i32 endmatch \
                  endfn\n\
                  fn none(T) returns r: Option(T) do return Option.none() endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "7i32").as_str()));
}

#[test]
fn a_never_typed_call_leaves() {
    // A call whose result is `Never` ends its path, as a branch and as the
    // last statement of a body.
    let source = "fn main returns r: I32 do \
                  if true then return 1i32 else spin(0i32) endif endfn\n\
                  fn spin takes {n: I32} returns r: Never do spin(n) endfn\n\
                  fn halt returns r: I32 do spin(0i32) endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "1i32").as_str()));
}
