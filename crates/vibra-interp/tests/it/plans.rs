//! Plans, `await`, and invocation identity (`docs/spec/06-runtime.md`,
//! "Cold plans and invocation identity", "Callable contract execution",
//! "Tail calls", "Activations and memory", "Audit trace", "What has no
//! encoding"; `docs/spec/07-diagnostics-and-conformance.md`, "Required
//! coverage" 13).
//!
//! `inc` is a `Proc({I32} I32 {})` returning its argument plus one; `spin`
//! is a `Proc({} Never {})` that awaits itself, not in tail position,
//! without end. A test body is a `Fn({} Void)` whose context is
//! `Procedure({})`, run by `run_test`: it is invocation 1.

use std::sync::Arc;

use vibra_diagnostics::DiagnosticCode;
use vibra_interp::{
    Evaluation, Execution, Instrumentation, Invocation, MemoryBudget, RuntimeError,
    TestExecution, evaluate, run, run_test, run_test_probing,
};
use vibra_ir::external::{CompilerIntrinsic, IntegerOp, IntegerType};
use vibra_ir::{
    BodyContext, CheckedFunction, ClosureBody, Construction, ExitGuard, Expr, Guard,
    IntentType, MatchArm, NamedTypes, Node, Pattern, ProcType, Row, SignatureRecord,
    Trap, Type,
};

use crate::{
    BASE, LIMITS, bool_type, boolean, call, call_value, construct, function,
    generic_row, if_else, int, integer, node, origin_at, program, row, set_with,
    signature, u64_add, u64_equal, u64_lit, u64_sub, var, void,
};

const STACK: usize = 256 * 1024;

/// Runs `work` on a thread whose stack is 256 KiB: a stack overflow there
/// aborts the test binary.
fn on_small_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(work)
        .expect("a thread")
        .join()
        .expect("the thread finished")
}

// ----- fixtures -----

fn proc_type_of(parameters: Vec<Type>, result: Type, effects: Row) -> Type {
    Type::Proc(Box::new(ProcType {
        parameters,
        labels: NamedTypes::empty(),
        result,
        effects,
    }))
}

/// `Proc({parameters} result {})`.
fn proc_type(parameters: Vec<Type>, result: Type) -> Type {
    proc_type_of(parameters, result, Row::empty())
}

/// `Intent(result {})`: the type of a plan.
fn intent(result: Type) -> Type {
    Type::Intent(Box::new(IntentType {
        result,
        effects: Row::empty(),
    }))
}

/// A named `proc` of `Proc({parameters} result {})`.
fn procedure(
    name: &str,
    parameters: Vec<Type>,
    result: Type,
    body: Node,
    slot_count: usize,
) -> CheckedFunction {
    function(
        name,
        SignatureRecord::new(Vec::new(), proc_type(parameters, result)).unwrap(),
        body,
        slot_count,
    )
}

/// A test body: `Fn({} Void)` in the procedure context `Procedure(row)`.
fn test_body_in(row: Row, body: Node, slot_count: usize) -> CheckedFunction {
    CheckedFunction {
        context: BodyContext::Procedure(row),
        ..function("test", signature(Vec::new(), Type::Void), body, slot_count)
    }
}

fn test_body(body: Node, slot_count: usize) -> CheckedFunction {
    test_body_in(Row::empty(), body, slot_count)
}

fn await_(plan: Node) -> Node {
    node(Expr::Await {
        plan: Box::new(plan),
    })
}

fn ret(value: Node) -> Node {
    node(Expr::Return(Box::new(value)))
}

fn seq(statements: Vec<Node>) -> Node {
    node(Expr::Sequence(statements))
}

fn let_slot(slot: usize, value: Node, body: Node) -> Node {
    node(Expr::Let {
        slot: Some(slot),
        value: Box::new(value),
        body: Box::new(body),
    })
}

fn i32_add(left: Node, right: Node) -> Node {
    integer(IntegerType::I32, IntegerOp::Add, vec![left, right])
}

fn i32_sub(left: Node, right: Node) -> Node {
    integer(IntegerType::I32, IntegerOp::Sub, vec![left, right])
}

/// Whether `left` compares as `case` with `right`, on `I32`.
fn i32_is(case: &str, left: Node, right: Node, otherwise: bool) -> Node {
    node(Expr::Match {
        subject: Box::new(integer(
            IntegerType::I32,
            IntegerOp::Compare,
            vec![left, right],
        )),
        arms: vec![
            MatchArm {
                pattern: Pattern::Case {
                    case: case.to_owned(),
                    fields: Vec::new(),
                },
                body: boolean(!otherwise),
            },
            MatchArm {
                pattern: Pattern::Discard,
                body: boolean(otherwise),
            },
        ],
        value_type: bool_type(),
    })
}

