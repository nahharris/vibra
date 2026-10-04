//! Patterns as data: what a checked pattern asks of the value it meets.
//!
//! A pattern is lowered by one recursive scheme, and the kind of the pattern
//! decides nothing in it except through this table. [`decompose`] maps one
//! checked pattern, at the type of the value it meets, to a [`Node`]: the one
//! **test** the pattern makes of that value in place, the **binder** it stores
//! the value in, and the **children**, the parts of the value that its
//! sub-patterns meet. The lowering then tests a node, descends into its
//! children, and binds a node in the same way for every kind, and the walk that
//! reports what the emitter cannot lower reads the same table.
//!
//! | Pattern | Test | Binder | Children |
//! | --- | --- | --- | --- |
//! | discard | none | none | none |
//! | binder | none | its slot | none |
//! | `bool` literal | the discriminant | none | none |
//! | scalar literal | the cell's bits | none | none |
//! | `str`, `bytes`, atom literal | equal characters or bytes | none | none |
//! | enum variant | the discriminant | none | the payload, when there is one |
//! | record | none | none | each written field |
//! | tuple | none | none | each component |
//! | wrapper | none | none | the representation |
//! | `(as member p)` | the member's discriminant | none | the payload |
//! | array | not lowered (Step 8b) | | |
//!
//! A pattern that names a constant is already the pattern of the constant's
//! value in the checked program, and `bool` has no pattern of its own: the
//! checker writes its two variants as boolean literals, which the table reads
//! as the discriminant of the enum `bool` is.

use vibra_ir::{Pattern, SourceOrigin, Type, Value};

use crate::form::{Form, NotLowered, UnloweredForm};
use crate::runtime::ValueClass;
use crate::types::{Shape, TypeEnv, class_of};

/// What a test of a value in place asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Test<'p> {
    /// Nothing: the pattern matches whatever the value is.
    Always,
    /// The cell holds exactly these bits: its low 32 bits, or all 64.
    Cell { wide: bool, bits: u64 },
    /// The object's discriminant is this.
    Variant(u32),
    /// The object holds the same characters or bytes as this literal.
    Data(&'p Value),
}

/// Whether a part of a value has a cell in the object that holds it.
///
/// An enum's payload of `void` has none, because the encoding has none, and the
/// payload of a generic type has one exactly when its type argument is not
/// `void`, which only the object itself says at run time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Presence {
    /// Always a cell.
    Always,
    /// Never a cell: the value is `void`.
    Never,
    /// A cell when the object has any component.
    Runtime,
}

impl Presence {
    /// The presence of the payload slot of type `ty` in an enum.
    pub(crate) const fn of_payload(ty: &Type) -> Self {
        match ty {
            Type::Void => Self::Never,
            Type::Param(_) => Self::Runtime,
            _ => Self::Always,
        }
    }
}

/// One part of a value that a sub-pattern meets.
#[derive(Clone, Debug)]
pub(crate) struct Child<'p> {
    pub(crate) pattern: &'p Pattern,
    /// The type of the part.
    pub(crate) ty: Type,
    /// Its position among the cells of the object.
    pub(crate) position: u32,
    /// The number of cells the object has when the part is present.
    pub(crate) len: u32,
    pub(crate) presence: Presence,
}

/// One checked pattern at the type of the value it meets.
#[derive(Clone, Debug)]
pub(crate) struct Node<'p> {
    pub(crate) test: Test<'p>,
    /// The slot of the checked IR the value is stored in, and its type.
    pub(crate) binder: Option<(usize, &'p Type)>,
    pub(crate) children: Vec<Child<'p>>,
}

impl<'p> Node<'p> {
    const fn new(test: Test<'p>) -> Self {
        Self {
            test,
            binder: None,
            children: Vec::new(),
        }
    }

    /// Whether the pattern asks nothing of the value and has no part to look
    /// into, and so binds it at most. It is all a part with no cell can meet.
    pub(crate) fn only_binds(&self) -> bool {
        self.test == Test::Always && self.children.is_empty()
    }

