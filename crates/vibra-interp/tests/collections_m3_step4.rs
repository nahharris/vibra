//! M3 Step 4 collections in the reference interpreter: canonical map order,
//! later-key replacement, and lookups that never trap.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use vibra_diagnostics::ByteSpan;
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, SourceOrigin, Type,
    TypeBody, TypeDefinition, TypeId, Value,
};

fn origin() -> SourceOrigin {
    SourceOrigin::new("case.vib", ByteSpan::new(0, 1))
}

fn literal(value: Value) -> Expr {
    Expr::literal(value, origin())
}

fn map_type() -> Type {
    Type::Map(Box::new(Type::Str), Box::new(Type::I32))
}

fn map_of(entries: &[(&str, i32)]) -> Expr {
    Expr::Map {
        value_type: map_type(),
        entries: entries
            .iter()
            .map(|(key, value)| {
                (
                    literal(Value::Str((*key).to_owned())),
                    literal(Value::I32(*value)),
                )
            })
            .collect(),
        origin: origin(),
    }
}

fn run(result: Type, body: Expr) -> String {
    let function = CheckedFunction::new(
        "main",
        FunctionSignature::new(Vec::new(), result),
        body,
        origin(),
    )
    .expect("function");
    // The standard option, which lookups answer with.
    let option = TypeDefinition::new(
        TypeId::new(vibra_ir::OPTION_ID, vibra_ir::OPTION_PATH),
        TypeBody::Enum(vec![
            ("some".to_owned(), Type::Param("t".to_owned())),
            ("none".to_owned(), Type::Void),
        ]),
    )
    .with_parameters(vec!["t".to_owned()]);
    let program =
        CheckedProgram::try_new_with_types(vec![option], Vec::new(), vec![function], 0)
            .expect("program");
    vibra_interp::run(&program)
        .expect("execution")
        .canonical_result()
}

#[test]
fn every_insertion_order_yields_one_canonical_encoding() {
    let orders: [&[(&str, i32)]; 4] = [
        &[("a", 1), ("b", 2), ("c", 3)],
        &[("c", 3), ("b", 2), ("a", 1)],
        &[("b", 2), ("c", 3), ("a", 1)],
        // A later duplicate key replaces the earlier value.
        &[("c", 9), ("a", 1), ("b", 2), ("c", 3)],
    ];
    let encodings = orders
        .iter()
        .map(|entries| run(map_type(), map_of(entries)))
        .collect::<Vec<_>>();
    assert!(
        encodings.iter().all(|encoding| *encoding == encodings[0]),
        "{encodings:?}"
    );
    assert!(encodings[0].contains(
        "(record kind: @map entries: (array (tuple \"a\" 1i32) (tuple \"b\" 2i32) (tuple \"c\" 3i32)))"
    ));
}

#[test]
fn an_out_of_range_lookup_answers_none() {
    let lookup = Expr::Lookup {
        collection: Box::new(Expr::Array {
            value_type: Type::Array(Box::new(Type::I32)),
            elements: vec![literal(Value::I32(7))],
            origin: origin(),
        }),
        key: Box::new(literal(Value::U64(5))),
        value_type: vibra_ir::option_type(Type::I32),
        origin: origin(),
    };
    let encoding = run(vibra_ir::option_type(Type::I32), lookup);
    assert!(encoding.contains("variant: @none"), "{encoding}");
}
