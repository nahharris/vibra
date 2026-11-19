//! The `vibra-interp` host tests, one test target, over hand-built checked
//! IR.
//!
//! Every expected value is written by hand from `docs/spec/06-runtime.md`
//! and `docs/spec/02-type-system.md`, or is a vector of the compiler
//! intrinsic registry, which `vibra-ir` writes by hand from the same
//! chapter. None is produced by this crate and copied back.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

mod depth;
mod evaluation;
mod intrinsics;
mod observation;
mod tests_and_dispatch;

use std::sync::Arc;

use vibra_diagnostics::ByteSpan;
use vibra_interp::{Evaluation, Limits, MemoryBudget, evaluate};
use vibra_ir::external::{CompilerIntrinsic, IntegerOp, IntegerType};
use vibra_ir::{
    Arguments, CallTarget, Case, CheckedFunction, CheckedGlobal, CheckedModuleSet,
    CheckedProgram, ClosureBody, Constant, Construction, Expr, FnType, FunctionBody,
    Implementation, ImplementationRecord, InterfaceDefinition, InterfaceKey,
    InterfaceMember, Library, LibraryTypes, ModuleSetParts, NamedTypes, Node,
    SignatureRecord, SourceOrigin, Type, TypeBody, TypeDefinition, TypeId, Value,
    Visibility,
};

/// The chapter's placeholder identity.
pub const SEED: &str =
    "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad";

pub fn id(path: &str) -> TypeId {
    TypeId::new(path, SEED).unwrap()
}

pub fn origin() -> SourceOrigin {
    SourceOrigin::new("test.vibra", ByteSpan::new(0, 0))
}

/// An origin that a test can recognize: `test.vibra` at `start..end`.
pub fn origin_at(start: usize, end: usize) -> SourceOrigin {
    SourceOrigin::new("test.vibra", ByteSpan::new(start, end))
}

pub fn node(expr: Expr) -> Node {
    Node::new(expr, origin())
}

/// The canonical path of each library declaration in the fixtures.
pub fn path(library: Library) -> &'static str {
    match library {
        Library::Bool => "std.bool.Bool",
        Library::Str => "std.str.Str",
        Library::Bytes => "std.bytes.Bytes",
        Library::Option => "std.option.Option",
        Library::Result => "std.result.Result",
        Library::Dict => "std.dict.Dict",
        Library::Iter => "std.iter.Iter",
        Library::Equatable => "std.equatable.Equatable",
        Library::Ordered => "std.ordered.Ordered",
        Library::Ordering => "std.ordering.Ordering",
        Library::ArithmeticError => "std.number.ArithmeticError",
        Library::ConversionError => "std.number.ConversionError",
    }
}

pub fn library_type(library: Library, arguments: Vec<Type>) -> Type {
    Type::Declared(id(path(library)), arguments)
}

pub fn bool_type() -> Type {
    library_type(Library::Bool, Vec::new())
}

pub fn str_type() -> Type {
    library_type(Library::Str, Vec::new())
}

pub fn bytes_type() -> Type {
    library_type(Library::Bytes, Vec::new())
}

pub fn ordering_type() -> Type {
    library_type(Library::Ordering, Vec::new())
}

pub fn option(value: Type) -> Type {
    library_type(Library::Option, vec![value])
}

pub fn result(value: Type, error: Type) -> Type {
    library_type(Library::Result, vec![value, error])
}

pub fn dict(key: Type, value: Type) -> Type {
    library_type(Library::Dict, vec![key, value])
}

/// Every library binding but `@equatable` and `@iter`.
pub fn library() -> LibraryTypes {
    Library::ALL
        .into_iter()
        .filter(|library| !matches!(library, Library::Equatable | Library::Iter))
        .fold(LibraryTypes::new(), |bindings, library| {
            bindings.with(library, id(path(library)))
        })
}

fn variant(library: Library, parameters: &[&str], cases: Vec<Case>) -> TypeDefinition {
    TypeDefinition::new(
        id(path(library)),
        parameters.iter().map(|name| (*name).to_owned()).collect(),
        TypeBody::Variant(cases),
        None,
        Visibility::Public,
    )
    .unwrap()
}

fn plain(names: &[&str]) -> Vec<Case> {
    names
        .iter()
        .map(|name| Case::new(*name, Vec::new()))
        .collect()
}

fn wrapper(
    library: Library,
    parameters: &[&str],
    representation: Type,
) -> TypeDefinition {
    TypeDefinition::new(
        id(path(library)),
        parameters.iter().map(|name| (*name).to_owned()).collect(),
        TypeBody::Wrapper(representation),
        None,
        Visibility::Private,
    )
    .unwrap()
}

