//! Helpers shared by the `vibra-resolve` integration tests: resolving one
//! module in isolation and reducing its diagnostics to what the tests assert.

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_resolve::{ResolveInput, Resolver};

/// The code and primary span of each diagnostic resolving `source` as the one
/// module of a package reports.
pub(crate) fn diagnostics(source: &str) -> Vec<(DiagnosticCode, ByteSpan)> {
    Resolver::resolve(ResolveInput::single_module(
        "demo",
        "1.0.0",
        "app",
        "main",
        source.as_bytes(),
    ))
    .diagnostics()
    .iter()
    .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span()))
    .collect()
}

/// The code of each diagnostic resolving `source` reports.
pub(crate) fn codes(source: &str) -> Vec<DiagnosticCode> {
    diagnostics(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

/// The one diagnostic `source` reports, which must be
/// `@name.reserved-declaration` at `binder` inside the first `anchor`.
pub(crate) fn rejected(source: &str, anchor: &str, binder: &str) {
    // The binder is the first spelling that is not a qualified path's head.
    fn binder_offset(anchor: &str, binder: &str) -> usize {
        anchor
            .match_indices(binder)
            .map(|(index, _)| index)
            .find(|index| !anchor[index + binder.len()..].starts_with('.'))
            .expect("binder")
    }
    let start = source.find(anchor).expect("anchor") + binder_offset(anchor, binder);
    assert_eq!(
        diagnostics(source),
        vec![(
            DiagnosticCode::NameReservedDeclaration,
            ByteSpan::new(start, start + binder.len())
        )],
        "{source}"
    );
}
