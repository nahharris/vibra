//! Procedures as values: the written `Proc` and `Intent` types, exact row
//! comparison, procedures named as values, and the anonymous `proc`.
//! Expectations come from the type chapter ("Functions as values", "Guards
//! and callable types", "Bare guards"), the source chapter ("Grammar",
//! "`return`, `try`, `await`"), the effects chapter ("Effect ceilings",
//! "Cold plans and performed rows"), the diagnostics chapter's rows of the
//! codes below, and question Q6 of the M2 plan.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level};
use vibra_ir::{Expr, FunctionBody, Node, Type};

use super::{SOURCE, at};
use crate::check::check_source;

/// The interfaces of step 08, a type that implements two of them, and a
/// `proc` that performs `Log`: the preamble `L` of step 09.
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

/// `emit`, which awaits its callback, and `quiet`, a `proc` with the empty
/// row.
const EMIT: &str = "proc emit takes {line: Str sink: Proc({Str} Void {Log})} returns d: Void \
                    performs {Log} do return await sink(line) endproc\n\
                    proc quiet takes {line: Str} returns d: Void do return void endproc\n";

/// `L` and then `rest`.
fn with_l(rest: &str) -> String {
    format!("{L}{rest}\n")
}

/// `L`, `EMIT`, and then `rest`.
fn with_emit(rest: &str) -> String {
    format!("{L}{EMIT}{rest}\n")
}

/// Each diagnostic with its code, level, primary span, and related spans.
type Seen = (DiagnosticCode, Level, ByteSpan, Vec<ByteSpan>);

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
                    .map(|related| related.span)
                    .collect(),
            )
        })
        .collect()
}

/// The diagnostics of `source`.
fn of(source: &str) -> Vec<Seen> {
    seen(check_source(SOURCE, source).diagnostics())
}

/// An error `code` at `span` with no related span.
fn error(code: DiagnosticCode, span: ByteSpan) -> Seen {
    (code, Level::Error, span, Vec::new())
}

/// A warning `code` at `span` with no related span.
fn warning(code: DiagnosticCode, span: ByteSpan) -> Seen {
    (code, Level::Warning, span, Vec::new())
}

/// Asserts that `source` checks with no diagnostic and lowers.
fn accepted(source: &str) {
    let result = check_source(SOURCE, source);
    assert_eq!(seen(result.diagnostics()), [], "{source}");
    assert!(result.module_set().is_some(), "{source}");
}

// A `Proc` or `Intent` type is written: parameter, result, `let`
// annotation, record field, type argument, and the representation of a
// declared type, which stands where another language has an alias (the type
// chapter has no transparent alias).

#[test]
fn a_proc_type_is_written_at_every_position() {
    let sink = "Proc({Str} Void {Log})";
    accepted(&with_l(&format!(
        "proc emit takes {{line: Str sink: {sink}}} returns d: Void performs {{Log}} do \
         return await sink(line) endproc"
    )));
    accepted(&with_l(&format!(
        "fn pick takes {{m: Memory}} returns sink: {sink} do return proc takes \
         {{line: Str}} returns d: Void performs {{Log}} do return await Log.append(m line) \
         endproc endfn"
    )));
    accepted(&with_l(&format!(
        "proc appendTo takes {{line: Str}} returns d: Void performs {{Log}} do \
         return await Log.append(Memory(lines: []) line) endproc\n\
         fn keep returns r: I32 do let s: {sink} be appendTo endlet return 1i32 endfn"
    )));
    accepted(&with_l(&format!(
        "type Hooks of {{onLine: {sink}}} endtype\n\
         proc fire takes {{hooks: Hooks}} returns d: Void performs {{Log}} do \
         return await hooks.onLine(\"x\") endproc"
    )));
    accepted(&with_l(&format!(
        "proc each takes {{sinks: List({sink})}} returns d: Void do return void endproc"
    )));
    accepted(&with_l(&format!(
        "type Sink of {sink} endtype\n\
         proc fire takes {{sink: Sink}} returns d: Void performs {{Log}} do \
         let Sink(run) be sink endlet return await run(\"x\") endproc"
    )));
    accepted(&with_l(
        "fn prepare takes {m: Memory} returns plan: Intent(Void {Log}) do \
         return Log.append(m \"x\") endfn",
    ));
}

