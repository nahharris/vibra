//! What the emitter knows of types: which values a type has, in what class,
//! and the shape of a compound type's components.
//!
//! A type's **class** says how a value of it sits in a cell: a scalar of one of
//! four widths, `void` (a scalar `0`), or a reference to an arena value. A
//! compound type's **shape** lists its components in the order the layout
//! stores them, which is the order the specification fixes for the host:
//! declaration order for a declared record, enum, or union, and canonical order
//! for an anonymous one, so a discriminant is an index into the list written
//! here and nowhere else.

use std::collections::BTreeMap;

use vibra_ir::{Type, TypeBody, TypeDefinition, TypeId};

use crate::runtime::ValueClass;

/// The class of a value of `ty`, or the name of the kind of type that no
/// lowered value represents yet: `param` for a type with a generic parameter
/// (Step 6 passes type arguments), `function` (Step 6), `array` and `dict`
/// (Step 8b), and `interface` for an interface value or `any` (Step 9).
pub(crate) fn class_of(ty: &Type) -> Result<ValueClass, &'static str> {
    if ty.has_params() {
        return Err("param");
    }
    Ok(match ty {
        // `never` has no value; where a slot is laid out it is a scalar zero.
        Type::Void | Type::Never => ValueClass::Void,
        Type::Char
        | Type::I8
        | Type::I16
        | Type::I32
        | Type::U8
        | Type::U16
        | Type::U32 => ValueClass::I32,
        Type::I64 | Type::U64 => ValueClass::I64,
        Type::F32 => ValueClass::F32,
        Type::F64 => ValueClass::F64,
        // `bool` is an enum arena value, and every other type below is an
        // arena value of its own kind.
        Type::Bool
        | Type::Str
        | Type::Bytes
        | Type::Atom
        | Type::AtomSingleton(_)
        | Type::Declared(_)
        | Type::Applied(..)
        | Type::Record(_)
        | Type::Enum(_)
        | Type::Tuple(_)
        | Type::Union(_) => ValueClass::Ref,
        Type::Function(_) => return Err("function"),
        Type::Array(_) => return Err("array"),
        Type::Dict(..) => return Err("dict"),
        Type::Interface(..) | Type::Any => return Err("interface"),
        Type::Param(_) => return Err("param"),
    })
}

/// The components of a compound type, in layout order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    /// A record: its fields.
    Record(Vec<(String, Type)>),
    /// An enum: its variants, each with the type of its payload slot.
    Enum(Vec<(String, Type)>),
    /// A wrapper: its representation.
    Wrapper(Type),
    /// A tuple: its components.
    Tuple(Vec<Type>),
    /// A union: its member types.
    Union(Vec<Type>),
}

impl Shape {
    /// The index of a field of a record shape.
    pub(crate) fn field(&self, name: &str) -> Option<usize> {
        match self {
            Self::Record(fields) => {
                fields.iter().position(|(field, _)| field == name)
            }
            _ => None,
        }
    }

    /// The discriminant of a variant of an enum shape, which is its
    /// declaration order.
    pub(crate) fn variant(&self, name: &str) -> Option<usize> {
        match self {
            Self::Enum(variants) => {
                variants.iter().position(|(variant, _)| variant == name)
            }
            _ => None,
        }
    }

    /// The types of the components, in layout order. An enum's components are
    /// not one list, so it has none here.
    pub(crate) fn components(&self) -> Vec<Type> {
        match self {
            Self::Record(fields) => {
                fields.iter().map(|(_, value)| value.clone()).collect()
            }
            Self::Tuple(components) | Self::Union(components) => components.clone(),
            Self::Wrapper(representation) => vec![representation.clone()],
            Self::Enum(_) => Vec::new(),
        }
    }
}

/// The declared types of a program, by identity.
#[derive(Debug)]
pub(crate) struct TypeEnv<'a> {
    definitions: BTreeMap<&'a TypeId, &'a TypeDefinition>,
}

impl<'a> TypeEnv<'a> {
    pub(crate) fn new(types: &'a [TypeDefinition]) -> Self {
        Self {
            definitions: types
                .iter()
                .map(|definition| (definition.id(), definition))
                .collect(),
        }
    }

    /// The shape of a compound type: a declared or applied type by its
    /// instantiated body, and an anonymous record, enum, tuple, or union by
    /// its own members. `None` for every other type.
    pub(crate) fn shape(&self, ty: &Type) -> Option<Shape> {
        match ty {
            Type::Declared(id) => {
                let definition = self.definitions.get(id)?;
                Some(from_body(definition.instantiate(&[])?))
            }
            Type::Applied(id, arguments) => {
                let definition = self.definitions.get(id)?;
                Some(from_body(definition.instantiate(arguments)?))
            }
            Type::Record(fields) => Some(Shape::Record(fields.clone())),
            Type::Enum(variants) => Some(Shape::Enum(variants.clone())),
            Type::Tuple(components) => Some(Shape::Tuple(components.clone())),
            Type::Union(members) => Some(Shape::Union(members.clone())),
            _ => None,
        }
    }
}

fn from_body(body: TypeBody) -> Shape {
    match body {
        TypeBody::Record(fields) => Shape::Record(fields),
        TypeBody::Enum(variants) => Shape::Enum(variants),
        TypeBody::Wrapper(representation) => Shape::Wrapper(representation),
        TypeBody::Tuple(components) => Shape::Tuple(components),
        TypeBody::Union(members) => Shape::Union(members),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generic_or_unlowered_type_names_the_step_that_owns_it() {
        assert_eq!(class_of(&Type::Param("t".to_owned())), Err("param"));
        assert_eq!(
            class_of(&Type::Tuple(vec![Type::Param("t".to_owned())])),
            Err("param")
        );
        assert_eq!(class_of(&Type::Array(Box::new(Type::I32))), Err("array"));
        assert_eq!(class_of(&Type::Any), Err("interface"));
        assert_eq!(class_of(&Type::Tuple(vec![Type::Array(Box::new(Type::I32))])), Ok(ValueClass::Ref));
    }

    #[test]
    fn scalars_are_cells_and_everything_else_is_a_reference() {
        assert_eq!(class_of(&Type::U8), Ok(ValueClass::I32));
        assert_eq!(class_of(&Type::U64), Ok(ValueClass::I64));
        assert_eq!(class_of(&Type::F32), Ok(ValueClass::F32));
        assert_eq!(class_of(&Type::Void), Ok(ValueClass::Void));
        assert_eq!(class_of(&Type::Bool), Ok(ValueClass::Ref));
        assert_eq!(class_of(&Type::Str), Ok(ValueClass::Ref));
    }

    #[test]
    fn an_anonymous_enum_orders_its_variants_as_the_type_does() {
        let env = TypeEnv::new(&[]);
        let shape = env
            .shape(&Type::Enum(vec![
                ("none".to_owned(), Type::Void),
                ("some".to_owned(), Type::I32),
            ]))
            .expect("a shape");
        assert_eq!(shape.variant("none"), Some(0));
        assert_eq!(shape.variant("some"), Some(1));
        assert_eq!(shape.variant("other"), None);
    }
}
