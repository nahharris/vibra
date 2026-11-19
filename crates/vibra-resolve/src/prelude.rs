//! The prelude names and the module list of the unit `@std`.
//!
//! `docs/spec/02-type-system.md`, "The prelude": the compiler-owned types,
//! the types and interfaces that play a role, `From`, `TryFrom`, `Any`,
//! `Self`, `Ordering`, and the values `true` and `false` are the only
//! declarations a program names without an import. The set is closed.
//! `docs/spec/04-programs-and-packages.md`, "The standard library", lists
//! the modules of `@std`.
//!
//! The embedded library is loaded in a later step. Until then these two
//! tables stand for it: a prelude name resolves to a prelude entity with no
//! declaration identity, and a path into `@std` resolves against the module
//! list only.

/// What a prelude name denotes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreludeKind {
    /// A type.
    Type,
    /// An interface.
    Interface,
    /// A value: `true` or `false`.
    Value,
}

/// The prelude: each name and what it denotes. `Self` is not in the table:
/// it denotes the receiver type and is visible only inside a `type`, an
/// `interface`, and an `impl` (`docs/spec/02-type-system.md`, "Dispatch").
const PRELUDE: &[(&str, PreludeKind)] = &[
    ("Void", PreludeKind::Type),
    ("Never", PreludeKind::Type),
    ("Char", PreludeKind::Type),
    ("Atom", PreludeKind::Type),
    ("I8", PreludeKind::Type),
    ("I16", PreludeKind::Type),
    ("I32", PreludeKind::Type),
    ("I64", PreludeKind::Type),
    ("U8", PreludeKind::Type),
    ("U16", PreludeKind::Type),
    ("U32", PreludeKind::Type),
    ("U64", PreludeKind::Type),
    ("F32", PreludeKind::Type),
    ("F64", PreludeKind::Type),
    ("List", PreludeKind::Type),
    ("Bool", PreludeKind::Type),
    ("Str", PreludeKind::Type),
    ("Bytes", PreludeKind::Type),
    ("Option", PreludeKind::Type),
    ("Result", PreludeKind::Type),
    ("Dict", PreludeKind::Type),
    ("Ordering", PreludeKind::Type),
    ("Iter", PreludeKind::Interface),
    ("Equatable", PreludeKind::Interface),
    ("Ordered", PreludeKind::Interface),
    ("From", PreludeKind::Interface),
    ("TryFrom", PreludeKind::Interface),
    ("Any", PreludeKind::Interface),
    ("true", PreludeKind::Value),
    ("false", PreludeKind::Value),
];

/// The receiver type's name.
pub const SELF: &str = "Self";

/// The name of the standard library's unit.
pub const STD_UNIT: &str = "@std";

/// The modules of `@std`, by path.
const STD_MODULES: &[&str] = &[
    "@std.assert",
    "@std.bool",
    "@std.builtin",
    "@std.bytes",
    "@std.core",
    "@std.env",
    "@std.fs",
    "@std.io",
    "@std.iter",
    "@std.option",
    "@std.policy",
    "@std.process",
    "@std.random",
    "@std.result",
    "@std.str",
    "@std.test",
    "@std.time",
];

/// What the prelude name `name` denotes, or `None` when `name` is not one.
/// `Self` is a prelude name with no entry here.
#[must_use]
pub fn prelude(name: &str) -> Option<PreludeKind> {
    PRELUDE
        .iter()
        .find(|(entry, _)| *entry == name)
        .map(|(_, kind)| *kind)
}

/// Whether `name` is a prelude name, `Self` included.
#[must_use]
pub fn is_prelude_name(name: &str) -> bool {
    name == SELF || prelude(name).is_some()
}

/// The `@std` module at `path`, when the library has one.
#[must_use]
pub fn std_module(path: &str) -> Option<&'static str> {
    STD_MODULES.iter().copied().find(|module| *module == path)
}
