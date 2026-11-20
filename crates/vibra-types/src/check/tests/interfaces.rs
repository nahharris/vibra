//! Interfaces and implementations: overlap is decided per module, and the
//! integer types carry their comparisons and conversions.

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_resolve::{PackageId, ResolveInput, SourceModule, SourceUnit, UnitKind};

use super::faults;
use crate::check::check_workspace;
use crate::stdlib::load_stdlib;

/// The module whose verdict is compared.
const MAIN: &str = "main.vib";

/// The code and span of every diagnostic `MAIN` gets when it is checked in
/// the unit `@app` beside the modules of `others` (`(path, source)`), then
/// those of every other module.
fn verdicts(main: &str, others: &[(&str, &str)]) -> (Faults, Faults) {
    let Ok(stdlib) = load_stdlib() else {
        let defect = vec![(DiagnosticCode::ToolUnavailable, ByteSpan::new(0, 0))];
        return (defect.clone(), defect);
    };
    let modules = std::iter::once(SourceModule::new(
        "@app.main",
        MAIN,
        main.as_bytes().to_vec(),
    ))
    .chain(others.iter().map(|(path, source)| {
        SourceModule::new(*path, format!("{path}.vib"), source.as_bytes().to_vec())
    }))
    .collect();
    let input = ResolveInput::new(
        PackageId::new("hello", "0.1.0"),
        vec![SourceUnit::new("@app", UnitKind::Lib, modules)],
        stdlib.overlay().embedded_unit(),
    );
    let result = check_workspace(&input, &["@app".to_owned()]);
    let (main, others): (Vec<_>, Vec<_>) = result
        .diagnostics()
        .iter()
        .partition(|diagnostic| diagnostic.source_id() == Some(MAIN));
    let faults = |found: Vec<&vibra_diagnostics::Diagnostic>| {
        found
            .iter()
            .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span()))
            .collect()
    };
    (faults(main), faults(others))
}

/// Codes and primary spans.
type Faults = Vec<(DiagnosticCode, ByteSpan)>;
#[test]
fn overlap_is_decided_without_implementations_elsewhere() {
    let disjoint = "type Celsius of I32\n\
                    visibility @public\n\
                    impl From(I8) fn convert takes {value: I8} returns converted: Self do return Celsius(From.convert(value)) endfn endimpl\n\
                    impl From(I16) fn convert takes {value: I16} returns converted: Self do return Celsius(From.convert(value)) endfn endimpl\n\
                    endtype\n";
    let overlapping = "type Tag(T)\n\
                       case only\n\
                       visibility @public\n\
                       impl From(T) fn convert takes {value: T} returns converted: Self do return Tag.only() endfn endimpl\n\
                       impl From(I8) fn convert takes {value: I8} returns converted: Self do return Tag.only() endfn endimpl\n\
                       endtype\n";
    for (main, receiver, applied) in [
        (disjoint, "Celsius", "Celsius"),
        (overlapping, "Tag", "Tag(I8)"),
    ] {
        // Another module of the package implements its own interface for
        // the same receiver, and `From` for a type of its own.
        let elsewhere = format!(
            "use @app.main.{receiver} enduse\n\
             interface Shout\n\
             fn shout takes {{value: Self}} returns text: Str endfn\n\
             impl {applied} fn shout takes {{value: Self}} returns text: Str do return \"loud\" endfn endimpl\n\
             endinterface\n\
             type Kelvin of I32\n\
             impl From(I8) fn convert takes {{value: I8}} returns converted: Self do return Kelvin(From.convert(value)) endfn endimpl\n\
             impl From({applied}) fn convert takes {{value: {applied}}} returns converted: Self do return Kelvin(0i32) endfn endimpl\n\
             endtype\n"
        );
        let (alone, _) = verdicts(main, &[]);
        let (beside, others) = verdicts(main, &[("@app.extra", &elsewhere)]);
        assert!(others.is_empty(), "{others:?}");
        assert_eq!(beside, alone);
        if main == overlapping {
            let later = main.find("impl From(I8)").map_or(0, |start| start);
            assert_eq!(
                alone,
                [(
                    DiagnosticCode::TypeOverlappingImplementation,
                    ByteSpan::new(later, later + "impl From(I8)".len())
                )]
            );
        } else {
            assert!(alone.is_empty(), "{alone:?}");
        }
    }
}
/// The eight integer types, each with whether it is signed and its width.
const INTEGERS: [(&str, bool, u32); 8] = [
    ("I8", true, 8),
    ("I16", true, 16),
    ("I32", true, 32),
    ("I64", true, 64),
    ("U8", false, 8),
    ("U16", false, 16),
    ("U32", false, 32),
    ("U64", false, 64),
];

