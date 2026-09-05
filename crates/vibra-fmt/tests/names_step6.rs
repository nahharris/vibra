//! Step 6 name formatting tests.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]

use std::path::Path;

use vibra_fmt::format_source;

#[test]
fn formatter_preserves_valid_name_spelling_and_is_idempotent() {
    let source = "(some.name:   @some.name a.b2-c - @- -: valid-name)";
    for path in [Path::new("names.vib"), Path::new("names.vibon")] {
        let formatted = format_source(path, source).expect("selected loader");
        assert_eq!(
            formatted,
            "(some.name: @some.name a.b2-c - @- -: valid-name)\n"
        );
        assert_eq!(
            format_source(path, &formatted).expect("idempotent formatting"),
            formatted
        );
    }
}

#[test]
fn formatter_preserves_invalid_name_bytes_for_recovery() {
    let source = "(seed a..b @-.name ok)";
    for path in [Path::new("names.vib"), Path::new("names.vibon")] {
        let formatted = format_source(path, source).expect("selected loader");
        assert_eq!(formatted, "(seed a..b @-.name ok)\n");
    }
}
