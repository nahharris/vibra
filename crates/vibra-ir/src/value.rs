//! The neutral value model: the values a constant, an observation, and the
//! canonical value encoding talk about.
//!
//! A value carries no type of its own except where the runtime chapter's
//! encoding needs one that the static type does not give: the held type of
//! an interface value. An executor keeps its own representation, under the
//! runtime chapter's "Representation latitude", and converts to this model
//! only to observe a value.

use crate::encode::{EncodeError, value_document};
use crate::{Type, TypeTable};

/// A value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// An `I8`.
    I8(i8),
    /// An `I16`.
    I16(i16),
    /// An `I32`.
    I32(i32),
    /// An `I64`.
    I64(i64),
    /// A `U8`.
    U8(u8),
    /// A `U16`.
    U16(u16),
    /// A `U32`.
    U32(u32),
    /// A `U64`.
    U64(u64),
    /// An `F32`, by its exact bits.
    F32(u32),
    /// An `F64`, by its exact bits.
    F64(u64),
    /// A `Char`.
    Char(char),
    /// An atom, without its `@`.
    Atom(String),
    /// `void`.
    Void,
    /// A `Str`: the value of the type playing `@str`.
    Str(String),
    /// A `Bytes`: the value of the type playing `@bytes`.
    Bytes(Vec<u8>),
    /// A tuple, structural or declared.
    Tuple(Vec<Self>),
    /// A `List`.
    List(Vec<Self>),
    /// A `Dict`, the value of the type playing `@dict`: its entries in the
    /// `Ordered` order of its keys, one per key.
    Dict(Vec<(Self, Self)>),
    /// A record, structural or declared, by field name.
    Record(Vec<(String, Self)>),
    /// A value of a variant type: its case and the case's fields.
    Variant {
        /// The case name.
        case: String,
        /// The case's fields, by name.
        fields: Vec<(String, Self)>,
    },
    /// A value of a wrapper type: its representation.
    Wrapper(Box<Self>),
    /// An interface value: the concrete value it holds and that value's
    /// closed type.
    Interface {
        /// The held value's type.
        held_type: Type,
        /// The held value.
        held: Box<Self>,
    },
    /// A module function as a value, at its type arguments.
    Function {
        /// The function's index in its module set.
        function: usize,
        /// The type arguments it was instantiated at.
        type_arguments: Vec<Type>,
    },
    /// A closure and its captured values.
    Closure {
        /// The captured values, by environment slot.
        captures: Vec<Self>,
    },
}

impl Value {
    /// The value `false` or `true` of a type playing `@bool`, whose cases
    /// are `false` and `true` with no fields.
    #[must_use]
    pub fn bool(value: bool) -> Self {
        Self::Variant {
            case: if value { "true" } else { "false" }.to_owned(),
            fields: Vec::new(),
        }
    }

    /// The `F32` of `value`.
    #[must_use]
    pub const fn f32(value: f32) -> Self {
        Self::F32(value.to_bits())
    }

    /// The `F64` of `value`.
    #[must_use]
    pub const fn f64(value: f64) -> Self {
        Self::F64(value.to_bits())
    }

    /// The compiler-owned scalar type of a scalar value, `None` for any
    /// other value.
    #[must_use]
    pub const fn scalar_type(&self) -> Option<Type> {
        Some(match self {
            Self::I8(_) => Type::I8,
            Self::I16(_) => Type::I16,
            Self::I32(_) => Type::I32,
            Self::I64(_) => Type::I64,
            Self::U8(_) => Type::U8,
            Self::U16(_) => Type::U16,
            Self::U32(_) => Type::U32,
            Self::U64(_) => Type::U64,
            Self::F32(_) => Type::F32,
            Self::F64(_) => Type::F64,
            Self::Char(_) => Type::Char,
            Self::Atom(_) => Type::Atom,
            Self::Void => Type::Void,
            _ => return None,
        })
    }

    /// Whether a function or a closure occurs in this value, at any depth,
    /// an interface value's held value included.
    #[must_use]
    pub fn holds_callable(&self) -> bool {
        let mut pending = vec![self];
        while let Some(value) = pending.pop() {
            match value {
                Self::Function { .. } | Self::Closure { .. } => return true,
                other => pending.extend(other.children()),
            }
        }
        false
    }

    /// The values directly inside this one.
    fn children(&self) -> Vec<&Self> {
        match self {
            Self::Tuple(values)
            | Self::List(values)
            | Self::Closure { captures: values } => values.iter().collect(),
            Self::Dict(entries) => entries
                .iter()
                .flat_map(|(key, value)| [key, value])
                .collect(),
            Self::Record(fields) | Self::Variant { fields, .. } => {
                fields.iter().map(|(_, value)| value).collect()
            }
            Self::Wrapper(value) | Self::Interface { held: value, .. } => vec![value],
            _ => Vec::new(),
        }
    }
}

/// A value observed at its closed type: what the canonical value encoding
/// writes as one `@value.v2` document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedValue {
    value_type: Type,
    value: Value,
}

impl ObservedValue {
    /// The observation of `value` at `value_type`.
    #[must_use]
    pub const fn new(value_type: Type, value: Value) -> Self {
        Self { value_type, value }
    }

    /// The observed type.
    #[must_use]
    pub const fn value_type(&self) -> &Type {
        &self.value_type
    }

    /// The observed value.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// The canonical bytes of the `@value.v2` document of this observation.
    ///
    /// # Errors
    ///
    /// The value has no encoding, or it does not fit its type.
    pub fn encode(&self, types: &TypeTable) -> Result<String, EncodeError> {
        value_document(&self.value, &self.value_type, types)
    }
}
