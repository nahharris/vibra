//! Step 4 minimal canonical formatter tests.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]

use std::path::Path;

use vibra_fmt::format_source;

#[test]
fn formatter_normalizes_trivia_line_endings_and_top_level_spacing() {
    let source = "(one   two)\r\n(foo (bar))  \r\n\r\n";
    let formatted = format_source(Path::new("main.vib"), source)
        .expect(".vib is a supported source document");

    assert_eq!(formatted, "(one two)\n\n(foo (bar))\n");
}

#[test]
fn formatter_is_idempotent_and_preserves_comments() {
    let source = "; heading\n(alpha\n  ; before beta\n  beta)\n\n(gamma)\n";
    let once = format_source(Path::new("main.vib"), source).expect("format source");
    let twice =
        format_source(Path::new("main.vib"), &once).expect("format canonical source");

    assert_eq!(
        once,
        "; heading\n(alpha\n  ; before beta\n  beta)\n\n(gamma)\n"
    );
    assert_eq!(once, twice);
}

#[test]
fn formatter_keeps_delimiters_blocked_around_multiline_comment_neighbors() {
    let source = "((inner\n; child comment\nvalue)\n; before beta\nbeta)\n";
    let formatted =
        format_source(Path::new("main.vib"), source).expect("format source");

    assert_eq!(
        formatted,
        "(\n  (inner\n    ; child comment\n    value)\n  ; before beta\n  beta)\n"
    );
}

#[test]
fn formatter_keeps_delimiters_separate_from_leading_or_trailing_comments() {
    let leading = format_source(Path::new("main.vib"), "(; leading\nalpha)\n")
        .expect("format leading comment");
    let trailing = format_source(Path::new("main.vib"), "(alpha\n; trailing\n)\n")
        .expect("format trailing comment");
    let only = format_source(Path::new("main.vib"), "(; only\n)\n")
        .expect("format comment-only list");

    assert_eq!(leading, "(\n  ; leading\n  alpha)\n");
    assert_eq!(trailing, "(alpha\n  ; trailing\n)\n");
    assert_eq!(only, "(\n  ; only\n)\n");
}

#[test]
fn formatter_keeps_a_closing_delimiter_within_the_column_limit() {
    let last = "x".repeat(86);
    let commented = format!("(first\n; before last\n{last})\n");
    let bare = format!("(first {last})\n");

    let formatted =
        format_source(Path::new("main.vib"), &commented).expect("format source");
    let uncommented =
        format_source(Path::new("main.vib"), &bare).expect("format source");

    assert_eq!(formatted, format!("(first\n  ; before last\n  {last}\n)\n"));
    // The guard is about columns, not comments: `{last})` would end at
    // column 89.
    assert_eq!(uncommented, format!("(first\n  {last}\n)\n"));
}

#[test]
fn formatter_uses_the_extension_selected_document_mode() {
    let formatted = format_source(Path::new("project.vibon"), "(record   value)\n")
        .expect(".vibon is a supported data document");
    assert_eq!(formatted, "(record value)\n");
}

#[test]
fn formatter_indents_a_long_list_and_nested_long_list_by_two_spaces() {
    let source = "(outer alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo (inner lima mike november oscar))";
    let formatted =
        format_source(Path::new("main.vib"), source).expect("format source");

    // A comment-free multiline list shares both delimiters with its inline
    // edge forms. A hanging `(` above `outer` is not canonical.
    assert_eq!(
        formatted,
        "(outer\n  alpha\n  bravo\n  charlie\n  delta\n  echo\n  foxtrot\n  golf\n  hotel\n  india\n  juliet\n  kilo\n  (inner lima mike november oscar))\n"
    );
}

#[test]
fn formatter_does_not_double_indent_a_nested_multiline_list() {
    let source = "(outer (inner alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike november))";
    let formatted =
        format_source(Path::new("main.vib"), source).expect("format source");

    // Both lists open beside their inline first form, and the outer closing
    // delimiter stacks onto the inner list's closing line rather than
    // orphaning itself below it.
    assert_eq!(
        formatted,
        "(outer\n  (inner\n    alpha\n    bravo\n    charlie\n    delta\n    echo\n    foxtrot\n    golf\n    hotel\n    india\n    juliet\n    kilo\n    lima\n    mike\n    november))\n"
    );
}

#[test]
fn formatter_preserves_recovered_source_and_opaque_leaf_text_exactly() {
    let source = "(opaque   leaf  \r\n";
    let formatted =
        format_source(Path::new("main.vib"), source).expect("format recovered source");

    assert_eq!(formatted, source);
}

#[test]
fn formatter_preserves_missing_separator_recovery_exactly() {
    let source = "(a(b))";
    let formatted =
        format_source(Path::new("main.vib"), source).expect("format recovered source");

    assert_eq!(formatted, source);
}

#[test]
fn formatter_handles_deeply_nested_accepted_lists_without_recursive_rendering() {
    const DEPTH: usize = 4_000;
    let source = format!("{}value{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
    let formatted = format_source(Path::new("deep.vib"), &source)
        .expect(".vib is a supported source document");

    assert!(formatted.ends_with('\n'));
    assert!(!formatted.is_empty());
}

#[test]
fn formatter_preserves_unterminated_quoted_leaf_bytes() {
    let source = "\"unterminated\n";
    let formatted =
        format_source(Path::new("main.vib"), source).expect("format recovered source");

    assert_eq!(formatted, source);
}
