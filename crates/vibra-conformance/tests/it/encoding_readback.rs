//! The canonical encodings `vibra-ir` writes read back through
//! `vibra-syntax`.
//!
//! `docs/spec/06-runtime.md`, "Canonical value encoding", fixes the bytes of
//! `@type.v2`, `@value.v2`, signature records, and implementation records,
//! and `docs/spec/07-diagnostics-and-conformance.md` compares an observation
//! as its decoded VIBON value. Each example of step 12's host tests is
//! written here twice: built with `vibra-ir` and encoded, and as the text the
//! chapter writes, with its layout where the chapter has one. Both are read
//! with `vibra_syntax::parse_data`, which must report no diagnostic, and the
//! decoded values must be equal, so the writer of `vibra-ir`, the chapter's
//! text, and the reader agree.

use vibra_ir::{
    Binder, Bound, Case, EntityRef, FnType, ImplementationRecord, IntentType,
    InterfaceKey, Library, LibraryTypes, NamedTypes, ProcType, Row, SignatureRecord,
    Type, TypeBody, TypeDefinition, TypeId, TypeTable, Value, Visibility, encode_type,
    encode_value, type_document, value_document,
};
use vibra_syntax::parse_data;

/// The chapter's placeholder identity.
const SEED: &str =
    "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad";

fn id(path: &str) -> TypeId {
    TypeId::new(path, SEED).unwrap()
}

fn path(library: Library) -> &'static str {
    match library {
        Library::Bool => "std.bool.Bool",
        Library::Str => "std.str.Str",
        Library::Bytes => "std.bytes.Bytes",
        Library::Option => "std.option.Option",
        Library::Result => "std.result.Result",
        Library::Dict => "std.dict.Dict",
        Library::Iter => "std.iter.Iter",
        Library::Equatable => "std.equatable.Equatable",
        Library::Ordered => "std.ordered.Ordered",
        Library::Ordering => "std.ordering.Ordering",
        Library::ArithmeticError => "std.number.ArithmeticError",
        Library::ConversionError => "std.number.ConversionError",
    }
}

fn declared(library: Library, arguments: &str) -> String {
    format!(
        "{{kind: @declared path: @{} seed: \"{SEED}\" arguments: [{arguments}]}}",
        path(library)
    )
}

fn example(path: &str) -> String {
    format!("{{kind: @declared path: @{path} seed: \"{SEED}\" arguments: []}}")
}

fn library_type(library: Library, arguments: Vec<Type>) -> Type {
    Type::Declared(id(path(library)), arguments)
}

fn named(entries: Vec<(&str, Type)>) -> NamedTypes {
    NamedTypes::new(entries.into_iter().map(|(name, ty)| (name.to_owned(), ty)))
        .unwrap()
}

fn definition(
    path: &str,
    parameters: &[&str],
    body: TypeBody,
    visibility: Visibility,
) -> TypeDefinition {
    TypeDefinition::new(
        id(path),
        parameters.iter().map(|name| (*name).to_owned()).collect(),
        body,
        None,
        visibility,
    )
    .unwrap()
}

/// The library bindings and declarations the value examples use, and the
/// three example declarations of step 12.
fn table() -> TypeTable {
    let plain = |names: &[&str]| {
        names
            .iter()
            .map(|name| Case::new(*name, Vec::new()))
            .collect::<Vec<_>>()
    };
    let library = [
        Library::Bool,
        Library::Str,
        Library::Bytes,
        Library::Option,
        Library::Dict,
    ]
    .into_iter()
    .fold(LibraryTypes::new(), |bindings, library| {
        bindings.with(library, id(path(library)))
    });
    let definitions = vec![
        definition(
            path(Library::Bool),
            &[],
            TypeBody::Variant(plain(&["false", "true"])),
            Visibility::Public,
        ),
        definition(
            path(Library::Option),
            &["T"],
            TypeBody::Variant(vec![
                Case::new("none", Vec::new()),
                Case::new("some", vec![("value".to_owned(), Type::Binder(0))]),
            ]),
            Visibility::Public,
        ),
        definition(
            path(Library::Str),
            &[],
            TypeBody::Wrapper(Type::list(Type::Char)),
            Visibility::Private,
        ),
        definition(
            path(Library::Bytes),
            &[],
            TypeBody::Wrapper(Type::list(Type::U8)),
            Visibility::Private,
        ),
        definition(
            path(Library::Dict),
            &["K", "V"],
            TypeBody::Wrapper(Type::list(Type::Tuple(vec![
                Type::Binder(0),
                Type::Binder(1),
            ]))),
            Visibility::Private,
        ),
        definition(
            "example.geometry.Point",
            &[],
            TypeBody::Record(vec![
                ("y".to_owned(), Type::I32),
                ("x".to_owned(), Type::I32),
            ]),
            Visibility::Public,
        ),
        definition(
            "example.geometry.Pair",
            &[],
            TypeBody::Tuple(vec![Type::I32, Type::I32]),
            Visibility::Public,
        ),
        definition(
            "example.units.Meters",
            &[],
            TypeBody::Wrapper(Type::F64),
            Visibility::Public,
        ),
    ];
    TypeTable::new(library, definitions)
}

