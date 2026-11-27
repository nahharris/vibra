//! Host dispatch through a host context (`docs/spec/06-runtime.md`, "Host
//! calls", "Host registry", "Cold plans and invocation identity", "Audit
//! trace", "Memory", "What has no encoding", "Traps";
//! `docs/spec/03-effects.md`, "Capability values", "Narrowing", "Operation
//! interfaces"; `docs/spec/04-packages-and-projects.md`, "Test worlds").
//!
//! The fixture declares the 17 standard operation interfaces, each with its
//! host-bound member, and `TestWorld` with its six members, all at the
//! paths of `@std`. `R` is `DirReadText`, whose member `read` is bound to
//! `fs.readText` and which also declares the narrowing members `limit`,
//! `restrict`, and `sub`, bound to the grant rows. A [`Recording`] context
//! answers from a table and logs every request, narrowing, test-world
//! call, and commit, in order.

use std::collections::VecDeque;
use std::iter;
use std::sync::Arc;

use vibra_interp::{
    EntryArgument, Execution, MemoryBudget, RuntimeError, run, run_entry, run_test,
};
use vibra_ir::authority::{
    GrantLimits, GrantToken, HostArgument, HostCompletion, HostContext, HostErrorCase,
    HostFault, HostFsKind, HostOk, HostReply, HostRequest, Issued, NarrowRequest,
    Narrowed, Receipt, ScopeErrorCase,
};
use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::external::compiler::NarrowingType;
use vibra_ir::external::host::{
    FS_KIND_CASES, FS_METADATA_FIELDS, HostErrorType, HostInterface, HostOp, HostType,
    Owner, TEST_WORLD_OWNER, TestWorldOp,
};
use vibra_ir::{
    Arguments, BodyContext, CallTarget, Case, CheckedFunction, CheckedModuleSet,
    Constant, Construction, ContractCall, Expr, FnType, FunctionBody, HostBinding,
    Implementation, ImplementationRecord, InterfaceDefinition, InterfaceKey,
    InterfaceMember, MatchArm, NamedTypes, Node, Pattern, ProcType, Row,
    SignatureRecord, SourceOrigin, Trap, Type, TypeBody, TypeDefinition, TypeId, Value,
    Visibility,
};

use crate::{
    BASE, base_parts, bytes_type, call, call_value, closure, construct, dict,
    err_value, function, id, if_else, library, lit, node, none, ok_value, option,
    origin, origin_at, program, result, signature, some, str_type, text, text_value,
    u64_equal, u64_lit, u64_sub, var, variant_value, void,
};

// ----- the recording context -----

/// The identity of the recording context: every token and receipt it
/// makes carries it.
const CONTEXT: u64 = 7;

fn token(slot: u32) -> GrantToken {
    GrantToken::new(CONTEXT, slot, 0)
}

/// An argument of a request, owned.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Arg {
    Text(String),
    Bytes(Vec<u8>),
    Path(Vec<String>),
    Count(u64),
}

/// A request, owned.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Request {
    op: HostOp,
    grant: GrantToken,
    arguments: Vec<Arg>,
    invocation: u64,
    parent: Option<u64>,
    origin: Option<SourceOrigin>,
    room: u64,
}

/// One call into the context, in order.
#[derive(Clone, Debug, PartialEq)]
enum Event {
    Request(Request),
    Commit(u64),
    Restrict(GrantToken, Value),
    Sub(GrantToken, Vec<String>),
    Limit(GrantToken, GrantLimits),
    World(TestWorldOp, GrantToken, Vec<Value>, u64),
}

/// A host context that answers from its queues, or by the row's sample
/// answer when a queue is empty, and logs every call.
#[derive(Default)]
struct Recording {
    events: Vec<Event>,
    replies: VecDeque<Result<HostReply, HostFault>>,
    narrowed: VecDeque<Result<Narrowed, HostFault>>,
    issued: VecDeque<Result<Issued, HostFault>>,
    commit_fault: Option<HostFault>,
    serial: u64,
    fresh: u32,
}

impl Recording {
    fn requests(&self) -> Vec<Request> {
        self.events
            .iter()
            .filter_map(|event| match event {
                Event::Request(request) => Some(request.clone()),
                _ => None,
            })
            .collect()
    }

    fn commits(&self) -> usize {
        self.events
            .iter()
            .filter(|event| matches!(event, Event::Commit(_)))
            .count()
    }

    /// The invocation and parent of each request, in order.
    fn ordinals(&self) -> Vec<(u64, Option<u64>)> {
        self.requests()
            .iter()
            .map(|request| (request.invocation, request.parent))
            .collect()
    }

    fn grants(&self) -> Vec<GrantToken> {
        self.requests()
            .iter()
            .map(|request| request.grant)
            .collect()
    }
}

/// The sample `Ok` answer of `op`, by the type its row returns.
fn sample_ok(op: HostOp) -> HostOk {
    match op.row().ok {
        HostType::Void => HostOk::Void,
        HostType::Str => HostOk::Text("text".to_owned()),
        HostType::Bytes => HostOk::Bytes(b"bytes".to_vec()),
        HostType::List(_) => HostOk::Names(vec!["a".to_owned(), "b".to_owned()]),
        HostType::FsMetadata => HostOk::Metadata {
            kind: HostFsKind::File,
            size: 3,
        },
        HostType::I64 => HostOk::I64(-5),
        HostType::U64 => HostOk::U64(9),
        other => panic!("no sample answer of {}", other.spelling()),
    }
}

impl HostContext for Recording {
    fn host(&mut self, request: HostRequest<'_>) -> Result<HostCompletion, HostFault> {
        let arguments = request
            .arguments
            .iter()
            .map(|argument| match argument {
                HostArgument::Text(text) => Arg::Text((*text).to_owned()),
                HostArgument::Bytes(bytes) => Arg::Bytes(bytes.to_vec()),
                HostArgument::Path(names) => Arg::Path(names.to_vec()),
                HostArgument::Count(count) => Arg::Count(*count),
            })
            .collect();
        self.events.push(Event::Request(Request {
            op: request.op,
            grant: request.grant,
            arguments,
            invocation: request.invocation,
            parent: request.parent,
            origin: request.origin.cloned(),
            room: request.room,
        }));
        let reply = match self.replies.pop_front() {
            Some(reply) => reply?,
            None => HostReply::Ok(sample_ok(request.op)),
        };
        let serial = self.serial;
        self.serial += 1;
        Ok(HostCompletion {
            reply,
            receipt: Some(Receipt::new(CONTEXT, serial)),
        })
    }

    fn commit(&mut self, receipt: Receipt) -> Result<(), HostFault> {
        if let Some(fault) = self.commit_fault.take() {
            return Err(fault);
        }
        self.events.push(Event::Commit(receipt.serial()));
        Ok(())
    }

    fn narrow(&mut self, request: NarrowRequest<'_>) -> Result<Narrowed, HostFault> {
        self.events.push(match request {
            NarrowRequest::Restrict { grant, policy } => {
                Event::Restrict(grant, policy.clone())
            }
            NarrowRequest::Sub { grant, prefix } => Event::Sub(grant, prefix.to_vec()),
            NarrowRequest::Limit { grant, limits } => Event::Limit(grant, limits),
        });
        self.fresh += 1;
        let fresh = token(100 + self.fresh);
        self.narrowed
            .pop_front()
            .unwrap_or(Ok(Narrowed::Grant(fresh)))
    }

    fn test_world(
        &mut self,
        op: TestWorldOp,
        world: GrantToken,
        arguments: &[Value],
        invocation: u64,
    ) -> Result<Issued, HostFault> {
        self.events
            .push(Event::World(op, world, arguments.to_vec(), invocation));
        self.issued.pop_front().unwrap_or_else(|| {
            Err(HostFault::InvalidHostValue("nothing to issue".to_owned()))
        })
    }
}

// ----- the fixture -----

const R: HostInterface = HostInterface::DirReadText;

/// The first host-bound function: one per row of `HostOp::ALL`, in order.
const HOST: usize = BASE;
const LIMIT: usize = BASE + 17;
const RESTRICT: usize = BASE + 18;
const SUB: usize = BASE + 19;
/// The first `TestWorld` member: one per row of `TestWorldOp::ALL`.
const WORLD: usize = BASE + 20;
/// The first function a test adds.
const OWN: usize = BASE + 26;

fn owner_id(owner: Owner) -> TypeId {
    id(owner.path().trim_start_matches('@'))
}

fn interface_id(interface: HostInterface) -> TypeId {
    owner_id(interface.owner())
}

fn interface_type(interface: HostInterface) -> Type {
    Type::Interface(interface_id(interface), Vec::new(), NamedTypes::empty())
}

fn world_id() -> TypeId {
    owner_id(TEST_WORLD_OWNER)
}

fn world_type() -> Type {
    Type::Interface(world_id(), Vec::new(), NamedTypes::empty())
}

fn r_id() -> TypeId {
    interface_id(R)
}

fn r_type() -> Type {
    interface_type(R)
}

fn row_of(ids: &[TypeId]) -> Row {
    Row::new(ids.iter().map(|id| id.entity().clone())).unwrap()
}

fn declared(path: &str) -> Type {
    Type::Declared(id(path), Vec::new())
}

fn relative_path() -> Type {
    declared("std.fs.RelativePath")
}

fn error_path(error: HostErrorType) -> String {
    format!(
        "{}.{}",
        error.module().trim_start_matches('@'),
        error.name()
    )
}

fn error_type(error: HostErrorType) -> Type {
    declared(&error_path(error))
}

fn limits_type() -> Type {
    declared("std.policy.GrantLimits")
}

fn scope_error() -> Type {
    declared("std.policy.ScopeError")
}

fn policy_type() -> Type {
    declared("std.fs.FsPolicy")
}

fn exit_code() -> Type {
    declared("std.process.ExitCode")
}

/// `Result(Str FsError)`: what `R.read` returns.
fn read_result() -> Type {
    result(str_type(), error_type(HostErrorType::Fs))
}