/// `left > right` on `I32`.
fn i32_greater(left: Node, right: Node) -> Node {
    i32_is("greater", left, right, false)
}

/// `left >= right` on `I32`.
fn i32_not_less(left: Node, right: Node) -> Node {
    i32_is("less", left, right, true)
}

/// `assert.equal(expected, actual)` at `ty`.
fn expect_equal(ty: Type, expected: Node, actual: Node) -> Node {
    generic_row(
        CompilerIntrinsic::AssertEqual,
        vec![ty],
        vec![expected, actual],
    )
}

fn expect_i32(expected: i32, actual: Node) -> Node {
    expect_equal(Type::I32, int(expected), actual)
}

/// `inc(n) = n + 1`, a `Proc({I32} I32 {})`.
fn inc(name: &str) -> CheckedFunction {
    procedure(
        name,
        vec![Type::I32],
        Type::I32,
        ret(i32_add(var(0, Type::I32), int(1))),
        1,
    )
}

/// `spin()`: awaits itself, not in tail position, without end.
fn spin(index: usize) -> CheckedFunction {
    let again = || await_(call(index, Vec::new()));
    procedure(
        "spin",
        Vec::new(),
        Type::Never,
        seq(vec![again(), again()]),
        0,
    )
}

/// Runs the test body `test`, the last of `functions`, which come after
/// the base parts.
fn run_body(
    functions: Vec<CheckedFunction>,
    budget: MemoryBudget,
) -> Result<TestExecution, RuntimeError> {
    let entry = BASE + functions.len() - 1;
    let set = set_with(functions, Vec::new(), Vec::new());
    run_test(&program(set, entry), budget)
}

/// [`run_body`] under the default budget, which must pass: every
/// assertion of the body held.
#[track_caller]
fn passes(functions: Vec<CheckedFunction>) -> Instrumentation {
    let execution = run_body(functions, MemoryBudget::DEFAULT).unwrap();
    assert_eq!(execution.failure, None, "an assertion of the body failed");
    execution.instrumentation
}

// ----- rows -----

/// Row "build does not run": building a plan of `spin` and dropping it
/// runs nothing of `spin`, so the run ends and only the body is an
/// invocation.
#[test]
fn build_does_not_run() {
    let body = test_body(seq(vec![call(BASE, Vec::new()), void()]), 0);
    let stats = passes(vec![spin(BASE), body]);
    assert_eq!(stats.invocation_count, 1);
    assert_eq!(stats.max_activation_depth, 1);
}

/// Row "await runs": `await inc(1)` is `2i32`, and is invocation 2.
#[test]
fn await_runs() {
    let body = test_body(
        seq(vec![expect_i32(2, await_(call(BASE, vec![int(1)])))]),
        0,
    );
    let stats = passes(vec![inc("inc"), body]);
    assert_eq!(stats.invocation_count, 2);
}

/// Row "await twice": one plan bound to a slot and awaited twice; a plan
/// has no "consumed" bit and no memoized result.
#[test]
fn await_twice() {
    let plan = || var(0, intent(Type::I32));
    let body = test_body(
        let_slot(
            0,
            call(BASE, vec![int(1)]),
            seq(vec![expect_i32(4, i32_add(await_(plan()), await_(plan())))]),
        ),
        1,
    );
    let stats = passes(vec![inc("inc"), body]);
    assert_eq!(stats.invocation_count, 3);
}

/// `guarded(n)`: `requires n > 0 else -1`, whose predicate is `predicate`
/// and whose body is `body`.
fn guarded(predicate: Node, body: Node) -> CheckedFunction {
    CheckedFunction {
        entry_guard: Some(Guard {
            predicate,
            consequence: int(-1),
        }),
        ..procedure("guarded", vec![Type::I32], Type::I32, body, 1)
    }
}

fn n() -> Node {
    var(0, Type::I32)
}

fn small_predicate() -> Node {
    i32_greater(n(), int(0))
}

/// The same predicate with more nodes: `(n + 0) - 0 > 0`.
fn large_predicate() -> Node {
    i32_greater(i32_sub(i32_add(n(), int(0)), int(0)), int(0))
}

fn small_body() -> Node {
    ret(n())
}

