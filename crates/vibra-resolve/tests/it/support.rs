//! Fixtures: a package `hello` with the library unit `@app` and the unit
//! `@tests`, built from `(module path, text)` pairs.

use vibra_diagnostics::{DiagnosticCode, Level};
use vibra_resolve::{
    PackageId, Resolution, ResolveInput, SourceModule, SourceUnit, UnitKind, resolve,
};

/// The source identity of a module path: `@app.a.b` is `src/app/a/b.vib`,
/// `@tests.t` is `tests/t.vib`.
pub(crate) fn source_id(path: &str) -> String {
    let parts = path.trim_start_matches('@').split('.').collect::<Vec<_>>();
    match parts.split_first() {
        Some((&"tests", rest)) => format!("tests/{}.vib", rest.join("/")),
        Some((unit, rest)) => format!("src/{unit}/{}.vib", rest.join("/")),
        None => String::new(),
    }
}

/// Resolves the modules `modules`, each a module path and its text.
pub(crate) fn resolve_modules(modules: &[(&str, &str)]) -> Resolution {
    let mut app = Vec::new();
    let mut tests = Vec::new();
    for (path, text) in modules {
        let module = SourceModule::new(*path, source_id(path), text.as_bytes());
        if path.starts_with("@tests.") {
            tests.push(module);
        } else {
            app.push(module);
        }
    }
    resolve(&ResolveInput::new(
        PackageId::new("hello", "0.1.0"),
        vec![
            SourceUnit::new("@app", UnitKind::Lib, app),
            SourceUnit::new("@tests", UnitKind::Tests, tests),
        ],
    ))
}

/// Resolves one module `@app.main`.
pub(crate) fn resolve_main(text: &str) -> Resolution {
    resolve_modules(&[("@app.main", text)])
}

/// One diagnostic, flattened: code, source, start, end, and the related
/// spans as (source, start, end).
pub(crate) type Flat = (
    DiagnosticCode,
    String,
    usize,
    usize,
    Vec<(String, usize, usize)>,
);

/// The diagnostics of `resolution`, flattened.
pub(crate) fn flat(resolution: &Resolution) -> Vec<Flat> {
    resolution
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code(),
                diagnostic.source_id().unwrap_or_default().to_owned(),
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end(),
                diagnostic
                    .related()
                    .iter()
                    .map(|related| {
                        (
                            related.source_id.clone().unwrap_or_default(),
                            related.span.start(),
                            related.span.end(),
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

/// The diagnostic codes of `resolution`, in order.
pub(crate) fn codes(resolution: &Resolution) -> Vec<DiagnosticCode> {
    resolution
        .diagnostics()
        .iter()
        .map(vibra_diagnostics::Diagnostic::code)
        .collect()
}

/// Asserts `resolution` has no diagnostic.
pub(crate) fn assert_clean(resolution: &Resolution) {
    assert!(
        resolution.diagnostics().is_empty(),
        "{:#?}",
        flat(resolution)
    );
}

/// The byte span of the `occurrence`-th (from zero) `needle` in `text`.
pub(crate) fn span_of(text: &str, needle: &str, occurrence: usize) -> (usize, usize) {
    let start = text
        .match_indices(needle)
        .nth(occurrence)
        .map(|(start, _)| start)
        .unwrap_or_else(|| {
            panic!("`{needle}` occurs fewer than {} times", occurrence + 1)
        });
    (start, start + needle.len())
}

/// Asserts `resolution` has exactly one diagnostic, an error of `code` in
/// `@app.main` at `span`, with `related` spans in `@app.main`.
pub(crate) fn assert_one(
    resolution: &Resolution,
    code: DiagnosticCode,
    span: (usize, usize),
    related: &[(usize, usize)],
) {
    let found = flat(resolution);
    let expected = vec![(
        code,
        "src/app/main.vib".to_owned(),
        span.0,
        span.1,
        related
            .iter()
            .map(|(start, end)| ("src/app/main.vib".to_owned(), *start, *end))
            .collect::<Vec<_>>(),
    )];
    assert_eq!(found, expected);
    if code != DiagnosticCode::StyleCasing {
        assert_eq!(code.level(), Level::Error);
    }
}
