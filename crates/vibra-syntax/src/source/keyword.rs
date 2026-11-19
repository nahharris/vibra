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
//! `do` is the one word with two readings: at a section boundary directly
//! inside a `fn`, `proc`, or `test` it is the body section, closed by that
//! declaration's own closer ([`BODY_CONSTRUCTS`]); anywhere else, a guard's
//! operand included, it opens a block closed by `enddo`.

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

/// The reserved type heads of "Grammar": a type expression or type body that
/// begins with one is parsed by that head's own production, never as an
/// applied path. They are identifiers to the lexer and name no declaration.
pub const TYPE_HEADS: &[&str] = &["Fn", "Proc", "Intent", "Intrinsic"];

/// The sections each declaration kind admits, one row per kind, from the
/// grammar's `type-item`, `interface-decl`, `impl`, `fn-decl`, `proc-decl`,
/// `anon-fn`, `anon-proc`, `test`, and `module-let` productions. The
/// structure of a declaration is not a row entry: the body of a `type` (`of`
/// or its `case` list), the signature of a callable (`takes`, `labelled`,
/// `returns`), the `takes` of a `test`, the body `do`, and the `be` of a
/// module value. A section that
/// is not in its kind's row is `@syntax.invalidSection`.
pub const ADMISSIBLE_SECTIONS: &[(&str, &[&str])] = &[
    (
        "type",
        &[
            "given",
            "where",
            "role",
            "visibility",
            "representation",
            "doc",
        ],
    ),
    ("interface", &["given", "role", "visibility", "doc"]),
    ("impl", &[]),
    (
        "fn",
        &[
            "given",
            "requires",
            "ensures",
            "visibility",
            "external",
            "symbol",
            "native",
            "doc",
        ],
    ),
    (
        "proc",
        &[
            "given",
            "requires",
            "ensures",
            "performs",
            "visibility",
            "external",
            "symbol",
            "doc",
        ],
    ),
    ("anon-fn", &["given", "requires", "ensures"]),
    ("anon-proc", &["given", "requires", "ensures", "performs"]),
    ("test", &["performs"]),
    ("let", &["visibility", "doc"]),
];

/// The sections of a callable's signature, admitted by every `fn` and
/// `proc`, named or anonymous, in the order of the `signature` production.
pub const SIGNATURE_SECTIONS: &[&str] = &["takes", "labelled", "returns"];

/// The guard sections a member of an `impl` does not declare, from the type
/// chapter's "Implementations": the contract's guards are its guards.
pub const IMPLEMENTATION_MEMBER_EXCLUDED: &[&str] = &["requires", "ensures", "where"];

/// The sections admissible only in the embedded standard library; elsewhere
/// each is `@syntax.invalidSection`.
pub const STANDARD_LIBRARY_SECTIONS: &[&str] =
    &["role", "external", "symbol", "native"];

/// The closed tag set of each section whose operand is a tag, from the
/// source chapter ("Declarations" and "External definitions"), the type
/// chapter ("Language core and standard library"), and the runtime chapter
/// ("External providers"). Any other operand is `@syntax.invalidForm` over
/// the section.
pub const TAG_SETS: &[(&str, &[&str])] = &[
    ("visibility", &["@public", "@private"]),
    ("representation", &["@public", "@private"]),
    ("external", &["@compiler", "@host"]),
    (
        "role",
        &[
            "@bool",
            "@str",
            "@bytes",
            "@option",
            "@result",
            "@dict",
            "@iter",
            "@equatable",
            "@ordered",
        ],
    ),
];

/// The sections declaration kind `kind` admits, or `None` for a kind with no
/// row.
#[must_use]
pub fn admissible_sections(kind: &str) -> Option<&'static [&'static str]> {
    ADMISSIBLE_SECTIONS
        .iter()
        .find(|(row, _)| *row == kind)
        .map(|(_, sections)| *sections)
}

/// The closed tag set of the section `keyword`, when its operand is a tag.
#[must_use]
pub fn tag_set(keyword: &str) -> Option<&'static [&'static str]> {
    TAG_SETS
        .iter()
        .find(|(row, _)| *row == keyword)
        .map(|(_, tags)| *tags)
}

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
