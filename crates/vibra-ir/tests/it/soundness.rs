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
        context: vibra_ir::BodyContext::Pure,
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
        parameters
            .iter()
            .map(|name| vibra_ir::Binder::new(*name, None).unwrap())
            .collect(),
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

// -- V10: a slot count allocates nothing in validation --------------------------

/// The largest slot count validates, and its slots are still bounded by it.
#[test]
fn v10_the_largest_slot_count_validates_without_allocating() {
    let set = |body: Node| {
        let mut parts = library_parts();
        parts.functions = vec![function(
            "wide",
            signature(Vec::new(), Type::I32),
            body,
            usize::MAX,
        )];
        CheckedModuleSet::try_new(parts)
    };
    assert!(set(int(1)).is_ok());
    assert!(matches!(
        set(variable(usize::MAX - 1, Type::I32)),
        Err(IrError::UnsetSlot(_))
    ));
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
            entry_guard: None,
            exit_guard: None,
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

// -- V12: a closure's guards ----------------------------------------------------

/// A closure of `Fn({I32} I32)` returning its argument, with the `requires`
/// and the `ensures` given, applied to `1i32`. Its slot 0 is the argument
/// and its slot 1 the candidate of the `ensures`.
fn guarded_closure(
    entry_guard: Option<vibra_ir::Guard>,
    exit_guard: Option<vibra_ir::ExitGuard>,
) -> Result<Arc<CheckedModuleSet>, IrError> {
    let closure = node(Expr::Closure {
        closure: Arc::new(vibra_ir::ClosureBody {
            value_type: Type::function(FnType::new(vec![Type::I32], Type::I32)),
            slot_count: 2,
            captures: Vec::new(),
            defaults: Vec::new(),
            body: variable(0, Type::I32),
            entry_guard,
            exit_guard,
        }),
        captures: Vec::new(),
    });
    let call = node(Expr::Call {
        target: CallTarget::Value(Box::new(closure)),
        arguments: positional(vec![int(1)]),
    });
    let mut parts = library_parts();
    parts.globals = vec![global("guarded", Type::I32, call)];
    CheckedModuleSet::try_new(parts)
}

fn truth() -> Node {
    literal(crate::bool_type(), Value::bool(true))
}

/// A closure's guards are checked as a function's are: a `Bool` predicate
/// and a consequence of the result type, in the closure's activation, the
/// `ensures` with its candidate bound.
#[test]
fn v12_a_closure_guard_is_checked_in_its_activation() {
    let guard = |predicate: Node, consequence: Node| vibra_ir::Guard {
        predicate,
        consequence,
    };
    let exit = |candidate: usize, guard| vibra_ir::ExitGuard { candidate, guard };
    assert!(
        guarded_closure(
            Some(guard(truth(), variable(0, Type::I32))),
            Some(exit(1, guard(truth(), variable(1, Type::I32)))),
        )
        .is_ok()
    );
    assert!(matches!(
        guarded_closure(Some(guard(int(1), int(0))), None),
        Err(IrError::TypeMismatch { .. })
    ));
    assert!(matches!(
        guarded_closure(Some(guard(truth(), truth())), None),
        Err(IrError::TypeMismatch { .. })
    ));
    // The candidate is not bound in the `requires`, and an `ensures` reads
    // only slots of the closure.
    assert!(matches!(
        guarded_closure(Some(guard(truth(), variable(1, Type::I32))), None),
        Err(IrError::UnsetSlot(_))
    ));
    assert!(guarded_closure(None, Some(exit(2, guard(truth(), int(0))))).is_err());
    assert!(
        guarded_closure(None, Some(exit(1, guard(truth(), variable(5, Type::I32)))))
            .is_err()
    );
}

// -- V11: no host stack in proportion to depth ---------------------------------

/// A depth far beyond what host recursion on a small stack survives.
const DEEP: usize = 100_000;

/// Runs `work` on a thread whose stack is 256 KiB.
fn on_small_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(work)
        .expect("a thread")
        .join()
        .expect("the thread finished")
}

fn void() -> Node {
    literal(Type::Void, Value::Void)
}

