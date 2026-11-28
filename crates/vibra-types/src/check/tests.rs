//! Unit tests of the checker. Expectations come from the type chapter.

use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level};
use vibra_interp::MemoryBudget;
use vibra_ir::CheckedModuleSet;
use vibra_resolve::{PackageId, ResolveInput, SourceModule, SourceUnit, UnitKind};

use super::{CheckResult, check_source, check_workspace, interpret};
use crate::stdlib::load_stdlib;

mod bodies;
mod bounds;
mod callables;
mod ceilings;
mod contracts;
mod discharge;
mod entry;
mod grants;
mod interfaces;
mod procs;
mod rows;
mod soundness;
mod testing;
mod values;

const SOURCE: &str = "main.vib";

/// The canonical result of running `source`, or the diagnostics.
fn run(source: &str) -> Result<String, String> {
    let outcome = interpret(SOURCE, source, MemoryBudget::DEFAULT);
    match outcome.run {
        Some(Ok(execution)) => Ok(execution.canonical.clone()),
        Some(Err(error)) => Err(format!("{error:?}")),
        None => Err(format!("{:#?}", outcome.check.diagnostics())),
    }
}

/// The canonical `@value.v2` document of a scalar.
fn value(ty: &str, text: &str) -> String {
    format!("{{format: @value.v2 typeInfo: @{ty} value: {text}}}")
}

/// The code and span of every diagnostic of `source`.
fn faults(source: &str) -> Vec<(DiagnosticCode, ByteSpan)> {
    faults_of(&check_source(SOURCE, source))
}

/// The code and span of every diagnostic of `result`.
fn faults_of(result: &CheckResult) -> Vec<(DiagnosticCode, ByteSpan)> {
    result
        .diagnostics()
        .iter()
        .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span()))
        .collect()
}

/// A diagnostic by its code, level, primary span, and related spans with
/// their messages.
type Seen = (DiagnosticCode, Level, ByteSpan, Vec<(ByteSpan, String)>);

/// Each diagnostic of `diagnostics`.
fn seen(diagnostics: &[Diagnostic]) -> Vec<Seen> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code(),
                diagnostic.level(),
                diagnostic.primary_span(),
                diagnostic
                    .related()
                    .iter()
                    .map(|related| (related.span, related.message.clone()))
                    .collect(),
            )
        })
        .collect()
}

/// Each diagnostic of `source`.
fn seen_in(source: &str) -> Vec<Seen> {
    seen(check_source(SOURCE, source).diagnostics())
}

/// The module set of `source`, which MUST have no diagnostic.
fn accepted(source: &str) -> Result<Arc<CheckedModuleSet>, String> {
    let result = check_source(SOURCE, source);
    if !result.diagnostics().is_empty() {
        return Err(format!("{source}\n{:#?}", result.diagnostics()));
    }
    result
        .module_set()
        .map(Arc::clone)
        .ok_or_else(|| format!("no module set for {source}"))
}

/// A module of a workspace: its path and its source.
type Module<'a> = (&'a str, &'a str);

