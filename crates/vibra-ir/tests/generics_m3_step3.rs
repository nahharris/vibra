//! M3 Step 3 generic IR: applied declared types, erased generic parameters,
//! and the validation that checks each construction against its instantiated
//! definition.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;

use vibra_diagnostics::ByteSpan;
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, IrError, SourceOrigin,
    Type, TypeBody, TypeDefinition, TypeId, Value, canonical_type,
};

fn origin() -> SourceOrigin {
    SourceOrigin::new("case.vib", ByteSpan::new(0, 1))
}

fn holder_id() -> TypeId {
    TypeId::new("@demo@1.0.0/app.main.holder", "app.main.holder")
}

/// `(deftype holder (record value t) where: (t any))`
fn holder() -> TypeDefinition {
    TypeDefinition::new(
        holder_id(),
        TypeBody::Record(vec![("value".to_owned(), Type::Param("t".to_owned()))]),
    )
    .with_parameters(vec!["t".to_owned()])
}

fn holder_of(argument: Type) -> Type {
    Type::Applied(holder_id(), vec![argument])
}

fn construct(value_type: Type, value: Value) -> Expr {
    Expr::Record {
        value_type,
        fields: vec![("value".to_owned(), Expr::literal(value, origin()))],
        origin: origin(),
    }
}

fn program(result: Type, body: Expr) -> Result<CheckedProgram, IrError> {
    let function = CheckedFunction::new(
        "main",
        FunctionSignature::new(Vec::new(), result),
        body,
        origin(),
    )?;
    CheckedProgram::try_new_with_types(vec![holder()], Vec::new(), vec![function], 0)
}

#[test]
fn an_applied_construction_and_projection_validate() {
    let project = Expr::Project {
        record: Box::new(construct(holder_of(Type::I32), Value::I32(1))),
        field: "value".to_owned(),
        value_type: Type::I32,
        origin: origin(),
    };
    program(Type::I32, project).expect("valid program");
}

#[test]
fn a_construction_disagreeing_with_its_instantiation_is_rejected() {
    let wrong = construct(holder_of(Type::I32), Value::Str("x".to_owned()));
    assert!(program(holder_of(Type::I32), wrong).is_err());
}

#[test]
fn an_applied_type_with_the_wrong_arity_is_rejected() {
    let bare = construct(Type::Declared(holder_id()), Value::I32(1));
    assert!(program(Type::Declared(holder_id()), bare).is_err());
    let doubled = Type::Applied(holder_id(), vec![Type::I32, Type::Str]);
    let wide = construct(doubled.clone(), Value::I32(1));
    assert!(program(doubled, wide).is_err());
}

#[test]
fn instantiation_substitutes_every_parameter() {
    assert_eq!(
        holder().instantiate(&[Type::Str]),
        Some(TypeBody::Record(vec![("value".to_owned(), Type::Str)]))
    );
    assert_eq!(holder().instantiate(&[]), None);
    let arguments = BTreeMap::from([("t".to_owned(), Type::Bool)]);
    assert_eq!(
        holder_of(Type::Param("t".to_owned())).substitute(&arguments),
        holder_of(Type::Bool)
    );
}

#[test]
fn an_erased_parameter_admits_any_type_and_nothing_else_widens() {
    let parameter = Type::Param("t".to_owned());
    assert!(parameter.admits(&Type::I32));
    assert!(holder_of(parameter).admits(&holder_of(Type::Str)));
    assert!(!holder_of(Type::I32).admits(&holder_of(Type::Str)));
}

#[test]
fn an_applied_type_encodes_its_arguments() {
    assert_eq!(
        canonical_type(&holder_of(Type::I32)),
        "(record type: @app.main.holder arguments: (array @i32))"
    );
}
