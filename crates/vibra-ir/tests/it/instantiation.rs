//! The instantiation graph (`docs/spec/02-type-system.md`, "Generics"): a
//! generic function that reaches itself at a type built around one of its
//! own type parameters is rejected, beside the nearest sets that are
//! accepted.

use vibra_ir::{
    Arguments, Binder, Bound, CallTarget, CheckedFunction, CheckedModuleSet, Constant,
    Construction, ContractCall, Expr, FnType, FunctionBody, Implementation,
    ImplementationRecord, InterfaceDefinition, InterfaceKey, InterfaceMember, IrError,
    ModuleSetParts, NamedTypes, Node, SignatureRecord, Type, TypeId, Value,
};

use crate::{id, library_parts, node, origin};

fn binder() -> Type {
    Type::Binder(0)
}

/// A function of one type parameter, `Fn({T} Void)`, under `bound`.
fn generic(name: &str, bound: Option<Bound>, body: Node) -> CheckedFunction {
    let signature = SignatureRecord::new(
        vec![Binder::new("T", bound).unwrap()],
        Type::function(FnType::new(vec![binder()], Type::Void)),
    )
    .unwrap();
    CheckedFunction {
        name: name.to_owned(),
        signature,
        defaults: Vec::new(),
        body: FunctionBody::Expr(Box::new(body)),
        entry_guard: None,
        exit_guard: None,
        slot_count: 1,
        origin: origin(),
    }
}

/// An operand of `ty`: the parameter itself, a `U8`, or an empty list.
fn operand(ty: &Type) -> Node {
    match ty {
        Type::Binder(_) => node(Expr::Variable {
            slot: 0,
            value_type: ty.clone(),
        }),
        Type::U8 => node(Expr::Literal(
            Constant::new(Type::U8, Value::U8(0)).unwrap(),
        )),
        _ => node(Expr::Construct {
            value_type: ty.clone(),
            construction: Construction::List(Vec::new()),
        }),
    }
}

/// A call of the function `function` at the type argument `at`.
fn call(function: usize, at: Type) -> Node {
    node(Expr::Call {
        arguments: Arguments {
            positional: vec![operand(&at)],
            labelled: Vec::new(),
        },
        target: CallTarget::Direct {
            function,
            type_arguments: vec![at],
        },
    })
}

fn void() -> Node {
    node(Expr::Literal(
        Constant::new(Type::Void, Value::Void).unwrap(),
    ))
}

fn set(functions: Vec<CheckedFunction>) -> Result<(), IrError> {
    let mut parts = library_parts();
    parts.functions = functions;
    CheckedModuleSet::try_new(parts).map(drop)
}

fn expanding(result: Result<(), IrError>, caller: &str, callee: &str) -> bool {
    matches!(
        result,
        Err(IrError::ExpandingInstantiation { name, callee: named, .. })
            if name == caller && named == callee
    )
}

#[test]
fn a_function_that_calls_itself_at_a_type_around_its_parameter_is_rejected() {
    let direct = set(vec![generic("f", None, call(0, Type::list(binder())))]);
    assert!(expanding(direct, "f", "f"));
}

#[test]
fn a_function_that_calls_itself_at_its_parameter_or_a_closed_type_is_accepted() {
    assert_eq!(set(vec![generic("f", None, call(0, binder()))]), Ok(()));
    assert_eq!(set(vec![generic("f", None, call(0, Type::U8))]), Ok(()));
    assert_eq!(
        set(vec![generic("f", None, call(0, Type::list(Type::U8)))]),
        Ok(())
    );
}

#[test]
fn two_functions_that_reach_each_other_around_a_parameter_are_rejected() {
    let mutual = set(vec![
        generic("f", None, call(1, Type::list(binder()))),
        generic("g", None, call(0, binder())),
    ]);
    assert!(expanding(mutual, "f", "g"));
}

/// The cycle is closed at a closed type: `g` never hands its parameter back.
#[test]
fn a_cycle_closed_at_a_closed_type_is_accepted() {
    assert_eq!(
        set(vec![
            generic("f", None, call(1, Type::list(binder()))),
            generic("g", None, call(0, Type::U8)),
        ]),
        Ok(())
    );
}

/// An instantiation outside every cycle may build around a parameter.
#[test]
fn an_expanding_instantiation_outside_a_cycle_is_accepted() {
    assert_eq!(
        set(vec![
            generic("f", None, call(1, Type::list(binder()))),
            generic("g", None, void()),
        ]),
        Ok(())
    );
}