/// Reads `written` and `text` and asserts both read with no diagnostic and
/// decode to equal values.
fn agree(label: &str, written: &str, text: &str) {
    let observed = parse_data(None, written.as_bytes());
    assert!(
        observed.diagnostics().is_empty(),
        "{label}: the written encoding `{written}` reads with {:?}",
        observed.diagnostics()
    );
    let expected = parse_data(None, text.as_bytes());
    assert!(
        expected.diagnostics().is_empty(),
        "{label}: the expected text reads with {:?}",
        expected.diagnostics()
    );
    let (Some(observed), Some(expected)) = (observed.value(), expected.value()) else {
        panic!("{label}: a document has no value");
    };
    assert!(
        observed.decoded_eq(expected),
        "{label}: `{written}` and `{text}` decode to different values"
    );
}

fn agree_type(label: &str, ty: &Type, text: &str) {
    agree(label, &encode_type(ty).unwrap(), text);
}

fn agree_value(label: &str, value: &Value, ty: &Type, text: &str) {
    agree(label, &encode_value(value, ty, &table()).unwrap(), text);
}

#[test]
fn every_scalar_type_reads_back() {
    for (ty, atom) in [
        (Type::I8, "@i8"),
        (Type::I16, "@i16"),
        (Type::I32, "@i32"),
        (Type::I64, "@i64"),
        (Type::U8, "@u8"),
        (Type::U16, "@u16"),
        (Type::U32, "@u32"),
        (Type::U64, "@u64"),
        (Type::F32, "@f32"),
        (Type::F64, "@f64"),
        (Type::Char, "@char"),
        (Type::Atom, "@atom"),
        (Type::Void, "@void"),
        (Type::Never, "@never"),
        (Type::Any, "@any"),
    ] {
        agree_type(atom, &ty, atom);
    }
}

