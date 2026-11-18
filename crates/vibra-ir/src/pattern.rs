//! Checked patterns: what a `match` arm and a `let` test and bind.
//!
//! There is no member pattern: a constant pattern is a [`Pattern::Literal`],
//! whatever constant expression it was written as.

use crate::expr::{Checker, Node, Shape, expect};
use crate::{Constant, IrError, Type};

/// A pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pattern {
    /// `_`: matches anything and binds nothing.
    Discard,
    /// Binds the subject to a slot.
    Bind(usize),
    /// A constant pattern: matches the subject equal to the constant.
    Literal(Constant),
    /// A tuple's components, every one.
    Tuple(Vec<Self>),
    /// Some fields of a record, by name.
    Record(Vec<(String, Self)>),
    /// A list of exactly this many elements.
    List(Vec<Self>),
    /// A wrapper's representation: `Str(chars)`, `Bytes(bytes)`, or a
    /// user wrapper.
    Wrapper(Box<Self>),
    /// A case of a variant type and some of its fields.
    Case {
        /// The case name.
        case: String,
        /// The fields matched, by name.
        fields: Vec<(String, Self)>,
    },
}

/// One arm of a `match`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchArm {
    /// The pattern.
    pub pattern: Pattern,
    /// The body run when the pattern matches, with its slots bound.
    pub body: Node,
}

impl Checker<'_> {
    /// Checks `pattern` against a subject of type `subject`, binding its
    /// slots; `bound` records what each slot held before.
    pub(crate) fn check_pattern(
        &mut self,
        pattern: &Pattern,
        subject: &Type,
        bound: &mut Vec<(usize, Option<Type>)>,
    ) -> Result<(), IrError> {
        match pattern {
            Pattern::Discard => Ok(()),
            Pattern::Bind(slot) => {
                let held = self.bind(*slot, subject.clone())?;
                bound.push((*slot, held));
                Ok(())
            }
            Pattern::Literal(constant) => {
                self.check_constant(constant)?;
                if matches!(constant.value_type(), Type::F32 | Type::F64) {
                    return Err(IrError::InvalidExpression(
                        "a float literal is not a pattern".to_owned(),
                    ));
                }
                expect(subject, constant.value_type())
            }
            Pattern::Tuple(components) => match self.shape(subject)? {
                Shape::Tuple(types) if types.len() == components.len() => types
                    .iter()
                    .zip(components)
                    .try_for_each(|(ty, component)| {
                        self.check_pattern(component, ty, bound)
                    }),
                _ => Err(mismatch("tuple", subject)),
            },
            Pattern::Record(fields) => match self.shape(subject)? {
                Shape::Record(declared) => self.check_named(&declared, fields, bound),
                _ => Err(mismatch("record", subject)),
            },
            Pattern::List(elements) => match self.shape(subject)? {
                Shape::List(element) => elements.iter().try_for_each(|pattern| {
                    self.check_pattern(pattern, &element, bound)
                }),
                _ => Err(mismatch("list", subject)),
            },
            Pattern::Wrapper(inner) => match self.shape(subject)? {
                Shape::Wrapper(representation) => {
                    self.check_pattern(inner, &representation, bound)
                }
                _ => Err(mismatch("wrapper", subject)),
            },
            Pattern::Case { case, fields } => match self.shape(subject)? {
                Shape::Variant(cases, arguments) => {
                    let declared = cases
                        .iter()
                        .find(|declared| declared.name == *case)
                        .ok_or_else(|| {
                            IrError::InvalidExpression(format!("no case `{case}`"))
                        })?;
                    let declared = declared
                        .fields
                        .iter()
                        .map(|(name, value)| {
                            (name.clone(), value.instantiate(&arguments))
                        })
                        .collect::<Vec<_>>();
                    self.check_named(&declared, fields, bound)
                }
                _ => Err(mismatch("case", subject)),
            },
        }
    }

    fn check_named(
        &mut self,
        declared: &[(String, Type)],
        fields: &[(String, Pattern)],
        bound: &mut Vec<(usize, Option<Type>)>,
    ) -> Result<(), IrError> {
        Self::check_field_subset(declared, fields)?;
        for (name, pattern) in fields {
            let ty = declared
                .iter()
                .find_map(|(field, ty)| (field == name).then_some(ty))
                .ok_or_else(|| {
                    IrError::InvalidExpression(format!("no field `{name}`"))
                })?;
            self.check_pattern(pattern, ty, bound)?;
        }
        Ok(())
    }
}

fn mismatch(what: &str, subject: &Type) -> IrError {
    IrError::InvalidExpression(format!("a {what} pattern on {subject}"))
}
