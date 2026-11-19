//! Evaluation (`docs/spec/06-runtime.md`, "Evaluation", "Module-value
//! initialization", and "Callable contract execution").

use vibra_interp::{Evaluation, MemoryBudget, run};
use vibra_ir::external::{CompilerIntrinsic, IntegerOp, IntegerType};
use vibra_ir::{
    Arguments, CallTarget, CheckedFunction, Constant, Construction, ExitGuard, Expr,
    FnType, Guard, Invariant, MatchArm, NamedTypes, Node, Pattern, Trap, Type,
    TypeBody, TypeDefinition, Value, Visibility,
};

use crate::{
    BASE, bool_type, boolean, call, construct, direct, eval, function, global, id,
    if_else, int, integer, node, none, option, origin_at, program, row, set_with,
    signature, some, some_value, text, text_value, u64_equal, u64_lit, u64_sub, var,
    variant_value,
};

/// An assertion, which traps outside a test, written at `start..start + 1`
/// so that the trap names which operand ran.
fn trap_at(start: usize) -> Node {
    let mut trap = row(CompilerIntrinsic::AssertTrue, vec![boolean(true)]);
    trap.origin = origin_at(start, start + 1);
    trap
}

fn sequence(statements: Vec<Node>) -> Node {
    node(Expr::Sequence(statements))
}

fn i32_sub(left: Node, right: Node) -> Node {
    integer(IntegerType::I32, IntegerOp::Sub, vec![left, right])
}

fn i32_equal(left: Node, right: Node) -> Node {
    integer(IntegerType::I32, IntegerOp::Equal, vec![left, right])
}

/// `Fn({I32} b: I32 a: I32 … I32)`: one positional parameter and the labels
/// `a` and `b`, which take slots 1 and 2 in name order.
fn labelled_type() -> Type {
    Type::function(FnType {
        parameters: vec![Type::I32],
        labels: NamedTypes::new([
            ("a".to_owned(), Type::I32),
            ("b".to_owned(), Type::I32),
        ])
        .unwrap(),
        result: Type::I32,
    })
}

/// `callee(positional b: labelled_b a: labelled_a)`, the labels written
/// out of name order. The callee computes `a - b`.
fn labelled_call(
    callee_trap: bool,
    positional_trap: bool,
    labelled_trap: bool,
) -> Node {
    let body = i32_sub(var(1, Type::I32), var(2, Type::I32));
    let zero = |label: &str| {
        (
            label.to_owned(),
            Constant::new(Type::I32, Value::I32(0)).unwrap(),
        )
    };
    let callee = node(Expr::Closure {
        closure: std::sync::Arc::new(vibra_ir::ClosureBody {
            value_type: labelled_type(),
            slot_count: 3,
            captures: Vec::new(),
            defaults: vec![zero("a"), zero("b")],
            body,
        }),
        captures: Vec::new(),
    });
    let operand = |trap: bool, start: usize, value: i32| {
        if trap {
            sequence(vec![trap_at(start), int(value)])
        } else {
            int(value)
        }
    };
    let callee = if callee_trap {
        sequence(vec![trap_at(1), callee])
    } else {
        callee
    };
    node(Expr::Call {
        target: CallTarget::Value(Box::new(callee)),
        arguments: Arguments {
            positional: vec![operand(positional_trap, 2, 0)],
            labelled: vec![
                ("b".to_owned(), operand(labelled_trap, 3, 1)),
                ("a".to_owned(), operand(labelled_trap, 4, 10)),
            ],
        },
    })
}

fn run_main(body: Node) -> Result<vibra_interp::Execution, vibra_interp::RuntimeError> {
    let set = set_with(
        vec![function("main", signature(Vec::new(), Type::I32), body, 0)],
        Vec::new(),
        Vec::new(),
    );
    run(&program(set, BASE), MemoryBudget::DEFAULT)
}

fn trap_start(body: Node) -> usize {
    let error = run_main(body).unwrap_err();
    let vibra_interp::RuntimeError::Trap { origin, trap, .. } = error else {
        panic!("a trap");
    };
    assert_eq!(trap, Trap::InvalidCheckedProgram);
    origin.unwrap().span().start()
}

