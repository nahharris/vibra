//! Step 5 literal-surface tests.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::Path;

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_syntax::{
    IntegerSuffix, Literal, LiteralClassification, TokenKind, classify, lex,
    parse_data, parse_source,
};

fn atom_texts(source: &str) -> Vec<String> {
    lex(source)
        .tokens()
        .iter()
        .filter(|token| token.kind() == TokenKind::Atom)
        .map(|token| token.text().to_owned())
        .collect()
}

fn span_of(source: &str, text: &str) -> ByteSpan {
    let start = source
        .find(text)
        .unwrap_or_else(|| panic!("{text:?} is not in the test source"));
    ByteSpan::sized(start, text.len())
}

#[test]
fn character_tokens_keep_delimiters_quotes_and_comments_attached() {
    let source = r##"(\) \( \; \" \🌱)"##;
    let lexed = lex(source);

    assert!(
        lexed.diagnostics().is_empty(),
        "character delimiters must not become parser delimiters or comments"
    );
    assert_eq!(
        atom_texts(source),
        vec![
            r"\)".to_owned(),
            r"\(".to_owned(),
            r"\;".to_owned(),
            r#"\""#.to_owned(),
            r"\🌱".to_owned(),
        ]
    );
    assert!(
        parse_source(Path::new("characters.vib"), source)
            .expect("source mode")
            .accepted()
    );
}

#[test]
fn malformed_character_literals_report_the_complete_token() {
    let source = r##"seed (\ab \newline-x \uD800 \u12 \u12345) next"##;
    let document =
        parse_source(Path::new("characters.vib"), source).expect("source mode");

    assert!(!document.accepted());
    assert_eq!(
        document
            .diagnostics()
            .iter()
            .map(|diagnostic| (diagnostic.code().as_atom(), diagnostic.primary_span()))
            .collect::<Vec<_>>(),
        vec![
            ("@syntax.invalid-character-literal", span_of(source, r"\ab")),
            (
                "@syntax.invalid-character-literal",
                span_of(source, r"\newline-x")
            ),
            (
                "@syntax.invalid-character-literal",
                span_of(source, r"\uD800")
            ),
            (
                "@syntax.invalid-character-literal",
                span_of(source, r"\u12")
            ),
            (
                "@syntax.invalid-character-literal",
                span_of(source, r"\u12345")
            ),
        ]
    );
}

#[test]
fn malformed_numeric_candidates_report_the_complete_token() {
    let source = r##"seed (1u128 1.0i32 1e+ 1a 0x10 1_000 2f64x 3.) -1u8 2f64"##;
    let document = parse_source(Path::new("numbers.vib"), source).expect("source mode");

    assert!(!document.accepted());
    assert_eq!(
        document
            .diagnostics()
            .iter()
            .map(|diagnostic| (diagnostic.code().as_atom(), diagnostic.primary_span()))
            .collect::<Vec<_>>(),
        vec![
            ("@syntax.invalid-numeric-literal", span_of(source, "1u128")),
            ("@syntax.invalid-numeric-literal", span_of(source, "1.0i32")),
            ("@syntax.invalid-numeric-literal", span_of(source, "1e+")),
            ("@syntax.invalid-numeric-literal", span_of(source, "1a")),
            ("@syntax.invalid-numeric-literal", span_of(source, "0x10")),
            ("@syntax.invalid-numeric-literal", span_of(source, "1_000")),
            ("@syntax.invalid-numeric-literal", span_of(source, "2f64x")),
            ("@syntax.invalid-numeric-literal", span_of(source, "3.")),
        ]
    );
}

