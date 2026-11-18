//! Bytes to tokens.
//!
//! The lexical structure is the source chapter's "Lexical structure"
//! (`docs/spec/01-source-language.md`), shared by `.vib` and `.vibon`
//! documents. Lexing is lossless: every byte of the input is in exactly one
//! token, so the concatenation of the token texts is the input.
//!
//! Lexing happens in two stages. [`check_encoding`] turns bytes into text: a
//! document that is not valid UTF-8 has no text and no tree. A byte-order mark
//! and a NUL are valid UTF-8 but are `@syntax.invalidEncoding` all the same;
//! their characters stay in the text as tokens that start nothing, so the
//! tree still holds every byte. [`lex`] then turns the text into tokens and
//! reports the literal, name, and separator faults.

use std::fmt;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use crate::literal::{
    NumberKind, continues_word, scan_character, scan_number, scan_quoted,
};
use crate::literal::{decode_character, decode_string};

/// The 47 reserved words of the source chapter, in its order.
pub const RESERVED_WORDS: &[&str] = &[
    "as",
    "await",
    "be",
    "case",
    "do",
    "doc",
    "else",
    "endfn",
    "enddo",
    "endif",
    "endimpl",
    "endinterface",
    "endlet",
    "endmatch",
    "endproc",
    "endtest",
    "endtype",
    "enduse",
    "ensures",
    "external",
    "fn",
    "given",
    "if",
    "impl",
    "interface",
    "labelled",
    "let",
    "match",
    "native",
    "of",
    "performs",
    "proc",
    "representation",
    "requires",
    "return",
    "returns",
    "role",
    "symbol",
    "takes",
    "test",
    "then",
    "try",
    "type",
    "use",
    "visibility",
    "void",
    "where",
];

/// Whether `text` is one of the [`RESERVED_WORDS`].
#[must_use]
pub fn is_reserved_word(text: &str) -> bool {
    RESERVED_WORDS.contains(&text)
}

/// The class of one token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// A run of spaces, tabs, carriage returns, and line feeds.
    Whitespace,
    /// `//` to the end of the line, without the line break.
    Comment,
    /// An identifier, well formed or not.
    Identifier,
    /// One of the [`RESERVED_WORDS`].
    ReservedWord,
    /// `@` followed by dot-separated parts, well formed or not.
    Atom,
    /// The discard `_`.
    Discard,
    /// An integer literal, well formed or not.
    Integer,
    /// A float literal, well formed or not.
    Float,
    /// A string literal, terminated or not.
    String,
    /// A doc string, terminated or not.
    DocString,
    /// A character literal, well formed or not.
    Character,
    /// `(`.
    OpenParen,
    /// `)`.
    CloseParen,
    /// `{`.
    OpenBrace,
    /// `}`.
    CloseBrace,
    /// `[`.
    OpenBracket,
    /// `]`.
    CloseBracket,
    /// `:`.
    Colon,
    /// `.`.
    Dot,
    /// A character that starts no token.
    Stray,
    /// A byte-order mark or a NUL: a character that starts no token and that
    /// [`check_encoding`] already reported.
    Undecodable,
}

impl TokenKind {
    /// Whether two tokens of this class must be separated: identifier,
    /// reserved word, number, atom, string, character, and doc string.
    #[must_use]
    pub const fn needs_separation(self) -> bool {
        matches!(
            self,
            Self::Identifier
                | Self::ReservedWord
                | Self::Integer
                | Self::Float
                | Self::Atom
                | Self::String
                | Self::DocString
                | Self::Character
        )
    }

    /// Whether this token is whitespace or a comment.
    #[must_use]
    pub const fn is_trivia(self) -> bool {
        matches!(self, Self::Whitespace | Self::Comment)
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

/// One token: its class and its bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Token {
    /// The class.
    pub kind: TokenKind,
    /// The bytes of the token in the document.
    pub span: ByteSpan,
}

/// The tokens of a text and the faults found while lexing it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lexed {
    /// Every token, in order; their texts concatenate to the input.
    pub tokens: Vec<Token>,
    /// The literal, name, and separator diagnostics, in input order.
    pub diagnostics: Vec<Diagnostic>,
}

