//! Paths in type positions and section operands.

use vibra_diagnostics::DiagnosticCode as C;

use crate::support::{
    assert_clean, assert_one, codes, flat, resolve_main, resolve_modules, span_of,
};

#[test]
fn every_type_position_of_a_declaration_is_resolved() {
    for (text, name) in [
        ("type A of {x: Nope} endtype\n", "Nope"),
        ("type A\n  case one {x: Nope}\nendtype\n", "Nope"),
        ("type A of List(Nope) endtype\n", "Nope"),
        ("type A of Fn({Nope} U8) endtype\n", "Nope"),
        ("type A of Fn({U8} Nope) endtype\n", "Nope"),
        ("type A of {U8 Nope} endtype\n", "Nope"),
        (
            "fn f takes {x: Nope} returns r: U8 do return 1u8 endfn\n",
            "Nope",
        ),
        (
            "fn f labelled {x: Nope be 1u8} returns r: U8 do return 1u8 endfn\n",
            "Nope",
        ),
        ("fn f returns r: Nope do return 1u8 endfn\n", "Nope"),
        ("let limit: Nope be 1u8 endlet\n", "Nope"),
        ("type A of Iter(Item: Nope) endtype\n", "Nope"),
    ] {
        assert_one(
            &resolve_main(text),
            C::NameUnknownSymbol,
            span_of(text, name, 0),
            &[],
        );
    }
}

/// A row entry that names nothing is the effect checker's
/// `@effect.invalidReference` (`docs/spec/03-effects.md`, "Effect
/// ceilings"; `docs/spec/07-diagnostics-and-conformance.md`, "one that
/// resolves to nothing included"), so resolution reports none of them; a
/// type written beside the row is still resolved.
#[test]
fn resolution_leaves_a_row_entry_that_names_nothing_to_the_checker() {
    for text in [
        "proc p returns r: U8 performs {Nope} do return 1u8 endproc\n",
        "proc p returns r: U8 performs {Nope.Deeper} do return 1u8 endproc\n",
        "use @std.option enduse\n\
         proc p returns r: U8 performs {option.Nope} do return 1u8 endproc\n",
        "test \"t\" performs {Nope} do endtest\n",
    ] {
        assert_clean(&resolve_main(text));
    }
    let text =
        "proc p takes {x: Nope} returns r: U8 performs {Gone} do return 1u8 endproc\n";
    assert_one(
        &resolve_main(text),
        C::NameUnknownSymbol,
        span_of(text, "Nope", 0),
        &[],
    );
}

#[test]
fn a_bound_that_is_not_an_interface() {
    let text = "type Box(T) of {item: T} given {T: U8} endtype\n";
    assert_one(
        &resolve_main(text),
        C::NameWrongEntityKind,
        span_of(text, "U8", 0),
        &[],
    );
    let parameter =
        "fn f(T U) takes {x: T} returns r: U8 given {T: U} do return 1u8 endfn\n";
    assert_one(
        &resolve_main(parameter),
        C::NameWrongEntityKind,
        span_of(parameter, "U", 2),
        &[],
    );
    let interface = "interface Shown endinterface\ntype Box(T) of {item: T} given {T: Shown} endtype\n";
    assert_clean(&resolve_main(interface));
}

#[test]
fn an_impl_target_in_a_type_is_an_interface() {
    let text = "type A of U8 impl U16 endimpl endtype\n";
    assert_one(
        &resolve_main(text),
        C::NameWrongEntityKind,
        span_of(text, "U16", 0),
        &[],
    );
    assert_clean(&resolve_main(
        "type A of U8\n  impl Equatable\n  endimpl\nendtype\n",
    ));
}

#[test]
fn an_impl_target_in_an_interface_is_a_type() {
    let text = "interface Shown\n  impl Ordered\n  endimpl\nendinterface\n";
    assert_one(
        &resolve_main(text),
        C::NameWrongEntityKind,
        span_of(text, "Ordered", 0),
        &[],
    );
}

#[test]
fn a_value_or_a_module_in_a_type_position() {
    let text = "let limit: U8 be 1u8 endlet\ntype A of limit endtype\n";
    assert_one(
        &resolve_main(text),
        C::NameWrongEntityKind,
        span_of(text, "limit", 1),
        &[],
    );
    let module = resolve_modules(&[
        (
            "@app.main",
            "use @app.model enduse\n\ntype A of model endtype\n",
        ),
        ("@app.model", ""),
    ]);
    assert_eq!(codes(&module), [C::NameWrongEntityKind]);
}

