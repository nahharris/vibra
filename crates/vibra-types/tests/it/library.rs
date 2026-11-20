//! The pure standard library through the checker and the interpreter: one
//! test per member of "Character, sequence and dictionary operations" of
//! `docs/spec/06-runtime.md`, named after it, and the measured properties of
//! `Ordered`'s default members and `Bool`'s operations. Every expected value
//! is written from the specification.

use vibra_interp::{Execution, MemoryBudget};
use vibra_ir::Value;

/// Checks and runs `source`, which must check clean and finish.
fn run(source: &str) -> Execution {
    let outcome = vibra_types::interpret("main.vib", source, MemoryBudget::DEFAULT);
    assert!(
        outcome.check.diagnostics().is_empty(),
        "{:?}",
        outcome.check.diagnostics()
    );
    outcome
        .run
        .expect("the source has an entry")
        .expect("the run finishes")
}

/// The value `source`'s entry returns.
fn value(source: &str) -> Value {
    run(source).value.value().clone()
}

/// How many checked nodes the run of `source` evaluated.
fn reductions(source: &str) -> u64 {
    run(source).instrumentation.reduction_count
}

fn case(name: &str, fields: Vec<(String, Value)>) -> Value {
    Value::Variant {
        case: name.to_owned(),
        fields,
    }
}

fn some(value: Value) -> Value {
    case("some", vec![("value".to_owned(), value)])
}

fn none() -> Value {
    case("none", Vec::new())
}

fn ok(value: Value) -> Value {
    case("ok", vec![("value".to_owned(), value)])
}

fn err(error: &str) -> Value {
    case("err", vec![("error".to_owned(), case(error, Vec::new()))])
}

fn boolean(holds: bool) -> Value {
    case(if holds { "true" } else { "false" }, Vec::new())
}

fn order(name: &str) -> Value {
    case(name, Vec::new())
}

fn text(text: &str) -> Value {
    Value::Str(text.to_owned())
}

fn chars(text: &str) -> Value {
    Value::List(text.chars().map(Value::Char).collect())
}

fn tuple(values: Vec<Value>) -> Value {
    Value::Tuple(values)
}

fn bools(values: &[bool]) -> Value {
    tuple(values.iter().map(|holds| boolean(*holds)).collect())
}

fn i32s(values: &[i32]) -> Value {
    Value::List(values.iter().map(|n| Value::I32(*n)).collect())
}

#[test]
fn char_to_u32() {
    assert_eq!(
        value(
            "fn main returns r: {U32 U32} do return {Char.toU32('A') Char.toU32('é')} endfn\n"
        ),
        tuple(vec![Value::U32(65), Value::U32(233)])
    );
}

#[test]
fn char_from_u32() {
    let source = "fn main returns r: {Option(Char) Option(Char) Option(Char)} do \
                  return {Char.fromU32(65u32) Char.fromU32(55296u32) Char.fromU32(1114112u32)} \
                  endfn\n";
    assert_eq!(
        value(source),
        tuple(vec![some(Value::Char('A')), none(), none()])
    );
}

#[test]
fn str_length() {
    let source = "fn main returns r: {U64 U64} do \
                  return {Str.length(\"héllo\") Str.length(\"\")} endfn\n";
    assert_eq!(value(source), tuple(vec![Value::U64(5), Value::U64(0)]));
}

#[test]
fn str_concat() {
    let source = "fn main returns r: {Str Str} do \
                  return {Str.concat(\"hé\" \"llo\") Str.concat(\"\" \"a\")} endfn\n";
    assert_eq!(value(source), tuple(vec![text("héllo"), text("a")]));
}

#[test]
fn str_slice() {
    let source = "fn main returns r: {Option(Str) Option(Str) Option(Str) Option(Str)} do \
                  return {Str.slice(\"héllo\" 1u64 3u64) Str.slice(\"héllo\" 2u64 2u64) \
                  Str.slice(\"héllo\" 0u64 6u64) Str.slice(\"héllo\" 3u64 1u64)} endfn\n";
    assert_eq!(
        value(source),
        tuple(vec![some(text("él")), some(text("")), none(), none()])
    );
}

#[test]
fn char_is_ascii_digit() {
    let source = "fn main returns r: {Bool Bool Bool Bool} do \
                  return {Char.isAsciiDigit('0') Char.isAsciiDigit('9') \
                  Char.isAsciiDigit('/') Char.isAsciiDigit(':')} endfn\n";
    assert_eq!(value(source), bools(&[true, true, false, false]));
}

