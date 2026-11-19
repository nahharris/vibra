//! The flat module scope, member namespaces, headers, prelude reservations,
//! and casing.

use vibra_diagnostics::DiagnosticCode as C;

use crate::support::{assert_clean, assert_one, codes, resolve_main, span_of};

#[test]
fn a_declaration_may_be_referenced_before_it_is_written() {
    assert_clean(&resolve_main("type A of B endtype\ntype B of U8 endtype\n"));
}

#[test]
fn a_repeated_top_level_name_is_a_redeclaration_at_the_later() {
    let text = "type A of U8 endtype\ntype A of U16 endtype\n";
    assert_one(
        &resolve_main(text),
        C::NameRedeclaration,
        span_of(text, "A", 1),
        &[span_of(text, "A", 0)],
    );
}

#[test]
fn names_collide_across_namespaces() {
    let text = "type limit of U8 endtype\nlet limit: U8 be 1u8 endlet\n";
    let resolution = resolve_main(text);
    assert_eq!(codes(&resolution), [C::StyleCasing, C::NameRedeclaration]);
}

#[test]
fn an_import_name_collides_with_a_declaration() {
    let text = "use @std.io enduse\nlet io: U8 be 1u8 endlet\n";
    assert_one(
        &resolve_main(text),
        C::NameRedeclaration,
        span_of(text, "io", 1),
        &[span_of(text, "io", 0)],
    );
}

#[test]
fn two_imports_with_one_name_collide() {
    let text = "use @std.io enduse\nuse @std.core as io enduse\n";
    assert_one(
        &resolve_main(text),
        C::NameRedeclaration,
        span_of(text, "io", 1),
        &[span_of(text, "io", 0)],
    );
}

#[test]
fn a_repeated_test_name_is_a_redeclaration() {
    let text = "test \"one\" do\n  void\nendtest\n\ntest \"one\" do\n  void\nendtest\n";
    let resolution = crate::support::resolve_modules(&[("@tests.t", text)]);
    let found = crate::support::flat(&resolution);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, C::NameRedeclaration);
    assert_eq!((found[0].2, found[0].3), span_of(text, "\"one\"", 1));
}

#[test]
fn fields_cases_and_members_share_one_namespace() {
    let text = "type U of {name: Str}\n  fn name takes {u: Self} returns s: Str do return u.name endfn\nendtype\n";
    assert_one(
        &resolve_main(text),
        C::NameMemberCollision,
        span_of(text, "name", 1),
        &[span_of(text, "name", 0)],
    );
    let cases = "type Shape\n  case empty\n  case empty\nendtype\n";
    assert_one(
        &resolve_main(cases),
        C::NameMemberCollision,
        span_of(cases, "empty", 1),
        &[span_of(cases, "empty", 0)],
    );
    let fields = "type Point of {x: I32 x: I32} endtype\n";
    assert_one(
        &resolve_main(fields),
        C::NameMemberCollision,
        span_of(fields, "x", 1),
        &[span_of(fields, "x", 0)],
    );
}

#[test]
fn an_associated_type_and_a_member_of_one_interface_collide() {
    let text = "interface Source\n  type item endtype\n  fn item takes {s: Self} returns r: U8\n  endfn\nendinterface\n";
    let resolution = resolve_main(text);
    assert!(
        codes(&resolution).contains(&C::NameMemberCollision),
        "{:#?}",
        crate::support::flat(&resolution)
    );
}

#[test]
fn a_nested_member_redeclaring_an_inherited_parameter() {
    let text = "type Box(T) of {item: T}\n  fn f(T) takes {b: Self} returns r: T do return b.item endfn\nendtype\n";
    assert_one(
        &resolve_main(text),
        C::NameGenericRedeclaration,
        span_of(text, "T", 2),
        &[],
    );
}

#[test]
fn an_impl_member_redeclaring_the_types_parameter() {
    let text = "type Box(T) of {item: T}\n  impl Equatable\n    fn equal(T) takes {left: Self right: Self} returns same: Bool do return true endfn\n  endimpl\nendtype\n";
    assert_one(
        &resolve_main(text),
        C::NameGenericRedeclaration,
        span_of(text, "T", 2),
        &[],
    );
}

#[test]
fn a_repeated_type_parameter() {
    let text = "type Pair(A A) of {left: A right: A} endtype\n";
    assert_one(
        &resolve_main(text),
        C::NameGenericRedeclaration,
        span_of(text, "A", 1),
        &[],
    );
}

