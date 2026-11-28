//! The effect rules a checked module set keeps (`docs/spec/03-effects.md`,
//! "Effect ceilings", "Cold plans and performed rows", "Operation
//! interfaces", "Capability values"; `docs/spec/06-runtime.md`, "What has no
//! encoding", "Audit trace"), over hand-built module sets.
//!
//! `I` is an interface with one `proc` member `p: Proc({Self} Void {I})`,
//! `J` another, and `K` an interface with only a `fn` member.

use std::sync::Arc;

use vibra_diagnostics::ByteSpan;
use vibra_ir::external::host::HostOp;
use vibra_ir::{
    Arguments, BodyContext, CallTarget, Case, CheckedFunction, CheckedGlobal,
    CheckedModuleSet, CheckedProgram, ClosureBody, Constant, Construction,
    ContractCall, EffectFault, Expr, FunctionBody, HostBinding, Implementation,
    ImplementationRecord, InterfaceDefinition, InterfaceKey, InterfaceMember, IrError,
    Library, ModuleSetParts, NamedTypes, Node, ProcType, Row, SignatureRecord,
    SourceOrigin, Type, TypeBody, TypeDefinition, Value, Visibility,
};

use crate::{
    fn_type, id, library_parts, library_type, node, on_small_stack, origin, str_type,
    variable, void,
};

const I: &str = "test.effects.I";
const J: &str = "test.effects.J";
const K: &str = "test.effects.K";
const DIR_READ_TEXT: &str = "std.fs.DirReadText";
const DIR_LIST: &str = "std.fs.DirList";

fn row(paths: &[&str]) -> Row {
    Row::new(paths.iter().map(|path| id(path).entity().clone())).unwrap()
}

fn interface_type(path: &str) -> Type {
    Type::Interface(id(path), Vec::new(), NamedTypes::empty())
}

fn proc_type(parameters: Vec<Type>, result: Type, effects: Row) -> Type {
    Type::Proc(Box::new(ProcType {
        parameters,
        labels: NamedTypes::empty(),
        result,
        effects,
    }))
}

/// An operation interface at `path` with one abstract member `p:
/// Proc({Self} Void {path})`.
fn operation_interface(path: &str) -> InterfaceDefinition {
    let member = SignatureRecord::interface_member(
        Vec::new(),
        Some(0),
        proc_type(vec![Type::SelfType], Type::Void, row(&[path])),
        Vec::new(),
    )
    .unwrap();
    InterfaceDefinition::new(
        id(path),
        Vec::new(),
        Vec::new(),
        vec![InterfaceMember {
            name: "p".to_owned(),
            signature: member,
            default: None,
        }],
    )
    .unwrap()
}

/// `K`: one `fn` member, so no root.
fn pure_interface() -> InterfaceDefinition {
    let member = SignatureRecord::interface_member(
        Vec::new(),
        Some(0),
        fn_type(vec![Type::SelfType], Type::Void),
        Vec::new(),
    )
    .unwrap();
    InterfaceDefinition::new(
        id(K),
        Vec::new(),
        Vec::new(),
        vec![InterfaceMember {
            name: "f".to_owned(),
            signature: member,
            default: None,
        }],
    )
    .unwrap()
}

fn function(name: &str, ty: Type, body: Node, slot_count: usize) -> CheckedFunction {
    CheckedFunction {
        name: name.to_owned(),
        signature: SignatureRecord::new(Vec::new(), ty).unwrap(),
        defaults: Vec::new(),
        body: FunctionBody::Expr(Box::new(body)),
        entry_guard: None,
        exit_guard: None,
        slot_count,
        origin: origin(),
        context: BodyContext::Pure,
    }
}

fn await_node(plan: Node) -> Node {
    node(Expr::Await {
        plan: Box::new(plan),
    })
}

fn call(target: CallTarget, positional: Vec<Node>) -> Node {
    node(Expr::Call {
        target,
        arguments: Arguments {
            positional,
            labelled: Vec::new(),
        },
    })
}

