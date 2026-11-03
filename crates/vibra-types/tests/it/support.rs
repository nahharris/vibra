//! Helpers shared by the `vibra-types` integration tests: the diagnostics of a
//! source checked on its own, reduced to the codes (and spans) the tests assert.

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_types::check_source;

/// The code and primary span of each diagnostic `source` reports.
pub(crate) fn diagnostics(source: &str) -> Vec<(DiagnosticCode, ByteSpan)> {
    check_source("case.vib", source)
        .diagnostics()
        .iter()
        .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span()))
        .collect()
}

/// The code of each diagnostic `source` reports.
pub(crate) fn codes(source: &str) -> Vec<DiagnosticCode> {
    check_source("case.vib", source)
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code())
        .collect()
}