#[test]
fn given_keys_name_the_headers_own_parameters_once() {
    let unknown = "type Box(T) of {item: T} given {U: Any} endtype\n";
    assert_one(
        &resolve_main(unknown),
        C::NameUnknownSymbol,
        span_of(unknown, "U", 0),
        &[],
    );
    let repeated = "type Box(T) of {item: T} given {T: Any T: Ordered} endtype\n";
    assert_one(
        &resolve_main(repeated),
        C::NameRedeclaration,
        span_of(repeated, "T", 3),
        &[span_of(repeated, "T", 2)],
    );
    let inherited = "type Box(T) of {item: T}\n  fn f returns r: U8 given {T: Any} do return 1u8 endfn\nendtype\n";
    assert_one(
        &resolve_main(inherited),
        C::NameUnknownSymbol,
        span_of(inherited, "T", 2),
        &[],
    );
}

#[test]
fn an_associated_type_with_the_name_of_a_parameter() {
    let text = "interface Source(Item) type Item endtype endinterface\n";
    assert_one(
        &resolve_main(text),
        C::NameGenericRedeclaration,
        span_of(text, "Item", 1),
        &[span_of(text, "Item", 0)],
    );
}

#[test]
fn prelude_names_are_reserved() {
    let ty = "type Option of U8 endtype\n";
    assert_one(
        &resolve_main(ty),
        C::NameReservedDeclaration,
        span_of(ty, "Option", 0),
        &[],
    );
    let interface = "interface Any endinterface\n";
    assert_one(
        &resolve_main(interface),
        C::NameReservedDeclaration,
        span_of(interface, "Any", 0),
        &[],
    );
    let parameter = "type Box(Option) of {item: U8} endtype\n";
    assert_one(
        &resolve_main(parameter),
        C::NameReservedDeclaration,
        span_of(parameter, "Option", 0),
        &[],
    );
    let value = "let true: U8 be 1u8 endlet\n";
    assert_one(
        &resolve_main(value),
        C::NameReservedValueSpelling,
        span_of(value, "true", 0),
        &[],
    );
    let function = "fn false returns r: U8 do return 1u8 endfn\n";
    assert_one(
        &resolve_main(function),
        C::NameReservedValueSpelling,
        span_of(function, "false", 0),
        &[],
    );
    let self_type = "type Self of U8 endtype\n";
    assert_one(
        &resolve_main(self_type),
        C::NameReservedDeclaration,
        span_of(self_type, "Self", 0),
        &[],
    );
}

#[test]
fn a_reserved_declaration_still_binds() {
    let text = "type Option of U8 endtype\ntype A of Option endtype\n";
    let resolution = resolve_main(text);
    assert_eq!(codes(&resolution), [C::NameReservedDeclaration]);
    assert_eq!(resolution.references().len(), 1);
}

#[test]
fn an_import_spelled_as_a_prelude_name() {
    let resolution = crate::support::resolve_modules(&[
        ("@app.main", "use @app.util as true enduse\n"),
        ("@app.util", ""),
    ]);
    assert_eq!(codes(&resolution), [C::NameReservedValueSpelling]);
    assert_eq!(
        crate::support::flat(&resolution)[0].2..crate::support::flat(&resolution)[0].3,
        17..21
    );
}

#[test]
fn casing_is_a_warning_and_changes_nothing() {
    let text = "type user of U8 endtype\n";
    let resolution = resolve_main(text);
    assert_one(&resolution, C::StyleCasing, span_of(text, "user", 0), &[]);
    assert!(!resolution.has_errors());
}

#[test]
fn casing_covers_members_fields_cases_parameters_and_aliases() {
    let text = "use @app.model as Model enduse\n\ntype Shape(item)\n  case Circle {Radius: F64}\n  fn Area takes {s: Self} returns a: F64 do return 0.0f64 endfn\nendtype\n\nlet Limit: U8 be 1u8 endlet\n";
    let resolution = crate::support::resolve_modules(&[
        ("@app.main", text),
        (
            "@app.model",
            "type User of U8\n  visibility @public\nendtype\n",
        ),
    ]);
    let spans = crate::support::flat(&resolution)
        .into_iter()
        .map(|(code, _, start, end, _)| (code, start, end))
        .collect::<Vec<_>>();
    let expected = ["Model", "item", "Circle", "Radius", "Area", "Limit"]
        .iter()
        .map(|name| {
            let (start, end) = span_of(text, name, 0);
            (C::StyleCasing, start, end)
        })
        .collect::<Vec<_>>();
    assert_eq!(spans, expected);
    let alias = crate::support::resolve_modules(&[
        ("@app.main", "use @app.model.User as person enduse\n"),
        (
            "@app.model",
            "type User of U8\n  visibility @public\nendtype\n",
        ),
    ]);
    assert_eq!(codes(&alias), [C::StyleCasing]);
}
