//! The canonical encodings against `docs/spec/06-runtime.md`, "Canonical
//! value encoding": the rows of "Types" and "Values", every example of
//! "Types", "Open signatures", and "Values", and what has no encoding.

use vibra_ir::{
    Binder, Bound, EncodeError, EntityRef, FnType, ImplementationRecord, IntentType,
    InterfaceKey, Library, ProcType, Row, SignatureRecord, Type, TypeBody,
    TypeDefinition, Value, Visibility, encode_type, encode_value, type_document,
    value_document,
};

use crate::{
    SEED, bool_type, declared, id, iter_interface, library_type, named, option,
    str_type, table, unlayout,
};

fn str_encoding() -> String {
    declared(Library::Str, "")
}

fn bool_encoding() -> String {
    declared(Library::Bool, "")
}

// -- Types --------------------------------------------------------------------

#[test]
fn every_scalar_is_its_bare_wire_atom() {
    let expected = [
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
    ];
    for (ty, atom) in expected {
        assert_eq!(encode_type(&ty).unwrap(), atom);
    }
}

#[test]
fn a_list_is_the_list_record_with_its_element() {
    assert_eq!(
        encode_type(&Type::list(Type::I32)).unwrap(),
        "{kind: @list arguments: [@i32]}"
    );
}

#[test]
fn a_structural_tuple_lists_its_components() {
    assert_eq!(
        encode_type(&Type::Tuple(vec![Type::I32, str_type()])).unwrap(),
        format!("{{kind: @tuple arguments: [@i32 {}]}}", str_encoding())
    );
    assert_eq!(
        encode_type(&Type::Tuple(Vec::new())).unwrap(),
        "{kind: @tuple arguments: []}"
    );
}

#[test]
fn a_structural_record_has_its_fields_in_canonical_name_order() {
    let record =
        Type::record([("b".to_owned(), str_type()), ("a".to_owned(), Type::I32)])
            .unwrap();
    assert_eq!(
        encode_type(&record).unwrap(),
        format!(
            "{{kind: @record fields: {{a: @i32 b: {}}}}}",
            str_encoding()
        )
    );
}

#[test]
fn an_atom_singleton_names_its_atom() {
    assert_eq!(
        encode_type(&Type::AtomSingleton("ready".to_owned())).unwrap(),
        "{kind: @atomSingleton atom: @ready}"
    );
}

#[test]
fn a_fn_has_its_labels_as_a_dict() {
    let ty = Type::function(FnType {
        parameters: vec![Type::I32],
        labels: named(vec![("verbose", bool_type())]),
        result: bool_type(),
    });
    let bool_encoding = bool_encoding();
    assert_eq!(
        encode_type(&ty).unwrap(),
        format!(
            "{{kind: @fn parameters: [@i32] labels: [\"verbose\": {bool_encoding}] \
             result: {bool_encoding}}}"
        )
    );
    assert_eq!(
        encode_type(&Type::function(FnType::new(Vec::new(), Type::Void))).unwrap(),
        "{kind: @fn parameters: [] labels: [:] result: @void}"
    );
}

#[test]
fn a_proc_and_an_intent_write_their_row_even_when_empty() {
    let proc_type = Type::Proc(Box::new(ProcType {
        parameters: vec![Type::I32],
        labels: named(Vec::new()),
        result: Type::Void,
        effects: Row::empty(),
    }));
    assert_eq!(
        encode_type(&proc_type).unwrap(),
        "{kind: @proc parameters: [@i32] labels: [:] result: @void effects: {roots: []}}"
    );
    let intent = Type::Intent(Box::new(IntentType {
        result: Type::Void,
        effects: Row::empty(),
    }));
    assert_eq!(
        encode_type(&intent).unwrap(),
        "{kind: @intent result: @void effects: {roots: []}}"
    );
}

#[test]
fn a_row_sorts_its_roots_by_path() {
    let root = |path: &str| EntityRef::new(path, SEED).unwrap();
    let intent = Type::Intent(Box::new(IntentType {
        result: Type::Void,
        effects: Row::new([root("std.time.Clock"), root("std.fs.Files")]).unwrap(),
    }));
    assert_eq!(
        encode_type(&intent).unwrap(),
        format!(
            "{{kind: @intent result: @void effects: {{roots: [\
             {{path: @std.fs.Files seed: \"{SEED}\"}} \
             {{path: @std.time.Clock seed: \"{SEED}\"}}]}}}}"
        )
    );
}