/// The definitions of the library types.
pub fn library_definitions() -> Vec<TypeDefinition> {
    vec![
        variant(Library::Bool, &[], plain(&["false", "true"])),
        variant(
            Library::Option,
            &["T"],
            vec![
                Case::new("none", Vec::new()),
                Case::new("some", vec![("value".to_owned(), Type::Binder(0))]),
            ],
        ),
        variant(
            Library::Result,
            &["T", "E"],
            vec![
                Case::new("ok", vec![("value".to_owned(), Type::Binder(0))]),
                Case::new("err", vec![("error".to_owned(), Type::Binder(1))]),
            ],
        ),
        variant(Library::Ordering, &[], plain(&["less", "equal", "greater"])),
        variant(
            Library::ArithmeticError,
            &[],
            plain(&["overflow", "divisionByZero", "invalidShift"]),
        ),
        variant(
            Library::ConversionError,
            &[],
            plain(&["outOfRange", "invalidFormat", "unrepresentable"]),
        ),
        wrapper(Library::Str, &[], Type::list(Type::Char)),
        wrapper(Library::Bytes, &[], Type::list(Type::U8)),
        wrapper(
            Library::Dict,
            &["K", "V"],
            Type::list(Type::Tuple(vec![Type::Binder(0), Type::Binder(1)])),
        ),
    ]
}

/// `Ordered`, with its member `compare: Fn({Self Self} Ordering)`.
pub fn ordered_interface() -> InterfaceDefinition {
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
    InterfaceDefinition::new(
        id(path(Library::Ordered)),
        Vec::new(),
        Vec::new(),
        vec![InterfaceMember {
            name: "compare".to_owned(),
            signature: compare,
            default: None,
        }],
    )
    .unwrap()
}

/// A plain signature with no binders.
pub fn signature(parameters: Vec<Type>, result: Type) -> SignatureRecord {
    SignatureRecord::new(Vec::new(), Type::function(FnType::new(parameters, result)))
        .unwrap()
}

