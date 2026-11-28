//! Tail calls and activation depth (`docs/spec/06-runtime.md`, "Tail calls"
//! and "Activations and memory").
//!
//! Every test here runs on a thread with a 256 KiB stack: the interpreter
//! must not consume host stack in proportion to the depth of Vibra calls.

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_interp::{Execution, MemoryBudget, RuntimeError, run};
use vibra_ir::{
    CallTarget, CheckedFunction, CheckedModuleSet, Construction, ContractCall,
    ExitGuard, Expr, Guard, Implementation, ImplementationRecord, InterfaceDefinition,
    InterfaceKey, InterfaceMember, ModuleSetParts, NamedTypes, Node, SignatureRecord,
    Type, Value,
};

use crate::{
    BASE, base_parts, call, call_value, closure, construct, direct, fn_type, function,
    id, if_else, node, on_small_stack, program, set_with, signature, tail, u64_add,
    u64_equal, u64_lit, u64_sub, var,
};

fn n() -> Node {
    var(0, Type::U64)
}

fn u64_fn() -> Type {
    fn_type(vec![Type::U64], Type::U64)
}

/// `if n == 0 then 0 else <recur>(n - 1)`.
fn count_down(recur: impl FnOnce(Node) -> Node) -> Node {
    if_else(
        u64_equal(n(), u64_lit(0)),
        u64_lit(0),
        recur(u64_sub(n(), u64_lit(1))),
    )
}

fn main_function(body: Node) -> CheckedFunction {
    function("main", signature(Vec::new(), Type::U64), body, 0)
}

fn run_parts(
    parts: ModuleSetParts,
    entry: usize,
    budget: MemoryBudget,
) -> Result<Execution, RuntimeError> {
    let set = CheckedModuleSet::try_new(parts).unwrap();
    run(&program(set, entry), budget)
}

fn run_functions(
    functions: Vec<CheckedFunction>,
    entry: usize,
    budget: MemoryBudget,
) -> Result<Execution, RuntimeError> {
    let set = set_with(functions, Vec::new(), Vec::new());
    run(&program(set, entry), budget)
}

/// A chain of 1,000,000 activations: `main` tail-calls `count(999_999)`,
/// which tail-calls itself down to `count(1)`. Each of the 999,999 calls
/// replaces its caller, so one activation is alive at a time.
#[test]
fn a_million_tail_calls_run_in_one_activation() {
    let execution = on_small_stack(|| {
        let count = function(
            "count",
            signature(vec![Type::U64], Type::U64),
            if_else(
                u64_equal(n(), u64_lit(1)),
                u64_lit(0),
                tail(direct(BASE), vec![u64_sub(n(), u64_lit(1))]),
            ),
            1,
        );
        let main = main_function(tail(direct(BASE), vec![u64_lit(999_999)]));
        run_functions(vec![count, main], BASE + 1, MemoryBudget::DEFAULT).unwrap()
    });
    assert_eq!(execution.value.value(), &Value::U64(0));
    assert_eq!(execution.instrumentation.max_activation_depth, 1);
    assert_eq!(execution.instrumentation.tail_transfer_count, 999_999);
}

const INDIRECT: u64 = 100_000;

/// `viaClosure(n)`: tail-calls a closure, which tail-calls `viaClosure`.
#[test]
fn a_tail_call_through_a_closure_replaces_the_activation() {
    let execution = on_small_stack(|| {
        let bounce = closure(
            u64_fn(),
            1,
            Vec::new(),
            tail(direct(BASE), vec![var(0, Type::U64)]),
        );
        let via = function(
            "viaClosure",
            signature(vec![Type::U64], Type::U64),
            count_down(|next| tail(CallTarget::Value(Box::new(bounce)), vec![next])),
            1,
        );
        let main = main_function(tail(direct(BASE), vec![u64_lit(INDIRECT)]));
        run_functions(vec![via, main], BASE + 1, MemoryBudget::DEFAULT).unwrap()
    });
    assert_eq!(execution.value.value(), &Value::U64(0));
    assert_eq!(execution.instrumentation.max_activation_depth, 1);
    assert_eq!(
        execution.instrumentation.tail_transfer_count,
        2 * INDIRECT + 1
    );
}

