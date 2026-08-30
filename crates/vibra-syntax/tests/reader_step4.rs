//! Step 4 reader-spine conformance tests.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]

use std::path::Path;

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_syntax::{
    DocumentMode, DocumentModeError, SyntaxKind, TokenKind, parse_document,
    parse_with_mode,
};

#[test]
fn lexer_preserves_utf8_tokens_trivia_and_half_open_byte_spans() {
    let source = "(α ; note\r\n  β)";
    let lexed = vibra_syntax::lex(source);
    let tokens = lexed.tokens();

    assert_eq!(tokens[0].kind(), TokenKind::OpenParen);
    assert_eq!(tokens[0].text(), "(");
    assert_eq!(tokens[0].span(), ByteSpan::new(0, 1));
    assert_eq!(tokens[1].kind(), TokenKind::Atom);
    assert_eq!(tokens[1].text(), "α");
    assert_eq!(tokens[1].span(), ByteSpan::new(1, 3));
    assert_eq!(tokens[2].kind(), TokenKind::Whitespace);
    assert_eq!(tokens[2].text(), " ");
    assert_eq!(tokens[3].kind(), TokenKind::LineComment);
    assert_eq!(tokens[3].text(), "; note");
    assert_eq!(tokens[4].kind(), TokenKind::Whitespace);
    assert_eq!(tokens[4].text(), "\r\n  ");
    assert_eq!(tokens[5].text(), "β");
    assert_eq!(tokens[5].span(), ByteSpan::new(14, 16));
    assert_eq!(tokens.last().expect("EOF token").kind(), TokenKind::Eof);
    assert!(lexed.diagnostics().is_empty());
}

#[test]
fn parser_tree_is_lossless_and_recovers_after_an_unmatched_close() {
    let source = "(alpha (beta)) ) (gamma";
    let document = parse_document(Path::new("main.vib"), source)
        .expect(".vib selects source mode");

    assert_eq!(document.mode(), DocumentMode::Source);
    assert_eq!(document.root().to_source(), source);
    assert!(document.root().contains_kind(SyntaxKind::Error));
    assert_eq!(document.diagnostics().len(), 2);
    assert_eq!(
        document.diagnostics()[0].code(),
        DiagnosticCode::SyntaxUnmatchedDelimiter
    );
    assert_eq!(
        document.diagnostics()[0].primary_span(),
        ByteSpan::new(15, 16)
    );
    assert_eq!(
        document.diagnostics()[1].primary_span(),
        ByteSpan::empty_at(23)
    );
    assert!(!document.accepted());
}

#[test]
fn parser_rejects_sibling_forms_without_required_trivia() {
    let document = parse_document(Path::new("main.vib"), "(a(b))")
        .expect(".vib selects source mode");

    assert_eq!(document.root().to_source(), "(a(b))");
    assert!(!document.accepted());
    assert!(document.recovered());
    assert!(document.root().contains_kind(SyntaxKind::Error));
    assert_eq!(document.diagnostics().len(), 1);
    assert_eq!(
        document.diagnostics()[0].code(),
        DiagnosticCode::SyntaxMissingSeparator
    );
    assert_eq!(
        document.diagnostics()[0].primary_span(),
        ByteSpan::empty_at(2)
    );
}

#[test]
fn parser_recovers_unterminated_quoted_leaf_losslessly() {
    let source = "\"unterminated\n";
    let document = parse_document(Path::new("main.vib"), source)
        .expect(".vib selects source mode");

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
}

#[test]
fn parser_handles_deeply_nested_lists_without_recursive_construction() {
    const DEPTH: usize = 20_000;
    let source = format!("{}value{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
    let document = parse_document(Path::new("deep.vib"), &source)
        .expect(".vib selects source mode");

    assert!(document.accepted());
    assert_eq!(document.root().to_source(), source);

    let mut node = &document.root().children()[0];
    for _ in 0..DEPTH {
        assert_eq!(node.kind(), SyntaxKind::List);
        node = node
            .children()
            .iter()
            .find(|child| matches!(child.kind(), SyntaxKind::List | SyntaxKind::Atom))
            .expect("each nested list has a form child");
    }
    assert_eq!(node.kind(), SyntaxKind::Atom);
    assert_eq!(node.leaf_text(), Some("value"));
}

#[test]
fn empty_root_is_composite_and_has_no_leaf_text() {
    let document = parse_document(Path::new("empty.vib"), "")
        .expect("empty source document parses");

    assert_eq!(document.root().kind(), SyntaxKind::Root);
    assert_eq!(document.root().leaf_text(), None);
    assert_eq!(document.root().to_source(), "");
}

#[test]
fn extension_selects_mode_without_content_sniffing_or_fallback() {
    let source = "(looks-like-source)";
    let source_document = parse_document(Path::new("project.vibon"), source)
        .expect(".vibon selects data mode even for source-shaped text");
    assert_eq!(source_document.mode(), DocumentMode::Data);
    assert!(source_document.accepted());

    assert_eq!(
        DocumentMode::from_path("src/main.vib").expect("source extension"),
        DocumentMode::Source
    );
    assert_eq!(
        DocumentMode::from_path("project.vibon").expect("data extension"),
        DocumentMode::Data
    );
    assert!(matches!(
        DocumentMode::from_path("project.vibra"),
        Err(DocumentModeError::UnsupportedExtension { .. })
    ));

    let wrong_loader =
        parse_with_mode(Path::new("project.vibon"), source, DocumentMode::Source)
            .expect("wrong loader returns a structured diagnostic result");
    assert!(!wrong_loader.accepted());
    assert_eq!(wrong_loader.diagnostics().len(), 1);
    assert_eq!(
        wrong_loader.diagnostics()[0].code(),
        DiagnosticCode::DataInvalidExtension
    );
    assert_eq!(wrong_loader.root().to_source(), "");
}
