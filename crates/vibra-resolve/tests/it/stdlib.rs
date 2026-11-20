//! The embedded unit `@std`: its modules resolve, a prelude name resolves to
//! its real declaration, and `Any` and `Self` are predeclared.

use vibra_diagnostics::DiagnosticCode as C;
use vibra_resolve::DeclarationKind;

use crate::support::{
    PRELUDE, assert_clean, codes, resolve_main, resolve_modules, span_of, std_identity,
};

/// The references of `resolution` as (start, end, identity).
fn references(resolution: &vibra_resolve::Resolution) -> Vec<(usize, usize, String)> {
    resolution
        .references()
        .iter()
        .map(|reference| {
            (
                reference.span().start(),
                reference.span().end(),
                reference.to().as_str().to_owned(),
            )
        })
        .collect()
}

#[test]
fn the_embedded_library_resolves_without_a_diagnostic_and_is_not_observed() {
    let resolution = resolve_modules(&[]);
    assert_clean(&resolution);
    assert!(resolution.modules().is_empty());
    assert!(resolution.declarations().is_empty());
    assert!(resolution.imports().is_empty());
    assert!(resolution.references().is_empty());
}

#[test]
fn a_prelude_name_resolves_to_its_declaration() {
    let text = "type A of Str endtype\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    let (start, end) = span_of(text, "Str", 0);
    assert_eq!(
        references(&resolution),
        [(
            start,
            end,
            std_identity("@std.str", DeclarationKind::Type, "Str")
        )]
    );
}

#[test]
fn every_prelude_name_of_a_type_resolves() {
    for (name, module, kind) in PRELUDE {
        if kind != DeclarationKind::Type {
            continue;
        }
        let text = format!("type A of {name} endtype\n");
        let resolution = resolve_main(&text);
        assert_clean(&resolution);
        let (start, end) = span_of(&text, name, 0);
        assert!(
            references(&resolution).contains(&(
                start,
                end,
                std_identity(module, kind, name)
            )),
            "{name}"
        );
    }
}

#[test]
fn any_is_predeclared_and_has_no_identity() {
    let resolution = resolve_main("type Box(T) of {item: T} given {T: Any} endtype\n");
    assert_clean(&resolution);
    assert!(resolution.references().is_empty());
    let text = "type A of Any.Item endtype\n";
    let resolution = resolve_main(text);
    assert_eq!(codes(&resolution), [C::NameUnknownSymbol]);
    let (start, end) = span_of(text, "Item", 0);
    let diagnostic = &resolution.diagnostics()[0];
    assert_eq!(
        (
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end()
        ),
        (start, end)
    );
}

#[test]
fn a_prelude_interface_resolves_as_a_bound() {
    let text = "type Box(T) of {item: T} given {T: Ordered} endtype\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    let (start, end) = span_of(text, "Ordered", 0);
    assert_eq!(
        references(&resolution),
        [(
            start,
            end,
            std_identity("@std.core", DeclarationKind::Interface, "Ordered")
        )]
    );
}

#[test]
fn a_prelude_name_is_reserved_in_every_local_module() {
    for name in PRELUDE
        .map(|(name, _, _)| name)
        .into_iter()
        .chain(["Any", "Self"])
    {
        if name.chars().next().is_some_and(char::is_lowercase) {
            continue;
        }
        let text = format!("type Box({name}) of {{item: U8}} endtype\n");
        assert_eq!(
            codes(&resolve_main(&text)),
            [C::NameReservedDeclaration],
            "{name}"
        );
    }
}
