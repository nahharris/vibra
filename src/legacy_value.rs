//! Internal compatibility value tree for the retiring map-based lowering seam.
//!
//! This is deliberately data-only: Vibra source is parsed by the typed
//! S-expression frontend, never by this module.  It replaces the incidental
//! YAML crate type previously used by the temporary adapter.

/// Preserves declaration order, matching the retired bridge's map semantics.
/// Lowering registers module declarations in this order.
pub type Mapping = indexmap::IndexMap<Value, Value>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Number {
    Integer(i64),
    Float(u64),
}

impl Number {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            Self::Float(_) => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Integer(value) => Some(*value as f64),
            Self::Float(bits) => Some(f64::from_bits(*bits)),
        }
    }
}

impl From<i64> for Number {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}
impl From<u64> for Number {
    fn from(value: u64) -> Self {
        Self::Integer(value.try_into().unwrap_or(i64::MAX))
    }
}
impl From<i32> for Number {
    fn from(value: i32) -> Self {
        Self::Integer(value.into())
    }
}
impl From<f64> for Number {
    fn from(value: f64) -> Self {
        Self::Float(value.to_bits())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaggedValue {
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Sequence(Vec<Value>),
    Mapping(Mapping),
    Tagged(Box<TaggedValue>),
}

impl std::hash::Hash for Value {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Null => {}
            Self::Bool(value) => value.hash(state),
            Self::Number(value) => value.hash(state),
            Self::String(value) => value.hash(state),
            Self::Sequence(values) => values.hash(state),
            Self::Mapping(values) => {
                for (key, value) in values {
                    key.hash(state);
                    value.hash(state);
                }
            }
            Self::Tagged(value) => value.value.hash(state),
        }
    }
}

impl serde::Serialize for Value {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{Error, SerializeMap, SerializeSeq};
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Number(Number::Integer(value)) => serializer.serialize_i64(*value),
            Self::Number(Number::Float(bits)) => serializer.serialize_f64(f64::from_bits(*bits)),
            Self::String(value) => serializer.serialize_str(value),
            Self::Sequence(values) => {
                let mut sequence = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    sequence.serialize_element(value)?;
                }
                sequence.end()
            }
            Self::Mapping(values) => {
                let mut map = serializer.serialize_map(Some(values.len()))?;
                for (key, value) in values {
                    let Self::String(key) = key else {
                        return Err(S::Error::custom(
                            "legacy compatibility map has a non-string JSON key",
                        ));
                    };
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
            Self::Tagged(value) => value.value.serialize(serializer),
        }
    }
}

impl std::ops::Index<&str> for Value {
    type Output = Value;
    fn index(&self, key: &str) -> &Self::Output {
        self.as_mapping()
            .and_then(|map| map.get(&Value::String(key.into())))
            .unwrap_or_else(|| panic!("missing legacy value key `{key}`"))
    }
}
impl std::ops::Index<usize> for Value {
    type Output = Value;
    fn index(&self, index: usize) -> &Self::Output {
        &self.as_sequence().expect("legacy value is not a sequence")[index]
    }
}

impl Value {
    pub fn as_mapping(&self) -> Option<&Mapping> {
        if let Self::Mapping(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_sequence(&self) -> Option<&Vec<Value>> {
        if let Self::Sequence(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Self::String(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        if let Self::Number(value) = self {
            value.as_i64()
        } else {
            None
        }
    }
    pub fn as_u64(&self) -> Option<u64> {
        self.as_i64().and_then(|value| value.try_into().ok())
    }
    pub fn as_f64(&self) -> Option<f64> {
        if let Self::Number(value) = self {
            value.as_f64()
        } else {
            None
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(value) = self {
            Some(*value)
        } else {
            None
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
    pub fn is_mapping(&self) -> bool {
        matches!(self, Self::Mapping(_))
    }
    pub fn is_string(&self) -> bool {
        matches!(self, Self::String(_))
    }
}
