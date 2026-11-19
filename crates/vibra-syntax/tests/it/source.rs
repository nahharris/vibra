//! The source grammar's skeleton: constructs, sections, `use`, the top
//! level, and recovery.

use vibra_diagnostics::DiagnosticCode::{
    self, NameReservedDeclaration, SyntaxDuplicateSection, SyntaxInvalidForm,
    SyntaxInvalidSection, SyntaxMissingSeparator, SyntaxUnmatchedDelimiter,
};
use vibra_syntax::source::keyword::{
    BODY_CONSTRUCTS, CONSTRUCTS, EXPRESSION_WORDS, SECTIONS, SEPARATORS, TOP_LEVEL,
};
use vibra_syntax::{
    Declaration, Document, RESERVED_WORDS, SyntaxKind, TokenKind, lex, module,
    parse_source, tree_document,
};

use crate::support::{Random, codes};

fn source(text: &str) -> Document {
    parse_source(None, text.as_bytes())
}

fn diagnostics(text: &str) -> Vec<(DiagnosticCode, usize, usize)> {
    codes(source(text).diagnostics())
}

#[test]
fn the_reserved_words_are_47_and_each_has_exactly_one_role() {
    assert_eq!(RESERVED_WORDS.len(), 47);
    let mut roles: Vec<&str> = CONSTRUCTS
        .iter()
        .flat_map(|(opener, closer)| [*opener, *closer])
        .chain(SECTIONS.iter().copied())
        .chain(SEPARATORS.iter().copied())
        .chain(EXPRESSION_WORDS.iter().copied())
        .collect();
    assert_eq!(roles.len(), 47);
    roles.sort_unstable();
    let mut words = RESERVED_WORDS.to_vec();
    words.sort_unstable();
    assert_eq!(roles, words);
    for word in TOP_LEVEL.iter().chain(BODY_CONSTRUCTS) {
        assert!(
            CONSTRUCTS.iter().any(|(opener, _)| opener == word),
            "{word}"
        );
    }
}

#[test]
fn the_lexer_classifies_every_reserved_word() {
    for word in RESERVED_WORDS {
        let lexed = lex(word);
        assert_eq!(lexed.tokens.len(), 1, "{word}");
        assert_eq!(lexed.tokens[0].kind, TokenKind::ReservedWord, "{word}");
    }
}

#[test]
fn every_reserved_word_is_rejected_as_an_identifier() {
    for word in RESERVED_WORDS {
        let text = format!("use @a as {word} enduse");
        let document = source(&text);
        assert!(document.has_errors(), "{text}");
        if *word == "enduse" {
            // The closer closes the `use`; the name is missing.
            assert_eq!(
                codes(document.diagnostics()),
                [
                    (SyntaxInvalidForm, 0, 16),
                    (SyntaxUnmatchedDelimiter, 17, 23)
                ],
                "{text}"
            );
            continue;
        }
        let start = 10;
        assert_eq!(
            codes(document.diagnostics()),
            [(NameReservedDeclaration, start, start + word.len())],
            "{text}"
        );
        let module = module(&document).unwrap();
        let alias = module.uses()[0].alias().unwrap();
        assert_eq!(alias.text(), *word);
        assert!(alias.is_reserved());
    }
}

#[test]
fn use_plain() {
    let document = source("use @std.io enduse");
    assert!(document.diagnostics().is_empty());
    let uses = module(&document).unwrap().uses();
    assert_eq!(uses.len(), 1);
    assert_eq!(uses[0].locator().unwrap().parts(), ["std", "io"]);
    assert!(uses[0].alias().is_none());
    assert_eq!(uses[0].name(), Some("io"));
    assert_eq!(
        tree_document(&document).unwrap(),
        "{format: @tree.v2 root: {kind: @source children: [{kind: @use children: [\
         {kind: @reservedWord text: \"use\"} {kind: @whitespace text: \" \"} \
         {kind: @atom text: \"@std.io\"} {kind: @whitespace text: \" \"} \
         {kind: @reservedWord text: \"enduse\"}]}]}}"
    );
}

#[test]
fn use_alias_and_exact_case() {
    let document = source("use @billing.cents as money enduse");
    assert!(document.diagnostics().is_empty());
    let uses = module(&document).unwrap().uses();
    assert_eq!(uses[0].alias().unwrap().text(), "money");
    assert_eq!(uses[0].name(), Some("money"));
    let document = source("use @Billing.CENTS enduse");
    assert!(document.diagnostics().is_empty());
    let uses = module(&document).unwrap().uses();
    assert_eq!(uses[0].locator().unwrap().parts(), ["Billing", "CENTS"]);
}

#[test]
fn use_alias_reserved() {
    let document = source("use @billing.cents as where enduse");
    assert_eq!(
        codes(document.diagnostics()),
        [(NameReservedDeclaration, 22, 27)]
    );
    let uses = module(&document).unwrap().uses();
    assert_eq!(uses[0].alias().unwrap().text(), "where");
}