#[test]
fn every_structural_type_reads_back() {
    let str_type = library_type(Library::Str, Vec::new());
    let bool_type = library_type(Library::Bool, Vec::new());
    let str_encoding = declared(Library::Str, "");
    let bool_encoding = declared(Library::Bool, "");
    agree_type(
        "list",
        &Type::list(Type::I32),
        "{kind: @list arguments: [@i32]}",
    );
    agree_type(
        "tuple",
        &Type::Tuple(vec![Type::I32, str_type.clone()]),
        &format!("{{kind: @tuple arguments: [@i32 {str_encoding}]}}"),
    );
    agree_type(
        "empty tuple",
        &Type::Tuple(Vec::new()),
        "{kind: @tuple arguments: []}",
    );
    agree_type(
        "record",
        &Type::record([("b".to_owned(), str_type), ("a".to_owned(), Type::I32)])
            .unwrap(),
        &format!("{{kind: @record fields: {{a: @i32 b: {str_encoding}}}}}"),
    );
    agree_type(
        "atom singleton",
        &Type::AtomSingleton("ready".to_owned()),
        "{kind: @atomSingleton atom: @ready}",
    );
    agree_type(
        "fn",
        &Type::function(FnType {
            parameters: vec![Type::I32],
            labels: named(vec![("verbose", bool_type.clone())]),
            result: bool_type,
        }),
        &format!(
            "{{kind: @fn parameters: [@i32] labels: [\"verbose\": {bool_encoding}] \
             result: {bool_encoding}}}"
        ),
    );
    agree_type(
        "fn without labels",
        &Type::function(FnType::new(Vec::new(), Type::Void)),
        "{kind: @fn parameters: [] labels: [:] result: @void}",
    );
    agree_type(
        "proc",
        &Type::Proc(Box::new(ProcType {
            parameters: vec![Type::I32],
            labels: named(Vec::new()),
            result: Type::Void,
            effects: Row::empty(),
        })),
        "{kind: @proc parameters: [@i32] labels: [:] result: @void effects: {roots: []}}",
    );
    let root = |path: &str| EntityRef::new(path, SEED).unwrap();
    agree_type(
        "intent",
        &Type::Intent(Box::new(IntentType {
            result: Type::Void,
            effects: Row::new([root("std.time.Clock"), root("std.fs.Files")]).unwrap(),
        })),
        &format!(
            "{{kind: @intent result: @void effects: {{roots: [\
             {{path: @std.fs.Files seed: \"{SEED}\"}} \
             {{path: @std.time.Clock seed: \"{SEED}\"}}]}}}}"
        ),
    );
    agree_type(
        "declared",
        &library_type(Library::Option, vec![Type::I32]),
        &declared(Library::Option, "@i32"),
    );
    agree_type(
        "interface value",
        &Type::Interface(
            id("std.iter.Iter"),
            Vec::new(),
            named(vec![("Item", Type::U8)]),
        ),
        &format!(
            "{{kind: @interface path: @std.iter.Iter seed: \"{SEED}\" arguments: [] \
             choices: [\"Item\": @u8]}}"
        ),
    );
}

#[test]
fn the_chapter_type_examples_read_back() {
    agree_type(
        "the declared example",
        &Type::Declared(id("example.count.Positive"), Vec::new()),
        r#"
{
  kind: @declared
  path: @example.count.Positive
  seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
  arguments: []
}
"#,
    );
    let ty = Type::function(FnType::new(
        vec![Type::Interface(
            id("std.iter.Iter"),
            Vec::new(),
            named(vec![("Item", Type::U8)]),
        )],
        Type::list(Type::U8),
    ));
    agree(
        "the @type.v2 example",
        &type_document(&ty).unwrap(),
        r#"
{
  format: @type.v2
  typeInfo: {
    kind: @fn
    parameters: [
      {
        kind: @interface
        path: @std.iter.Iter
        seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
        arguments: []
        choices: ["Item": @u8]
      }
    ]
    labels: [:]
    result: {kind: @list arguments: [@u8]}
  }
}
"#,
    );
}

#[test]
fn the_signature_and_implementation_records_read_back() {
    let option = |argument: Type| library_type(Library::Option, vec![argument]);
    let first = SignatureRecord::new(
        vec![Binder::new("T", None).unwrap()],
        Type::function(FnType::new(
            vec![Type::list(Type::Binder(0))],
            option(Type::Binder(0)),
        )),
    )
    .unwrap();
    agree(
        "the signature record of first",
        &first.encode().unwrap(),
        r#"
{
  binders: [{name: "T"}]
  typeInfo: {
    kind: @fn
    parameters: [{kind: @list arguments: [{kind: @binder index: 0u64}]}]
    labels: [:]
    result: {
      kind: @declared
      path: @std.option.Option
      seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
      arguments: [{kind: @binder index: 0u64}]
    }
  }
}
"#,
    );
    let next = SignatureRecord::interface_member(
        Vec::new(),
        Some(0),
        Type::function(FnType::new(
            vec![Type::SelfType],
            option(Type::Tuple(vec![
                Type::Associated("Item".to_owned()),
                Type::SelfType,
            ])),
        )),
        vec!["Item".to_owned()],
    )
    .unwrap();
    let self_item =
        "{kind: @tuple arguments: [{kind: @associated name: \"Item\"} {kind: @self}]}";
    agree(
        "the signature record of Iter.next",
        &next.encode().unwrap(),
        &format!(
            "{{binders: [] receiver: 0u64 typeInfo: {{kind: @fn parameters: [{{kind: @self}}] \
             labels: [:] result: {}}}}}",
            declared(Library::Option, self_item)
        ),
    );
    let bounded = SignatureRecord::new(
        vec![
            Binder::new(
                "T",
                Some(Bound {
                    contract: InterfaceKey::new(id("std.ordered.Ordered"), Vec::new()),
                    choices: named(Vec::new()),
                }),
            )
            .unwrap(),
        ],
        Type::function(FnType::new(vec![Type::Binder(0)], Type::Binder(0))),
    )
    .unwrap();
    agree(
        "a bound",
        &bounded.encode().unwrap(),
        &format!(
            "{{binders: [{{name: \"T\" bound: {{contract: {{path: @std.ordered.Ordered seed: \
             \"{SEED}\" arguments: []}} choices: [:]}}}}] typeInfo: {{kind: @fn parameters: \
             [{{kind: @binder index: 0u64}}] labels: [:] result: {{kind: @binder index: 0u64}}}}}}"
        ),
    );
    let implementation = ImplementationRecord::new(
        vec![Binder::new("T", None).unwrap()],
        InterfaceKey::new(id("std.iter.Iter"), Vec::new()),
        Type::list(Type::Binder(0)),
        named(vec![("Item", Type::Binder(0))]),
    )
    .unwrap();
    agree(
        "an implementation record",
        &implementation.encode().unwrap(),
        &format!(
            "{{binders: [{{name: \"T\"}}] contract: {{path: @std.iter.Iter seed: \"{SEED}\" \
             arguments: []}} receiver: {{kind: @list arguments: [{{kind: @binder index: 0u64}}]}} \
             choices: [\"Item\": {{kind: @binder index: 0u64}}]}}"
        ),
    );
}

