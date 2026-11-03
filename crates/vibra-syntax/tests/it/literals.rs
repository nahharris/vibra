//! Step 5 literal-surface tests. Malformed literal diagnostics, canonical
//! spellings, and source and data parity are asserted by the
//! `V1-SRC-READER-*` conformance cases; the decoded values and classification
//! of the valid forms are here.

use vibra_syntax::{
    IntegerSuffix, Literal, LiteralClassification, TokenKind,
    canonical_string_spelling, classify, lex,
};

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

#[test]
fn the_canonical_string_spelling_escapes_controls_and_decodes_back() {
    assert_eq!(canonical_string_spelling("plain"), "\"plain\"");
    assert_eq!(
        canonical_string_spelling("q\" b\\ n\n r\r t\t"),
        r#""q\" b\\ n\n r\r t\t""#
    );
    // Other C0 controls and DEL use `\u{hex}`; everything else is literal.
    assert_eq!(
        canonical_string_spelling("\u{0}\u{1b}\u{1f}\u{7f}é🌱"),
        r#""\u{0}\u{1b}\u{1f}\u{7f}é🌱""#
    );
    let value = "a\u{1}\"\\\n\u{7f}z";
    let LiteralClassification::Literal(Literal::String(decoded)) =
        classify(&canonical_string_spelling(value))
    else {
        panic!("the canonical spelling is a valid string literal");
    };
    assert_eq!(decoded.value(), value);
}