#[test]
fn char_is_ascii_alphabetic() {
    let source = "fn main returns r: {Bool Bool Bool Bool Bool Bool Bool Bool} do \
                  return {Char.isAsciiAlphabetic('a') Char.isAsciiAlphabetic('z') \
                  Char.isAsciiAlphabetic('A') Char.isAsciiAlphabetic('Z') \
                  Char.isAsciiAlphabetic('@') Char.isAsciiAlphabetic('[') \
                  Char.isAsciiAlphabetic('`') Char.isAsciiAlphabetic('{')} endfn\n";
    assert_eq!(
        value(source),
        bools(&[true, true, true, true, false, false, false, false])
    );
}

#[test]
fn char_is_ascii_whitespace() {
    let source = "fn main returns r: {Bool Bool Bool Bool Bool Bool} do \
                  return {Char.isAsciiWhitespace(' ') Char.isAsciiWhitespace('\\t') \
                  Char.isAsciiWhitespace('\\n') Char.isAsciiWhitespace('\\r') \
                  Char.isAsciiWhitespace('\\u{B}') Char.isAsciiWhitespace('\\u{A0}')} endfn\n";
    assert_eq!(
        value(source),
        bools(&[true, true, true, true, false, false])
    );
}

#[test]
fn str_trim() {
    let source = "fn main returns r: {Str Str Str Str} do \
                  return {Str.trim(\"\\t a b \\r\\n\") Str.trim(\"\") Str.trim(\"a\") \
                  Str.trim(\"\\u{B}a\\u{A0}\")} endfn\n";
    assert_eq!(
        value(source),
        tuple(vec![text("a b"), text(""), text("a"), text("\u{B}a\u{A0}")])
    );
}

#[test]
fn str_to_utf8() {
    let source = "fn main returns r: {Bytes Bytes} do \
                  return {Str.toUtf8(\"é\") Str.toUtf8(\"\")} endfn\n";
    assert_eq!(
        value(source),
        tuple(vec![
            Value::Bytes(vec![0xC3, 0xA9]),
            Value::Bytes(Vec::new())
        ])
    );
}

#[test]
fn str_from_utf8() {
    let source = "use @std.core.ConversionError enduse\n\
                  fn main returns r: {Result(Str ConversionError) Result(Str ConversionError) \
                  Result(Str ConversionError)} do \
                  return {Str.fromUtf8(Bytes([195u8 169u8])) Str.fromUtf8(Bytes([237u8 160u8 128u8])) \
                  Str.fromUtf8(Bytes([195u8]))} endfn\n";
    assert_eq!(
        value(source),
        tuple(vec![
            ok(text("é")),
            err("invalidFormat"),
            err("invalidFormat")
        ])
    );
}

#[test]
fn str_constructor_and_pattern() {
    let source = "fn main returns r: {Str List(Char)} do \
                  let Str(scalars) be \"hé\" endlet \
                  return {Str(['a' 'é']) scalars} endfn\n";
    assert_eq!(value(source), tuple(vec![text("aé"), chars("hé")]));
}

#[test]
fn str_equatable_and_ordered() {
    let source = "fn main returns r: {Bool Bool Ordering Ordering Ordering Ordering} do \
                  return {Equatable.equal(\"ab\" \"ab\") Equatable.equal(\"ab\" \"a\") \
                  Ordered.compare(\"a\" \"ab\") Ordered.compare(\"b\" \"ab\") \
                  Ordered.compare(\"é\" \"z\") Ordered.compare(\"\" \"\")} endfn\n";
    assert_eq!(
        value(source),
        tuple(vec![
            boolean(true),
            boolean(false),
            order("less"),
            order("greater"),
            order("greater"),
            order("equal"),
        ])
    );
}

#[test]
fn bytes_length() {
    let source = "fn main returns r: {U64 U64} do \
                  return {Bytes.length(Bytes([1u8 2u8 3u8])) Bytes.length(Bytes([]))} endfn\n";
    assert_eq!(value(source), tuple(vec![Value::U64(3), Value::U64(0)]));
}

#[test]
fn bytes_concat() {
    let source = "fn main returns r: Bytes do \
                  return Bytes.concat(Bytes([1u8]) Bytes([2u8 3u8])) endfn\n";
    assert_eq!(value(source), Value::Bytes(vec![1, 2, 3]));
}