#[test]
fn a_row_that_does_not_resolve_is_reported_at_every_position() {
    let bad = "Proc({Str} Void {Shown})";
    for (rest, nth) in [
        (
            format!(
                "proc emit takes {{sink: {bad}}} returns d: Void do return void endproc"
            ),
            1,
        ),
        (
            format!("fn pick returns sink: {bad} do return pick() endfn"),
            1,
        ),
        (
            format!(
                "fn keep takes {{s: Proc({{Str}} Void)}} returns r: I32 do \
                 let t: {bad} be s endlet return 1i32 endfn"
            ),
            1,
        ),
        (format!("type Hooks of {{onLine: {bad}}} endtype"), 1),
        (
            format!(
                "proc each takes {{sinks: List({bad})}} returns d: Void do return void endproc"
            ),
            1,
        ),
        (format!("type Sink of {bad} endtype"), 1),
    ] {
        let source = with_l(&rest);
        let found = of(&source);
        assert!(
            found.contains(&error(
                DiagnosticCode::EffectInvalidReference,
                at(&source, "Shown", nth)
            )),
            "{source}\n{found:#?}"
        );
        assert!(
            found.iter().all(|(_, level, ..)| *level == Level::Error),
            "{source}\n{found:#?}"
        );
    }
}

#[test]
fn a_written_row_resolves_as_a_performs_row() {
    let source = with_l(
        "proc emit takes {sink: Proc({Str} Void {Log Log})} returns d: Void do \
         return void endproc",
    );
    // The second entry, related to the first.
    let row = at(&source, "{Log Log}", 0).start();
    assert_eq!(
        of(&source),
        [(
            DiagnosticCode::EffectDuplicateRoot,
            Level::Error,
            ByteSpan::new(row + 5, row + 8),
            vec![ByteSpan::new(row + 1, row + 4)]
        )]
    );
    let source = with_l(
        "fn prepare takes {m: Memory} returns plan: Intent(Void {Memory}) do \
         return Log.append(m \"x\") endfn",
    );
    assert_eq!(
        of(&source),
        [error(
            DiagnosticCode::EffectInvalidReference,
            at(&source, "Memory", 3)
        )]
    );
}

#[test]
fn an_omitted_row_is_the_empty_row() {
    // `double` has the empty row, so it is a `Proc({I32} I32)`.
    let source = "proc double takes {n: I32} returns twice: I32 do return I32.add(n n) endproc\n\
                  proc run returns r: I32 do let f: Proc({I32} I32) be double endlet \
                  return await f(2i32) endproc\n";
    accepted(source);
    let source = "proc double takes {n: I32} returns twice: I32 do return I32.add(n n) endproc\n\
                  proc run returns r: I32 do let f: Proc({I32} I32 {}) be double endlet \
                  return await f(2i32) endproc\n";
    accepted(source);
}

// `@type.suspiciousRow` (Q6).

#[test]
fn a_row_written_as_the_result_is_suspicious() {
    let source = with_l(
        "fn keep takes {s: Proc({Str} {Log})} returns r: I32 do return 1i32 endfn",
    );
    assert_eq!(
        of(&source),
        [warning(
            DiagnosticCode::TypeSuspiciousRow,
            at(&source, "Proc({Str} {Log})", 0)
        )]
    );
    let source = with_l(
        "fn keep takes {p: Intent({Log Clock})} returns r: I32 do return 1i32 endfn",
    );
    assert_eq!(
        of(&source),
        [warning(
            DiagnosticCode::TypeSuspiciousRow,
            at(&source, "Intent({Log Clock})", 0)
        )]
    );
    // Read twice, as a parameter type is, it is reported once.
    let source = with_l(
        "proc keep takes {s: Proc({Str} {Log})} returns r: I32 do return 1i32 endproc\n\
         proc run returns r: I32 do return 1i32 endproc",
    );
    assert_eq!(
        of(&source),
        [warning(
            DiagnosticCode::TypeSuspiciousRow,
            at(&source, "Proc({Str} {Log})", 0)
        )]
    );
}