    /// Whether the pattern does nothing at all: no test, no binder, no part.
    pub(crate) fn is_inert(&self) -> bool {
        self.only_binds() && self.binder.is_none()
    }
}

/// How a literal sits in memory, which is all a literal expression and a
/// literal pattern each need to know of a [`Value`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Literal<'v> {
    /// A scalar: the class that holds it and its bits, zero-extended to 64.
    Scalar(ValueClass, u64),
    /// A `bool`, the enum whose variants `false` and `true` have the
    /// discriminants `0` and `1`.
    Bool(bool),
    /// A `str`: an object of its characters.
    Str(&'v str),
    /// An atom: an object of the characters of its spelling.
    Atom(&'v str),
    /// `bytes`: an object of its bytes.
    Bytes(&'v [u8]),
}

/// Classifies a literal.
pub(crate) fn literal(value: &Value) -> Literal<'_> {
    let narrow = |value: i32| u64::from(value.cast_unsigned());
    match value {
        Value::Void => Literal::Scalar(ValueClass::Void, 0),
        Value::Char(value) => {
            Literal::Scalar(ValueClass::I32, u64::from(u32::from(*value)))
        }
        Value::I8(value) => Literal::Scalar(ValueClass::I32, narrow(i32::from(*value))),
        Value::I16(value) => {
            Literal::Scalar(ValueClass::I32, narrow(i32::from(*value)))
        }
        Value::I32(value) => Literal::Scalar(ValueClass::I32, narrow(*value)),
        Value::U8(value) => Literal::Scalar(ValueClass::I32, u64::from(*value)),
        Value::U16(value) => Literal::Scalar(ValueClass::I32, u64::from(*value)),
        Value::U32(value) => Literal::Scalar(ValueClass::I32, u64::from(*value)),
        Value::I64(value) => Literal::Scalar(ValueClass::I64, value.cast_unsigned()),
        Value::U64(value) => Literal::Scalar(ValueClass::I64, *value),
        Value::F32(bits) => Literal::Scalar(ValueClass::F32, u64::from(*bits)),
        Value::F64(bits) => Literal::Scalar(ValueClass::F64, *bits),
        Value::Bool(value) => Literal::Bool(*value),
        Value::Str(text) => Literal::Str(text),
        Value::Atom(name) => Literal::Atom(name),
        Value::Bytes(bytes) => Literal::Bytes(bytes),
    }
}

/// The table: what `pattern` asks of a value of type `ty`.
///
/// # Errors
///
/// The form a pattern or a type of this step does not lower: an array pattern,
/// a wrapper pattern over `str` or `bytes`, or a type with no shape where the
/// pattern needs one.
pub(crate) fn decompose<'p>(
    env: &TypeEnv<'_>,
    pattern: &'p Pattern,
    ty: &Type,
    origin: &SourceOrigin,
) -> Result<Node<'p>, NotLowered> {
    let unshaped = || {
        NotLowered::type_of(class_of(ty).err().unwrap_or("shape"), Some(origin.clone()))
    };
    let shape = || env.shape(ty).ok_or_else(unshaped);
    let position = |index: usize| u32::try_from(index).unwrap_or(u32::MAX);
    Ok(match pattern {
        Pattern::Wildcard => Node::new(Test::Always),
        Pattern::Bind { slot, value_type } => Node {
            binder: Some((*slot, value_type)),
            ..Node::new(Test::Always)
        },
        Pattern::Literal(value) => Node::new(match literal(value) {
            Literal::Scalar(class, bits) => Test::Cell {
                wide: matches!(class, ValueClass::I64 | ValueClass::F64),
                bits,
            },
            Literal::Bool(value) => Test::Variant(u32::from(value)),
            Literal::Str(_) | Literal::Atom(_) | Literal::Bytes(_) => Test::Data(value),
        }),
        Pattern::Variant { variant, payload } => {
            let shape = shape()?;
            let Shape::Enum(variants) = &shape else {
                return Err(unshaped());
            };
            let discriminant = shape.variant(variant).ok_or_else(unshaped)?;
            let slot = variants.get(discriminant).map(|(_, slot)| slot.clone());
            let mut node = Node::new(Test::Variant(position(discriminant)));
            if let (Some(payload), Some(slot)) = (payload.as_deref(), slot) {
                node.children.push(Child {
                    pattern: payload,
                    presence: Presence::of_payload(&slot),
                    ty: slot,
                    position: 0,
                    len: 1,
                });
            }
            node
        }
        Pattern::Record(fields) => {
            let shape = shape()?;
            let types = shape.components();
            let mut node = Node::new(Test::Always);
            for (name, field) in fields {
                let index = shape.field(name).ok_or_else(unshaped)?;
                node.children.push(Child {
                    pattern: field,
                    ty: types.get(index).cloned().ok_or_else(unshaped)?,
                    position: position(index),
                    len: position(types.len()),
                    presence: Presence::Always,
                });
            }
            node
        }
        Pattern::Tuple(items) => {
            let shape = shape()?;
            let types = shape.components();
            if !matches!(shape, Shape::Tuple(_)) || types.len() != items.len() {
                return Err(unshaped());
            }
            let mut node = Node::new(Test::Always);
            for (index, (item, component)) in items.iter().zip(types).enumerate() {
                node.children.push(Child {
                    pattern: item,
                    ty: component,
                    position: position(index),
                    len: position(items.len()),
                    presence: Presence::Always,
                });
            }
            node
        }
        // A wrapper over `str` or `bytes` is written over an array.
        Pattern::Wrap(_) if matches!(ty, Type::Str | Type::Bytes) => {
            return Err(NotLowered::from_uses(vec![UnloweredForm::new(
                Form::Wrap,
                Some(origin.clone()),
            )])
            .unwrap_or_else(|| NotLowered::single(Form::Wrap)));
        }
        Pattern::Wrap(inner) => {
            let Shape::Wrapper(representation) = shape()? else {
                return Err(unshaped());
            };
            let mut node = Node::new(Test::Always);
            node.children.push(Child {
                pattern: inner,
                ty: representation,
                position: 0,
                len: 1,
                presence: Presence::Always,
            });
            node
        }
        Pattern::Array(_) => {
            return Err(NotLowered::from_uses(vec![
                UnloweredForm::new(Form::Array, Some(origin.clone()))
                    .with_detail("pattern"),
            ])
            .unwrap_or_else(|| NotLowered::single(Form::Array)));
        }
        Pattern::Member {
            index,
            member,
            pattern,
        } => {
            let mut node = Node::new(Test::Variant(position(*index)));
            node.children.push(Child {
                pattern,
                ty: member.clone(),
                position: 0,
                len: 1,
                presence: Presence::Always,
            });
            node
        }
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use vibra_diagnostics::ByteSpan;

    fn origin() -> SourceOrigin {
        SourceOrigin::new("input.vib", ByteSpan::new(0, 1))
    }

    fn node<'p>(pattern: &'p Pattern, ty: &Type) -> Node<'p> {
        decompose(&TypeEnv::new(&[]), pattern, ty, &origin())
            .expect("a lowered pattern")
    }

    #[test]
    fn a_discard_does_nothing_and_a_binder_only_binds() {
        let discard = node(&Pattern::Wildcard, &Type::I32);
        assert!(discard.is_inert());
        let binder = Pattern::Bind {
            slot: 3,
            value_type: Type::Str,
        };
        let bound = node(&binder, &Type::Str);
        assert!(bound.only_binds() && !bound.is_inert());
        assert_eq!(bound.binder, Some((3, &Type::Str)));
    }

    #[test]
    fn a_literal_is_the_test_its_representation_asks() {
        let narrow = Pattern::Literal(Value::I8(-3));
        assert_eq!(
            node(&narrow, &Type::I8).test,
            Test::Cell {
                wide: false,
                bits: 0xFFFF_FFFD
            }
        );
        let wide = Pattern::Literal(Value::U64(7));
        assert_eq!(
            node(&wide, &Type::U64).test,
            Test::Cell {
                wide: true,
                bits: 7
            }
        );
        // `bool` is an enum, so its literal is a discriminant.
        let truth = Pattern::Literal(Value::Bool(true));
        assert_eq!(node(&truth, &Type::Bool).test, Test::Variant(1));
        let text = Pattern::Literal(Value::Str("a".to_owned()));
        assert!(matches!(node(&text, &Type::Str).test, Test::Data(_)));
        let atom = Pattern::Literal(Value::Atom("ok".to_owned()));
        assert!(matches!(node(&atom, &Type::Atom).test, Test::Data(_)));
    }

    #[test]
    fn a_variant_tests_its_discriminant_and_a_void_payload_has_no_cell() {
        let ty = Type::Enum(vec![
            ("none".to_owned(), Type::Void),
            ("some".to_owned(), Type::I32),
        ]);
        let pattern = Pattern::Variant {
            variant: "some".to_owned(),
            payload: Some(Box::new(Pattern::Wildcard)),
        };
        let found = node(&pattern, &ty);
        assert_eq!(found.test, Test::Variant(1));
        assert_eq!(found.children[0].presence, Presence::Always);
        let none = Pattern::Variant {
            variant: "none".to_owned(),
            payload: Some(Box::new(Pattern::Wildcard)),
        };
        assert_eq!(node(&none, &ty).children[0].presence, Presence::Never);
        assert_eq!(
            Presence::of_payload(&Type::Param("t".to_owned())),
            Presence::Runtime
        );
    }

    #[test]
    fn a_tuple_and_a_record_name_the_parts_their_patterns_meet() {
        let tuple = Type::Tuple(vec![Type::I32, Type::Str]);
        let pattern = Pattern::Tuple(vec![Pattern::Wildcard, Pattern::Wildcard]);
        let found = node(&pattern, &tuple);
        assert_eq!(found.test, Test::Always);
        let parts = found
            .children
            .iter()
            .map(|child| (child.position, child.len, child.ty.clone()))
            .collect::<Vec<_>>();
        assert_eq!(parts, vec![(0, 2, Type::I32), (1, 2, Type::Str)]);
        let record = Type::Record(vec![
            ("a".to_owned(), Type::I32),
            ("b".to_owned(), Type::Str),
        ]);
        let pattern = Pattern::Record(vec![("b".to_owned(), Pattern::Wildcard)]);
        let found = node(&pattern, &record);
        assert_eq!(found.children.len(), 1);
        assert_eq!((found.children[0].position, found.children[0].len), (1, 2));
    }

    #[test]
    fn a_member_tests_its_position_in_the_union() {
        let pattern = Pattern::Member {
            index: 2,
            member: Type::U8,
            pattern: Box::new(Pattern::Wildcard),
        };
        let union = Type::Union(vec![Type::Bool, Type::Str, Type::U8]);
        let found = node(&pattern, &union);
        assert_eq!(found.test, Test::Variant(2));
        assert_eq!(found.children[0].ty, Type::U8);
    }

    #[test]
    fn an_array_pattern_and_a_wrapper_over_text_name_their_forms() {
        let array = Pattern::Array(vec![Pattern::Wildcard]);
        let error = decompose(
            &TypeEnv::new(&[]),
            &array,
            &Type::Array(Box::new(Type::I32)),
            &origin(),
        )
        .expect_err("not lowered");
        assert_eq!(error.forms()[0].form(), Form::Array);
        assert_eq!(error.forms()[0].detail(), Some("pattern"));
        let wrapper = Pattern::Wrap(Box::new(Pattern::Wildcard));
        let error = decompose(&TypeEnv::new(&[]), &wrapper, &Type::Str, &origin())
            .expect_err("not lowered");
        assert_eq!(error.forms()[0].form(), Form::Wrap);
    }
}