#[test]
fn a_declared_type_writes_its_arguments() {
    assert_eq!(
        encode_type(&option(Type::I32)).unwrap(),
        declared(Library::Option, "@i32")
    );
}

#[test]
fn an_interface_value_type_fixes_every_choice() {
    let iter = Type::Interface(
        id("std.iter.Iter"),
        Vec::new(),
        named(vec![("Item", Type::U8)]),
    );
    assert_eq!(
        encode_type(&iter).unwrap(),
        format!(
            "{{kind: @interface path: @std.iter.Iter seed: \"{SEED}\" arguments: [] \
             choices: [\"Item\": @u8]}}"
        )
    );
}

#[test]
fn the_chapter_declared_example_is_byte_equal() {
    let chapter = r#"
{
  kind: @declared
  path: @example.count.Positive
  seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
  arguments: []
}
"#;
    let ty = Type::Declared(id("example.count.Positive"), Vec::new());
    assert_eq!(encode_type(&ty).unwrap(), unlayout(chapter));
}

#[test]
fn the_chapter_type_document_example_is_byte_equal() {
    let chapter = r#"
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
"#;
    let ty = Type::function(FnType::new(
        vec![Type::Interface(
            id("std.iter.Iter"),
            Vec::new(),
            named(vec![("Item", Type::U8)]),
        )],
        Type::list(Type::U8),
    ));
    assert_eq!(type_document(&ty).unwrap(), unlayout(chapter));
}

#[test]
fn an_open_form_in_a_type_document_is_rejected() {
    for open in [
        Type::Binder(0),
        Type::list(Type::Binder(0)),
        Type::SelfType,
        Type::Associated("Item".to_owned()),
    ] {
        assert!(
            matches!(type_document(&open), Err(EncodeError::OpenForm(_))),
            "{open}"
        );
    }
}

// -- Open signatures ----------------------------------------------------------

#[test]
fn the_chapter_signature_record_of_first_is_byte_equal() {
    let chapter = r#"
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
"#;
    let first = SignatureRecord::new(
        vec![Binder::new("T", None).unwrap()],
        Type::function(FnType::new(
            vec![Type::list(Type::Binder(0))],
            option(Type::Binder(0)),
        )),
    )
    .unwrap();
    assert_eq!(first.encode().unwrap(), unlayout(chapter));
}

#[test]
fn the_signature_record_of_iter_next_has_its_receiver_self_and_item() {
    let interface = iter_interface();
    let next = &interface.member("next").unwrap().signature;
    let self_item =
        "{kind: @tuple arguments: [{kind: @associated name: \"Item\"} {kind: @self}]}";
    assert_eq!(
        next.encode().unwrap(),
        format!(
            "{{binders: [] receiver: 0u64 typeInfo: {{kind: @fn parameters: [{{kind: @self}}] \
             labels: [:] result: {}}}}}",
            declared(Library::Option, self_item)
        )
    );
}

#[test]
fn a_bound_writes_its_interface_key_and_choices() {
    let ordered = InterfaceKey::new(id("std.ordered.Ordered"), Vec::new());
    let record = SignatureRecord::new(
        vec![
            Binder::new(
                "T",
                Some(Bound {
                    contract: ordered,
                    choices: named(Vec::new()),
                }),
            )
            .unwrap(),
        ],
        Type::function(FnType::new(vec![Type::Binder(0)], Type::Binder(0))),
    )
    .unwrap();
    assert_eq!(
        record.encode().unwrap(),
        format!(
            "{{binders: [{{name: \"T\" bound: {{contract: {{path: @std.ordered.Ordered seed: \
             \"{SEED}\" arguments: []}} choices: [:]}}}}] typeInfo: {{kind: @fn parameters: \
             [{{kind: @binder index: 0u64}}] labels: [:] result: {{kind: @binder index: 0u64}}}}}}"
        )
    );
}

#[test]
fn an_implementation_record_has_binders_contract_receiver_and_choices() {
    let record = ImplementationRecord::new(
        vec![Binder::new("T", None).unwrap()],
        InterfaceKey::new(id("std.iter.Iter"), Vec::new()),
        Type::list(Type::Binder(0)),
        named(vec![("Item", Type::Binder(0))]),
    )
    .unwrap();
    assert_eq!(
        record.encode().unwrap(),
        format!(
            "{{binders: [{{name: \"T\"}}] contract: {{path: @std.iter.Iter seed: \"{SEED}\" \
             arguments: []}} receiver: {{kind: @list arguments: [{{kind: @binder index: 0u64}}]}} \
             choices: [\"Item\": {{kind: @binder index: 0u64}}]}}"
        )
    );
}

