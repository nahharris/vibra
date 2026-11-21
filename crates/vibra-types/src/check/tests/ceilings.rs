//! Performed rows against ceilings, their witness chains, and unused roots.
//! Expectations come from the effects chapter ("Effect ceilings", "Cold
//! plans and performed rows", "Operation interfaces", "Tests and model
//! profiles"), the diagnostics chapter's rows of `@effect.outsideCeiling`
//! and `@contract.unusedEffect` and the paragraph on their related spans,
//! and question Q5 of the M2 plan.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level};
use vibra_ir::{
    Callable, CheckedModuleSet, EffectFault, EntityRef, IrError, ModuleSetParts, Row,
    Type,
};
use vibra_resolve::{PackageId, ResolveInput, SourceModule, SourceUnit, UnitKind};

use super::{SOURCE, at};
use crate::check::effects::analysis::observed;
use crate::check::{CheckResult, check_source, check_workspace};

/// The interfaces of step 08, a type that implements two of them, and a
/// `proc` that performs `Log`.
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

/// Each diagnostic of `result` with its code, level, primary span, and
/// related spans and notes.
type Seen = (DiagnosticCode, Level, ByteSpan, Vec<(ByteSpan, String)>);

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

/// An `@effect.outsideCeiling` at `span` with the chain `related`.
fn outside(span: ByteSpan, related: &[(ByteSpan, &str)]) -> Seen {
    (
        DiagnosticCode::EffectOutsideCeiling,
        Level::Error,
        span,
        related
            .iter()
            .map(|(span, note)| (*span, (*note).to_owned()))
            .collect(),
    )
}

/// An `@contract.unusedEffect` at `span`.
fn unused(span: ByteSpan) -> Seen {
    (
        DiagnosticCode::ContractUnusedEffect,
        Level::Warning,
        span,
        Vec::new(),
    )
}

/// The span of `inner` inside the last occurrence of `outer` in `source`.
fn within(source: &str, outer: &str, inner: &str) -> ByteSpan {
    let start = source
        .rfind(outer)
        .unwrap_or(usize::MAX)
        .saturating_add(outer.find(inner).unwrap_or(usize::MAX));
    ByteSpan::new(start, start.saturating_add(inner.len()))
}