/// `depth` closures of `Fn({} Void)`, each the first statement of the
/// body of the one around it.
fn nested_closures(depth: usize) -> Node {
    let mut body = void();
    for _ in 0..depth {
        let closure = node(Expr::Closure {
            closure: Arc::new(vibra_ir::ClosureBody {
                value_type: Type::function(FnType::new(Vec::new(), Type::Void)),
                slot_count: 0,
                captures: Vec::new(),
                defaults: Vec::new(),
                body,
                entry_guard: None,
                exit_guard: None,
            }),
            captures: Vec::new(),
        });
        body = node(Expr::Sequence(vec![closure, void()]));
    }
    body
}

/// A body of 100,000 nested closures is checked, its facts gathered with
/// every closure numbered, and the set dropped, on a small stack.
#[test]
fn v11_a_body_nested_100_000_deep_validates_on_a_small_stack() {
    let closures = on_small_stack(|| {
        let mut parts = library_parts();
        parts.functions = vec![function(
            "main",
            signature(Vec::new(), Type::Void),
            nested_closures(DEEP),
            0,
        )];
        CheckedModuleSet::try_new(parts).map(|set| set.flow().closures.len())
    });
    assert_eq!(closures, Ok(DEEP));
}

/// A sequence of `length` statements of one kind, each binding slot 0 for
/// the rest of the sequence, which ends by reading it: `let`s, `let …
/// else`s whose fallback returns, or destructuring `let`s, each a `match` of
/// one arm.
fn spine(kind: &str, length: usize) -> Node {
    let mut rest = variable(0, Type::I32);
    for k in 0..length {
        let value = Box::new(int(i32::try_from(k).expect("a small length")));
        let body = Box::new(rest);
        rest = node(match kind {
            "let" => Expr::Let {
                slot: Some(0),
                value,
                body,
            },
            "letElse" => Expr::LetElse {
                pattern: vibra_ir::Pattern::Bind(0),
                value,
                fallback: Box::new(node(Expr::Return(Box::new(int(0))))),
                body,
            },
            _ => Expr::Match {
                subject: value,
                arms: vec![vibra_ir::MatchArm {
                    pattern: vibra_ir::Pattern::Bind(0),
                    body: *body,
                }],
                value_type: Type::I32,
            },
        });
    }
    rest
}

/// The length of a sequence is not nesting: a body of 100,000 sequential
/// `let`s, `let … else`s, or destructuring `let`s is validated, cloned,
/// compared, written for debugging, and dropped on a small stack, each walk
/// a loop down the rest of the sequence.
#[test]
fn v11_a_sequence_100_000_long_costs_no_stack() {
    for kind in ["let", "letElse", "match"] {
        let (valid, same, written) = on_small_stack(move || {
            let body = spine(kind, DEEP);
            let copy = body.clone();
            let same = copy == body;
            let written = format!("{copy:?}").len();
            let mut parts = library_parts();
            parts.functions =
                vec![function("main", signature(Vec::new(), Type::I32), body, 1)];
            let valid = CheckedModuleSet::try_new(parts).is_ok();
            (valid, same, written)
        });
        assert!(valid, "{kind}: the body validates");
        assert!(same, "{kind}: a clone is equal");
        assert!(written > DEEP, "{kind}: every link is written");
    }
}

/// A clone of a sequence is the sequence, and two sequences that differ in
/// one link, or in their length, differ.
#[test]
fn a_sequence_compares_link_by_link() {
    for kind in ["let", "letElse", "match"] {
        let body = spine(kind, 3);
        assert_eq!(body.clone(), body, "{kind}");
        assert_ne!(spine(kind, 4), body, "{kind}");
        let mut changed = spine(kind, 3);
        let mut link = &mut changed;
        for _ in 0..2 {
            link = match &mut link.expr {
                Expr::Let { body, .. } | Expr::LetElse { body, .. } => body,
                Expr::Match { arms, .. } => &mut arms[0].body,
                _ => unreachable!(),
            };
        }
        match &mut link.expr {
            Expr::Let { value, .. }
            | Expr::LetElse { value, .. }
            | Expr::Match { subject: value, .. } => **value = int(9),
            _ => unreachable!(),
        }
        assert_ne!(changed, body, "{kind}");
        assert_eq!(format!("{:?}", body.clone()), format!("{body:?}"));
    }
}

/// `List` applied `levels` times to `U8`.
fn lists(levels: usize) -> Type {
    (0..levels).fold(Type::U8, |ty, _| Type::list(ty))
}

