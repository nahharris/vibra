//! Evaluation limits, dispatch, and the forms this interpreter does not run
//! (`docs/spec/06-runtime.md`, "Generic instantiation", "Activations and
//! memory"; `docs/spec/02-type-system.md`, "Interface values").

use vibra_diagnostics::DiagnosticCode;
use vibra_interp::{
    Evaluation, Limit, Limits, MemoryBudget, RuntimeError, evaluate, run,
};
use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::{
    CallTarget, CheckedFunction, CheckedModuleSet, ContractCall, Expr, Implementation,
    ImplementationRecord, InterfaceDefinition, InterfaceKey, InterfaceMember,
    ModuleSetParts, NamedTypes, Node, ProcType, Row, SignatureRecord, Type, Value,
};

use crate::{
    BASE, SEED, base_parts, call, direct, fn_type, function, id, if_else, int, lit,
    node, program, row, set_with, signature, str_type, tail, text, text_value, u64_add,
    u64_equal, u64_lit, u64_sub, var,
};

/// `spin(n)`: counts down to zero by tail calls. A reduction is one node:
/// a step at `n > 0` evaluates eight (the `if`, the `==` and its two
/// operands, the tail call, the `-` and its two operands), and the last
/// step five (the `if`, the `==` and its operands, and `0`).
fn spin() -> CheckedFunction {
    let n = || var(0, Type::U64);
    function(
        "spin",
        signature(vec![Type::U64], Type::U64),
        if_else(
            u64_equal(n(), u64_lit(0)),
            u64_lit(0),
            tail(direct(BASE), vec![u64_sub(n(), u64_lit(1))]),
        ),
        1,
    )
}

/// A loop of 10,007 reductions (the call and its operand, 1,250 steps of
/// eight, and the last of five) reaches a limit of 100 and finishes under a
/// limit of 100,000; its exact count is the boundary.
#[test]
fn the_reduction_count_is_the_evaluation_limit() {
    let set = set_with(vec![spin()], Vec::new(), Vec::new());
    let expr = call(BASE, vec![u64_lit(1_250)]).expr;
    let under = |reductions: u64| {
        evaluate(
            &set,
            &expr,
            &[],
            Limits {
                reductions,
                memory: MemoryBudget::DEFAULT,
            },
        )
    };
    assert_eq!(under(100), Evaluation::LimitReached(Limit::Reductions));
    assert_eq!(under(100_000), Evaluation::Value(Value::U64(0)));
    assert_eq!(under(10_007), Evaluation::Value(Value::U64(0)));
    assert_eq!(under(10_006), Evaluation::LimitReached(Limit::Reductions));
}

#[test]
fn an_evaluation_that_exhausts_its_memory_reaches_the_memory_limit() {
    let forever = function(
        "forever",
        signature(vec![Type::U64], Type::U64),
        u64_add(u64_lit(1), call(BASE, vec![var(0, Type::U64)])),
        1,
    );
    let set = set_with(vec![forever], Vec::new(), Vec::new());
    let outcome = evaluate(
        &set,
        &call(BASE, vec![u64_lit(0)]).expr,
        &[],
        Limits {
            reductions: u64::MAX,
            memory: MemoryBudget::new(1024 * 1024),
        },
    );
    assert_eq!(outcome, Evaluation::LimitReached(Limit::Memory));
}

fn describe_id() -> vibra_ir::TypeId {
    id("test.describe.Describe")
}

fn member(result: Type) -> SignatureRecord {
    SignatureRecord::interface_member(
        Vec::new(),
        Some(0),
        fn_type(vec![Type::SelfType], result),
        Vec::new(),
    )
    .unwrap()
}

fn contract(member: &str, receiver: Type) -> CallTarget {
    CallTarget::Contract(Box::new(ContractCall {
        interface: describe_id(),
        member: member.to_owned(),
        receiver,
        arguments: Vec::new(),
        choices: NamedTypes::empty(),
        member_arguments: Vec::new(),
    }))
}