/// The diagnostics of `source`, checked as `@app.main`.
fn check(source: &str) -> Vec<Seen> {
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

/// Checks `@app.main` with `app` and `@tests.main` with `test`.
fn in_tests(app: &str, test: &str) -> Result<CheckResult, String> {
    let stdlib = crate::stdlib::load_stdlib().map_err(|error| error.to_string())?;
    let input = ResolveInput::new(
        PackageId::new("hello", "0.1.0"),
        vec![
            SourceUnit::new(
                "@app",
                UnitKind::Lib,
                vec![SourceModule::new(
                    "@app.main",
                    "src/app/main.vib",
                    app.as_bytes().to_vec(),
                )],
            ),
            SourceUnit::new(
                "@tests",
                UnitKind::Tests,
                vec![SourceModule::new(
                    "@tests.main",
                    "tests/main.vib",
                    test.as_bytes().to_vec(),
                )],
            ),
        ],
        stdlib.overlay().embedded_unit(),
    );
    Ok(check_workspace(
        &input,
        &["@app".to_owned(), "@tests".to_owned()],
    ))
}

/// `L` with every member public, so that a test module reaches it.
fn public_l() -> String {
    L.replace("interface Log\n", "interface Log\n  visibility @public\n")
        .replace(
            "interface Clock\n",
            "interface Clock\n  visibility @public\n",
        )
        .replace(
            "type Memory of {lines: List(Str)}\n",
            "type Memory of {lines: List(Str)}\n  visibility @public\n",
        )
        .replace(
            "proc note takes {m: Memory} returns done: Void performs {Log} do",
            "proc note takes {m: Memory} returns done: Void performs {Log} \
         visibility @public do",
        )
}

#[test]
fn a_row_within_its_ceiling_is_accepted() -> Result<(), String> {
    accepted(L)?;
    accepted(&with_l(
        "proc outer takes {m: Memory} returns d: Void performs {Log} do \
         return await note(m) endproc",
    ))?;
    Ok(())
}

#[test]
fn recursion_closes() -> Result<(), String> {
    accepted(&with_l(
        "proc loop takes {m: Memory n: I32} returns d: Void performs {Log} do \
         if Ordered.greater(n 0i32) then return await loop(m I32.sub(n 1i32)) endif \
         return await Log.append(m \"x\") endproc",
    ))?;
    Ok(())
}

#[test]
fn a_direct_member_await_outside_the_ceiling() {
    let source = with_l(
        "proc bad takes {m: Memory} returns d: Void do \
         return await Log.append(m \"x\") endproc",
    );
    assert_eq!(
        check(&source),
        [outside(
            at(&source, "await Log.append(m \"x\")", 1),
            &[(
                at(&source, "Log.append(m \"x\")", 1),
                "performs Log via Log.append"
            )]
        )]
    );
}

#[test]
fn a_transitive_await_relates_the_whole_chain() {
    let source = with_l(
        "proc bad takes {m: Memory} returns d: Void do return await note(m) endproc",
    );
    assert_eq!(
        check(&source),
        [outside(
            at(&source, "await note(m)", 0),
            &[
                (at(&source, "note(m)", 0), "performs Log via note"),
                (
                    at(&source, "Log.append(m \"x\")", 0),
                    "performs Log via Log.append"
                ),
            ]
        )]
    );
}

#[test]
fn once_per_body_and_root_at_the_earliest_await() {
    let source = with_l(
        "proc bad takes {m: Memory} returns d: Void do \
         let _ be await Log.append(m \"a\") endlet \
         return await Log.append(m \"b\") endproc",
    );
    assert_eq!(
        check(&source),
        [outside(
            at(&source, "await Log.append(m \"a\")", 0),
            &[(
                at(&source, "Log.append(m \"a\")", 0),
                "performs Log via Log.append"
            )]
        )]
    );
    let source = with_l(
        "proc bad takes {m: Memory} returns d: Void do \
         let _ be await Log.append(m \"a\") endlet \
         let _ be await Clock.now(m) endlet return void endproc",
    );
    assert_eq!(
        check(&source),
        [
            outside(
                at(&source, "await Log.append(m \"a\")", 0),
                &[(
                    at(&source, "Log.append(m \"a\")", 0),
                    "performs Log via Log.append"
                )]
            ),
            outside(
                at(&source, "await Clock.now(m)", 0),
                &[(
                    at(&source, "Clock.now(m)", 0),
                    "performs Clock via Clock.now"
                )]
            ),
        ]
    );
}

#[test]
fn no_relation_between_roots() {
    let source = with_l(
        "proc bad takes {m: Memory} returns d: Void performs {Clock} do \
         let _ be await Clock.now(m) endlet return await Log.append(m \"x\") endproc",
    );
    assert_eq!(
        check(&source),
        [outside(
            at(&source, "await Log.append(m \"x\")", 1),
            &[(
                at(&source, "Log.append(m \"x\")", 1),
                "performs Log via Log.append"
            )]
        )]
    );
}

#[test]
fn a_stored_plan_contributes_where_it_is_awaited() {
    let source = with_l(
        "proc bad takes {m: Memory} returns d: Void do \
         let plan be Log.append(m \"x\") endlet return await plan endproc",
    );
    assert_eq!(
        check(&source),
        [outside(
            at(&source, "await plan", 0),
            &[(
                within(&source, "await plan", "plan"),
                "performs Log via plan (indirect)"
            )]
        )]
    );
}

#[test]
fn a_member_through_an_interface_value_is_indirect() {
    let source = with_l(
        "proc bad takes {l: Log} returns d: Void do return await Log.append(l \"x\") endproc",
    );
    assert_eq!(
        check(&source),
        [outside(
            at(&source, "await Log.append(l \"x\")", 0),
            &[(
                at(&source, "Log.append(l \"x\")", 0),
                "performs Log via Log.append (indirect)"
            )]
        )]
    );
}

#[test]
fn an_await_inside_if() {
    let source = with_l(
        "proc bad takes {m: Memory b: Bool} returns d: Void do \
         if b then return await Log.append(m \"x\") endif return void endproc",
    );
    assert_eq!(
        check(&source),
        [outside(
            at(&source, "await Log.append(m \"x\")", 1),
            &[(
                at(&source, "Log.append(m \"x\")", 1),
                "performs Log via Log.append"
            )]
        )]
    );
}

#[test]
fn an_await_inside_match() {
    let source = with_l(
        "proc bad takes {m: Memory n: I32} returns d: Void do \
         let _ be match n case 0i32 await Log.append(m \"x\") case _ void endmatch endlet \
         return void endproc",
    );
    assert_eq!(
        check(&source),
        [outside(
            at(&source, "await Log.append(m \"x\")", 1),
            &[(
                at(&source, "Log.append(m \"x\")", 1),
                "performs Log via Log.append"
            )]
        )]
    );
}

#[test]
fn a_proc_member_of_a_type() {
    let source = L.replace(
        "type Memory of {lines: List(Str)}\n",
        "type Memory of {lines: List(Str)}\n  proc flush takes {self: Self} \
         returns d: Void do return await note(self) endproc\n",
    );
    assert_eq!(
        check(&source),
        [outside(
            at(&source, "await note(self)", 0),
            &[
                (at(&source, "note(self)", 0), "performs Log via note"),
                (
                    at(&source, "Log.append(m \"x\")", 0),
                    "performs Log via Log.append"
                ),
            ]
        )]
    );
}

#[test]
fn a_default_member_fits_its_complete_row() -> Result<(), String> {
    let stamp = "  proc stamp takes {log: Self clock: Clock} returns d: Void do \
                 let _ be await Clock.now(clock) endlet return void endproc\n";
    let source = L.replacen(
        "  endproc\nendinterface\n",
        &format!("  endproc\n{stamp}endinterface\n"),
        1,
    );
    assert_eq!(
        check(&source),
        [outside(
            at(&source, "await Clock.now(clock)", 0),
            &[(
                at(&source, "Clock.now(clock)", 0),
                "performs Clock via Clock.now (indirect)"
            )]
        )]
    );
    // The owner is in the complete row: awaiting the owner's member fits,
    // and an owner the body does not perform is no warning (Q5).
    let owner = "  proc twice takes {log: Self entry: Str} returns d: Void do \
                 let _ be await Log.append(log entry) endlet return void endproc\n\
                   proc idle takes {log: Self} returns d: Void do return void endproc\n";
    accepted(&L.replacen(
        "  endproc\nendinterface\n",
        &format!("  endproc\n{owner}endinterface\n"),
        1,
    ))?;
    Ok(())
}

#[test]
fn an_impl_member_that_forwards_wider_is_rejected() {
    let source = L.replace(
        "returns done: Void do\n      return void\n",
        "returns done: Void do\n      let _ be await Clock.now(log) endlet return void\n",
    );
    assert_eq!(
        check(&source),
        [outside(
            at(&source, "await Clock.now(log)", 0),
            &[(
                at(&source, "Clock.now(log)", 0),
                "performs Clock via Clock.now"
            )]
        )]
    );
}

#[test]
fn a_test_performs_within_its_row() -> Result<(), String> {
    let app = public_l();
    let test = "use @std.assert enduse\nuse @app.main enduse\n\
                test \"t\" do let _ be await main.note(main.Memory(lines: [])) endlet endtest\n";
    let result = in_tests(&app, test)?;
    let await_span = at(test, "await main.note(main.Memory(lines: []))", 0);
    assert_eq!(
        seen(result.diagnostics()),
        [outside(
            await_span,
            &[
                (
                    at(test, "main.note(main.Memory(lines: []))", 0),
                    "performs Log via note"
                ),
                (
                    at(&app, "Log.append(m \"x\")", 0),
                    "performs Log via Log.append"
                ),
            ]
        )]
    );
    let sources = result
        .diagnostics()
        .iter()
        .flat_map(|diagnostic| {
            std::iter::once(diagnostic.source_id().map(str::to_owned)).chain(
                diagnostic
                    .related()
                    .iter()
                    .map(|related| related.source_id.clone()),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        sources,
        [
            Some("tests/main.vib".to_owned()),
            Some("tests/main.vib".to_owned()),
            Some("src/app/main.vib".to_owned()),
        ]
    );
    let fits = "use @std.assert enduse\nuse @app.main enduse\n\
                test \"t\" performs {main.Log} do \
                let _ be await main.note(main.Memory(lines: [])) endlet endtest\n";
    let result = in_tests(&app, fits)?;
    assert_eq!(seen(result.diagnostics()), []);
    Ok(())
}

#[test]
fn mutual_recursion_reports_in_the_narrower_body() {
    let source = with_l(
        "proc a takes {m: Memory} returns d: Void performs {Log} do \
         let _ be await b(m) endlet return await Log.append(m \"x\") endproc\n\
         proc b takes {m: Memory} returns d: Void do return await a(m) endproc",
    );
    assert_eq!(
        check(&source),
        [outside(
            at(&source, "await a(m)", 0),
            &[
                (at(&source, "a(m)", 0), "performs Log via a"),
                (
                    at(&source, "Log.append(m \"x\")", 1),
                    "performs Log via Log.append"
                ),
            ]
        )]
    );
}

#[test]
fn an_unused_root_is_a_warning_at_its_entry() -> Result<(), String> {
    let source = with_l(
        "proc idle takes {m: Memory} returns d: Void performs {Log} do return void endproc",
    );
    assert_eq!(
        check(&source),
        [unused(within(&source, "performs {Log} do", "Log"))]
    );
    let source = with_l(
        "proc half takes {m: Memory} returns d: Void performs {Log Clock} do \
         return await Log.append(m \"x\") endproc",
    );
    assert_eq!(
        check(&source),
        [unused(within(&source, "performs {Log Clock}", "Clock"))]
    );
    // Building a plan performs nothing (Q5).
    let source = with_l(
        "proc build takes {m: Memory} returns d: Void performs {Log} do \
         let _ be Log.append(m \"x\") endlet return void endproc",
    );
    assert_eq!(
        check(&source),
        [unused(within(&source, "performs {Log} do", "Log"))]
    );
    // The additive row of a default member.
    let source = L.replacen(
        "  endproc\nendinterface\n",
        "  endproc\n  proc both takes {log: Self} returns d: Void performs {Clock} do \
         return void endproc\nendinterface\n",
        1,
    );
    assert_eq!(
        check(&source),
        [unused(within(&source, "performs {Clock}", "Clock"))]
    );
    // A warning leaves the program accepted and lowered.
    let result = check_source(
        SOURCE,
        &with_l(
            "proc idle takes {m: Memory} returns d: Void performs {Log} do \
             return void endproc",
        ),
    );
    assert!(!result.has_errors());
    assert!(result.module_set().is_some());
    Ok(())
}

#[test]
fn an_unused_root_of_a_test_leaves_it_runnable() -> Result<(), String> {
    let app = public_l();
    let test = "use @std.assert enduse\nuse @app.main enduse\n\
                test \"t\" performs {main.Log} do assert.true(true) endtest\n";
    let result = in_tests(&app, test)?;
    assert_eq!(
        seen(result.diagnostics()),
        [unused(at(test, "main.Log", 0))]
    );
    assert!(result.tests().iter().all(|test| test.function().is_some()));
    Ok(())
}

#[test]
fn no_warning_for_an_impl_member_or_an_owner() -> Result<(), String> {
    // `L`'s `impl Log` member awaits nothing, and a default member of `Log`
    // that returns `void` performs nothing of its owner (Q5).
    accepted(L)?;
    accepted(&L.replacen(
        "  endproc\nendinterface\n",
        "  endproc\n  proc idle takes {log: Self} returns d: Void do return void endproc\n\
         endinterface\n",
        1,
    ))?;
    Ok(())
}

#[test]
fn a_body_with_an_error_reports_no_unused_root() {
    let source = with_l(
        "proc idle takes {m: Memory} returns d: Void performs {Log} do \
         let x: I32 be \"a\" endlet return void endproc",
    );
    assert_eq!(
        check(&source)
            .into_iter()
            .map(|(code, ..)| code)
            .collect::<Vec<_>>(),
        [DiagnosticCode::TypeMismatch]
    );
}

/// The row prefix of the review of step 11: two operation interfaces, a
/// `proc` that performs `Log`, and one with the empty row.
const ROW_PREFIX: &str = "interface Log proc emit takes {m: Self} returns d: Void endproc \
                          endinterface\n\
                          interface Tick proc tick takes {m: Self} returns d: Void endproc \
                          endinterface\n\
                          proc loud takes {m: Log} returns d: Void performs {Log} do \
                          return await Log.emit(m) endproc\n\
                          proc quiet takes {m: Log} returns d: Void do return void endproc\n";

/// A witness chain that meets a cycle keeps its prefix up to the first
/// repeated hop: an `@effect.outsideCeiling` always has "the chain of calls
/// as related spans" (probe `cycleNotes`).
#[test]
fn a_witness_cycle_keeps_the_chain_up_to_the_repeated_hop() {
    let source = "interface Log proc emit takes {m: Self} returns d: Void endproc \
                  endinterface\n\
                  proc loop returns d: Void performs {Log} do return await loop() endproc\n\
                  proc run returns d: Void do return await loop() endproc\n";
    assert_eq!(
        check(source),
        [outside(
            at(source, "await loop()", 1),
            &[
                (at(source, "loop()", 1), "performs Log via loop"),
                (at(source, "loop()", 0), "performs Log via loop"),
            ]
        )]
    );
}

/// An awaited operand with an error of its own still starts the chain: the
/// call it runs is not known, so it is one indirect hop and the chain ends
/// there.
#[test]
fn an_awaited_operand_with_an_error_starts_the_chain() {
    let source = with_l(
        "proc bad takes {m: Memory} returns d: Void do \
         return await Log.append(m 1i32) endproc",
    );
    assert_eq!(
        check(&source),
        [
            outside(
                at(&source, "await Log.append(m 1i32)", 0),
                &[(
                    at(&source, "Log.append(m 1i32)", 0),
                    "performs Log via Log.append(m 1i32) (indirect)"
                )]
            ),
            (
                DiagnosticCode::TypeArgumentMismatch,
                Level::Error,
                at(&source, "1i32", 0),
                Vec::new()
            ),
        ]
    );
}

/// A member applied at a closed receiver is statically resolved, so its hop
/// is not marked indirect (probe `closedNotes`, ruling F2 of the review of
/// step 11).
#[test]
fn a_member_at_a_closed_receiver_is_not_indirect() {
    let source = format!(
        "{ROW_PREFIX}type Model of I32 impl Log proc emit takes {{m: Self}} returns d: Void \
         do return void endproc endimpl endtype\n\
         proc run takes {{m: Model}} returns d: Void do return await Log.emit(m) endproc\n"
    );
    assert_eq!(
        check(&source),
        [outside(
            at(&source, "await Log.emit(m)", 1),
            &[(at(&source, "Log.emit(m)", 1), "performs Log via Log.emit")]
        )]
    );
}

/// A declaration that holds an anonymous `proc` with an error has an error,
/// so its unused roots are not reported (probe `anonWarning`): with a type
/// error, and with an anonymous `proc` whose only error is
/// `@effect.outsideCeiling`.
#[test]
fn an_anonymous_proc_with_an_error_hides_the_unused_roots_of_its_declaration() {
    let codes = |source: &str| {
        check(source)
            .into_iter()
            .map(|(code, ..)| code)
            .collect::<Vec<_>>()
    };
    let source = format!(
        "{ROW_PREFIX}proc outer returns d: Void performs {{Log}} do let f be proc returns \
         d: Void do let x: I32 be \"x\" endlet return void endproc endlet return void endproc\n"
    );
    assert_eq!(codes(&source), [DiagnosticCode::TypeMismatch]);
    let source = format!(
        "{ROW_PREFIX}proc outer returns d: Void performs {{Log}} do let f be proc takes \
         {{m: Log}} returns d: Void do return await Log.emit(m) endproc endlet return void \
         endproc\n"
    );
    assert_eq!(codes(&source), [DiagnosticCode::EffectOutsideCeiling]);
}

/// The roots of `rows`.
fn union(rows: &[Row]) -> BTreeSet<EntityRef> {
    rows.iter()
        .flat_map(|row| row.roots().iter().cloned())
        .collect()
}

/// The rows the checked IR recomputes: for each declaration, by the name it
/// records, the roots its own body awaits and those of each `Proc` closure
/// it creates, directly or through other closures; and the row of each
/// `Proc` closure's type.
type IrRows = BTreeMap<
    String,
    (
        BTreeSet<EntityRef>,
        Vec<(BTreeSet<EntityRef>, BTreeSet<EntityRef>)>,
    ),
>;

fn ir_rows(set: &CheckedModuleSet, rows: &mut IrRows) -> Result<(), String> {
    let flow = set.flow();
    let named = set
        .globals()
        .iter()
        .map(|global| global.name.clone())
        .zip(&flow.globals)
        .chain(
            set.functions()
                .iter()
                .map(|function| function.name.clone())
                .zip(&flow.functions),
        );
    for (name, facts) in named {
        let mut closures = Vec::new();
        let mut pending = vec![facts];
        while let Some(facts) = pending.pop() {
            for (callable, _) in &facts.values {
                let Callable::Closure(index) = callable else {
                    continue;
                };
                let (ty, inner) = flow
                    .closures
                    .get(*index)
                    .ok_or_else(|| format!("no closure {index}"))?;
                if let Type::Proc(signature) = ty {
                    closures.push((
                        union(&inner.awaited),
                        signature.effects.roots().iter().cloned().collect(),
                    ));
                } else if !inner.awaited.is_empty() {
                    return Err(format!("a `Fn` closure of {name} awaits"));
                }
                pending.push(inner);
            }
        }
        closures.sort();
        let found = (union(&facts.awaited), closures);
        if let Some(before) = rows.get(&name)
            && *before != found
        {
            return Err(format!("{name} has two rows in the module sets"));
        }
        rows.insert(name, found);
    }
    Ok(())
}

/// One module of a corpus case: its unit, its module path, its source
/// identity, and its text.
type CaseModule = (&'static str, &'static str, &'static str, &'static str);

/// One module of the corpus case `$id`, of prefix `EFFECT`: `app` for
/// `src/app/$stem.vib`, `tests` for `tests/$stem.vib`.
macro_rules! case_module {
    (app $id:literal $stem:literal) => {
        (
            "@app",
            concat!("@app.", $stem),
            concat!("src/app/", $stem, ".vib"),
            include_str!(concat!(
                "../../../../../conformance/cases/EFFECT/",
                $id,
                "/tree/src/app/",
                $stem,
                ".vib"
            )),
        )
    };
    (tests $id:literal $stem:literal) => {
        (
            "@tests",
            concat!("@tests.", $stem),
            concat!("tests/", $stem, ".vib"),
            include_str!(concat!(
                "../../../../../conformance/cases/EFFECT/",
                $id,
                "/tree/tests/",
                $stem,
                ".vib"
            )),
        )
    };
}

/// The corpus case `$id` and its modules.
macro_rules! case {
    ($id:literal: $($unit:ident $stem:literal),+) => {
        ($id, &[$(case_module!($unit $id $stem)),+])
    };
}

/// Every corpus case of prefix `EFFECT` whose expected verdict is
/// `accepted`, with its modules. A semantic crate reads no file, so the
/// sources are embedded; `vibra-conformance`'s test `effect_rows` keeps this
/// list equal to the accepted cases of the corpus.
const ACCEPTED_EFFECT_CASES: &[(&str, &[CaseModule])] = &[
    case!("V2-EFFECT-await-in-if": app "main"),
    case!("V2-EFFECT-await-in-match-and-do": app "main"),
    case!("V2-EFFECT-await-in-test": app "main", tests "run"),
    case!("V2-EFFECT-callback-row-exact": app "main"),
    case!("V2-EFFECT-callback-row-wrapped": app "main"),
    case!("V2-EFFECT-ceiling-exact": app "main"),
    case!("V2-EFFECT-ceiling-transitive": app "main"),
    case!("V2-EFFECT-default-proc-member": app "main"),
    case!("V2-EFFECT-fn-member-pure": app "main"),
    case!("V2-EFFECT-generic-operation-interface": app "main"),
    case!("V2-EFFECT-impl-host-member-supplied": app "main"),
    case!("V2-EFFECT-impl-in-interface": app "main", app "note"),
    case!("V2-EFFECT-impl-keeps-row": app "main"),
    case!("V2-EFFECT-intent-type-result": app "main"),
    case!("V2-EFFECT-member-additive-row": app "main"),
    case!("V2-EFFECT-member-plan-closed-receiver": app "main"),
    case!("V2-EFFECT-member-plan-interface-value": app "main"),
    case!("V2-EFFECT-member-plan-type-parameter": app "main"),
    case!("V2-EFFECT-module-value-plan": app "main"),
    case!("V2-EFFECT-named-proc-value": app "main"),
    case!("V2-EFFECT-operation-interface": app "main"),
    case!("V2-EFFECT-plan-discarded": app "main"),
    case!("V2-EFFECT-proc-await-direct": app "main"),
    case!("V2-EFFECT-proc-await-twice": app "main"),
    case!("V2-EFFECT-proc-generic": app "main"),
    case!("V2-EFFECT-proc-labelled": app "main"),
    case!("V2-EFFECT-proc-member-of-type": app "main"),
    case!("V2-EFFECT-proc-plan-bound": app "main"),
    case!("V2-EFFECT-proc-recursive": app "main"),
    case!("V2-EFFECT-proc-type-empty-row": app "main"),
    case!("V2-EFFECT-proc-type-field": app "main"),
    case!("V2-EFFECT-proc-type-parameter": app "main"),
    case!("V2-EFFECT-proc-type-result": app "main"),
    case!("V2-EFFECT-recursion-closes": app "main"),
    case!("V2-EFFECT-row-on-proc": app "main"),
    case!("V2-EFFECT-row-on-test": app "main", tests "run"),
    case!("V2-EFFECT-row-two-roots": app "main"),
    case!("V2-EFFECT-std-copy-note": app "main"),
    case!("V2-EFFECT-std-each-interface": app "main"),
    case!("V2-EFFECT-std-prepare-write": app "main"),
    case!("V2-EFFECT-std-read-all": app "main"),
    case!("V2-EFFECT-test-row-empty-written": app "main", tests "run"),
];

/// For every accepted corpus case of prefix `EFFECT`, the performed row the
/// checker computes for each body (`Performed::of`) equals the row the
/// checked IR recomputes from the `Intent` types of the same body's awaits
/// (`Facts::awaited`), in both directions (architecture, risk 1; F7 of the
/// review of step 11). An anonymous `proc` is a `Proc` closure of its
/// declaration; a `Proc` closure with no anonymous `proc`, which a contract
/// member named as a value becomes, awaits exactly its type's row.
#[test]
fn checker_rows_equal_the_checked_ir_rows_on_every_accepted_effect_case()
-> Result<(), String> {
    let stdlib = crate::stdlib::load_stdlib().map_err(|error| error.to_string())?;
    for (name, modules) in ACCEPTED_EFFECT_CASES {
        let mut units: Vec<(&str, UnitKind, Vec<SourceModule>)> = Vec::new();
        for (unit, path, source_id, text) in *modules {
            let module = SourceModule::new(*path, *source_id, text.as_bytes().to_vec());
            match units.iter_mut().find(|(known, ..)| known == unit) {
                Some((.., found)) => found.push(module),
                None => {
                    let kind = if *unit == "@tests" {
                        UnitKind::Tests
                    } else {
                        UnitKind::Lib
                    };
                    units.push((unit, kind, vec![module]));
                }
            }
        }
        let names = units
            .iter()
            .map(|(unit, ..)| (*unit).to_owned())
            .collect::<Vec<_>>();
        let input = ResolveInput::new(
            PackageId::new("hello", "0.1.0"),
            units
                .into_iter()
                .map(|(unit, kind, modules)| SourceUnit::new(unit, kind, modules))
                .collect(),
            stdlib.overlay().embedded_unit(),
        );
        observed::take();
        let result = check_workspace(&input, &names);
        let checker = observed::take();
        if result.has_errors() {
            return Err(format!("{name}: {:#?}", result.diagnostics()));
        }
        let mut ir = IrRows::new();
        for set in [result.module_set(), result.test_set()]
            .into_iter()
            .flatten()
        {
            ir_rows(set, &mut ir).map_err(|error| format!("{name}: {error}"))?;
        }
        let mut own = BTreeMap::new();
        let mut anonymous = BTreeMap::<String, Vec<BTreeSet<EntityRef>>>::new();
        for row in checker {
            if row.anonymous {
                anonymous
                    .entry(row.declaration)
                    .or_default()
                    .push(row.roots);
            } else if own.insert(row.declaration.clone(), row.roots).is_some() {
                return Err(format!("{name}: {} has two rows", row.declaration));
            }
        }
        // From the checker to the IR: every body the checker analysed.
        for (declaration, roots) in &own {
            let (awaited, _) = ir
                .get(declaration)
                .ok_or_else(|| format!("{name}: the IR has no {declaration}"))?;
            assert_eq!(awaited, roots, "{name}: {declaration}");
        }
        // From the IR to the checker: every body of the IR, and every
        // `Proc` closure.
        for (declaration, (awaited, closures)) in &ir {
            match own.get(declaration) {
                Some(roots) => assert_eq!(roots, awaited, "{name}: {declaration}"),
                None => assert!(
                    awaited.is_empty(),
                    "{name}: {declaration} awaits and the checker has no row"
                ),
            }
            let mut left = closures.clone();
            for roots in anonymous.get(declaration).into_iter().flatten() {
                let at = left
                    .iter()
                    .position(|(awaited, _)| awaited == roots)
                    .ok_or_else(|| {
                        format!("{name}: {declaration}: no closure performs {roots:?}")
                    })?;
                left.remove(at);
            }
            for (awaited, row) in left {
                assert_eq!(awaited, row, "{name}: {declaration}: a member value");
            }
        }
        for declaration in anonymous.keys() {
            assert!(
                ir.contains_key(declaration),
                "{name}: the IR has no {declaration}"
            );
        }
    }
    Ok(())
}

/// A chain of `count` procedures, each awaiting the next, the last awaiting
/// `Log.append`, and a `proc` with the empty row awaiting the first.
fn long_chain(count: usize) -> String {
    let mut source = with_l("");
    for index in 0..count {
        let next = if index + 1 == count {
            "Log.append(m \"x\")".to_owned()
        } else {
            format!("p{}(m)", index + 1)
        };
        source.push_str(&format!(
            "proc p{index} takes {{m: Memory}} returns d: Void performs {{Log}} do \
             return await {next} endproc\n"
        ));
    }
    source.push_str(
        "proc bad takes {m: Memory} returns d: Void do return await p0(m) endproc\n",
    );
    source
}

/// The witness of a 10,000-procedure chain is found with an explicit work
/// list: on a 256 KiB thread, the search does not overflow.
#[test]
fn a_ten_thousand_procedure_chain_on_a_small_stack() -> Result<(), String> {
    const COUNT: usize = 10_000;
    let source = long_chain(COUNT);
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let result = check_source(SOURCE, &source);
            result
                .diagnostics()
                .iter()
                .map(|diagnostic| (diagnostic.code(), diagnostic.related().len()))
                .collect::<Vec<_>>()
        })
        .map_err(|error| error.to_string())?;
    let found = handle
        .join()
        .map_err(|_| "the check overflowed".to_owned())?;
    assert_eq!(found, [(DiagnosticCode::EffectOutsideCeiling, COUNT + 1)]);
    Ok(())
}

/// The parts of `set`, to build it again.
fn parts(set: &CheckedModuleSet) -> ModuleSetParts {
    ModuleSetParts {
        library: set.library().clone(),
        types: set.types().types().values().cloned().collect(),
        interfaces: set.interfaces().values().cloned().collect(),
        globals: set.globals().to_vec(),
        functions: set.functions().to_vec(),
        implementations: set.implementations().to_vec(),
    }
}

/// The module set of `accepted`, built again by hand with the body of the
/// function named `twin` in the place of the body of the function named
/// `narrow`: the root path the IR rejects, and the function it names.
fn spliced(
    set: &CheckedModuleSet,
    narrow: &str,
    twin: &str,
) -> Result<(String, String), String> {
    let mut parts = parts(set);
    CheckedModuleSet::try_new(parts.clone()).map_err(|error| error.to_string())?;
    let find = |name: &str| {
        parts
            .functions
            .iter()
            .position(|function| function.name.ends_with(name))
            .ok_or_else(|| format!("no function {name}"))
    };
    let (narrow, twin) = (find(narrow)?, find(twin)?);
    let donor = parts
        .functions
        .get(twin)
        .map(|function| (function.body.clone(), function.slot_count))
        .ok_or("no twin")?;
    let target = parts
        .functions
        .get_mut(narrow)
        .ok_or("no narrow function")?;
    target.body = donor.0;
    target.slot_count = donor.1;
    match CheckedModuleSet::try_new(parts) {
        Err(IrError::Effect { owner, fault }) => match *fault {
            EffectFault::OutsideCeiling { root, .. } => {
                Ok((root.path().to_owned(), owner.unwrap_or_default()))
            }
            other => Err(format!("{other}")),
        },
        Err(other) => Err(format!("{other}")),
        Ok(_) => Err("the spliced module set was accepted".to_owned()),
    }
}

/// For each negative case of the corpus, the same lowered program built by
/// hand is rejected by the checked IR (`vibra-ir::effects`). The program is
/// the case's own, with the faulty body moved into a twin whose ceiling
/// holds what it performs, so the checker lowers it; the twin's lowered
/// body is then put back in place of the trivial body that stood for it.
#[test]
fn the_checked_ir_rejects_each_negative_case_built_by_hand() -> Result<(), String> {
    let trivial = "do return void endproc";
    let cases: [(&str, String, &str); 11] = [
        (
            "V2-EFFECT-outside-ceiling-direct",
            with_l(&format!(
                "proc bad takes {{m: Memory}} returns d: Void {trivial}\n\
                 proc twin takes {{m: Memory}} returns d: Void performs {{Log}} do \
                 return await Log.append(m \"x\") endproc"
            )),
            "app.main.Log",
        ),
        (
            "V2-EFFECT-outside-ceiling-transitive",
            with_l(&format!(
                "proc bad takes {{m: Memory}} returns d: Void {trivial}\n\
                 proc twin takes {{m: Memory}} returns d: Void performs {{Log}} do \
                 return await note(m) endproc"
            )),
            "app.main.Log",
        ),
        (
            "V2-EFFECT-outside-ceiling-earliest",
            with_l(&format!(
                "proc bad takes {{m: Memory}} returns d: Void {trivial}\n\
                 proc twin takes {{m: Memory}} returns d: Void performs {{Log}} do \
                 let _ be await Log.append(m \"a\") endlet \
                 return await Log.append(m \"b\") endproc"
            )),
            "app.main.Log",
        ),
        (
            "V2-EFFECT-outside-ceiling-two-roots",
            with_l(&format!(
                "proc bad takes {{m: Memory}} returns d: Void {trivial}\n\
                 proc twin takes {{m: Memory}} returns d: Void performs {{Log Clock}} do \
                 let _ be await Log.append(m \"a\") endlet \
                 let _ be await Clock.now(m) endlet return void endproc"
            )),
            "app.main.Log",
        ),
        (
            "V2-EFFECT-outside-ceiling-wrong-root",
            with_l(
                "proc bad takes {m: Memory} returns d: Void performs {Clock} do \
                 let _ be await Clock.now(m) endlet return void endproc\n\
                 proc twin takes {m: Memory} returns d: Void performs {Clock Log} do \
                 let _ be await Clock.now(m) endlet return await Log.append(m \"x\") endproc",
            ),
            "app.main.Log",
        ),
        (
            "V2-EFFECT-outside-ceiling-stored-plan",
            with_l(&format!(
                "proc bad takes {{m: Memory}} returns d: Void {trivial}\n\
                 proc twin takes {{m: Memory}} returns d: Void performs {{Log}} do \
                 let plan be Log.append(m \"x\") endlet return await plan endproc"
            )),
            "app.main.Log",
        ),
        (
            "V2-EFFECT-outside-ceiling-interface-value",
            with_l(&format!(
                "proc bad takes {{l: Log}} returns d: Void {trivial}\n\
                 proc twin takes {{l: Log}} returns d: Void performs {{Log}} do \
                 return await Log.append(l \"x\") endproc"
            )),
            "app.main.Log",
        ),
        (
            "V2-EFFECT-outside-ceiling-in-if",
            with_l(&format!(
                "proc bad takes {{m: Memory b: Bool}} returns d: Void {trivial}\n\
                 proc twin takes {{m: Memory b: Bool}} returns d: Void performs {{Log}} do \
                 if b then return await Log.append(m \"x\") endif return void endproc"
            )),
            "app.main.Log",
        ),
        (
            "V2-EFFECT-outside-ceiling-type-member",
            L.replace(
                "type Memory of {lines: List(Str)}\n",
                &format!(
                    "type Memory of {{lines: List(Str)}}\n  proc flush takes {{self: Self}} \
                     returns d: Void {trivial}\n  proc twin takes {{self: Self}} \
                     returns d: Void performs {{Log}} do return await note(self) endproc\n"
                ),
            ),
            "app.main.Log",
        ),
        (
            "V2-EFFECT-outside-ceiling-default-member",
            L.replacen(
                "  endproc\nendinterface\n",
                &format!(
                    "  endproc\n  proc stamp takes {{log: Self clock: Clock}} returns d: Void \
                     {trivial}\n  proc twin takes {{log: Self clock: Clock}} returns d: Void \
                     performs {{Clock}} do let _ be await Clock.now(clock) endlet \
                     return void endproc\nendinterface\n"
                ),
                1,
            ),
            "app.main.Clock",
        ),
        (
            "V2-EFFECT-mutual-recursion-outside",
            with_l(&format!(
                "proc a takes {{m: Memory}} returns d: Void performs {{Log}} do \
                 let _ be await b(m) endlet return await Log.append(m \"x\") endproc\n\
                 proc b takes {{m: Memory}} returns d: Void {trivial}\n\
                 proc twin takes {{m: Memory}} returns d: Void performs {{Log}} do \
                 return await a(m) endproc"
            )),
            "app.main.Log",
        ),
    ];
    for (case, source, root) in cases {
        let set = accepted(&source).map_err(|error| format!("{case}: {error}"))?;
        let narrow = match case {
            "V2-EFFECT-outside-ceiling-type-member" => ".flush",
            "V2-EFFECT-outside-ceiling-default-member" => ".stamp",
            "V2-EFFECT-mutual-recursion-outside" => ".b",
            _ => ".bad",
        };
        let (rejected, owner) = spliced(&set, narrow, ".twin")
            .map_err(|error| format!("{case}: {error}"))?;
        assert_eq!(rejected, root, "{case}");
        assert!(owner.ends_with(narrow), "{case}: {owner}");
    }
    Ok(())
}

/// The two negative cases whose faulty body is an `impl` member or a test,
/// built by hand the same way.
#[test]
fn the_checked_ir_rejects_an_impl_member_and_a_test_built_by_hand() -> Result<(), String>
{
    // An `impl Log` member: its twin is a `proc` with the same parameters,
    // `Self` being `Memory`, and the row that holds `Clock`.
    let source = with_l(
        "proc twin takes {log: Memory entry: Str} returns done: Void performs {Clock} do \
         let _ be await Clock.now(log) endlet return void endproc",
    );
    let set = accepted(&source)?;
    let impl_member = set
        .functions()
        .iter()
        .position(|function| {
            function.name.contains("Memory") && function.name.ends_with(".append")
        })
        .and_then(|index| set.functions().get(index))
        .map(|function| function.name.clone())
        .ok_or("no impl member")?;
    let (root, owner) = spliced(&set, &impl_member, ".twin")?;
    assert_eq!(root, "app.main.Clock");
    assert_eq!(owner, impl_member);
    // A test with the empty row: its twin is a test whose row holds `Log`.
    let app = public_l();
    let test = "use @std.assert enduse\nuse @app.main enduse\n\
                test \"t\" do let _ be 0i32 endlet endtest\n\
                test \"twin\" performs {main.Log} do \
                let _ be await main.note(main.Memory(lines: [])) endlet endtest\n";
    let result = in_tests(&app, test)?;
    assert_eq!(seen(result.diagnostics()), []);
    let set = result.test_set().ok_or("no test set")?;
    let (root, owner) = spliced(set, "\"t\"", "\"twin\"")?;
    assert_eq!(root, "app.main.Log");
    assert!(owner.ends_with("\"t\""), "{owner}");
    Ok(())
}