/// `interface.p(slot)` through an interface value: a plan of `{interface}`.
fn build_p(interface: &str, slot: usize) -> Node {
    call(
        CallTarget::Contract(Box::new(ContractCall {
            interface: id(interface),
            member: "p".to_owned(),
            receiver: interface_type(interface),
            arguments: Vec::new(),
            choices: NamedTypes::empty(),
            member_arguments: Vec::new(),
        })),
        vec![variable(slot, interface_type(interface))],
    )
}

fn parts(functions: Vec<CheckedFunction>) -> ModuleSetParts {
    let mut parts = library_parts();
    parts.interfaces.extend([
        operation_interface(I),
        operation_interface(J),
        pure_interface(),
    ]);
    parts.functions = functions;
    parts
}

fn build(functions: Vec<CheckedFunction>) -> Result<Arc<CheckedModuleSet>, IrError> {
    CheckedModuleSet::try_new(parts(functions))
}

#[track_caller]
fn effect_fault(
    result: Result<Arc<CheckedModuleSet>, IrError>,
    owner: &str,
) -> EffectFault {
    match result {
        Err(IrError::Effect {
            owner: Some(found),
            fault,
        }) => {
            assert_eq!(found, owner, "the failure names its function");
            *fault
        }
        other => panic!("expected an effect failure in `{owner}`, got {other:?}"),
    }
}

/// 03 Cold plans: "`await` is valid only in a `proc` body and in a `test`
/// body".
#[test]
fn await_in_a_procedure() {
    let body = await_node(build_p(I, 0));
    let f = function(
        "f",
        proc_type(vec![interface_type(I)], Type::Void, row(&[I])),
        body,
        1,
    );
    build(vec![f]).unwrap();
}

/// The same: a test body runs in a procedure context the runner owns.
#[test]
fn await_in_a_test_context() {
    let plan = function(
        "plan",
        proc_type(Vec::new(), Type::Void, Row::empty()),
        void(),
        0,
    );
    let mut test = function(
        "test",
        fn_type(Vec::new(), Type::Void),
        await_node(call(
            CallTarget::Direct {
                function: 0,
                type_arguments: Vec::new(),
            },
            Vec::new(),
        )),
        0,
    );
    test.context = BodyContext::Procedure(Row::empty());
    build(vec![plan, test]).unwrap();
}

/// `ATK-ir-await-in-fn`: the same body in a `Fn` with a pure context.
#[test]
fn atk_ir_await_in_fn() {
    let f = function(
        "f",
        fn_type(vec![interface_type(I)], Type::Void),
        await_node(build_p(I, 0)),
        1,
    );
    assert_eq!(
        effect_fault(build(vec![f]), "f"),
        EffectFault::AwaitOutsideProcedure
    );
}

/// 06 Cold plans: "It never runs inside a `fn`": a `Fn` closure inside a
/// procedure runs in no procedure context.
#[test]
fn await_in_a_fn_closure_inside_a_procedure() {
    let plan = function(
        "plan",
        proc_type(Vec::new(), Type::Void, Row::empty()),
        void(),
        0,
    );
    let closure = ClosureBody {
        value_type: fn_type(Vec::new(), Type::Void),
        slot_count: 0,
        captures: Vec::new(),
        defaults: Vec::new(),
        body: await_node(call(
            CallTarget::Direct {
                function: 0,
                type_arguments: Vec::new(),
            },
            Vec::new(),
        )),
        entry_guard: None,
        exit_guard: None,
    };
    let body = node(Expr::Let {
        slot: None,
        value: Box::new(node(Expr::Closure {
            closure: Arc::new(closure),
            captures: Vec::new(),
        })),
        body: Box::new(void()),
    });
    let f = function(
        "f",
        proc_type(vec![interface_type(I)], Type::Void, row(&[I])),
        body,
        1,
    );
    assert_eq!(
        effect_fault(build(vec![plan, f]), "f"),
        EffectFault::AwaitOutsideProcedure
    );
}

/// 03 Cold plans: "`await plan` runs the plan".
#[test]
fn await_of_a_non_plan() {
    let one = node(Expr::Literal(
        Constant::new(Type::I32, Value::I32(1)).unwrap(),
    ));
    let f = function(
        "f",
        proc_type(Vec::new(), Type::Void, Row::empty()),
        await_node(one),
        0,
    );
    assert_eq!(
        effect_fault(build(vec![f]), "f"),
        EffectFault::NotAPlan(Box::new(Type::I32))
    );
}

