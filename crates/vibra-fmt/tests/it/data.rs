//! Step 7 canonical VIBON formatting tests. Canonical data layout, dict key
//! order, and comments are asserted by the `V1-SRC-FMT-vibon-*` conformance
//! cases; the bytes of a document the data loader rejects are here.

use vibra_fmt::format_source;

#[test]
fn formatter_preserves_invalid_data_bytes() {
    for (path, source) in [
        ("invalid.vibon", "(record value)\r\n"),
        ("names.vibon", "(array a..b @-.name ok)"),
    ] {
        let formatted = format_source(path, source).expect("data mode");
        assert_eq!(formatted, source, "{path}");
    }
}
