//! Checked patterns (M3 Step 5).
//!
//! A checked pattern is already resolved against its expected type: a binder
//! names its activation slot and type, a constructor names its variant, and a
//! record pattern keeps only the fields it wrote. `match` arms, destructuring
//! `let`, and destructuring parameters all lower to these patterns.

use crate::{Type, Value};

/// One checked pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pattern {
    /// A discard: matches every value and binds nothing.
    Wildcard,
    /// A binder: matches every value and stores it in `slot`.
    Bind {
        /// The activation slot the matched value is stored in.
        slot: usize,
        /// The statically checked type of the bound value.
        value_type: Type,
    },
    /// A literal: matches exactly one value.
    Literal(Value),
    /// An enum variant, declared or anonymous, with its payload pattern;
    /// `None` for a `void` payload.
    Variant {
        /// The selected variant name.
        variant: String,
        /// The payload pattern, when the variant carries one.
        payload: Option<Box<Pattern>>,
    },
    /// A record, declared or anonymous; omitted fields match anything.
    Record(Vec<(String, Pattern)>),
    /// A tuple, declared or anonymous, with one pattern per component.
    Tuple(Vec<Pattern>),
    /// A wrapper type's representation.
    Wrap(Box<Pattern>),
    /// An array of exactly these elements.
    Array(Vec<Pattern>),
}

impl Pattern {
    /// Every binder in the pattern as `(slot, type)`, in written order.
    #[must_use]
    pub fn bindings(&self) -> Vec<(usize, Type)> {
        let mut found = Vec::new();
        self.collect_bindings(&mut found);
        found
    }

    fn collect_bindings(&self, found: &mut Vec<(usize, Type)>) {
        match self {
            Self::Wildcard | Self::Literal(_) => {}
            Self::Bind { slot, value_type } => found.push((*slot, value_type.clone())),
            Self::Variant { payload, .. } => {
                if let Some(payload) = payload {
                    payload.collect_bindings(found);
                }
            }
            Self::Record(fields) => {
                for (_, field) in fields {
                    field.collect_bindings(found);
                }
            }
            Self::Tuple(items) | Self::Array(items) => {
                for item in items {
                    item.collect_bindings(found);
                }
            }
            Self::Wrap(inner) => inner.collect_bindings(found),
        }
    }

    /// The canonical encoding of the pattern, for checked-program output.
    #[must_use]
    pub fn canonical_vibon(&self) -> String {
        match self {
            Self::Wildcard => "(record kind: @wildcard)".to_owned(),
            Self::Bind { slot, value_type } => format!(
                "(record kind: @bind slot: {slot}u64 type: {})",
                crate::canonical_type(value_type)
            ),
            Self::Literal(value) => {
                format!("(record kind: @literal value: {})", value.canonical_vibon())
            }
            Self::Variant { variant, payload } => format!(
                "(record kind: @variant variant: @{variant}{})",
                payload
                    .as_deref()
                    .map(|payload| format!(" payload: {}", payload.canonical_vibon()))
                    .unwrap_or_default()
            ),
            Self::Record(fields) => format!(
                "(record kind: @record fields: (record{}))",
                fields
                    .iter()
                    .map(|(name, field)| format!(
                        " {name}: {}",
                        field.canonical_vibon()
                    ))
                    .collect::<String>()
            ),
            Self::Tuple(items) => {
                format!("(record kind: @tuple items: {})", encode_all(items))
            }
            Self::Wrap(inner) => {
                format!("(record kind: @wrap value: {})", inner.canonical_vibon())
            }
            Self::Array(items) => {
                format!("(record kind: @array items: {})", encode_all(items))
            }
        }
    }
}

fn encode_all(items: &[Pattern]) -> String {
    format!(
        "(array{})",
        items
            .iter()
            .map(|item| format!(" {}", item.canonical_vibon()))
            .collect::<String>()
    )
}

/// One `match` arm: a pattern and the result evaluated when it matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchArm {
    /// The checked pattern.
    pub pattern: Pattern,
    /// The result expression, evaluated with the pattern's binders in scope.
    pub body: crate::Expr,
}
