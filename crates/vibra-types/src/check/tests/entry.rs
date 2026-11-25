//! Unit tests of the entry of a binary target. Expectations come from the
//! programs chapter, "Targets" ("It MUST resolve to a module-level `proc`
//! with an entry signature"; "A parameter of an operation interface type,
//! or of `Option` of one, is a leaf, named by the parameter … one leaf for
//! each field, named by the parameter and the field, and so on through
//! nested records"; "`effects` … is a row in data form"), and "Modules and
//! imports" ("Parts that remain resolve against the module's declarations";
//! re-exports do not exist).

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_resolve::{PackageId, ResolveInput, SourceModule, SourceUnit, UnitKind};

use super::at;
use crate::check::{
    CheckResult, DataReference, GrantShape, TargetEntry, check_targets, check_workspace,
};
use crate::stdlib::load_stdlib;

/// The project file the references are written in.
const PROJECT: &str = "project.vibon";

/// The imports of the fixtures that take grants.
const IMPORTS: &str = "use @std.env.EnvReadArgs enduse\n\nuse @std.fs.DirReadText enduse\n\n\
                       use @std.fs.DirWriteText enduse\n\nuse @std.io.StdoutText enduse\n\n\
                       use @std.process.ExitCode enduse\n\n";

/// The input of a binary unit `@app` whose one module `@app.main` holds
/// `source`.
fn input(source: &str) -> Result<ResolveInput, String> {
    let stdlib = load_stdlib().map_err(|error| error.to_string())?;
    Ok(ResolveInput::new(
        PackageId::new("hello", "0.1.0"),
        vec![SourceUnit::new(
            "@app",
            UnitKind::Bin,
            vec![SourceModule::new(
                "@app.main",
                "src/app/main.vib",
                source.as_bytes().to_vec(),
            )],
        )],
        stdlib.overlay().embedded_unit(),
    ))
}

/// The check of `source` with one target whose `entry` is `entry` and whose
/// `effects` list is `effects`, each reference at a span of its own.
fn check(source: &str, entry: &str, effects: &[&str]) -> Result<CheckResult, String> {
    let reference = |index: usize, path: &str| {
        DataReference::new(path, ByteSpan::new(index * 100, index * 100 + path.len()))
    };
    let target = TargetEntry::new(
        PROJECT,
        Some(reference(0, entry)),
        effects
            .iter()
            .enumerate()
            .map(|(index, path)| reference(index + 1, path))
            .collect(),
    );
    Ok(check_targets(
        &input(source)?,
        &["@app".to_owned()],
        &[target],
    ))
}

/// The code and span of every diagnostic of the one target.
fn codes(result: &CheckResult) -> Vec<(DiagnosticCode, ByteSpan)> {
    result
        .targets()
        .iter()
        .flat_map(|target| target.diagnostics())
        .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span()))
        .collect()
}

#[test]
fn the_leaves_are_named_by_the_parameter_and_each_field() -> Result<(), String> {
    let source = format!(
        "{IMPORTS}proc main\n  takes {{args: EnvReadArgs out: Option(StdoutText) \
         logs: {{writer: DirWriteText inner: {{reader: Option(DirReadText)}}}}}}\n  \
         returns code: ExitCode\n  do\n    return ExitCode(0u8)\nendproc\n"
    );
    let result = check(&source, "@app.main.main", &[])?;
    assert!(!result.has_errors(), "{:?}", result.diagnostics());
    let [target] = result.targets() else {
        return Err("one target".to_owned());
    };
    assert!(
        target.diagnostics().is_empty(),
        "{:?}",
        target.diagnostics()
    );
    let signature = target.signature().ok_or("the entry has a signature")?;
    assert_eq!(signature.function(), "app.main.main");
    assert_eq!(signature.source_id(), "src/app/main.vib");
    assert_eq!(signature.declaration(), at(&source, "main", 0));
    let leaves = signature
        .leaves()
        .iter()
        .map(|leaf| {
            (
                leaf.name().to_owned(),
                leaf.leaf().interface().path().to_owned(),
                leaf.leaf().optional(),
            )
        })
        .collect::<Vec<_>>();
    let leaf = |name: &str, path: &str, optional: bool| {
        (name.to_owned(), path.to_owned(), optional)
    };
    assert_eq!(
        leaves,
        [
            leaf("@args", "std.env.EnvReadArgs", false),
            leaf("@out", "std.io.StdoutText", true),
            leaf("@logs.inner.reader", "std.fs.DirReadText", true),
            leaf("@logs.writer", "std.fs.DirWriteText", false),
        ]
    );
    let Some((_, GrantShape::Record(fields))) = signature.parameters().get(2) else {
        return Err("the third parameter is a record".to_owned());
    };
    assert_eq!(fields.len(), 2);
    Ok(())
}