/// 03 Effect ceilings: "requires it to be a subset of the ceiling".
#[test]
fn row_contained() {
    let body = node(Expr::Sequence(vec![
        await_node(build_p(I, 0)),
        await_node(build_p(J, 1)),
    ]));
    let f = function(
        "f",
        proc_type(
            vec![interface_type(I), interface_type(J)],
            Type::Void,
            row(&[I, J]),
        ),
        body,
        2,
    );
    build(vec![f]).unwrap();
}

/// `ATK-ir-await-outside-row`: a plan of `{I}` awaited under `{}`.
#[test]
fn atk_ir_await_outside_row() {
    let f = function(
        "f",
        proc_type(vec![interface_type(I)], Type::Void, Row::empty()),
        await_node(build_p(I, 0)),
        1,
    );
    assert_eq!(
        effect_fault(build(vec![f]), "f"),
        EffectFault::OutsideCeiling {
            root: id(I).entity().clone(),
            ceiling: Row::empty(),
        }
    );
}

/// 03 Cold plans: "`await plan` contributes the plan's complete row": a
/// callback's row is in its type.
#[test]
fn row_of_a_callback() {
    let callback = proc_type(Vec::new(), Type::Void, row(&[J]));
    let body = await_node(call(
        CallTarget::Value(Box::new(variable(0, callback.clone()))),
        Vec::new(),
    ));
    let f = function(
        "f",
        proc_type(vec![callback], Type::Void, row(&[I])),
        body,
        1,
    );
    assert_eq!(
        effect_fault(build(vec![f]), "f"),
        EffectFault::OutsideCeiling {
            root: id(J).entity().clone(),
            ceiling: row(&[I]),
        }
    );
}

/// 03 Effect ceilings: "A `fn` performs nothing. It may build and return a
/// plan".
#[test]
fn building_contributes_nothing() {
    let plan = Type::Intent(Box::new(vibra_ir::IntentType {
        result: Type::Void,
        effects: row(&[I]),
    }));
    let f = function(
        "f",
        fn_type(vec![interface_type(I)], plan),
        build_p(I, 0),
        1,
    );
    build(vec![f]).unwrap();
}

/// `ATK-ir-row-widened-at-call`: 03 Effect ceilings, "Callable types
/// compare rows exactly".
#[test]
fn atk_ir_row_widened_at_call() {
    let narrow = proc_type(Vec::new(), Type::Void, row(&[I]));
    let wide = proc_type(Vec::new(), Type::Void, row(&[I, J]));
    let takes = function(
        "takes",
        fn_type(vec![narrow.clone()], Type::Void),
        void(),
        1,
    );
    let passes = function(
        "passes",
        fn_type(vec![wide.clone()], Type::Void),
        call(
            CallTarget::Direct {
                function: 0,
                type_arguments: Vec::new(),
            },
            vec![variable(0, wide.clone())],
        ),
        1,
    );
    assert_eq!(
        build(vec![takes, passes]).unwrap_err(),
        IrError::TypeMismatch {
            expected: Box::new(narrow),
            actual: Box::new(wide),
        }
    );
}

/// 03 Operation interfaces: "An interface with only `fn` members is not a
/// root".
#[test]
fn a_row_entry_that_is_no_root() {
    let f = function("f", proc_type(Vec::new(), Type::Void, row(&[K])), void(), 0);
    assert_eq!(
        effect_fault(build(vec![f]), "f"),
        EffectFault::NotARoot(id(K).entity().clone())
    );
}

/// A procedure context on a `Proc` function states its type's row, or none
/// is valid.
#[test]
fn a_procedure_context_that_states_another_row() {
    let mut f = function(
        "f",
        proc_type(vec![interface_type(I)], Type::Void, row(&[I])),
        await_node(build_p(I, 0)),
        1,
    );
    f.context = BodyContext::Procedure(row(&[I, J]));
    assert_eq!(
        effect_fault(build(vec![f]), "f"),
        EffectFault::ContextRow {
            typed: row(&[I]),
            stated: row(&[I, J]),
        }
    );
}

