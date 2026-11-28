//! Named procedures, plans, and `await`. Expectations come from the source
//! chapter ("`fn` and `proc`", "`return`, `try`, `await`", "Bodies and
//! statements"), the type chapter ("Application", "Guards and callable
//! types"), the effects chapter ("Cold plans and performed rows").

use vibra_diagnostics::DiagnosticCode;
use vibra_ir::{BodyContext, CheckedModuleSet, Expr, FunctionBody, Node, Row, Type};

use super::{SOURCE, accepted, at, faults, faults_of, workspace};
use crate::check::check_source;

/// The procedure every case builds on.
const P: &str =
    "proc double takes {n: I32} returns twice: I32 do return I32.add(n n) endproc\n";

/// `P` and then `rest`.
fn with_p(rest: &str) -> String {
    format!("{P}{rest}\n")
}

/// The lowered function named `name` of `set`.
fn function<'s>(
    set: &'s CheckedModuleSet,
    source: &str,
    name: &str,
) -> Result<&'s vibra_ir::CheckedFunction, String> {
    let result = check_source(SOURCE, source);
    let index = result
        .functions()
        .iter()
        .find(|(qualified, _)| qualified == name)
        .map(|(_, index)| *index);
    index
        .and_then(|index| set.functions().get(index))
        .ok_or_else(|| format!("no function {name} in {:?}", result.functions()))
}

/// The number of `await` nodes in `node`.
fn awaits(node: &Node) -> usize {
    format!("{node:?}").matches("Await {").count()
}

/// The body of `function`.
fn body(function: &vibra_ir::CheckedFunction) -> Result<&Node, String> {
    match &function.body {
        FunctionBody::Expr(node) => Ok(node),
        other => Err(format!("not a body: {other:?}")),
    }
}

#[test]
fn a_proc_has_a_proc_type_and_runs_in_a_procedure_context() -> Result<(), String> {
    let source = with_p(
        "proc run takes {n: I32} returns r: I32 do return await double(n) endproc",
    );
    let set = accepted(&source)?;
    let double = function(&set, &source, "app.main.double")?;
    let Type::Proc(signature) = double.signature.type_info() else {
        return Err(format!("{:?}", double.signature.type_info()));
    };
    assert_eq!(signature.parameters, [Type::I32]);
    assert!(signature.labels.is_empty());
    assert_eq!(signature.result, Type::I32);
    assert_eq!(signature.effects, Row::empty());
    assert_eq!(double.context, BodyContext::Procedure(Row::empty()));
    let run = body(function(&set, &source, "app.main.run")?)?;
    assert_eq!(awaits(run), 1);
    // `return await double(n)`: the awaited call is no tail call.
    let Expr::Return(operand) = &run.expr else {
        return Err(format!("{:?}", run.expr));
    };
    let Expr::Await { plan } = &operand.expr else {
        return Err(format!("{:?}", operand.expr));
    };
    assert!(matches!(plan.expr, Expr::Call { .. }), "{:?}", plan.expr);
    Ok(())
}

#[test]
fn the_types_document_lists_a_proc_with_its_proc_type() -> Result<(), String> {
    let source = with_p(
        "proc run takes {n: I32} returns r: I32 do return await double(n) endproc",
    );
    let result = check_source(SOURCE, &source);
    let document = result.types_document().unwrap_or_default();
    assert!(
        document.contains(
            "typeInfo: {kind: @proc parameters: [@i32] labels: [:] result: @i32 \
             effects: {roots: []}}"
        ),
        "{document}"
    );
    assert!(!document.contains("kind: @fn"), "{document}");
    Ok(())
}

#[test]
fn a_plan_is_stored_and_awaited_any_number_of_times() -> Result<(), String> {
    accepted(&with_p(
        "proc run takes {n: I32} returns r: I32 do let plan be double(n) endlet \
         return await plan endproc",
    ))?;
    let source = with_p(
        "proc run takes {n: I32} returns r: I32 do let plan be double(n) endlet \
         let a be await plan endlet return I32.add(a await plan) endproc",
    );
    let set = accepted(&source)?;
    assert_eq!(awaits(body(function(&set, &source, "app.main.run")?)?), 2);
    Ok(())
}

