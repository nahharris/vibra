//! Constant values of any type.
//!
//! `docs/spec/02-type-system.md`, "Constant patterns": a constant expression is
//! a literal, an atom, or `void`; a construction whose operands are all
//! constant; or a read of another constant. A labelled default is a constant
//! expression, so a signature holds one as a [`Constant`]: the checked form of
//! such an expression with no source origin and no module-value read, which
//! two signatures compare and print by value.

use vibra_diagnostics::ByteSpan;

use crate::{Expr, SourceOrigin, Type, Value, canonical_expr};

/// A checked constant expression, by value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Constant {
    /// A primitive value.
    Primitive(Value),
    /// A declared or anonymous tuple.
    Tuple {
        /// The tuple type.
        value_type: Type,
        /// Component constants in order.
        components: Vec<Constant>,
    },
    /// A declared or anonymous record.
    Record {
        /// The record type.
        value_type: Type,
        /// Field constants in checked order.
        fields: Vec<(String, Constant)>,
    },
    /// An enum variant, declared or anonymous.
    Variant {
        /// The enum type.
        value_type: Type,
        /// The selected variant.
        variant: String,
        /// The payload constant; `None` for a `void` payload.
        payload: Option<Box<Constant>>,
    },
    /// A wrapper type's representation.
    Wrap {
        /// The wrapper type.
        value_type: Type,
        /// The representation constant.
        value: Box<Constant>,
    },
    /// A union member widened to its union.
    Member {
        /// The union type.
        value_type: Type,
        /// The member's index in the union's member list.
        index: usize,
        /// The member constant.
        value: Box<Constant>,
    },
}

impl Constant {
    /// The statically known type of this constant.
    #[must_use]
    pub fn value_type(&self) -> Type {
        match self {
            Self::Primitive(value) => value.ty(),
            Self::Tuple { value_type, .. }
            | Self::Record { value_type, .. }
            | Self::Variant { value_type, .. }
            | Self::Wrap { value_type, .. }
            | Self::Member { value_type, .. } => value_type.clone(),
        }
    }

    /// The constant a checked expression denotes, when it is one: a literal, a
    /// construction whose operands are constants, or a widening of one. A read
    /// of a module value is not one on its own.
    #[must_use]
    pub fn from_expr(expression: &Expr) -> Option<Self> {
        Self::from_expr_with(expression, &mut |_| None)
    }

    /// As [`Self::from_expr`], where `global` gives the constant a read of the
    /// module value at an index denotes, when that value is a constant.
    pub fn from_expr_with(
        expression: &Expr,
        global: &mut dyn FnMut(usize) -> Option<Self>,
    ) -> Option<Self> {
        Some(match expression {
            Expr::Global { index, .. } => global(*index)?,
            Expr::Literal { value, .. } => Self::Primitive(value.clone()),
            Expr::Tuple {
                value_type,
                components,
                ..
            } => Self::Tuple {
                value_type: value_type.clone(),
                components: components
                    .iter()
                    .map(|component| Self::from_expr_with(component, global))
                    .collect::<Option<_>>()?,
            },
            Expr::Record {
                value_type, fields, ..
            } => Self::Record {
                value_type: value_type.clone(),
                fields: fields
                    .iter()
                    .map(|(name, field)| {
                        Some((name.clone(), Self::from_expr_with(field, global)?))
                    })
                    .collect::<Option<_>>()?,
            },
            Expr::Variant {
                value_type,
                variant,
                payload,
                ..
            } => {
                // A `bool` variant is the literal of its representation.
                if *value_type == Type::Bool && payload.is_none() {
                    return Some(Self::Primitive(Value::Bool(variant == "true")));
                }
                Self::Variant {
                    value_type: value_type.clone(),
                    variant: variant.clone(),
                    payload: match payload {
                        Some(payload) => {
                            Some(Box::new(Self::from_expr_with(payload, global)?))
                        }
                        None => None,
                    },
                }
            }
            Expr::Wrap {
                value_type, value, ..
            } => Self::Wrap {
                value_type: value_type.clone(),
                value: Box::new(Self::from_expr_with(value, global)?),
            },
            Expr::Widen {
                value,
                value_type,
                member,
                ..
            } => match member {
                None => Self::from_expr_with(value, global)?,
                Some(index) => Self::Member {
                    value_type: value_type.clone(),
                    index: *index,
                    value: Box::new(Self::from_expr_with(value, global)?),
                },
            },
            _ => return None,
        })
    }

    /// This constant as a checked expression at `origin`.
    #[must_use]
    pub fn to_expr(&self, origin: &SourceOrigin) -> Expr {
        match self {
            Self::Primitive(value) => Expr::literal(value.clone(), origin.clone()),
            Self::Tuple {
                value_type,
                components,
            } => Expr::Tuple {
                value_type: value_type.clone(),
                components: components
                    .iter()
                    .map(|component| component.to_expr(origin))
                    .collect(),
                origin: origin.clone(),
            },
            Self::Record { value_type, fields } => Expr::Record {
                value_type: value_type.clone(),
                fields: fields
                    .iter()
                    .map(|(name, field)| (name.clone(), field.to_expr(origin)))
                    .collect(),
                origin: origin.clone(),
            },
            Self::Variant {
                value_type,
                variant,
                payload,
            } => Expr::Variant {
                value_type: value_type.clone(),
                variant: variant.clone(),
                payload: payload
                    .as_ref()
                    .map(|payload| Box::new(payload.to_expr(origin))),
                origin: origin.clone(),
            },
            Self::Wrap { value_type, value } => Expr::Wrap {
                value_type: value_type.clone(),
                value: Box::new(value.to_expr(origin)),
                origin: origin.clone(),
            },
            Self::Member {
                value_type,
                index,
                value,
            } => Expr::Widen {
                value: Box::new(value.to_expr(origin)),
                value_type: value_type.clone(),
                member: Some(*index),
                origin: origin.clone(),
            },
        }
    }

    /// The canonical value encoding of this constant, which carries no origin.
    #[must_use]
    pub fn canonical_vibon(&self) -> String {
        match self {
            Self::Primitive(value) => value.canonical_vibon(),
            other => canonical_expr(
                &other.to_expr(&SourceOrigin::new("", ByteSpan::empty_at(0))),
            ),
        }
    }
}