/// The same result with more nodes: `(n + 0) - 0`.
fn large_body() -> Node {
    ret(i32_sub(i32_add(n(), int(0)), int(0)))
}

/// A body that awaits `guarded(argument)` twice, expecting `expected`.
fn await_guarded_twice(argument: i32, expected: i32) -> CheckedFunction {
    let plan = || var(0, intent(Type::I32));
    test_body(
        let_slot(
            0,
            call(BASE, vec![int(argument)]),
            seq(vec![
                expect_i32(expected, await_(plan())),
                expect_i32(expected, await_(plan())),
            ]),
        ),
        1,
    )
}

/// Row "`requires … else` per await": the guard runs on each `await`, and
/// its consequence is the result each time. The body's reduction count is
/// zero: a larger body changes no count. The comparison is sensitive: with
/// the guard true, the larger body does change it.
#[test]
fn requires_else_runs_on_each_await() {
    let count = |body: Node, argument: i32, expected: i32| {
        passes(vec![
            guarded(small_predicate(), body),
            await_guarded_twice(argument, expected),
        ])
    };
    let small = count(small_body(), 0, -1);
    let large = count(large_body(), 0, -1);
    assert_eq!(small.invocation_count, 3);
    assert_eq!(small.reduction_count, large.reduction_count);
    let small = count(small_body(), 5, 5);
    let large = count(large_body(), 5, 5);
    assert!(small.reduction_count < large.reduction_count);
}

/// Row "`requires … else` not at build": the same plan, built and never
/// awaited, evaluates no node of the predicate: a larger predicate changes
/// no count. Awaited once, it does.
#[test]
fn requires_else_does_not_run_at_build() {
    let count = |predicate: Node, awaited: bool| {
        let built = call(BASE, vec![int(0)]);
        let statement = if awaited {
            expect_i32(-1, await_(built))
        } else {
            built
        };
        passes(vec![
            guarded(predicate, small_body()),
            test_body(seq(vec![statement, void()]), 0),
        ])
    };
    let small = count(small_predicate(), false);
    let large = count(large_predicate(), false);
    assert_eq!(small.invocation_count, 1);
    assert_eq!(small.reduction_count, large.reduction_count);
    let small = count(small_predicate(), true);
    let large = count(large_predicate(), true);
    assert!(small.reduction_count < large.reduction_count);
}

/// Row "`ensures … else` on await": `clamp(n) = n - 5` with `ensures
/// result >= 0 else 0`. A negative candidate is replaced; a positive one is
/// returned.
#[test]
fn ensures_else_runs_on_await() {
    let clamp = CheckedFunction {
        exit_guard: Some(ExitGuard {
            candidate: 1,
            guard: Guard {
                predicate: i32_not_less(var(1, Type::I32), int(0)),
                consequence: int(0),
            },
        }),
        ..procedure(
            "clamp",
            vec![Type::I32],
            Type::I32,
            ret(i32_sub(n(), int(5))),
            2,
        )
    };
    let body = test_body(
        seq(vec![
            expect_i32(0, await_(call(BASE, vec![int(1)]))),
            expect_i32(4, await_(call(BASE, vec![int(9)]))),
        ]),
        0,
    );
    let stats = passes(vec![clamp, body]);
    assert_eq!(stats.invocation_count, 3);
}

/// Row "plan through a record": a plan stored in a record field, read back,
/// and awaited.
#[test]
fn a_plan_through_a_record() {
    let record_type = Type::record([("p".to_owned(), intent(Type::I32))]).unwrap();
    let record = construct(
        record_type,
        Construction::Record(vec![("p".to_owned(), call(BASE, vec![int(1)]))]),
    );
    let field = node(Expr::Field {
        record: Box::new(record),
        field: "p".to_owned(),
    });
    let body = test_body(seq(vec![expect_i32(2, await_(field))]), 0);
    let stats = passes(vec![inc("inc"), body]);
    assert_eq!(stats.invocation_count, 2);
}

/// Row "plan captured by a closure": an anonymous `Proc({} I32 {})` that
/// awaits the plan it captured. Its own run is an invocation, and so is the
/// captured plan's.
#[test]
fn a_plan_captured_by_a_closure() {
    let captured = node(Expr::Captured {
        slot: 0,
        value_type: intent(Type::I32),
    });
    let anonymous = node(Expr::Closure {
        closure: Arc::new(ClosureBody {
            value_type: proc_type(Vec::new(), Type::I32),
            slot_count: 0,
            captures: vec![intent(Type::I32)],
            defaults: Vec::new(),
            body: ret(await_(captured)),
            entry_guard: None,
            exit_guard: None,
        }),
        captures: vec![call(BASE, vec![int(1)])],
    });
    let body = test_body(
        seq(vec![expect_i32(
            2,
            await_(call_value(anonymous, Vec::new())),
        )]),
        0,
    );
    let stats = passes(vec![inc("inc"), body]);
    assert_eq!(stats.invocation_count, 3);
}