#[test]
fn every_value_example_reads_back() {
    let str_type = library_type(Library::Str, Vec::new());
    agree_value(
        "bytes",
        &Value::Bytes(vec![1, 2, 255]),
        &library_type(Library::Bytes, Vec::new()),
        "{kind: @bytes values: [1u8 2u8 255u8]}",
    );
    let pair = Value::Tuple(vec![Value::I32(1), Value::I32(-2)]);
    agree_value(
        "anonymous tuple",
        &pair,
        &Type::Tuple(vec![Type::I32, Type::I32]),
        "{kind: @tuple values: [1i32 -2i32]}",
    );
    agree_value(
        "declared tuple",
        &pair,
        &Type::Declared(id("example.geometry.Pair"), Vec::new()),
        &format!(
            "{{kind: @tuple typeInfo: {} values: [1i32 -2i32]}}",
            example("example.geometry.Pair")
        ),
    );
    agree_value(
        "list",
        &Value::List(vec![Value::U8(3), Value::U8(1)]),
        &Type::list(Type::U8),
        "{kind: @list values: [3u8 1u8]}",
    );
    agree_value(
        "dict",
        &Value::Dict(vec![
            (Value::Str("a".to_owned()), Value::I32(1)),
            (Value::Str("b".to_owned()), Value::I32(2)),
        ]),
        &library_type(Library::Dict, vec![str_type.clone(), Type::I32]),
        "{kind: @dict entries: [{kind: @tuple values: [\"a\" 1i32]} \
         {kind: @tuple values: [\"b\" 2i32]}]}",
    );
    let point = Value::Record(vec![
        ("x".to_owned(), Value::I32(1)),
        ("y".to_owned(), Value::I32(2)),
    ]);
    agree_value(
        "declared record",
        &point,
        &Type::Declared(id("example.geometry.Point"), Vec::new()),
        &format!(
            "{{kind: @record typeInfo: {} fields: {{y: 2i32 x: 1i32}}}}",
            example("example.geometry.Point")
        ),
    );
    agree_value(
        "anonymous record",
        &point,
        &Type::record([("y".to_owned(), Type::I32), ("x".to_owned(), Type::I32)])
            .unwrap(),
        "{kind: @record fields: {x: 1i32 y: 2i32}}",
    );
    let option = library_type(Library::Option, vec![Type::I32]);
    let option_encoding = declared(Library::Option, "@i32");
    agree_value(
        "variant with a payload",
        &Value::Variant {
            case: "some".to_owned(),
            fields: vec![("value".to_owned(), Value::I32(5))],
        },
        &option,
        &format!(
            "{{kind: @enum typeInfo: {option_encoding} variant: @some payload: {{value: 5i32}}}}"
        ),
    );
    agree_value(
        "fieldless variant",
        &Value::Variant {
            case: "none".to_owned(),
            fields: Vec::new(),
        },
        &option,
        &format!("{{kind: @enum typeInfo: {option_encoding} variant: @none}}"),
    );
    agree_value(
        "bool",
        &Value::bool(true),
        &library_type(Library::Bool, Vec::new()),
        &format!(
            "{{kind: @enum typeInfo: {} variant: @true}}",
            declared(Library::Bool, "")
        ),
    );
    agree_value(
        "wrapper",
        &Value::Wrapper(Box::new(Value::f64(1.5))),
        &Type::Declared(id("example.units.Meters"), Vec::new()),
        &format!(
            "{{kind: @wrapper typeInfo: {} value: 1.5f64}}",
            example("example.units.Meters")
        ),
    );
    agree_value(
        "string",
        &Value::Str("a\"b\\c\n\r\t\u{1}\u{1F}\u{7F}\u{E9}'".to_owned()),
        &str_type,
        "\"a\\\"b\\\\c\\n\\r\\t\\u{1}\\u{1F}\\u{7F}\u{E9}'\"",
    );
    for (character, text) in [('\'', "'\\''"), ('"', "'\"'"), ('\u{1B}', "'\\u{1B}'")] {
        agree_value(text, &Value::Char(character), &Type::Char, text);
    }
}

