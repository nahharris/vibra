//! The checked IR as a trust boundary: every finding of the review of
//! `vibra-ir` and `vibra-interp` as a set `CheckedModuleSet::try_new` must
//! reject, beside the nearest set it accepts.

use std::sync::Arc;

use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::{
    Arguments, Binder, Bound, CallTarget, CheckedFunction, CheckedGlobal,
    CheckedModuleSet, Constant, Construction, ContractCall, Expr, FnType, FunctionBody,
    Implementation, ImplementationRecord, InterfaceDefinition, InterfaceKey,
    InterfaceMember, IrError, Library, ModuleSetParts, NamedTypes, Node,
    SignatureRecord, Type, TypeBody, TypeDefinition, TypeId, Value, Visibility,
};

use crate::{
    id, library, library_parts, library_type, named, node, option, origin, path,
};

fn signature(parameters: Vec<Type>, result: Type) -> SignatureRecord {
    SignatureRecord::new(Vec::new(), Type::function(FnType::new(parameters, result)))
        .unwrap()
}

fn function(
    name: &str,
    signature: SignatureRecord,
    body: Node,
    slot_count: usize,
) -> CheckedFunction {
    CheckedFunction {
        name: name.to_owned(),
        signature,
        defaults: Vec::new(),
        body: FunctionBody::Expr(Box::new(body)),
        entry_guard: None,
        exit_guard: None,
        slot_count,
        origin: origin(),
    }
}

fn global(name: &str, value_type: Type, initializer: Node) -> CheckedGlobal {
    CheckedGlobal {
        name: name.to_owned(),
        value_type,
        initializer,
        slot_count: 0,
        origin: origin(),
    }
}

fn literal(value_type: Type, value: Value) -> Node {
    node(Expr::Literal(Constant::new(value_type, value).unwrap()))
}

fn int(value: i32) -> Node {
    literal(Type::I32, Value::I32(value))
}

fn variable(slot: usize, value_type: Type) -> Node {
    node(Expr::Variable { slot, value_type })
}

fn positional(positional: Vec<Node>) -> Arguments {
    Arguments {
        positional,
        labelled: Vec::new(),
    }
}

fn iter_id() -> TypeId {
    id(path(Library::Iter))
}

/// `Iter(Item: item)`, the interface value type.
fn iter_of(item: Type) -> Type {
    Type::Interface(iter_id(), Vec::new(), named(vec![("Item", item)]))
}

/// The bound `Iter(Item: item)`.
fn iter_bound(item: Type) -> Bound {
    Bound {
        contract: InterfaceKey::new(iter_id(), Vec::new()),
        choices: named(vec![("Item", item)]),
    }
}

/// `List(I32)` holding `1i32`.
fn list_value() -> Node {
    node(Expr::Construct {
        value_type: Type::list(Type::I32),
        construction: Construction::List(vec![int(1)]),
    })
}

fn widen(value: Node, value_type: Type) -> Node {
    node(Expr::WidenToInterface {
        value: Box::new(value),
        value_type,
    })
}

/// `Iter.next` for `List(I32)`, by the row `list.next`: function 0.
fn list_next() -> CheckedFunction {
    let list = Type::list(Type::I32);
    CheckedFunction::external(
        "next",
        signature(
            vec![list.clone()],
            option(Type::Tuple(vec![Type::I32, list])),
        ),
        CompilerIntrinsic::ListNext,
        vec![Type::I32],
        origin(),
    )
}

/// `impl Iter for List(I32)` with `Item = I32`, whose `next` is function 0.
fn list_iter() -> Implementation {
    Implementation::new(
        ImplementationRecord::new(
            Vec::new(),
            InterfaceKey::new(iter_id(), Vec::new()),
            Type::list(Type::I32),
            named(vec![("Item", Type::I32)]),
        )
        .unwrap(),
        vec![("next".to_owned(), 0)],
    )
    .unwrap()
}

