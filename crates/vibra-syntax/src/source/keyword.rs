//! The roles of the reserved words in the source grammar.
//!
//! `docs/spec/01-source-language.md`, "Reserved words", lists the 47 words
//! ([`crate::RESERVED_WORDS`]). Every word has exactly one role here, and the
//! lexer, the parser, the formatter, and the tests read these tables:
//!
//! - an opener keyword opens a construct closed by its typed closer
//!   ([`CONSTRUCTS`]);
//! - a section keyword starts a section of the construct it is written in
//!   ([`SECTIONS`], in the canonical order of "Canonical format");
//! - a separator separates the parts of its construct ([`SEPARATORS`]);
//! - an expression word is an ordinary token of an expression
//!   ([`EXPRESSION_WORDS`]).
//!
//! `do` is the one word with two readings: directly inside a `fn`, `proc`, or
//! `test` it is the body section, closed by that declaration's own closer
//! ([`BODY_CONSTRUCTS`]); anywhere else it opens a block closed by `enddo`.

/// Every opener keyword with its typed closer.
pub const CONSTRUCTS: &[(&str, &str)] = &[
    ("type", "endtype"),
    ("interface", "endinterface"),
    ("impl", "endimpl"),
    ("fn", "endfn"),
    ("proc", "endproc"),
    ("let", "endlet"),
    ("test", "endtest"),
    ("use", "enduse"),
    ("if", "endif"),
    ("match", "endmatch"),
    ("do", "enddo"),
];

/// The openers of the seven top-level forms, in the order of the `top`
/// production.
pub const TOP_LEVEL: &[&str] =
    &["use", "type", "interface", "fn", "proc", "let", "test"];

/// The constructs in which `do` is the body section.
pub const BODY_CONSTRUCTS: &[&str] = &["fn", "proc", "test"];

/// The section keywords, in the canonical order of the source chapter's
/// "Canonical format". `as` and `be`, which that list does not name, come
/// last: each is the only reorderable section of its form.
pub const SECTIONS: &[&str] = &[
    "of",
    "takes",
    "labelled",
    "returns",
    "given",
    "where",
    "requires",
    "ensures",
    "performs",
    "role",
    "visibility",
    "representation",
    "external",
    "symbol",
    "native",
    "doc",
    "as",
    "be",
];

/// The separators inside their constructs.
pub const SEPARATORS: &[&str] = &["then", "else", "case"];

/// The reserved words that are ordinary tokens of an expression.
pub const EXPRESSION_WORDS: &[&str] = &["return", "try", "await", "void"];

/// The closer of the construct `opener` opens.
#[must_use]
pub fn closer_of(opener: &str) -> Option<&'static str> {
    CONSTRUCTS
        .iter()
        .find(|(word, _)| *word == opener)
        .map(|(_, closer)| *closer)
}

/// `word` itself, when it is an opener keyword.
#[must_use]
pub fn opener_keyword(word: &str) -> Option<&'static str> {
    CONSTRUCTS
        .iter()
        .map(|(opener, _)| *opener)
        .find(|opener| *opener == word)
}

/// The opener of the construct `closer` closes.
#[must_use]
pub fn opener_of(closer: &str) -> Option<&'static str> {
    CONSTRUCTS
        .iter()
        .find(|(_, word)| *word == closer)
        .map(|(opener, _)| *opener)
}

/// Whether `word` is a section keyword.
#[must_use]
pub fn is_section(word: &str) -> bool {
    SECTIONS.contains(&word)
}

/// The position of the section `word` in the canonical order.
#[must_use]
pub fn section_rank(word: &str) -> Option<usize> {
    SECTIONS.iter().position(|section| *section == word)
}