#[test]
fn every_scalar_value_reads_back() {
    for (value, ty, text) in [
        (Value::I8(-128), Type::I8, "-128i8"),
        (Value::U64(u64::MAX), Type::U64, "18446744073709551615u64"),
        (Value::f64(100.0), Type::F64, "100.0f64"),
        (Value::f64(0.0025), Type::F64, "0.0025f64"),
        (Value::f64(1.5e-7), Type::F64, "1.5e-7f64"),
        (Value::f64(1e16), Type::F64, "1e16f64"),
        (Value::f32(0.0), Type::F32, "0.0f32"),
        (Value::Void, Type::Void, "void"),
        (Value::Atom("a.b".to_owned()), Type::Atom, "@a.b"),
        (
            Value::Atom("ready".to_owned()),
            Type::AtomSingleton("ready".to_owned()),
            "@ready",
        ),
        (
            Value::F32(0xFFC0_1234),
            Type::F32,
            "{kind: @nonFinite value: @nan}",
        ),
        (
            Value::F64(0x7FF0_0000_0000_0001),
            Type::F64,
            "{kind: @nonFinite value: @nan}",
        ),
        (
            Value::f32(f32::INFINITY),
            Type::F32,
            "{kind: @nonFinite value: @positiveInfinity}",
        ),
    ] {
        agree_value(text, &value, &ty, text);
    }
}

#[test]
fn the_chapter_value_examples_read_back() {
    let types = table();
    agree(
        "200u8",
        &value_document(&Value::U8(200), &Type::U8, &types).unwrap(),
        "{format: @value.v2 typeInfo: @u8 value: 200u8}",
    );
    agree(
        "negative infinity",
        &value_document(&Value::f64(f64::NEG_INFINITY), &Type::F64, &types).unwrap(),
        "{format: @value.v2 typeInfo: @f64 value: {kind: @nonFinite value: @negativeInfinity}}",
    );
    let record = Type::record([
        ("scale".to_owned(), Type::F64),
        ("ratio".to_owned(), Type::F32),
    ])
    .unwrap();
    let value = Value::Record(vec![
        ("scale".to_owned(), Value::f64(1e20)),
        ("ratio".to_owned(), Value::f32(-0.0)),
    ]);
    agree(
        "the record example",
        &value_document(&value, &record, &types).unwrap(),
        r"
{
  format: @value.v2
  typeInfo: {kind: @record fields: {ratio: @f32 scale: @f64}}
  value: {kind: @record fields: {ratio: -0.0f32 scale: 1e20f64}}
}
",
    );
    agree(
        "Any holding 200u8",
        &encode_value(
            &Value::Interface {
                held_type: Type::U8,
                held: Box::new(Value::U8(200)),
            },
            &Type::Any,
            &types,
        )
        .unwrap(),
        "{kind: @interface held: {format: @value.v2 typeInfo: @u8 value: 200u8}}",
    );
}