#[test]
fn bytes_slice() {
    let source = "fn main returns r: {Option(Bytes) Option(Bytes) Option(Bytes)} do \
                  let b be Bytes([1u8 2u8 3u8]) endlet \
                  return {Bytes.slice(b 1u64 3u64) Bytes.slice(b 0u64 4u64) Bytes.slice(b 2u64 1u64)} \
                  endfn\n";
    assert_eq!(
        value(source),
        tuple(vec![some(Value::Bytes(vec![2, 3])), none(), none()])
    );
}

#[test]
fn bytes_constructor_and_pattern() {
    let source = "fn main returns r: {Bytes List(U8)} do \
                  let Bytes(items) be Str.toUtf8(\"é\") endlet \
                  return {Bytes([7u8]) items} endfn\n";
    assert_eq!(
        value(source),
        tuple(vec![
            Value::Bytes(vec![7]),
            Value::List(vec![Value::U8(0xC3), Value::U8(0xA9)])
        ])
    );
}

#[test]
fn bytes_equatable_and_ordered() {
    let source = "fn main returns r: {Bool Bool Ordering Ordering Ordering} do \
                  return {Equatable.equal(Bytes([1u8]) Bytes([1u8])) \
                  Equatable.equal(Bytes([1u8]) Bytes([1u8 0u8])) \
                  Ordered.compare(Bytes([1u8 2u8]) Bytes([1u8 3u8])) \
                  Ordered.compare(Bytes([2u8]) Bytes([1u8 255u8])) \
                  Ordered.compare(Bytes([1u8]) Bytes([1u8 0u8]))} endfn\n";
    assert_eq!(
        value(source),
        tuple(vec![
            boolean(true),
            boolean(false),
            order("less"),
            order("greater"),
            order("less"),
        ])
    );
}

#[test]
fn list_literal() {
    let source = "fn main returns r: {List(I32) List(I32)} do \
                  let empty: List(I32) be [] endlet return {[3i32 1i32 2i32] empty} endfn\n";
    assert_eq!(value(source), tuple(vec![i32s(&[3, 1, 2]), i32s(&[])]));
}

#[test]
fn list_length() {
    let source =
        "fn main returns r: U64 do return List.length([1i32 2i32 3i32]) endfn\n";
    assert_eq!(value(source), Value::U64(3));
}

#[test]
fn list_append() {
    let source =
        "fn main returns r: List(I32) do return List.append([1i32] 2i32) endfn\n";
    assert_eq!(value(source), i32s(&[1, 2]));
}

#[test]
fn list_concat() {
    let source = "fn main returns r: List(I32) do return List.concat([1i32] [2i32 3i32]) endfn\n";
    assert_eq!(value(source), i32s(&[1, 2, 3]));
}

#[test]
fn list_slice() {
    let source = "fn main returns r: {Option(List(I32)) Option(List(I32))} do \
                  return {List.slice([1i32 2i32 3i32] 0u64 2u64) List.slice([1i32] 0u64 2u64)} \
                  endfn\n";
    assert_eq!(value(source), tuple(vec![some(i32s(&[1, 2])), none()]));
}

#[test]
fn dict_literal() {
    let source = "fn main returns r: Dict(Str I32) do \
                  return [\"b\": 1i32 \"a\": 2i32 \"b\": 3i32] endfn\n";
    assert_eq!(
        value(source),
        Value::Dict(vec![(text("a"), Value::I32(2)), (text("b"), Value::I32(3))])
    );
}

#[test]
fn dict_entries() {
    let source = "fn main returns r: List({Str I32}) do \
                  return Dict.entries([\"b\": 1i32 \"a\": 2i32]) endfn\n";
    assert_eq!(
        value(source),
        Value::List(vec![
            tuple(vec![text("a"), Value::I32(2)]),
            tuple(vec![text("b"), Value::I32(1)]),
        ])
    );
}

#[test]
fn dict_next() {
    let source = "fn main returns r: {Option({{Str I32} Dict(Str I32)}) Option({{Str I32} Dict(Str I32)})} do \
                  let empty: Dict(Str I32) be [:] endlet \
                  return {Iter.next([\"b\": 1i32 \"a\": 2i32]) Iter.next(empty)} endfn\n";
    assert_eq!(
        value(source),
        tuple(vec![
            some(tuple(vec![
                tuple(vec![text("a"), Value::I32(2)]),
                Value::Dict(vec![(text("b"), Value::I32(1))]),
            ])),
            none(),
        ])
    );
}

