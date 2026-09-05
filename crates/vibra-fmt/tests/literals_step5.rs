//! Step 5 literal formatting tests.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]

use std::path::Path;

use vibra_fmt::format_source;
use vibra_syntax::{Literal, LiteralClassification, TokenKind, parse_source};

#[test]
fn formatter_uses_canonical_character_spellings() {
    let source =
        r##"( \u0061   \u000A \u000D \u0020 \u0009 \u0000 \u000B \u0041 \u003B \🌱 )"##;
    let formatted =
        format_source(Path::new("characters.vib"), source).expect("source mode");

    assert_eq!(
        formatted,
        r##"(\a \newline \return \space \tab \u0000 \u000B \A \; \🌱)
"##
    );
    assert_eq!(
        format_source(Path::new("characters.vib"), &formatted)
            .expect("formatted source"),
        formatted
    );
}

#[test]
fn formatter_preserves_valid_string_and_numeric_leaf_spellings() {
    let source = r###"(true   false void "a\n" 001u8 2.500f64)"###;
    let formatted =
        format_source(Path::new("literals.vib"), source).expect("source mode");

    assert_eq!(
        formatted,
        r###"(true false void "a\n" 001u8 2.500f64)
"###
    );
    assert_eq!(
        format_source(Path::new("literals.vib"), &formatted).expect("formatted source"),
        formatted
    );
}

#[test]
fn formatter_preserves_unterminated_escape_bytes_in_both_document_modes() {
    for extension in ["vib", "vibon"] {
        let path = format!("unfinished.{extension}");
        let source = "\"unfinished\\\r\n";
        let formatted = format_source(&path, source).expect("selected mode");
        assert_eq!(formatted, source, "{path}");
    }
}

#[test]
fn canonical_character_width_controls_the_88_column_boundary() {
    let fits = "x".repeat(77);
    let overflows = "x".repeat(78);

    let fits_source = format!("({fits} \\u000A)");
    let fits_formatted = format_source(Path::new("boundary.vib"), &fits_source)
        .expect("88-column source");
    assert_eq!(fits_formatted, format!("({fits} \\newline)\n"));

    let overflows_source = format!("({overflows} \\u000A)");
    let overflows_formatted =
        format_source(Path::new("boundary.vib"), &overflows_source)
            .expect("89-column source");
    assert_eq!(overflows_formatted, format!("({overflows}\n  \\newline)\n"));
}

#[test]
fn canonical_character_width_is_checked_at_nested_indentation() {
    let fits = "x".repeat(75);
    let overflows = "x".repeat(76);
    let fits_source = format!("(({fits} \\u000A))");
    let overflows_source = format!("(({overflows} \\u000A))");

    let fits_formatted = format_source(Path::new("nested.vib"), &fits_source)
        .expect("nested inline source");
    assert_eq!(fits_formatted, format!("(({fits} \\newline))\n"));

    let overflows_formatted = format_source(Path::new("nested.vib"), &overflows_source)
        .expect("nested multiline source");
    assert!(overflows_formatted.contains(&format!("\n  ({overflows}")));
    assert_eq!(
        format_source(Path::new("nested.vib"), &overflows_formatted)
            .expect("reformatted nested source"),
        overflows_formatted
    );
}

#[test]
fn formatting_preserves_decoded_string_values_in_both_document_modes() {
    let source = r###"("a\n" "quote\"" "astral\u{1F600}")"###;
    let values = |path: &str, text: &str| {
        parse_source(path, text)
            .or_else(|_| vibra_syntax::parse_data(path, text))
            .expect("literal document")
            .tokens()
            .iter()
            .filter(|token| token.kind() == TokenKind::Atom)
            .filter_map(|token| match token.literal() {
                Some(LiteralClassification::Literal(Literal::String(value))) => {
                    Some(value.value().to_owned())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };

    for path in ["values.vib", "values.vibon"] {
        let formatted = format_source(Path::new(path), source).expect("selected mode");
        assert_eq!(values(path, source), values(path, &formatted));
        assert_eq!(
            format_source(Path::new(path), &formatted).expect("idempotent mode"),
            formatted
        );
    }
}