#[test]
fn procs_as_members_generic_labelled_and_recursive() -> Result<(), String> {
    let source = with_p(
        "type Box of {n: I32} proc twice takes {self: Self} returns r: I32 do \
         return await double(self.n) endproc endtype",
    );
    let set = accepted(&source)?;
    let twice = function(&set, &source, "app.main.Box.twice")?;
    assert!(matches!(twice.signature.type_info(), Type::Proc(_)));
    accepted(&with_p(
        "proc same(T) takes {value: T} returns r: T do return value endproc\n\
         proc run returns r: I32 do return await same(1i32) endproc",
    ))?;
    accepted(&with_p(
        "proc scale takes {n: I32} labelled {by: I32 be 2i32} returns r: I32 do \
         return I32.add(n by) endproc\n\
         proc run returns r: I32 do let a be await scale(1i32) endlet \
         return await scale(a by: 3i32) endproc",
    ))?;
    accepted(&with_p(
        "proc down takes {n: I32} returns r: I32 do if Ordered.greater(n 0i32) then \
         return await down(I32.sub(n 1i32)) endif return 0i32 endproc",
    ))?;
    Ok(())
}

#[test]
fn a_proc_member_has_a_proc_identity() -> Result<(), String> {
    let source = with_p(
        "type Box of {n: I32} proc twice takes {self: Self} returns r: I32 do \
         return await double(self.n) endproc endtype",
    );
    let result = check_source(SOURCE, &source);
    let document = result.types_document().unwrap_or_default();
    for (owner, name) in [("Box", "twice"), ("", "double")] {
        let identity = crate::stdlib::declaration_identity(
            "source",
            "0.0.0",
            "@app.main",
            owner,
            "@proc",
            name,
        );
        assert!(
            document.contains(&format!("{{declaration: \"{identity}\"")),
            "{name}: {document}"
        );
    }
    Ok(())
}

#[test]
fn the_operand_of_await_is_a_plan() -> Result<(), String> {
    let source = with_p("proc run returns r: I32 do return await 1i32 endproc");
    assert_eq!(
        faults(&source),
        [(
            DiagnosticCode::EffectInvalidAwait,
            at(&source, "await 1i32", 0)
        )]
    );
    let source = with_p(
        "fn one returns r: I32 do return 1i32 endfn\n\
         proc run returns r: I32 do return await one endproc",
    );
    assert_eq!(
        faults(&source),
        [(
            DiagnosticCode::EffectInvalidAwait,
            at(&source, "await one", 0)
        )]
    );
    // A `Result` holding a plan is no plan.
    let source = with_p(
        "fn wrap(T) takes {value: T} returns r: Result(T I32) do \
         return Result.ok(value: value) endfn\n\
         proc run returns r: I32 do return await wrap(double(1i32)) endproc",
    );
    assert_eq!(
        faults(&source),
        [(
            DiagnosticCode::EffectInvalidAwait,
            at(&source, "await wrap(double(1i32))", 0)
        )]
    );
    Ok(())
}

#[test]
fn a_statement_of_another_type_is_a_mismatch() -> Result<(), String> {
    // The general rule a forgotten `await` falls under.
    let source = "fn f returns r: I32 do I32.add(1i32 1i32) return 0i32 endfn\n";
    assert_eq!(
        faults(source),
        [(
            DiagnosticCode::TypeMismatch,
            at(source, "I32.add(1i32 1i32)", 0)
        )]
    );
    // A `Void` statement is accepted.
    let source = "fn g returns r: Void do return void endfn\n\
                  fn f returns r: I32 do g() return 0i32 endfn\n";
    assert_eq!(faults(source), []);
    Ok(())
}

#[test]
fn applying_a_proc_builds_a_plan() -> Result<(), String> {
    // A statement that builds a plan and drops it.
    let source = with_p("proc run returns r: I32 do double(1i32) return 0i32 endproc");
    assert_eq!(
        faults(&source),
        [(DiagnosticCode::TypeMismatch, at(&source, "double(1i32)", 0))]
    );
    // `let _` discards it on purpose, in a `fn` too.
    accepted(&with_p(
        "fn f returns r: I32 do let _ be double(1i32) endlet return 0i32 endfn",
    ))?;
    // The application is a plan, not its result.
    let source = with_p("fn f takes {n: I32} returns r: I32 do return double(n) endfn");
    assert_eq!(
        faults(&source),
        [(DiagnosticCode::TypeMismatch, at(&source, "double(n)", 0))]
    );
    // The arguments are checked as for a function.
    let source =
        with_p("proc run returns r: I32 do return await double(\"x\") endproc");
    assert_eq!(
        faults(&source),
        [(
            DiagnosticCode::TypeArgumentMismatch,
            at(&source, "\"x\"", 0)
        )]
    );
    let source =
        with_p("proc run returns r: I32 do return await double(1i32 2i32) endproc");
    assert_eq!(
        faults(&source),
        [(
            DiagnosticCode::TypeArgumentMismatch,
            at(&source, "double(1i32 2i32)", 0)
        )]
    );
    // A body that runs off its end.
    let source =
        with_p("proc run returns r: I32 do let _ be double(1i32) endlet endproc");
    assert_eq!(
        faults(&source),
        [(
            DiagnosticCode::TypeMissingReturn,
            at(&source, "let _ be double(1i32) endlet", 0)
        )]
    );
    Ok(())
}