// ----- host bindings -----

fn variant(path: &str, cases: &[&str]) -> TypeDefinition {
    TypeDefinition::new(
        id(path),
        Vec::new(),
        TypeBody::Variant(
            cases
                .iter()
                .map(|case| Case::new(*case, Vec::new()))
                .collect(),
        ),
        None,
        Visibility::Public,
    )
    .unwrap()
}

fn relative_path() -> TypeDefinition {
    TypeDefinition::new(
        id("std.fs.RelativePath"),
        Vec::new(),
        TypeBody::Wrapper(Type::list(str_type())),
        None,
        Visibility::Private,
    )
    .unwrap()
}

fn fs_error() -> Type {
    Type::Declared(id("std.fs.FsError"), Vec::new())
}

fn read_result() -> Type {
    library_type(Library::Result, vec![str_type(), fs_error()])
}

/// The type of a member `read` that takes a path of type `at` and performs
/// `effects`.
fn read_type(at: Type, effects: Row) -> Type {
    proc_type(vec![Type::SelfType, at], read_result(), effects)
}

fn member_record(ty: Type) -> SignatureRecord {
    SignatureRecord::interface_member(Vec::new(), Some(0), ty, Vec::new()).unwrap()
}

fn relative() -> Type {
    Type::Declared(id("std.fs.RelativePath"), Vec::new())
}

/// The function bound to `fs.readText`, with the member record `record`.
fn host_function(name: &str, record: SignatureRecord) -> CheckedFunction {
    CheckedFunction {
        name: name.to_owned(),
        signature: record,
        defaults: Vec::new(),
        body: FunctionBody::Host(HostBinding::Operation(HostOp::FsReadText)),
        entry_guard: None,
        exit_guard: None,
        slot_count: 2,
        origin: origin(),
        context: BodyContext::Pure,
    }
}

/// An interface at `path` whose member `name` has the default `default`.
fn interface_with(
    path: &str,
    name: &str,
    record: SignatureRecord,
    default: Option<usize>,
) -> InterfaceDefinition {
    InterfaceDefinition::new(
        id(path),
        Vec::new(),
        Vec::new(),
        vec![InterfaceMember {
            name: name.to_owned(),
            signature: record,
            default,
        }],
    )
    .unwrap()
}

/// The parts with `RelativePath`, `FsError`, `DirReadText` (`read`, its
/// default the function at index `0` when `bound_on_read` holds), and
/// `DirList` (`read`, its default the function at index `0` otherwise), and
/// the host function, of type `member`, at index `0`.
fn host_parts(member: &Type, bound_on_read: bool) -> ModuleSetParts {
    let read_record = member_record(read_type(relative(), row(&[DIR_READ_TEXT])));
    let list_record = member_record(read_type(relative(), row(&[DIR_READ_TEXT])));
    let bound_record = member_record(member.clone());
    let mut parts = library_parts();
    parts.types.extend([
        relative_path(),
        variant("std.fs.FsError", &["other", "denied"]),
    ]);
    let (read, list) = if bound_on_read {
        (bound_record.clone(), list_record)
    } else {
        (read_record, bound_record.clone())
    };
    parts.interfaces.extend([
        interface_with(DIR_READ_TEXT, "read", read, bound_on_read.then_some(0)),
        interface_with(DIR_LIST, "read", list, (!bound_on_read).then_some(0)),
    ]);
    parts.functions = vec![host_function("std.fs.read", bound_record)];
    parts
}

/// 03 Operation interfaces: "its signature and owner MUST match its
/// registry row".
#[test]
fn host_body_on_its_owner() {
    let member = read_type(relative(), row(&[DIR_READ_TEXT]));
    CheckedModuleSet::try_new(host_parts(&member, true)).unwrap();
}

/// `ATK-ir-host-body-wrong-owner`: the same body on `DirList`'s member, a
/// member that is otherwise well formed for `DirList`: its row is
/// `{DirList}`.
#[test]
fn atk_ir_host_body_wrong_owner() {
    let member = read_type(relative(), row(&[DIR_LIST]));
    let fault = effect_fault(
        CheckedModuleSet::try_new(host_parts(&member, false)),
        "std.fs.read",
    );
    assert!(
        matches!(&fault, EffectFault::HostMismatch { symbol: "fs.readText", message } if message.contains("owner")),
        "{fault:?}"
    );
}

