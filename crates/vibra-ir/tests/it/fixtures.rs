//! The hand-built checked IR the host tests share: the chapter's placeholder
//! identity, the library declarations, the example declarations of the
//! value encoding, plain builders, and a thread with a small stack.
//!
//! The tests of `vibra-ir` declare this module; those of `vibra-interp` and
//! `vibra-conformance` include it with `#[path]` and use some of it.

use vibra_diagnostics::ByteSpan;
use vibra_ir::{
    Arguments, CallTarget, Case, CheckedFunction, CheckedGlobal, Constant, Expr,
    FnType, FunctionBody, Library, LibraryTypes, NamedTypes, Node, SignatureRecord,
    SourceOrigin, Type, TypeBody, TypeDefinition, TypeId, Value, Visibility,
};

/// The chapter's placeholder identity.
pub(crate) const SEED: &str =
    "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad";

pub(crate) fn id(path: &str) -> TypeId {
    TypeId::new(path, SEED).unwrap()
}

pub(crate) fn origin() -> SourceOrigin {
    SourceOrigin::new("test.vibra", ByteSpan::new(0, 0))
}

pub(crate) fn node(expr: Expr) -> Node {
    Node::new(expr, origin())
}

/// The canonical path of each library declaration in the fixtures.
pub(crate) fn path(library: Library) -> &'static str {
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
        Library::Encodable => "std.encodable.Encodable",
        Library::Ordering => "std.ordering.Ordering",
        Library::ArithmeticError => "std.number.ArithmeticError",
        Library::ConversionError => "std.number.ConversionError",
    }
}

/// `{kind: @declared …}` of a library type in the fixtures, with its
/// arguments' encodings.
pub(crate) fn declared(library: Library, arguments: &str) -> String {
    format!(
        "{{kind: @declared path: @{} seed: \"{SEED}\" arguments: [{arguments}]}}",
        path(library)
    )
}

/// `{kind: @declared …}` of the declaration at `path`, with no arguments.
pub(crate) fn example(path: &str) -> String {
    format!("{{kind: @declared path: @{path} seed: \"{SEED}\" arguments: []}}")
}

pub(crate) fn library_type(library: Library, arguments: Vec<Type>) -> Type {
    Type::Declared(id(path(library)), arguments)
}

pub(crate) fn bool_type() -> Type {
    library_type(Library::Bool, Vec::new())
}

pub(crate) fn str_type() -> Type {
    library_type(Library::Str, Vec::new())
}

pub(crate) fn option(value: Type) -> Type {
    library_type(Library::Option, vec![value])
}

/// `{a: T …}` in the given order.
pub(crate) fn named(entries: Vec<(&str, Type)>) -> NamedTypes {
    NamedTypes::new(entries.into_iter().map(|(name, ty)| (name.to_owned(), ty)))
        .unwrap()
}

/// The library bindings of every role but `excluded`, each to the
/// declaration at its path.
pub(crate) fn library_without(excluded: &[Library]) -> LibraryTypes {
    Library::ALL
        .into_iter()
        .filter(|library| !excluded.contains(library))
        .fold(LibraryTypes::new(), |bindings, library| {
            bindings.with(library, id(path(library)))
        })
}

fn variant(library: Library, parameters: &[&str], cases: Vec<Case>) -> TypeDefinition {
    TypeDefinition::new(
        id(path(library)),
        parameters
            .iter()
            .map(|name| vibra_ir::Binder::new(*name, None).unwrap())
            .collect(),
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
        parameters
            .iter()
            .map(|name| vibra_ir::Binder::new(*name, None).unwrap())
            .collect(),
        TypeBody::Wrapper(representation),
        None,
        Visibility::Private,
    )
    .unwrap()
}

/// The definitions of the library types in the fixtures.
pub(crate) fn library_definitions() -> Vec<TypeDefinition> {
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

/// The runtime chapter's example record `example.geometry.Point`.
pub(crate) fn point() -> TypeDefinition {
    TypeDefinition::new(
        id("example.geometry.Point"),
        Vec::new(),
        TypeBody::Record(vec![
            ("y".to_owned(), Type::I32),
            ("x".to_owned(), Type::I32),
        ]),
        None,
        Visibility::Public,
    )
    .unwrap()
}

/// The runtime chapter's example tuple `example.geometry.Pair`.
pub(crate) fn pair() -> TypeDefinition {
    TypeDefinition::new(
        id("example.geometry.Pair"),
        Vec::new(),
        TypeBody::Tuple(vec![Type::I32, Type::I32]),
        None,
        Visibility::Public,
    )
    .unwrap()
}

/// The runtime chapter's example wrapper `example.units.Meters`.
pub(crate) fn meters() -> TypeDefinition {
    TypeDefinition::new(
        id("example.units.Meters"),
        Vec::new(),
        TypeBody::Wrapper(Type::F64),
        None,
        Visibility::Public,
    )
    .unwrap()
}

pub(crate) fn fn_type(parameters: Vec<Type>, result: Type) -> Type {
    Type::function(FnType::new(parameters, result))
}

/// A plain signature with no binders.
pub(crate) fn signature(parameters: Vec<Type>, result: Type) -> SignatureRecord {
    SignatureRecord::new(Vec::new(), fn_type(parameters, result)).unwrap()
}

/// A pure function with an expression body.
pub(crate) fn function(
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

pub(crate) fn global(name: &str, value_type: Type, initializer: Node) -> CheckedGlobal {
    CheckedGlobal {
        name: name.to_owned(),
        value_type,
        initializer,
        slot_count: 0,
        origin: origin(),
    }
}

pub(crate) fn int(value: i32) -> Node {
    node(Expr::Literal(
        Constant::new(Type::I32, Value::I32(value)).unwrap(),
    ))
}

pub(crate) fn void() -> Node {
    node(Expr::Literal(
        Constant::new(Type::Void, Value::Void).unwrap(),
    ))
}

pub(crate) fn variable(slot: usize, value_type: Type) -> Node {
    node(Expr::Variable { slot, value_type })
}

pub(crate) fn positional(positional: Vec<Node>) -> Arguments {
    Arguments {
        positional,
        labelled: Vec::new(),
    }
}

/// A direct call of `function` with no type arguments.
pub(crate) fn call(function: usize, arguments: Vec<Node>) -> Node {
    node(Expr::Call {
        target: CallTarget::Direct {
            function,
            type_arguments: Vec::new(),
        },
        arguments: positional(arguments),
    })
}

/// The stack of the thread [`on_small_stack`] starts: 256 KiB.
pub(crate) const STACK: usize = 256 * 1024;

/// Runs `work` on a thread whose stack is [`STACK`]: a stack overflow there
/// aborts the test binary.
pub(crate) fn on_small_stack<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> T {
    std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(work)
        .expect("a thread")
        .join()
        .expect("the thread finished")
}
