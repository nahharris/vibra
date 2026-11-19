//! The forms the transition profile does not run, and the totality of the
//! emitter over the node kinds (`docs/spec/06-runtime.md`, "Wasm
//! transition"): a node it does not lower is `EmitError::Unsupported`, which
//! names the node and where it was written, and the whole module fails. A
//! node it lowers runs as the reference interpreter runs it.

use std::collections::BTreeSet;
use std::sync::Arc;

use vibra_ir::external::{IntegerOp, IntegerType};
use vibra_ir::{
    Arguments, CallTarget, CheckedFunction, CheckedGlobal, CheckedProgram, ClosureBody,
    Constant, Construction, Expr, FnType, Guard, NamedTypes, Node, Pattern,
    SignatureRecord, Type, Value,
};
use vibra_wasm::EmitError;

use crate::emitted::agrees;
use crate::programs::{
    call, function, if_else, int, integer, lit, main_returning, matching, node, none,
    option, origin, origin_at, program, runner, set_of, set_with_globals, signature,
    some, tail_call, u64_lit, var,
};

/// The name of a node kind. The match has no wildcard, so a new kind of
/// [`Expr`] does not compile until it has a name and a sample below.
const fn kind_of(expr: &Expr) -> &'static str {
    match expr {
        Expr::Literal(_) => "literal",
        Expr::Intrinsic { .. } => "intrinsic",
        Expr::Sequence(_) => "sequence",
        Expr::Variable { .. } => "variable",
        Expr::Global(_) => "global",
        Expr::Function { .. } => "function",
        Expr::Closure { .. } => "closure",
        Expr::Captured { .. } => "captured",
        Expr::Let { .. } => "let",
        Expr::LetElse { .. } => "letElse",
        Expr::Match { .. } => "match",
        Expr::If { .. } => "if",
        Expr::Construct { .. } => "construct",
        Expr::Field { .. } => "field",
        Expr::Lookup { .. } => "lookup",
        Expr::Call { .. } => "call",
        Expr::TailCall { .. } => "tailCall",
        Expr::Return(_) => "return",
        Expr::Try(_) => "try",
        Expr::WidenToInterface { .. } => "widenToInterface",
        Expr::WidenAtom(_) => "widenAtom",
        Expr::Default(_) => "default",
    }
}

/// Every kind [`kind_of`] names.
const KINDS: [&str; 22] = [
    "literal",
    "intrinsic",
    "sequence",
    "variable",
    "global",
    "function",
    "closure",
    "captured",
    "let",
    "letElse",
    "match",
    "if",
    "construct",
    "field",
    "lookup",
    "call",
    "tailCall",
    "return",
    "try",
    "widenToInterface",
    "widenAtom",
    "default",
];

fn add(left: Node, right: Node) -> Node {
    integer(IntegerType::I32, IntegerOp::Add, vec![left, right])
}

fn let_in(slot: usize, value: Node, body: Node) -> Node {
    node(Expr::Let {
        slot: Some(slot),
        value: Box::new(value),
        body: Box::new(body),
    })
}

fn call_value(callee: Node, arguments: Vec<Node>) -> Node {
    node(Expr::Call {
        target: CallTarget::Value(Box::new(callee)),
        arguments: Arguments {
            positional: arguments,
            labelled: Vec::new(),
        },
    })
}

fn i32_fn() -> Type {
    Type::function(FnType::new(vec![Type::I32], Type::I32))
}

/// `main` at 0, which calls `helper` at 1 with `argument`, and `helper`.
fn calling(result: Type, argument: Node, helper: CheckedFunction) -> CheckedProgram {
    let main = function(
        "main",
        signature(Vec::new(), result),
        call(1, vec![argument]),
        0,
    );
    program(set_of(vec![main, helper]), 0)
}

/// `identity(x: I32) I32`.
fn identity() -> CheckedFunction {
    function(
        "identity",
        signature(vec![Type::I32], Type::I32),
        var(0, Type::I32),
        1,
    )
}

