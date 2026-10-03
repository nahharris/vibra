//! Observable values and their canonical encoding.
//!
//! `docs/spec/06-runtime.md` defines one canonical VIBON encoding for every
//! value except a function. An [`ObservedValue`] is such a value: the result of
//! a run, or an operand of a failed assertion. Function values have no
//! encoding and never become one.
//!
//! A value may be nested arbitrarily deep, since the language bounds its depth
//! only by memory. Encoding and releasing one therefore walk an explicit
//! worklist and use bounded host stack.

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

/// One pending piece of an encoding: text that follows, or a value to expand.
enum Piece<'a> {
    Text(&'a str),
    Value(&'a ObservedValue),
}

impl ObservedValue {
    /// The canonical value encoding.
    #[must_use]
    pub fn canonical_vibon(&self) -> String {
        let mut output = String::new();
        // Pieces pop in encoding order, so a value pushes what follows its
        // own opening text in reverse.
        let mut pending = vec![Piece::Value(self)];
        while let Some(piece) = pending.pop() {
            match piece {
                Piece::Text(text) => output.push_str(text),
                Piece::Value(value) => value.open(&mut output, &mut pending),
            }
        }
        output
    }

    /// Writes this value's opening text and schedules the rest.
    fn open<'a>(&'a self, output: &mut String, pending: &mut Vec<Piece<'a>>) {
        match self {
            Self::Primitive(value) => output.push_str(&value.canonical_vibon()),
            Self::Record { type_id, fields } => {
                output.push_str("(record kind: @record");
                output.push_str(&type_field(type_id.as_ref()));
                output.push_str(" fields: (record");
                pending.push(Piece::Text("))"));
                for (name, value) in fields.iter().rev() {
                    pending.push(Piece::Value(value));
                    pending.push(Piece::Text(": "));
                    pending.push(Piece::Text(name));
                    pending.push(Piece::Text(" "));
                }
            }
            Self::Enum {
                type_id,
                variant,
                payload,
            } => {
                output.push_str("(record kind: @enum");
                output.push_str(&type_field(type_id.as_ref()));
                output.push_str(" variant: @");
                output.push_str(variant);
                pending.push(Piece::Text(")"));
                if let Some(payload) = payload.as_deref() {
                    output.push_str(" payload: ");
                    pending.push(Piece::Value(payload));
                }
            }
            Self::Wrapper { type_id, value } => {
                output.push_str("(record kind: @wrapper type: @");
                output.push_str(type_id.path());
                output.push_str(" value: ");
                pending.push(Piece::Text(")"));
                pending.push(Piece::Value(value));
            }
            Self::Tuple { type_id, values } => {
                output.push_str("(record kind: @tuple");
                output.push_str(&type_field(type_id.as_ref()));
                output.push_str(" values: (array");
                pending.push(Piece::Text("))"));
                for value in values.iter().rev() {
                    pending.push(Piece::Value(value));
                    pending.push(Piece::Text(" "));
                }
            }
            Self::Array(values) => {
                output.push_str("(record kind: @array values: (array");
                pending.push(Piece::Text("))"));
                for value in values.iter().rev() {
                    pending.push(Piece::Value(value));
                    pending.push(Piece::Text(" "));
                }
            }
            Self::Union {
                type_id,
                member,
                value,
            } => {
                output.push_str("(record kind: @union");
                output.push_str(&type_field(type_id.as_ref()));
                output.push_str(" member: ");
                output.push_str(&canonical_type(member));
                output.push_str(" value: ");
                pending.push(Piece::Text(")"));
                pending.push(Piece::Value(value));
            }
            Self::Dict(entries) => {
                output.push_str("(record kind: @dict entries: (array");
                pending.push(Piece::Text("))"));
                for (key, value) in entries.iter().rev() {
                    pending.push(Piece::Text(")"));
                    pending.push(Piece::Value(value));
                    pending.push(Piece::Text(" "));
                    pending.push(Piece::Value(key));
                    pending.push(Piece::Text(" (tuple "));
                }
            }
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

    /// Moves every compound value nested directly in this one into `out`.
    fn take_children(&mut self, out: &mut Vec<Self>) {
        fn compound(value: ObservedValue) -> Option<ObservedValue> {
            (!matches!(value, ObservedValue::Primitive(_))).then_some(value)
        }
        match self {
            Self::Primitive(_) => {}
            Self::Record { fields, .. } => out.extend(
                std::mem::take(fields)
                    .into_iter()
                    .filter_map(|(_, value)| compound(value)),
            ),
            Self::Enum { payload, .. } => {
                out.extend(payload.take().and_then(|payload| compound(*payload)));
            }
            Self::Wrapper { value, .. } | Self::Union { value, .. } => {
                let nested =
                    std::mem::replace(&mut **value, Self::Primitive(Value::Void));
                out.extend(compound(nested));
            }
            Self::Tuple { values, .. } | Self::Array(values) => {
                out.extend(std::mem::take(values).into_iter().filter_map(compound));
            }
            Self::Dict(entries) => {
                for (key, value) in std::mem::take(entries) {
                    out.extend(compound(key));
                    out.extend(compound(value));
                }
            }
        }
    }
}

impl Drop for ObservedValue {
    /// Releases a value with bounded host stack, however deeply it is nested.
    fn drop(&mut self) {
        if matches!(self, Self::Primitive(_)) {
            return;
        }
        let mut pending = Vec::new();
        self.take_children(&mut pending);
        while let Some(mut value) = pending.pop() {
            value.take_children(&mut pending);
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn leaf(value: u32) -> ObservedValue {
        ObservedValue::Primitive(Value::U32(value))
    }

    #[test]
    fn iterative_encoding_matches_the_recursive_layout() {
        let value = ObservedValue::Record {
            type_id: None,
            fields: vec![
                (
                    "items".to_owned(),
                    ObservedValue::Array(vec![leaf(1), leaf(2)]),
                ),
                (
                    "pairs".to_owned(),
                    ObservedValue::Dict(vec![(leaf(3), leaf(4))]),
                ),
                (
                    "choice".to_owned(),
                    ObservedValue::Enum {
                        type_id: None,
                        variant: "some".to_owned(),
                        payload: Some(Box::new(ObservedValue::Tuple {
                            type_id: None,
                            values: vec![leaf(5)],
                        })),
                    },
                ),
                (
                    "none".to_owned(),
                    ObservedValue::Enum {
                        type_id: None,
                        variant: "none".to_owned(),
                        payload: None,
                    },
                ),
                (
                    "member".to_owned(),
                    ObservedValue::Union {
                        type_id: None,
                        member: Box::new(Type::U32),
                        value: Box::new(leaf(6)),
                    },
                ),
            ],
        };
        assert_eq!(
            value.canonical_vibon(),
            "(record kind: @record fields: (record items: (record kind: @array values: (array 1u32 2u32)) pairs: (record kind: @dict entries: (array (tuple 3u32 4u32))) choice: (record kind: @enum variant: @some payload: (record kind: @tuple values: (array 5u32))) none: (record kind: @enum variant: @none) member: (record kind: @union member: @u32 value: 6u32)))"
        );
    }

    #[test]
    fn a_value_nested_a_million_levels_deep_encodes_and_drops_on_a_small_stack() {
        std::thread::Builder::new()
            .stack_size(128 * 1024)
            .spawn(|| {
                let mut value = leaf(0);
                for _ in 0..1_000_000 {
                    value = ObservedValue::Array(vec![value]);
                }
                let text = value.canonical_vibon();
                assert_eq!(text.matches("kind: @array").count(), 1_000_000);
                drop(value);
            })
            .expect("thread")
            .join()
            .expect("bounded stack");
    }
}