#[test]
fn a_misplaced_open_form_is_rejected_in_a_record() {
    let function = |ty: Type| Type::function(FnType::new(vec![ty], Type::Void));
    assert!(SignatureRecord::new(Vec::new(), function(Type::Binder(0))).is_err());
    assert!(SignatureRecord::new(Vec::new(), function(Type::SelfType)).is_err());
    assert!(
        SignatureRecord::new(Vec::new(), function(Type::Associated("Item".to_owned())))
            .is_err()
    );
    assert!(
        SignatureRecord::interface_member(
            Vec::new(),
            Some(0),
            function(Type::SelfType),
            vec!["Item".to_owned()],
        )
        .is_ok()
    );
    assert!(
        SignatureRecord::interface_member(
            Vec::new(),
            None,
            function(Type::Associated("Other".to_owned())),
            vec!["Item".to_owned()],
        )
        .is_err()
    );
}

// -- Values -------------------------------------------------------------------

fn point() -> TypeDefinition {
    TypeDefinition::new(
        id("example.geometry.Point"),
        Vec::new(),
        TypeBody::Record(vec![
            ("y".to_owned(), Type::I32),
            ("x".to_owned(), Type::I32),
        ]),
        None,
        Visibility::Public,
    )
    .unwrap()
}

fn pair() -> TypeDefinition {
    TypeDefinition::new(
        id("example.geometry.Pair"),
        Vec::new(),
        TypeBody::Tuple(vec![Type::I32, Type::I32]),
        None,
        Visibility::Public,
    )
    .unwrap()
}

fn meters() -> TypeDefinition {
    TypeDefinition::new(
        id("example.units.Meters"),
        Vec::new(),
        TypeBody::Wrapper(Type::F64),
        None,
        Visibility::Public,
    )
    .unwrap()
}

fn example(path: &str) -> String {
    format!("{{kind: @declared path: @{path} seed: \"{SEED}\" arguments: []}}")
}

fn encode(value: &Value, ty: &Type) -> String {
    encode_value(value, ty, &table(vec![point(), pair(), meters()])).unwrap()
}

#[test]
fn a_bytes_value_is_the_bytes_record() {
    assert_eq!(
        encode(
            &Value::Bytes(vec![1, 2, 255]),
            &library_type(Library::Bytes, Vec::new())
        ),
        "{kind: @bytes values: [1u8 2u8 255u8]}"
    );
}

#[test]
fn a_tuple_has_a_type_only_when_declared() {
    let value = Value::Tuple(vec![Value::I32(1), Value::I32(-2)]);
    assert_eq!(
        encode(&value, &Type::Tuple(vec![Type::I32, Type::I32])),
        "{kind: @tuple values: [1i32 -2i32]}"
    );
    assert_eq!(
        encode(
            &value,
            &Type::Declared(id("example.geometry.Pair"), Vec::new())
        ),
        format!(
            "{{kind: @tuple typeInfo: {} values: [1i32 -2i32]}}",
            example("example.geometry.Pair")
        )
    );
}

#[test]
fn a_list_keeps_its_order() {
    assert_eq!(
        encode(
            &Value::List(vec![Value::U8(3), Value::U8(1)]),
            &Type::list(Type::U8)
        ),
        "{kind: @list values: [3u8 1u8]}"
    );
}

#[test]
fn a_dict_lists_its_entries_in_key_order_as_tuples() {
    let dict = library_type(Library::Dict, vec![str_type(), Type::I32]);
    let value = Value::Dict(vec![
        (Value::Str("a".to_owned()), Value::I32(1)),
        (Value::Str("b".to_owned()), Value::I32(2)),
    ]);
    assert_eq!(
        encode(&value, &dict),
        "{kind: @dict entries: [{kind: @tuple values: [\"a\" 1i32]} \
         {kind: @tuple values: [\"b\" 2i32]}]}"
    );
}