/// Whether every value of `source` is a value of `target`.
const fn total(source: (bool, u32), target: (bool, u32)) -> bool {
    match (source.0, target.0) {
        (true, false) => false,
        (false, true) => target.1 > source.1,
        _ => target.1 >= source.1,
    }
}

/// Every ordered pair of distinct integer types, with whether the
/// conversion is total.
fn pairs() -> Vec<(&'static str, &'static str, bool)> {
    INTEGERS
        .iter()
        .flat_map(|&(source, signed, width)| {
            INTEGERS
                .iter()
                .filter(move |&&(target, _, _)| target != source)
                .map(move |&(target, target_signed, target_width)| {
                    (
                        source,
                        target,
                        total((signed, width), (target_signed, target_width)),
                    )
                })
        })
        .collect()
}

#[test]
fn every_integer_pair_converts_by_from_or_try_from() {
    let pairs = pairs();
    assert_eq!(pairs.len(), 56);
    let mut source = String::from("use @std.core.ConversionError enduse\n");
    for (from, to, total) in &pairs {
        if *total {
            source.push_str(&format!(
                "fn c{from}{to} takes {{v: {from}}} returns r: {to} do return From.convert(v) endfn\n"
            ));
        } else {
            source.push_str(&format!(
                "fn c{from}{to} takes {{v: {from}}} returns r: Result({to} ConversionError) do return TryFrom.convert(v) endfn\n"
            ));
        }
    }
    let found = faults(&source);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn no_partial_integer_pair_has_from() {
    let partial = pairs()
        .into_iter()
        .filter(|(_, _, total)| !total)
        .collect::<Vec<_>>();
    assert_eq!(partial.len(), 38);
    let mut source = String::new();
    for (from, to, _) in &partial {
        source.push_str(&format!(
            "fn c{from}{to} takes {{v: {from}}} returns r: {to} do return From.convert(v) endfn\n"
        ));
    }
    let found = faults(&source);
    assert_eq!(found.len(), partial.len(), "{found:?}");
    assert!(
        found
            .iter()
            .all(|(code, _)| *code == DiagnosticCode::TypeUnsatisfiedBound),
        "{found:?}"
    );
}

#[test]
fn no_total_integer_pair_has_try_from() {
    let mut source = String::from("use @std.core.ConversionError enduse\n");
    let total = pairs()
        .into_iter()
        .filter(|(_, _, total)| *total)
        .collect::<Vec<_>>();
    assert_eq!(total.len(), 18);
    for (from, to, _) in &total {
        source.push_str(&format!(
            "fn c{from}{to} takes {{v: {from}}} returns r: Result({to} ConversionError) do return TryFrom.convert(v) endfn\n"
        ));
    }
    let found = faults(&source);
    assert_eq!(found.len(), total.len(), "{found:?}");
    assert!(
        found
            .iter()
            .all(|(code, _)| *code == DiagnosticCode::TypeUnsatisfiedBound),
        "{found:?}"
    );
}

#[test]
fn every_integer_and_char_is_equatable_and_ordered() {
    let mut source = String::new();
    for name in INTEGERS.iter().map(|(name, _, _)| *name).chain(["Char"]) {
        source.push_str(&format!(
            "fn e{name} takes {{a: {name} b: {name}}} returns r: Bool do return Equatable.equal(a b) endfn\n\
             fn o{name} takes {{a: {name} b: {name}}} returns r: Ordering do return Ordered.compare(a b) endfn\n\
             fn l{name} takes {{a: {name} b: {name}}} returns r: Bool do return Ordered.less(a b) endfn\n"
        ));
    }
    let found = faults(&source);
    assert!(found.is_empty(), "{found:?}");
}