#[test]
fn use_late() {
    let text = "type A of U8 endtype\n\nuse @std.io enduse";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 22, 25)]);
    let document = source(text);
    let declarations = module(&document).unwrap().declarations();
    assert!(matches!(declarations[0], Declaration::Type(_)));
    assert!(matches!(declarations[1], Declaration::Use(_)));
    // A use after a use is not late.
    assert!(diagnostics("use @a enduse use @b enduse").is_empty());
}

#[test]
fn as_outside_use() {
    assert_eq!(
        diagnostics("use @std.io enduse\nas"),
        [(SyntaxInvalidForm, 19, 21)]
    );
    assert_eq!(
        diagnostics("fn f returns r: I32 as x do return 1i32 endfn"),
        [(SyntaxInvalidForm, 20, 22)]
    );
    assert_eq!(
        diagnostics("fn f returns r: {as} do return 1i32 endfn"),
        [(SyntaxInvalidForm, 17, 19)]
    );
}

#[test]
fn use_missing_locator_string_locator_and_missing_alias() {
    assert_eq!(diagnostics("use enduse"), [(SyntaxInvalidForm, 0, 10)]);
    assert_eq!(
        diagnostics("use \"std.io\" enduse"),
        [(SyntaxInvalidForm, 0, 19)]
    );
    assert_eq!(
        diagnostics("use @std.io as enduse"),
        [(SyntaxInvalidForm, 0, 21)]
    );
    assert_eq!(diagnostics("use as a enduse"), [(SyntaxInvalidForm, 0, 15)]);
}

#[test]
fn use_extra_tokens_repeated_as_and_other_sections() {
    assert_eq!(
        diagnostics("use @a @b enduse"),
        [(SyntaxInvalidForm, 0, 16)]
    );
    assert_eq!(
        diagnostics("use @a as b c enduse"),
        [(SyntaxInvalidForm, 0, 20)]
    );
    assert_eq!(
        diagnostics("use @a if x endif enduse"),
        [(SyntaxInvalidForm, 0, 24)]
    );
    assert_eq!(
        diagnostics("use @a as b as c enduse"),
        [(SyntaxDuplicateSection, 12, 14)]
    );
    assert_eq!(
        diagnostics("use @a visibility @public enduse"),
        [(SyntaxInvalidSection, 7, 25)]
    );
}

#[test]
fn top_literal() {
    assert_eq!(diagnostics("1i32"), [(SyntaxInvalidForm, 0, 4)]);
}

#[test]
fn top_paren() {
    let document = source("(defn main () void void)");
    assert_eq!(codes(document.diagnostics()), [(SyntaxInvalidForm, 0, 24)]);
    let root = document.root().unwrap();
    assert_eq!(root.children().len(), 1);
    assert_eq!(root.children()[0].kind(), SyntaxKind::Error);
}

#[test]
fn a_run_of_top_level_tokens_is_one_invalid_form_and_keeps_its_constructs_whole() {
    assert_eq!(
        diagnostics("x y z\nuse @a enduse"),
        [(SyntaxInvalidForm, 0, 5)]
    );
    assert_eq!(
        diagnostics("if c then x endif"),
        [(SyntaxInvalidForm, 0, 17)]
    );
    assert_eq!(diagnostics("endfn"), [(SyntaxUnmatchedDelimiter, 0, 5)]);
}

#[test]
fn closer_wrong() {
    let document = source("use @std.io endfn");
    assert_eq!(
        codes(document.diagnostics()),
        [
            (SyntaxUnmatchedDelimiter, 0, 3),
            (SyntaxUnmatchedDelimiter, 12, 17)
        ]
    );
    let uses = module(&document).unwrap().uses();
    assert_eq!(uses[0].locator().unwrap().text(), "@std.io");
    let use_node = uses[0].construct().node();
    assert!(
        use_node
            .children()
            .iter()
            .any(|child| child.kind() == SyntaxKind::Error)
    );
    assert_eq!(
        use_node.children().last().unwrap().kind(),
        SyntaxKind::Missing
    );
}

#[test]
fn closer_by_kind() {
    let document = source("fn f returns r: I32 do match x endfn");
    assert_eq!(
        codes(document.diagnostics()),
        [(SyntaxUnmatchedDelimiter, 31, 36)]
    );
    let unmatched = &document.diagnostics()[0];
    assert_eq!(unmatched.related().len(), 1);
    assert_eq!(unmatched.related()[0].span.start(), 23);
    assert_eq!(unmatched.related()[0].span.end(), 28);
    // A bracket closer resolves by kind through constructs too.
    let document = source("fn f takes {x: I32 endfn");
    assert_eq!(
        codes(document.diagnostics()),
        [(SyntaxUnmatchedDelimiter, 19, 24)]
    );
}