/// A type nests at most `Type::MAX_DEPTH` levels; a deeper one is rejected
/// before anything recurses over it, and a host-built one 100,000 deep is
/// rejected and dropped on a small stack.
#[test]
fn v11_a_type_nested_deeper_than_the_bound_is_rejected() {
    let empty = |levels: usize| {
        let ty = lists(levels);
        let mut parts = library_parts();
        parts.globals = vec![global(
            "empty",
            ty.clone(),
            node(Expr::Construct {
                value_type: ty,
                construction: Construction::List(Vec::new()),
            }),
        )];
        CheckedModuleSet::try_new(parts)
    };
    assert!(empty(Type::MAX_DEPTH - 1).is_ok());
    assert!(matches!(
        empty(Type::MAX_DEPTH),
        Err(IrError::InvalidType(_))
    ));
    let rejected = on_small_stack(|| {
        matches!(
            Constant::new(lists(DEEP), Value::List(Vec::new())),
            Err(IrError::InvalidType(_))
        )
    });
    assert!(rejected);
}

/// The debug form of a value is the one a derive would write.
#[test]
fn v11_a_value_formats_as_derived() {
    let value = Value::Variant {
        case: "some".to_owned(),
        fields: vec![(
            "value".to_owned(),
            Value::Tuple(vec![
                Value::I8(1),
                Value::Dict(vec![(Value::Str("k".to_owned()), Value::Void)]),
                Value::Interface {
                    held_type: Type::U8,
                    held: Box::new(Value::U8(2)),
                },
                Value::Wrapper(Box::new(Value::List(Vec::new()))),
            ]),
        )],
    };
    assert_eq!(
        format!("{value:?}"),
        "Variant { case: \"some\", fields: [(\"value\", Tuple([I8(1), \
         Dict([(Str(\"k\"), Void)]), Interface { held_type: U8, held: U8(2) }, \
         Wrapper(List([]))]))] }"
    );
}

/// A value, a pattern, an expression tree, and a type each 100,000 deep
/// are built, cloned or formatted, and dropped on a small stack.
#[test]
fn v11_deep_values_patterns_trees_and_types_are_handled_on_a_small_stack() {
    let (same, formatted) = on_small_stack(|| {
        let mut value = Value::Void;
        let mut expected = String::from("Void");
        for _ in 0..DEEP {
            value = Value::Wrapper(Box::new(value));
        }
        expected.insert_str(0, &"Wrapper(".repeat(DEEP));
        expected.push_str(&")".repeat(DEEP));
        let copy = value.clone();
        let same = copy == value;
        let formatted = format!("{copy:?}") == expected;
        let mut pattern = vibra_ir::Pattern::Discard;
        for _ in 0..DEEP {
            pattern = vibra_ir::Pattern::Wrapper(Box::new(pattern));
        }
        let mut tree = void();
        for _ in 0..DEEP {
            tree = node(Expr::Return(Box::new(tree)));
        }
        let ty = lists(DEEP);
        drop((value, copy, pattern, tree, ty));
        (same, formatted)
    });
    assert!(same);
    assert!(formatted);
}

// -- Defaults of a generic function -----------------------------------------

/// `pad(T)` with the labelled parameter `tail: List(T)` whose default is
/// `default`.
fn padded(default: Constant) -> Result<Arc<CheckedModuleSet>, IrError> {
    let record = SignatureRecord::new(
        vec![Binder::new("T", None).unwrap()],
        Type::function(FnType {
            parameters: Vec::new(),
            labels: named(vec![("tail", Type::list(Type::Binder(0)))]),
            result: Type::I32,
        }),
    )
    .unwrap();
    let mut pad = function("pad", record, int(1), 1);
    pad.defaults = vec![("tail".to_owned(), default)];
    let mut parts = library_parts();
    parts.functions.push(pad);
    CheckedModuleSet::try_new(parts)
}

/// A constant is closed: a default whose label's type mentions a binder has
/// that type with every binder `Never`, and no other.
#[test]
fn a_generic_default_is_closed_at_never() {
    let empty =
        |element: Type| Constant::new(Type::list(element), Value::List(Vec::new()));
    assert!(padded(empty(Type::Never).unwrap()).is_ok());
    assert!(matches!(
        padded(empty(Type::I32).unwrap()),
        Err(IrError::TypeMismatch { .. })
    ));
    assert!(empty(Type::Binder(0)).is_err());
}

