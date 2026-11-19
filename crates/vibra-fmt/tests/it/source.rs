//! The layout of source documents' top-level forms.

use vibra_fmt::format_document;
use vibra_syntax::parse_source;

/// Formats `text` and checks that formatting the output again changes
/// nothing.
fn format(text: &str) -> String {
    let output = format_document(&parse_source(None, text.as_bytes())).expect("a text");
    let again = format_document(&parse_source(None, output.as_bytes()));
    assert_eq!(again.as_deref(), Some(output.as_str()), "idempotent for {text:?}");
    output
}

#[test]
fn a_use_is_one_line_with_one_trailing_newline() {
    assert_eq!(format("use @std.io enduse"), "use @std.io enduse\n");
    assert_eq!(
        format("use   @billing.cents\n  as   money\nenduse"),
        "use @billing.cents as money enduse\n"
    );
}

#[test]
fn top_level_forms_are_separated_by_one_blank_line() {
    assert_eq!(
        format("use @a enduse use @b enduse\n\n\n\nuse @c enduse\n"),
        "use @a enduse\n\nuse @b enduse\n\nuse @c enduse\n"
    );
}

#[test]
fn comments_stay_on_their_line_or_before_the_next_form() {
    assert_eq!(
        format("// head\nuse @std.io enduse // trail\n\n\n// own line\nuse @std.fs enduse"),
        "// head\nuse @std.io enduse // trail\n\n// own line\nuse @std.fs enduse\n"
    );
    assert_eq!(
        format("// one\n\n\n// two\nuse @a enduse\n// last   "),
        "// one\n\n// two\nuse @a enduse\n\n// last\n"
    );
}

#[test]
fn crlf_line_endings_become_lf() {
    assert_eq!(format("use @std.io enduse\r\n"), "use @std.io enduse\n");
    assert_eq!(
        format("use @a enduse\r\n\r\n\r\nuse @b enduse\r\n"),
        "use @a enduse\n\nuse @b enduse\n"
    );
}

#[test]
fn the_formatter_moves_no_top_level_form() {
    let late = "type A of U8 endtype\n\nuse @std.io enduse";
    assert_eq!(format(late), late);
}

#[test]
fn a_recovered_or_unavailable_document_is_returned_byte_for_byte() {
    for input in [
        "use @a enduse\n???\nuse @b enduse",
        "use @std.io endfn",
        "use  @a  as  where  enduse",
        "fn f returns r: I32 do match x endfn",
        "fn f   returns r: I32 endfn",
        "(defn main () void void)",
    ] {
        assert_eq!(format(input), input);
    }
}

#[test]
fn an_empty_module_formats_to_nothing() {
    assert_eq!(format(""), "");
    assert_eq!(format(" \n\n"), "");
}
