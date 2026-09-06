//! Step 8 declaration and type AST tests.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::Path;

use vibra_diagnostics::DiagnosticCode;
use vibra_syntax::{Declaration, DeftypeBody, TypeExpr, TypeMember, parse_source};

#[test]
fn all_seven_top_forms_and_nested_owners_have_native_structure() {
    let source = r#"
(import io @std.io)
(deftype user (record name str id u64)
  where: (t any)
  visibility: @public
  (defn map (value self) self value))
(defint printable
  (defn render (value self) str))
(deffect read
  (defn file (path str) str external: @host symbol: "fs.read"))
(def answer u64 1u64 visibility: @public)
(defn main (value i32) (fn (i32) i32) visibility: @public 0i32)
(test "smoke" 0i32)
"#;
    let document =
        parse_source(Path::new("declarations.vib"), source).expect("source loader");
    assert!(document.accepted(), "{:?}", document.diagnostics());
    let ast = document.ast().expect("declaration AST");
    assert_eq!(ast.declarations().len(), 7);

    let Declaration::Deftype(deftype) = &ast.declarations()[1] else {
        panic!("expected deftype")
    };
    assert!(matches!(deftype.body(), DeftypeBody::Record(fields) if fields.len() == 2));
    assert!(matches!(
        deftype.members().first(),
        Some(TypeMember::Method(method)) if method.name().value() == "map"
    ));

    let Declaration::Defn(function) = &ast.declarations()[5] else {
        panic!("expected defn")
    };
    assert!(matches!(function.result(), TypeExpr::Function(_)));
}

#[test]
fn type_expression_and_deftype_body_contexts_are_distinct() {
    let source = r#"
(deftype box (record value (array i32)))
(defn make (value (array i32)) (fn ((array i32)) (array i32)) value)
"#;
    let document = parse_source(Path::new("types.vib"), source).expect("source loader");
    assert!(document.accepted(), "{:?}", document.diagnostics());
    let ast = document.ast().expect("declaration AST");
    let Declaration::Deftype(deftype) = &ast.declarations()[0] else {
        panic!("expected deftype")
    };
    let DeftypeBody::Record(fields) = deftype.body() else {
        panic!("expected record body")
    };
    assert!(matches!(fields[0].ty(), TypeExpr::Array(_)));
    let Declaration::Defn(function) = &ast.declarations()[1] else {
        panic!("expected defn")
    };
    assert!(matches!(function.result(), TypeExpr::Function(_)));
}

#[test]
fn anonymous_type_bodies_in_type_positions_are_rejected() {
    let source = "(defn bad (value (record name str)) (array (enum one void)) 0i32)";
    let document =
        parse_source(Path::new("invalid-types.vib"), source).expect("source loader");
    assert!(!document.accepted());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::TypeAnonymousTypeBody
    }));
}

#[test]
fn reserved_type_heads_cannot_be_declaration_or_generic_names() {
    let source = r#"
(deftype map (newtype i32))
(defint array)
(deftype valid (newtype i32) where: (tuple any))
"#;
    let document =
        parse_source(Path::new("reserved.vib"), source).expect("source loader");
    assert_eq!(
        document
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code()
                == DiagnosticCode::NameReservedDeclaration)
            .count(),
        3
    );
}

#[test]
fn empty_bodies_and_nested_impls_are_structural_step8_forms() {
    let source = r#"
(deftype empty void)
(deftype packet (record tag str)
  (impl packet (defn encode () bytes 0i32)))
(defint marker
  (impl packet (defn inspect (value packet) str)))
(deffect none)
"#;
    let document =
        parse_source(Path::new("owners.vib"), source).expect("source loader");
    assert!(document.accepted(), "{:?}", document.diagnostics());
    let ast = document.ast().expect("declaration AST");
    assert!(matches!(ast.declarations()[0], Declaration::Deftype(_)));
    let Declaration::Deftype(packet) = &ast.declarations()[1] else {
        panic!("expected packet deftype")
    };
    assert!(matches!(
        packet.members().first(),
        Some(TypeMember::Implementation(_))
    ));
    let Declaration::Defint(marker) = &ast.declarations()[2] else {
        panic!("expected marker defint")
    };
    assert!(matches!(
        marker.members().first(),
        Some(TypeMember::Implementation(_))
    ));
    assert!(matches!(ast.declarations()[3], Declaration::Deffect(_)));
}

#[test]
fn malformed_declaration_shapes_are_rejected_without_reader_panics() {
    let cases = [
        "(impl i32 (defn m () i32 0i32))",
        "(import io)",
        "(defn f)",
        "(defn f (value) i32)",
        "(defn f () i32 labelled: (value i32))",
        "(defn f () i32 visibility: @public visibility: @private)",
        "(defn f () i32 0i32 visibility: @public)",
        "(defn f () i32 variadic: (rest (array i32) extra))",
        "(deftype value (union i32))",
        "(deftype value (newtype i32 i64))",
        "(deftype value (record field (record nested i32)))",
        "(defn qualified.name () i32)",
        "(test \"smoke\" effects: (@read) 0i32)",
        "(defn host () void external: @host symbol: \"host.op\")",
        "(deffect io (defn op () void external: @compiler symbol: \"io.op\"))",
        "(deffect io (defn op () void external: @host symbol: \"io.op\" effects: (other)))",
    ];
    for source in cases {
        let document =
            parse_source(Path::new("malformed.vib"), source).expect("source loader");
        assert!(!document.accepted(), "accepted malformed source: {source}");
    }
}

#[test]
fn type_body_variants_and_function_type_attributes_are_structured() {
    let source = r#"
(deftype status (enum ready void failed str))
(deftype value (union i32 str))
(deftype boxed (newtype (array i32)))
(defn collect () (fn (i32) str labelled: (limit i32) variadic: (array str) effects: (io)) "")
"#;
    let document =
        parse_source(Path::new("variants.vib"), source).expect("source loader");
    assert!(document.accepted(), "{:?}", document.diagnostics());
    let ast = document.ast().expect("declaration AST");
    assert!(matches!(
        &ast.declarations()[0],
        Declaration::Deftype(value) if matches!(value.body(), DeftypeBody::Enum(fields) if fields.len() == 2)
    ));
    assert!(matches!(
        &ast.declarations()[1],
        Declaration::Deftype(value) if matches!(value.body(), DeftypeBody::Union(members) if members.len() == 2)
    ));
    assert!(matches!(
        &ast.declarations()[2],
        Declaration::Deftype(value) if matches!(value.body(), DeftypeBody::Newtype(_))
    ));
    let Declaration::Defn(function) = &ast.declarations()[3] else {
        panic!("expected collect defn")
    };
    let TypeExpr::Function(signature) = function.result() else {
        panic!("expected function type")
    };
    assert_eq!(signature.labelled().len(), 1);
    assert!(signature.variadic().is_some());
    assert_eq!(signature.effects().len(), 1);
}

#[test]
fn member_namespace_collisions_and_literal_types_are_rejected() {
    let source = r#"
(deftype user (record name str)
  (defn name () str))
(defn invalid () 0i32)
"#;
    let document =
        parse_source(Path::new("member-collision.vib"), source).expect("source loader");
    assert!(!document.accepted());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::NameMemberCollision
    }));
    assert!(
        document.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::SyntaxInvalidForm
        })
    );
}