/// The UTF-8 byte-order mark.
const BYTE_ORDER_MARK: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Checks the encoding of a document.
///
/// Returns the text when the bytes are valid UTF-8, and one
/// `@syntax.invalidEncoding` over the offending bytes of a leading byte-order
/// mark, of each NUL, and of each invalid UTF-8 sequence.
#[must_use]
pub fn check_encoding(bytes: &[u8]) -> (Option<&str>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    if bytes.starts_with(BYTE_ORDER_MARK) {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::SyntaxInvalidEncoding,
            ByteSpan::new(0, BYTE_ORDER_MARK.len()),
            "a document must not start with a byte-order mark",
        ));
    }
    let mut offset = 0;
    let mut valid = true;
    loop {
        let rest = bytes.get(offset..).unwrap_or_default();
        match std::str::from_utf8(rest) {
            Ok(text) => {
                push_nuls(&mut diagnostics, text, offset);
                break;
            }
            Err(error) => {
                valid = false;
                let good = error.valid_up_to();
                if let Some(text) = rest
                    .get(..good)
                    .and_then(|prefix| std::str::from_utf8(prefix).ok())
                {
                    push_nuls(&mut diagnostics, text, offset);
                }
                let bad = error.error_len().unwrap_or(rest.len() - good);
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::SyntaxInvalidEncoding,
                    ByteSpan::new(offset + good, offset + good + bad),
                    "these bytes are not valid UTF-8",
                ));
                offset += good + bad;
            }
        }
    }
    let text = if valid {
        std::str::from_utf8(bytes).ok()
    } else {
        None
    };
    (text, diagnostics)
}

fn push_nuls(diagnostics: &mut Vec<Diagnostic>, text: &str, offset: usize) {
    for (at, _) in text.match_indices('\0') {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::SyntaxInvalidEncoding,
            ByteSpan::sized(offset + at, 1),
            "a document must not contain a NUL",
        ));
    }
}

/// Whether `character` starts an identifier token: an ASCII letter, a
/// non-ASCII letter, or `_` followed by a word character.
fn starts_identifier(character: char, next: Option<char>) -> bool {
    character.is_ascii_alphabetic()
        || (!character.is_ascii() && character.is_alphabetic())
        || (character == '_' && next.is_some_and(continues_identifier))
}

/// Whether `character` continues an identifier token. The hyphen, `?`, and
/// `!` are not identifier characters, but a token that holds one is one
/// malformed identifier, `@syntax.invalidName`, not several tokens.
fn continues_identifier(character: char) -> bool {
    continues_word(character) || matches!(character, '-' | '?' | '!')
}

/// Whether `text` matches the identifier production.
fn is_identifier(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| character.is_ascii_alphanumeric())
}

/// Whether `text` matches the atom production.
pub(crate) fn is_atom(text: &str) -> bool {
    text.strip_prefix('@')
        .is_some_and(|parts| parts.split('.').all(is_identifier))
}

fn char_at(text: &str, at: usize) -> Option<char> {
    text.get(at..)?.chars().next()
}

fn run_length(rest: &str, mut accepts: impl FnMut(char) -> bool) -> usize {
    rest.chars()
        .take_while(|character| accepts(*character))
        .map(char::len_utf8)
        .sum()
}