/// The library parts with `Iter` for `List(I32)`, `functions` after
/// `list_next`, and `globals`.
fn with_list_iter(
    globals: Vec<CheckedGlobal>,
    functions: Vec<CheckedFunction>,
) -> Result<Arc<CheckedModuleSet>, IrError> {
    let mut parts = library_parts();
    parts.functions.push(list_next());
    parts.functions.extend(functions);
    parts.globals = globals;
    parts.implementations.push(list_iter());
    CheckedModuleSet::try_new(parts)
}

fn contract(
    interface: TypeId,
    member: &str,
    receiver: Type,
    choices: NamedTypes,
    argument: Node,
) -> Node {
    node(Expr::Call {
        target: CallTarget::Contract(Box::new(ContractCall {
            interface,
            member: member.to_owned(),
            receiver,
            arguments: Vec::new(),
            choices,
            member_arguments: Vec::new(),
        })),
        arguments: positional(vec![argument]),
    })
}

// -- V1: widening and boxed calls ---------------------------------------------

/// A widening takes the choices of the implementation it selects; any
/// other label forges evidence.
#[test]
fn v1_a_widening_takes_the_choices_of_the_selected_implementation() {
    let at = |item: Type| {
        with_list_iter(
            vec![global(
                "boxed",
                iter_of(item.clone()),
                widen(list_value(), iter_of(item)),
            )],
            Vec::new(),
        )
    };
    assert!(at(Type::I32).is_ok());
    assert!(matches!(at(Type::U8), Err(IrError::InvalidExpression(_))));
}

#[test]
fn v1_a_widening_needs_an_implementation() {
    let set = with_list_iter(
        vec![global(
            "boxed",
            iter_of(Type::I32),
            widen(int(1), iter_of(Type::I32)),
        )],
        Vec::new(),
    );
    assert!(matches!(set, Err(IrError::InvalidExpression(_))));
}

/// `Other`, with `peek: Fn({Self} I32)`.
fn other_interface() -> InterfaceDefinition {
    let peek = SignatureRecord::interface_member(
        Vec::new(),
        Some(0),
        Type::function(FnType::new(vec![Type::SelfType], Type::I32)),
        Vec::new(),
    )
    .unwrap();
    InterfaceDefinition::new(
        id("test.other.Other"),
        Vec::new(),
        Vec::new(),
        vec![InterfaceMember {
            name: "peek".to_owned(),
            signature: peek,
            default: None,
        }],
    )
    .unwrap()
}

/// A call through an interface value names that value's interface and
/// choices.
#[test]
fn v1_a_boxed_call_names_the_interface_and_choices_of_its_receiver() {
    let boxed = || widen(list_value(), iter_of(Type::I32));
    let next = |choice: Type| {
        let result = option(Type::Tuple(vec![choice.clone(), iter_of(Type::I32)]));
        let call = contract(
            iter_id(),
            "next",
            iter_of(Type::I32),
            named(vec![("Item", choice)]),
            boxed(),
        );
        with_list_iter(vec![global("first", result, call)], Vec::new())
    };
    assert!(next(Type::I32).is_ok());
    assert!(matches!(next(Type::U8), Err(IrError::InvalidExpression(_))));
    let mut parts = library_parts();
    parts.functions.push(list_next());
    parts.implementations.push(list_iter());
    parts.interfaces.push(other_interface());
    parts.globals = vec![global(
        "peeked",
        Type::I32,
        contract(
            id("test.other.Other"),
            "peek",
            iter_of(Type::I32),
            NamedTypes::empty(),
            boxed(),
        ),
    )];
    assert!(matches!(
        CheckedModuleSet::try_new(parts),
        Err(IrError::InvalidExpression(_))
    ));
}

/// A generic function `f(T)` that widens its parameter to `Iter(Item: I32)`.
fn widening_generic(bound: Option<Bound>) -> CheckedFunction {
    let record = SignatureRecord::new(
        vec![Binder::new("T", bound).unwrap()],
        Type::function(FnType::new(vec![Type::Binder(0)], iter_of(Type::I32))),
    )
    .unwrap();
    function(
        "box",
        record,
        widen(variable(0, Type::Binder(0)), iter_of(Type::I32)),
        1,
    )
}