/// One program for each node kind, with the node of that kind it holds.
#[allow(clippy::too_many_lines)]
fn samples() -> Vec<(Node, CheckedProgram)> {
    let mut samples = Vec::new();
    let mut root = |node: Node, result: Type, slots: usize| {
        samples.push((node.clone(), main_returning(result, node, slots)));
    };
    root(int(1), Type::I32, 0);
    root(add(int(1), int(2)), Type::I32, 0);
    root(node(Expr::Sequence(vec![int(1), int(2)])), Type::I32, 0);
    root(let_in(0, int(4), var(0, Type::I32)), Type::I32, 1);
    root(
        if_else(
            integer(IntegerType::I32, IntegerOp::Equal, vec![int(1), int(1)]),
            int(2),
            int(3),
        ),
        Type::I32,
        0,
    );
    root(some(Type::I32, int(1)), option(Type::I32), 0);
    root(
        matching(
            none(Type::I32),
            vec![
                (
                    Pattern::Case {
                        case: "some".to_owned(),
                        fields: vec![("value".to_owned(), Pattern::Bind(0))],
                    },
                    var(0, Type::I32),
                ),
                (Pattern::Discard, int(0)),
            ],
            Type::I32,
        ),
        Type::I32,
        1,
    );
    let record_type = Type::record([("a".to_owned(), Type::I32)]).unwrap();
    root(
        node(Expr::Field {
            record: Box::new(node(Expr::Construct {
                value_type: record_type,
                construction: Construction::Record(vec![("a".to_owned(), int(6))]),
            })),
            field: "a".to_owned(),
        }),
        Type::I32,
        0,
    );
    root(
        node(Expr::Lookup {
            collection: Box::new(node(Expr::Construct {
                value_type: Type::list(Type::I32),
                construction: Construction::List(vec![int(1), int(2)]),
            })),
            key: Box::new(u64_lit(1)),
        }),
        option(Type::I32),
        0,
    );
    root(
        node(Expr::WidenToInterface {
            value: Box::new(lit(Type::U8, Value::U8(200))),
            value_type: Type::Any,
        }),
        Type::Any,
        0,
    );
    root(
        node(Expr::WidenAtom(Box::new(lit(
            Type::AtomSingleton("red".to_owned()),
            Value::Atom("red".to_owned()),
        )))),
        Type::Atom,
        0,
    );
    let captured = node(Expr::Captured {
        slot: 0,
        value_type: Type::I32,
    });
    let closure = node(Expr::Closure {
        closure: Arc::new(ClosureBody {
            value_type: i32_fn(),
            slot_count: 1,
            captures: vec![Type::I32],
            defaults: Vec::new(),
            body: add(var(0, Type::I32), captured.clone()),
        }),
        captures: vec![int(2)],
    });
    let read = var(0, Type::I32);
    samples.push((
        read.clone(),
        main_returning(Type::I32, let_in(0, int(4), read), 1),
    ));
    let applied = call_value(closure.clone(), vec![int(1)]);
    samples.push((closure, main_returning(Type::I32, applied.clone(), 0)));
    samples.push((captured, main_returning(Type::I32, applied, 0)));

    let value = node(Expr::Function {
        function: 1,
        type_arguments: Vec::new(),
    });
    let main = function(
        "main",
        signature(Vec::new(), Type::I32),
        call_value(value.clone(), vec![int(5)]),
        0,
    );
    samples.push((value, program(set_of(vec![main, identity()]), 0)));

    let direct = call(1, vec![int(3)]);
    samples.push((direct, calling(Type::I32, int(3), identity())));
    let tail = tail_call(1, vec![int(3)]);
    let main = function("main", signature(Vec::new(), Type::I32), tail.clone(), 0);
    samples.push((tail, program(set_of(vec![main, identity()]), 0)));

    let global = node(Expr::Global(0));
    let main = function("main", signature(Vec::new(), Type::I32), global.clone(), 0);
    let held = CheckedGlobal {
        name: "seven".to_owned(),
        value_type: Type::I32,
        initializer: int(7),
        slot_count: 0,
        origin: origin(),
    };
    samples.push((global, program(set_with_globals(vec![main], vec![held]), 0)));

    // `unwrap(x: Option(I32)) I32`: `let some(value: v) = x else return -1`.
    let fallback = node(Expr::Return(Box::new(int(-1))));
    let let_else = node(Expr::LetElse {
        pattern: Pattern::Case {
            case: "some".to_owned(),
            fields: vec![("value".to_owned(), Pattern::Bind(1))],
        },
        value: Box::new(var(0, option(Type::I32))),
        fallback: Box::new(fallback.clone()),
        body: Box::new(var(1, Type::I32)),
    });
    let unwrap = function(
        "unwrap",
        signature(vec![option(Type::I32)], Type::I32),
        let_else.clone(),
        2,
    );
    samples.push((
        let_else,
        calling(Type::I32, none(Type::I32), unwrap.clone()),
    ));
    samples.push((
        fallback,
        calling(Type::I32, some(Type::I32, int(8)), unwrap),
    ));

    // `increment(x: Option(I32)) Option(I32)`: `some(x? + 1)`.
    let attempt = node(Expr::Try(Box::new(var(0, option(Type::I32)))));
    let increment = function(
        "increment",
        signature(vec![option(Type::I32)], option(Type::I32)),
        some(Type::I32, add(attempt.clone(), int(1))),
        1,
    );
    samples.push((
        attempt,
        calling(option(Type::I32), some(Type::I32, int(9)), increment),
    ));

    // `step(n: I32 step: I32 = 5) I32`, called with the label not written.
    let default = node(Expr::Default(Type::I32));
    let step = CheckedFunction {
        defaults: vec![(
            "step".to_owned(),
            Constant::new(Type::I32, Value::I32(5)).unwrap(),
        )],
        ..function(
            "step",
            SignatureRecord::new(
                Vec::new(),
                Type::function(FnType {
                    parameters: vec![Type::I32],
                    labels: NamedTypes::new([("step".to_owned(), Type::I32)]).unwrap(),
                    result: Type::I32,
                }),
            )
            .unwrap(),
            add(var(0, Type::I32), var(1, Type::I32)),
            2,
        )
    };
    let main = function(
        "main",
        signature(Vec::new(), Type::I32),
        node(Expr::Call {
            target: CallTarget::Direct {
                function: 1,
                type_arguments: Vec::new(),
            },
            arguments: Arguments {
                positional: vec![int(1)],
                labelled: vec![("step".to_owned(), default.clone())],
            },
        }),
        0,
    );
    samples.push((default, program(set_of(vec![main, step]), 0)));
    samples
}