#[test]
fn there_is_no_projection_path_through_a_type() {
    let text = "interface Source\n  type Item endtype\nendinterface\ntype Once of U8 endtype\ntype A of Once.Item endtype\ntype B of Source.Item endtype\n";
    let resolution = resolve_main(text);
    let found = flat(&resolution);
    let once = span_of(text, "Once", 0);
    let source = span_of(text, "Source", 0);
    let main = "src/app/main.vib".to_owned();
    let first = span_of(text, "Item", 1);
    let second = span_of(text, "Item", 2);
    assert_eq!(
        found,
        [
            (
                C::NameUnknownSymbol,
                main.clone(),
                first.0,
                first.1,
                vec![(main.clone(), once.0, once.1)]
            ),
            (
                C::NameUnknownSymbol,
                main.clone(),
                second.0,
                second.1,
                vec![(main, source.0, source.1)]
            ),
        ]
    );
}

#[test]
fn a_member_in_a_type_position_is_the_wrong_kind() {
    let text = "type Shape\n  case empty\nendtype\ntype A of Shape.empty endtype\n";
    assert_one(
        &resolve_main(text),
        C::NameWrongEntityKind,
        span_of(text, "Shape.empty", 0),
        &[],
    );
}

#[test]
fn an_associated_type_is_named_bare_inside_its_interface_and_impl() {
    let text = "interface Source\n  type Item endtype\n  fn next takes {s: Self} returns r: Item\n  endfn\nendinterface\ntype Once of U8\n  impl Source\n    type Item of U8 endtype\n    fn next takes {s: Self} returns r: Item do return 1u8 endfn\n  endimpl\nendtype\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    let item = resolution
        .declarations()
        .iter()
        .find(|declaration| declaration.anchor().name() == "Item")
        .unwrap()
        .identity()
        .clone();
    let to_item = resolution
        .references()
        .iter()
        .filter(|reference| reference.to() == &item)
        .count();
    assert_eq!(to_item, 2, "the interface's and the impl's bare `Item`");
}

#[test]
fn a_choice_does_not_see_its_own_impls_associated_names() {
    let text = "interface Source\n  type Item endtype\nendinterface\ntype Once of U8\n  impl Source\n    type Item of Option(Item) endtype\n  endimpl\nendtype\n";
    assert_one(
        &resolve_main(text),
        C::NameUnknownSymbol,
        span_of(text, "Item", 2),
        &[],
    );
}

#[test]
fn self_is_visible_only_inside_a_type_an_interface_and_an_impl() {
    assert_clean(&resolve_main(
        "type U of U8\n  fn f takes {u: Self} returns r: Self do return u endfn\nendtype\n",
    ));
    let text = "fn f takes {s: Self} returns r: U8 do return 1u8 endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameUnknownSymbol,
        span_of(text, "Self", 0),
        &[],
    );
}

#[test]
fn a_prelude_path_reaches_its_members() {
    let text = "type A of Option.some endtype\n";
    let resolution = resolve_main(text);
    let (start, end) = span_of(text, "Option.some", 0);
    assert_eq!(
        flat(&resolution),
        [(
            C::NameWrongEntityKind,
            "src/app/main.vib".to_owned(),
            start,
            end,
            Vec::new()
        )]
    );
    let text = "type A of Option.nothere endtype\n";
    let resolution = resolve_main(text);
    let (start, end) = span_of(text, "nothere", 0);
    let (_, option) = crate::support::STD_MODULES
        .iter()
        .find(|(path, _)| *path == "@std.option")
        .unwrap();
    let option = std::str::from_utf8(option).unwrap();
    let (name, name_end) = span_of(option, "Option", 0);
    assert_eq!(
        flat(&resolution),
        [(
            C::NameUnknownSymbol,
            "src/app/main.vib".to_owned(),
            start,
            end,
            vec![("@std.option".to_owned(), name, name_end)]
        )]
    );
}

#[test]
fn a_failed_import_reports_nothing_more_at_its_uses() {
    let resolution = resolve_modules(&[(
        "@app.main",
        "use @app.nothere enduse\n\ntype A of nothere.User endtype\n",
    )]);
    assert_eq!(codes(&resolution), [C::ModuleUnknownPath]);
}