/// The input of the package `hello` with the units `units`, each its name,
/// its kind, and its modules. A module of `@tests` is read from `tests/`,
/// and a module of another unit from the unit's directory in `src/`.
fn input(units: &[(&str, UnitKind, &[Module<'_>])]) -> Result<ResolveInput, String> {
    let stdlib = load_stdlib().map_err(|error| error.to_string())?;
    let units = units
        .iter()
        .map(|(name, kind, modules)| {
            let root = match kind {
                UnitKind::Tests => "tests".to_owned(),
                _ => format!("src/{}", name.trim_start_matches('@')),
            };
            let modules = modules
                .iter()
                .map(|(path, source)| {
                    let file = path
                        .strip_prefix(name)
                        .unwrap_or(path)
                        .trim_start_matches('.')
                        .replace('.', "/");
                    SourceModule::new(
                        *path,
                        format!("{root}/{file}.vib"),
                        source.as_bytes().to_vec(),
                    )
                })
                .collect();
            SourceUnit::new(*name, *kind, modules)
        })
        .collect();
    Ok(ResolveInput::new(
        PackageId::new("hello", "0.1.0"),
        units,
        stdlib.overlay().embedded_unit(),
    ))
}

/// Checks the library unit `@app` of the modules `app` beside the test unit
/// `@tests` of the modules `tests`, covering `units`.
fn workspace(
    app: &[Module<'_>],
    tests: &[Module<'_>],
    units: &[&str],
) -> Result<CheckResult, String> {
    let input = input(&[
        ("@app", UnitKind::Lib, app),
        ("@tests", UnitKind::Tests, tests),
    ])?;
    let units = units
        .iter()
        .map(|unit| (*unit).to_owned())
        .collect::<Vec<_>>();
    Ok(check_workspace(&input, &units))
}

/// The interfaces `Log`, `Clock`, and `Shown`, a type that implements the
/// first two, and a `proc` that performs `Log`.
const L: &str = "interface Log\n  proc append\n    takes {log: Self entry: Str}\n    \
                 returns done: Void\n  endproc\nendinterface\n\n\
                 interface Clock\n  proc now takes {clock: Self} returns at: I64 endproc\n\
                 endinterface\n\n\
                 interface Shown\n  fn show takes {value: Self} returns text: Str endfn\n\
                 endinterface\n\n\
                 type Memory of {lines: List(Str)}\n  impl Log\n    proc append takes \
                 {log: Self entry: Str} returns done: Void do\n      return void\n    \
                 endproc\n  endimpl\n  impl Clock\n    proc now takes {clock: Self} \
                 returns at: I64 do return 0i64 endproc\n  endimpl\nendtype\n\n\
                 proc note takes {m: Memory} returns done: Void performs {Log} do\n  \
                 return await Log.append(m \"x\")\nendproc\n";

/// `L` and then `rest`.
fn with_l(rest: &str) -> String {
    format!("{L}{rest}\n")
}

/// The span of the `nth` occurrence of `needle` in `source`.
fn at(source: &str, needle: &str, nth: usize) -> ByteSpan {
    let start = source
        .match_indices(needle)
        .nth(nth)
        .map_or(usize::MAX, |(start, _)| start);
    ByteSpan::new(start, start.saturating_add(needle.len()))
}

#[test]
fn the_standard_library_checks_clean() {
    let result = check_source(SOURCE, "fn main returns r: U8 do return 7u8 endfn\n");
    assert!(
        !result.has_errors(),
        "{:#?}\nnotes: {:#?}",
        result.diagnostics(),
        result.notes()
    );
    let names = result
        .functions()
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    for member in ["not", "and", "or", "xor"] {
        assert!(
            names.contains(&format!("std.bool.Bool.{member}").as_str()),
            "{names:?}"
        );
    }
    assert!(result.notes().is_empty(), "{:#?}", result.notes());
}

#[test]
fn ambiguous_literal_has_one_note() {
    let source = "fn main returns r: I32 do let a be 1 endlet return a endfn\n";
    let result = check_source(SOURCE, source);
    let diagnostics = result.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    let Some(diagnostic) = diagnostics.first() else {
        return;
    };
    assert_eq!(diagnostic.code(), DiagnosticCode::TypeAmbiguousInference);
    assert_eq!(diagnostic.primary_span(), at(source, "1", 0));
    assert_eq!(diagnostic.notes().len(), 1);
}

#[test]
fn tail_calls_run_in_constant_depth() {
    let source = "fn count takes {n: I32 acc: I32} returns r: I32 do \
                  if Equatable.equal(n 0i32) then return acc \
                  else return count(I32.sub(n 1i32) I32.add(acc 1i32)) endif endfn\n\
                  fn main returns r: I32 do return count(100000i32 0i32) endfn\n";
    let outcome = interpret(SOURCE, source, MemoryBudget::DEFAULT);
    assert!(
        matches!(outcome.run, Some(Ok(_))),
        "{:?} {:#?}",
        outcome.run,
        outcome.check.diagnostics()
    );
    let Some(Ok(execution)) = outcome.run else {
        return;
    };
    assert_eq!(execution.canonical, value("i32", "100000i32"));
    assert_eq!(execution.instrumentation.max_activation_depth, 1);
}

/// The member functions of `@std.builtin`'s `Dict` and `List` check clean:
/// each is lowered and none is skipped with a note.
#[test]
fn dict_and_list_members_check_clean() {
    let result = check_source(SOURCE, "fn main returns r: U8 do return 7u8 endfn\n");
    assert!(!result.has_errors(), "{:#?}", result.diagnostics());
    let names = result
        .functions()
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    for member in [
        "std.builtin.Dict.entries",
        "std.builtin.List.length",
        "std.builtin.List.append",
        "std.builtin.List.concat",
        "std.builtin.List.slice",
    ] {
        assert!(names.contains(&member), "{member} in {names:?}");
    }
    for note in result.notes() {
        assert!(
            !note.contains("`std.builtin.Dict") && !note.contains("`std.builtin.List"),
            "{note}"
        );
    }
    assert_eq!(
        run("fn main returns r: U64 do return List.length([1i32 2i32]) endfn\n")
            .as_deref(),
        Ok(value("u64", "2u64").as_str())
    );
}

/// The lowered body of the function `name` of an accepted `source`, as its
/// debug text.
fn lowered(source: &str, name: &str) -> String {
    let result = check_source(SOURCE, source);
    assert!(!result.has_errors(), "{:#?}", result.diagnostics());
    let set = result
        .module_set
        .as_ref()
        .map(|set| set.functions().to_vec());
    let function = set
        .into_iter()
        .flatten()
        .find(|function| function.name.ends_with(name));
    format!("{:?}", function.map(|function| function.body))
}

/// A record value and a constructor are lowered with their operands in
/// written order, which the interpreter then runs in that order
/// (`vibra-interp`'s `products_evaluate_operands_in_written_order`): the
/// field `b`, written first, comes first.
#[test]
fn products_lower_operands_in_written_order() {
    let source =
        "fn main returns r: {a: I32 b: I32} do return {b: 2i32 a: 1i32} endfn\n";
    let body = lowered(source, "main");
    assert!(body.contains("Record([(\"b\", "), "{body}");
    let source = "type P of {a: I32 b: I32} endtype\n\
                  fn main returns r: P do return P(b: 2i32 a: 1i32) endfn\n";
    let body = lowered(source, "main");
    assert!(body.contains("Record([(\"b\", "), "{body}");
    let source = "type S case c {a: I32 b: I32} endtype\n\
                  fn main returns r: S do return S.c(b: 2i32 a: 1i32) endfn\n";
    let body = lowered(source, "main");
    assert!(body.contains("fields: [(\"b\", "), "{body}");
}

/// A constructor binds labels exactly when its shape has field names
/// (`docs/spec/02-type-system.md`, "Application").
#[test]
fn constructor_labels_follow_the_shape() {
    let user = "type User of {name: Str id: U64} endtype\n";
    let source =
        format!("{user}fn f returns r: User do return User(\"Ada\" 1u64) endfn\n");
    assert_eq!(
        faults(&source),
        [(
            DiagnosticCode::TypeArgumentMismatch,
            at(&source, "User(\"Ada\" 1u64)", 0)
        )]
    );
    let source = "type Pair of {I32 Str} endtype\n\
                  fn f returns r: Pair do return Pair(a: 1i32 b: \"x\") endfn\n";
    assert_eq!(
        faults(source),
        [
            (
                DiagnosticCode::TypeArgumentMismatch,
                at(source, "a: 1i32", 0)
            ),
            (
                DiagnosticCode::TypeArgumentMismatch,
                at(source, "b: \"x\"", 0)
            ),
        ]
    );
}

/// A type argument is inferred from the expected type when no operand
/// fixes it, and a generic function value takes its arguments from the
/// written type it is bound to.
#[test]
fn type_arguments_from_the_expected_type() {
    let source = "fn main returns r: Option(I32) do return none() endfn\n\
                  fn none(T) returns r: Option(T) do return Option.none() endfn\n";
    assert!(run(source).is_ok(), "{:?}", run(source));
    let source = "fn main returns r: I32 do let f: Fn({I32} I32) be id endlet return f(4i32) endfn\n\
                  fn id(T) takes {x: T} returns r: T do return x endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("i32", "4i32").as_str()));
    let source = "fn id(T) takes {x: T} returns r: T do return x endfn\n\
                  fn main returns r: I32 do let f be id endlet return 1i32 endfn\n";
    assert_eq!(
        faults(source),
        [(DiagnosticCode::TypeAmbiguousInference, at(source, "id", 1))]
    );
}

/// The representation is hidden from other modules only: the declaring
/// module builds and reads a private representation.
#[test]
fn private_representation_is_visible_in_its_module() {
    let source = "type Token of {id: U64} visibility @public representation @private endtype\n\
                  fn main returns r: U64 do let t be Token(id: 3u64) endlet return t.id endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("u64", "3u64").as_str()));
}

/// Recursion through a `Dict` or a callable is finite; a parameter held in
/// place is expanded, so holding a type in a `Box` of itself repeats.
#[test]
fn finite_size_through_collections_and_callables() {
    for source in [
        "type T of {kids: Dict(Str T)} endtype\n",
        "type T of {next: Fn({} T)} endtype\n",
        "type Box(V) of {item: V} endtype\ntype T of {kids: List(Box(T))} endtype\n",
    ] {
        assert_eq!(faults(source), [], "{source}");
    }
    let source = "type Box(V) of {item: V} endtype\ntype T of {b: Box(T)} endtype\n";
    assert_eq!(
        faults(source),
        [(
            DiagnosticCode::TypeInfiniteSize,
            at(source, "type T of {b: Box(T)} endtype", 0)
        )]
    );
}

/// A callable's own type is one level more than its deepest parameter, so a
/// parameter type of depth 64 makes the callable 65 deep; the same type is
/// accepted on a local.
#[test]
fn callable_depth_counts_its_parameters() {
    let deepest = format!("{}I32{}", "List(".repeat(63), ")".repeat(63));
    let source =
        format!("fn f takes {{x: {deepest}}} returns r: I32 do return 1i32 endfn\n");
    assert_eq!(
        faults(&source),
        [(DiagnosticCode::TypeDepthExceeded, at(&source, &deepest, 0))]
    );
    let source = format!(
        "fn f returns r: I32 do let x: {deepest} be [] endlet return 1i32 endfn\n"
    );
    assert_eq!(faults(&source), []);
}