/// The callee, then the positional arguments, then the labelled arguments
/// in written order; each label lands in its slot in name order.
#[test]
fn evaluation_order_is_callee_positional_then_labelled_as_written() {
    assert_eq!(trap_start(labelled_call(true, true, true)), 1);
    assert_eq!(trap_start(labelled_call(false, true, true)), 2);
    // `b` is written before `a`, so its operand runs first.
    assert_eq!(trap_start(labelled_call(false, false, true)), 3);
    let value = run_main(labelled_call(false, false, false)).unwrap();
    assert_eq!(value.value.value(), &Value::I32(9));
}

/// `f(x: Option(I32)) I32`: `let some(value: v) = x else return -1`, then
/// `v`.
fn unwrap_or_minus_one() -> CheckedFunction {
    let body = node(Expr::LetElse {
        pattern: Pattern::Case {
            case: "some".to_owned(),
            fields: vec![("value".to_owned(), Pattern::Bind(1))],
        },
        value: Box::new(var(0, option(Type::I32))),
        fallback: Box::new(node(Expr::Return(Box::new(int(-1))))),
        body: Box::new(var(1, Type::I32)),
    });
    function(
        "unwrapOrMinusOne",
        signature(vec![option(Type::I32)], Type::I32),
        body,
        2,
    )
}

/// When the pattern fails, only the fallback runs: the body after the `let`,
/// a trap here, does not.
#[test]
fn let_else_runs_only_the_fallback_when_the_pattern_fails() {
    let body = node(Expr::LetElse {
        pattern: Pattern::Case {
            case: "some".to_owned(),
            fields: vec![("value".to_owned(), Pattern::Bind(1))],
        },
        value: Box::new(var(0, option(Type::I32))),
        fallback: Box::new(node(Expr::Return(Box::new(int(-1))))),
        body: Box::new(sequence(vec![trap_at(1), var(1, Type::I32)])),
    });
    let guarded_body = function(
        "fallbackOnly",
        signature(vec![option(Type::I32)], Type::I32),
        body,
        2,
    );
    let set = set_with(vec![guarded_body], Vec::new(), Vec::new());
    assert_eq!(
        eval(&set, &call(BASE, vec![none(Type::I32)])),
        Evaluation::Value(Value::I32(-1))
    );
}

#[test]
fn let_else_binds_on_a_match_and_runs_only_the_fallback_otherwise() {
    let set = set_with(vec![unwrap_or_minus_one()], Vec::new(), Vec::new());
    assert_eq!(
        eval(&set, &call(BASE, vec![some(Type::I32, int(5))])),
        Evaluation::Value(Value::I32(5))
    );
    assert_eq!(
        eval(&set, &call(BASE, vec![none(Type::I32)])),
        Evaluation::Value(Value::I32(-1))
    );
}

/// The first arm that matches runs; a later arm that also matches does not,
/// and its body (a trap here) is never evaluated.
#[test]
fn match_runs_the_first_arm_that_matches() {
    let set = set_with(Vec::new(), Vec::new(), Vec::new());
    let arm = |pattern: Pattern, body: Node| MatchArm { pattern, body };
    let literal = |value: i32| {
        Pattern::Literal(Constant::new(Type::I32, Value::I32(value)).unwrap())
    };
    let subject = some(Type::I32, int(2));
    let matched = node(Expr::Match {
        subject: Box::new(subject),
        arms: vec![
            arm(
                Pattern::Case {
                    case: "some".to_owned(),
                    fields: vec![("value".to_owned(), literal(3))],
                },
                text("three"),
            ),
            arm(
                Pattern::Case {
                    case: "some".to_owned(),
                    fields: vec![("value".to_owned(), Pattern::Bind(0))],
                },
                text("some"),
            ),
            arm(Pattern::Discard, sequence(vec![trap_at(1), text("any")])),
        ],
        value_type: crate::str_type(),
    });
    assert_eq!(eval(&set, &matched), Evaluation::Value(text_value("some")));
}

/// `depth(n: U64) U64`: `0` at zero, else `1 + depth(n - 1)`, which is not a
/// tail call.
pub(crate) fn depth_function(index: usize) -> CheckedFunction {
    let n = || var(0, Type::U64);
    let body = if_else(
        u64_equal(n(), u64_lit(0)),
        u64_lit(0),
        crate::u64_add(u64_lit(1), call(index, vec![u64_sub(n(), u64_lit(1))])),
    );
    function("depth", signature(vec![Type::U64], Type::U64), body, 1)
}