#[test]
fn the_row_keeps_each_performs_entry_and_its_span() -> Result<(), String> {
    let source = format!(
        "{IMPORTS}proc main takes {{args: EnvReadArgs}} returns code: ExitCode \
         performs {{StdoutText EnvReadArgs}} do return ExitCode(0u8) endproc\n"
    );
    let result = check(&source, "@app.main.main", &["@std.io.StdoutText"])?;
    let signature = result
        .targets()
        .first()
        .and_then(|target| target.signature())
        .ok_or("the entry has a signature")?;
    let entries = signature
        .row_entries()
        .iter()
        .map(|entry| (entry.interface().path().to_owned(), entry.span()))
        .collect::<Vec<_>>();
    assert_eq!(
        entries,
        [
            ("std.io.StdoutText".to_owned(), at(&source, "StdoutText", 1)),
            (
                "std.env.EnvReadArgs".to_owned(),
                at(&source, "EnvReadArgs", 2)
            ),
        ]
    );
    assert_eq!(
        signature
            .row()
            .roots()
            .iter()
            .map(|root| root.path().to_owned())
            .collect::<Vec<_>>(),
        ["std.env.EnvReadArgs", "std.io.StdoutText"]
    );
    let ceiling = result
        .targets()
        .first()
        .and_then(|target| target.ceiling())
        .ok_or("the list is a row")?;
    assert_eq!(
        ceiling
            .iter()
            .map(|(root, span)| (root.path().to_owned(), *span))
            .collect::<Vec<_>>(),
        [("std.io.StdoutText".to_owned(), ByteSpan::new(100, 118))]
    );
    Ok(())
}

#[test]
fn a_reference_names_a_declaration_of_the_module_never_an_import() -> Result<(), String>
{
    // `ExitCode` is imported into `@app.main`; the reference
    // `@app.main.ExitCode` reaches no declaration of the module.
    let source = "use @std.process.ExitCode enduse\n\n\
                  proc main returns code: ExitCode do return ExitCode(0u8) endproc\n";
    let result = check(source, "@app.main.ExitCode", &["@app.main.ExitCode"])?;
    assert_eq!(
        codes(&result),
        [
            (DiagnosticCode::NameUnknownSymbol, ByteSpan::new(0, 18)),
            (
                DiagnosticCode::EffectInvalidReference,
                ByteSpan::new(100, 118)
            ),
        ]
    );
    let [target] = result.targets() else {
        return Err("one target".to_owned());
    };
    assert!(target.signature().is_none() && target.ceiling().is_none());
    assert!(
        target
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.source_id() == Some(PROJECT))
    );
    Ok(())
}

#[test]
fn naming_an_entry_changes_nothing_the_check_reports_of_the_source()
-> Result<(), String> {
    // The entry is private; naming it is no import and no access.
    let source = "use @std.process.ExitCode enduse\n\n\
                  proc start returns code: ExitCode do return ExitCode(0u8) endproc\n";
    let with_target = check(source, "@app.main.start", &[])?;
    let without = check_workspace(&input(source)?, &["@app".to_owned()]);
    assert_eq!(with_target.diagnostics(), without.diagnostics());
    assert!(with_target.module_set().is_some());
    assert!(with_target.targets().first().is_some_and(|target| {
        target.diagnostics().is_empty() && target.signature().is_some()
    }));
    assert!(without.targets().is_empty());
    Ok(())
}

#[test]
fn members_cases_and_fields_have_their_own_kind() -> Result<(), String> {
    let source = "use @std.process.ExitCode enduse\n\n\
                  interface Log\n  proc append takes {log: Self entry: Str} returns done: Void endproc\nendinterface\n\n\
                  interface Shown\n  fn show takes {value: Self} returns text: Str endfn\nendinterface\n\n\
                  type Mode\n  case quiet\n  case loud\nendtype\n\n\
                  type Pair of {left: I32 right: I32} endtype\n\n\
                  proc main returns code: ExitCode do return ExitCode(0u8) endproc\n";
    let cases = [
        // An interface member is a member, so no module-level `proc`.
        (
            "@app.main.Log.append",
            DiagnosticCode::ProjectInvalidEntrySignature,
        ),
        ("@app.main.Mode.loud", DiagnosticCode::NameWrongEntityKind),
        ("@app.main.Pair.left", DiagnosticCode::NameWrongEntityKind),
        ("@app.main.Log", DiagnosticCode::NameWrongEntityKind),
        ("@app.main.Mode.nope", DiagnosticCode::NameUnknownSymbol),
        ("@app.main.main.more", DiagnosticCode::NameUnknownSymbol),
        ("@app.nope.main", DiagnosticCode::NameUnknownSymbol),
        ("@app", DiagnosticCode::NameUnknownSymbol),
    ];
    for (entry, code) in cases {
        let result = check(source, entry, &[])?;
        assert_eq!(
            codes(&result),
            [(code, ByteSpan::new(0, entry.len()))],
            "{entry}"
        );
    }
    let member = check(source, "@app.main.Log.append", &[])?;
    let related = member
        .targets()
        .first()
        .and_then(|target| target.diagnostics().first())
        .and_then(|diagnostic| diagnostic.related().first())
        .map(|related| (related.source_id.clone(), related.span));
    assert_eq!(
        related,
        Some((Some("src/app/main.vib".to_owned()), at(source, "append", 0)))
    );
    for (effect, why) in [
        ("@app.main.Shown", "no operation interface"),
        ("@app.main.Log.append", "a member"),
        ("@app.main.main", "a callable"),
        ("@app.main.Mode", "a type"),
        ("@app.main", "a module"),
    ] {
        let result = check(source, "@app.main.main", &[effect])?;
        assert_eq!(
            codes(&result),
            [(
                DiagnosticCode::EffectInvalidReference,
                ByteSpan::new(100, 100 + effect.len())
            )],
            "{effect}: {why}"
        );
    }
    Ok(())
}