#[test]
fn emit_unsupported_is_total() {
    let mut covered = BTreeSet::new();
    let mut unsupported = BTreeSet::new();
    for (node, program) in samples() {
        let kind = kind_of(&node.expr);
        covered.insert(kind);
        match vibra_wasm::emit(&program) {
            // A kind the profile lowers runs as the interpreter runs it.
            Ok(_) => {
                agrees(&program, &runner());
            }
            Err(EmitError::Unsupported { node, .. }) => {
                unsupported.insert(node);
            }
            Err(error) => panic!("`{kind}`: {error}"),
        }
    }
    assert_eq!(covered, KINDS.into_iter().collect::<BTreeSet<_>>());
    // The refusals of these samples are the transition profile's: module
    // values and interface values.
    assert_eq!(
        unsupported,
        BTreeSet::from(["global", "interfaceValue"]),
        "the refused node kinds"
    );
}

#[test]
fn emit_unsupported_guard() {
    let consequence = Node::new(
        Expr::Literal(Constant::new(Type::I32, Value::I32(0)).unwrap()),
        origin_at(30, 31),
    );
    let guarded = CheckedFunction {
        entry_guard: Some(Guard {
            predicate: integer(
                IntegerType::I32,
                IntegerOp::Equal,
                vec![int(1), int(1)],
            ),
            consequence,
        }),
        ..function("main", signature(Vec::new(), Type::I32), int(1), 0)
    };
    let refused = vibra_wasm::emit(&program(set_of(vec![guarded]), 0));
    assert_eq!(
        refused.map(|_| ()),
        Err(EmitError::Unsupported {
            node: "entryGuard",
            origin: origin_at(30, 31),
        })
    );
}

#[test]
fn emit_unsupported_test_with_takes() {
    // The IR has no grant type yet: a test that takes anything has a
    // parameter, and the profile runs only a test that takes nothing.
    let takes = CheckedFunction {
        origin: origin_at(40, 50),
        ..function(
            "readsConfig",
            signature(vec![Type::I32], Type::Void),
            lit(Type::Void, Value::Void),
            1,
        )
    };
    let set = set_of(vec![takes]);
    assert_eq!(
        vibra_wasm::emit_tests(&set, &[0]).map(|_| ()),
        Err(EmitError::Unsupported {
            node: "testTakes",
            origin: origin_at(40, 50),
        })
    );
}
