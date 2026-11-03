//! M4 Step 14: the effect row is part of the function type
//! (`docs/spec/03-effects.md`, "Function effect rows"; `docs/spec/06-runtime.md`,
//! "Canonical value encoding").

use vibra_ir::{EffectRoot, EffectRow, FunctionSignature, Type, canonical_type};

fn root(path: &str) -> EffectRoot {
    EffectRoot::new(format!("@pkg@1.0.0/{path}"), path)
}

fn signature(effects: EffectRow) -> FunctionSignature {
    FunctionSignature::new(vec![Type::I32], Type::I32).with_effects(effects)
}

#[test]
fn a_function_type_with_no_effects_writes_an_empty_row_record() {
    assert_eq!(
        canonical_type(&Type::Function(Box::new(signature(EffectRow::empty())))),
        "(record type: @fn parameters: (array @i32) labelled: (record) result: @i32 \
         effects: (record roots: (array)))"
    );
}

#[test]
fn a_row_writes_its_canonical_roots_sorted_after_the_result() {
    let row = EffectRow::new([
        root("std.io.stdout"),
        root("app.audit"),
        root("std.fs.write"),
    ]);
    assert_eq!(
        canonical_type(&Type::Function(Box::new(signature(row)))),
        "(record type: @fn parameters: (array @i32) labelled: (record) result: @i32 \
         effects: (record roots: (array @app.audit @std.fs.write @std.io.stdout)))"
    );
}

#[test]
fn the_variadic_tail_still_precedes_the_result_and_the_row_follows_it() {
    let tail = Type::Array(Box::new(Type::I32));
    let function = signature(EffectRow::new([root("app.audit")])).with_variadic(tail);
    let encoded = canonical_type(&Type::Function(Box::new(function)));
    let variadic = encoded.find("variadic:").expect("variadic field");
    let result = encoded.find("result:").expect("result field");
    let effects = encoded.find("effects:").expect("effects field");
    assert!(variadic < result && result < effects, "{encoded}");
}

#[test]
fn rows_take_part_in_function_type_equality_in_both_directions() {
    let empty = Type::Function(Box::new(signature(EffectRow::empty())));
    let audit =
        Type::Function(Box::new(signature(EffectRow::new([root("app.audit")]))));
    assert_ne!(empty, audit);
    assert!(!empty.same_shape(&audit));
    assert!(!audit.same_shape(&empty));
    assert!(!empty.admits(&audit));
    assert!(!audit.admits(&empty));
    assert!(audit.same_shape(&audit.clone()));
}

#[test]
fn textually_equal_roots_of_two_packages_are_two_rows() {
    let one = EffectRow::new([EffectRoot::new("@one@1.0.0/app.log", "app.log")]);
    let two = EffectRow::new([EffectRoot::new("@two@1.0.0/app.log", "app.log")]);
    assert_ne!(one, two);
    assert!(
        !Type::Function(Box::new(signature(one)))
            .same_shape(&Type::Function(Box::new(signature(two))))
    );
}

#[test]
fn substitution_keeps_the_row() {
    let row = EffectRow::new([root("app.audit")]);
    let function =
        FunctionSignature::new(vec![Type::Param("t".to_owned())], Type::Void)
            .with_effects(row.clone());
    let instantiated = function.substitute(&std::collections::BTreeMap::from([(
        "t".to_owned(),
        Type::I32,
    )]));
    assert_eq!(instantiated.effects(), &row);
}

#[test]
fn a_display_of_a_function_type_names_a_nonempty_row() {
    let plain = Type::Function(Box::new(signature(EffectRow::empty())));
    let effectful =
        Type::Function(Box::new(signature(EffectRow::new([root("app.audit")]))));
    assert_eq!(plain.to_string(), "(fn (i32) i32)");
    assert_eq!(effectful.to_string(), "(fn (i32) i32 effects: (app.audit))");
}