#[test]
fn a_row_that_is_written_or_a_result_that_is_no_row_is_not_suspicious() {
    accepted(&with_l(
        "fn keep takes {s: Proc({Str} {Log} {Log})} returns r: I32 do return 1i32 endfn",
    ));
    // A `returns` type is never reported.
    accepted(&with_l(
        "fn pair takes {a: Memory} returns both: {Memory Memory} do return {a a} endfn",
    ));
    // A component that is no operation interface.
    accepted(&with_l(
        "fn keep takes {s: Proc({Str} {Log Shown})} returns r: I32 do return 1i32 endfn",
    ));
    accepted(&with_l(
        "fn keep takes {s: Proc({Str} {Memory})} returns r: I32 do return 1i32 endfn",
    ));
}

// Rows compare exactly: an argument, a returned value, a field
// initializer, and a `let` with an annotation.

/// An anonymous `proc` of `Sink`'s parameters with the row `row`.
fn sink_closure(row: &str) -> String {
    format!(
        "proc takes {{line: Str}} returns d: Void {row} do \
         return await Log.append(Memory(lines: []) line) endproc"
    )
}

#[test]
fn an_argument_row_compares_exactly() {
    let exact = sink_closure("performs {Log}");
    accepted(&with_emit(&format!(
        "proc run returns d: Void performs {{Log}} do return await emit(\"x\" {exact}) endproc"
    )));
    let wider = "proc takes {line: Str} returns d: Void performs {Log Clock} do \
                 return await Log.append(Memory(lines: []) line) endproc";
    let source = with_emit(&format!(
        "proc run returns d: Void performs {{Log}} do return await emit(\"x\" {wider}) endproc"
    ));
    assert_eq!(
        of(&source),
        [error(
            DiagnosticCode::TypeArgumentMismatch,
            at(&source, wider, 0)
        )]
    );
    let narrower = "proc takes {line: Str} returns d: Void do return void endproc";
    let source = with_emit(&format!(
        "proc run returns d: Void performs {{Log}} do return await emit(\"x\" {narrower}) \
         endproc"
    ));
    assert_eq!(
        of(&source),
        [error(
            DiagnosticCode::TypeArgumentMismatch,
            at(&source, narrower, 0)
        )]
    );
    // A procedure that performs less is wrapped in an anonymous `proc` that
    // declares the wider row, whose unused root is a warning.
    let wrapped = "proc takes {line: Str} returns d: Void performs {Log} do \
                   return await quiet(line) endproc";
    let source = with_emit(&format!(
        "proc run returns d: Void performs {{Log}} do return await emit(\"x\" {wrapped}) \
         endproc"
    ));
    let log = source
        .find(wrapped)
        .map_or(0, |start| start + wrapped.find("Log").unwrap_or_default());
    assert_eq!(
        of(&source),
        [warning(
            DiagnosticCode::ContractUnusedEffect,
            ByteSpan::new(log, log + "Log".len())
        )]
    );
}

#[test]
fn a_returned_row_compares_exactly() {
    accepted(&with_l(
        "fn pick takes {m: Memory} returns sink: Proc({Str} Void {Log}) do \
         return proc takes {line: Str} returns d: Void performs {Log} do \
         return await Log.append(m line) endproc endfn",
    ));
    let closure = "proc takes {line: Str} returns d: Void performs {Log} do \
                   return await Log.append(m line) endproc";
    let source = with_l(&format!(
        "fn pick takes {{m: Memory}} returns sink: Proc({{Str}} Void {{}}) do \
         return {closure} endfn"
    ));
    assert_eq!(
        of(&source),
        [error(DiagnosticCode::TypeMismatch, at(&source, closure, 0))]
    );
}