/// An assertion, which traps outside a test, written at `start..start + 1`
/// so that the trap names which operand ran.
fn trap_at(start: usize) -> Node {
    let mut trap = row(CompilerIntrinsic::AssertTrue, vec![boolean(true)]);
    trap.origin = origin_at(start, start + 1);
    trap
}

/// An operand that evaluates to `value`, trapping at `start` first when
/// `trap` holds.
fn operand(trap: bool, start: usize, value: i32) -> Node {
    if trap {
        seq(vec![trap_at(start), int(value)])
    } else {
        int(value)
    }
}

/// Row "operands in written order": `inc2(a, b)` evaluates `a`, then `b`,
/// when it is applied: the plan is never awaited, and the first operand
/// that traps is the first written.
#[test]
fn operands_are_evaluated_in_written_order_at_application() {
    let inc2 = procedure(
        "inc2",
        vec![Type::I32, Type::I32],
        Type::I32,
        ret(i32_add(var(0, Type::I32), var(1, Type::I32))),
        2,
    );
    let outcome = |first: bool, second: bool| {
        let main = function(
            "main",
            signature(Vec::new(), Type::Void),
            seq(vec![
                call(BASE, vec![operand(first, 1, 1), operand(second, 2, 2)]),
                void(),
            ]),
            0,
        );
        let set = set_with(vec![inc2.clone(), main], Vec::new(), Vec::new());
        run(&program(set, BASE + 1), MemoryBudget::DEFAULT)
    };
    let trapped_at = |result: Result<Execution, RuntimeError>| match result {
        Err(RuntimeError::Trap {
            trap: Trap::InvalidCheckedProgram,
            origin: Some(origin),
            ..
        }) => origin.span().start(),
        other => panic!("expected a trap, got {other:?}"),
    };
    assert_eq!(trapped_at(outcome(true, true)), 1);
    assert_eq!(trapped_at(outcome(false, true)), 2);
    let execution = outcome(false, false).unwrap();
    assert_eq!(execution.instrumentation.invocation_count, 1);
}

/// `count(n)`: `if n == 0 then return 0 else return await count(n - 1)`,
/// a `Proc({U64} U64 {})` at `index`.
fn count_down(index: usize) -> CheckedFunction {
    let n = || var(0, Type::U64);
    procedure(
        "count",
        vec![Type::U64],
        Type::U64,
        if_else(
            u64_equal(n(), u64_lit(0)),
            ret(u64_lit(0)),
            ret(await_(call(index, vec![u64_sub(n(), u64_lit(1))]))),
        ),
        1,
    )
}

/// A body that awaits `count(transfers)`, not in tail position.
fn count_body(transfers: u64) -> CheckedFunction {
    test_body(
        seq(vec![expect_equal(
            Type::U64,
            u64_lit(0),
            await_(call(BASE, vec![u64_lit(transfers)])),
        )]),
        0,
    )
}

fn tail_awaits(transfers: u64, budget: MemoryBudget) -> TestExecution {
    on_small_stack(move || {
        run_body(vec![count_down(BASE), count_body(transfers)], budget).unwrap()
    })
}

/// Row "`return await` in tail position": 1,000 and 100,000 tail awaits
/// each transfer once, and hold the same activations, continuations, and
/// live memory: a transfer adds no continuation, no frame, and no retained
/// value.
#[test]
fn return_await_in_tail_position_reuses_the_activation() {
    let short = tail_awaits(1_000, MemoryBudget::DEFAULT);
    let long = tail_awaits(100_000, MemoryBudget::DEFAULT);
    assert_eq!(short.failure, None);
    assert_eq!(long.failure, None);
    let (short, long) = (short.instrumentation, long.instrumentation);
    assert_eq!(short.tail_transfer_count, 1_000);
    assert_eq!(long.tail_transfer_count, 100_000);
    // The body and one activation of `count`.
    assert_eq!(short.max_activation_depth, 2);
    assert_eq!(short.max_activation_depth, long.max_activation_depth);
    assert_eq!(short.max_continuation_depth, long.max_continuation_depth);
    assert!(short.peak_live_bytes > 0);
    assert_eq!(short.peak_live_bytes, long.peak_live_bytes);
    assert_eq!(short.invocation_count, 1_002);
    assert_eq!(long.invocation_count, 100_002);
}