#[test]
fn a_function_value_and_a_closure_body_are_instantiations() {
    let value = node(Expr::Sequence(vec![
        node(Expr::Function {
            function: 0,
            type_arguments: vec![Type::list(binder())],
        }),
        void(),
    ]));
    assert!(expanding(set(vec![generic("f", None, value)]), "f", "f"));
    let closure = node(Expr::Closure {
        closure: std::sync::Arc::new(vibra_ir::ClosureBody {
            value_type: Type::function(FnType::new(Vec::new(), Type::Void)),
            slot_count: 0,
            captures: Vec::new(),
            defaults: Vec::new(),
            body: node(Expr::Call {
                arguments: Arguments {
                    positional: vec![node(Expr::Construct {
                        value_type: Type::list(binder()),
                        construction: Construction::List(Vec::new()),
                    })],
                    labelled: Vec::new(),
                },
                target: CallTarget::Direct {
                    function: 0,
                    type_arguments: vec![Type::list(binder())],
                },
            }),
            entry_guard: None,
            exit_guard: None,
        }),
        captures: Vec::new(),
    });
    let body = node(Expr::Sequence(vec![closure, void()]));
    assert!(expanding(set(vec![generic("f", None, body)]), "f", "f"));
}

fn show_id() -> TypeId {
    id("test.show.Show")
}

fn show_bound() -> Bound {
    Bound {
        contract: InterfaceKey::new(show_id(), Vec::new()),
        choices: NamedTypes::empty(),
    }
}

/// `Show.show` on `receiver`.
fn show(receiver: Type) -> Node {
    node(Expr::Call {
        arguments: Arguments {
            positional: vec![operand(&receiver)],
            labelled: Vec::new(),
        },
        target: CallTarget::Contract(Box::new(ContractCall {
            interface: show_id(),
            member: "show".to_owned(),
            receiver,
            arguments: Vec::new(),
            choices: NamedTypes::empty(),
            member_arguments: Vec::new(),
        })),
    })
}

/// `interface Show` with `show: Fn({Self} Void)` and
/// `impl(T: Show) Show for List(T)`, whose `show` is function 0 with the
/// body `show_list`; `others` follow it.
fn show_set(show_list: Node, others: Vec<CheckedFunction>) -> Result<(), IrError> {
    let member = SignatureRecord::interface_member(
        Vec::new(),
        Some(0),
        Type::function(FnType::new(vec![Type::SelfType], Type::Void)),
        Vec::new(),
    )
    .unwrap();
    let interface = InterfaceDefinition::new(
        show_id(),
        Vec::new(),
        Vec::new(),
        vec![InterfaceMember {
            name: "show".to_owned(),
            signature: member,
            default: None,
        }],
    )
    .unwrap();
    let list = Type::list(binder());
    let signature = SignatureRecord::new(
        vec![Binder::new("T", Some(show_bound())).unwrap()],
        Type::function(FnType::new(vec![list.clone()], Type::Void)),
    )
    .unwrap();
    let show_list = CheckedFunction {
        signature,
        ..generic("showList", None, show_list)
    };
    let implementation = Implementation::new(
        ImplementationRecord::new(
            vec![Binder::new("T", Some(show_bound())).unwrap()],
            InterfaceKey::new(show_id(), Vec::new()),
            list,
            NamedTypes::empty(),
        )
        .unwrap(),
        vec![("show".to_owned(), 0)],
    )
    .unwrap();
    let mut parts: ModuleSetParts = library_parts();
    parts.interfaces.push(interface);
    parts.functions = vec![show_list];
    parts.functions.extend(others);
    parts.implementations.push(implementation);
    CheckedModuleSet::try_new(parts).map(drop)
}

/// A contract call whose receiver selects an implementation instantiates
/// that implementation's member.
#[test]
fn a_contract_call_instantiates_the_member_it_selects() {
    let list = Type::list(binder());
    assert_eq!(show_set(void(), Vec::new()), Ok(()));
    assert!(expanding(
        show_set(show(Type::list(list)), Vec::new()),
        "showList",
        "showList"
    ));
}

/// The evidence for a bound instantiates the members of the implementation
/// it selects: `f(T)` hands `List(T)` to a bounded parameter, and the
/// implementation for `List(T)` calls `f` around its own parameter.
#[test]
fn the_evidence_for_a_bound_instantiates_the_selected_implementation() {
    let list = Type::list(binder());
    let f = |body: Node| generic("f", Some(show_bound()), body);
    let g = || generic("g", Some(show_bound()), void());
    assert_eq!(
        show_set(void(), vec![f(call(2, list.clone())), g()]),
        Ok(())
    );
    // The cycle through the evidence hands the parameter back unchanged.
    let f_at_parameter = node(Expr::Sequence(vec![
        node(Expr::Function {
            function: 1,
            type_arguments: vec![binder()],
        }),
        void(),
    ]));
    assert_eq!(
        show_set(f_at_parameter, vec![f(call(2, list.clone())), g()]),
        Ok(())
    );
    assert!(expanding(
        show_set(call(1, list.clone()), vec![f(call(2, list)), g()]),
        "showList",
        "f"
    ));
}