#[test]
fn a_field_row_compares_exactly() {
    let hooks = "type Hooks of {onLine: Proc({Str} Void {Log})} endtype\n\
                 proc appendTo takes {line: Str} returns d: Void performs {Log} do \
                 return await Log.append(Memory(lines: []) line) endproc\n";
    accepted(&with_emit(&format!(
        "{hooks}fn make returns h: Hooks do return Hooks(onLine: appendTo) endfn"
    )));
    let source = with_emit(&format!(
        "{hooks}fn make returns h: Hooks do return Hooks(onLine: quiet) endfn"
    ));
    assert_eq!(
        of(&source),
        [error(
            DiagnosticCode::TypeArgumentMismatch,
            at(&source, "quiet", 1)
        )]
    );
}

#[test]
fn a_let_annotation_row_compares_exactly() {
    accepted(&with_l(
        "fn prepare takes {m: Memory} returns r: I32 do \
         let plan: Intent(Void {Log}) be Log.append(m \"x\") endlet return 1i32 endfn",
    ));
    let source = with_l(
        "fn prepare takes {m: Memory} returns r: I32 do \
         let plan: Intent(Void {}) be Log.append(m \"x\") endlet return 1i32 endfn",
    );
    assert_eq!(
        of(&source),
        [error(
            DiagnosticCode::TypeMismatch,
            at(&source, "Log.append(m \"x\")", 1)
        )]
    );
}

#[test]
fn a_row_compares_exactly_where_a_type_parameter_is_inferred() {
    let call = "proc call(T) takes {f: Proc({T} Void {Log}) x: T} returns d: Void \
                performs {Log} do return await f(x) endproc\n";
    accepted(&with_emit(&format!(
        "{call}proc appendTo takes {{line: Str}} returns d: Void performs {{Log}} do \
         return await Log.append(Memory(lines: []) line) endproc\n\
         proc run returns d: Void performs {{Log}} do return await call(appendTo \"x\") \
         endproc"
    )));
    let source = with_emit(&format!(
        "{call}proc run returns d: Void performs {{Log}} do return await call(quiet \"x\") \
         endproc"
    ));
    assert_eq!(
        of(&source),
        [error(
            DiagnosticCode::TypeArgumentMismatch,
            at(&source, "quiet", 1)
        )]
    );
}

#[test]
fn fn_and_proc_are_distinct_types() {
    let source = "fn twice takes {line: Str} returns d: Void do return void endfn\n\
                  proc call takes {sink: Proc({Str} Void {})} returns d: Void do \
                  return await sink(\"x\") endproc\n\
                  proc run returns d: Void do return await call(twice) endproc\n";
    assert_eq!(
        of(source),
        [error(
            DiagnosticCode::TypeArgumentMismatch,
            at(source, "twice", 1)
        )]
    );
    let source = "proc double takes {n: I32} returns twice: I32 do return I32.add(n n) endproc\n\
                  fn call takes {f: Fn({I32} I32)} returns r: I32 do return f(1i32) endfn\n\
                  fn run returns r: I32 do return call(double) endfn\n";
    assert_eq!(
        of(source),
        [error(
            DiagnosticCode::TypeArgumentMismatch,
            at(source, "double", 1)
        )]
    );
}

// A procedure is a value: a module-level `proc`, a member of a type, a
// contract member, an anonymous `proc`, and one captured in a closure.