/// A source whose `Probe` is `Ordered` through a `compare` that spins
/// before it answers, so each call of it costs many reductions, followed by
/// `main`'s body.
fn probe_source(result: &str, body: &str) -> String {
    format!(
        "fn main returns r: {result} do return {body} endfn\n\
         fn spin takes {{n: U64}} returns r: U64 do \
         if Equatable.equal(n 0u64) then return 0u64 else return spin(U64.sub(n 1u64)) endif endfn\n\
         type Probe of U64\n  impl Ordered\n    fn compare\n      takes {{left: Self right: Self}}\n      \
         returns order: Ordering\n      do\n        let Probe(a) be left endlet\n        \
         let Probe(b) be right endlet\n        \
         return Ordered.compare(U64.add(a spin(500u64)) b)\n    endfn\n  endimpl\nendtype\n"
    )
}

#[test]
fn ordered_default_members_call_compare_once_per_pair() {
    let one = reductions(&probe_source(
        "Ordering",
        "Ordered.compare(Probe(1u64) Probe(2u64))",
    ));
    let two = reductions(&probe_source(
        "{Ordering Ordering}",
        "{Ordered.compare(Probe(1u64) Probe(2u64)) Ordered.compare(Probe(1u64) Probe(2u64))}",
    ));
    let cost = two - one;
    assert!(cost > 1000, "a compare costs {cost} reductions");
    let calls = |result: &str, body: &str| {
        let measured = reductions(&probe_source(result, body));
        (measured + cost / 2).saturating_sub(one) / cost + 1
    };
    for member in ["less", "lessOrEqual", "greater", "greaterOrEqual"] {
        let body = format!("Ordered.{member}(Probe(1u64) Probe(2u64))");
        assert_eq!(calls("Bool", &body), 1, "{member}");
    }
    for member in ["min", "max"] {
        let body = format!("Ordered.{member}(Probe(1u64) Probe(2u64))");
        assert_eq!(calls("Probe", &body), 1, "{member}");
    }
    // The lower bound first; the upper bound only when the value is not
    // below it.
    assert_eq!(
        calls(
            "Bool",
            "Ordered.inRange(Probe(0u64) Probe(1u64) Probe(3u64))"
        ),
        1
    );
    assert_eq!(
        calls(
            "Bool",
            "Ordered.inRange(Probe(2u64) Probe(1u64) Probe(3u64))"
        ),
        2
    );
}

#[test]
fn ordered_min_and_max_return_the_left_operand_on_a_tie() {
    let source = "fn main returns r: {Str Str} do \
                  let left be Keyed(key: 1u8 tag: \"left\") endlet \
                  let right be Keyed(key: 1u8 tag: \"right\") endlet \
                  let Keyed(key: _ tag: least) be Ordered.min(left right) endlet \
                  let Keyed(key: _ tag: greatest) be Ordered.max(left right) endlet \
                  return {least greatest} endfn\n\
                  type Keyed of {key: U8 tag: Str}\n  impl Ordered\n    fn compare\n      \
                  takes {left: Self right: Self}\n      returns order: Ordering\n      do\n        \
                  return Ordered.compare(left.key right.key)\n    endfn\n  endimpl\nendtype\n";
    assert_eq!(value(source), tuple(vec![text("left"), text("left")]));
}

/// A source with `costly`, a `Bool` that spins before it answers, followed
/// by `main`'s body.
fn bool_source(body: &str) -> String {
    format!(
        "fn main returns r: Bool do return {body} endfn\n\
         fn costly takes {{n: U64}} returns r: Bool do \
         if Equatable.equal(n 0u64) then return true else return costly(U64.sub(n 1u64)) endif endfn\n"
    )
}

#[test]
fn bool_operations_evaluate_both_operands() {
    let cheap = reductions(&bool_source("costly(0u64)"));
    let spun = reductions(&bool_source("costly(500u64)"));
    let cost = spun - cheap;
    assert!(cost > 1000, "the spin costs {cost} reductions");
    for (operation, decided) in [("and", "false"), ("or", "true"), ("xor", "true")] {
        let quick = reductions(&bool_source(&format!(
            "Bool.{operation}({decided} costly(0u64))"
        )));
        let slow = reductions(&bool_source(&format!(
            "Bool.{operation}({decided} costly(500u64))"
        )));
        assert_eq!(slow - quick, cost, "{operation}");
    }
    let short = reductions(&bool_source(
        "if false then costly(500u64) else false endif",
    ));
    assert!(short < cheap + cost / 2, "`if` evaluates one branch");
}