/// Lexes a text into tokens.
///
/// The concatenation of the token texts is `text`. Diagnostics are in input
/// order: `@syntax.invalidNumericLiteral`, `@syntax.invalidName`,
/// `@syntax.invalidStringLiteral`, and `@syntax.invalidCharacterLiteral` over
/// a malformed token, `@syntax.unmatchedDelimiter` over a string or doc string
/// that reaches the end of the file, and `@syntax.missingSeparator` at a
/// zero-width span where the second of two adjacent tokens that need
/// separation starts.
#[must_use]
pub fn lex(text: &str) -> Lexed {
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut at = 0;
    while let Some(character) = char_at(text, at) {
        let rest = text.get(at..).unwrap_or_default();
        let next = char_at(text, at + character.len_utf8());
        let (kind, len) = match character {
            ' ' | '\t' | '\r' | '\n' => (
                TokenKind::Whitespace,
                run_length(rest, |c| matches!(c, ' ' | '\t' | '\r' | '\n')),
            ),
            '/' if next == Some('/') => (
                TokenKind::Comment,
                run_length(rest, |c| !matches!(c, '\n' | '\r')),
            ),
            '"' => {
                let scan = scan_quoted(rest);
                let kind = if scan.doc {
                    TokenKind::DocString
                } else {
                    TokenKind::String
                };
                let span = ByteSpan::sized(at, scan.len);
                if !scan.terminated {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::SyntaxUnmatchedDelimiter,
                        span,
                        "this string reaches the end of the file without its closing quote",
                    ));
                } else if !scan.doc
                    && rest.get(..scan.len).and_then(decode_string).is_none()
                {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::SyntaxInvalidStringLiteral,
                        span,
                        "this string has a malformed or unsupported escape",
                    ));
                }
                (kind, scan.len)
            }
            '\'' => {
                let (len, terminated) = scan_character(rest);
                let valid =
                    terminated && rest.get(..len).and_then(decode_character).is_some();
                if !valid {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::SyntaxInvalidCharacterLiteral,
                        ByteSpan::sized(at, len),
                        "a character literal holds exactly one Unicode scalar",
                    ));
                }
                (TokenKind::Character, len)
            }
            '-' if next.is_some_and(|c| c.is_ascii_digit()) => {
                number(rest, at, &mut diagnostics)
            }
            '0'..='9' => number(rest, at, &mut diagnostics),
            '@' if next.is_some_and(continues_word) => {
                let len = 1 + run_length(rest.get(1..).unwrap_or_default(), |c| {
                    continues_identifier(c) || c == '.'
                });
                let token = rest.get(..len).unwrap_or_default();
                if !is_atom(token) {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::SyntaxInvalidName,
                        ByteSpan::sized(at, len),
                        "an atom part starts with an ASCII letter and continues with \
                         ASCII letters and digits",
                    ));
                }
                (TokenKind::Atom, len)
            }
            c if starts_identifier(c, next) => {
                let len = run_length(rest, continues_identifier);
                let token = rest.get(..len).unwrap_or_default();
                if !is_identifier(token) {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::SyntaxInvalidName,
                        ByteSpan::sized(at, len),
                        "an identifier starts with an ASCII letter and continues with \
                         ASCII letters and digits",
                    ));
                    (TokenKind::Identifier, len)
                } else if is_reserved_word(token) {
                    (TokenKind::ReservedWord, len)
                } else {
                    (TokenKind::Identifier, len)
                }
            }
            '_' => (TokenKind::Discard, 1),
            '(' => (TokenKind::OpenParen, 1),
            ')' => (TokenKind::CloseParen, 1),
            '{' => (TokenKind::OpenBrace, 1),
            '}' => (TokenKind::CloseBrace, 1),
            '[' => (TokenKind::OpenBracket, 1),
            ']' => (TokenKind::CloseBracket, 1),
            ':' => (TokenKind::Colon, 1),
            '.' => (TokenKind::Dot, 1),
            '\0' => (TokenKind::Undecodable, 1),
            '\u{FEFF}' if at == 0 => (TokenKind::Undecodable, character.len_utf8()),
            other => (TokenKind::Stray, other.len_utf8()),
        };
        tokens.push(Token {
            kind,
            span: ByteSpan::sized(at, len),
        });
        at += len;
    }
    for pair in tokens.windows(2) {
        if let [first, second] = pair
            && first.kind.needs_separation()
            && second.kind.needs_separation()
        {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::SyntaxMissingSeparator,
                ByteSpan::empty_at(second.span.start()),
                "two tokens must be separated by whitespace, a bracket, or `:`",
            ));
        }
    }
    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end(),
        )
    });
    Lexed {
        tokens,
        diagnostics,
    }
}

fn number(
    rest: &str,
    at: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> (TokenKind, usize) {
    let scan = scan_number(rest);
    if !scan.valid {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::SyntaxInvalidNumericLiteral,
            ByteSpan::sized(at, scan.len),
            "an unknown numeric suffix, an integer suffix on a float, or trailing text",
        ));
    }
    let kind = match scan.kind {
        NumberKind::Integer => TokenKind::Integer,
        NumberKind::Float => TokenKind::Float,
    };
    (kind, scan.len)
}
