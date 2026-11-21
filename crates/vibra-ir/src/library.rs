//! The standard-library declarations the IR depends on: the types and
//! interfaces that play a language role (`docs/spec/02-type-system.md`,
//! "Language core and standard library"), and the three core types the
//! compiler intrinsic registry names (`docs/spec/06-runtime.md`, "Compiler
//! intrinsic registry").

use std::collections::BTreeMap;

use crate::external::compiler::NarrowingType;
use crate::{IrError, Type, TypeId};

/// A standard-library declaration the IR names by what it does, never by
/// its identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Library {
    /// The variant type playing `@bool`, with the cases `false` and `true`.
    Bool,
    /// The wrapper over `List(Char)` playing `@str`.
    Str,
    /// The wrapper over `List(U8)` playing `@bytes`.
    Bytes,
    /// The variant type playing `@option`, with `none()` and `some(value:)`.
    Option,
    /// The variant type playing `@result`, with `ok(value:)` and
    /// `err(error:)`.
    Result,
    /// The type playing `@dict`.
    Dict,
    /// The interface playing `@iter`.
    Iter,
    /// The interface playing `@equatable`.
    Equatable,
    /// The interface playing `@ordered`.
    Ordered,
    /// `Ordering`, with the cases `less`, `equal`, and `greater`.
    Ordering,
    /// `ArithmeticError`, with `overflow`, `divisionByZero`, and
    /// `invalidShift`.
    ArithmeticError,
    /// `ConversionError`, with `outOfRange`, `invalidFormat`, and
    /// `unrepresentable`.
    ConversionError,
}

impl Library {
    /// Every library declaration, the nine roles first in the order of the
    /// role table.
    pub const ALL: [Self; 12] = [
        Self::Bool,
        Self::Str,
        Self::Bytes,
        Self::Option,
        Self::Result,
        Self::Dict,
        Self::Iter,
        Self::Equatable,
        Self::Ordered,
        Self::Ordering,
        Self::ArithmeticError,
        Self::ConversionError,
    ];

    /// The declaration's source name, such as `Option`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bool => "Bool",
            Self::Str => "Str",
            Self::Bytes => "Bytes",
            Self::Option => "Option",
            Self::Result => "Result",
            Self::Dict => "Dict",
            Self::Iter => "Iter",
            Self::Equatable => "Equatable",
            Self::Ordered => "Ordered",
            Self::Ordering => "Ordering",
            Self::ArithmeticError => "ArithmeticError",
            Self::ConversionError => "ConversionError",
        }
    }

    /// The role atom, such as `@option`, `None` for the three core types,
    /// which play no role.
    #[must_use]
    pub const fn role(self) -> Option<&'static str> {
        Some(match self {
            Self::Bool => "@bool",
            Self::Str => "@str",
            Self::Bytes => "@bytes",
            Self::Option => "@option",
            Self::Result => "@result",
            Self::Dict => "@dict",
            Self::Iter => "@iter",
            Self::Equatable => "@equatable",
            Self::Ordered => "@ordered",
            Self::Ordering | Self::ArithmeticError | Self::ConversionError => {
                return None;
            }
        })
    }

    /// Whether the declaration is an interface rather than a type.
    #[must_use]
    pub const fn is_interface(self) -> bool {
        matches!(self, Self::Iter | Self::Equatable | Self::Ordered)
    }

    /// The number of type parameters the declaration has.
    #[must_use]
    pub const fn arity(self) -> usize {
        match self {
            Self::Option => 1,
            Self::Result | Self::Dict => 2,
            _ => 0,
        }
    }
}

/// The identity bound to each library declaration, as the checker binds
/// them when it loads the standard library, and to each
/// [narrowing type](NarrowingType) the grant rows of the compiler intrinsic
/// registry name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryTypes {
    roles: BTreeMap<Library, TypeId>,
    narrowing: BTreeMap<NarrowingType, TypeId>,
}

impl LibraryTypes {
    /// No binding.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            roles: BTreeMap::new(),
            narrowing: BTreeMap::new(),
        }
    }

    /// The same bindings with `library` bound to `id`.
    #[must_use]
    pub fn with(mut self, library: Library, id: TypeId) -> Self {
        self.roles.insert(library, id);
        self
    }

    /// The same bindings with the narrowing type `declaration` bound to
    /// `id`.
    #[must_use]
    pub fn with_narrowing(mut self, declaration: NarrowingType, id: TypeId) -> Self {
        self.narrowing.insert(declaration, id);
        self
    }

    /// The identity bound to `library`.
    #[must_use]
    pub fn get(&self, library: Library) -> Option<&TypeId> {
        self.roles.get(&library)
    }

    /// The identity bound to the narrowing type `declaration`.
    #[must_use]
    pub fn narrowing(&self, declaration: NarrowingType) -> Option<&TypeId> {
        self.narrowing.get(&declaration)
    }

    /// The library declaration bound to `id`, if any.
    #[must_use]
    pub fn library_of(&self, id: &TypeId) -> Option<Library> {
        self.roles
            .iter()
            .find_map(|(library, bound)| (bound == id).then_some(*library))
    }

    /// Every binding of a library declaration.
    pub fn bindings(&self) -> impl Iterator<Item = (Library, &TypeId)> {
        self.roles.iter().map(|(library, id)| (*library, id))
    }

    /// Every binding of a narrowing type.
    pub fn narrowing_bindings(&self) -> impl Iterator<Item = (NarrowingType, &TypeId)> {
        self.narrowing
            .iter()
            .map(|(declaration, id)| (*declaration, id))
    }

    /// The library type `library` applied to `arguments`.
    ///
    /// # Errors
    ///
    /// Nothing is bound to `library`.
    pub fn declared(
        &self,
        library: Library,
        arguments: Vec<Type>,
    ) -> Result<Type, IrError> {
        self.get(library)
            .map(|id| Type::Declared(id.clone(), arguments))
            .ok_or(IrError::UnboundLibrary(library))
    }

    /// The arguments of `ty` when it is the library type `library`.
    #[must_use]
    pub fn arguments_of<'a>(
        &self,
        library: Library,
        ty: &'a Type,
    ) -> Option<&'a [Type]> {
        match ty {
            Type::Declared(id, arguments) if self.get(library) == Some(id) => {
                Some(arguments)
            }
            _ => None,
        }
    }
}
