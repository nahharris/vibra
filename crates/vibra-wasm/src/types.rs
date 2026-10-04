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

use std::collections::{BTreeMap, BTreeSet};

use vibra_ir::{FunctionSignature, Type, TypeBody, TypeDefinition, TypeId};

use crate::runtime::ValueClass;

/// The class of a value of `ty`, or the name of the kind of type that no
/// lowered value represents yet: `array` and `dict` (Step 8b), and `interface`
/// for an interface value or `any` (Step 9). A generic parameter has no class
/// of its own: a value of it is whatever its type argument says, so the class
/// is read from the cell at run time ([`ValueClass::Dyn`]). A compound type
/// that mentions a parameter is an arena value like any other.
pub(crate) fn class_of(ty: &Type) -> Result<ValueClass, &'static str> {
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
        | Type::Union(_)
        | Type::Function(_) => ValueClass::Ref,
        Type::Param(_) => ValueClass::Dyn,
        Type::Array(_) => return Err("array"),
        Type::Dict(..) => return Err("dict"),
        Type::Interface(..) | Type::Any => return Err("interface"),
    })
}

/// Adds the name of every generic parameter `ty` mentions to `into`.
pub(crate) fn params_of(ty: &Type, into: &mut BTreeSet<String>) {
    if let Type::Param(name) = ty {
        into.insert(name.clone());
    }
    for component in ty.components() {
        params_of(&component, into);
    }
}

/// The names of the generic parameters a signature mentions, in name order.
pub(crate) fn signature_params(signature: &FunctionSignature) -> Vec<String> {
    let mut names = BTreeSet::new();
    for slot in signature.slot_types() {
        params_of(&slot, &mut names);
    }
    params_of(&signature.result(), &mut names);
    names.into_iter().collect()
}

/// The name a generic parameter has in a body: a quantified `lambda` parameter
/// is `name#index@site` in its signature and `name` where the body writes it.
pub(crate) fn written_name(name: &str) -> &str {
    name.split_once('#').map_or(name, |(written, _)| written)
}

/// Binds the generic parameters of `pattern` so that it equals `actual`,
/// recording for each the type it meets, as the reference interpreter binds a
/// call: a parameter takes the first type it is matched with, and a shape that
/// does not match binds nothing. The types recorded may name the parameters of
/// the caller, which the caller resolves where the call is made.
pub(crate) fn bind(pattern: &Type, actual: &Type, bound: &mut BTreeMap<String, Type>) {
    let all =
        |patterns: &[Type], actuals: &[Type], bound: &mut BTreeMap<String, Type>| {
            if patterns.len() == actuals.len() {
                for (pattern, actual) in patterns.iter().zip(actuals) {
                    bind(pattern, actual, bound);
                }
            }
        };
    match (pattern, actual) {
        (Type::Param(name), _) => {
            bound.entry(name.clone()).or_insert_with(|| actual.clone());
        }
        (Type::Applied(left, patterns), Type::Applied(right, actuals))
        | (Type::Interface(left, patterns), Type::Interface(right, actuals))
            if left == right =>
        {
            all(patterns, actuals, bound);
        }
        (Type::Tuple(patterns), Type::Tuple(actuals))
        | (Type::Union(patterns), Type::Union(actuals)) => {
            all(patterns, actuals, bound)
        }
        (Type::Array(pattern), Type::Array(actual)) => bind(pattern, actual, bound),
        (Type::Dict(key, value), Type::Dict(actual_key, actual_value)) => {
            bind(key, actual_key, bound);
            bind(value, actual_value, bound);
        }
        (Type::Record(patterns), Type::Record(actuals))
        | (Type::Enum(patterns), Type::Enum(actuals)) => {
            if patterns.len() == actuals.len() {
                for ((name, pattern), (actual_name, actual)) in
                    patterns.iter().zip(actuals)
                {
                    if name == actual_name {
                        bind(pattern, actual, bound);
                    }
                }
            }
        }
        (Type::Function(pattern), Type::Function(actual)) => {
            all(&pattern.slot_types(), &actual.slot_types(), bound);
            bind(&pattern.result(), &actual.result(), bound);
        }
        _ => {}
    }
}

/// What distinguishes a type from every other type of the same children: the
/// key a run-time type descriptor is numbered by. A generic parameter that
/// nothing fixed has no head.
pub(crate) fn head_key(ty: &Type) -> String {
    match ty {
        Type::AtomSingleton(name) => format!("atom:{name}"),
        Type::Declared(id) => format!("declared:{id:?}"),
        Type::Applied(id, arguments) => format!("applied:{id:?}/{}", arguments.len()),
        Type::Interface(id, arguments) => {
            format!("interface:{id:?}/{}", arguments.len())
        }
        Type::Record(members) | Type::Enum(members) => {
            let names = members
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join(",");
            format!("{}:{names}", ty.as_str())
        }
        Type::Tuple(components) | Type::Union(components) => {
            format!("{}/{}", ty.as_str(), components.len())
        }
        Type::Function(signature) => {
            let labelled = signature
                .labelled()
                .iter()
                .map(|parameter| parameter.name())
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "fn/{}/{labelled}/{}",
                signature.parameters().len(),
                u8::from(signature.variadic().is_some())
            )
        }
        Type::Param(_) => "unresolved".to_owned(),
        other => other.as_str().to_owned(),
    }
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
            Self::Record(fields) => fields.iter().position(|(field, _)| field == name),
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
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_generic_parameter_is_classed_at_run_time_and_an_unlowered_type_names_its_step()
    {
        assert_eq!(class_of(&Type::Param("t".to_owned())), Ok(ValueClass::Dyn));
        assert_eq!(
            class_of(&Type::Tuple(vec![Type::Param("t".to_owned())])),
            Ok(ValueClass::Ref)
        );
        assert_eq!(
            class_of(&Type::Function(Box::new(FunctionSignature::new(
                vec![Type::Param("t".to_owned())],
                Type::Param("t".to_owned())
            )))),
            Ok(ValueClass::Ref)
        );
        assert_eq!(class_of(&Type::Array(Box::new(Type::I32))), Err("array"));
        assert_eq!(class_of(&Type::Any), Err("interface"));
        assert_eq!(
            class_of(&Type::Tuple(vec![Type::Array(Box::new(Type::I32))])),
            Ok(ValueClass::Ref)
        );
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