#[test]
fn v1_a_widened_type_parameter_needs_its_bound() {
    let set = |bound| with_list_iter(Vec::new(), vec![widening_generic(bound)]);
    assert!(set(Some(iter_bound(Type::I32))).is_ok());
    assert!(matches!(set(None), Err(IrError::InvalidExpression(_))));
    assert!(matches!(
        set(Some(iter_bound(Type::U8))),
        Err(IrError::InvalidExpression(_))
    ));
}

// -- V3: generic calls and bound evidence -------------------------------------

/// `keep(T)` under `bound`, returning its argument: function 1.
fn keep(bound: Bound) -> CheckedFunction {
    let record = SignatureRecord::new(
        vec![Binder::new("T", Some(bound)).unwrap()],
        Type::function(FnType::new(vec![Type::Binder(0)], Type::Binder(0))),
    )
    .unwrap();
    function("keep", record, variable(0, Type::Binder(0)), 1)
}

fn keep_at(argument: Type, operand: Node) -> Node {
    node(Expr::Call {
        target: CallTarget::Direct {
            function: 1,
            type_arguments: vec![argument],
        },
        arguments: positional(vec![operand]),
    })
}

#[test]
fn v3_a_generic_call_satisfies_the_bound_of_each_binder() {
    let list = Type::list(Type::I32);
    let call = |bound: Bound, argument: Type, operand: Node| {
        with_list_iter(
            vec![global("kept", argument.clone(), keep_at(argument, operand))],
            vec![keep(bound)],
        )
    };
    assert!(call(iter_bound(Type::I32), list.clone(), list_value()).is_ok());
    assert!(matches!(
        call(iter_bound(Type::I32), Type::I32, int(1)),
        Err(IrError::InvalidExpression(_))
    ));
    assert!(matches!(
        call(iter_bound(Type::U8), list, list_value()),
        Err(IrError::InvalidExpression(_))
    ));
}

#[test]
fn v3_a_function_value_satisfies_the_bounds_of_its_instantiation() {
    let value = |argument: Type| {
        let ty = Type::function(FnType::new(vec![argument.clone()], argument.clone()));
        let function = node(Expr::Function {
            function: 1,
            type_arguments: vec![argument],
        });
        with_list_iter(
            vec![global("keeper", ty, function)],
            vec![keep(iter_bound(Type::I32))],
        )
    };
    assert!(value(Type::list(Type::I32)).is_ok());
    assert!(matches!(
        value(Type::I32),
        Err(IrError::InvalidExpression(_))
    ));
}

/// A contract call on a type parameter needs the parameter's bound.
#[test]
fn v3_an_open_contract_call_needs_bound_evidence() {
    let generic = |bound: Option<Bound>| {
        let result = option(Type::Tuple(vec![Type::I32, Type::Binder(0)]));
        let record = SignatureRecord::new(
            vec![Binder::new("T", bound).unwrap()],
            Type::function(FnType::new(vec![Type::Binder(0)], result)),
        )
        .unwrap();
        let body = contract(
            iter_id(),
            "next",
            Type::Binder(0),
            named(vec![("Item", Type::I32)]),
            variable(0, Type::Binder(0)),
        );
        with_list_iter(Vec::new(), vec![function("first", record, body, 1)])
    };
    assert!(generic(Some(iter_bound(Type::I32))).is_ok());
    assert!(matches!(generic(None), Err(IrError::InvalidExpression(_))));
    assert!(matches!(
        generic(Some(iter_bound(Type::U8))),
        Err(IrError::InvalidExpression(_))
    ));
}

fn ordered_id() -> TypeId {
    id(path(Library::Ordered))
}

fn ordering_type() -> Type {
    library_type(Library::Ordering, Vec::new())
}

