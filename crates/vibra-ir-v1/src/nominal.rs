//! Declared type identities and definitions.
//!
//! `docs/spec/02-type-system.md` makes every `deftype` a new identity. The
//! checked program carries one [`TypeDefinition`] per declared type it uses, so
//! the interpreter and renderers never consult source or the resolver.

use crate::Type;

/// The identity of one declared type.
///
/// `id` is the resolver's canonical declaration identity and decides equality;
/// `path` is the canonical atom path, without its leading `@`, that the
/// canonical value encoding renders.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId {
    id: String,
    path: String,
}

impl TypeId {
    /// The canonical declaration identity.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The canonical atom path without its leading `@`.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// The body of a declared type, in declaration order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeBody {
    /// Closed, named fields.
    Record(Vec<(String, Type)>),
    /// Variants, each with one payload slot; `void` marks a nullary variant.
    Enum(Vec<(String, Type)>),
    /// A distinct identity over one representation type.
    Wrapper(Type),
    /// Positional components.
    Tuple(Vec<Type>),
    /// Member types, in declaration order, which fixes discriminants.
    Union(Vec<Type>),
}

impl TypeBody {
    /// Every slot of the body with its name: a field, a variant, a tuple
    /// component spelled by its index, or the unnamed representation.
    #[must_use]
    pub fn slots(&self) -> Vec<(String, Type)> {
        match self {
            Self::Record(members) | Self::Enum(members) => members.clone(),
            Self::Wrapper(representation) => {
                vec![(String::new(), representation.clone())]
            }
            Self::Tuple(components) | Self::Union(components) => components
                .iter()
                .enumerate()
                .map(|(index, value)| (index.to_string(), value.clone()))
                .collect(),
        }
    }
}

/// One declared type of a checked program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDefinition {
    id: TypeId,
    parameters: Vec<String>,
    body: TypeBody,
}

impl TypeDefinition {
    /// The generic parameter names in `where:` order.
    #[must_use]
    pub fn parameters(&self) -> &[String] {
        &self.parameters
    }

    /// The body with its generic parameters replaced by `arguments`, or `None`
    /// when the argument count differs from the parameter list.
    #[must_use]
    pub fn instantiate(&self, arguments: &[Type]) -> Option<TypeBody> {
        if arguments.len() != self.parameters.len() {
            return None;
        }
        let map: std::collections::BTreeMap<String, Type> = self
            .parameters
            .iter()
            .cloned()
            .zip(arguments.iter().cloned())
            .collect();
        let substitute = |members: &[(String, Type)]| {
            members
                .iter()
                .map(|(name, value)| (name.clone(), value.substitute(&map)))
                .collect()
        };
        Some(match &self.body {
            TypeBody::Record(fields) => TypeBody::Record(substitute(fields)),
            TypeBody::Enum(variants) => TypeBody::Enum(substitute(variants)),
            TypeBody::Wrapper(representation) => {
                TypeBody::Wrapper(representation.substitute(&map))
            }
            TypeBody::Tuple(components) => TypeBody::Tuple(
                components
                    .iter()
                    .map(|value| value.substitute(&map))
                    .collect(),
            ),
            TypeBody::Union(members) => TypeBody::Union(
                members.iter().map(|value| value.substitute(&map)).collect(),
            ),
        })
    }

    /// The declared identity.
    #[must_use]
    pub const fn id(&self) -> &TypeId {
        &self.id
    }

    /// The declared body.
    #[must_use]
    pub const fn body(&self) -> &TypeBody {
        &self.body
    }
}
