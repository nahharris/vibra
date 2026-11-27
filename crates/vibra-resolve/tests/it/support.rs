//! Fixtures: a package `hello` with the library unit `@app` and the unit
//! `@tests`, built from `(module path, text)` pairs, beside the embedded
//! standard library read from `stdlib/`.

use vibra_diagnostics::{DiagnosticCode, Level};
use vibra_resolve::{
    Anchor, DeclarationKind, EmbeddedUnit, PackageId, PreludeBinding, Resolution,
    ResolveInput, SourceModule, SourceUnit, UnitKind, resolve,
};

/// The embedded standard library's package.
pub(crate) const STD_PACKAGE: (&str, &str) = ("std", "0.0.2");

/// The modules of `stdlib/`, by module path.
pub(crate) const STD_MODULES: [(&str, &[u8]); 8] = [
    (
        "@std.bool",
        include_bytes!("../../../../stdlib/src/std/bool.vib"),
    ),
    (
        "@std.builtin",
        include_bytes!("../../../../stdlib/src/std/builtin.vib"),
    ),
    (
        "@std.bytes",
        include_bytes!("../../../../stdlib/src/std/bytes.vib"),
    ),
    (
        "@std.core",
        include_bytes!("../../../../stdlib/src/std/core.vib"),
    ),
    (
        "@std.iter",
        include_bytes!("../../../../stdlib/src/std/iter.vib"),
    ),
    (
        "@std.option",
        include_bytes!("../../../../stdlib/src/std/option.vib"),
    ),
    (
        "@std.result",
        include_bytes!("../../../../stdlib/src/std/result.vib"),
    ),
    (
        "@std.str",
        include_bytes!("../../../../stdlib/src/std/str.vib"),
    ),
];

/// The prelude table of the type chapter, "The prelude", as the modules of
/// `stdlib/` declare it: each name, its module, and its kind.
pub(crate) const PRELUDE: [(&str, &str, DeclarationKind); 29] = [
    ("Void", "@std.builtin", DeclarationKind::Type),
    ("Never", "@std.builtin", DeclarationKind::Type),
    ("Atom", "@std.builtin", DeclarationKind::Type),
    ("Char", "@std.builtin", DeclarationKind::Type),
    ("I8", "@std.builtin", DeclarationKind::Type),
    ("I16", "@std.builtin", DeclarationKind::Type),
    ("I32", "@std.builtin", DeclarationKind::Type),
    ("I64", "@std.builtin", DeclarationKind::Type),
    ("U8", "@std.builtin", DeclarationKind::Type),
    ("U16", "@std.builtin", DeclarationKind::Type),
    ("U32", "@std.builtin", DeclarationKind::Type),
    ("U64", "@std.builtin", DeclarationKind::Type),
    ("F32", "@std.builtin", DeclarationKind::Type),
    ("F64", "@std.builtin", DeclarationKind::Type),
    ("List", "@std.builtin", DeclarationKind::Type),
    ("Str", "@std.str", DeclarationKind::Type),
    ("Bytes", "@std.bytes", DeclarationKind::Type),
    ("Dict", "@std.builtin", DeclarationKind::Type),
    ("Bool", "@std.bool", DeclarationKind::Type),
    ("true", "@std.bool", DeclarationKind::Value),
    ("false", "@std.bool", DeclarationKind::Value),
    ("Option", "@std.option", DeclarationKind::Type),
    ("Result", "@std.result", DeclarationKind::Type),
    ("Ordering", "@std.core", DeclarationKind::Type),
    ("Equatable", "@std.core", DeclarationKind::Interface),
    ("Ordered", "@std.core", DeclarationKind::Interface),
    ("From", "@std.core", DeclarationKind::Interface),
    ("TryFrom", "@std.core", DeclarationKind::Interface),
    ("Iter", "@std.iter", DeclarationKind::Interface),
];

/// The identity of the top-level declaration `name` of kind `kind` in the
/// standard-library module `module`.
pub(crate) fn std_identity(module: &str, kind: DeclarationKind, name: &str) -> String {
    Anchor::new(STD_PACKAGE.0, STD_PACKAGE.1, module, "", kind, name)
        .identity()
        .as_str()
        .to_owned()
}

/// The embedded unit `@std` over the modules of `stdlib/`.
pub(crate) fn embedded() -> EmbeddedUnit {
    EmbeddedUnit::new(
        PackageId::new(STD_PACKAGE.0, STD_PACKAGE.1),
        "@std",
        STD_MODULES
            .iter()
            .map(|(path, bytes)| SourceModule::new(*path, *path, *bytes))
            .collect(),
        PRELUDE
            .iter()
            .map(|(name, module, kind)| {
                PreludeBinding::new(*name, std_identity(module, *kind, name))
            })
            .collect(),
    )
}

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
        embedded(),
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