#[test]
fn a_record_has_declaration_order_when_declared_and_name_order_otherwise() {
    let value = Value::Record(vec![
        ("x".to_owned(), Value::I32(1)),
        ("y".to_owned(), Value::I32(2)),
    ]);
    assert_eq!(
        encode(
            &value,
            &Type::Declared(id("example.geometry.Point"), Vec::new())
        ),
        format!(
            "{{kind: @record typeInfo: {} fields: {{y: 2i32 x: 1i32}}}}",
            example("example.geometry.Point")
        )
    );
    let anonymous =
        Type::record([("y".to_owned(), Type::I32), ("x".to_owned(), Type::I32)])
            .unwrap();
    assert_eq!(
        encode(&value, &anonymous),
        "{kind: @record fields: {x: 1i32 y: 2i32}}"
    );
}

#[test]
fn a_variant_has_a_payload_only_for_a_case_with_fields() {
    let ty = option(Type::I32);
    let type_info = declared(Library::Option, "@i32");
    let some = Value::Variant {
        case: "some".to_owned(),
        fields: vec![("value".to_owned(), Value::I32(5))],
    };
    assert_eq!(
        encode(&some, &ty),
        format!(
            "{{kind: @enum typeInfo: {type_info} variant: @some payload: {{value: 5i32}}}}"
        )
    );
    let none = Value::Variant {
        case: "none".to_owned(),
        fields: Vec::new(),
    };
    assert_eq!(
        encode(&none, &ty),
        format!("{{kind: @enum typeInfo: {type_info} variant: @none}}")
    );
}

#[test]
fn a_bool_is_a_variant_of_the_type_playing_bool() {
    assert_eq!(
        encode(&Value::bool(true), &bool_type()),
        format!(
            "{{kind: @enum typeInfo: {} variant: @true}}",
            bool_encoding()
        )
    );
}

#[test]
fn a_wrapper_holds_its_representation() {
    assert_eq!(
        encode(
            &Value::Wrapper(Box::new(Value::f64(1.5))),
            &Type::Declared(id("example.units.Meters"), Vec::new())
        ),
        format!(
            "{{kind: @wrapper typeInfo: {} value: 1.5f64}}",
            example("example.units.Meters")
        )
    );
}

#[test]
fn a_string_and_a_char_are_their_shortest_literals() {
    let text = "a\"b\\c\n\r\t\u{1}\u{1F}\u{7F}\u{E9}'";
    assert_eq!(
        encode(&Value::Str(text.to_owned()), &str_type()),
        "\"a\\\"b\\\\c\\n\\r\\t\\u{1}\\u{1F}\\u{7F}\u{E9}'\""
    );
    assert_eq!(encode(&Value::Char('\''), &Type::Char), "'\\''");
    assert_eq!(encode(&Value::Char('"'), &Type::Char), "'\"'");
    assert_eq!(encode(&Value::Char('\u{1B}'), &Type::Char), "'\\u{1B}'");
}

#[test]
fn scalars_have_their_suffix_and_floats_their_canonical_text() {
    assert_eq!(encode(&Value::I8(-128), &Type::I8), "-128i8");
    assert_eq!(
        encode(&Value::U64(u64::MAX), &Type::U64),
        "18446744073709551615u64"
    );
    assert_eq!(encode(&Value::f64(100.0), &Type::F64), "100.0f64");
    assert_eq!(encode(&Value::f64(0.0025), &Type::F64), "0.0025f64");
    assert_eq!(encode(&Value::f64(1.5e-7), &Type::F64), "1.5e-7f64");
    assert_eq!(encode(&Value::f64(1e16), &Type::F64), "1e16f64");
    assert_eq!(encode(&Value::f32(0.0), &Type::F32), "0.0f32");
    assert_eq!(encode(&Value::Void, &Type::Void), "void");
    assert_eq!(encode(&Value::Atom("a.b".to_owned()), &Type::Atom), "@a.b");
    assert_eq!(
        encode(
            &Value::Atom("ready".to_owned()),
            &Type::AtomSingleton("ready".to_owned())
        ),
        "@ready"
    );
}