#[test]
fn closer_eof() {
    assert_eq!(diagnostics("fn f"), [(SyntaxUnmatchedDelimiter, 0, 2)]);
    // Trailing whitespace stays outside a construct closed as missing.
    assert_eq!(diagnostics("fn f\n\n"), [(SyntaxUnmatchedDelimiter, 0, 2)]);
}

#[test]
fn duplicate_section() {
    assert_eq!(
        diagnostics("fn f returns r: I32 returns s: I32 do return 1i32 endfn"),
        [(SyntaxDuplicateSection, 20, 27)]
    );
}

#[test]
fn do_comes_last_and_the_name_comes_first() {
    assert_eq!(
        diagnostics("fn f returns r: I32 do return x visibility @public endfn"),
        [(SyntaxInvalidForm, 0, 56)]
    );
    assert_eq!(
        diagnostics("fn returns r: I32 do return 1i32 endfn"),
        [(SyntaxInvalidForm, 0, 38)]
    );
    // Inside a body, `do` opens a block closed by `enddo`.
    assert!(diagnostics("fn f returns r: I32 do do return x enddo endfn").is_empty());
}

#[test]
fn every_form_is_read_in_its_own_grammar() {
    for text in [
        "type A of U8 endtype",
        "interface I endinterface",
        "let x: I32 be 1i32 endlet",
        "fn f returns r: I32 do return 1i32 endfn",
        "proc p returns r: Void do return void endproc",
        "test \"t\" do endtest",
    ] {
        assert!(diagnostics(text).is_empty(), "{text}");
    }
}

#[test]
fn recovery_then_valid() {
    let document = source("use @a enduse\n???\nuse @b enduse");
    assert_eq!(codes(document.diagnostics()), [(SyntaxInvalidForm, 14, 17)]);
    let uses = module(&document).unwrap().uses();
    let locators: Vec<&str> = uses
        .iter()
        .map(|view| view.locator().unwrap().text())
        .collect();
    assert_eq!(locators, ["@a", "@b"]);
}

/// Checks the tree invariant and the diagnostic spans of one source text.
fn check_invariants(text: &str) {
    let document = source(text);
    let root = document.root().expect("valid UTF-8 has a tree");
    let mut at = 0;
    for leaf in root.leaves() {
        assert_eq!(leaf.span().start(), at, "leaves are contiguous in {text:?}");
        assert_eq!(
            leaf.kind() == SyntaxKind::Missing,
            leaf.span().is_empty(),
            "{text:?}"
        );
        at = leaf.span().end();
    }
    assert_eq!(at, text.len(), "the leaves cover {text:?}");
    for diagnostic in document.diagnostics() {
        let span = diagnostic.primary_span();
        assert!(span.end() <= text.len(), "{text:?}");
        if diagnostic.code() != SyntaxMissingSeparator {
            assert!(!span.is_empty(), "{text:?}: {:?}", diagnostic.code());
        }
    }
    let _ = tree_document(&document);
    if let Some(module) = module(&document) {
        for declaration in module.declarations() {
            let _ = declaration.construct().sections();
            if let Declaration::Let(value) = declaration {
                let _ = (value.name(), value.written_type(), value.visibility());
                crate::expr::walk(value.value());
            }
        }
    }
}

#[test]
fn random_keyword_sequences_never_panic_and_keep_every_byte_in_one_leaf() {
    const OTHER: &[&str] = &[
        "f",
        "x",
        "I32",
        "@std.io",
        "\"s\"",
        "1i32",
        "{",
        "}",
        "(",
        ")",
        "[",
        "]",
        ":",
        ".",
        "_",
        "?",
        "// c\n",
        "\"\"\"d\"\"\"",
    ];
    const GAPS: &[&str] = &[" ", "\n", "", "  ", "\r\n"];
    let mut random = Random::new(0x5EC7_10A5);
    for _ in 0..4000 {
        let len = random.below(24);
        let mut text = String::new();
        for _ in 0..len {
            let pick = random.below(RESERVED_WORDS.len() + OTHER.len());
            text.push_str(
                RESERVED_WORDS
                    .get(pick)
                    .copied()
                    .unwrap_or_else(|| OTHER[pick - RESERVED_WORDS.len()]),
            );
            text.push_str(GAPS[random.below(GAPS.len())]);
        }
        check_invariants(&text);
    }
}

#[test]
fn deep_construct_nesting_is_parsed_checked_and_dropped_without_recursion() {
    const DEPTH: usize = 100_000;
    let text = format!(
        "fn f returns r: I32 do {}x{} endfn",
        "if c then ".repeat(DEPTH),
        " endif".repeat(DEPTH)
    );
    let document = source(&text);
    assert!(document.diagnostics().is_empty());
    assert!(tree_document(&document).unwrap().len() > DEPTH);
    let open = format!("fn f do {}", "match x ".repeat(DEPTH));
    let recovered = source(&open);
    assert_eq!(recovered.diagnostics().len(), DEPTH + 1);
    drop(recovered);
    drop(document);
}
