//! M3 Step 2 declared-type IR: definitions, construction, projection, and the
//! checked-program validation that rejects IR a checker must never produce.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use vibra_diagnostics::ByteSpan;
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, IrError, ObservedValue,
    SourceOrigin, Type, TypeBody, TypeDefinition, TypeId, Value, canonical_members,
    canonical_type,
};

fn origin() -> SourceOrigin {
    SourceOrigin::new("case.vib", ByteSpan::new(0, 1))
}

fn point_id() -> TypeId {
    TypeId::new("@demo@1.0.0/app.main.point", "app.main.point")
}

fn point() -> TypeDefinition {
    TypeDefinition::new(
        point_id(),
        TypeBody::Record(vec![
            ("x".to_owned(), Type::I32),
            ("y".to_owned(), Type::I32),
        ]),
    )
}

fn literal(value: Value) -> Expr {
    Expr::literal(value, origin())
}

fn program(
    types: Vec<TypeDefinition>,
    result: Type,
    body: Expr,
) -> Result<CheckedProgram, IrError> {
    let function = CheckedFunction::new(
        "main",
        FunctionSignature::new(Vec::new(), result),
        body,
        origin(),
    )?;
    CheckedProgram::try_new_with_types(types, Vec::new(), vec![function], 0)
}

#[test]
fn a_declared_record_construction_and_projection_validate() {
    let record = Expr::Record {
        value_type: Type::Declared(point_id()),
        fields: vec![
            ("x".to_owned(), literal(Value::I32(1))),
            ("y".to_owned(), literal(Value::I32(2))),
        ],
        origin: origin(),
    };
    let project = Expr::Project {
        record: Box::new(record),
        field: "y".to_owned(),
        value_type: Type::I32,
        origin: origin(),
    };
    let program = program(vec![point()], Type::I32, project).expect("valid program");
    assert_eq!(program.types().len(), 1);
}

#[test]
fn a_declared_type_without_a_definition_is_rejected() {
    let record = Expr::Record {
        value_type: Type::Declared(point_id()),
        fields: vec![
            ("x".to_owned(), literal(Value::I32(1))),
            ("y".to_owned(), literal(Value::I32(2))),
        ],
        origin: origin(),
    };
    assert!(program(Vec::new(), Type::Declared(point_id()), record).is_err());
}

#[test]
fn a_construction_disagreeing_with_its_definition_is_rejected() {
    let missing = Expr::Record {
        value_type: Type::Declared(point_id()),
        fields: vec![("x".to_owned(), literal(Value::I32(1)))],
        origin: origin(),
    };
    assert!(program(vec![point()], Type::Declared(point_id()), missing).is_err());

    let wrong_type = Expr::Record {
        value_type: Type::Declared(point_id()),
        fields: vec![
            ("x".to_owned(), literal(Value::I32(1))),
            ("y".to_owned(), literal(Value::Str("two".to_owned()))),
        ],
        origin: origin(),
    };
    assert!(program(vec![point()], Type::Declared(point_id()), wrong_type).is_err());

    let unknown_field = Expr::Project {
        record: Box::new(Expr::Record {
            value_type: Type::Declared(point_id()),
            fields: vec![
                ("x".to_owned(), literal(Value::I32(1))),
                ("y".to_owned(), literal(Value::I32(2))),
            ],
            origin: origin(),
        }),
        field: "z".to_owned(),
        value_type: Type::I32,
        origin: origin(),
    };
    assert!(program(vec![point()], Type::I32, unknown_field).is_err());
}

#[test]
fn a_repeated_definition_is_rejected() {
    let body = literal(Value::I32(0));
    assert!(program(vec![point(), point()], Type::I32, body).is_err());
}

#[test]
fn anonymous_members_have_one_canonical_order_and_encoding() {
    let written = vec![("name".to_owned(), Type::Str), ("id".to_owned(), Type::U64)];
    let reordered = vec![("id".to_owned(), Type::U64), ("name".to_owned(), Type::Str)];
    assert_eq!(
        canonical_members(written),
        canonical_members(reordered.clone())
    );
    assert_eq!(
        canonical_type(&Type::Record(canonical_members(reordered))),
        "(record type: @record fields: (record id: @u64 name: @str))"
    );
}

#[test]
fn observed_values_render_the_canonical_value_encoding() {
    let value = ObservedValue::Enum {
        type_id: Some(TypeId::new("@demo@1.0.0/app.main.shape", "app.main.shape")),
        variant: "circle".to_owned(),
        payload: Some(Box::new(ObservedValue::Primitive(Value::U32(3)))),
    };
    assert_eq!(
        value.canonical_vibon(),
        "(record kind: @enum type: @app.main.shape variant: @circle payload: 3u32)"
    );
    let anonymous = ObservedValue::Record {
        type_id: None,
        fields: vec![("a".to_owned(), ObservedValue::Primitive(Value::Bool(true)))],
    };
    assert_eq!(
        anonymous.canonical_vibon(),
        "(record kind: @record fields: (record a: true))"
    );
    assert!(ObservedValue::Primitive(Value::I32(2)) == Value::I32(2));
}