/// A module value is evaluated once per execution, on its first read: a
/// second read costs only its own reduction.
#[test]
fn a_module_value_is_evaluated_once() {
    let runs = |reads: usize| {
        let depth = depth_function(BASE);
        let body = sequence((0..reads).map(|_| node(Expr::Global(0))).collect());
        let main = function("main", signature(Vec::new(), Type::U64), body, 0);
        let set = set_with(
            vec![depth, main],
            vec![global("deep", Type::U64, call(BASE, vec![u64_lit(1_000)]))],
            Vec::new(),
        );
        run(&program(set, BASE + 1), MemoryBudget::DEFAULT).unwrap()
    };
    let once = runs(1);
    let twice = runs(2);
    assert_eq!(once.value.value(), &Value::U64(1_000));
    assert!(once.instrumentation.reduction_count > 1_000);
    // Both bodies are sequences; the second only adds its second read.
    assert_eq!(
        twice.instrumentation.reduction_count,
        once.instrumentation.reduction_count + 1
    );
}

/// Reading a module value while it is being evaluated is the trap
/// `@runtime.invalidCheckedProgram`.
#[test]
fn reading_a_module_value_during_its_evaluation_traps() {
    let set = set_with(
        Vec::new(),
        vec![global(
            "loop",
            Type::I32,
            integer(
                IntegerType::I32,
                IntegerOp::Add,
                vec![node(Expr::Global(0)), int(1)],
            ),
        )],
        Vec::new(),
    );
    assert_eq!(
        eval(&set, &node(Expr::Global(0))),
        Evaluation::Defect(Trap::InvalidCheckedProgram)
    );
}

fn guarded(
    name: &str,
    body: Node,
    entry: Option<Guard>,
    exit: Option<ExitGuard>,
) -> CheckedFunction {
    CheckedFunction {
        entry_guard: entry,
        exit_guard: exit,
        ..function(name, signature(vec![Type::I32], Type::I32), body, 2)
    }
}

/// `requires x == 0 else -1`: a false condition gives the consequence and
/// skips both the body and the `ensures`, which would trap here.
#[test]
fn requires_else_skips_the_body_and_the_ensures() {
    let requires = || Guard {
        predicate: i32_equal(var(0, Type::I32), int(0)),
        consequence: int(-1),
    };
    let trapping_ensures = ExitGuard {
        candidate: 1,
        guard: Guard {
            predicate: sequence(vec![trap_at(1), boolean(true)]),
            consequence: int(0),
        },
    };
    let skipped = guarded(
        "skipped",
        sequence(vec![trap_at(2), int(7)]),
        Some(requires()),
        Some(trapping_ensures),
    );
    let admitted = guarded(
        "admitted",
        integer(
            IntegerType::I32,
            IntegerOp::Add,
            vec![var(0, Type::I32), int(100)],
        ),
        Some(requires()),
        None,
    );
    let set = set_with(vec![skipped, admitted], Vec::new(), Vec::new());
    assert_eq!(
        eval(&set, &call(BASE, vec![int(5)])),
        Evaluation::Value(Value::I32(-1))
    );
    assert_eq!(
        eval(&set, &call(BASE + 1, vec![int(0)])),
        Evaluation::Value(Value::I32(100))
    );
}

/// `ensures result == 4 else result + 100`: the check runs on the candidate
/// bound to the result name, the consequence sees the same binding, and its
/// value (106, which fails the check) is returned without a second check.
#[test]
fn ensures_else_checks_the_candidate_once_and_returns_the_consequence_unchecked() {
    let doubled = guarded(
        "doubled",
        integer(
            IntegerType::I32,
            IntegerOp::Mul,
            vec![var(0, Type::I32), int(2)],
        ),
        None,
        Some(ExitGuard {
            candidate: 1,
            guard: Guard {
                predicate: i32_equal(var(1, Type::I32), int(4)),
                consequence: integer(
                    IntegerType::I32,
                    IntegerOp::Add,
                    vec![var(1, Type::I32), int(100)],
                ),
            },
        }),
    );
    let set = set_with(vec![doubled], Vec::new(), Vec::new());
    assert_eq!(
        eval(&set, &call(BASE, vec![int(2)])),
        Evaluation::Value(Value::I32(4))
    );
    assert_eq!(
        eval(&set, &call(BASE, vec![int(3)])),
        Evaluation::Value(Value::I32(106))
    );
}