fn contract_call(member: &str, receiver: Type, argument: Node) -> Node {
    node(Expr::Call {
        target: contract(member, receiver),
        arguments: crate::positional(vec![argument]),
    })
}

fn implementation(receiver: Type, members: Vec<(&str, usize)>) -> Implementation {
    Implementation::new(
        ImplementationRecord::new(
            Vec::new(),
            InterfaceKey::new(describe_id(), Vec::new()),
            receiver,
            NamedTypes::empty(),
        )
        .unwrap(),
        members
            .into_iter()
            .map(|(name, function)| (name.to_owned(), function))
            .collect(),
    )
    .unwrap()
}

/// `Describe`: `describe(Self) Str`, and `twice(Self) Str`, whose default
/// concatenates two `describe`s of its receiver.
///
/// Functions: `BASE` describes `I32`, `BASE + 1` describes `U64`, and
/// `BASE + 2` is the default of `twice`, which neither overrides.
fn describe_parts() -> ModuleSetParts {
    let describe = |name: &str, receiver: Type, answer: &str| {
        function(name, signature(vec![receiver], str_type()), text(answer), 1)
    };
    let twice_body = row(
        CompilerIntrinsic::StrConcat,
        vec![
            contract_call("describe", Type::SelfType, var(0, Type::SelfType)),
            contract_call("describe", Type::SelfType, var(0, Type::SelfType)),
        ],
    );
    let twice = function("twice", member(str_type()), twice_body, 1);
    let interface = InterfaceDefinition::new(
        describe_id(),
        Vec::new(),
        Vec::new(),
        vec![
            InterfaceMember {
                name: "describe".to_owned(),
                signature: member(str_type()),
                default: None,
            },
            InterfaceMember {
                name: "twice".to_owned(),
                signature: member(str_type()),
                default: Some(BASE + 2),
            },
        ],
    )
    .unwrap();
    let mut parts = base_parts();
    parts.interfaces.push(interface);
    parts.functions.extend([
        describe("describeI32", Type::I32, "int"),
        describe("describeU64", Type::U64, "unsigned"),
        twice,
    ]);
    parts.implementations.extend([
        implementation(Type::I32, vec![("describe", BASE)]),
        implementation(Type::U64, vec![("describe", BASE + 1)]),
    ]);
    parts
}

fn describe_set() -> std::sync::Arc<CheckedModuleSet> {
    CheckedModuleSet::try_new(describe_parts()).unwrap()
}

/// Two implementations of one interface: each receiver type selects its
/// own.
#[test]
fn dispatch_selects_the_implementation_of_the_receivers_type() {
    let set = describe_set();
    assert_eq!(
        crate::eval(&set, &contract_call("describe", Type::I32, int(5))),
        Evaluation::Value(text_value("int"))
    );
    assert_eq!(
        crate::eval(&set, &contract_call("describe", Type::U64, u64_lit(5))),
        Evaluation::Value(text_value("unsigned"))
    );
}

fn boxed() -> Type {
    Type::Interface(describe_id(), Vec::new(), NamedTypes::empty())
}

fn widen(value: Node) -> Node {
    node(Expr::WidenToInterface {
        value: Box::new(value),
        value_type: boxed(),
    })
}

/// Through an interface value, the held value's implementation runs.
#[test]
fn an_interface_value_carries_its_implementation() {
    let set = describe_set();
    assert_eq!(
        crate::eval(&set, &contract_call("describe", boxed(), widen(int(5)))),
        Evaluation::Value(text_value("int"))
    );
    assert_eq!(
        crate::eval(&set, &contract_call("describe", boxed(), widen(u64_lit(5)))),
        Evaluation::Value(text_value("unsigned"))
    );
}