/// The library parts with `Ordered` bound, whose `compare` is
/// `Fn({Self Self} Ordering)`, and `impl(T: Ordered) Ordered for List(T)`,
/// whose `compare` is function 0.
fn ordered_parts() -> ModuleSetParts {
    let compare = SignatureRecord::interface_member(
        Vec::new(),
        Some(0),
        Type::function(FnType::new(
            vec![Type::SelfType, Type::SelfType],
            ordering_type(),
        )),
        Vec::new(),
    )
    .unwrap();
    let ordered = InterfaceDefinition::new(
        ordered_id(),
        Vec::new(),
        Vec::new(),
        vec![InterfaceMember {
            name: "compare".to_owned(),
            signature: compare,
            default: None,
        }],
    )
    .unwrap();
    let element = || {
        Binder::new(
            "T",
            Some(Bound {
                contract: InterfaceKey::new(ordered_id(), Vec::new()),
                choices: NamedTypes::empty(),
            }),
        )
        .unwrap()
    };
    let list = Type::list(Type::Binder(0));
    let equal = node(Expr::Construct {
        value_type: ordering_type(),
        construction: Construction::Case {
            case: "equal".to_owned(),
            fields: Vec::new(),
        },
    });
    let compare_lists = function(
        "compareLists",
        SignatureRecord::new(
            vec![element()],
            Type::function(FnType::new(
                vec![list.clone(), list.clone()],
                ordering_type(),
            )),
        )
        .unwrap(),
        equal,
        2,
    );
    let implementation = Implementation::new(
        ImplementationRecord::new(
            vec![element()],
            InterfaceKey::new(ordered_id(), Vec::new()),
            list,
            NamedTypes::empty(),
        )
        .unwrap(),
        vec![("compare".to_owned(), 0)],
    )
    .unwrap();
    let mut parts = library_parts();
    parts.library = library().with(Library::Ordered, ordered_id());
    parts.interfaces.push(ordered);
    parts.functions.push(compare_lists);
    parts.implementations.push(implementation);
    parts
}

fn dict_type(key: Type) -> Type {
    library_type(Library::Dict, vec![key, Type::I32])
}

fn empty_dict(key: Type) -> CheckedGlobal {
    global(
        "table",
        dict_type(key.clone()),
        node(Expr::Construct {
            value_type: dict_type(key),
            construction: Construction::Dict(Vec::new()),
        }),
    )
}

/// A dict key implements `Ordered`: structurally, by an implementation, or
/// by an implementation whose binder bounds hold in turn.
#[test]
fn v3_a_dict_key_implements_ordered() {
    let set = |key: Type| {
        let mut parts = ordered_parts();
        parts.globals = vec![empty_dict(key)];
        CheckedModuleSet::try_new(parts)
    };
    let atoms = Type::Tuple(vec![Type::Atom, Type::AtomSingleton("on".to_owned())]);
    assert!(set(atoms.clone()).is_ok());
    assert!(set(Type::list(atoms)).is_ok());
    assert!(matches!(set(Type::I32), Err(IrError::InvalidType(_))));
    assert!(matches!(
        set(Type::list(Type::F32)),
        Err(IrError::InvalidType(_))
    ));
    assert!(matches!(
        set(Type::Tuple(vec![Type::Atom, Type::F64])),
        Err(IrError::InvalidType(_))
    ));
}

/// A lookup compares keys too.
#[test]
fn v3_a_lookup_needs_an_ordered_key() {
    let lookup = |key: Type, probe: Node| {
        let table = node(Expr::Construct {
            value_type: dict_type(key.clone()),
            construction: Construction::Dict(Vec::new()),
        });
        let mut parts = ordered_parts();
        parts.globals = vec![global(
            "found",
            option(Type::I32),
            node(Expr::Lookup {
                collection: Box::new(table),
                key: Box::new(probe),
            }),
        )];
        CheckedModuleSet::try_new(parts)
    };
    assert!(
        lookup(Type::Atom, literal(Type::Atom, Value::Atom("a".to_owned()))).is_ok()
    );
    assert!(lookup(Type::I32, int(1)).is_err());
}