fn small_id() -> vibra_ir::TypeId {
    id("test.small.Small")
}

/// `Small` wraps `I32` `where x < 10`, with or without `else @tooBig`.
fn small(consequence: Option<Constant>, predicate: usize) -> TypeDefinition {
    TypeDefinition::new(
        small_id(),
        Vec::new(),
        TypeBody::Wrapper(Type::I32),
        Some(Invariant {
            predicate,
            consequence,
        }),
        Visibility::Public,
    )
    .unwrap()
}

fn below_ten() -> CheckedFunction {
    let order = integer(
        IntegerType::I32,
        IntegerOp::Compare,
        vec![var(0, Type::I32), int(10)],
    );
    let body = node(Expr::Match {
        subject: Box::new(order),
        arms: vec![
            MatchArm {
                pattern: Pattern::Case {
                    case: "less".to_owned(),
                    fields: Vec::new(),
                },
                body: boolean(true),
            },
            MatchArm {
                pattern: Pattern::Discard,
                body: boolean(false),
            },
        ],
        value_type: bool_type(),
    });
    function("belowTen", signature(vec![Type::I32], bool_type()), body, 1)
}

fn small_of(value: i32) -> Node {
    construct(
        Type::Declared(small_id(), Vec::new()),
        Construction::Wrapper(Box::new(int(value))),
    )
}

/// `where P else E` gives `Result.ok` of the value when `P` holds and
/// `Result.err(error: E)` otherwise.
#[test]
fn where_else_gives_a_result() {
    let too_big = Constant::new(Type::Atom, Value::Atom("tooBig".to_owned())).unwrap();
    let set = set_with(
        vec![below_ten()],
        Vec::new(),
        vec![small(Some(too_big), BASE)],
    );
    let wrapped = |value: i32| Value::Wrapper(Box::new(Value::I32(value)));
    assert_eq!(
        eval(&set, &small_of(5)),
        Evaluation::Value(variant_value("ok", vec![("value", wrapped(5))]))
    );
    assert_eq!(
        eval(&set, &small_of(50)),
        Evaluation::Value(variant_value(
            "err",
            vec![("error", Value::Atom("tooBig".to_owned()))]
        ))
    );
}

/// A bare `where` was discharged by the checker and leaves no run-time
/// check: the predicate, a trap here, never runs.
#[test]
fn a_bare_where_leaves_no_check() {
    let trapping = function(
        "trapping",
        signature(vec![Type::I32], bool_type()),
        sequence(vec![trap_at(1), boolean(true)]),
        1,
    );
    let set = set_with(vec![trapping], Vec::new(), vec![small(None, BASE)]);
    assert_eq!(
        eval(&set, &small_of(50)),
        Evaluation::Value(Value::Wrapper(Box::new(Value::I32(50))))
    );
    // The operands of a construction whose check was folded away still run.
    let trapping_operand = construct(
        Type::Declared(small_id(), Vec::new()),
        Construction::Wrapper(Box::new(sequence(vec![trap_at(2), int(1)]))),
    );
    assert_eq!(
        eval(&set, &trapping_operand),
        Evaluation::Defect(Trap::InvalidCheckedProgram)
    );
}

/// `try` unwraps `some` and leaves the callable with `none`.
#[test]
fn try_propagates_none_and_unwraps_some() {
    let body = some(
        Type::I32,
        integer(
            IntegerType::I32,
            IntegerOp::Add,
            vec![node(Expr::Try(Box::new(var(0, option(Type::I32))))), int(1)],
        ),
    );
    let increment = function(
        "increment",
        signature(vec![option(Type::I32)], option(Type::I32)),
        body,
        1,
    );
    let set = set_with(vec![increment], Vec::new(), Vec::new());
    assert_eq!(
        eval(&set, &call(BASE, vec![some(Type::I32, int(1))])),
        Evaluation::Value(some_value(Value::I32(2)))
    );
    assert_eq!(
        eval(&set, &call(BASE, vec![none(Type::I32)])),
        Evaluation::Value(crate::none_value())
    );
}

