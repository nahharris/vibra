//! Step 4 minimal canonical formatter tests. The layout, comment, and recovery
//! behavior is asserted by the `V1-SRC-FMT` and `V1-SRC-READER` conformance
//! cases; only the stack-depth guarantee, which a case cannot state, is here.

#![allow(clippy::expect_used)]

use std::path::Path;

use vibra_fmt::format_source;

#[test]
fn formatter_handles_deeply_nested_accepted_lists_without_recursive_rendering() {
    const DEPTH: usize = 4_000;
    let source = format!("{}value{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
    let formatted = format_source(Path::new("deep.vib"), &source)
        .expect(".vib is a supported source document");

    assert!(formatted.ends_with('\n'));
    assert!(!formatted.is_empty());
}