#[test]
fn plans_and_worlds_are_checked() -> Result<(), String> {
    // An `await` of a plan whose row is not empty is checked against the
    // ceiling.
    let log = "interface Log proc append takes {log: Self entry: Str} returns d: Void \
               endproc endinterface\n\
               type Memory of I32 impl Log proc append takes {log: Self entry: Str} \
               returns d: Void do return void endproc endimpl endtype\n";
    let source = format!(
        "{log}proc p takes {{m: Memory}} returns d: Void performs {{Log}} do \
         return await Log.append(m \"x\") endproc\n"
    );
    assert_eq!(faults(&source), []);
    // A `proc` named as a value has its `Proc` type, and a contract `proc`
    // member named as a value has its own.
    let source = with_p(
        "proc run returns r: I32 do let f: Proc({I32} I32) be double endlet \
         return await f(1i32) endproc",
    );
    assert_eq!(faults(&source), []);
    let source = format!(
        "{log}fn f returns r: I32 do let _: Proc({{Memory Str}} Void {{Log}}) be \
         Log.append endlet return 1i32 endfn\n"
    );
    assert_eq!(faults(&source), []);
    // A test that takes a world is checked and lowered beside the others.
    let test = "use @std.assert enduse\n\
                use @std.test.TestWorld enduse\n\
                test \"world\" takes {world: TestWorld} do assert.true(true) endtest\n\
                test \"plain\" do assert.true(true) endtest\n";
    let result = in_tests("", test)?;
    assert_eq!(faults_of(&result), []);
    let lowered = result
        .tests()
        .iter()
        .map(|test| (test.name(), test.function().is_some()))
        .collect::<Vec<_>>();
    assert_eq!(lowered, [("world", true), ("plain", true)]);
    Ok(())
}

/// The library module every test case imports as `math`.
const MATH: &str = "proc double takes {n: I32} returns twice: I32 visibility @public do \
                    return I32.add(n n) endproc\n";

/// The lowered function of the test `name` of `result`.
fn test_function<'r>(
    result: &'r crate::check::CheckResult,
    name: &str,
) -> Result<&'r vibra_ir::CheckedFunction, String> {
    let index = result
        .tests()
        .iter()
        .find(|test| test.name() == name)
        .and_then(|test| test.function())
        .ok_or_else(|| format!("the test {name} was not lowered"))?;
    result
        .test_set()
        .and_then(|set| set.functions().get(index))
        .ok_or_else(|| format!("no function {index} for the test {name}"))
}

#[test]
fn a_test_body_runs_in_a_procedure_context_with_the_empty_row() -> Result<(), String> {
    let test = "use @std.assert enduse\nuse @app.math enduse\n\
                test \"awaits\" do assert.equal(4i32 await math.double(2i32)) endtest\n\
                test \"empty row\" performs {} do assert.true(true) endtest\n";
    let result = in_tests(MATH, test)?;
    assert_eq!(faults_of(&result), []);
    for name in ["awaits", "empty row"] {
        let function = test_function(&result, name)?;
        // A test is not applicable: its function is a `Fn({} Void)`, never a
        // `Proc`, and its body runs in a procedure context.
        let Type::Fn(signature) = function.signature.type_info() else {
            return Err(format!("{name}: {:?}", function.signature.type_info()));
        };
        assert!(signature.parameters.is_empty(), "{name}");
        assert_eq!(signature.result, Type::Void, "{name}");
        assert_eq!(
            function.context,
            BodyContext::Procedure(Row::empty()),
            "{name}"
        );
    }
    assert_eq!(awaits(body(test_function(&result, "awaits")?)?), 1);
    Ok(())
}