#[test]
fn a_named_proc_is_a_value_of_its_proc_type() {
    accepted(&with_emit(
        "proc appendTo takes {line: Str} returns d: Void performs {Log} do \
         return await Log.append(Memory(lines: []) line) endproc\n\
         proc run returns d: Void performs {Log} do return await emit(\"x\" appendTo) endproc",
    ));
    accepted(&with_emit(
        "type Out of {n: I32} proc write takes {line: Str} returns d: Void performs {Log} do \
         return await Log.append(Memory(lines: []) line) endproc endtype\n\
         proc run returns d: Void performs {Log} do return await emit(\"x\" Out.write) \
         endproc",
    ));
    // A contract `proc` member at a closed receiver has the member's row.
    accepted(&with_l(
        "proc run takes {m: Memory} returns d: Void performs {Log} do \
         let f: Proc({Memory Str} Void {Log}) be Log.append endlet \
         return await f(m \"x\") endproc",
    ));
    // A `proc` value captured in a closure.
    accepted(&with_emit(
        "proc run returns d: Void performs {Log} do \
         let s: Proc({Str} Void {Log}) be note2 endlet \
         let g be proc takes {line: Str} returns d: Void performs {Log} do \
         return await s(line) endproc endlet \
         return await emit(\"x\" g) endproc\n\
         proc note2 takes {line: Str} returns d: Void performs {Log} do \
         return await Log.append(Memory(lines: []) line) endproc",
    ));
}

#[test]
fn a_proc_value_of_another_row_is_refused_at_every_value_position() {
    // A member of a type with the empty row.
    let source = with_emit(
        "type Out of {n: I32} proc write takes {line: Str} returns d: Void do \
         return void endproc endtype\n\
         proc run returns d: Void performs {Log} do return await emit(\"x\" Out.write) \
         endproc",
    );
    assert_eq!(
        of(&source),
        [error(
            DiagnosticCode::TypeArgumentMismatch,
            at(&source, "Out.write", 0)
        )]
    );
    // A contract `proc` member keeps its complete row.
    let source = with_l(
        "proc run takes {m: Memory} returns d: Void do \
         let f: Proc({Memory Str} Void) be Log.append endlet \
         return await f(m \"x\") endproc",
    );
    assert_eq!(
        of(&source),
        [error(
            DiagnosticCode::TypeMismatch,
            at(&source, "Log.append", 1)
        )]
    );
    // A procedure value captured in an anonymous `proc` whose row omits the
    // value's: the hop through the captured name is indirect.
    let source = with_emit(
        "proc run returns d: Void do \
         let s: Proc({Str} Void {Log}) be note2 endlet \
         let g be proc takes {line: Str} returns d: Void do \
         return await s(line) endproc endlet return void endproc\n\
         proc note2 takes {line: Str} returns d: Void performs {Log} do \
         return await Log.append(Memory(lines: []) line) endproc",
    );
    assert_eq!(
        of(&source),
        [(
            DiagnosticCode::EffectOutsideCeiling,
            Level::Error,
            at(&source, "await s(line)", 0),
            vec![at(&source, "s(line)", 0)]
        )]
    );
}

#[test]
fn a_named_proc_value_follows_the_rules_of_fn_values() {
    // A bare `requires` is refused at the conversion, related to the guard.
    let call = "proc call takes {f: Proc({I32} Void)} returns d: Void do \
                return await f(1i32) endproc\n";
    let source = format!(
        "{call}proc pos takes {{n: I32}} returns d: Void \
         requires Ordered.greater(n 0i32) do return void endproc\n\
         proc run returns d: Void do return await call(pos) endproc\n"
    );
    assert_eq!(
        of(&source),
        [(
            DiagnosticCode::ContractObligationLost,
            Level::Error,
            at(&source, "pos", 1),
            vec![at(&source, "Ordered.greater(n 0i32)", 0)]
        )]
    );
    // A guard with `else` travels with the value.
    accepted(&format!(
        "{call}proc pos takes {{n: I32}} returns d: Void \
         requires Ordered.greater(n 0i32) else void do return void endproc\n\
         proc run returns d: Void do return await call(pos) endproc\n"
    ));
}
#[test]
fn a_proc_with_a_mismatched_row_is_no_value_of_the_type() {
    let source = with_emit(
        "proc run returns d: Void performs {Log} do return await emit(\"x\" quiet) endproc",
    );
    assert_eq!(
        of(&source),
        [error(
            DiagnosticCode::TypeArgumentMismatch,
            at(&source, "quiet", 1)
        )]
    );
}

