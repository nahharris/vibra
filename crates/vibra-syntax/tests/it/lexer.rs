//! The lexer: token classes, literal rules, encoding, and losslessness.

use vibra_diagnostics::DiagnosticCode::{
    SyntaxInvalidCharacterLiteral, SyntaxInvalidEncoding, SyntaxInvalidName,
    SyntaxInvalidNumericLiteral, SyntaxInvalidStringLiteral, SyntaxMissingSeparator,
    SyntaxUnmatchedDelimiter,
};
use vibra_syntax::{RESERVED_WORDS, TokenKind, check_encoding, is_reserved_word, lex};

use crate::support::{Random, codes};

fn kinds(text: &str) -> Vec<(TokenKind, &str)> {
    lex(text)
        .tokens
        .iter()
        .map(|token| (token.kind, token.span.text(text).unwrap()))
        .collect()
}

#[test]
fn lexing_then_concatenating_token_texts_is_the_identity() {
    let mut random = Random::new(0x5EED_0003);
    for _ in 0..4000 {
        let text = random.text(48);
        let lexed = lex(&text);
        let mut at = 0;
        let mut joined = String::new();
        for token in &lexed.tokens {
            assert_eq!(token.span.start(), at, "tokens are contiguous in {text:?}");
            assert!(!token.span.is_empty(), "no token is empty in {text:?}");
            joined.push_str(token.span.text(&text).expect("a token is on boundaries"));
            at = token.span.end();
        }
        assert_eq!(joined, text);
    }
}

#[test]
fn the_47_reserved_words_are_recognized_before_the_identifier() {
    assert_eq!(RESERVED_WORDS.len(), 47);
    assert!(RESERVED_WORDS.contains(&"void"));
    for word in RESERVED_WORDS {
        assert!(is_reserved_word(word));
        assert_eq!(kinds(word), [(TokenKind::ReservedWord, *word)]);
    }
    assert_eq!(kinds("true"), [(TokenKind::Identifier, "true")]);
    assert_eq!(kinds("voids"), [(TokenKind::Identifier, "voids")]);
}

#[test]
fn every_token_class_is_produced() {
    let text = "x fn @a.b _ 1u8 2.5 \"s\" \"\"\"d\"\"\" 'c' ( ) { } [ ] : . // c\n#";
    let found = kinds(text)
        .into_iter()
        .filter(|(kind, _)| *kind != TokenKind::Whitespace)
        .collect::<Vec<_>>();
    assert_eq!(
        found,
        [
            (TokenKind::Identifier, "x"),
            (TokenKind::ReservedWord, "fn"),
            (TokenKind::Atom, "@a.b"),
            (TokenKind::Discard, "_"),
            (TokenKind::Integer, "1u8"),
            (TokenKind::Float, "2.5"),
            (TokenKind::String, "\"s\""),
            (TokenKind::DocString, "\"\"\"d\"\"\""),
            (TokenKind::Character, "'c'"),
            (TokenKind::OpenParen, "("),
            (TokenKind::CloseParen, ")"),
            (TokenKind::OpenBrace, "{"),
            (TokenKind::CloseBrace, "}"),
            (TokenKind::OpenBracket, "["),
            (TokenKind::CloseBracket, "]"),
            (TokenKind::Colon, ":"),
            (TokenKind::Dot, "."),
            (TokenKind::Comment, "// c"),
            (TokenKind::Stray, "#"),
        ]
    );
    assert!(lex(text).diagnostics.is_empty());
}

#[test]
fn a_minus_directly_before_a_digit_is_part_of_the_number() {
    assert_eq!(kinds("-7i64"), [(TokenKind::Integer, "-7i64")]);
    assert_eq!(
        kinds("- 7"),
        [
            (TokenKind::Stray, "-"),
            (TokenKind::Whitespace, " "),
            (TokenKind::Integer, "7")
        ]
    );
    assert_eq!(kinds("1e3"), [(TokenKind::Float, "1e3")]);
    assert_eq!(kinds("2f64"), [(TokenKind::Float, "2f64")]);
    assert_eq!(kinds("1E-3f32"), [(TokenKind::Float, "1E-3f32")]);
}

#[test]
fn numeric_faults_cover_the_token() {
    let text = "{a: 1i128 b: 2.5i32 c: 3u8x}";
    assert_eq!(
        codes(&lex(text).diagnostics),
        [
            (SyntaxInvalidNumericLiteral, 4, 9),
            (SyntaxInvalidNumericLiteral, 13, 19),
            (SyntaxInvalidNumericLiteral, 23, 27),
        ]
    );
    // Range is not the lexer's: a well-formed literal out of range is fine.
    assert!(lex("-1u8 300u8").diagnostics.is_empty());
}