#[test]
fn await_at_any_depth_of_a_test_body() -> Result<(), String> {
    let test = "use @std.assert enduse\nuse @app.math enduse\n\
                test \"deep\" do \
                let plan be math.double(1i32) endlet \
                let a be do let x be await plan endlet I32.add(x 1i32) enddo endlet \
                if Ordered.greater(a 0i32) then assert.equal(2i32 await plan) \
                else assert.true(false) endif \
                match true case true assert.equal(6i32 I32.add(a await plan)) \
                case false assert.true(false) endmatch \
                endtest\n";
    let result = in_tests(MATH, test)?;
    assert_eq!(faults_of(&result), []);
    assert_eq!(awaits(body(test_function(&result, "deep")?)?), 3);
    Ok(())
}

#[test]
fn await_in_an_anonymous_fn_of_a_test_body() -> Result<(), String> {
    let test = "use @std.assert enduse\nuse @app.math enduse\n\
                test \"anon\" do \
                let f be fn returns r: I32 do return await math.double(1i32) endfn endlet \
                assert.equal(2i32 f()) endtest\n";
    let result = in_tests(MATH, test)?;
    assert_eq!(
        faults_of(&result),
        [(
            DiagnosticCode::EffectInvalidAwait,
            at(test, "await math.double(1i32)", 0)
        )]
    );
    Ok(())
}

/// `need`, whose bare `requires` asks for a positive operand, and `rest`.
fn with_need(rest: &str) -> String {
    format!(
        "fn need takes {{n: I32}} returns r: I32 \
         requires Ordered.greater(n 0i32) do return n endfn\n{rest}\n"
    )
}

/// The one diagnostic of `source`, by code and primary span.
fn single(
    source: &str,
) -> Result<(DiagnosticCode, vibra_diagnostics::ByteSpan), String> {
    let found = faults(source);
    match found.as_slice() {
        [one] => Ok(*one),
        other => Err(format!("{source}\n{other:#?}")),
    }
}

// The soundness rows of GUARD-7 and `await` (`docs/spec/02-type-system.md`,
// "Bare guards", rule 7): running a plan is the call, so after `await
// f(args)`, whose operand is directly the application of a named `proc`, the
// callee's proven bare `ensures` holds of the awaited result. A plan that
// reaches `await` any other way gives no fact, building a plan gives none,
// and nothing reads through a `proc` body.

/// `one`, a `proc` whose bare `ensures` states a positive result.
const ONE: &str =
    "proc one returns r: I32 ensures Ordered.greater(r 0i32) do return 1i32 endproc";

#[test]
fn a_proc_awaited_directly_gives_its_result_fact() -> Result<(), String> {
    // The control: the same `ensures` on a `fn` is a certified result.
    accepted(&with_need(
        "fn one returns r: I32 ensures Ordered.greater(r 0i32) do return 1i32 endfn\n\
         fn run returns r: I32 do return need(one()) endfn",
    ))?;
    // The awaited result of a named `proc` applied directly has the fact.
    accepted(&with_need(&format!(
        "{ONE}\nproc run returns r: I32 do return need(await one()) endproc"
    )))?;
    // The `proc`'s own `ensures` is still proved of its body.
    accepted(&format!("{ONE}\n"))?;
    Ok(())
}

#[test]
fn a_plan_awaited_through_a_binding_gives_no_result_fact() -> Result<(), String> {
    let source = with_need(&format!(
        "{ONE}\nproc run returns r: I32 do let p be one() endlet \
         return need(await p) endproc"
    ));
    assert_eq!(
        single(&source)?,
        (
            DiagnosticCode::ContractUnproved,
            at(&source, "need(await p)", 0)
        )
    );
    Ok(())
}

#[test]
fn a_plan_applied_and_never_awaited_states_nothing() -> Result<(), String> {
    // Building the plan of a `proc` with a bare `ensures` is no call of it:
    // there is no obligation and no fact.
    accepted(&format!(
        "{ONE}\nfn build returns r: I32 do let _ be one() endlet return 0i32 endfn\n"
    ))?;
    Ok(())
}

#[test]
fn a_plan_applied_twice_and_awaited_once() -> Result<(), String> {
    // The application that is awaited directly gives the fact; the one only
    // built gives none, and does not stand for the other.
    accepted(&with_need(&format!(
        "{ONE}\nproc run returns r: I32 do let _ be one() endlet \
         return need(await one()) endproc"
    )))?;
    let source = with_need(&format!(
        "{ONE}\nproc run returns r: I32 do let p be one() endlet let _ be one() endlet \
         return need(await p) endproc"
    ));
    assert_eq!(
        single(&source)?,
        (
            DiagnosticCode::ContractUnproved,
            at(&source, "need(await p)", 0)
        )
    );
    Ok(())
}

