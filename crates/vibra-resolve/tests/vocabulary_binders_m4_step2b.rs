//! The closed import-free vocabulary's values at binder sites
//! (`docs/spec/02-type-system.md`, "Language core and standard library"; M4
//! Step 2b). `true` and `false` are names no binder may take. A pattern site
//! never binds them: the name is a constant pattern there.

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_resolve::{ResolveInput, Resolver};

fn diagnostics(source: &str) -> Vec<(DiagnosticCode, ByteSpan)> {
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

/// The one diagnostic `source` reports, which must be
/// `@name.reserved-declaration` at `binder` inside the first `anchor`.
fn rejected(source: &str, anchor: &str, binder: &str) {
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

#[test]
fn a_vocabulary_value_is_rejected_as_a_pure_name_binder() {
    for spelling in ["true", "false"] {
        let labelled =
            format!("(defn f () i32\n  labelled: ({spelling} i32 0i32)\n  0i32)");
        rejected(&labelled, &format!("({spelling} i32 0i32)"), spelling);
        let variadic =
            format!("(defn f () i32\n  variadic: ({spelling} (array i32))\n  0i32)");
        rejected(&variadic, &format!("({spelling} (array i32))"), spelling);
        let lambda = format!(
            "(defn f () i32 (let g (lambda () i32 labelled: ({spelling} i32 0i32) 0i32)) 0i32)"
        );
        rejected(&lambda, &format!("({spelling} i32 0i32)"), spelling);
    }
}

#[test]
fn a_user_module_cannot_declare_a_vocabulary_value() {
    for spelling in ["true", "false"] {
        let def = format!("(def {spelling} i32 1i32)");
        assert_eq!(
            diagnostics(&def),
            vec![(
                DiagnosticCode::NameReservedValueSpelling,
                ByteSpan::new(0, def.len())
            )],
            "{def}"
        );
        let function = format!("(defn {spelling} () i32 1i32)");
        assert_eq!(
            diagnostics(&function),
            vec![(
                DiagnosticCode::NameReservedValueSpelling,
                ByteSpan::new(0, function.len())
            )],
            "{function}"
        );
    }
}
