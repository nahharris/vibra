//! Observable values and their canonical encoding.
//!
//! `docs/spec/06-runtime.md` defines one canonical VIBON encoding for every
//! value except a function. An [`ObservedValue`] is such a value: the result of
//! a run, or an operand of a failed assertion. Function values have no
//! encoding and never become one.

use crate::{Type, TypeId, Value, canonical_type};

/// A function-free value with a canonical encoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObservedValue {
    /// A primitive value.
    Primitive(Value),
    /// A record; fields in declaration order when declared, canonical order
    /// when anonymous.
    Record {
        /// The declared type, or `None` for an anonymous record.
        type_id: Option<TypeId>,
        /// Field values.
        fields: Vec<(String, ObservedValue)>,
    },
    /// An enum value.
    Enum {
        /// The declared type, or `None` for an anonymous enum.
        type_id: Option<TypeId>,
        /// The selected variant.
        variant: String,
        /// The payload; `None` for a `void` payload slot.
        payload: Option<Box<ObservedValue>>,
    },
    /// A wrapper-type value.
    Wrapper {
        /// The declared wrapper type.
        type_id: TypeId,
        /// The representation value.
        value: Box<ObservedValue>,
    },
    /// A tuple value.
    Tuple {
        /// The declared type, or `None` for an anonymous tuple.
        type_id: Option<TypeId>,
        /// Component values in order.
        values: Vec<ObservedValue>,
    },
    /// An array value.
    Array(Vec<ObservedValue>),
    /// A dict value; entries in canonical key order.
    Dict(Vec<(ObservedValue, ObservedValue)>),
    /// A union value: its member type and that member's value.
    Union {
        /// The declared union, or `None` for an anonymous union.
        type_id: Option<TypeId>,
        /// The member type the value holds.
        member: Box<Type>,
        /// The member value.
        value: Box<ObservedValue>,
    },
}

impl ObservedValue {
    /// The canonical value encoding.
    #[must_use]
    pub fn canonical_vibon(&self) -> String {
        match self {
            Self::Primitive(value) => value.canonical_vibon(),
            Self::Record { type_id, fields } => format!(
                "(record kind: @record{} fields: (record{}))",
                type_field(type_id.as_ref()),
                fields
                    .iter()
                    .map(|(name, value)| format!(
                        " {name}: {}",
                        value.canonical_vibon()
                    ))
                    .collect::<String>()
            ),
            Self::Enum {
                type_id,
                variant,
                payload,
            } => format!(
                "(record kind: @enum{} variant: @{variant}{})",
                type_field(type_id.as_ref()),
                payload
                    .as_deref()
                    .map(|payload| format!(" payload: {}", payload.canonical_vibon()))
                    .unwrap_or_default()
            ),
            Self::Wrapper { type_id, value } => format!(
                "(record kind: @wrapper type: @{} value: {})",
                type_id.path(),
                value.canonical_vibon()
            ),
            Self::Tuple { type_id, values } => format!(
                "(record kind: @tuple{} values: (array{}))",
                type_field(type_id.as_ref()),
                encode_all(values)
            ),
            Self::Array(values) => {
                format!(
                    "(record kind: @array values: (array{}))",
                    encode_all(values)
                )
            }
            Self::Union {
                type_id,
                member,
                value,
            } => format!(
                "(record kind: @union{} member: {} value: {})",
                type_field(type_id.as_ref()),
                canonical_type(member),
                value.canonical_vibon()
            ),
            Self::Dict(entries) => format!(
                "(record kind: @dict entries: (array{}))",
                entries
                    .iter()
                    .map(|(key, value)| format!(
                        " (tuple {} {})",
                        key.canonical_vibon(),
                        value.canonical_vibon()
                    ))
                    .collect::<String>()
            ),
        }
    }

    /// A typed result observation, `(record type: T value: v)`.
    #[must_use]
    pub fn canonical_observation(&self, value_type: &Type) -> String {
        format!(
            "(record type: {} value: {})\n",
            canonical_type(value_type),
            self.canonical_vibon()
        )
    }

    /// The primitive value, when this is one.
    #[must_use]
    pub const fn as_primitive(&self) -> Option<&Value> {
        match self {
            Self::Primitive(value) => Some(value),
            Self::Record { .. }
            | Self::Enum { .. }
            | Self::Wrapper { .. }
            | Self::Tuple { .. }
            | Self::Array(_)
            | Self::Dict(_)
            | Self::Union { .. } => None,
        }
    }
}

impl PartialEq<Value> for ObservedValue {
    fn eq(&self, other: &Value) -> bool {
        self.as_primitive() == Some(other)
    }
}

fn type_field(type_id: Option<&TypeId>) -> String {
    type_id
        .map(|type_id| format!(" type: @{}", type_id.path()))
        .unwrap_or_default()
}

fn encode_all(values: &[ObservedValue]) -> String {
    values
        .iter()
        .map(|value| format!(" {}", value.canonical_vibon()))
        .collect()
}