/// Row "tail await under a small budget": the 100,000 run fits twice the
/// live memory of the 1,000 run.
#[test]
fn tail_awaits_fit_a_small_budget() {
    let short = tail_awaits(1_000, MemoryBudget::DEFAULT);
    let budget = MemoryBudget::new(2 * short.instrumentation.peak_live_bytes);
    let long = tail_awaits(100_000, budget);
    assert_eq!(long.failure, None);
    assert_eq!(long.instrumentation.tail_transfer_count, 100_000);
}

/// Row "a tail await has its parent": the body (1) awaits `p` (2), not in
/// tail position; `p` tail-awaits `q` (3); `q` tail-awaits `r` (4). The
/// probe reads the invocation in `r`, and in the body after the `await`.
#[test]
fn a_tail_await_has_its_parent() {
    const PROBE: usize = BASE;
    const R: usize = BASE + 1;
    const Q: usize = BASE + 2;
    const P: usize = BASE + 3;
    let probe = function("probe", signature(Vec::new(), Type::Void), void(), 0);
    let r = procedure(
        "r",
        Vec::new(),
        Type::Void,
        seq(vec![call(PROBE, Vec::new()), ret(void())]),
        0,
    );
    let q = procedure(
        "q",
        Vec::new(),
        Type::Void,
        ret(await_(call(R, Vec::new()))),
        0,
    );
    let p = procedure(
        "p",
        Vec::new(),
        Type::Void,
        ret(await_(call(Q, Vec::new()))),
        0,
    );
    let body = test_body(
        seq(vec![
            await_(call(P, Vec::new())),
            call(PROBE, Vec::new()),
            void(),
        ]),
        0,
    );
    let entry = BASE + 4;
    let set = set_with(vec![probe, r, q, p, body], Vec::new(), Vec::new());
    let (execution, probed) =
        run_test_probing(&program(set, entry), MemoryBudget::DEFAULT, PROBE).unwrap();
    assert_eq!(execution.failure, None);
    assert_eq!(
        probed,
        vec![
            Invocation {
                ordinal: 4,
                parent: Some(3),
            },
            Invocation {
                ordinal: 1,
                parent: None,
            },
        ]
    );
    assert_eq!(execution.instrumentation.invocation_count, 4);
    assert_eq!(execution.instrumentation.tail_transfer_count, 2);
}

/// Row "non-tail await depth": `depth(n) = if n == 0 then 0 else 1 + await
/// depth(n - 1)`, 100,000 deep, on a 256 KiB host thread: every await is
/// a continuation, and none is a host stack frame.
#[test]
fn non_tail_awaits_100_000_deep_run_on_a_small_stack() {
    const DEPTH: u64 = 100_000;
    let execution = on_small_stack(|| {
        let n = || var(0, Type::U64);
        let depth = procedure(
            "depth",
            vec![Type::U64],
            Type::U64,
            if_else(
                u64_equal(n(), u64_lit(0)),
                ret(u64_lit(0)),
                ret(u64_add(
                    u64_lit(1),
                    await_(call(BASE, vec![u64_sub(n(), u64_lit(1))])),
                )),
            ),
            1,
        );
        let body = test_body(
            seq(vec![expect_equal(
                Type::U64,
                u64_lit(DEPTH),
                await_(call(BASE, vec![u64_lit(DEPTH)])),
            )]),
            0,
        );
        run_body(vec![depth, body], MemoryBudget::DEFAULT).unwrap()
    });
    assert_eq!(execution.failure, None);
    let stats = execution.instrumentation;
    assert_eq!(stats.tail_transfer_count, 0);
    assert_eq!(stats.invocation_count, DEPTH + 2);
    // The body and `depth` at 100,000 down to 0.
    assert_eq!(stats.max_activation_depth, 100_002);
    assert!(stats.max_continuation_depth > 100_000);
}

/// Row "unbounded await recursion": the memory host event.
#[test]
fn unbounded_await_recursion_exhausts_memory() {
    let budget = MemoryBudget::new(8 * 1024 * 1024);
    let error = on_small_stack(move || {
        let body = test_body(seq(vec![await_(call(BASE, Vec::new())), void()]), 0);
        run_body(vec![spin(BASE), body], budget).unwrap_err()
    });
    assert_eq!(error, RuntimeError::MemoryExhausted { budget });
    assert!(error.is_host_event());
    assert_eq!(error.code(), DiagnosticCode::RuntimeMemoryExhausted);
}