/// The same: the body with `at: Str`.
#[test]
fn host_body_wrong_signature() {
    let member = read_type(str_type(), row(&[DIR_READ_TEXT]));
    let fault = effect_fault(
        CheckedModuleSet::try_new(host_parts(&member, true)),
        "std.fs.read",
    );
    assert!(
        matches!(&fault, EffectFault::HostMismatch { symbol: "fs.readText", message } if message.contains("signature")),
        "{fault:?}"
    );
}

/// 03 Operation interfaces: "Such a member has no body and no `performs`".
#[test]
fn host_body_with_an_additive_row() {
    let member = read_type(relative(), row(&[DIR_READ_TEXT, DIR_LIST]));
    let fault = effect_fault(
        CheckedModuleSet::try_new(host_parts(&member, true)),
        "std.fs.read",
    );
    assert!(
        matches!(&fault, EffectFault::HostMismatch { symbol: "fs.readText", message } if message.contains("row")),
        "{fault:?}"
    );
}

/// `fs.readText`'s member on `DirReadText` and an implementation of it for
/// `I32` whose `read` is the function at index `1`.
fn implemented(supplies: bool) -> ModuleSetParts {
    let member = read_type(relative(), row(&[DIR_READ_TEXT]));
    let mut parts = host_parts(&member, true);
    let err = node(Expr::Construct {
        value_type: read_result(),
        construction: Construction::Case {
            case: "err".to_owned(),
            fields: vec![(
                "error".to_owned(),
                node(Expr::Construct {
                    value_type: fs_error(),
                    construction: Construction::Case {
                        case: "other".to_owned(),
                        fields: Vec::new(),
                    },
                }),
            )],
        },
    });
    parts.functions.push(function(
        "I32.read",
        proc_type(
            vec![Type::I32, relative()],
            read_result(),
            row(&[DIR_READ_TEXT]),
        ),
        err,
        2,
    ));
    let record = ImplementationRecord::new(
        Vec::new(),
        InterfaceKey::new(id(DIR_READ_TEXT), Vec::new()),
        Type::I32,
        NamedTypes::empty(),
    )
    .unwrap();
    let members = if supplies {
        vec![("read".to_owned(), 1)]
    } else {
        Vec::new()
    };
    parts.implementations = vec![Implementation::new(record, members).unwrap()];
    parts
}

/// Q2: a host-bound member is the implementation of sealed grants only, so
/// for an `impl` it is abstract: the `impl` supplies it.
#[test]
fn an_implementation_supplies_the_host_bound_member() {
    CheckedModuleSet::try_new(implemented(true)).unwrap();
}

/// Q2: an `impl` that lacks the host-bound member is rejected.
#[test]
fn an_implementation_without_the_host_bound_member_is_rejected() {
    assert!(matches!(
        CheckedModuleSet::try_new(implemented(false)),
        Err(IrError::InvalidType(message)) if message.contains("does not supply `read`")
    ));
}

// ----- what has no encoding -----

/// `ATK-ir-grant-constant`: 03 Capability values, "No source expression
/// makes a grant".
#[test]
fn atk_ir_grant_constant() {
    let mut parts = implemented(true);
    let grant = interface_type(DIR_READ_TEXT);
    let constant = Constant::new(
        grant.clone(),
        Value::Interface {
            held_type: Type::I32,
            held: Box::new(Value::I32(1)),
        },
    )
    .unwrap();
    parts.globals.push(CheckedGlobal {
        name: "grant".to_owned(),
        value_type: grant.clone(),
        initializer: node(Expr::Literal(constant)),
        slot_count: 0,
        origin: origin(),
    });
    assert_eq!(
        CheckedModuleSet::try_new(parts).unwrap_err(),
        IrError::NoEncoding {
            what: "a constant",
            ty: Box::new(grant),
        }
    );
}

