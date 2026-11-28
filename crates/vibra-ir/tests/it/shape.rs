//! The shape check of `CheckedModuleSet::try_new` and the initializer-cycle
//! analysis of `CheckedProgram::for_entry`
//! (`docs/spec/06-runtime.md`, "Module-value initialization").

use std::sync::Arc;

use vibra_ir::external::{CompilerIntrinsic, IntegerOp, IntegerType};
use vibra_ir::{
    Arguments, Binder, CallTarget, CheckedFunction, CheckedGlobal, CheckedModuleSet,
    CheckedProgram, ClosureBody, Constant, Construction, ContractCall, Expr, FnType,
    Implementation, ImplementationRecord, InterfaceKey, Invariant, IrError, Library,
    ModuleSetParts, Node, SignatureRecord, Type, TypeBody, TypeDefinition, Value,
    Visibility,
};

use crate::{
    bool_type, call, function, global, id, int, library_parts, library_type, named,
    node, option, origin, path, positional, signature, str_type, variable, void,
};

fn main_function() -> CheckedFunction {
    function("main", signature(Vec::new(), Type::Void), void(), 0)
}

fn parts(
    globals: Vec<CheckedGlobal>,
    functions: Vec<CheckedFunction>,
) -> ModuleSetParts {
    ModuleSetParts {
        globals,
        functions,
        ..library_parts()
    }
}

fn build(
    globals: Vec<CheckedGlobal>,
    functions: Vec<CheckedFunction>,
) -> Result<Arc<CheckedModuleSet>, IrError> {
    CheckedModuleSet::try_new(parts(globals, functions))
}

fn identity() -> CheckedFunction {
    function(
        "identity",
        signature(vec![Type::I32], Type::I32),
        variable(0, Type::I32),
        1,
    )
}

#[test]
fn a_well_formed_set_is_accepted_and_runs_from_its_entry() {
    let set = build(
        vec![global("one", Type::I32, call(0, vec![int(1)]))],
        vec![identity(), main_function()],
    )
    .unwrap();
    let program = CheckedProgram::for_entry(set, 1).unwrap();
    assert_eq!(program.entry(), 1);
}

#[test]
fn a_call_with_the_wrong_arity_is_rejected() {
    let error = build(
        vec![global("one", Type::I32, call(0, Vec::new()))],
        vec![identity()],
    )
    .unwrap_err();
    assert_eq!(
        error,
        IrError::ArityMismatch {
            what: "arguments",
            expected: 1,
            actual: 0
        }
    );
}

#[test]
fn a_let_whose_body_reads_an_unset_slot_is_rejected() {
    let body = node(Expr::Let {
        slot: Some(0),
        value: Box::new(int(1)),
        body: Box::new(variable(1, Type::I32)),
    });
    let error = build(
        Vec::new(),
        vec![function("read", signature(Vec::new(), Type::I32), body, 2)],
    )
    .unwrap_err();
    assert_eq!(error, IrError::UnsetSlot(1));
}

#[test]
fn a_slot_is_unset_again_after_its_let() {
    let body = node(Expr::Sequence(vec![
        node(Expr::Let {
            slot: Some(0),
            value: Box::new(int(1)),
            body: Box::new(void()),
        }),
        variable(0, Type::I32),
    ]));
    let error = build(
        Vec::new(),
        vec![function("read", signature(Vec::new(), Type::I32), body, 1)],
    )
    .unwrap_err();
    assert_eq!(error, IrError::UnsetSlot(0));
}

#[test]
fn a_slot_read_at_another_type_or_past_the_frame_is_rejected() {
    let wrong_type = function(
        "read",
        signature(vec![Type::I32], Type::I64),
        variable(0, Type::I64),
        1,
    );
    assert!(matches!(
        build(Vec::new(), vec![wrong_type]),
        Err(IrError::SlotTypeMismatch { slot: 0, .. })
    ));
    let past = function(
        "read",
        signature(vec![Type::I32], Type::I32),
        variable(3, Type::I32),
        1,
    );
    assert_eq!(
        build(Vec::new(), vec![past]).unwrap_err(),
        IrError::SlotOutOfRange { slot: 3, count: 1 }
    );
}

#[test]
fn a_default_outside_a_label_and_a_return_outside_a_callable_are_misplaced() {
    let default = global("value", Type::I32, node(Expr::Default(Type::I32)));
    assert!(matches!(
        build(vec![default], Vec::new()),
        Err(IrError::MisplacedNode(_))
    ));
    let returned = global("value", Type::I32, node(Expr::Return(Box::new(int(1)))));
    assert!(matches!(
        build(vec![returned], Vec::new()),
        Err(IrError::MisplacedNode(_))
    ));
}