/// `try` unwraps `ok` and leaves the callable with `err`, its error
/// unchanged.
#[test]
fn try_propagates_err_and_unwraps_ok() {
    let carrier = || crate::result(Type::I32, crate::str_type());
    let case = |case: &str, field: &str, value: Node| {
        construct(
            carrier(),
            Construction::Case {
                case: case.to_owned(),
                fields: vec![(field.to_owned(), value)],
            },
        )
    };
    let body = case(
        "ok",
        "value",
        integer(
            IntegerType::I32,
            IntegerOp::Add,
            vec![node(Expr::Try(Box::new(var(0, carrier())))), int(1)],
        ),
    );
    let increment =
        function("increment", signature(vec![carrier()], carrier()), body, 1);
    let set = set_with(vec![increment], Vec::new(), Vec::new());
    assert_eq!(
        eval(&set, &call(BASE, vec![case("ok", "value", int(1))])),
        Evaluation::Value(crate::ok_value(Value::I32(2)))
    );
    assert_eq!(
        eval(&set, &call(BASE, vec![case("err", "error", text("bad"))])),
        Evaluation::Value(crate::variant_value(
            "err",
            vec![("error", text_value("bad"))]
        ))
    );
}

/// Tuple, list, record, and literal patterns take their subject apart: a
/// list pattern matches only its own length, and a record pattern ignores
/// the fields it omits.
#[test]
fn match_takes_products_and_lists_apart() {
    let set = set_with(Vec::new(), Vec::new(), Vec::new());
    let literal = |value: i32| {
        Pattern::Literal(Constant::new(Type::I32, Value::I32(value)).unwrap())
    };
    let add = |left: Node, right: Node| {
        integer(IntegerType::I32, IntegerOp::Add, vec![left, right])
    };
    let pair = crate::tuple_of(
        vec![Type::I32, Type::list(Type::I32)],
        vec![int(1), crate::list_of(Type::I32, vec![int(2), int(3)])],
    );
    let matched = node(Expr::Match {
        subject: Box::new(pair),
        arms: vec![
            MatchArm {
                pattern: Pattern::Tuple(vec![
                    literal(1),
                    Pattern::List(vec![Pattern::Bind(0)]),
                ]),
                body: int(-1),
            },
            MatchArm {
                pattern: Pattern::Tuple(vec![
                    literal(1),
                    Pattern::List(vec![Pattern::Bind(0), Pattern::Bind(1)]),
                ]),
                body: add(var(0, Type::I32), var(1, Type::I32)),
            },
            MatchArm {
                pattern: Pattern::Discard,
                body: int(0),
            },
        ],
        value_type: Type::I32,
    });
    assert_eq!(eval(&set, &matched), Evaluation::Value(Value::I32(5)));
    let fields =
        NamedTypes::new([("a".to_owned(), Type::I32), ("b".to_owned(), Type::I32)])
            .unwrap();
    let record = construct(
        Type::Record(fields),
        Construction::Record(vec![("a".to_owned(), int(1)), ("b".to_owned(), int(7))]),
    );
    let matched = node(Expr::Match {
        subject: Box::new(record),
        arms: vec![MatchArm {
            pattern: Pattern::Record(vec![("b".to_owned(), Pattern::Bind(0))]),
            body: var(0, Type::I32),
        }],
        value_type: Type::I32,
    });
    assert_eq!(eval(&set, &matched), Evaluation::Value(Value::I32(7)));
}

/// An unwritten label takes the callee's default.
#[test]
fn an_unwritten_label_takes_the_callees_default() {
    let ty = Type::function(FnType {
        parameters: vec![Type::I32],
        labels: NamedTypes::new([("step".to_owned(), Type::I32)]).unwrap(),
        result: Type::I32,
    });
    let step = CheckedFunction {
        defaults: vec![(
            "step".to_owned(),
            Constant::new(Type::I32, Value::I32(5)).unwrap(),
        )],
        ..function(
            "step",
            vibra_ir::SignatureRecord::new(Vec::new(), ty).unwrap(),
            integer(
                IntegerType::I32,
                IntegerOp::Add,
                vec![var(0, Type::I32), var(1, Type::I32)],
            ),
            2,
        )
    };
    let set = set_with(vec![step], Vec::new(), Vec::new());
    let call = node(Expr::Call {
        target: direct(BASE),
        arguments: Arguments {
            positional: vec![int(1)],
            labelled: vec![("step".to_owned(), node(Expr::Default(Type::I32)))],
        },
    });
    assert_eq!(eval(&set, &call), Evaluation::Value(Value::I32(6)));
}