/// A registry type as a type of the fixture. The registry's types nest a
/// few levels at most.
fn host_type(ty: HostType) -> Type {
    match ty {
        HostType::Void => Type::Void,
        HostType::Str => str_type(),
        HostType::Bytes => bytes_type(),
        HostType::I64 => Type::I64,
        HostType::U64 => Type::U64,
        HostType::RelativePath => relative_path(),
        HostType::FsKind => declared("std.fs.FsKind"),
        HostType::FsMetadata => declared("std.fs.FsMetadata"),
        HostType::List(element) => Type::list(host_type(*element)),
        HostType::Dict(key, value) => dict(host_type(*key), host_type(*value)),
        HostType::Interface(interface) => interface_type(interface),
        HostType::TestWorld => world_type(),
        HostType::Record(fields) => Type::Record(
            NamedTypes::new(
                fields
                    .iter()
                    .map(|field| (field.name.to_owned(), host_type(field.ty))),
            )
            .unwrap(),
        ),
    }
}

fn definition(path: &str, body: TypeBody) -> TypeDefinition {
    TypeDefinition::new(id(path), Vec::new(), body, None, Visibility::Public).unwrap()
}

fn plain<'n>(names: impl IntoIterator<Item = &'n str>) -> TypeBody {
    TypeBody::Variant(
        names
            .into_iter()
            .map(|name| Case::new(name, Vec::new()))
            .collect(),
    )
}

/// The `@std` declarations the host rows and the grant rows name.
fn host_definitions() -> Vec<TypeDefinition> {
    let optional = || option(Type::U64);
    let mut definitions = vec![
        definition(
            "std.fs.RelativePath",
            TypeBody::Wrapper(Type::list(str_type())),
        ),
        definition("std.fs.FsKind", plain(FS_KIND_CASES)),
        definition(
            "std.fs.FsMetadata",
            TypeBody::Record(
                FS_METADATA_FIELDS
                    .iter()
                    .map(|field| (field.name.to_owned(), host_type(field.ty)))
                    .collect(),
            ),
        ),
        definition("std.fs.FsPolicy", plain(["everything", "nothing"])),
        definition(
            "std.policy.ScopeError",
            plain([
                "invalidPolicy",
                "unsupportedVersion",
                "policyTooComplex",
                "foreignRoot",
                "invalidPath",
            ]),
        ),
        definition(
            "std.policy.GrantLimits",
            TypeBody::Record(vec![
                ("invocations".to_owned(), optional()),
                ("inputBytes".to_owned(), optional()),
                ("outputBytes".to_owned(), optional()),
            ]),
        ),
        definition("std.process.ExitCode", TypeBody::Wrapper(Type::U8)),
    ];
    for error in HostErrorType::ALL {
        definitions.push(definition(&error_path(error), plain(error.cases())));
    }
    definitions
}

fn member_signature(ty: Type) -> SignatureRecord {
    SignatureRecord::interface_member(Vec::new(), Some(0), ty, Vec::new()).unwrap()
}

/// The type of the host-bound member of `op`.
fn host_member_type(op: HostOp) -> Type {
    let row = op.row();
    Type::Proc(Box::new(ProcType {
        parameters: iter::once(Type::SelfType)
            .chain(
                row.parameters
                    .iter()
                    .map(|parameter| host_type(parameter.ty)),
            )
            .collect(),
        labels: NamedTypes::empty(),
        result: result(host_type(row.ok), error_type(row.error)),
        effects: row_of(&[interface_id(op.interface())]),
    }))
}

/// The type of the `TestWorld` member `op`.
fn world_member_type(op: TestWorldOp) -> Type {
    let row = op.row();
    Type::Proc(Box::new(ProcType {
        parameters: vec![Type::SelfType],
        labels: NamedTypes::new(
            row.labelled
                .iter()
                .map(|parameter| (parameter.name.to_owned(), host_type(parameter.ty))),
        )
        .unwrap(),
        result: host_type(row.result),
        effects: row_of(&[world_id()]),
    }))
}

/// The default of a labelled parameter of a `TestWorld` member: the empty
/// value of its type.
fn empty_default(ty: &Type) -> Constant {
    let value = if *ty == str_type() {
        text_value("")
    } else if matches!(ty, Type::Declared(..)) {
        Value::Dict(Vec::new())
    } else {
        Value::List(Vec::new())
    };
    Constant::new(ty.clone(), value).unwrap()
}

fn host_function(name: &str, ty: Type, binding: HostBinding) -> CheckedFunction {
    let (slot_count, defaults) = match &ty {
        Type::Proc(signature) => (
            signature.parameters.len() + signature.labels.entries().len(),
            signature
                .labels
                .entries()
                .iter()
                .map(|(label, ty)| (label.clone(), empty_default(ty)))
                .collect(),
        ),
        _ => (0, Vec::new()),
    };
    CheckedFunction {
        context: BodyContext::Pure,
        name: name.to_owned(),
        signature: member_signature(ty),
        defaults,
        body: FunctionBody::Host(binding),
        entry_guard: None,
        exit_guard: None,
        slot_count,
        origin: origin(),
    }
}

fn limit_member_type() -> Type {
    Type::function(FnType::new(
        vec![Type::SelfType, limits_type()],
        Type::SelfType,
    ))
}

fn restrict_member_type() -> Type {
    Type::function(FnType::new(
        vec![Type::SelfType, policy_type()],
        result(Type::SelfType, scope_error()),
    ))
}

fn sub_member_type() -> Type {
    Type::function(FnType::new(
        vec![Type::SelfType, relative_path()],
        result(Type::SelfType, scope_error()),
    ))
}

/// The fixture's functions, from `HOST` to `OWN`.
fn fixture_functions() -> Vec<CheckedFunction> {
    let mut functions = HostOp::ALL
        .into_iter()
        .map(|op| {
            host_function(
                op.symbol(),
                host_member_type(op),
                HostBinding::Operation(op),
            )
        })
        .collect::<Vec<_>>();
    for (intrinsic, ty) in [
        (CompilerIntrinsic::GrantLimit, limit_member_type()),
        (CompilerIntrinsic::GrantRestrict, restrict_member_type()),
        (CompilerIntrinsic::GrantSub, sub_member_type()),
    ] {
        functions.push(CheckedFunction::external(
            intrinsic.symbol(),
            member_signature(ty),
            intrinsic,
            vec![Type::SelfType],
            origin(),
        ));
    }
    functions.extend(TestWorldOp::ALL.into_iter().map(|op| {
        host_function(
            op.symbol(),
            world_member_type(op),
            HostBinding::TestWorld(op),
        )
    }));
    functions
}

fn host_index(op: HostOp) -> usize {
    HOST + HostOp::ALL.iter().position(|row| *row == op).unwrap()
}

fn world_index(op: TestWorldOp) -> usize {
    WORLD + TestWorldOp::ALL.iter().position(|row| *row == op).unwrap()
}