#[test]
fn an_if_needs_a_bool_condition() {
    let condition = node(Expr::If {
        condition: Box::new(int(1)),
        then_branch: Box::new(int(2)),
        else_branch: Some(Box::new(int(3))),
    });
    assert!(matches!(
        build(vec![global("value", Type::I32, condition)], Vec::new()),
        Err(IrError::TypeMismatch { .. })
    ));
    let truth = node(Expr::Literal(
        Constant::new(bool_type(), Value::bool(true)).unwrap(),
    ));
    let accepted = node(Expr::If {
        condition: Box::new(truth),
        then_branch: Box::new(int(2)),
        else_branch: Some(Box::new(int(3))),
    });
    assert!(build(vec![global("value", Type::I32, accepted)], Vec::new()).is_ok());
}

#[test]
fn an_initializer_cycle_through_a_function_names_one_global() {
    // `a` calls `getB`, which reads `b`, which reads `a`.
    let set = build(
        vec![
            global("a", Type::I32, call(0, Vec::new())),
            global("b", Type::I32, node(Expr::Global(0))),
        ],
        vec![
            function(
                "getB",
                signature(Vec::new(), Type::I32),
                node(Expr::Global(1)),
                0,
            ),
            main_function(),
        ],
    )
    .unwrap();
    assert_eq!(
        CheckedProgram::for_entry(set, 1).unwrap_err(),
        IrError::InitializerCycle {
            global: 0,
            name: "a".to_owned()
        }
    );
}

#[test]
fn an_initializer_cycle_through_a_stored_closure_is_found() {
    // `f` holds a closure that reads `a`; `a` calls the value of `f`.
    let thunk = Type::function(FnType::new(Vec::new(), Type::I32));
    let closure = node(Expr::Closure {
        closure: Arc::new(ClosureBody {
            value_type: thunk.clone(),
            slot_count: 0,
            captures: Vec::new(),
            defaults: Vec::new(),
            body: node(Expr::Global(0)),
            entry_guard: None,
            exit_guard: None,
        }),
        captures: Vec::new(),
    });
    let a = node(Expr::Call {
        target: CallTarget::Value(Box::new(node(Expr::Global(1)))),
        arguments: Arguments::default(),
    });
    let set = build(
        vec![global("a", Type::I32, a), global("f", thunk, closure)],
        vec![main_function()],
    )
    .unwrap();
    assert!(matches!(
        CheckedProgram::for_entry(set, 0),
        Err(IrError::InitializerCycle { global: 0, .. })
    ));
}

#[test]
fn mutually_recursive_functions_are_not_a_cycle() {
    let tail = |function| {
        node(Expr::TailCall {
            target: CallTarget::Direct {
                function,
                type_arguments: Vec::new(),
            },
            arguments: Arguments::default(),
        })
    };
    let set = build(
        vec![global("x", Type::I32, call(0, Vec::new()))],
        vec![
            function("f", signature(Vec::new(), Type::I32), tail(1), 0),
            function("g", signature(Vec::new(), Type::I32), tail(0), 0),
            main_function(),
        ],
    )
    .unwrap();
    assert!(CheckedProgram::for_entry(set, 2).is_ok());
}

#[test]
fn an_entry_that_takes_inputs_or_does_not_exist_is_rejected() {
    let set = build(Vec::new(), vec![identity()]).unwrap();
    assert!(matches!(
        CheckedProgram::for_entry(Arc::clone(&set), 0),
        Err(IrError::InvalidEntry(_))
    ));
    assert_eq!(
        CheckedProgram::for_entry(set, 5).unwrap_err(),
        IrError::UnknownFunction(5)
    );
}

#[test]
fn an_external_function_has_its_row_signature() {
    let add = CompilerIntrinsic::Integer(IntegerType::I32, IntegerOp::Add);
    let good = CheckedFunction::external(
        "add",
        signature(vec![Type::I32, Type::I32], Type::I32),
        add,
        Vec::new(),
        origin(),
    );
    assert!(build(Vec::new(), vec![good]).is_ok());
    let bad = CheckedFunction::external(
        "add",
        signature(vec![Type::I32], Type::I32),
        add,
        Vec::new(),
        origin(),
    );
    assert!(matches!(
        build(Vec::new(), vec![bad]),
        Err(IrError::TypeMismatch { .. })
    ));
}

// -- implementations and contract calls ---------------------------------------

/// `Iter.next` for `List(T)`, by the row `list.next`, at `element`, with the
/// binders of `binders`.
fn list_next(element: Type, binders: usize) -> CheckedFunction {
    let binders = (0..binders)
        .map(|index| Binder::new(format!("T{index}"), None).unwrap())
        .collect();
    let list = Type::list(element.clone());
    let record = SignatureRecord::new(
        binders,
        Type::function(FnType::new(
            vec![list.clone()],
            option(Type::Tuple(vec![element.clone(), list])),
        )),
    )
    .unwrap();
    CheckedFunction::external(
        "next",
        record,
        CompilerIntrinsic::ListNext,
        vec![element],
        origin(),
    )
}

