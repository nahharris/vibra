//! Step 6 name formatting tests.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]

use std::path::Path;

use vibra_fmt::format_source;

#[test]
fn formatter_preserves_valid_name_spelling_and_is_idempotent() {
    let source = "(some.name:   @some.name a.b2-c - @- -: valid-name)";
    let formatted =
        format_source(Path::new("names.vib"), source).expect("source loader");
    assert_eq!(
        formatted,
        "(some.name: @some.name a.b2-c - @- -: valid-name)\n"
    );
    assert_eq!(
        format_source(Path::new("names.vib"), &formatted)
            .expect("idempotent formatting"),
        formatted
    );

    let data = "(record some.name:   @some.name version: @some.name)";
    let formatted = format_source(Path::new("names.vibon"), data).expect("data loader");
    assert_eq!(
        formatted,
        "(record some.name: @some.name version: @some.name)\n"
    );
    assert_eq!(
        format_source(Path::new("names.vibon"), &formatted)
            .expect("idempotent formatting"),
        formatted
    );
}

#[test]
fn formatter_preserves_invalid_name_bytes_for_recovery() {
    let source = "(seed a..b @-.name ok)";
    let formatted =
        format_source(Path::new("names.vib"), source).expect("source loader");
    assert_eq!(formatted, "(seed a..b @-.name ok)\n");

    let data = "(array a..b @-.name ok)";
    let formatted = format_source(Path::new("names.vibon"), data).expect("data loader");
    assert_eq!(formatted, data);
}