/// 06 What has no encoding: "a plan … have no value encoding".
#[test]
fn a_plan_constant() {
    let plan = Type::Intent(Box::new(vibra_ir::IntentType {
        result: Type::Void,
        effects: Row::empty(),
    }));
    assert!(Constant::new(plan.clone(), Value::Void).is_err());
    assert!(Constant::new(Type::list(plan), Value::List(Vec::new())).is_err());
}

/// 03 Capability values: a grant "is no dict key", because it implements no
/// `Ordered`; the dict-key rule alone refuses it, where a dict is built.
#[test]
fn a_grant_dict_key() {
    let member = read_type(relative(), row(&[DIR_READ_TEXT]));
    let mut parts = host_parts(&member, true);
    let dict = library_type(
        Library::Dict,
        vec![interface_type(DIR_READ_TEXT), str_type()],
    );
    parts.functions.push(function(
        "keyed",
        fn_type(Vec::new(), dict.clone()),
        node(Expr::Construct {
            value_type: dict,
            construction: Construction::Dict(Vec::new()),
        }),
        0,
    ));
    assert!(matches!(
        CheckedModuleSet::try_new(parts),
        Err(IrError::InvalidType(message)) if message.contains("does not implement `Ordered`")
    ));
}

// ----- source origins -----

fn at(source: &str, start: usize, end: usize) -> SourceOrigin {
    SourceOrigin::new(source, ByteSpan::new(start, end))
}

fn void_at(origin: SourceOrigin) -> Node {
    Node::new(
        Expr::Literal(Constant::new(Type::Void, Value::Void).unwrap()),
        origin,
    )
}

/// 06 Origins: "A source origin says where a node of the checked program
/// was written … `{sourceId: Str start: U64 end: U64}`": a checked program
/// keeps each node's record exactly as written, and two origins with the
/// same source and offsets are the same origin.
#[test]
fn an_origin_known() {
    let body = Node::new(
        Expr::Sequence(vec![
            void_at(at("a.vib", 9, 12)),
            void_at(at("b.vib", 0, 4)),
        ]),
        at("a.vib", 2, 30),
    );
    let mut main = function("main", fn_type(Vec::new(), Type::Void), body, 0);
    main.origin = at("a.vib", 2, 5);
    let program = CheckedProgram::for_entry(build(vec![main]).unwrap(), 0).unwrap();
    let main = &program.module_set().functions()[0];
    assert_eq!(main.origin, at("a.vib", 2, 5));
    let FunctionBody::Expr(body) = &main.body else {
        panic!("an expression body");
    };
    assert_eq!(body.origin.source_id(), "a.vib");
    assert_eq!(
        (body.origin.span().start(), body.origin.span().end()),
        (2, 30)
    );
    let Expr::Sequence(items) = &body.expr else {
        panic!("a sequence");
    };
    let origins = items
        .iter()
        .map(|item| item.origin.clone())
        .collect::<Vec<_>>();
    assert_eq!(origins, [at("a.vib", 9, 12), at("b.vib", 0, 4)]);
}

/// 06 Origins: "Where no source origin is known, … a value holds
/// `Option.none()`": a host request names the origin of its `await`, or
/// none, as for a startup read.
#[test]
fn an_origin_absent() {
    use vibra_ir::authority::{GrantToken, HostRequest};
    let written = at("src/hello/main.vib", 42, 68);
    let request = |origin, parent| HostRequest {
        op: HostOp::IoStdoutText,
        grant: GrantToken::new(1, 0, 0),
        arguments: &[],
        invocation: 2,
        parent,
        origin,
        room: 0,
    };
    let entry = request(Some(&written), Some(1));
    assert_eq!(entry.origin, Some(&at("src/hello/main.vib", 42, 68)));
    let startup = request(None, None);
    assert_eq!(startup.origin, None);
}

// ----- depth -----

/// Architecture, risk 5: no host stack grows with the depth of a body.
#[test]
fn deep_body() {
    let validated = on_small_stack(|| {
        let mut body = await_node(build_p(I, 0));
        for _ in 0..100_000 {
            body = node(Expr::Sequence(vec![body]));
        }
        let f = function(
            "f",
            proc_type(vec![interface_type(I)], Type::Void, row(&[I])),
            body,
            1,
        );
        build(vec![f]).is_ok()
    });
    assert!(validated);
}