/// `ATK-interp-await-in-pure-frame`: the checked IR keeps every `await` in
/// a procedure context, so the only `await` the interpreter can be handed
/// outside one is the root of a compile-time evaluation, which runs in an
/// activation with no procedure context. It traps as an invalid checked
/// program instead of running `inc`.
#[test]
fn atk_interp_await_in_pure_frame() {
    let set = set_with(vec![inc("inc")], Vec::new(), Vec::new());
    let awaited = await_(call(BASE, vec![int(1)]));
    assert_eq!(
        evaluate(&set, &awaited.expr, &[], LIMITS),
        Evaluation::Defect(Trap::InvalidCheckedProgram)
    );
    // Building the plan is pure, and an evaluation may do it: only its
    // observation fails, for a plan has no encoding.
    let built = call(BASE, vec![int(1)]);
    assert_eq!(
        evaluate(&set, &built.expr, &[], LIMITS),
        Evaluation::Defect(Trap::InvalidCheckedProgram)
    );
}

/// Row "a plan as a result": the entry returns a plan, which has no value
/// encoding.
#[test]
fn a_plan_as_a_result_has_no_observation() {
    let main = function(
        "main",
        signature(Vec::new(), intent(Type::I32)),
        call(BASE, vec![int(1)]),
        0,
    );
    let set = set_with(vec![inc("inc"), main], Vec::new(), Vec::new());
    let error = run(&program(set, BASE + 1), MemoryBudget::DEFAULT).unwrap_err();
    assert_eq!(
        error,
        RuntimeError::UnsupportedValue {
            value_type: intent(Type::I32),
        }
    );
    assert_eq!(error.code(), DiagnosticCode::ToolUnsupportedValue);
}

/// The same behind an interface value: the static type did not show it,
/// and the observation is the explicit encoding error.
#[test]
fn a_plan_behind_an_interface_value_has_no_observation() {
    let main = function(
        "main",
        signature(Vec::new(), Type::Any),
        node(Expr::WidenToInterface {
            value: Box::new(call(BASE, vec![int(1)])),
            value_type: Type::Any,
        }),
        0,
    );
    let set = set_with(vec![inc("inc"), main], Vec::new(), Vec::new());
    let error = run(&program(set, BASE + 1), MemoryBudget::DEFAULT).unwrap_err();
    assert_eq!(
        error,
        RuntimeError::UnsupportedValue {
            value_type: Type::Any,
        }
    );
}

// ----- memory -----

/// An entry that holds a list of `count` live plans of `inc`, then
/// returns.
fn plans_program(count: usize) -> vibra_ir::CheckedProgram {
    let plans = construct(
        Type::list(intent(Type::I32)),
        Construction::List((0..count).map(|_| call(BASE, vec![int(1)])).collect()),
    );
    let main = function(
        "main",
        signature(Vec::new(), Type::Void),
        let_slot(0, plans, void()),
        1,
    );
    program(
        set_with(vec![inc("inc"), main], Vec::new(), Vec::new()),
        BASE + 1,
    )
}

/// The least budget under which `program` completes. Completion is
/// monotone in the budget: exhaustion is decided on the live bytes at the
/// same points of a deterministic run.
fn least_budget(program: &vibra_ir::CheckedProgram) -> u64 {
    let completes = |bytes: u64| run(program, MemoryBudget::new(bytes)).is_ok();
    let (mut low, mut high) = (0_u64, MemoryBudget::DEFAULT.bytes);
    assert!(completes(high));
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        if completes(middle) {
            high = middle;
        } else {
            low = middle;
        }
    }
    high
}

/// Row "plan memory is accounted": 1,000 live plans and 1,010 have
/// different footprints, and a budget between the two completes the
/// smaller and exhausts the larger.
#[test]
fn plan_memory_is_accounted() {
    let (fewer, more) = (plans_program(1_000), plans_program(1_010));
    let (small, large) = (least_budget(&fewer), least_budget(&more));
    assert!(small < large, "{small} {large}");
    let budget = MemoryBudget::new(small + (large - small) / 2);
    assert!(run(&fewer, budget).is_ok());
    assert_eq!(
        run(&more, budget).unwrap_err(),
        RuntimeError::MemoryExhausted { budget }
    );
}