#[test]
fn string_escape_faults_cover_the_token() {
    let text = "{a: \"\\q\" b: \"\\u{d800}\" c: \"\\u{110000}\"}";
    assert_eq!(
        codes(&lex(text).diagnostics),
        [
            (SyntaxInvalidStringLiteral, 4, 8),
            (SyntaxInvalidStringLiteral, 12, 22),
            (SyntaxInvalidStringLiteral, 26, 38),
        ]
    );
    assert!(
        lex("\"\\\" \\\\ \\n \\r \\t \\u{1} \\u{10FFFF}\"")
            .diagnostics
            .is_empty()
    );
}

#[test]
fn an_unterminated_string_is_only_an_unmatched_delimiter() {
    assert_eq!(
        codes(&lex("\"open").diagnostics),
        [(SyntaxUnmatchedDelimiter, 0, 5)]
    );
    // Even with a malformed escape inside.
    assert_eq!(
        codes(&lex("\"a\\q").diagnostics),
        [(SyntaxUnmatchedDelimiter, 0, 4)]
    );
    assert_eq!(
        codes(&lex("\"\"\"doc").diagnostics),
        [(SyntaxUnmatchedDelimiter, 0, 6)]
    );
}

#[test]
fn a_string_keeps_its_line_breaks() {
    let text = "\"a\r\nb\"";
    assert_eq!(kinds(text), [(TokenKind::String, text)]);
    assert!(lex(text).diagnostics.is_empty());
}

#[test]
fn character_faults_cover_the_token() {
    let text = "{a: '' b: 'ab' c: '\\q'}";
    assert_eq!(
        codes(&lex(text).diagnostics),
        [
            (SyntaxInvalidCharacterLiteral, 4, 6),
            (SyntaxInvalidCharacterLiteral, 10, 14),
            (SyntaxInvalidCharacterLiteral, 18, 22),
        ]
    );
    assert!(
        lex("'x' '\\'' '\\\"' '\\u{1F600}' '\u{1F600}'")
            .diagnostics
            .is_empty()
    );
    assert_eq!(
        codes(&lex("'\\u{d800}'").diagnostics),
        [(SyntaxInvalidCharacterLiteral, 0, 10)]
    );
}

#[test]
fn a_malformed_identifier_is_one_invalid_name() {
    assert_eq!(
        codes(&lex("{my_field: 1u8}").diagnostics),
        [(SyntaxInvalidName, 1, 9)]
    );
    for text in ["my-field", "ok?", "go!", "é", "café", "_x"] {
        let lexed = lex(text);
        assert_eq!(lexed.tokens.len(), 1, "{text}");
        assert_eq!(
            codes(&lexed.diagnostics),
            [(SyntaxInvalidName, 0, text.len())],
            "{text}"
        );
    }
}

#[test]
fn adjacent_tokens_need_a_separator_and_both_are_kept() {
    let lexed = lex("{a: 1u8\"x\"}");
    assert_eq!(codes(&lexed.diagnostics), [(SyntaxMissingSeparator, 7, 7)]);
    assert!(
        lexed
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::String)
    );
    assert_eq!(
        codes(&lex("\"a\"b").diagnostics),
        [(SyntaxMissingSeparator, 3, 3)]
    );
    // A bracket or `:` separates.
    assert!(lex("{{1u8}{2u8}}").diagnostics.is_empty());
    assert!(lex("[\"a\":\"b\"]").diagnostics.is_empty());
}

#[test]
fn encoding_faults_cover_the_offending_bytes() {
    let (text, diagnostics) = check_encoding(&[0xEF, 0xBB, 0xBF, 0x7B, 0x7D]);
    assert!(text.is_some());
    assert_eq!(codes(&diagnostics), [(SyntaxInvalidEncoding, 0, 3)]);

    let (text, diagnostics) = check_encoding(&[0x7B, 0x00, 0x7D]);
    assert!(text.is_some());
    assert_eq!(codes(&diagnostics), [(SyntaxInvalidEncoding, 1, 2)]);

    let (text, diagnostics) = check_encoding(&[0x7B, 0xFF, 0x7D]);
    assert!(text.is_none(), "a document that is not UTF-8 has no text");
    assert_eq!(codes(&diagnostics), [(SyntaxInvalidEncoding, 1, 2)]);

    let (text, diagnostics) = check_encoding(&[0x7B, 0xE2, 0x82]);
    assert!(text.is_none());
    assert_eq!(codes(&diagnostics), [(SyntaxInvalidEncoding, 1, 3)]);
}