/// A function with an expression body.
pub fn function(
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

pub fn global(name: &str, value_type: Type, initializer: Node) -> CheckedGlobal {
    CheckedGlobal {
        name: name.to_owned(),
        value_type,
        initializer,
        slot_count: 0,
        origin: origin(),
    }
}

/// The parts every set starts from: the library types, `Ordered`, and its
/// implementation for `Str` by the row `str.compare`, which is function 0.
pub fn base_parts() -> ModuleSetParts {
    let compare = CheckedFunction::external(
        "compare",
        signature(vec![str_type(), str_type()], ordering_type()),
        CompilerIntrinsic::StrCompare,
        Vec::new(),
        origin(),
    );
    ModuleSetParts {
        library: library(),
        types: library_definitions(),
        interfaces: vec![ordered_interface()],
        functions: vec![compare],
        implementations: vec![ordered_for(str_type(), 0)],
        ..ModuleSetParts::default()
    }
}

/// The number of functions in [`base_parts`]; a test's own functions come
/// after them.
pub const BASE: usize = 1;

/// `Ordered` for `receiver`, whose `compare` is `function`.
pub fn ordered_for(receiver: Type, function: usize) -> Implementation {
    Implementation::new(
        ImplementationRecord::new(
            Vec::new(),
            InterfaceKey::new(id(path(Library::Ordered)), Vec::new()),
            receiver,
            NamedTypes::empty(),
        )
        .unwrap(),
        vec![("compare".to_owned(), function)],
    )
    .unwrap()
}

/// A set of the base parts with `functions` after them, `globals`, and the
/// extra declarations.
pub fn set_with(
    functions: Vec<CheckedFunction>,
    globals: Vec<CheckedGlobal>,
    types: Vec<TypeDefinition>,
) -> Arc<CheckedModuleSet> {
    let mut parts = base_parts();
    parts.functions.extend(functions);
    parts.globals = globals;
    parts.types.extend(types);
    CheckedModuleSet::try_new(parts).unwrap()
}

pub fn library_set() -> Arc<CheckedModuleSet> {
    set_with(Vec::new(), Vec::new(), Vec::new())
}

/// The program of `set` that starts in `entry`.
pub fn program(set: Arc<CheckedModuleSet>, entry: usize) -> CheckedProgram {
    CheckedProgram::for_entry(set, entry).unwrap()
}

pub const LIMITS: Limits = Limits {
    reductions: 50_000_000,
    memory: MemoryBudget::DEFAULT,
};

/// Evaluates `node` with no inputs and generous limits.
pub fn eval(set: &CheckedModuleSet, node: &Node) -> Evaluation {
    evaluate(set, &node.expr, &[], LIMITS)
}

// -- expression builders ------------------------------------------------------

pub fn lit(value_type: Type, value: Value) -> Node {
    node(Expr::Literal(Constant::new(value_type, value).unwrap()))
}

pub fn int(value: i32) -> Node {
    lit(Type::I32, Value::I32(value))
}

pub fn u64_lit(value: u64) -> Node {
    lit(Type::U64, Value::U64(value))
}

pub fn text(value: &str) -> Node {
    lit(str_type(), Value::Str(value.to_owned()))
}

pub fn boolean(value: bool) -> Node {
    lit(bool_type(), Value::bool(value))
}

pub fn void() -> Node {
    lit(Type::Void, Value::Void)
}

pub fn var(slot: usize, value_type: Type) -> Node {
    node(Expr::Variable { slot, value_type })
}

pub fn captured(slot: usize, value_type: Type) -> Node {
    node(Expr::Captured { slot, value_type })
}

pub fn row(intrinsic: CompilerIntrinsic, arguments: Vec<Node>) -> Node {
    node(Expr::Intrinsic {
        intrinsic,
        type_arguments: Vec::new(),
        arguments,
    })
}

pub fn generic_row(
    intrinsic: CompilerIntrinsic,
    type_arguments: Vec<Type>,
    arguments: Vec<Node>,
) -> Node {
    node(Expr::Intrinsic {
        intrinsic,
        type_arguments,
        arguments,
    })
}

pub fn integer(integer: IntegerType, op: IntegerOp, arguments: Vec<Node>) -> Node {
    row(CompilerIntrinsic::Integer(integer, op), arguments)
}

pub fn positional(positional: Vec<Node>) -> Arguments {
    Arguments {
        positional,
        labelled: Vec::new(),
    }
}

pub fn direct(function: usize) -> CallTarget {
    CallTarget::Direct {
        function,
        type_arguments: Vec::new(),
    }
}

pub fn call(function: usize, arguments: Vec<Node>) -> Node {
    node(Expr::Call {
        target: direct(function),
        arguments: positional(arguments),
    })
}

pub fn tail(target: CallTarget, arguments: Vec<Node>) -> Node {
    node(Expr::TailCall {
        target,
        arguments: positional(arguments),
    })
}

pub fn call_value(callee: Node, arguments: Vec<Node>) -> Node {
    node(Expr::Call {
        target: CallTarget::Value(Box::new(callee)),
        arguments: positional(arguments),
    })
}

pub fn if_else(condition: Node, then_branch: Node, else_branch: Node) -> Node {
    node(Expr::If {
        condition: Box::new(condition),
        then_branch: Box::new(then_branch),
        else_branch: Some(Box::new(else_branch)),
    })
}

pub fn let_in(slot: usize, value: Node, body: Node) -> Node {
    node(Expr::Let {
        slot: Some(slot),
        value: Box::new(value),
        body: Box::new(body),
    })
}

pub fn construct(value_type: Type, construction: Construction) -> Node {
    node(Expr::Construct {
        value_type,
        construction,
    })
}

pub fn list_of(element: Type, items: Vec<Node>) -> Node {
    construct(Type::list(element), Construction::List(items))
}

pub fn tuple_of(types: Vec<Type>, items: Vec<Node>) -> Node {
    construct(Type::Tuple(types), Construction::Tuple(items))
}

pub fn some(value_type: Type, value: Node) -> Node {
    construct(
        option(value_type),
        Construction::Case {
            case: "some".to_owned(),
            fields: vec![("value".to_owned(), value)],
        },
    )
}

pub fn none(value_type: Type) -> Node {
    construct(
        option(value_type),
        Construction::Case {
            case: "none".to_owned(),
            fields: Vec::new(),
        },
    )
}

pub fn closure(
    value_type: Type,
    slot_count: usize,
    captures: Vec<(Type, Node)>,
    body: Node,
) -> Node {
    let (types, nodes): (Vec<Type>, Vec<Node>) = captures.into_iter().unzip();
    node(Expr::Closure {
        closure: Arc::new(ClosureBody {
            value_type,
            slot_count,
            captures: types,
            body,
        }),
        captures: nodes,
    })
}

pub fn fn_type(parameters: Vec<Type>, result: Type) -> Type {
    Type::function(FnType::new(parameters, result))
}

/// `left == right` on `U64`.
pub fn u64_equal(left: Node, right: Node) -> Node {
    integer(IntegerType::U64, IntegerOp::Equal, vec![left, right])
}

/// `left - right` on `U64`, saturating.
pub fn u64_sub(left: Node, right: Node) -> Node {
    integer(IntegerType::U64, IntegerOp::Sub, vec![left, right])
}

/// `left + right` on `U64`, saturating.
pub fn u64_add(left: Node, right: Node) -> Node {
    integer(IntegerType::U64, IntegerOp::Add, vec![left, right])
}

// -- neutral values -----------------------------------------------------------

pub fn variant_value(case: &str, fields: Vec<(&str, Value)>) -> Value {
    Value::Variant {
        case: case.to_owned(),
        fields: fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    }
}

pub fn some_value(value: Value) -> Value {
    variant_value("some", vec![("value", value)])
}

pub fn none_value() -> Value {
    variant_value("none", Vec::new())
}

pub fn ok_value(value: Value) -> Value {
    variant_value("ok", vec![("value", value)])
}

pub fn err_value(case: &str) -> Value {
    variant_value("err", vec![("error", variant_value(case, Vec::new()))])
}

pub fn text_value(text: &str) -> Value {
    Value::Str(text.to_owned())
}