fn host_interfaces() -> Vec<InterfaceDefinition> {
    let mut interfaces = HostInterface::ALL
        .into_iter()
        .map(|interface| {
            let op = interface.operation();
            let mut members = vec![InterfaceMember {
                name: interface.member().to_owned(),
                signature: member_signature(host_member_type(op)),
                default: Some(host_index(op)),
            }];
            if interface == R {
                for (name, ty, function) in [
                    ("limit", limit_member_type(), LIMIT),
                    ("restrict", restrict_member_type(), RESTRICT),
                    ("sub", sub_member_type(), SUB),
                ] {
                    members.push(InterfaceMember {
                        name: name.to_owned(),
                        signature: member_signature(ty),
                        default: Some(function),
                    });
                }
            }
            InterfaceDefinition::new(
                interface_id(interface),
                Vec::new(),
                Vec::new(),
                members,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    interfaces.push(
        InterfaceDefinition::new(
            world_id(),
            Vec::new(),
            Vec::new(),
            TestWorldOp::ALL
                .into_iter()
                .map(|op| InterfaceMember {
                    name: op.member().to_owned(),
                    signature: member_signature(world_member_type(op)),
                    default: Some(world_index(op)),
                })
                .collect(),
        )
        .unwrap(),
    );
    interfaces
}

/// The fixture with the functions `own` from `OWN` on, the extra types,
/// and the implementations.
fn set_of(
    own: Vec<CheckedFunction>,
    types: Vec<TypeDefinition>,
    implementations: Vec<Implementation>,
) -> Arc<CheckedModuleSet> {
    let mut parts = base_parts();
    parts.library = library()
        .with_narrowing(NarrowingType::FsPolicy, id("std.fs.FsPolicy"))
        .with_narrowing(NarrowingType::RelativePath, id("std.fs.RelativePath"))
        .with_narrowing(NarrowingType::ScopeError, id("std.policy.ScopeError"))
        .with_narrowing(NarrowingType::GrantLimits, id("std.policy.GrantLimits"));
    parts.types.extend(host_definitions());
    parts.types.extend(types);
    parts.interfaces.extend(host_interfaces());
    parts.functions.extend(fixture_functions());
    parts.functions.extend(own);
    parts.implementations.extend(implementations);
    CheckedModuleSet::try_new(parts).unwrap()
}

// ----- builders -----

fn await_(plan: Node) -> Node {
    node(Expr::Await {
        plan: Box::new(plan),
    })
}

fn await_at(plan: Node, at: SourceOrigin) -> Node {
    Node::new(
        Expr::Await {
            plan: Box::new(plan),
        },
        at,
    )
}

fn ret(value: Node) -> Node {
    node(Expr::Return(Box::new(value)))
}

fn seq(statements: Vec<Node>) -> Node {
    node(Expr::Sequence(statements))
}

fn let_in(slot: usize, value: Node, body: Node) -> Node {
    node(Expr::Let {
        slot: Some(slot),
        value: Box::new(value),
        body: Box::new(body),
    })
}

fn field(record: Node, name: &str) -> Node {
    node(Expr::Field {
        record: Box::new(record),
        field: name.to_owned(),
    })
}

fn widen(value: Node) -> Node {
    node(Expr::WidenToInterface {
        value: Box::new(value),
        value_type: r_type(),
    })
}

fn case(value_type: Type, name: &str, fields: Vec<(&str, Node)>) -> Node {
    construct(
        value_type,
        Construction::Case {
            case: name.to_owned(),
            fields: fields
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
        },
    )
}

fn record(value_type: Type, fields: Vec<(&str, Node)>) -> Node {
    construct(
        value_type,
        Construction::Record(
            fields
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
        ),
    )
}

fn path_value(names: &[&str]) -> Value {
    Value::Wrapper(Box::new(Value::List(
        names.iter().map(|name| text_value(name)).collect(),
    )))
}

fn path_lit(names: &[&str]) -> Node {
    lit(relative_path(), path_value(names))
}

fn bytes_lit(bytes: &[u8]) -> Node {
    lit(bytes_type(), Value::Bytes(bytes.to_vec()))
}

/// A call of the member `member` of `interface` on `receiver` of static
/// type `receiver_type`.
fn contract(
    interface: TypeId,
    member: &str,
    receiver_type: Type,
    positional: Vec<Node>,
    labelled: Vec<(String, Node)>,
) -> Node {
    node(Expr::Call {
        target: CallTarget::Contract(Box::new(ContractCall {
            interface,
            member: member.to_owned(),
            receiver: receiver_type,
            arguments: Vec::new(),
            choices: NamedTypes::empty(),
            member_arguments: Vec::new(),
        })),
        arguments: Arguments {
            positional,
            labelled,
        },
    })
}

/// The plan of the host member of `op` on `receiver` and `operands`.
fn op_plan(op: HostOp, receiver: Node, operands: Vec<Node>) -> Node {
    contract(
        interface_id(op.interface()),
        op.interface().member(),
        interface_type(op.interface()),
        iter::once(receiver).chain(operands).collect(),
        Vec::new(),
    )
}

/// The plan of `R.read(receiver at)`.
fn read_plan(receiver: Node, at: &[&str]) -> Node {
    op_plan(HostOp::FsReadText, receiver, vec![path_lit(at)])
}

/// `await R.read(receiver at)`.
fn read(receiver: Node, at: &[&str]) -> Node {
    await_(read_plan(receiver, at))
}

/// `R.member(receiver operand)` for a narrowing member.
fn narrowing(member: &str, receiver: Node, operand: Node) -> Node {
    contract(
        r_id(),
        member,
        r_type(),
        vec![receiver, operand],
        Vec::new(),
    )
}

fn limits(invocations: u64) -> Node {
    record(
        limits_type(),
        vec![
            ("invocations", some(Type::U64, u64_lit(invocations))),
            ("inputBytes", none(Type::U64)),
            ("outputBytes", none(Type::U64)),
        ],
    )
}

/// A sample operand of a parameter of `ty`, and the argument it becomes.
fn sample(ty: HostType) -> (Node, Arg) {
    match ty {
        HostType::Str => (text("hi"), Arg::Text("hi".to_owned())),
        HostType::Bytes => (bytes_lit(&[1, 2]), Arg::Bytes(vec![1, 2])),
        HostType::RelativePath => (
            path_lit(&["a", "b"]),
            Arg::Path(vec!["a".to_owned(), "b".to_owned()]),
        ),
        HostType::U64 => (u64_lit(4), Arg::Count(4)),
        other => panic!("no sample operand of {}", other.spelling()),
    }
}

/// `await` of the host member of `op` on `receiver` and sample operands.
fn sampled(op: HostOp, receiver: Node) -> Node {
    let operands = op
        .row()
        .parameters
        .iter()
        .map(|parameter| sample(parameter.ty).0)
        .collect();
    await_(op_plan(op, receiver, operands))
}

fn sample_arguments(op: HostOp) -> Vec<Arg> {
    op.row()
        .parameters
        .iter()
        .map(|parameter| sample(parameter.ty).1)
        .collect()
}

/// A `proc` of `Proc({parameters} result {effects})` that runs in the
/// procedure context of its row.
fn procedure(
    name: &str,
    parameters: Vec<Type>,
    result: Type,
    effects: &[TypeId],
    body: Node,
    slot_count: usize,
) -> CheckedFunction {
    let row = row_of(effects);
    CheckedFunction {
        context: BodyContext::Procedure(row.clone()),
        ..function(
            name,
            SignatureRecord::new(
                Vec::new(),
                Type::Proc(Box::new(ProcType {
                    parameters,
                    labels: NamedTypes::empty(),
                    result,
                    effects: row,
                })),
            )
            .unwrap(),
            body,
            slot_count,
        )
    }
}

/// `main(r: R) returns Result(Str FsError) performs {R}` with `body`.
fn main_over_r(body: Node, slot_count: usize) -> CheckedFunction {
    procedure(
        "main",
        vec![r_type()],
        read_result(),
        &[r_id()],
        body,
        slot_count,
    )
}

/// Runs the last of `own` as the entry, the first invocation, on `grants`.
fn run_main(
    own: Vec<CheckedFunction>,
    types: Vec<TypeDefinition>,
    implementations: Vec<Implementation>,
    grants: Vec<EntryArgument>,
    host: &mut Recording,
) -> Result<Execution, RuntimeError> {
    run_main_from(
        own,
        types,
        implementations,
        grants,
        1,
        host,
        MemoryBudget::DEFAULT,
    )
}

fn run_main_from(
    own: Vec<CheckedFunction>,
    types: Vec<TypeDefinition>,
    implementations: Vec<Implementation>,
    grants: Vec<EntryArgument>,
    first_invocation: u64,
    host: &mut Recording,
    budget: MemoryBudget,
) -> Result<Execution, RuntimeError> {
    let entry = OWN + own.len() - 1;
    let set = set_of(own, types, implementations);
    run_entry(&program(set, entry), grants, first_invocation, host, budget)
}

/// `main(r)` reading `notes.txt` through `r`, run on the grant 1.
fn read_once(host: &mut Recording) -> Result<Execution, RuntimeError> {
    let main = main_over_r(ret(read(var(0, r_type()), &["notes.txt"])), 1);
    run_main(
        vec![main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        host,
    )
}

#[track_caller]
fn assert_trap(outcome: Result<Execution, RuntimeError>, expected: Trap) {
    match outcome {
        Err(RuntimeError::Trap { trap, .. }) => assert_eq!(trap, expected),
        other => panic!("expected the trap {expected:?}, got {other:?}"),
    }
}

// ----- models -----

/// `Memory of {text: Str}`.
fn memory_type() -> Type {
    declared("test.Memory")
}

fn memory_definition() -> TypeDefinition {
    definition(
        "test.Memory",
        TypeBody::Record(vec![("text".to_owned(), str_type())]),
    )
}

fn memory(text_value: &str) -> Node {
    record(memory_type(), vec![("text", text(text_value))])
}

/// `Forward of {inner: R}`.
fn forward_type() -> Type {
    declared("test.Forward")
}

fn forward_definition() -> TypeDefinition {
    definition(
        "test.Forward",
        TypeBody::Record(vec![("inner".to_owned(), r_type())]),
    )
}

/// `impl R for receiver` with `members`.
fn implementation(receiver: Type, members: Vec<(&str, usize)>) -> Implementation {
    Implementation::new(
        ImplementationRecord::new(
            Vec::new(),
            InterfaceKey::new(r_id(), Vec::new()),
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

/// `Memory`'s `read`: `Result.ok(value: self.text)`.
fn memory_read() -> CheckedFunction {
    procedure(
        "Memory.read",
        vec![memory_type(), relative_path()],
        read_result(),
        &[r_id()],
        ret(case(
            read_result(),
            "ok",
            vec![("value", field(var(0, memory_type()), "text"))],
        )),
        2,
    )
}

/// `Memory`'s `limit`: returns itself.
fn memory_limit() -> CheckedFunction {
    function(
        "Memory.limit",
        signature(vec![memory_type(), limits_type()], memory_type()),
        var(0, memory_type()),
        2,
    )
}

/// A model's `restrict` and `sub`: `Result.ok(value: self)`. An `impl` of
/// `R` supplies them, as it supplies `limit` (spec questions Q1, Q2).
fn keeps_itself(model: Type) -> [CheckedFunction; 2] {
    let ok = || {
        case(
            result(model.clone(), scope_error()),
            "ok",
            vec![("value", var(0, model.clone()))],
        )
    };
    [
        function(
            "restrict",
            signature(
                vec![model.clone(), policy_type()],
                result(model.clone(), scope_error()),
            ),
            ok(),
            2,
        ),
        function(
            "sub",
            signature(
                vec![model.clone(), relative_path()],
                result(model.clone(), scope_error()),
            ),
            ok(),
            2,
        ),
    ]
}

/// `Memory`'s members from `first` on: `read`, `limit`, `restrict`, `sub`.
fn memory_members() -> Vec<CheckedFunction> {
    let mut members = vec![memory_read(), memory_limit()];
    members.extend(keeps_itself(memory_type()));
    members
}

/// `impl R for model` whose `read`, `limit`, `restrict`, and `sub` are
/// the functions from `first` on.
fn model_impl(model: Type, first: usize) -> Implementation {
    implementation(
        model,
        vec![
            ("read", first),
            ("limit", first + 1),
            ("restrict", first + 2),
            ("sub", first + 3),
        ],
    )
}

// ----- rows -----

/// Row "a host call": one request, its reply delivered as the member's
/// `Result`, then one commit.
#[test]
fn a_host_call() {
    let mut host = Recording::default();
    let execution = read_once(&mut host).unwrap();
    assert_eq!(execution.value.value(), &ok_value(text_value("text")));
    let [Event::Request(request), Event::Commit(0)] = host.events.as_slice() else {
        panic!("one request, then its commit: {:?}", host.events);
    };
    assert_eq!(request.op, HostOp::FsReadText);
    assert_eq!(request.grant, token(1));
    assert_eq!(request.arguments, [Arg::Path(vec!["notes.txt".to_owned()])]);
    assert_eq!((request.invocation, request.parent), (2, Some(1)));
    assert_eq!(request.origin, Some(origin()));
}

/// `main(r)` holding a 4,000-byte text in its frame while it reads through
/// `r`, so that the request is made at the run's peak footprint.
fn read_holding_text() -> CheckedFunction {
    main_over_r(
        let_in(
            1,
            text(&"x".repeat(4_000)),
            let_in(
                2,
                read(var(0, r_type()), &["notes.txt"]),
                ret(var(2, read_result())),
            ),
        ),
        3,
    )
}

/// Runs [`read_holding_text`] on the grant 1 under `budget`, with an empty
/// text as the reply.
fn room_run(budget: MemoryBudget) -> (Result<Execution, RuntimeError>, Recording) {
    let mut host = Recording::default();
    host.replies
        .push_back(Ok(HostReply::Ok(HostOk::Text(String::new()))));
    let outcome = run_main_from(
        vec![read_holding_text()],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        1,
        &mut host,
        budget,
    );
    (outcome, host)
}

/// Row "the room of a request": the room is the bytes the budget has left
/// when the request is made, measured afresh. With 100 bytes left, the room
/// is 100.
#[test]
fn the_room_of_a_request() {
    let (first, host) = room_run(MemoryBudget::DEFAULT);
    first.unwrap();
    let live = MemoryBudget::DEFAULT.bytes - host.requests()[0].room;
    let (outcome, host) = room_run(MemoryBudget::new(live + 100));
    assert_eq!(host.requests()[0].room, 100, "{outcome:?}");
    outcome.unwrap();
}
/// A reply the budget cannot account is the host event, and its receipt is
/// never committed.
#[test]
fn a_reply_the_budget_cannot_hold_is_the_host_event() {
    let mut first = Recording::default();
    read_once(&mut first).unwrap();
    let live = MemoryBudget::DEFAULT.bytes - first.requests()[0].room;
    let main = main_over_r(ret(read(var(0, r_type()), &["notes.txt"])), 1);
    let mut host = Recording::default();
    host.replies
        .push_back(Ok(HostReply::Ok(HostOk::Text("x".repeat(100_000)))));
    let outcome = run_main_from(
        vec![main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        1,
        &mut host,
        MemoryBudget::new(live + 100),
    );
    assert!(
        matches!(outcome, Err(RuntimeError::MemoryExhausted { .. })),
        "{outcome:?}"
    );
    assert_eq!(host.commits(), 0);
}

/// Row "an error reply": `Err(notFound)` is `Result.err(FsError.notFound)`,
/// not a trap, and its event is committed.
#[test]
fn an_error_reply() {
    let mut host = Recording::default();
    host.replies.push_back(Ok(HostReply::Err(
        HostErrorCase::new(HostErrorType::Fs, "notFound").unwrap(),
    )));
    let execution = read_once(&mut host).unwrap();
    assert_eq!(execution.value.value(), &err_value("notFound"));
    assert_eq!(host.commits(), 1);
}

/// An entry that awaits the host member of each of `ops` once, on its
/// grant parameter of the same position.
fn each_of(ops: &[HostOp]) -> CheckedFunction {
    let interfaces = ops.iter().map(|op| op.interface()).collect::<Vec<_>>();
    let mut statements = interfaces
        .iter()
        .enumerate()
        .map(|(slot, interface)| {
            sampled(interface.operation(), var(slot, interface_type(*interface)))
        })
        .collect::<Vec<_>>();
    statements.push(void());
    procedure(
        "main",
        interfaces
            .iter()
            .map(|interface| interface_type(*interface))
            .collect(),
        Type::Void,
        &interfaces
            .iter()
            .map(|interface| interface_id(*interface))
            .collect::<Vec<_>>(),
        seq(statements),
        ops.len(),
    )
}

/// Row "each argument form": `Str`, `Bytes`, `RelativePath`, the `U64` of
/// `random.generateBytes` and of `io.stdinText`, and none.
#[test]
fn each_argument_form() {
    let ops = [
        HostOp::IoStdoutText,
        HostOp::IoStdoutBytes,
        HostOp::FsReadText,
        HostOp::RandomGenerateBytes,
        HostOp::IoStdinText,
        HostOp::TimeNowWall,
    ];
    let mut host = Recording::default();
    run_main(
        vec![each_of(&ops)],
        Vec::new(),
        Vec::new(),
        (1..=6)
            .map(|slot| EntryArgument::Grant(token(slot)))
            .collect(),
        &mut host,
    )
    .unwrap();
    let requests = host.requests();
    assert_eq!(
        requests
            .iter()
            .map(|request| (request.op, request.arguments.clone()))
            .collect::<Vec<_>>(),
        ops.iter()
            .map(|op| (*op, sample_arguments(*op)))
            .collect::<Vec<_>>()
    );
    assert_eq!(requests[0].arguments, [Arg::Text("hi".to_owned())]);
    assert_eq!(requests[1].arguments, [Arg::Bytes(vec![1, 2])]);
    assert_eq!(
        requests[2].arguments,
        [Arg::Path(vec!["a".to_owned(), "b".to_owned()])]
    );
    assert_eq!(requests[3].arguments, [Arg::Count(4)]);
    assert_eq!(requests[4].arguments, [Arg::Count(4)]);
    assert!(requests[5].arguments.is_empty());
    assert_eq!(host.commits(), 6);
}

/// Row "each result form": `Void`, `Text`, `Bytes`, `Names`, `Metadata`,
/// `I64`, and `U64`, each as the `Ok` of its member's `Result`.
#[test]
fn each_result_form() {
    let ops = [
        HostOp::FsWriteText,
        HostOp::FsReadText,
        HostOp::FsReadFile,
        HostOp::FsReadEntries,
        HostOp::FsMetadataStat,
        HostOp::TimeNowWall,
        HostOp::TimeNowMonotonic,
    ];
    assert_eq!(
        ops.map(|op| op.row().ok.spelling()),
        [
            "Void",
            "Str",
            "Bytes",
            "List(Str)",
            "FsMetadata",
            "I64",
            "U64"
        ]
        .map(str::to_owned)
    );
    let interfaces = ops.map(HostOp::interface);
    let results = ops
        .iter()
        .map(|op| result(host_type(op.row().ok), error_type(op.row().error)))
        .collect::<Vec<_>>();
    let main = procedure(
        "main",
        interfaces
            .iter()
            .map(|interface| interface_type(*interface))
            .collect(),
        Type::Tuple(results.clone()),
        &interfaces.map(interface_id),
        ret(construct(
            Type::Tuple(results),
            Construction::Tuple(
                interfaces
                    .iter()
                    .enumerate()
                    .map(|(slot, interface)| {
                        sampled(
                            interface.operation(),
                            var(slot, interface_type(*interface)),
                        )
                    })
                    .collect(),
            ),
        )),
        ops.len(),
    );
    let mut host = Recording::default();
    let execution = run_main(
        vec![main],
        Vec::new(),
        Vec::new(),
        (1..=7)
            .map(|slot| EntryArgument::Grant(token(slot)))
            .collect(),
        &mut host,
    )
    .unwrap();
    assert_eq!(
        execution.value.value(),
        &Value::Tuple(vec![
            ok_value(Value::Void),
            ok_value(text_value("text")),
            ok_value(Value::Bytes(b"bytes".to_vec())),
            ok_value(Value::List(vec![text_value("a"), text_value("b")])),
            ok_value(Value::Record(vec![
                ("kind".to_owned(), variant_value("file", Vec::new())),
                ("size".to_owned(), Value::U64(3)),
            ])),
            ok_value(Value::I64(-5)),
            ok_value(Value::U64(9)),
        ])
    );
}

/// Row `ATK-broker-no-charge-without-await`: building the plan of a host
/// member 1,000 times and dropping it makes no request.
#[test]
fn atk_broker_no_charge_without_await() {
    let mut statements = (0..1_000)
        .map(|_| read_plan(var(0, r_type()), &["a"]))
        .collect::<Vec<_>>();
    statements.push(void());
    let main = procedure(
        "main",
        vec![r_type()],
        Type::Void,
        &[r_id()],
        seq(statements),
        1,
    );
    let mut host = Recording::default();
    run_main(
        vec![main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    )
    .unwrap();
    assert!(host.events.is_empty(), "{:?}", host.events);
}

/// The type of the plan of `R.read`.
fn read_intent() -> Type {
    Type::Intent(Box::new(vibra_ir::IntentType {
        result: read_result(),
        effects: row_of(&[r_id()]),
    }))
}

/// Row `V2-RUNTIME-await-twice-two-events`: one plan awaited twice is two
/// requests, invocations 2 and 3, both of parent 1.
#[test]
fn v2_runtime_await_twice_two_events() {
    let plan = || var(1, read_intent());
    let main = main_over_r(
        let_in(
            1,
            read_plan(var(0, r_type()), &["a"]),
            seq(vec![await_(plan()), ret(await_(plan()))]),
        ),
        2,
    );
    let mut host = Recording::default();
    run_main(
        vec![main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    )
    .unwrap();
    assert_eq!(host.ordinals(), [(2, Some(1)), (3, Some(1))]);
    assert_eq!(host.commits(), 2);
}

/// Row "invocation and parent": the body (1) awaits `outer` (2), which
/// awaits the pure `inner` (3) and then the host member (4, parent 2).
#[test]
fn invocation_and_parent() {
    let inner = procedure("inner", Vec::new(), Type::Void, &[], ret(void()), 0);
    let outer = procedure(
        "outer",
        vec![r_type()],
        read_result(),
        &[r_id()],
        seq(vec![
            await_(call(OWN, Vec::new())),
            let_in(
                1,
                read(var(0, r_type()), &["a"]),
                ret(var(1, read_result())),
            ),
        ]),
        2,
    );
    let main = main_over_r(
        let_in(
            1,
            await_(call(OWN + 1, vec![var(0, r_type())])),
            ret(var(1, read_result())),
        ),
        2,
    );
    let mut host = Recording::default();
    run_main(
        vec![inner, outer, main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    )
    .unwrap();
    assert_eq!(host.ordinals(), [(4, Some(2))]);
}

/// Row "parent across a tail await": `p` (2) tail-awaits the host member
/// (3, parent 2); the body then awaits the member again (4, parent 1).
#[test]
fn parent_across_a_tail_await() {
    let p = procedure(
        "p",
        vec![r_type()],
        read_result(),
        &[r_id()],
        ret(read(var(0, r_type()), &["a"])),
        1,
    );
    let main = main_over_r(
        seq(vec![
            await_(call(OWN, vec![var(0, r_type())])),
            ret(read(var(0, r_type()), &["b"])),
        ]),
        1,
    );
    let mut host = Recording::default();
    run_main(
        vec![p, main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    )
    .unwrap();
    assert_eq!(host.ordinals(), [(3, Some(2)), (4, Some(1))]);
}

/// Row "first invocation of an entry": the entry's plan is invocation 3,
/// so its `await` is 4 with parent 3.
#[test]
fn first_invocation_of_an_entry() {
    let main = main_over_r(ret(read(var(0, r_type()), &["a"])), 1);
    let mut host = Recording::default();
    run_main_from(
        vec![main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        3,
        &mut host,
        MemoryBudget::DEFAULT,
    )
    .unwrap();
    assert_eq!(host.ordinals(), [(4, Some(3))]);
}

/// Row "source origin": each request carries the origin of its `await`.
#[test]
fn source_origin() {
    let main = main_over_r(
        seq(vec![
            await_at(read_plan(var(0, r_type()), &["a"]), origin_at(10, 20)),
            ret(await_at(
                read_plan(var(0, r_type()), &["b"]),
                origin_at(30, 40),
            )),
        ]),
        1,
    );
    let mut host = Recording::default();
    run_main(
        vec![main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    )
    .unwrap();
    assert_eq!(
        host.requests()
            .into_iter()
            .map(|request| request.origin)
            .collect::<Vec<_>>(),
        [Some(origin_at(10, 20)), Some(origin_at(30, 40))]
    );
}

/// Row "model receiver": a held `Memory` selects its own `read`, and no
/// request is made.
#[test]
fn model_receiver() {
    let main = procedure(
        "main",
        Vec::new(),
        read_result(),
        &[r_id()],
        ret(read(widen(memory("memo")), &["a"])),
        0,
    );
    let mut host = Recording::default();
    let mut own = memory_members();
    own.push(main);
    let execution = run_main(
        own,
        vec![memory_definition()],
        vec![model_impl(memory_type(), OWN)],
        Vec::new(),
        &mut host,
    )
    .unwrap();
    assert_eq!(execution.value.value(), &ok_value(text_value("memo")));
    assert!(host.events.is_empty(), "{:?}", host.events);
}

/// Row "grant receiver through interface value": a grant passed where `R`
/// is expected and awaited in the callee is one request.
#[test]
fn grant_receiver_through_interface_value() {
    let reader = procedure(
        "reader",
        vec![r_type()],
        read_result(),
        &[r_id()],
        let_in(
            1,
            read(var(0, r_type()), &["a"]),
            ret(var(1, read_result())),
        ),
        2,
    );
    let main = main_over_r(
        let_in(
            1,
            await_(call(OWN, vec![var(0, r_type())])),
            ret(var(1, read_result())),
        ),
        2,
    );
    let mut host = Recording::default();
    run_main(
        vec![reader, main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(5))],
        &mut host,
    )
    .unwrap();
    assert_eq!(host.grants(), [token(5)]);
}

/// Row "reply of wrong shape": `Ok(U64(1))` for `fs.readText` is
/// `@runtime.invalidHostValue`, and nothing is committed.
#[test]
fn reply_of_wrong_shape() {
    let mut host = Recording::default();
    host.replies.push_back(Ok(HostReply::Ok(HostOk::U64(1))));
    assert_trap(read_once(&mut host), Trap::InvalidHostValue);
    assert_eq!(host.requests().len(), 1);
    assert_eq!(host.commits(), 0);
}

/// An error case of another error type is a reply of the wrong shape too.
#[test]
fn an_error_of_another_type_is_a_reply_of_the_wrong_shape() {
    let mut host = Recording::default();
    host.replies.push_back(Ok(HostReply::Err(
        HostErrorCase::new(HostErrorType::Io, HostErrorType::Io.cases()[0]).unwrap(),
    )));
    assert_trap(read_once(&mut host), Trap::InvalidHostValue);
    assert_eq!(host.commits(), 0);
}

/// Row "fault from context": `InvalidHostValue` is the trap.
#[test]
fn fault_from_context() {
    let mut host = Recording::default();
    host.replies
        .push_back(Err(HostFault::InvalidHostValue("a stale token".to_owned())));
    assert_trap(read_once(&mut host), Trap::InvalidHostValue);
    assert_eq!(host.commits(), 0);
}

/// A fault of the commit is the trap too.
#[test]
fn a_fault_of_the_commit_is_the_trap() {
    let mut host = Recording {
        commit_fault: Some(HostFault::InvalidHostValue("not pending".to_owned())),
        ..Recording::default()
    };
    assert_trap(read_once(&mut host), Trap::InvalidHostValue);
}

/// Row "memory fault": `MemoryExhausted` is the host event.
#[test]
fn memory_fault() {
    let mut host = Recording::default();
    host.replies.push_back(Err(HostFault::MemoryExhausted));
    let outcome = read_once(&mut host);
    assert!(
        matches!(outcome, Err(RuntimeError::MemoryExhausted { .. })),
        "{outcome:?}"
    );
    assert_eq!(host.commits(), 0);
}

/// `main(r)` whose continuation of `R.read`, `advance`, runs on the reply
/// before anything else, and which then returns `read_refused()`.
fn advance_then_refuse(advance: impl FnOnce(Node) -> Node) -> CheckedFunction {
    main_over_r(
        let_in(
            1,
            advance(read(var(0, r_type()), &["notes.txt"])),
            ret(read_refused()),
        ),
        2,
    )
}

/// F7 of the review of grants: a trap on each side of the commit. 06 "Audit
/// trace": the event is appended "after its continuation has advanced", and
/// "a trap … appends no completed success". Between the adapter's return
/// and the commit, the reply is checked against the row, accounted, and
/// handed to the `await` that waits for it; a reply of another shape traps
/// there, `@runtime.invalidHostValue`, and its receipt is dropped. Once the
/// `await` has the value the event is committed, and a trap in the next
/// node, here a match with no arm for `Err(notFound)` (a continuation the
/// checker never builds, `@runtime.invalidCheckedProgram`), keeps that one
/// event: it neither drops it nor adds another.
#[test]
fn atk_trap_on_each_side_of_the_commit() {
    let mut host = Recording::default();
    host.replies.push_back(Ok(HostReply::Ok(HostOk::U64(1))));
    let outcome = run_main(
        vec![advance_then_refuse(|reply| reply)],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    );
    assert_trap(outcome, Trap::InvalidHostValue);
    assert_eq!(host.requests().len(), 1);
    assert_eq!(host.commits(), 0, "a trap in the window drops the receipt");
    let only_ok = |reply: Node| {
        node(Expr::Match {
            subject: Box::new(reply),
            arms: vec![MatchArm {
                pattern: Pattern::Case {
                    case: "ok".to_owned(),
                    fields: vec![("value".to_owned(), Pattern::Discard)],
                },
                body: read_refused(),
            }],
            value_type: read_result(),
        })
    };
    let mut host = Recording::default();
    host.replies.push_back(Ok(HostReply::Err(
        HostErrorCase::new(HostErrorType::Fs, "notFound").unwrap(),
    )));
    let outcome = run_main(
        vec![advance_then_refuse(only_ok)],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    );
    assert_trap(outcome, Trap::InvalidCheckedProgram);
    assert_eq!(host.requests().len(), 1);
    assert_eq!(host.commits(), 1, "a trap after the window keeps the event");
    // The same continuation on an `ok` reply runs to the end, one commit.
    let mut host = Recording::default();
    run_main(
        vec![advance_then_refuse(only_ok)],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    )
    .unwrap();
    assert_eq!(host.commits(), 1);
}

/// F7 of the review of grants: memory exhaustion in the window between the
/// adapter's return and the commit. With 100 bytes of room left, the reply
/// is a 100,000-byte text, which is accounted when the `await` takes it:
/// the run is the host event `@runtime.memoryExhausted`, and 06 "Audit
/// trace" appends no event, since "a host event … appends no completed
/// success". The request was made; its receipt is dropped uncommitted.
#[test]
fn atk_exhaustion_in_the_window_before_the_commit() {
    let run = |budget: MemoryBudget, reply: &str| {
        let mut host = Recording::default();
        host.replies
            .push_back(Ok(HostReply::Ok(HostOk::Text(reply.to_owned()))));
        let outcome = run_main_from(
            vec![advance_then_refuse(|reply| reply)],
            Vec::new(),
            Vec::new(),
            vec![EntryArgument::Grant(token(1))],
            1,
            &mut host,
            budget,
        );
        (outcome, host)
    };
    let (first, host) = run(MemoryBudget::DEFAULT, "");
    first.unwrap();
    let live = MemoryBudget::DEFAULT.bytes - host.requests()[0].room;
    let (outcome, host) = run(MemoryBudget::new(live + 100), &"x".repeat(100_000));
    assert!(
        matches!(outcome, Err(RuntimeError::MemoryExhausted { .. })),
        "{outcome:?}"
    );
    assert_eq!(host.requests().len(), 1);
    assert_eq!(host.commits(), 0);
}

/// F7 of the review of grants: memory exhaustion after the commit. The
/// continuation, once the `await` has the reply, builds a 200,000-byte
/// text. Over every budget from below the request to above the whole run,
/// a run that made the request committed its event exactly once, whether
/// it was then exhausted or completed: the event of a completed call is
/// appended "after its continuation has advanced" (06 "Audit trace"), and
/// an exhaustion that follows neither drops it nor adds another. Both
/// outcomes are reached.
#[test]
fn an_exhaustion_after_the_commit_keeps_the_event() {
    let big = "y".repeat(200_000);
    let run = |budget: MemoryBudget| {
        let mut host = Recording::default();
        host.replies
            .push_back(Ok(HostReply::Ok(HostOk::Text(String::new()))));
        let main = main_over_r(
            let_in(
                1,
                read(var(0, r_type()), &["notes.txt"]),
                seq(vec![text(&big), ret(read_refused())]),
            ),
            2,
        );
        let outcome = run_main_from(
            vec![main],
            Vec::new(),
            Vec::new(),
            vec![EntryArgument::Grant(token(1))],
            1,
            &mut host,
            budget,
        );
        (outcome, host)
    };
    let (first, host) = run(MemoryBudget::DEFAULT);
    first.unwrap();
    let live = MemoryBudget::DEFAULT.bytes - host.requests()[0].room;
    let (mut exhausted, mut completed) = (0, 0);
    for bytes in (live.saturating_sub(1_024)..live + 400_000).step_by(997) {
        let (outcome, host) = run(MemoryBudget::new(bytes));
        if host.requests().is_empty() {
            continue;
        }
        assert_eq!(host.commits(), 1, "budget {bytes}: {outcome:?}");
        match outcome {
            Err(RuntimeError::MemoryExhausted { .. }) => exhausted += 1,
            Ok(_) => completed += 1,
            other => panic!("budget {bytes}: {other:?}"),
        }
    }
    assert!(
        exhausted > 0,
        "some budget exhausts the run after the commit"
    );
    assert!(completed > 0, "some budget completes the run");
}
/// `FsError.<first case>`, as a fallback `Result.err` of `R.read`.
fn read_refused() -> Node {
    case(
        read_result(),
        "err",
        vec![(
            "error",
            case(
                error_type(HostErrorType::Fs),
                HostErrorType::Fs.cases()[0],
                Vec::new(),
            ),
        )],
    )
}

/// Reads through the `ok` of `narrowed`, a `Result(R ScopeError)`, or
/// gives `read_refused()`.
fn read_if_ok(narrowed: Node) -> Node {
    node(Expr::Match {
        subject: Box::new(narrowed),
        arms: vec![
            MatchArm {
                pattern: Pattern::Case {
                    case: "ok".to_owned(),
                    fields: vec![("value".to_owned(), Pattern::Bind(1))],
                },
                body: read(var(1, r_type()), &["a"]),
            },
            MatchArm {
                pattern: Pattern::Discard,
                body: read_refused(),
            },
        ],
        value_type: read_result(),
    })
}

/// Row "restrict on a grant": the narrowed grant is `Result.ok` holding the
/// returned token, and the request holds the policy's neutral value.
#[test]
fn restrict_on_a_grant() {
    let policy = case(policy_type(), "nothing", Vec::new());
    let main = main_over_r(
        ret(read_if_ok(narrowing("restrict", var(0, r_type()), policy))),
        2,
    );
    let mut host = Recording::default();
    host.narrowed.push_back(Ok(Narrowed::Grant(token(2))));
    let execution = run_main(
        vec![main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    )
    .unwrap();
    assert_eq!(execution.value.value(), &ok_value(text_value("text")));
    assert_eq!(
        host.events[0],
        Event::Restrict(token(1), variant_value("nothing", Vec::new()))
    );
    assert_eq!(host.grants(), [token(2)]);
}

/// Row "restrict refused": `Refused(policyTooComplex)` is
/// `Result.err(ScopeError.policyTooComplex)`.
#[test]
fn restrict_refused() {
    let policy = case(policy_type(), "everything", Vec::new());
    let main = procedure(
        "main",
        vec![r_type()],
        result(r_type(), scope_error()),
        &[r_id()],
        ret(narrowing("restrict", var(0, r_type()), policy)),
        1,
    );
    let mut host = Recording::default();
    host.narrowed
        .push_back(Ok(Narrowed::Refused(ScopeErrorCase::PolicyTooComplex)));
    let execution = run_main(
        vec![main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    )
    .unwrap();
    assert_eq!(execution.value.value(), &err_value("policyTooComplex"));
}

/// A refusal the operation never gives is a reply of the wrong shape:
/// `foreignRoot` from `restrict`, any refusal from `limit`.
#[test]
fn a_refusal_the_operation_never_gives_is_the_trap() {
    let policy = case(policy_type(), "everything", Vec::new());
    let main = procedure(
        "main",
        vec![r_type()],
        result(r_type(), scope_error()),
        &[r_id()],
        ret(narrowing("restrict", var(0, r_type()), policy)),
        1,
    );
    let mut host = Recording::default();
    host.narrowed
        .push_back(Ok(Narrowed::Refused(ScopeErrorCase::ForeignRoot)));
    let outcome = run_main(
        vec![main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    );
    assert_trap(outcome, Trap::InvalidHostValue);
    let main = main_over_r(
        ret(read(
            narrowing("limit", var(0, r_type()), limits(3)),
            &["a"],
        )),
        1,
    );
    let mut host = Recording::default();
    host.narrowed
        .push_back(Ok(Narrowed::Refused(ScopeErrorCase::PolicyTooComplex)));
    let outcome = run_main(
        vec![main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    );
    assert_trap(outcome, Trap::InvalidHostValue);
}

/// `invalidPath` is the error of a rejected `RelativePath` construction and
/// nothing else, so `restrict`, `sub`, and `limit` never give it: a context
/// that refuses with it replies with the wrong shape.
#[test]
fn invalid_path_from_a_narrowing_is_the_trap() {
    let policy = case(policy_type(), "everything", Vec::new());
    let mains = [
        procedure(
            "main",
            vec![r_type()],
            result(r_type(), scope_error()),
            &[r_id()],
            ret(narrowing("restrict", var(0, r_type()), policy)),
            1,
        ),
        procedure(
            "main",
            vec![r_type()],
            result(r_type(), scope_error()),
            &[r_id()],
            ret(narrowing("sub", var(0, r_type()), path_lit(&["logs"]))),
            1,
        ),
        main_over_r(
            ret(read(
                narrowing("limit", var(0, r_type()), limits(3)),
                &["a"],
            )),
            1,
        ),
    ];
    for main in mains {
        let mut host = Recording::default();
        host.narrowed
            .push_back(Ok(Narrowed::Refused(ScopeErrorCase::InvalidPath)));
        let outcome = run_main(
            vec![main],
            Vec::new(),
            Vec::new(),
            vec![EntryArgument::Grant(token(1))],
            &mut host,
        );
        assert_trap(outcome, Trap::InvalidHostValue);
    }
}

/// `sub` on a grant: the prefix's names, and the `ok` of the returned
/// token.
#[test]
fn sub_on_a_grant() {
    let main = main_over_r(
        ret(read_if_ok(narrowing(
            "sub",
            var(0, r_type()),
            path_lit(&["logs", "today"]),
        ))),
        2,
    );
    let mut host = Recording::default();
    host.narrowed.push_back(Ok(Narrowed::Grant(token(2))));
    run_main(
        vec![main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    )
    .unwrap();
    assert_eq!(
        host.events[0],
        Event::Sub(token(1), vec!["logs".to_owned(), "today".to_owned()])
    );
    assert_eq!(host.grants(), [token(2)]);
}

/// Row "limit on a grant": the returned token, and the bounds the record
/// sets.
#[test]
fn limit_on_a_grant() {
    let main = main_over_r(
        ret(read(
            narrowing("limit", var(0, r_type()), limits(3)),
            &["a"],
        )),
        1,
    );
    let mut host = Recording::default();
    host.narrowed.push_back(Ok(Narrowed::Grant(token(2))));
    run_main(
        vec![main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    )
    .unwrap();
    assert_eq!(
        host.events[0],
        Event::Limit(
            token(1),
            GrantLimits {
                invocations: Some(3),
                ..GrantLimits::default()
            }
        )
    );
    assert_eq!(host.grants(), [token(2)]);
}

/// Row "narrowing on a model": a held `Memory` selects its own `limit`,
/// which returns itself, and no narrowing is asked.
#[test]
fn narrowing_on_a_model() {
    let main = procedure(
        "main",
        Vec::new(),
        read_result(),
        &[r_id()],
        ret(read(
            narrowing("limit", widen(memory("memo")), limits(1)),
            &["a"],
        )),
        0,
    );
    let mut host = Recording::default();
    let mut own = memory_members();
    own.push(main);
    let execution = run_main(
        own,
        vec![memory_definition()],
        vec![model_impl(memory_type(), OWN)],
        Vec::new(),
        &mut host,
    )
    .unwrap();
    assert_eq!(execution.value.value(), &ok_value(text_value("memo")));
    assert!(host.events.is_empty(), "{:?}", host.events);
}

/// Row "a forwarding model narrows its grant": `Forward`'s `limit` narrows
/// the grant it holds, once, and the model it returns holds the returned
/// token.
#[test]
fn a_forwarding_model_narrows_its_grant() {
    let inner = || field(var(0, forward_type()), "inner");
    let forward_read = procedure(
        "Forward.read",
        vec![forward_type(), relative_path()],
        read_result(),
        &[r_id()],
        ret(await_(op_plan(
            HostOp::FsReadText,
            inner(),
            vec![var(1, relative_path())],
        ))),
        2,
    );
    let forward_limit = function(
        "Forward.limit",
        signature(vec![forward_type(), limits_type()], forward_type()),
        record(
            forward_type(),
            vec![("inner", narrowing("limit", inner(), var(1, limits_type())))],
        ),
        2,
    );
    let main = main_over_r(
        ret(read(
            narrowing(
                "limit",
                widen(record(forward_type(), vec![("inner", var(0, r_type()))])),
                limits(2),
            ),
            &["a"],
        )),
        1,
    );
    let mut host = Recording::default();
    host.narrowed.push_back(Ok(Narrowed::Grant(token(2))));
    let [forward_restrict, forward_sub] = keeps_itself(forward_type());
    run_main(
        vec![
            forward_read,
            forward_limit,
            forward_restrict,
            forward_sub,
            main,
        ],
        vec![forward_definition()],
        vec![model_impl(forward_type(), OWN)],
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    )
    .unwrap();
    let narrowings = host
        .events
        .iter()
        .filter(|event| matches!(event, Event::Limit(..)))
        .collect::<Vec<_>>();
    assert_eq!(
        narrowings,
        [&Event::Limit(
            token(1),
            GrantLimits {
                invocations: Some(2),
                ..GrantLimits::default()
            }
        )]
    );
    assert_eq!(host.grants(), [token(2)]);
}

/// Row "narrowing is no invocation": a `limit` takes no ordinal, so the host
/// call after it is invocation 2.
#[test]
fn narrowing_is_no_invocation() {
    let main = main_over_r(
        ret(read(
            narrowing("limit", var(0, r_type()), limits(3)),
            &["a"],
        )),
        1,
    );
    let mut host = Recording::default();
    let execution = run_main(
        vec![main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    )
    .unwrap();
    assert_eq!(host.ordinals(), [(2, Some(1))]);
    assert_eq!(execution.instrumentation.invocation_count, 2);
}

/// The fields of the record `TestWorld.directory` returns: each name and
/// the interface of its grant.
fn directory_fields() -> Vec<(&'static str, HostInterface)> {
    let HostType::Record(fields) = TestWorldOp::Directory.row().result else {
        panic!("`directory` returns a record");
    };
    fields
        .iter()
        .map(|field| match field.ty {
            HostType::Interface(interface) => (field.name, interface),
            other => panic!("a field of {}", other.spelling()),
        })
        .collect()
}

/// Row "test-world row": `TestWorld.directory` in a test body, whose world
/// `run_test` hands in, issues a bundle of six grants, and the body's
/// record holds each under its name.
#[test]
fn test_world_row() {
    let fields = directory_fields();
    assert_eq!(fields.len(), 6);
    let bundle = host_type(TestWorldOp::Directory.row().result);
    let files = lit(dict(relative_path(), str_type()), Value::Dict(Vec::new()));
    let mut statements = fields
        .iter()
        .map(|(name, interface)| {
            sampled(interface.operation(), field(var(1, bundle.clone()), name))
        })
        .collect::<Vec<_>>();
    statements.push(void());
    let effects = iter::once(world_id())
        .chain(fields.iter().map(|(_, interface)| interface_id(*interface)))
        .collect::<Vec<_>>();
    let body = CheckedFunction {
        context: BodyContext::Procedure(row_of(&effects)),
        ..function(
            "test",
            signature(vec![world_type()], Type::Void),
            let_in(
                1,
                await_(contract(
                    world_id(),
                    TestWorldOp::Directory.member(),
                    world_type(),
                    vec![var(0, world_type())],
                    vec![("files".to_owned(), files)],
                )),
                seq(statements),
            ),
            2,
        )
    };
    let mut host = Recording::default();
    host.issued.push_back(Ok(Issued::Bundle(
        fields
            .iter()
            .zip(11..)
            .map(|((name, _), slot)| (*name, token(slot)))
            .collect(),
    )));
    let set = set_of(vec![body], Vec::new(), Vec::new());
    let execution = run_test(
        &program(set, OWN),
        Some(token(9)),
        Some(&mut host),
        MemoryBudget::DEFAULT,
    )
    .unwrap();
    assert_eq!(execution.failure, None);
    assert_eq!(
        host.events[0],
        Event::World(
            TestWorldOp::Directory,
            token(9),
            vec![Value::Dict(Vec::new())],
            2
        )
    );
    assert_eq!(
        host.requests()
            .iter()
            .map(|request| (request.op, request.grant))
            .collect::<Vec<_>>(),
        fields
            .iter()
            .zip(11..)
            .map(|((_, interface), slot)| (interface.operation(), token(slot)))
            .collect::<Vec<_>>()
    );
}

/// Row "test-world output": `{stdout: Bytes stderr: Bytes}` of what the
/// context issues.
#[test]
fn test_world_output() {
    let output = host_type(TestWorldOp::Output.row().result);
    let main = procedure(
        "main",
        vec![world_type()],
        output,
        &[world_id()],
        ret(await_(contract(
            world_id(),
            TestWorldOp::Output.member(),
            world_type(),
            vec![var(0, world_type())],
            Vec::new(),
        ))),
        1,
    );
    let mut host = Recording::default();
    host.issued.push_back(Ok(Issued::Output {
        stdout: b"out".to_vec(),
        stderr: b"err".to_vec(),
    }));
    let execution = run_main(
        vec![main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(9))],
        &mut host,
    )
    .unwrap();
    assert_eq!(
        execution.value.value(),
        &Value::Record(vec![
            ("stderr".to_owned(), Value::Bytes(b"err".to_vec())),
            ("stdout".to_owned(), Value::Bytes(b"out".to_vec())),
        ])
    );
    assert_eq!(
        host.events,
        [Event::World(TestWorldOp::Output, token(9), Vec::new(), 2)]
    );
}

/// The entry of row "entry arguments": `main(a: R o: Option(R) b: {reader:
/// R})` reads through `a`, through `o` when it is present, and through
/// `b.reader`.
fn entry_with_arguments() -> CheckedFunction {
    let bundle =
        Type::Record(NamedTypes::new([("reader".to_owned(), r_type())]).unwrap());
    let optional = node(Expr::Match {
        subject: Box::new(var(1, option(r_type()))),
        arms: vec![
            MatchArm {
                pattern: Pattern::Case {
                    case: "some".to_owned(),
                    fields: vec![("value".to_owned(), Pattern::Bind(3))],
                },
                body: read(var(3, r_type()), &["o"]),
            },
            MatchArm {
                pattern: Pattern::Discard,
                body: read_refused(),
            },
        ],
        value_type: read_result(),
    });
    procedure(
        "main",
        vec![r_type(), option(r_type()), bundle.clone()],
        Type::Void,
        &[r_id()],
        seq(vec![
            read(var(0, r_type()), &["a"]),
            optional,
            read(field(var(2, bundle), "reader"), &["b"]),
            void(),
        ]),
        4,
    )
}

/// Row "entry arguments": a grant, an absent optional grant, and a bundle;
/// then a present optional grant.
#[test]
fn entry_arguments() {
    let bundle = || {
        EntryArgument::Record(vec![(
            "reader".to_owned(),
            EntryArgument::Grant(token(2)),
        )])
    };
    let mut host = Recording::default();
    run_main(
        vec![entry_with_arguments()],
        Vec::new(),
        Vec::new(),
        vec![
            EntryArgument::Grant(token(1)),
            EntryArgument::Absent,
            bundle(),
        ],
        &mut host,
    )
    .unwrap();
    assert_eq!(host.grants(), [token(1), token(2)]);
    let mut host = Recording::default();
    run_main(
        vec![entry_with_arguments()],
        Vec::new(),
        Vec::new(),
        vec![
            EntryArgument::Grant(token(1)),
            EntryArgument::Grant(token(3)),
            bundle(),
        ],
        &mut host,
    )
    .unwrap();
    assert_eq!(host.grants(), [token(1), token(3), token(2)]);
}

/// An entry argument that is no value of its parameter is
/// `@runtime.invalidHostValue`.
#[test]
fn an_entry_argument_that_does_not_fit_is_the_trap() {
    let mut host = Recording::default();
    let outcome = run_main(
        vec![entry_with_arguments()],
        Vec::new(),
        Vec::new(),
        vec![
            EntryArgument::Absent,
            EntryArgument::Absent,
            EntryArgument::Record(Vec::new()),
        ],
        &mut host,
    );
    assert_trap(outcome, Trap::InvalidHostValue);
    assert!(host.events.is_empty());
}

/// Row "entry result": `ExitCode(3u8)` is the execution's value.
#[test]
fn entry_result() {
    let main = procedure(
        "main",
        Vec::new(),
        exit_code(),
        &[],
        ret(construct(
            exit_code(),
            Construction::Wrapper(Box::new(lit(Type::U8, Value::U8(3)))),
        )),
        0,
    );
    let mut host = Recording::default();
    let execution =
        run_main(vec![main], Vec::new(), Vec::new(), Vec::new(), &mut host).unwrap();
    assert_eq!(execution.value.value_type(), &exit_code());
    assert_eq!(
        execution.value.value(),
        &Value::Wrapper(Box::new(Value::U8(3)))
    );
}

/// Row `ATK-encode-grant`: a run whose result is a grant has no value
/// encoding.
#[test]
fn atk_encode_grant() {
    let main = procedure(
        "main",
        vec![r_type()],
        r_type(),
        &[],
        ret(var(0, r_type())),
        1,
    );
    let mut host = Recording::default();
    let outcome = run_main(
        vec![main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    );
    assert!(
        matches!(&outcome, Err(RuntimeError::UnsupportedValue { value_type }) if *value_type == r_type()),
        "{outcome:?}"
    );
}

/// Row "grant captured in a closure": a `Proc` closure that captures a
/// grant, awaited after the frame that made it returned, is one request
/// with that token.
#[test]
fn grant_captured_in_a_closure() {
    let closure_type = Type::Proc(Box::new(ProcType {
        parameters: Vec::new(),
        labels: NamedTypes::empty(),
        result: read_result(),
        effects: row_of(&[r_id()]),
    }));
    let make = function(
        "make",
        signature(vec![r_type()], closure_type.clone()),
        closure(
            closure_type.clone(),
            0,
            vec![(r_type(), var(0, r_type()))],
            ret(read(crate::captured(0, r_type()), &["a"])),
        ),
        1,
    );
    let main = main_over_r(
        let_in(
            1,
            call(OWN, vec![var(0, r_type())]),
            ret(await_(call_value(var(1, closure_type), Vec::new()))),
        ),
        2,
    );
    let mut host = Recording::default();
    let execution = run_main(
        vec![make, main],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(4))],
        &mut host,
    )
    .unwrap();
    assert_eq!(execution.value.value(), &ok_value(text_value("text")));
    assert_eq!(host.grants(), [token(4)]);
}

/// The ambient APIs: the filesystem, the environment, the clocks, the
/// operating system's random source, and the process's standard streams.
const AMBIENT_PATTERNS: &[&str] = &[
    "std::fs",
    "fs::",
    "File::open",
    "File::create",
    "read_dir(",
    "std::env",
    "env::var",
    "env::args",
    "env::current_dir",
    "SystemTime",
    "Instant::now",
    "std::time::Instant",
    "UNIX_EPOCH",
    "getrandom",
    "RandomState",
    "std::io::stdin",
    "io::stdin(",
    "io::stdout(",
    "io::stderr(",
    "std::process",
    "println!",
    "eprintln!",
    "print!(",
    "eprint!(",
];

/// The attribute that gates host-language test code.
const TEST_GATE: &str = "#[cfg(test)]";

/// Whether `text`, trimmed, opens a module: `mod name`, with or without a
/// visibility.
fn opens_module(text: &str) -> bool {
    let text = text.trim_start();
    let text = ["pub(crate) ", "pub(super) ", "pub "]
        .iter()
        .find_map(|visibility| text.strip_prefix(visibility))
        .unwrap_or(text);
    text.starts_with("mod ")
}

/// The lines of `source`, numbered from one, that are not in a
/// `#[cfg(test)]` module: the attribute, the `mod` it gates, and that
/// module's block, matched brace by brace, are left out. A `#[cfg(test)]`
/// field, function, or other item is kept, and so is every line after the
/// module.
fn outside_test_modules(source: &str) -> Vec<(usize, &str)> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut kept = Vec::new();
    let mut index = 0;
    while let Some(line) = lines.get(index) {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(TEST_GATE) else {
            kept.push((index + 1, *line));
            index += 1;
            continue;
        };
        // The gated item: on the attribute's line, or on the next line that
        // is neither an attribute nor a comment.
        let mut item = index;
        if rest.trim().is_empty() {
            item += 1;
            while lines.get(item).is_some_and(|text| {
                let text = text.trim_start();
                text.starts_with("#[") || text.starts_with("//")
            }) {
                item += 1;
            }
        }
        let opens = lines.get(item).is_some_and(|text| {
            let text = if item == index { rest } else { text };
            opens_module(text)
        });
        if !opens {
            kept.push((index + 1, *line));
            index += 1;
            continue;
        }
        // Skip through the line that closes the module's block, or through
        // `mod name;`.
        let mut depth: i64 = 0;
        let mut opened = false;
        let mut end = item;
        while let Some(text) = lines.get(end) {
            let text = if end == index { rest } else { text };
            let code = text.split("//").next().unwrap_or(text);
            for character in code.chars() {
                match character {
                    '{' => {
                        depth += 1;
                        opened = true;
                    }
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            end += 1;
            if (opened && depth <= 0) || (!opened && code.trim_end().ends_with(';')) {
                break;
            }
        }
        index = end;
    }
    kept
}

/// The ambient APIs `source` names outside comments and its `#[cfg(test)]`
/// modules, which are host-language test code and run no program.
fn ambient_references(source: &str) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    for (number, line) in outside_test_modules(source) {
        if line.trim_start().starts_with("//") {
            continue;
        }
        let code = line.split("//").next().unwrap_or(line);
        for pattern in AMBIENT_PATTERNS {
            if code.contains(pattern) {
                found.push((number, *pattern));
            }
        }
    }
    found
}

/// Row `ATK-ambient-read-in-interp`: no source of the interpreter names an
/// ambient API; every host effect passes through the context.
#[test]
fn atk_ambient_read_in_interp() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut pending = vec![root];
    let mut scanned = 0;
    let mut found = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                scanned += 1;
                let source = std::fs::read_to_string(&path).unwrap();
                for (line, pattern) in ambient_references(&source) {
                    found.push(format!("{}:{line} names `{pattern}`", path.display()));
                }
            }
        }
    }
    assert!(scanned >= 10, "the scan reads the interpreter's sources");
    assert!(found.is_empty(), "ambient reads:\n{}", found.join("\n"));
    // The scan is not vacuous.
    assert_eq!(
        ambient_references("let now = std::time::SystemTime::now();"),
        [(1, "SystemTime")]
    );
}

/// F3 of the review of grants: the scan skips only a `#[cfg(test)]`
/// module, never the rest of a file after a `#[cfg(test)]` field or item,
/// and the delivery observer of `src/host.rs` is such a field.
#[test]
fn atk_ambient_read_after_a_test_only_field() {
    let field = "struct Slot {\n    #[cfg(test)]\n    delivered: u8,\n}\n\
                 fn read() { let _ = std::fs::read(\"x\"); }\n";
    assert_eq!(ambient_references(field), [(5, "std::fs"), (5, "fs::")]);
    let module = "#[cfg(test)]\nmod tests {\n    fn f() { std::env::args(); }\n}\n\n\
                  fn after() { let _ = std::env::var(\"X\"); }\n";
    assert_eq!(
        ambient_references(module),
        [(6, "std::env"), (6, "env::var")]
    );
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("host.rs"),
    )
    .unwrap();
    let lines = source.lines().collect::<Vec<_>>();
    let field = lines
        .iter()
        .position(|line| line.trim_start().starts_with("pub(crate) delivered:"))
        .expect("the delivery observer");
    assert_eq!(
        lines[..field]
            .iter()
            .rev()
            .map(|line| line.trim())
            .find(|line| !line.starts_with("///")),
        Some("#[cfg(test)]"),
        "the delivery observer exists only in the unit tests"
    );
}

/// Row "no context": a host body reached with no host context is
/// `@tool.unavailable`. A test body that takes a world is handed one, and
/// no context; under `run`, an entry that takes a grant is unavailable
/// before it starts.
#[test]
fn no_context() {
    let body = CheckedFunction {
        context: BodyContext::Procedure(row_of(&[world_id()])),
        ..function(
            "test",
            signature(vec![world_type()], Type::Void),
            seq(vec![
                await_(contract(
                    world_id(),
                    TestWorldOp::Output.member(),
                    world_type(),
                    vec![var(0, world_type())],
                    Vec::new(),
                )),
                void(),
            ]),
            1,
        )
    };
    let set = set_of(vec![body], Vec::new(), Vec::new());
    let outcome = run_test(
        &program(set, OWN),
        Some(token(9)),
        None,
        MemoryBudget::DEFAULT,
    );
    assert!(
        matches!(&outcome, Err(RuntimeError::Unavailable { message }) if message.contains("testWorld.output")),
        "{outcome:?}"
    );
    let main = CheckedFunction {
        context: BodyContext::Procedure(row_of(&[r_id()])),
        ..function(
            "main",
            signature(vec![r_type()], read_result()),
            ret(read(var(0, r_type()), &["a"])),
            1,
        )
    };
    let set = set_of(vec![main], Vec::new(), Vec::new());
    let outcome = run(&program(set, OWN), MemoryBudget::DEFAULT);
    assert!(
        matches!(&outcome, Err(RuntimeError::Unavailable { .. })),
        "{outcome:?}"
    );
}
/// `down(r n)`: awaits `down(r n - 1)`, not in tail position, and at `0`
/// reads through `r`.
fn down() -> CheckedFunction {
    procedure(
        "down",
        vec![r_type(), Type::U64],
        read_result(),
        &[r_id()],
        ret(if_else(
            u64_equal(var(1, Type::U64), u64_lit(0)),
            read(var(0, r_type()), &["a"]),
            let_in(
                2,
                await_(call(
                    OWN,
                    vec![var(0, r_type()), u64_sub(var(1, Type::U64), u64_lit(1))],
                )),
                var(2, read_result()),
            ),
        )),
        3,
    )
}

/// Row "deep await chain": 100,000 nested procedures ending in a host call
/// run on a 256 KiB thread and make one request.
#[test]
fn deep_await_chain() {
    const DEPTH: u64 = 100_000;
    let requests = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let main = main_over_r(
                ret(await_(call(OWN, vec![var(0, r_type()), u64_lit(DEPTH)]))),
                1,
            );
            let mut host = Recording::default();
            run_main(
                vec![down(), main],
                Vec::new(),
                Vec::new(),
                vec![EntryArgument::Grant(token(1))],
                &mut host,
            )
            .unwrap();
            host.ordinals()
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].0, DEPTH + 3);
}

// ----- the grant rows, for `intrinsics.rs` -----

/// The registry row `intrinsic` on a sealed grant: one narrowing of the
/// grant 1, whose result holds the returned grant 2.
pub(crate) fn narrowing_row(intrinsic: CompilerIntrinsic) {
    let (member, operand) = match intrinsic {
        CompilerIntrinsic::GrantLimit => ("limit", limits(5)),
        CompilerIntrinsic::GrantRestrict => {
            ("restrict", case(policy_type(), "everything", Vec::new()))
        }
        CompilerIntrinsic::GrantSub => ("sub", path_lit(&["d"])),
        other => panic!("`{}` is no grant row", other.symbol()),
    };
    let narrowed = narrowing(member, var(0, r_type()), operand);
    let body = if intrinsic == CompilerIntrinsic::GrantLimit {
        ret(read(narrowed, &["a"]))
    } else {
        ret(read_if_ok(narrowed))
    };
    let mut host = Recording::default();
    host.narrowed.push_back(Ok(Narrowed::Grant(token(2))));
    run_main(
        vec![main_over_r(body, 2)],
        Vec::new(),
        Vec::new(),
        vec![EntryArgument::Grant(token(1))],
        &mut host,
    )
    .unwrap();
    let expected = match intrinsic {
        CompilerIntrinsic::GrantLimit => Event::Limit(
            token(1),
            GrantLimits {
                invocations: Some(5),
                ..GrantLimits::default()
            },
        ),
        CompilerIntrinsic::GrantRestrict => {
            Event::Restrict(token(1), variant_value("everything", Vec::new()))
        }
        _ => Event::Sub(token(1), vec!["d".to_owned()]),
    };
    assert_eq!(host.events[0], expected, "{}", intrinsic.symbol());
    assert_eq!(host.grants(), [token(2)], "{}", intrinsic.symbol());
}
