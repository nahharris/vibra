//! Step 8 declaration formatting tests.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use vibra_fmt::format_source;
use vibra_syntax::parse_source;

#[test]
fn formatter_orders_declaration_attributes_before_body_forms() {
    let source =
        "(defn f () i32 doc: \"docs\" visibility: @public where: (t any) 0i32)";
    let formatted =
        format_source(Path::new("declarations.vib"), source).expect("source mode");
    assert_eq!(
        formatted,
        "(defn f () i32 where: (t any) visibility: @public doc: \"docs\" 0i32)\n"
    );
    assert_eq!(
        format_source(Path::new("declarations.vib"), &formatted)
            .expect("idempotent formatting"),
        formatted
    );
}

#[test]
fn formatter_reorders_complete_attribute_groups_with_comments() {
    let source = "(defn f () i32 doc: \"docs\" ; doc comment\n visibility: @public where: (t any) 0i32)";
    let expected = "(\n  defn\n  f\n  ()\n  i32\n  where:\n  (t any)\n  visibility:\n  @public\n  doc:\n  \"docs\"\n  ; doc comment\n  0i32\n)\n";
    let formatted =
        format_source(Path::new("declarations.vib"), source).expect("source mode");
    assert_eq!(formatted, expected);
    assert_eq!(
        format_source(Path::new("declarations.vib"), &formatted)
            .expect("idempotent formatting"),
        formatted
    );
    assert!(
        parse_source(Path::new("declarations.vib"), &formatted)
            .expect("source loader")
            .accepted()
    );
}

#[test]
fn formatter_places_type_methods_before_impl_blocks() {
    let source = "(deftype box void (impl (box i32) (defn make () box 0i32)) (defn map (value box) box value))";
    let formatted =
        format_source(Path::new("declarations.vib"), source).expect("source mode");
    let method = formatted.find("(defn map").expect("method is present");
    let implementation = formatted.find("(impl").expect("implementation is present");
    assert!(method < implementation, "methods must precede impl blocks");
    assert_eq!(
        format_source(Path::new("declarations.vib"), &formatted)
            .expect("idempotent formatting"),
        formatted
    );
}
