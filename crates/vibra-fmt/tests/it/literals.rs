//! Step 5 literal formatting tests. The canonical character spellings, the
//! 88-column boundary they control, and the retained numeric and string
//! spellings are asserted by the `V1-SRC-READER-literal-*` and
//! `V1-SRC-FMT-literal-character-*` conformance cases.

use vibra_fmt::format_source;

#[test]
fn formatter_preserves_unterminated_escape_bytes_in_both_document_modes() {
    for extension in ["vib", "vibon"] {
        let path = format!("unfinished.{extension}");
        let source = "\"unfinished\\\r\n";
        let formatted = format_source(&path, source).expect("selected mode");
        assert_eq!(formatted, source, "{path}");
    }
}
