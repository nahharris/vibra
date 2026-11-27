//! The `vibra-ir` host tests, one test target.
//!
//! The expected bytes are written by hand from `docs/spec/06-runtime.md`;
//! none is produced by this crate and copied back.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

mod effects;
mod encode;
mod instantiation;
mod registry;
mod shape;
mod soundness;

use vibra_diagnostics::ByteSpan;
use vibra_ir::{
    Case, Expr, FnType, InterfaceDefinition, InterfaceMember, Library, LibraryTypes,
    ModuleSetParts, NamedTypes, Node, SignatureRecord, SourceOrigin, Type, TypeBody,
    TypeDefinition, TypeId, TypeTable, Visibility,
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
        Library::Encodable => "std.encodable.Encodable",
        Library::Ordering => "std.ordering.Ordering",
        Library::ArithmeticError => "std.number.ArithmeticError",
        Library::ConversionError => "std.number.ConversionError",
    }
}

/// `{kind: @declared …}` of a library type in the fixtures, with its
/// arguments' encodings.
pub fn declared(library: Library, arguments: &str) -> String {
    format!(
        "{{kind: @declared path: @{} seed: \"{SEED}\" arguments: [{arguments}]}}",
        path(library)
    )
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

pub fn option(value: Type) -> Type {
    library_type(Library::Option, vec![value])
}

/// The library bindings of the fixtures: the ten roles but `@equatable`,
/// `@ordered`, and `@encodable`, and the three core types.
pub fn library() -> LibraryTypes {
    Library::ALL
        .into_iter()
        .filter(|library| {
            !matches!(
                library,
                Library::Equatable | Library::Ordered | Library::Encodable
            )
        })
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

/// `Iter`, with its associated type `Item` and its member
/// `next: Fn({Self} Option({Item Self}))`.
pub fn iter_interface() -> InterfaceDefinition {
    let next = SignatureRecord::interface_member(
        Vec::new(),
        Some(0),
        Type::function(FnType::new(
            vec![Type::SelfType],
            option(Type::Tuple(vec![
                Type::Associated("Item".to_owned()),
                Type::SelfType,
            ])),
        )),
        vec!["Item".to_owned()],
    )
    .unwrap();
    InterfaceDefinition::new(
        id(path(Library::Iter)),
        Vec::new(),
        vec!["Item".to_owned()],
        vec![InterfaceMember {
            name: "next".to_owned(),
            signature: next,
            default: None,
        }],
    )
    .unwrap()
}

/// The table the value tests encode with: the library types and `extra`.
pub fn table(extra: Vec<TypeDefinition>) -> TypeTable {
    TypeTable::new(library(), library_definitions().into_iter().chain(extra))
}

/// The parts of a module set holding the library declarations only.
pub fn library_parts() -> ModuleSetParts {
    ModuleSetParts {
        library: library(),
        types: library_definitions(),
        interfaces: vec![iter_interface()],
        ..ModuleSetParts::default()
    }
}

/// `{a: T …}` in the given order.
pub fn named(entries: Vec<(&str, Type)>) -> NamedTypes {
    NamedTypes::new(entries.into_iter().map(|(name, ty)| (name.to_owned(), ty)))
        .unwrap()
}

/// The runtime chapter's "Traps": exactly two, each with its code.
#[test]
fn there_are_two_traps_with_their_atoms() {
    let atoms = vibra_ir::Trap::ALL.map(vibra_ir::Trap::atom);
    assert_eq!(
        atoms,
        [
            "@runtime.invalidCheckedProgram",
            "@runtime.invalidHostValue"
        ]
    );
}

/// `text` without layout: each line trimmed, the lines joined by one
/// space, and no space just inside a bracket.
pub fn unlayout(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .replace("{ ", "{")
        .replace("[ ", "[")
        .replace(" }", "}")
        .replace(" ]", "]")
}
