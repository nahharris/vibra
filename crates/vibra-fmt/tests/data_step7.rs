//! Step 7 canonical VIBON formatting tests.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]

use std::path::Path;

use vibra_fmt::format_source;

#[test]
fn formatter_canonicalizes_valid_data_and_sorts_generic_map_keys() {
    let source = "(map @z 2 @a \\u0061)";
    let formatted =
        format_source(Path::new("values.vibon"), source).expect("data mode");
    assert_eq!(formatted, "(map @a \\a @z 2)\n");
    assert_eq!(
        format_source(Path::new("values.vibon"), &formatted).expect("idempotent data"),
        formatted
    );
}

#[test]
fn formatter_preserves_invalid_data_bytes() {
    let source = "(record value)\r\n";
    let formatted =
        format_source(Path::new("invalid.vibon"), source).expect("data mode");
    assert_eq!(formatted, source);
}

#[test]
fn formatter_canonicalizes_source_and_data_separately() {
    let source = "(array   @value 1)";
    assert_eq!(
        format_source(Path::new("values.vibon"), source).expect("data mode"),
        "(array @value 1)\n"
    );
    assert_eq!(
        format_source(Path::new("values.vib"), source).expect("source mode"),
        "(array @value 1)\n"
    );
}

#[test]
fn formatter_preserves_comments_around_valid_data() {
    let source = "; before\r\n(array   @value)\r\n; after\r\n";
    let formatted =
        format_source(Path::new("commented.vibon"), source).expect("data mode");
    assert_eq!(formatted, "; before\n(array @value)\n\n; after\n");
}