/// In a member of an interface a default's type may hold `Self` and the
/// association slots too; its closed type has `Never` at each, and at no
/// other position.
#[test]
fn a_default_is_closed_at_never_at_every_open_form() {
    let open = Type::Tuple(vec![
        Type::list(Type::SelfType),
        Type::list(Type::Associated("Item".to_owned())),
        Type::list(Type::Binder(1)),
        Type::I32,
    ]);
    assert_eq!(
        open.at_never(),
        Type::Tuple(vec![
            Type::list(Type::Never),
            Type::list(Type::Never),
            Type::list(Type::Never),
            Type::I32,
        ])
    );
    assert_eq!(Type::list(Type::U8).at_never(), Type::list(Type::U8));
}

/// `dict.fromList` keys its dict as a literal does: a closed key without
/// `Ordered` is rejected, and an atom, which is ordered structurally, keys
/// it.
#[test]
fn a_dict_built_from_a_list_needs_an_ordered_key() {
    let build = |key: Type| {
        let entry = Type::Tuple(vec![key.clone(), Type::I32]);
        let body = node(Expr::Intrinsic {
            intrinsic: CompilerIntrinsic::DictFromList,
            type_arguments: vec![key.clone(), Type::I32],
            arguments: vec![literal(Type::list(entry), Value::List(Vec::new()))],
        });
        let result = library_type(Library::Dict, vec![key, Type::I32]);
        let mut parts = library_parts();
        parts
            .functions
            .push(function("build", signature(Vec::new(), result), body, 0));
        CheckedModuleSet::try_new(parts)
    };
    assert!(build(Type::Atom).is_ok());
    assert!(matches!(build(Type::F64), Err(IrError::InvalidType(_))));
}

/// The bound `Ordered` of the parts [`ordered_parts`] gives.
fn ordered_bound() -> Bound {
    Bound {
        contract: InterfaceKey::new(ordered_id(), Vec::new()),
        choices: NamedTypes::empty(),
    }
}

/// A bounded declared type is applied only at arguments that meet its
/// bound: closed, or a binder whose own bound shows it, in a signature, a
/// type body, and a global; and a bound refers only to the type's
/// parameters.
#[test]
fn a_bounded_declared_type_is_applied_only_where_its_bound_holds() {
    let sorted = || {
        TypeDefinition::new(
            id("test.t.Sorted"),
            vec![Binder::new("T", Some(ordered_bound())).unwrap()],
            TypeBody::Wrapper(Type::list(Type::Binder(0))),
            None,
            Visibility::Public,
        )
        .unwrap()
    };
    let at = |argument: Type| declared_at("test.t.Sorted", vec![argument]);
    let taking = |binder: Option<Option<Bound>>, argument: Type| {
        let mut parts = ordered_parts();
        parts.types.push(sorted());
        let binders = binder
            .map(|bound| vec![Binder::new("T", bound).unwrap()])
            .unwrap_or_default();
        let record = SignatureRecord::new(
            binders,
            Type::function(FnType::new(vec![at(argument)], Type::I32)),
        )
        .unwrap();
        parts.functions.push(function("f", record, int(0), 1));
        CheckedModuleSet::try_new(parts)
    };
    assert!(taking(None, Type::Atom).is_ok());
    assert!(taking(None, Type::list(Type::Atom)).is_ok());
    assert!(matches!(
        taking(None, Type::F32),
        Err(IrError::InvalidExpression(_))
    ));
    assert!(taking(Some(Some(ordered_bound())), Type::Binder(0)).is_ok());
    assert!(matches!(
        taking(Some(None), Type::Binder(0)),
        Err(IrError::InvalidExpression(_))
    ));
    let holding = |argument: Type| {
        let mut parts = ordered_parts();
        parts.types.push(sorted());
        parts
            .types
            .push(record_type("test.t.Holder", &[], vec![("s", at(argument))]));
        CheckedModuleSet::try_new(parts)
    };
    assert!(holding(Type::Atom).is_ok());
    assert!(matches!(
        holding(Type::F64),
        Err(IrError::InvalidExpression(_))
    ));
    let held = |argument: Type| {
        let mut parts = ordered_parts();
        parts.types.push(sorted());
        let value_type = at(argument.clone());
        parts.globals.push(global(
            "empty",
            value_type.clone(),
            node(Expr::Construct {
                value_type,
                construction: Construction::Wrapper(Box::new(node(Expr::Construct {
                    value_type: Type::list(argument),
                    construction: Construction::List(Vec::new()),
                }))),
            }),
        ));
        CheckedModuleSet::try_new(parts)
    };
    assert!(held(Type::Atom).is_ok());
    assert!(matches!(
        held(Type::F32),
        Err(IrError::InvalidExpression(_))
    ));
    let outside = Bound {
        contract: InterfaceKey::new(ordered_id(), vec![Type::Binder(3)]),
        choices: NamedTypes::empty(),
    };
    assert!(
        TypeDefinition::new(
            id("test.t.Bad"),
            vec![Binder::new("T", Some(outside)).unwrap()],
            TypeBody::Wrapper(Type::Binder(0)),
            None,
            Visibility::Public,
        )
        .is_err()
    );
}