#[test]
fn the_chapter_value_examples_are_byte_equal() {
    let types = table(Vec::new());
    assert_eq!(
        value_document(&Value::U8(200), &Type::U8, &types).unwrap(),
        "{format: @value.v2 typeInfo: @u8 value: 200u8}"
    );
    assert_eq!(
        value_document(&Value::f64(f64::NEG_INFINITY), &Type::F64, &types).unwrap(),
        "{format: @value.v2 typeInfo: @f64 value: {kind: @nonFinite value: @negativeInfinity}}"
    );
    let chapter = r"
{
  format: @value.v2
  typeInfo: {kind: @record fields: {ratio: @f32 scale: @f64}}
  value: {kind: @record fields: {ratio: -0.0f32 scale: 1e20f64}}
}
";
    let record = Type::record([
        ("scale".to_owned(), Type::F64),
        ("ratio".to_owned(), Type::F32),
    ])
    .unwrap();
    let value = Value::Record(vec![
        ("scale".to_owned(), Value::f64(1e20)),
        ("ratio".to_owned(), Value::f32(-0.0)),
    ]);
    assert_eq!(
        value_document(&value, &record, &types).unwrap(),
        unlayout(chapter)
    );
    let held = Value::Interface {
        held_type: Type::U8,
        held: Box::new(Value::U8(200)),
    };
    assert_eq!(
        encode_value(&held, &Type::Any, &types).unwrap(),
        "{kind: @interface held: {format: @value.v2 typeInfo: @u8 value: 200u8}}"
    );
}

#[test]
fn every_nan_is_the_one_canonical_nan() {
    let types = table(Vec::new());
    for bits in [0xFFC0_1234_u32, 0x7F80_0001, 0x7FC0_0000] {
        assert_eq!(
            encode_value(&Value::F32(bits), &Type::F32, &types).unwrap(),
            "{kind: @nonFinite value: @nan}"
        );
    }
    for bits in [0xFFF4_0000_DEAD_BEEF_u64, 0x7FF0_0000_0000_0001] {
        assert_eq!(
            encode_value(&Value::F64(bits), &Type::F64, &types).unwrap(),
            "{kind: @nonFinite value: @nan}"
        );
    }
    assert_eq!(
        encode_value(&Value::f32(f32::INFINITY), &Type::F32, &types).unwrap(),
        "{kind: @nonFinite value: @positiveInfinity}"
    );
}

// -- What has no encoding -----------------------------------------------------

/// A closure value that captured one `I32`, and its type.
fn closure() -> (Type, Value) {
    (
        Type::function(FnType::new(Vec::new(), Type::I32)),
        Value::Closure {
            captures: vec![Value::I32(1)],
        },
    )
}

#[test]
fn a_closure_has_no_encoding() {
    let types = table(Vec::new());
    let (ty, value) = closure();
    assert!(matches!(
        encode_value(&value, &ty, &types),
        Err(EncodeError::NoEncoding(_))
    ));
    assert!(matches!(
        value_document(&value, &ty, &types),
        Err(EncodeError::NoEncoding(_))
    ));
    let function = Value::Function {
        function: 0,
        type_arguments: Vec::new(),
    };
    assert!(matches!(
        encode_value(&function, &ty, &types),
        Err(EncodeError::NoEncoding(_))
    ));
}

#[test]
fn an_interface_value_holding_a_closure_has_no_encoding() {
    let types = table(Vec::new());
    let (ty, value) = closure();
    let held = Value::Interface {
        held_type: ty,
        held: Box::new(value),
    };
    assert!(matches!(
        encode_value(&held, &Type::Any, &types),
        Err(EncodeError::NoEncoding(_))
    ));
    let nested = Value::List(vec![held]);
    assert!(matches!(
        encode_value(&nested, &Type::list(Type::Any), &types),
        Err(EncodeError::NoEncoding(_))
    ));
}

#[test]
fn a_value_that_does_not_fit_its_type_is_a_mismatch() {
    let types = table(Vec::new());
    assert!(matches!(
        encode_value(&Value::I32(1), &Type::I64, &types),
        Err(EncodeError::Mismatch { .. })
    ));
    assert!(matches!(
        encode_value(&Value::Void, &Type::Never, &types),
        Err(EncodeError::NeverValue)
    ));
    assert!(matches!(
        encode_value(
            &Value::Record(vec![("a".to_owned(), Value::I32(1))]),
            &Type::record([("a".to_owned(), Type::I32), ("b".to_owned(), Type::I32)])
                .unwrap(),
            &types
        ),
        Err(EncodeError::Mismatch { .. })
    ));
    assert!(matches!(
        encode_value(
            &Value::Tuple(Vec::new()),
            &Type::Declared(id("example.unknown.Thing"), Vec::new()),
            &types
        ),
        Err(EncodeError::UnknownType(_))
    ));
}
