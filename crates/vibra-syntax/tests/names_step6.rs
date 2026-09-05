//! Step 6 name-surface conformance tests.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::Path;

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_syntax::{NameClassification, NameKind, TokenKind, parse_data, parse_source};

fn name_tokens(document: &vibra_syntax::Document) -> Vec<(&str, NameKind)> {
    document
        .tokens()
        .iter()
        .filter_map(|token| match token.name() {
            Some(NameClassification::Name(name)) => Some((token.text(), name.kind())),
            Some(NameClassification::Invalid) | None => None,
        })
        .collect()
}

fn span_of(source: &str, text: &str, occurrence: usize) -> ByteSpan {
    let start = source
        .match_indices(text)
        .nth(occurrence)
        .expect("text occurrence")
        .0;
    ByteSpan::new(start, start + text.len())
}

#[test]
fn valid_names_expose_category_value_segments_and_raw_spelling() {
    let source = "(alpha alpha1 alpha- alpha--b a.b2-c some.name: @some.name - @- -:)";
    let document = parse_source(Path::new("names.vib"), source).expect("source mode");

    assert!(document.accepted(), "{:?}", document.diagnostics());
    let names = document
        .tokens()
        .iter()
        .filter_map(|token| token.name().map(|classification| (token, classification)))
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 10);

    let first = names[0].0;
    let Some(NameClassification::Name(name)) = first.name() else {
        panic!("first name classification")
    };
    assert_eq!(name.raw(), "alpha");
    assert_eq!(name.value(), "alpha");
    assert_eq!(name.segments(), ["alpha"]);
    assert_eq!(name.kind(), NameKind::Symbol);
    assert_eq!(first.span(), span_of(source, "alpha", 0));

    let label = names
        .iter()
        .find(|(token, _)| token.text() == "some.name:")
        .expect("label");
    let Some(NameClassification::Name(name)) = label.0.name() else {
        panic!("label classification")
    };
    assert_eq!(name.kind(), NameKind::Label);
    assert_eq!(name.value(), "some.name");
    assert_eq!(name.segments(), ["some", "name"]);

    let atom = names
        .iter()
        .find(|(token, _)| token.text() == "@some.name")
        .expect("atom");
    let Some(NameClassification::Name(name)) = atom.0.name() else {
        panic!("atom classification")
    };
    assert_eq!(name.kind(), NameKind::Atom);
    assert_eq!(name.value(), "some.name");

    for discard in ["-", "@-", "-:"] {
        let token = names
            .iter()
            .find(|(token, _)| token.text() == discard)
            .expect("discard");
        let Some(NameClassification::Name(name)) = token.0.name() else {
            panic!("discard classification")
        };
        assert_eq!(name.kind(), NameKind::Discard);
        assert!(name.is_discard());
        assert_eq!(name.raw(), discard);
    }
}

#[test]
fn malformed_names_report_complete_spans_and_keep_following_siblings() {
    let source = "(seed -a a..b a.1b Upper _x a/b a? a! -.name @-.name -:.name ok)";
    let document = parse_source(Path::new("names.vib"), source).expect("source mode");

    assert!(!document.accepted());
    let invalid = [
        ("-a", 0),
        ("a..b", 0),
        ("a.1b", 0),
        ("Upper", 0),
        ("_x", 0),
        ("a/b", 0),
        ("a?", 0),
        ("a!", 0),
        ("-.name", 0),
        ("@-.name", 0),
        ("-:.name", 0),
    ];
    assert_eq!(
        document
            .diagnostics()
            .iter()
            .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span()))
            .collect::<Vec<_>>(),
        invalid
            .iter()
            .map(|(text, occurrence)| {
                (
                    DiagnosticCode::SyntaxInvalidName,
                    span_of(source, text, *occurrence),
                )
            })
            .collect::<Vec<_>>()
    );

    let ok = document
        .tokens()
        .iter()
        .find(|token| token.text() == "ok")
        .expect("following valid sibling");
    assert!(matches!(ok.name(), Some(NameClassification::Name(_))));
    assert_eq!(ok.span(), span_of(source, "ok", 0));
}

#[test]
fn literal_diagnostics_take_precedence_over_name_validation() {
    let source = r#"(true void 1u128 1e+ "bad\q" \ab valid)"#;
    let document = parse_source(Path::new("names.vib"), source).expect("source mode");

    assert!(!document.accepted());
    assert_eq!(
        document
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code())
            .collect::<Vec<_>>(),
        vec![
            DiagnosticCode::SyntaxInvalidNumericLiteral,
            DiagnosticCode::SyntaxInvalidNumericLiteral,
            DiagnosticCode::SyntaxInvalidStringLiteral,
            DiagnosticCode::SyntaxInvalidCharacterLiteral,
        ]
    );
    assert!(
        document
            .tokens()
            .iter()
            .filter(|token| token.kind() == TokenKind::Atom)
            .filter(|token| token.text() != "valid")
            .all(|token| token.name().is_none())
    );
}

#[test]
fn the_name_surface_is_identical_in_source_and_data_modes() {
    let source = "(alpha some.name: @some.name - @- -: valid-name)";
    let source_document =
        parse_source(Path::new("names.vib"), source).expect("source mode");
    let data_document =
        parse_data(Path::new("names.vibon"), source).expect("data mode");

    assert!(source_document.accepted());
    assert!(data_document.accepted());
    assert_eq!(name_tokens(&source_document), name_tokens(&data_document));
}