/// V9: a closure value carries its checked defaults, so a call through the
/// value binds them for an unwritten label, inside a function the module
/// set validated.
#[test]
fn v9_a_call_through_a_closure_value_binds_its_defaults() {
    let ty = Type::function(FnType {
        parameters: vec![Type::I32],
        labels: NamedTypes::new([("step".to_owned(), Type::I32)]).unwrap(),
        result: Type::I32,
    });
    let stepper = node(Expr::Closure {
        closure: std::sync::Arc::new(vibra_ir::ClosureBody {
            value_type: ty,
            slot_count: 2,
            captures: Vec::new(),
            defaults: vec![(
                "step".to_owned(),
                Constant::new(Type::I32, Value::I32(5)).unwrap(),
            )],
            body: integer(
                IntegerType::I32,
                IntegerOp::Add,
                vec![var(0, Type::I32), var(1, Type::I32)],
            ),
        }),
        captures: Vec::new(),
    });
    let call = node(Expr::Call {
        target: CallTarget::Value(Box::new(stepper)),
        arguments: Arguments {
            positional: vec![int(1)],
            labelled: vec![("step".to_owned(), node(Expr::Default(Type::I32)))],
        },
    });
    let main = function("main", crate::signature(Vec::new(), Type::I32), call, 0);
    let set = set_with(vec![main], Vec::new(), Vec::new());
    assert_eq!(
        eval(&set, &crate::call(BASE, Vec::new())),
        Evaluation::Value(Value::I32(6))
    );
}

/// Runs `body` as an entry of result type `value_type` and gives where its
/// trap was written.
fn trap_start_of(value_type: Type, body: Node) -> usize {
    let set = set_with(
        vec![function("main", signature(Vec::new(), value_type), body, 0)],
        Vec::new(),
        Vec::new(),
    );
    let error = run(&program(set, BASE), MemoryBudget::DEFAULT).unwrap_err();
    let vibra_interp::RuntimeError::Trap { origin, trap, .. } = error else {
        panic!("a trap");
    };
    assert_eq!(trap, Trap::InvalidCheckedProgram);
    origin.unwrap().span().start()
}

/// `value`, after a trap written at `start..start + 1`.
fn trapping(start: usize, value: Node) -> Node {
    sequence(vec![trap_at(start), value])
}

/// The operands of a tuple, a record, a list, and a dict run in written
/// order, a dict entry's key before its value (`docs/spec/06-runtime.md`,
/// "Evaluation"). Of two operands that both trap, the first written traps.
#[test]
fn products_evaluate_operands_in_written_order() {
    let pair = vec![Type::I32, Type::I32];
    let tuple = crate::tuple_of(pair, vec![trapping(2, int(1)), trapping(3, int(2))]);
    assert_eq!(
        trap_start_of(Type::Tuple(vec![Type::I32, Type::I32]), tuple),
        2
    );
    let record_type =
        Type::record([("a".to_owned(), Type::I32), ("b".to_owned(), Type::I32)])
            .unwrap();
    // `b` is written before `a`, so its operand runs first.
    let record = construct(
        record_type.clone(),
        Construction::Record(vec![
            ("b".to_owned(), trapping(2, int(2))),
            ("a".to_owned(), trapping(3, int(1))),
        ]),
    );
    assert_eq!(trap_start_of(record_type, record), 2);
    let list =
        crate::list_of(Type::I32, vec![trapping(2, int(1)), trapping(3, int(2))]);
    assert_eq!(trap_start_of(Type::list(Type::I32), list), 2);
    let atom = |name: &str| crate::lit(Type::Atom, Value::Atom(name.to_owned()));
    let table = crate::dict(Type::Atom, Type::I32);
    let entries = |key_trap: bool| {
        let key = if key_trap {
            trapping(2, atom("b"))
        } else {
            atom("b")
        };
        construct(
            table.clone(),
            Construction::Dict(vec![
                (key, trapping(3, int(1))),
                (trapping(4, atom("a")), trapping(5, int(2))),
            ]),
        )
    };
    assert_eq!(trap_start_of(table.clone(), entries(true)), 2);
    assert_eq!(trap_start_of(table.clone(), entries(false)), 3);
}