// The anonymous `proc`.

#[test]
fn an_anonymous_proc_is_checked_against_its_own_ceiling() {
    let source = with_l(
        "fn pick takes {m: Memory} returns sink: Proc({Str} Void) do \
         return proc takes {line: Str} returns d: Void do \
         return await Log.append(m line) endproc endfn",
    );
    let found = of(&source);
    let [(code, Level::Error, span, related)] = found.as_slice() else {
        panic_free_fail(&found);
        return;
    };
    assert_eq!(*code, DiagnosticCode::EffectOutsideCeiling);
    assert_eq!(*span, at(&source, "await Log.append(m line)", 0));
    assert_eq!(related, &[at(&source, "Log.append(m line)", 0)]);
    // A row that names no operation interface.
    let source = with_l(
        "fn pick returns sink: Proc({Str} Void) do \
         return proc takes {line: Str} returns d: Void performs {Shown} do \
         return void endproc endfn",
    );
    assert_eq!(
        of(&source),
        [error(
            DiagnosticCode::EffectInvalidReference,
            at(&source, "Shown", 1)
        )]
    );
}

/// Fails the test with `found`.
fn panic_free_fail(found: &[Seen]) {
    assert_eq!(found.len(), 1, "{found:#?}");
}

#[test]
fn building_an_anonymous_proc_performs_nothing() {
    // The `fn` builds the closure and performs nothing; the closure's own
    // `await` contributes to its own row.
    accepted(&with_l(
        "fn pick takes {m: Memory} returns sink: Proc({Str} Void {Log}) do \
         let s be proc takes {line: Str} returns d: Void performs {Log} do \
         return await Log.append(m line) endproc endlet return s endfn",
    ));
    // In a `proc` with the empty row, building it is no `await`.
    accepted(&with_l(
        "proc build takes {m: Memory} returns r: I32 do \
         let s be proc takes {line: Str} returns d: Void performs {Log} do \
         return await Log.append(m line) endproc endlet return 1i32 endproc",
    ));
}

#[test]
fn an_await_in_an_anonymous_fn_of_an_anonymous_proc_is_invalid() {
    let source = with_l(
        "proc run takes {m: Memory} returns r: I32 do \
         let s be proc returns d: I32 do \
         let f be fn returns e: Void do return await Log.append(m \"x\") endfn endlet \
         return 1i32 endproc endlet return 1i32 endproc",
    );
    assert_eq!(
        of(&source),
        [error(
            DiagnosticCode::EffectInvalidAwait,
            at(&source, "await Log.append(m \"x\")", 1)
        )]
    );
}

#[test]
fn an_anonymous_fn_with_a_row_is_an_invalid_section() {
    let source = with_l(
        "fn pick returns r: I32 do \
         let f be fn takes {line: Str} returns d: Void performs {Log} do return void endfn \
         endlet return 1i32 endfn",
    );
    let found = of(&source);
    assert!(
        found
            .iter()
            .any(|(code, ..)| *code == DiagnosticCode::SyntaxInvalidSection),
        "{found:#?}"
    );
    assert!(
        found
            .iter()
            .all(|(code, ..)| *code != DiagnosticCode::ToolUnavailable),
        "{found:#?}"
    );
}