#[test]
fn each_run_of_a_plan_is_its_own() -> Result<(), String> {
    // Two `await` nodes of the same application are two runs: what a branch
    // learns of one result is no fact of the other, written inline or bound.
    let plain = "proc one returns r: I32 do return 1i32 endproc";
    let source = with_need(&format!(
        "{plain}\nproc run returns r: I32 do \
         if Ordered.greater(await one() 0i32) then return need(await one()) endif \
         return 0i32 endproc"
    ));
    assert_eq!(
        single(&source)?,
        (
            DiagnosticCode::ContractUnproved,
            at(&source, "need(await one())", 0)
        )
    );
    let source = with_need(&format!(
        "{plain}\nproc run returns r: I32 do let a be await one() endlet \
         let b be await one() endlet \
         if Ordered.greater(a 0i32) then return need(b) endif return 0i32 endproc"
    ));
    assert_eq!(
        single(&source)?,
        (DiagnosticCode::ContractUnproved, at(&source, "need(b)", 0))
    );
    // A result bound by `let` keeps its fact (`GUARD-4`).
    accepted(&with_need(&format!(
        "{ONE}\nproc run returns r: I32 do let x be await one() endlet \
         return need(x) endproc"
    )))?;
    Ok(())
}
#[test]
fn a_proc_with_requires_else_gives_no_result_fact() -> Result<(), String> {
    // `requires C else V` skips the body and the `ensures` when `C` is
    // false ("Guards with `else`"), so its `ensures` is no fact of the
    // result, and the caller has no obligation.
    let source = with_need(
        "proc half takes {n: I32} returns r: I32 \
         requires Ordered.greater(n 0i32) else 0i32 \
         ensures Ordered.greater(r 0i32) do return n endproc\n\
         proc run returns r: I32 do return need(await half(5i32)) endproc",
    );
    assert_eq!(
        single(&source)?,
        (
            DiagnosticCode::ContractUnproved,
            at(&source, "need(await half(5i32))", 0)
        )
    );
    Ok(())
}

#[test]
fn an_anonymous_proc_gives_no_result_fact() -> Result<(), String> {
    // An anonymous `proc` is no named `proc`: its proven `ensures` is never
    // a result fact, however its plan is awaited.
    let source = with_need(
        "proc run returns r: I32 do \
         let f be proc returns r: I32 ensures Ordered.greater(r 0i32) do \
         return 1i32 endproc endlet \
         return need(await f()) endproc",
    );
    assert_eq!(
        single(&source)?,
        (
            DiagnosticCode::ContractUnproved,
            at(&source, "need(await f())", 0)
        )
    );
    Ok(())
}
#[test]
fn a_predicate_that_applies_a_proc_is_outside_the_fragment() -> Result<(), String> {
    // "A certified function is a `fn`" ("Predicates"): applying a `proc` in
    // a predicate is a call of a function that is not certified.
    let yes = "fn yes(T) takes {value: T} returns b: Bool do return true endfn\n";
    // The control: a `fn` there is certified.
    accepted(&format!(
        "{yes}fn same takes {{n: I32}} returns r: I32 do return n endfn\n\
         fn guarded takes {{n: I32}} returns r: I32 requires yes(same(n)) else 0i32 \
         do return n endfn\n"
    ))?;
    // The same body on a `proc` is not certified.
    let source = format!(
        "{yes}proc same takes {{n: I32}} returns r: I32 do return n endproc\n\
         fn guarded takes {{n: I32}} returns r: I32 requires yes(same(n)) else 0i32 \
         do return n endfn\n"
    );
    assert_eq!(
        single(&source)?,
        (
            DiagnosticCode::ContractUnsupportedPredicate,
            at(&source, "same(n)", 0)
        )
    );
    Ok(())
}

/// Checks the library module `@app.math` with `app` and the test module
/// `@tests.math` with `test`.
fn in_tests(app: &str, test: &str) -> Result<crate::check::CheckResult, String> {
    workspace(
        &[("@app.math", app)],
        &[("@tests.math", test)],
        &["@app", "@tests"],
    )
}