#[test]
fn terminated_malformed_strings_report_the_complete_quoted_leaf() {
    let source = r###"seed ("bad\q" "bad\u{}" "bad\u{D800}" "bad\u{110000}" "bad\u{1234567}") true"###;
    let document = parse_source(Path::new("strings.vib"), source).expect("source mode");

    assert!(!document.accepted());
    let malformed = [
        r#""bad\q""#,
        r#""bad\u{}""#,
        r#""bad\u{D800}""#,
        r#""bad\u{110000}""#,
        r#""bad\u{1234567}""#,
    ];
    assert_eq!(
        document
            .diagnostics()
            .iter()
            .map(|diagnostic| (diagnostic.code().as_atom(), diagnostic.primary_span()))
            .collect::<Vec<_>>(),
        malformed
            .iter()
            .map(|text| { ("@syntax.invalid-string-literal", span_of(source, text),) })
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_unterminated_string_with_an_unfinished_escape_has_only_recovery() {
    let source = r#"seed "unfinished\"#;
    let document = parse_source(Path::new("strings.vib"), source).expect("source mode");

    assert!(!document.accepted());
    assert!(document.recovered());
    assert_eq!(document.root().to_source(), source);
    assert_eq!(document.diagnostics().len(), 1);
    assert_eq!(
        document.diagnostics()[0].code(),
        DiagnosticCode::SyntaxUnmatchedDelimiter
    );
    assert_eq!(
        document.diagnostics()[0].primary_span(),
        ByteSpan::empty_at(source.len())
    );
    let recovered_quote = document
        .root()
        .children()
        .iter()
        .find(|node| node.leaf_text().is_some_and(|text| text.starts_with('"')))
        .expect("recovered quoted leaf");
    assert!(recovered_quote.literal().is_none());
}

#[test]
fn the_literal_surface_is_identical_in_source_and_data_modes() {
    let source =
        r###"seed (true false void \newline \u0061 1u8 25i32 2.5f64 2f64 "ok\n")"###;

    let source_document =
        parse_source(Path::new("input.vib"), source).expect("source loader");
    let data_document =
        parse_data(Path::new("input.vibon"), source).expect("data loader");
    assert!(
        source_document.accepted(),
        "source: {:?}",
        source_document.diagnostics()
    );
    assert!(
        data_document.accepted(),
        "data: {:?}",
        data_document.diagnostics()
    );
}

#[test]
fn valid_character_matrix_decodes_names_hex_and_astral_scalars() {
    let source =
        r##"(\a \0 \newline \return \space \tab \u0061 \u0041 \u00e9 \u00E9 \🌱)"##;
    let characters = lex(source)
        .tokens()
        .iter()
        .filter_map(|token| match token.literal() {
            Some(LiteralClassification::Literal(Literal::Character(character))) => {
                Some((token.text().to_owned(), character.value(), token.span()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        characters
            .iter()
            .map(|(_, value, _)| *value)
            .collect::<Vec<_>>(),
        vec!['a', '0', '\n', '\r', ' ', '\t', 'a', 'A', 'é', 'é', '🌱']
    );
    for (raw, _, span) in characters {
        assert_eq!(span.text(source), Some(raw.as_str()));
    }
}

#[test]
fn a_bare_backslash_and_whitespace_after_it_recover_before_valid_siblings() {
    let source = r##"(\ \a \u123 \u12345 \newline-x \tab)"##;
    let document =
        parse_source(Path::new("characters.vib"), source).expect("source mode");

    assert!(!document.accepted());
    assert_eq!(
        document
            .diagnostics()
            .iter()
            .map(|diagnostic| (diagnostic.code().as_atom(), diagnostic.primary_span()))
            .collect::<Vec<_>>(),
        vec![
            ("@syntax.invalid-character-literal", span_of(source, "\\")),
            (
                "@syntax.invalid-character-literal",
                span_of(source, r"\u123")
            ),
            (
                "@syntax.invalid-character-literal",
                span_of(source, r"\u12345")
            ),
            (
                "@syntax.invalid-character-literal",
                span_of(source, r"\newline-x")
            ),
        ]
    );
    assert_eq!(
        atom_texts(source),
        vec![
            "\\".to_owned(),
            r"\a".to_owned(),
            r"\u123".to_owned(),
            r"\u12345".to_owned(),
            r"\newline-x".to_owned(),
            r"\tab".to_owned(),
        ]
    );
}

#[test]
fn strings_decode_every_supported_escape_without_changing_raw_spelling() {
    let source =
        r###"("plain" "quote\"" "slash\\" "lf\n" "cr\r" "tab\t" "astral\u{1F600}")"###;
    let strings = lex(source)
        .tokens()
        .iter()
        .filter_map(|token| match token.literal() {
            Some(LiteralClassification::Literal(Literal::String(string))) => {
                Some((token.text().to_owned(), string.value().to_owned()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        strings,
        vec![
            (String::from("\"plain\""), String::from("plain")),
            (String::from("\"quote\\\"\""), String::from("quote\"")),
            (String::from("\"slash\\\\\""), String::from("slash\\")),
            (String::from("\"lf\\n\""), String::from("lf\n")),
            (String::from("\"cr\\r\""), String::from("cr\r")),
            (String::from("\"tab\\t\""), String::from("tab\t")),
            (
                String::from("\"astral\\u{1F600}\""),
                String::from("astral😀"),
            ),
        ]
    );
}

#[test]
fn numeric_literals_keep_exact_digits_bodies_and_all_supported_suffixes() {
    let long_digits = "9".repeat(4096);
    let source = format!(
        "1i8 2i16 3i32 4i64 5u8 6u16 7u32 8u64 1.5f32 2.5f64 123 2.5 1e+3 1e-3 2f64 -1u8 {long_digits}"
    );
    let lexed = lex(&source);
    let tokens = lexed
        .tokens()
        .iter()
        .filter(|token| token.kind() == TokenKind::Atom)
        .collect::<Vec<_>>();

    let integer_suffixes = [
        IntegerSuffix::I8,
        IntegerSuffix::I16,
        IntegerSuffix::I32,
        IntegerSuffix::I64,
        IntegerSuffix::U8,
        IntegerSuffix::U16,
        IntegerSuffix::U32,
        IntegerSuffix::U64,
    ];
    for (token, suffix) in tokens.iter().zip(integer_suffixes) {
        let Some(LiteralClassification::Literal(Literal::Integer(integer))) =
            token.literal()
        else {
            panic!("{} was not an integer literal", token.text());
        };
        assert_eq!(integer.suffix(), Some(suffix));
    }

    for token in tokens.iter().skip(8).take(2) {
        assert!(matches!(
            token.literal(),
            Some(LiteralClassification::Literal(Literal::Float(_)))
        ));
    }
    let unsuffixed = tokens.get(10..15).expect("unsuffixed numeric tail");
    assert!(matches!(
        unsuffixed[0].literal(),
        Some(LiteralClassification::Literal(Literal::Integer(_)))
    ));
    for token in &unsuffixed[1..] {
        assert!(matches!(
            token.literal(),
            Some(LiteralClassification::Literal(Literal::Float(_)))
        ));
    }

    let negative = tokens.get(15).expect("negative suffixed integer");
    let Some(LiteralClassification::Literal(Literal::Integer(integer))) =
        negative.literal()
    else {
        panic!("{} was not an integer literal", negative.text());
    };
    assert!(integer.is_negative());
    assert_eq!(integer.digits(), "1");
    assert_eq!(integer.suffix(), Some(IntegerSuffix::U8));

    let long = tokens.last().expect("long integer");
    let Some(LiteralClassification::Literal(Literal::Integer(integer))) =
        long.literal()
    else {
        panic!("long digits were not an integer literal");
    };
    assert_eq!(integer.raw(), long_digits);
    assert_eq!(integer.digits(), long_digits);
}

#[test]
fn booleans_void_and_numeric_candidates_are_classified_before_names() {
    let classifications = ["true", "false", "void", "2f64", "-1u8"]
        .into_iter()
        .map(classify)
        .collect::<Vec<_>>();

    assert!(matches!(
        classifications[0],
        LiteralClassification::Literal(Literal::Boolean(ref value)) if value.value()
    ));
    assert!(matches!(
        classifications[1],
        LiteralClassification::Literal(Literal::Boolean(ref value)) if !value.value()
    ));
    assert!(matches!(
        classifications[2],
        LiteralClassification::Literal(Literal::Void(_))
    ));
    assert!(matches!(
        classifications[3],
        LiteralClassification::Literal(Literal::Float(_))
    ));
    assert!(matches!(
        classifications[4],
        LiteralClassification::Literal(Literal::Integer(_))
    ));
}