#[test]
fn an_anonymous_proc_lowers_to_a_closure_of_its_proc_type() {
    let source = with_l(
        "fn pick takes {m: Memory} returns sink: Proc({Str} Void {Log}) do \
         return proc takes {line: Str} returns d: Void performs {Log} do \
         return await Log.append(m line) endproc endfn",
    );
    let result = check_source(SOURCE, &source);
    assert_eq!(seen(result.diagnostics()), []);
    let Some(set) = result.module_set() else {
        return;
    };
    let Some(index) = result
        .functions()
        .iter()
        .find(|(name, _)| name == "app.main.pick")
        .map(|(_, index)| *index)
    else {
        return;
    };
    let Some(FunctionBody::Expr(body)) = set.functions().get(index).map(|f| &f.body)
    else {
        return;
    };
    let closure = closure_of(body);
    let Some(Type::Proc(signature)) = &closure else {
        assert!(closure.is_some(), "{body:?}");
        return;
    };
    assert_eq!(signature.parameters.len(), 1);
    assert_eq!(signature.effects.roots().len(), 1);
}

/// The type of the first closure in `node`.
fn closure_of(node: &Node) -> Option<Type> {
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        match &node.expr {
            Expr::Closure { closure, .. } => return Some(closure.value_type.clone()),
            Expr::Return(inner) => pending.push(inner),
            Expr::Let { value, body, .. } => {
                pending.push(body);
                pending.push(value);
            }
            Expr::Sequence(nodes) => pending.extend(nodes.iter().rev()),
            _ => {}
        }
    }
    None
}

// A callback's row, and the witness through it.

#[test]
fn a_callback_contributes_its_row_marked_indirect() {
    let source = format!(
        "{L}proc emit takes {{line: Str sink: Proc({{Str}} Void {{Log}})}} returns d: Void \
         do return await sink(line) endproc\n"
    );
    let result = check_source(SOURCE, &source);
    let [diagnostic] = result.diagnostics() else {
        assert_eq!(result.diagnostics().len(), 1, "{:#?}", result.diagnostics());
        return;
    };
    assert_eq!(diagnostic.code(), DiagnosticCode::EffectOutsideCeiling);
    assert_eq!(
        diagnostic.primary_span(),
        at(&source, "await sink(line)", 0)
    );
    let related = diagnostic
        .related()
        .iter()
        .map(|related| (related.span, related.message.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        related,
        [(
            at(&source, "sink(line)", 0),
            "performs Log via sink (indirect)"
        )]
    );
}

// A plan a `fn` returns is no tail call.

#[test]
fn a_returned_plan_is_built_not_tail_called() {
    let source = with_l(
        "fn prepare takes {m: Memory} returns plan: Intent(Void {Log}) do \
         return Log.append(m \"x\") endfn\n\
         fn twice takes {n: I32} returns plan: Intent(I32) do return double(n) endfn\n\
         proc double takes {n: I32} returns r: I32 do return I32.add(n n) endproc\n\
         proc main returns r: I32 do return await twice(2i32) endproc",
    );
    accepted(&source);
}

// A procedure value awaited twice, and an anonymous `proc` capturing a
// local; `V2-RUNTIME-callback-awaited` and `V2-RUNTIME-anon-proc-captures`
// run them.

#[test]
fn an_awaited_callback_and_a_captured_local_check() {
    accepted(
        "proc double takes {n: I32} returns r: I32 do return I32.add(n n) endproc\n\
         proc apply takes {f: Proc({I32} I32) n: I32} returns r: I32 do \
         let a be await f(n) endlet return await f(a) endproc\n\
         fn adder takes {k: I32} returns f: Proc({I32} I32) do \
         return proc takes {n: I32} returns r: I32 do return I32.add(n k) endproc \
         endfn\n\
         proc run returns r: I32 do return await apply(adder(1i32) 2i32) endproc\n",
    );
}
// The last statement of a body whose value is not used: "An expression
// statement has type `Void` or `Never`" ("Bodies and statements"). Only the
// last statement of a branch whose value is used, of an `if`, a `match` arm,
// or a `do` block, gives its value; a callable body leaves with `return`,
// and the `else` of a `let` leaves on every path.

/// `count`, a `fn` whose result is `I32`.
const COUNT: &str = "fn count returns n: I32 do return 1i32 endfn\n";

#[test]
fn the_last_statement_of_a_fn_or_a_proc_body_is_a_statement() {
    let source = format!("{COUNT}fn run returns d: Void do count() endfn\n");
    assert_eq!(
        of(&source),
        [
            error(DiagnosticCode::TypeMismatch, at(&source, "count()", 0)),
            error(DiagnosticCode::TypeMissingReturn, at(&source, "count()", 0)),
        ]
    );
    let source = format!("{COUNT}proc run returns d: Void do count() endproc\n");
    assert_eq!(
        of(&source),
        [
            error(DiagnosticCode::TypeMismatch, at(&source, "count()", 0)),
            error(DiagnosticCode::TypeMissingReturn, at(&source, "count()", 0)),
        ]
    );
    // A plan whose `await` was forgotten, last in a `proc` body.
    let source = "proc one returns n: I32 do return 1i32 endproc\n\
                  proc run returns n: I32 do one() endproc\n";
    assert_eq!(
        of(source),
        [
            error(DiagnosticCode::TypeMismatch, at(source, "one()", 0)),
            error(DiagnosticCode::TypeMissingReturn, at(source, "one()", 0)),
        ]
    );
}

#[test]
fn the_last_statement_of_a_test_body_is_a_statement() -> Result<(), String> {
    let source = format!(
        "use @std.assert enduse\n{COUNT}test \"last\" do assert.true(true) count() endtest\n\
         test \"inner\" do count() assert.true(true) endtest\n"
    );
    let found = seen(in_tests(&source)?.diagnostics());
    assert_eq!(
        found,
        [
            error(DiagnosticCode::TypeMismatch, at(&source, "count()", 0)),
            error(DiagnosticCode::TypeMismatch, at(&source, "count()", 1)),
        ]
    );
    Ok(())
}

/// Checks the test module `@tests.main`, whose source is `source`, beside an
/// empty library.
fn in_tests(source: &str) -> Result<crate::check::CheckResult, String> {
    use vibra_resolve::{PackageId, ResolveInput, SourceModule, SourceUnit, UnitKind};
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
                    Vec::new(),
                )],
            ),
            SourceUnit::new(
                "@tests",
                UnitKind::Tests,
                vec![SourceModule::new(
                    "@tests.main",
                    "tests/main.vib",
                    source.as_bytes().to_vec(),
                )],
            ),
        ],
        stdlib.overlay().embedded_unit(),
    );
    Ok(crate::check::check_workspace(
        &input,
        &["@tests".to_owned()],
    ))
}