fn iter_for(binders: usize, element: Type, function: usize) -> Implementation {
    let binders = (0..binders)
        .map(|index| Binder::new(format!("T{index}"), None).unwrap())
        .collect();
    Implementation::new(
        ImplementationRecord::new(
            binders,
            InterfaceKey::new(id(path(Library::Iter)), Vec::new()),
            Type::list(element.clone()),
            named(vec![("Item", element)]),
        )
        .unwrap(),
        vec![("next".to_owned(), function)],
    )
    .unwrap()
}

fn next_of_list(choice: Type) -> Node {
    let element = node(Expr::Construct {
        value_type: Type::list(Type::I32),
        construction: Construction::List(vec![int(1)]),
    });
    node(Expr::Call {
        target: CallTarget::Contract(Box::new(ContractCall {
            interface: id(path(Library::Iter)),
            member: "next".to_owned(),
            receiver: Type::list(Type::I32),
            arguments: Vec::new(),
            choices: named(vec![("Item", choice)]),
            member_arguments: Vec::new(),
        })),
        arguments: positional(vec![element]),
    })
}

fn next_type() -> Type {
    option(Type::Tuple(vec![Type::I32, Type::list(Type::I32)]))
}

#[test]
fn a_contract_call_selects_the_implementation_and_its_choices() {
    let parts = ModuleSetParts {
        implementations: vec![iter_for(1, Type::Binder(0), 0)],
        ..parts(
            vec![global("first", next_type(), next_of_list(Type::I32))],
            vec![list_next(Type::Binder(0), 1)],
        )
    };
    assert!(CheckedModuleSet::try_new(parts).is_ok());
}

#[test]
fn a_contract_call_with_other_choices_is_rejected() {
    let parts = ModuleSetParts {
        implementations: vec![iter_for(1, Type::Binder(0), 0)],
        ..parts(
            vec![global("first", next_type(), next_of_list(Type::U8))],
            vec![list_next(Type::Binder(0), 1)],
        )
    };
    assert!(CheckedModuleSet::try_new(parts).is_err());
}

#[test]
fn overlapping_implementations_are_rejected() {
    let parts = ModuleSetParts {
        implementations: vec![
            iter_for(1, Type::Binder(0), 0),
            iter_for(0, Type::I32, 1),
        ],
        ..parts(
            Vec::new(),
            vec![list_next(Type::Binder(0), 1), list_next(Type::I32, 0)],
        )
    };
    assert!(matches!(
        CheckedModuleSet::try_new(parts),
        Err(IrError::Duplicate {
            what: "implementation",
            ..
        })
    ));
}

#[test]
fn an_implementation_member_of_the_wrong_type_is_rejected() {
    let parts = ModuleSetParts {
        implementations: vec![iter_for(0, Type::I32, 0)],
        ..parts(Vec::new(), vec![list_next(Type::U8, 0)])
    };
    assert!(matches!(
        CheckedModuleSet::try_new(parts),
        Err(IrError::TypeMismatch { .. })
    ));
}

// -- construction -------------------------------------------------------------

#[test]
fn a_construction_under_an_invariant_with_a_consequence_is_a_result() {
    let positive = id("example.count.Positive");
    let consequence =
        Constant::new(str_type(), Value::Str("notPositive".to_owned())).unwrap();
    let definition = TypeDefinition::new(
        positive.clone(),
        Vec::new(),
        TypeBody::Wrapper(Type::I32),
        Some(Invariant {
            predicate: 0,
            consequence: Some(consequence),
        }),
        Visibility::Private,
    )
    .unwrap();
    let predicate = function(
        "isPositive",
        signature(vec![Type::I32], bool_type()),
        node(Expr::Intrinsic {
            intrinsic: CompilerIntrinsic::Integer(IntegerType::I32, IntegerOp::Equal),
            type_arguments: Vec::new(),
            arguments: vec![variable(0, Type::I32), int(0)],
        }),
        1,
    );
    let construct = || {
        node(Expr::Construct {
            value_type: Type::Declared(positive.clone(), Vec::new()),
            construction: Construction::Wrapper(Box::new(int(1))),
        })
    };
    let with = |value_type: Type| {
        let mut parts = parts(
            vec![global("one", value_type, construct())],
            vec![predicate.clone()],
        );
        parts.types.push(definition.clone());
        CheckedModuleSet::try_new(parts)
    };
    let result = library_type(
        Library::Result,
        vec![Type::Declared(positive.clone(), Vec::new()), str_type()],
    );
    assert!(with(result).is_ok());
    assert!(matches!(
        with(Type::Declared(positive.clone(), Vec::new())),
        Err(IrError::TypeMismatch { .. })
    ));
}
