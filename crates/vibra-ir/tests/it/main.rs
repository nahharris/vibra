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
mod fixtures;
mod instantiation;
mod registry;
mod shape;
mod soundness;

pub(crate) use fixtures::*;

use vibra_ir::{
    FnType, InterfaceDefinition, InterfaceMember, Library, LibraryTypes,
    ModuleSetParts, SignatureRecord, Type, TypeDefinition, TypeTable,
};

/// The library bindings of the fixtures: the ten roles but `@equatable`,
/// `@ordered`, and `@encodable`, and the three core types.
pub fn library() -> LibraryTypes {
    library_without(&[Library::Equatable, Library::Ordered, Library::Encodable])
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
