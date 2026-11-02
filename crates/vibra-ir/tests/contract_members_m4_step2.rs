//! M4 Step 2 typed IR for a contract member with its own generic parameters:
//! the call carries the member's type arguments, each implementation declares
//! one slot per generic parameter, and the canonical form shows the list.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use vibra_diagnostics::ByteSpan;
use vibra_ir::{
    CallTarget, CheckedFunction, CheckedProgram, Expr, FunctionSignature, Implements,
    IrError, SourceOrigin, Type, TypeId, Value,
};

fn origin() -> SourceOrigin {
    SourceOrigin::new("case.vib", ByteSpan::new(0, 1))
}

fn interface() -> TypeId {
    TypeId::new("@demo@1.0.0/app.main.shape", "app.main.shape")
}

/// The member `(defn tag (value self other u) i32 where: (u any))` at `i32`.
fn signature() -> FunctionSignature {
    FunctionSignature::new(vec![Type::I32, Type::Str], Type::I32)
}

fn implementation(member_generics: Vec<Option<String>>) -> CheckedFunction {
    let body = Expr::literal(Value::I32(7), origin());
    CheckedFunction::new("shape.impl.tag", signature(), body, origin())
        .expect("function")
        .with_implements(Implements {
            interface: interface(),
            member: "tag".to_owned(),
            receiver: Type::I32,
            arguments: Vec::new(),
            member_generics,
        })
}

fn call(member_types: Vec<Type>) -> Expr {
    Expr::Call {
        target: CallTarget::Contract {
            interface: interface(),
            member: "tag".to_owned(),
            receiver: 0,
            arguments: Vec::new(),
            member_types,
            destination: None,
            signature: Box::new(signature()),
            closed: None,
        },
        arguments: vec![
            Expr::literal(Value::I32(1), origin()),
            Expr::literal(Value::Str("x".to_owned()), origin()),
        ],
        result: Type::I32,
        tail: false,
        origin: origin(),
    }
}

fn program(
    generics: Vec<Option<String>>,
    member_types: Vec<Type>,
) -> Result<CheckedProgram, IrError> {
    let main = CheckedFunction::new(
        "main",
        FunctionSignature::new(Vec::new(), Type::I32),
        call(member_types),
        origin(),
    )?;
    CheckedProgram::try_new(vec![main, implementation(generics)], 0)
}

#[test]
fn a_call_passes_one_type_argument_for_each_generic_the_implementation_declares() {
    let program = program(vec![Some("u".to_owned())], vec![Type::Str]).expect("valid");
    let canonical = program.canonical_vibon();
    assert!(
        canonical.contains(
            "contract: @app.main.shape member: @tag receiver: 0u64 types: (array @str) result: @i32"
        ),
        "{canonical}"
    );
}

#[test]
fn a_non_generic_member_has_no_types_field() {
    let program = program(Vec::new(), Vec::new()).expect("valid");
    assert!(!program.canonical_vibon().contains("types: (array"));
}

#[test]
fn a_call_whose_type_arguments_disagree_with_an_implementation_is_rejected() {
    // Too few, and too many.
    for (generics, types) in [
        (vec![Some("u".to_owned())], Vec::new()),
        (Vec::new(), vec![Type::Str]),
        (vec![Some("u".to_owned())], vec![Type::Str, Type::I32]),
    ] {
        assert!(
            matches!(program(generics, types), Err(IrError::InvalidExpression(_))),
            "an implementation and a call must agree on the member's generics"
        );
    }
}

#[test]
fn an_implementation_may_leave_a_slot_the_call_still_fills() {
    // A generic the implementation's signature never mentions need not be
    // declared there, but the slot is kept so positions stay aligned.
    program(vec![None], vec![Type::Str]).expect("valid");
}