/// `viaField(n)`: tail-calls the function held in a record's field.
#[test]
fn a_tail_call_of_a_function_held_in_a_field_replaces_the_activation() {
    let execution = on_small_stack(|| {
        let record_type = Type::record([("step".to_owned(), u64_fn())]).unwrap();
        let record = construct(
            record_type,
            Construction::Record(vec![(
                "step".to_owned(),
                node(Expr::Function {
                    function: BASE,
                    type_arguments: Vec::new(),
                }),
            )]),
        );
        let field = node(Expr::Field {
            record: Box::new(record),
            field: "step".to_owned(),
        });
        let via = function(
            "viaField",
            signature(vec![Type::U64], Type::U64),
            count_down(|next| tail(CallTarget::Value(Box::new(field)), vec![next])),
            1,
        );
        let main = main_function(tail(direct(BASE), vec![u64_lit(INDIRECT)]));
        run_functions(vec![via, main], BASE + 1, MemoryBudget::DEFAULT).unwrap()
    });
    assert_eq!(execution.value.value(), &Value::U64(0));
    assert_eq!(execution.instrumentation.max_activation_depth, 1);
    assert_eq!(execution.instrumentation.tail_transfer_count, INDIRECT + 1);
}

fn step_id() -> vibra_ir::TypeId {
    id("test.step.Step")
}

/// `Step`, with its member `step: Fn({Self} U64)`.
fn step_interface() -> InterfaceDefinition {
    let member = SignatureRecord::interface_member(
        Vec::new(),
        Some(0),
        fn_type(vec![Type::SelfType], Type::U64),
        Vec::new(),
    )
    .unwrap();
    InterfaceDefinition::new(
        step_id(),
        Vec::new(),
        Vec::new(),
        vec![InterfaceMember {
            name: "step".to_owned(),
            signature: member,
            default: None,
        }],
    )
    .unwrap()
}

fn step_call(receiver: Type) -> CallTarget {
    CallTarget::Contract(Box::new(ContractCall {
        interface: step_id(),
        member: "step".to_owned(),
        receiver,
        arguments: Vec::new(),
        choices: NamedTypes::empty(),
        member_arguments: Vec::new(),
    }))
}

/// A set where `Step` is implemented for `U64` by `member`, which is
/// function `BASE`, and `main` is function `BASE + 1`.
fn step_parts(member: Node, main: Node) -> ModuleSetParts {
    let mut parts = base_parts();
    parts.interfaces.push(step_interface());
    parts.functions.push(function(
        "step",
        signature(vec![Type::U64], Type::U64),
        member,
        1,
    ));
    parts.functions.push(main_function(main));
    parts.implementations.push(
        Implementation::new(
            ImplementationRecord::new(
                Vec::new(),
                InterfaceKey::new(step_id(), Vec::new()),
                Type::U64,
                NamedTypes::empty(),
            )
            .unwrap(),
            vec![("step".to_owned(), BASE)],
        )
        .unwrap(),
    );
    parts
}

/// The member `Step.step` on `U64` tail-calls `Step.step`, dispatched
/// statically.
#[test]
fn a_tail_call_of_a_contract_member_replaces_the_activation() {
    let execution = on_small_stack(|| {
        let member = count_down(|next| tail(step_call(Type::U64), vec![next]));
        let main = tail(step_call(Type::U64), vec![u64_lit(INDIRECT)]);
        run_parts(step_parts(member, main), BASE + 1, MemoryBudget::DEFAULT).unwrap()
    });
    assert_eq!(execution.value.value(), &Value::U64(0));
    assert_eq!(execution.instrumentation.max_activation_depth, 1);
    assert_eq!(execution.instrumentation.tail_transfer_count, INDIRECT + 1);
}

/// The same member reached through an interface value: the call unboxes
/// the receiver, and a result without `Self` leaves nothing to rebox, so
/// the call still replaces the activation.
#[test]
fn a_tail_call_through_an_interface_value_replaces_the_activation() {
    let execution = on_small_stack(|| {
        let boxed = Type::Interface(step_id(), Vec::new(), NamedTypes::empty());
        let widen = |value: Node| {
            node(Expr::WidenToInterface {
                value: Box::new(value),
                value_type: boxed.clone(),
            })
        };
        let member =
            count_down(|next| tail(step_call(boxed.clone()), vec![widen(next)]));
        let main = tail(step_call(boxed.clone()), vec![widen(u64_lit(INDIRECT))]);
        run_parts(step_parts(member, main), BASE + 1, MemoryBudget::DEFAULT).unwrap()
    });
    assert_eq!(execution.value.value(), &Value::U64(0));
    assert_eq!(execution.instrumentation.max_activation_depth, 1);
    assert_eq!(execution.instrumentation.tail_transfer_count, INDIRECT + 1);
}