// -- V5: a supplied member over a default body --------------------------------

fn describe_id() -> TypeId {
    id("test.describe.Describe")
}

/// `Fn({Self} I32)`, a member with a receiver slot.
fn receiver_member() -> SignatureRecord {
    SignatureRecord::interface_member(
        Vec::new(),
        Some(0),
        Type::function(FnType::new(vec![Type::SelfType], Type::I32)),
        Vec::new(),
    )
    .unwrap()
}

/// `Describe` with `get` and the default `twice` (function 1), implemented
/// for `I32` by `get` (function 0) and, when `supply_twice`, by `twice`
/// (function 2) as well.
fn describe_parts(supply_twice: bool) -> ModuleSetParts {
    let own = || signature(vec![Type::I32], Type::I32);
    let interface = InterfaceDefinition::new(
        describe_id(),
        Vec::new(),
        Vec::new(),
        vec![
            InterfaceMember {
                name: "get".to_owned(),
                signature: receiver_member(),
                default: None,
            },
            InterfaceMember {
                name: "twice".to_owned(),
                signature: receiver_member(),
                default: Some(1),
            },
        ],
    )
    .unwrap();
    let mut members = vec![("get".to_owned(), 0)];
    if supply_twice {
        members.push(("twice".to_owned(), 2));
    }
    let implementation = Implementation::new(
        ImplementationRecord::new(
            Vec::new(),
            InterfaceKey::new(describe_id(), Vec::new()),
            Type::I32,
            NamedTypes::empty(),
        )
        .unwrap(),
        members,
    )
    .unwrap();
    let mut parts = library_parts();
    parts.interfaces.push(interface);
    parts.functions = vec![
        function("get", own(), variable(0, Type::I32), 1),
        function("twice", receiver_member(), int(2), 1),
        function("twice", own(), int(3), 1),
    ];
    parts.implementations.push(implementation);
    parts
}

#[test]
fn v5_an_implementation_does_not_supply_a_default_member() {
    assert!(CheckedModuleSet::try_new(describe_parts(false)).is_ok());
    assert!(matches!(
        CheckedModuleSet::try_new(describe_parts(true)),
        Err(IrError::InvalidType(_))
    ));
}

// -- V6: finite size ----------------------------------------------------------

fn record_type(
    name: &str,
    parameters: &[&str],
    fields: Vec<(&str, Type)>,
) -> TypeDefinition {
    TypeDefinition::new(
        id(name),
        parameters.iter().map(|name| (*name).to_owned()).collect(),
        TypeBody::Record(
            fields
                .into_iter()
                .map(|(name, ty)| (name.to_owned(), ty))
                .collect(),
        ),
        None,
        Visibility::Public,
    )
    .unwrap()
}

fn with_types(types: Vec<TypeDefinition>) -> Result<Arc<CheckedModuleSet>, IrError> {
    let mut parts = library_parts();
    parts.types.extend(types);
    CheckedModuleSet::try_new(parts)
}

fn declared_at(name: &str, arguments: Vec<Type>) -> Type {
    Type::Declared(id(name), arguments)
}

/// A declaration that holds itself inline, directly, through a variant, or
/// through a parameter that another declaration holds inline, has no finite
/// size.
#[test]
fn v6_a_declaration_holding_itself_inline_is_rejected() {
    let direct = record_type(
        "test.t.Loop",
        &[],
        vec![("next", declared_at("test.t.Loop", vec![]))],
    );
    assert!(matches!(
        with_types(vec![direct]),
        Err(IrError::InvalidType(_))
    ));
    let optional = record_type(
        "test.t.Chain",
        &[],
        vec![("next", option(declared_at("test.t.Chain", vec![])))],
    );
    assert!(matches!(
        with_types(vec![optional]),
        Err(IrError::InvalidType(_))
    ));
    let holder = || record_type("test.t.Box", &["T"], vec![("value", Type::Binder(0))]);
    let through = record_type(
        "test.t.Bad",
        &[],
        vec![(
            "inner",
            Type::Tuple(vec![
                Type::I32,
                declared_at("test.t.Box", vec![declared_at("test.t.Bad", vec![])]),
            ]),
        )],
    );
    assert!(matches!(
        with_types(vec![holder(), through]),
        Err(IrError::InvalidType(_))
    ));
    let mutual = vec![
        record_type(
            "test.t.A",
            &[],
            vec![("b", declared_at("test.t.B", vec![]))],
        ),
        record_type(
            "test.t.B",
            &[],
            vec![("a", declared_at("test.t.A", vec![]))],
        ),
    ];
    assert!(matches!(with_types(mutual), Err(IrError::InvalidType(_))));
}