/// A call of `dict.fromList` at a key that stays open in the caller keys a
/// dict exactly when the caller's bound on that parameter shows `Ordered`.
#[test]
fn an_open_key_of_a_dict_built_from_a_list_reads_the_callers_bound() {
    let build = |bound: Option<Bound>| {
        let entries = |key: Type| Type::list(Type::Tuple(vec![key, Type::Binder(1)]));
        let generic = SignatureRecord::new(
            vec![
                Binder::new("K", None).unwrap(),
                Binder::new("V", None).unwrap(),
            ],
            Type::function(FnType::new(
                vec![entries(Type::Binder(0))],
                library_type(Library::Dict, vec![Type::Binder(0), Type::Binder(1)]),
            )),
        )
        .unwrap();
        let mut parts = ordered_parts();
        let convert = parts.functions.len();
        parts.functions.push(CheckedFunction::external(
            "convert",
            generic,
            CompilerIntrinsic::DictFromList,
            vec![Type::Binder(0), Type::Binder(1)],
            origin(),
        ));
        let entry = Type::Tuple(vec![Type::Binder(0), Type::I32]);
        let call = node(Expr::Call {
            target: CallTarget::Direct {
                function: convert,
                type_arguments: vec![Type::Binder(0), Type::I32],
            },
            arguments: positional(vec![node(Expr::Construct {
                value_type: Type::list(entry),
                construction: Construction::List(Vec::new()),
            })]),
        });
        let result = library_type(Library::Dict, vec![Type::Binder(0), Type::I32]);
        let record = SignatureRecord::new(
            vec![Binder::new("K", bound).unwrap()],
            Type::function(FnType::new(Vec::new(), result)),
        )
        .unwrap();
        parts.functions.push(function("build", record, call, 0));
        CheckedModuleSet::try_new(parts)
    };
    assert!(build(Some(ordered_bound())).is_ok());
    assert!(matches!(build(None), Err(IrError::InvalidType(_))));
}

/// A function whose body is `dict.fromList` at its own binders keys no dict
/// at a closed key without `Ordered`, wherever a call instantiates it.
#[test]
fn an_instantiation_of_a_dict_built_from_a_list_needs_an_ordered_key() {
    let build = |key: Type| {
        let entries = |key: Type| Type::list(Type::Tuple(vec![key, Type::Binder(1)]));
        let generic = SignatureRecord::new(
            vec![
                Binder::new("K", None).unwrap(),
                Binder::new("V", None).unwrap(),
            ],
            Type::function(FnType::new(
                vec![entries(Type::Binder(0))],
                library_type(Library::Dict, vec![Type::Binder(0), Type::Binder(1)]),
            )),
        )
        .unwrap();
        let convert = CheckedFunction::external(
            "convert",
            generic,
            CompilerIntrinsic::DictFromList,
            vec![Type::Binder(0), Type::Binder(1)],
            origin(),
        );
        let entry = Type::Tuple(vec![key.clone(), Type::I32]);
        let call = node(Expr::Call {
            target: CallTarget::Direct {
                function: 0,
                type_arguments: vec![key.clone(), Type::I32],
            },
            arguments: positional(vec![literal(
                Type::list(entry),
                Value::List(Vec::new()),
            )]),
        });
        let result = library_type(Library::Dict, vec![key, Type::I32]);
        let mut parts = library_parts();
        parts.functions.push(convert);
        parts
            .functions
            .push(function("build", signature(Vec::new(), result), call, 0));
        CheckedModuleSet::try_new(parts)
    };
    assert!(build(Type::Atom).is_ok());
    assert!(matches!(build(Type::F64), Err(IrError::InvalidType(_))));
}
