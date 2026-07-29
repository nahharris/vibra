//! Internal compatibility value tree for the retiring map-based lowering seam.
//!
//! This is deliberately data-only: Vibra source is parsed by the typed
//! S-expression frontend, never by this module.  It replaces the incidental
//! YAML crate type previously used by the temporary adapter.

use std::collections::BTreeMap;

pub type Mapping = BTreeMap<Value, Value>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub struct TaggedValue {
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Sequence(Vec<Value>),
    Mapping(Mapping),
    Tagged(Box<TaggedValue>),
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
