//! Structural source-position query coverage for Step 10.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::Path;

use vibra_syntax::{
    FactStatus, GrammarCategory, QueryError, SyntaxKind, parse_data, parse_source,
};

fn offset(source: &str, needle: &str) -> usize {
    source
        .find(needle)
        .expect("fixture contains the queried text")
}

#[test]
fn source_queries_report_contextual_slots_and_exact_continuations() {
    let source =
        "; 🌱 prefix\n(defn greet (value str) str visibility: @public (as str value))";
    let document = parse_source(Path::new("query.vib"), source).expect("source loader");

    let head = document
        .query_position(offset(source, "defn"))
        .expect("declaration head query");
    assert_eq!(head.category(), GrammarCategory::Declaration);
    assert_eq!(head.status(), FactStatus::Exact);
    assert_eq!(head.syntax_kind(), SyntaxKind::Atom);
    assert_eq!(
        head.permitted_forms().unwrap_or_default(),
        [
            "import", "deftype", "defint", "deffect", "def", "defn", "test"
        ]
    );

    let pattern = document
        .query_position(offset(source, "value"))
        .expect("pattern query");
    assert_eq!(pattern.category(), GrammarCategory::Pattern);
    assert_eq!(pattern.span().text(source), Some("value"));

    let value_type = document
        .query_position(offset(source, "str"))
        .expect("type query");
    assert_eq!(value_type.category(), GrammarCategory::Type);
    assert_eq!(
        value_type.permitted_forms().unwrap_or_default(),
        ["tuple", "array", "map", "fn"]
    );

    let attribute = document
        .query_position(offset(source, "visibility:"))
        .expect("attribute query");
    assert_eq!(attribute.category(), GrammarCategory::DeclarationAttribute);
    assert_eq!(
        attribute.permitted_labels().unwrap_or_default(),
        [
            "where",
            "labelled",
            "variadic",
            "visibility",
            "effects",
            "external",
            "symbol",
            "doc"
        ]
    );

    let expression = document
        .query_position(source.rfind("value").expect("body value"))
        .expect("expression query");
    assert_eq!(expression.category(), GrammarCategory::Expression);
    assert_eq!(expression.span().text(source), Some("value"));

    let comment = document
        .query_position(offset(source, "🌱"))
        .expect("comment query");
    assert_eq!(comment.category(), GrammarCategory::Trivia);
    assert_eq!(comment.status(), FactStatus::Unavailable);
    assert!(comment.permitted_forms().is_none());
    assert!(comment.permitted_labels().is_none());

    let eof = document.query_position(source.len()).expect("EOF query");
    assert_eq!(eof.offset(), source.len());
    assert_eq!(eof.category(), GrammarCategory::Declaration);
    assert_eq!(eof.span().end(), source.len());
}

#[test]
fn source_queries_reject_invalid_offsets_without_slicing_utf8() {
    let source = "(α)";
    let document =
        parse_source(Path::new("unicode.vib"), source).expect("source loader");

    assert_eq!(
        document.query_position(2),
        Err(QueryError::InteriorUtf8Offset { offset: 2 })
    );
    assert_eq!(
        document.query_position(source.len().saturating_add(1)),
        Err(QueryError::OffsetOutOfBounds {
            offset: source.len().saturating_add(1),
            length: source.len(),
        })
    );
}

#[test]
fn source_queries_prefer_zero_width_recovery_and_keep_siblings_exact() {
    let source = "(defn good (value str) str value) (defn broken (value str) str";
    let document =
        parse_source(Path::new("recovered.vib"), source).expect("source loader");

    let sibling = document
        .query_position(offset(source, "good"))
        .expect("valid sibling query");
    assert_eq!(sibling.status(), FactStatus::Exact);
    assert_eq!(sibling.category(), GrammarCategory::Declaration);

    let eof = document
        .query_position(source.len())
        .expect("recovery query");
    assert_eq!(eof.syntax_kind(), SyntaxKind::Error);
    assert_eq!(eof.category(), GrammarCategory::Recovery);
    assert_eq!(eof.status(), FactStatus::Recovered);
    assert!(eof.span().is_empty());
}

#[test]
fn data_queries_report_record_slots_and_container_forms() {
    let source = "(record name: (array 1))";
    let document = parse_data(Path::new("query.vibon"), source).expect("data loader");

    let label = document
        .query_position(offset(source, "name:"))
        .expect("data label query");
    assert_eq!(label.category(), GrammarCategory::DataField);
    assert_eq!(label.status(), FactStatus::Exact);

    let container = document
        .query_position(offset(source, "array"))
        .expect("data container query");
    assert_eq!(container.category(), GrammarCategory::DataField);
    assert_eq!(
        container.permitted_forms().unwrap_or_default(),
        ["record", "array", "tuple", "map"]
    );

    let repeated = document
        .query_position(offset(source, "array"))
        .expect("deterministic query");
    assert_eq!(container, repeated);
}

#[test]
fn source_queries_keep_application_and_lambda_slots_distinct() {
    let source = "(defn f (value str) str (call types: (str) value))\n(defn g (value str) str (lambda (x str) str effects: (@read) x))";
    let document =
        parse_source(Path::new("nested-query.vib"), source).expect("source loader");

    let types_label = source.find("types:").expect("types label");
    let types_query = document
        .query_position(types_label)
        .expect("application label query");
    assert_eq!(types_query.category(), GrammarCategory::Expression);
    assert_eq!(
        types_query.permitted_labels().unwrap_or_default(),
        ["types"]
    );

    let types_value = source
        .match_indices("str")
        .nth(2)
        .map(|(start, _)| start)
        .expect("type argument");
    let type_argument = document
        .query_position(types_value)
        .expect("type argument query");
    assert_eq!(type_argument.category(), GrammarCategory::Type);

    let effect = source.find("@read").expect("effect reference");
    let effect_query = document.query_position(effect).expect("effect query");
    assert_eq!(effect_query.category(), GrammarCategory::EffectRow);
    assert_eq!(effect_query.status(), FactStatus::Exact);

    let lambda_label = source.find("effects:").expect("lambda effects label");
    let lambda_attribute = document
        .query_position(lambda_label)
        .expect("lambda attribute query");
    assert_eq!(
        lambda_attribute.category(),
        GrammarCategory::DeclarationAttribute
    );
    assert_eq!(
        lambda_attribute.permitted_labels().unwrap_or_default(),
        ["labelled", "variadic", "effects"]
    );
}

#[test]
fn source_queries_report_type_field_and_type_attribute_contexts() {
    let source = "(deftype box (record value str) visibility: @public)";
    let document =
        parse_source(Path::new("type-query.vib"), source).expect("source loader");

    let field = document
        .query_position(source.find("value").expect("field"))
        .expect("field query");
    assert_eq!(field.category(), GrammarCategory::DataField);
    assert!(
        field
            .permitted_forms()
            .is_some_and(|forms| forms.is_empty())
    );

    let attribute = document
        .query_position(source.find("visibility:").expect("attribute"))
        .expect("attribute query");
    assert_eq!(attribute.category(), GrammarCategory::DeclarationAttribute);
    assert_eq!(
        attribute.permitted_labels().unwrap_or_default(),
        ["where", "visibility", "doc"]
    );
}