/// Recursion through a list, a dict, or a callable type has a finite size.
#[test]
fn v6_recursion_through_an_indirection_is_accepted() {
    let tree = record_type(
        "test.t.Tree",
        &[],
        vec![("children", Type::list(declared_at("test.t.Tree", vec![])))],
    );
    assert!(with_types(vec![tree]).is_ok());
    let table = record_type(
        "test.t.Table",
        &[],
        vec![(
            "rows",
            library_type(
                Library::Dict,
                vec![Type::Atom, declared_at("test.t.Table", vec![])],
            ),
        )],
    );
    assert!(with_types(vec![table]).is_ok());
    let lazy = record_type(
        "test.t.Lazy",
        &[],
        vec![(
            "force",
            Type::function(FnType::new(Vec::new(), declared_at("test.t.Lazy", vec![]))),
        )],
    );
    assert!(with_types(vec![lazy]).is_ok());
    let holder = record_type("test.t.Box", &["T"], vec![("value", Type::Binder(0))]);
    let boxed = record_type(
        "test.t.Good",
        &[],
        vec![(
            "inner",
            declared_at(
                "test.t.Box",
                vec![Type::list(declared_at("test.t.Good", vec![]))],
            ),
        )],
    );
    assert!(with_types(vec![holder, boxed]).is_ok());
}

// -- V9: defaults travel with the callable value ------------------------------

/// `Fn({I32 step: I32} I32)`.
fn stepping() -> Type {
    Type::function(FnType {
        parameters: vec![Type::I32],
        labels: named(vec![("step", Type::I32)]),
        result: Type::I32,
    })
}

fn step_default() -> Vec<(String, Constant)> {
    vec![(
        "step".to_owned(),
        Constant::new(Type::I32, Value::I32(5)).unwrap(),
    )]
}

/// A closure of `stepping()` with `defaults`, called through its value with
/// the label unwritten.
fn closure_call(
    defaults: Vec<(String, Constant)>,
) -> Result<Arc<CheckedModuleSet>, IrError> {
    let stepper = node(Expr::Closure {
        closure: Arc::new(vibra_ir::ClosureBody {
            value_type: stepping(),
            slot_count: 2,
            captures: Vec::new(),
            defaults,
            body: variable(1, Type::I32),
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
    let mut parts = library_parts();
    parts.globals = vec![global("stepped", Type::I32, call)];
    CheckedModuleSet::try_new(parts)
}

#[test]
fn v9_a_closure_carries_a_default_for_each_label() {
    assert!(closure_call(step_default()).is_ok());
    assert!(matches!(
        closure_call(Vec::new()),
        Err(IrError::InvalidExpression(_))
    ));
}

#[test]
fn v9_a_function_carries_a_default_for_each_label() {
    let set = |defaults: Vec<(String, Constant)>| {
        let record = SignatureRecord::new(Vec::new(), stepping()).unwrap();
        let mut parts = library_parts();
        parts.functions = vec![CheckedFunction {
            defaults,
            ..function("step", record, variable(1, Type::I32), 2)
        }];
        CheckedModuleSet::try_new(parts)
    };
    assert!(set(step_default()).is_ok());
    assert!(matches!(
        set(Vec::new()),
        Err(IrError::InvalidExpression(_))
    ));
}