#[test]
fn the_last_statement_of_a_let_fallback_is_a_statement() {
    let source = format!(
        "{COUNT}fn run takes {{o: Option(I32)}} returns r: I32 do \
         let Option.some(value: v) be o else count() endlet return v endfn\n"
    );
    // One fault at the statement, which is the whole fallback: the fallback
    // that runs off its end is not reported again at the same span.
    assert_eq!(
        of(&source),
        [error(
            DiagnosticCode::TypeMismatch,
            at(&source, "count()", 0)
        )]
    );
    // With two statements the fallback is a span of its own, and both are
    // reported.
    let source = format!(
        "{COUNT}fn run takes {{o: Option(I32)}} returns r: I32 do \
         let Option.some(value: v) be o else let _ be 0i32 endlet count() endlet \
         return v endfn\n"
    );
    assert_eq!(
        of(&source),
        [
            error(
                DiagnosticCode::TypeMismatch,
                at(&source, "let _ be 0i32 endlet count()", 0)
            ),
            error(DiagnosticCode::TypeMismatch, at(&source, "count()", 0)),
        ]
    );
}

#[test]
fn the_last_statement_of_a_branch_whose_value_is_used_gives_it() {
    accepted(&format!(
        "{COUNT}fn run takes {{go: Bool}} returns r: I32 do \
         return if go then count() else 2i32 endif endfn\n"
    ));
    accepted(&format!(
        "{COUNT}fn run returns r: I32 do let a be do let b be 1i32 endlet count() enddo \
         endlet return a endfn\n"
    ));
}