#[test]
fn a_repeat_is_related_to_the_first_in_the_project_file() -> Result<(), String> {
    let source = "use @std.process.ExitCode enduse\n\n\
                  proc main returns code: ExitCode do return ExitCode(0u8) endproc\n";
    let result = check(
        source,
        "@app.main.main",
        &[
            "@std.io.StdoutText",
            "@std.fs.DirList",
            "@std.io.StdoutText",
            "@std.io.StdoutText",
        ],
    )?;
    let [target] = result.targets() else {
        return Err("one target".to_owned());
    };
    let repeats = target
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code(),
                diagnostic.primary_span(),
                diagnostic
                    .related()
                    .iter()
                    .map(|related| (related.source_id.clone(), related.span))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    let first = vec![(Some(PROJECT.to_owned()), ByteSpan::new(100, 118))];
    assert_eq!(
        repeats,
        [
            (
                DiagnosticCode::EffectDuplicateRoot,
                ByteSpan::new(300, 318),
                first.clone()
            ),
            (
                DiagnosticCode::EffectDuplicateRoot,
                ByteSpan::new(400, 418),
                first
            ),
        ]
    );
    assert!(target.ceiling().is_none());
    assert!(target.signature().is_some());
    Ok(())
}

#[test]
fn a_header_with_its_own_fault_is_reported_once() -> Result<(), String> {
    // The parameter's type names nothing: resolution reports it, and the
    // entry adds nothing.
    let source = "use @std.process.ExitCode enduse\n\n\
                  proc main takes {x: Missing} returns code: ExitCode do return ExitCode(0u8) endproc\n";
    let result = check(source, "@app.main.main", &[])?;
    assert!(result.has_errors());
    assert!(codes(&result).is_empty());
    // A row that names a type is the row's fault, at the row; the entry
    // adds nothing.
    let source = "use @std.process.ExitCode enduse\n\ntype Plain of U8 endtype\n\n\
                  proc main returns code: ExitCode performs {Plain} do return ExitCode(0u8) endproc\n";
    let result = check(source, "@app.main.main", &[])?;
    assert_eq!(
        result
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code())
            .collect::<Vec<_>>(),
        [DiagnosticCode::EffectInvalidReference]
    );
    assert!(codes(&result).is_empty());
    assert!(
        result
            .targets()
            .first()
            .is_some_and(|target| target.signature().is_none())
    );
    Ok(())
}

#[test]
fn a_record_of_grants_fifty_records_deep_is_a_grant_shape() -> Result<(), String> {
    // The walk is bounded by the type depth, which the checker enforces
    // before the entry is read.
    let depth = 50;
    let mut ty = "DirReadText".to_owned();
    for _ in 0..depth {
        ty = format!("{{inner: {ty}}}");
    }
    let source = format!(
        "{IMPORTS}proc main takes {{deep: {ty}}} returns code: ExitCode do return ExitCode(0u8) endproc\n"
    );
    let result = check(&source, "@app.main.main", &[])?;
    assert!(!result.has_errors(), "{:?}", result.diagnostics());
    let leaves = result
        .targets()
        .first()
        .and_then(|target| target.signature())
        .map(crate::check::EntrySignature::leaves)
        .ok_or("the entry has a signature")?;
    let expected = format!("@deep{}", ".inner".repeat(depth));
    assert_eq!(
        leaves.iter().map(|leaf| leaf.name()).collect::<Vec<_>>(),
        [expected.as_str()]
    );
    Ok(())
}
