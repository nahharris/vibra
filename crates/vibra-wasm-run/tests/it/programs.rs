//! Hand-built checked programs for the tests that emit and run a module.
//!
//! The library declarations are the ones the interpreter's tests build, so
//! that a program runs the same way in both executors. Every expected value
//! is written by hand from `docs/spec/06-runtime.md`.

use std::sync::Arc;

use vibra_diagnostics::ByteSpan;
use vibra_interp::MemoryBudget;
use vibra_ir::external::{CompilerIntrinsic, IntegerOp, IntegerType};
use vibra_ir::{
    Arguments, CallTarget, Case, CheckedFunction, CheckedGlobal, CheckedModuleSet,
    CheckedProgram, Constant, Construction, Expr, FnType, FunctionBody, Library,
    LibraryTypes, MatchArm, ModuleSetParts, Node, Pattern, SignatureRecord,
    SourceOrigin, Type, TypeBody, TypeDefinition, TypeId, Value, Visibility,
};
use vibra_wasm_run::{MemoryLimit, Runner};

/// The chapter's placeholder identity.
const SEED: &str =
    "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad";

/// The memory limit both executors run under.
pub(crate) const LIMIT: usize = 64 * 1024 * 1024;

fn id(path: &str) -> TypeId {
    TypeId::new(path, SEED).unwrap()
}

/// An origin a test can recognize: `test.vibra` at `start..end`.
pub(crate) fn origin_at(start: usize, end: usize) -> SourceOrigin {
    SourceOrigin::new("test.vibra", ByteSpan::new(start, end))
}

pub(crate) fn origin() -> SourceOrigin {
    origin_at(0, 0)
}

pub(crate) fn node(expr: Expr) -> Node {
    Node::new(expr, origin())
}

fn path(library: Library) -> &'static str {
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

fn library_type(library: Library, arguments: Vec<Type>) -> Type {
    Type::Declared(id(path(library)), arguments)
}

pub(crate) fn option(value: Type) -> Type {
    library_type(Library::Option, vec![value])
}

/// The library bindings of the declared library types.
fn library() -> LibraryTypes {
    [
        Library::Bool,
        Library::Str,
        Library::Bytes,
        Library::Option,
        Library::Result,
        Library::Dict,
        Library::Ordering,
        Library::ArithmeticError,
        Library::ConversionError,
    ]
    .into_iter()
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

fn library_definitions() -> Vec<TypeDefinition> {
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

/// A plain signature with no binders.
pub(crate) fn signature(parameters: Vec<Type>, result: Type) -> SignatureRecord {
    SignatureRecord::new(Vec::new(), Type::function(FnType::new(parameters, result)))
        .unwrap()
}

/// A function with an expression body.
pub(crate) fn function(
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

/// The set of the library declarations and `functions`.
pub(crate) fn set_of(functions: Vec<CheckedFunction>) -> Arc<CheckedModuleSet> {
    set_with_globals(functions, Vec::new())
}

/// The set of the library declarations, `functions`, and `globals`.
pub(crate) fn set_with_globals(
    functions: Vec<CheckedFunction>,
    globals: Vec<CheckedGlobal>,
) -> Arc<CheckedModuleSet> {
    CheckedModuleSet::try_new(ModuleSetParts {
        library: library(),
        types: library_definitions(),
        functions,
        globals,
        ..ModuleSetParts::default()
    })
    .unwrap()
}

/// The program of `set` that starts in `entry`.
pub(crate) fn program(set: Arc<CheckedModuleSet>, entry: usize) -> CheckedProgram {
    CheckedProgram::for_entry(set, entry).unwrap()
}

/// The program whose one function, `main`, returns `body` of `result`.
pub(crate) fn main_returning(
    result: Type,
    body: Node,
    slot_count: usize,
) -> CheckedProgram {
    program(
        set_of(vec![function(
            "main",
            signature(Vec::new(), result),
            body,
            slot_count,
        )]),
        0,
    )
}

pub(crate) fn lit(value_type: Type, value: Value) -> Node {
    node(Expr::Literal(Constant::new(value_type, value).unwrap()))
}

pub(crate) fn int(value: i32) -> Node {
    lit(Type::I32, Value::I32(value))
}

pub(crate) fn u64_lit(value: u64) -> Node {
    lit(Type::U64, Value::U64(value))
}

pub(crate) fn var(slot: usize, value_type: Type) -> Node {
    node(Expr::Variable { slot, value_type })
}

pub(crate) fn row(intrinsic: CompilerIntrinsic, arguments: Vec<Node>) -> Node {
    generic_row(intrinsic, Vec::new(), arguments)
}

pub(crate) fn generic_row(
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

pub(crate) fn integer(
    integer: IntegerType,
    op: IntegerOp,
    arguments: Vec<Node>,
) -> Node {
    row(CompilerIntrinsic::Integer(integer, op), arguments)
}

pub(crate) fn call(function: usize, arguments: Vec<Node>) -> Node {
    node(Expr::Call {
        target: direct(function),
        arguments: Arguments {
            positional: arguments,
            labelled: Vec::new(),
        },
    })
}

pub(crate) fn tail_call(function: usize, arguments: Vec<Node>) -> Node {
    node(Expr::TailCall {
        target: direct(function),
        arguments: Arguments {
            positional: arguments,
            labelled: Vec::new(),
        },
    })
}

fn direct(function: usize) -> CallTarget {
    CallTarget::Direct {
        function,
        type_arguments: Vec::new(),
    }
}

pub(crate) fn if_else(condition: Node, then_branch: Node, else_branch: Node) -> Node {
    node(Expr::If {
        condition: Box::new(condition),
        then_branch: Box::new(then_branch),
        else_branch: Some(Box::new(else_branch)),
    })
}

pub(crate) fn some(value_type: Type, value: Node) -> Node {
    node(Expr::Construct {
        value_type: option(value_type),
        construction: Construction::Case {
            case: "some".to_owned(),
            fields: vec![("value".to_owned(), value)],
        },
    })
}

pub(crate) fn none(value_type: Type) -> Node {
    node(Expr::Construct {
        value_type: option(value_type),
        construction: Construction::Case {
            case: "none".to_owned(),
            fields: Vec::new(),
        },
    })
}

/// `match subject { arms }` of `value_type`.
pub(crate) fn matching(
    subject: Node,
    arms: Vec<(Pattern, Node)>,
    value_type: Type,
) -> Node {
    node(Expr::Match {
        subject: Box::new(subject),
        arms: arms
            .into_iter()
            .map(|(pattern, body)| MatchArm { pattern, body })
            .collect(),
        value_type,
    })
}

/// A runner under [`LIMIT`].
pub(crate) fn runner() -> Runner {
    Runner::new(MemoryLimit::new(LIMIT)).unwrap()
}

/// The interpreter's budget under [`LIMIT`].
pub(crate) fn budget() -> MemoryBudget {
    MemoryBudget::new(u64::try_from(LIMIT).unwrap())
}