/// A default member runs with its call's receiver: its own contract calls
/// on `Self` dispatch to the receiver's implementation.
#[test]
fn a_default_member_runs_with_its_calls_receiver() {
    let set = describe_set();
    assert_eq!(
        crate::eval(&set, &contract_call("twice", Type::I32, int(5))),
        Evaluation::Value(text_value("intint"))
    );
    assert_eq!(
        crate::eval(&set, &contract_call("twice", boxed(), widen(u64_lit(5)))),
        Evaluation::Value(text_value("unsignedunsigned"))
    );
}

fn counter_id() -> vibra_ir::TypeId {
    id("test.counter.Counter")
}

/// `Counter`: `down(Self) Self`, implemented for `U64` by function `BASE`.
fn counter_parts(main: Node) -> ModuleSetParts {
    let interface = InterfaceDefinition::new(
        counter_id(),
        Vec::new(),
        Vec::new(),
        vec![InterfaceMember {
            name: "down".to_owned(),
            signature: member(Type::SelfType),
            default: None,
        }],
    )
    .unwrap();
    let down = function(
        "down",
        signature(vec![Type::U64], Type::U64),
        u64_sub(var(0, Type::U64), u64_lit(1)),
        1,
    );
    let counter = Type::Interface(counter_id(), Vec::new(), NamedTypes::empty());
    let mut parts = base_parts();
    parts.interfaces.push(interface);
    parts.functions.extend([
        down,
        function("main", signature(Vec::new(), counter), main, 0),
    ]);
    parts.implementations.push(
        Implementation::new(
            ImplementationRecord::new(
                Vec::new(),
                InterfaceKey::new(counter_id(), Vec::new()),
                Type::U64,
                NamedTypes::empty(),
            )
            .unwrap(),
            vec![("down".to_owned(), BASE)],
        )
        .unwrap(),
    );
    parts
}

/// A `Self` in the member's result is reboxed with the same implementation:
/// `Counter.down` through a `Counter` gives a `Counter`.
#[test]
fn a_self_result_is_reboxed() {
    let counter = Type::Interface(counter_id(), Vec::new(), NamedTypes::empty());
    let main = node(Expr::Call {
        target: CallTarget::Contract(Box::new(ContractCall {
            interface: counter_id(),
            member: "down".to_owned(),
            receiver: counter.clone(),
            arguments: Vec::new(),
            choices: NamedTypes::empty(),
            member_arguments: Vec::new(),
        })),
        arguments: crate::positional(vec![node(Expr::WidenToInterface {
            value: Box::new(u64_lit(5)),
            value_type: counter,
        })]),
    });
    let set = CheckedModuleSet::try_new(counter_parts(main)).unwrap();
    let execution = run(&program(set, BASE + 1), MemoryBudget::DEFAULT).unwrap();
    let ty = format!(
        "{{kind: @interface path: @test.counter.Counter seed: \"{SEED}\" arguments: [] choices: [:]}}"
    );
    assert_eq!(
        execution.canonical,
        format!(
            "{{format: @value.v2 typeInfo: {ty} value: \
             {{kind: @interface held: {{format: @value.v2 typeInfo: @u64 value: 4u64}}}}}}"
        )
    );
}

/// Applying a `proc` builds a plan, which is Milestone 2's: the run is
/// `@tool.unavailable`, and so is a `proc` entry.
#[test]
fn a_proc_is_unavailable() {
    let proc_type = Type::Proc(Box::new(ProcType {
        parameters: Vec::new(),
        labels: NamedTypes::empty(),
        result: Type::Void,
        effects: Row::empty(),
    }));
    let task = function(
        "task",
        SignatureRecord::new(Vec::new(), proc_type).unwrap(),
        lit(Type::Void, Value::Void),
        0,
    );
    let set = set_with(vec![task], Vec::new(), Vec::new());
    let error = run(&program(set, BASE), MemoryBudget::DEFAULT).unwrap_err();
    assert!(matches!(error, RuntimeError::Unavailable { .. }));
    assert_eq!(error.code(), DiagnosticCode::ToolUnavailable);
}