/// A pending `ensures result == 1 else 7` is work after the return: the
/// written tail call is an ordinary call, and every activation stays alive.
/// The innermost candidate, 0, fails, so its consequence runs; each caller's
/// candidate is then 7, which fails in turn.
#[test]
fn a_pending_ensures_keeps_a_call_out_of_tail_position() {
    let execution = on_small_stack(|| {
        let checked = CheckedFunction {
            exit_guard: Some(ExitGuard {
                candidate: 1,
                guard: Guard {
                    predicate: u64_equal(var(1, Type::U64), u64_lit(1)),
                    consequence: u64_lit(7),
                },
            }),
            ..function(
                "checked",
                signature(vec![Type::U64], Type::U64),
                count_down(|next| tail(direct(BASE), vec![next])),
                2,
            )
        };
        let main = main_function(call(BASE, vec![u64_lit(100)]));
        run_functions(vec![checked, main], BASE + 1, MemoryBudget::DEFAULT).unwrap()
    });
    assert_eq!(execution.value.value(), &Value::U64(7));
    // `main`, then `checked` at 100 down to 0.
    assert_eq!(execution.instrumentation.max_activation_depth, 102);
    assert_eq!(execution.instrumentation.tail_transfer_count, 0);
}

/// A `try` around a call inspects the carrier after it: no transfer.
#[test]
fn a_try_keeps_a_call_out_of_tail_position() {
    let execution = on_small_stack(|| {
        let option_u64 = crate::option(Type::U64);
        let body = if_else(
            u64_equal(n(), u64_lit(0)),
            crate::some(Type::U64, u64_lit(0)),
            crate::some(
                Type::U64,
                node(Expr::Try(Box::new(tail(
                    direct(BASE),
                    vec![u64_sub(n(), u64_lit(1))],
                )))),
            ),
        );
        let tried = function(
            "tried",
            signature(vec![Type::U64], option_u64.clone()),
            body,
            1,
        );
        let main = function(
            "main",
            signature(Vec::new(), option_u64),
            call(BASE, vec![u64_lit(50)]),
            0,
        );
        run_functions(vec![tried, main], BASE + 1, MemoryBudget::DEFAULT).unwrap()
    });
    assert_eq!(execution.instrumentation.tail_transfer_count, 0);
    assert_eq!(execution.instrumentation.max_activation_depth, 52);
}

/// Recursion with work after the call, 100,000 deep, under 64 MiB.
#[test]
fn non_tail_recursion_100_000_deep_fits_64_mib() {
    let execution = on_small_stack(|| {
        let depth = crate::evaluation::depth_function(BASE);
        let main = main_function(call(BASE, vec![u64_lit(100_000)]));
        run_functions(
            vec![depth, main],
            BASE + 1,
            MemoryBudget::new(64 * 1024 * 1024),
        )
        .unwrap()
    });
    assert_eq!(execution.value.value(), &Value::U64(100_000));
    assert_eq!(execution.instrumentation.max_activation_depth, 100_002);
    assert!(execution.instrumentation.peak_memory_bytes <= 64 * 1024 * 1024);
}

/// Unbounded recursion exhausts the budget: the host event
/// `@runtime.memoryExhausted`, at `0..0` with no source identity.
#[test]
fn unbounded_recursion_is_memory_exhaustion_a_host_event() {
    let budget = MemoryBudget::new(8 * 1024 * 1024);
    let error = on_small_stack(move || {
        let forever = function(
            "forever",
            signature(vec![Type::U64], Type::U64),
            u64_add(u64_lit(1), call(BASE, vec![n()])),
            1,
        );
        let main = main_function(call(BASE, vec![u64_lit(0)]));
        run_functions(vec![forever, main], BASE + 1, budget).unwrap_err()
    });
    assert_eq!(error, RuntimeError::MemoryExhausted { budget });
    assert!(error.is_host_event());
    assert_eq!(error.code(), DiagnosticCode::RuntimeMemoryExhausted);
    let diagnostic = error.diagnostic();
    assert_eq!(diagnostic.primary_span(), ByteSpan::new(0, 0));
    assert_eq!(diagnostic.source_id(), None);
}

/// A call of a closure value that is not in tail position creates an
/// activation, and returns to its caller.
#[test]
fn an_ordinary_call_of_a_closure_returns_to_its_caller() {
    let increment = closure(
        u64_fn(),
        1,
        Vec::new(),
        u64_add(var(0, Type::U64), u64_lit(1)),
    );
    let main = main_function(u64_add(
        call_value(increment, vec![u64_lit(1)]),
        u64_lit(10),
    ));
    let execution = run_functions(vec![main], BASE, MemoryBudget::DEFAULT).unwrap();
    assert_eq!(execution.value.value(), &Value::U64(12));
    assert_eq!(execution.instrumentation.max_activation_depth, 2);
}
