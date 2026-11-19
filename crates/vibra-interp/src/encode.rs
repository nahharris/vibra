//! Conversion between runtime values and the neutral [`Value`] of the
//! checked IR, at a closed static type, and the `@value.v2` observation
//! (`docs/spec/06-runtime.md`, "Canonical value encoding").
//!
//! The conversion is directed by the type, because a runtime value does not
//! carry its own: a `Str`, a `Bool`, and a tag are rendered as the declared
//! types they belong to. Every NaN leaves as the canonical one,
//! so neither its sign nor its payload is observable.

use std::rc::Rc;

use vibra_ir::{
    CANONICAL_NAN_F32, CANONICAL_NAN_F64, CheckedModuleSet, EncodeError, Library, Type,
    TypeBody, Value, encode_value, value_document,
};

use crate::machine::Halt;
use crate::value::{CaseVal, FunctionVal, Held, Repr, Val};

fn mismatch(ty: &Type) -> Halt {
    Halt::Invalid(format!("a value that does not fit {ty}"))
}

const fn canonical_f32(bits: u32) -> u32 {
    if f32::from_bits(bits).is_nan() {
        CANONICAL_NAN_F32
    } else {
        bits
    }
}

const fn canonical_f64(bits: u64) -> u64 {
    if f64::from_bits(bits).is_nan() {
        CANONICAL_NAN_F64
    } else {
        bits
    }
}

/// The declared fields of a case or a record of `id` at `arguments`.
fn instantiated(fields: &[(String, Type)], arguments: &[Type]) -> Vec<(String, Type)> {
    fields
        .iter()
        .map(|(name, ty)| (name.clone(), ty.instantiate(arguments)))
        .collect()
}

fn named_fields(
    set: &CheckedModuleSet,
    declared: &[(String, Type)],
    written: &[(&str, Val<'_>)],
    ty: &Type,
) -> Result<Vec<(String, Value)>, Halt> {
    if declared.len() != written.len() {
        return Err(mismatch(ty));
    }
    declared
        .iter()
        .map(|(name, field_type)| {
            let value = written
                .iter()
                .find_map(|(field, value)| (*field == name.as_str()).then_some(value))
                .ok_or_else(|| mismatch(ty))?;
            Ok((name.clone(), to_neutral(set, value, field_type)?))
        })
        .collect()
}

/// The neutral value of `value` at the closed type `ty`.
pub(crate) fn to_neutral(
    set: &CheckedModuleSet,
    value: &Val<'_>,
    ty: &Type,
) -> Result<Value, Halt> {
    Ok(match (ty, &value.0) {
        (Type::I8, Repr::I8(n)) => Value::I8(*n),
        (Type::I16, Repr::I16(n)) => Value::I16(*n),
        (Type::I32, Repr::I32(n)) => Value::I32(*n),
        (Type::I64, Repr::I64(n)) => Value::I64(*n),
        (Type::U8, Repr::U8(n)) => Value::U8(*n),
        (Type::U16, Repr::U16(n)) => Value::U16(*n),
        (Type::U32, Repr::U32(n)) => Value::U32(*n),
        (Type::U64, Repr::U64(n)) => Value::U64(*n),
        (Type::F32, Repr::F32(bits)) => Value::F32(canonical_f32(*bits)),
        (Type::F64, Repr::F64(bits)) => Value::F64(canonical_f64(*bits)),
        (Type::Char, Repr::Char(c)) => Value::Char(*c),
        (Type::Atom | Type::AtomSingleton(_), Repr::Atom(atom)) => {
            Value::Atom((*atom).to_owned())
        }
        (Type::Void, Repr::Void) => Value::Void,
        (Type::Any | Type::Interface(..), Repr::Interface(held)) => Value::Interface {
            held_type: held.held_type.clone(),
            held: Box::new(to_neutral(set, &held.value, &held.held_type)?),
        },
        (Type::List(element), Repr::List(items)) => Value::List(
            items
                .iter()
                .map(|item| to_neutral(set, item, element))
                .collect::<Result<_, _>>()?,
        ),
        (Type::Tuple(types), Repr::Tuple(values)) if types.len() == values.len() => {
            Value::Tuple(
                values
                    .iter()
                    .zip(types)
                    .map(|(value, ty)| to_neutral(set, value, ty))
                    .collect::<Result<_, _>>()?,
            )
        }
        (Type::Record(fields), Repr::Record(written)) => {
            Value::Record(named_fields(set, fields.entries(), written, ty)?)
        }
        (Type::Fn(_) | Type::Proc(_), Repr::Function(function)) => Value::Function {
            function: function.function,
            type_arguments: function.type_arguments.clone(),
        },
        (Type::Fn(_) | Type::Proc(_), Repr::Closure(closure)) => Value::Closure {
            captures: closure
                .captures
                .iter()
                .zip(&closure.body.captures)
                .map(|(value, ty)| to_neutral(set, value, &closure.env.apply(ty)))
                .collect::<Result<_, _>>()?,
        },
        (Type::Declared(id, arguments), _) => {
            declared_to_neutral(set, value, ty, id, arguments)?
        }
        _ => return Err(mismatch(ty)),
    })
}

fn declared_to_neutral(
    set: &CheckedModuleSet,
    value: &Val<'_>,
    ty: &Type,
    id: &vibra_ir::TypeId,
    arguments: &[Type],
) -> Result<Value, Halt> {
    match (set.library().library_of(id), &value.0) {
        (Some(Library::Bool), Repr::Bool(b)) => return Ok(Value::bool(*b)),
        (Some(Library::Str), Repr::Str(text)) => {
            return Ok(Value::Str(text.to_string()));
        }
        (Some(Library::Bytes), Repr::Bytes(bytes)) => {
            return Ok(Value::Bytes(bytes.to_vec()));
        }
        (Some(Library::Dict), Repr::Dict(dict)) => {
            let [key, item] = arguments else {
                return Err(mismatch(ty));
            };
            return Ok(Value::Dict(
                dict.entries
                    .iter()
                    .map(|(k, v)| {
                        Ok((to_neutral(set, k, key)?, to_neutral(set, v, item)?))
                    })
                    .collect::<Result<_, Halt>>()?,
            ));
        }
        (Some(Library::Option), Repr::Opt(inner)) => {
            let [item] = arguments else {
                return Err(mismatch(ty));
            };
            return Ok(match inner {
                None => Value::Variant {
                    case: "none".to_owned(),
                    fields: Vec::new(),
                },
                Some(inner) => Value::Variant {
                    case: "some".to_owned(),
                    fields: vec![("value".to_owned(), to_neutral(set, inner, item)?)],
                },
            });
        }
        (
            Some(
                Library::Bool
                | Library::Str
                | Library::Bytes
                | Library::Dict
                | Library::Option,
            ),
            _,
        ) => {
            return Err(mismatch(ty));
        }
        _ => {}
    }
    let definition = set.types().definition(id).ok_or_else(|| mismatch(ty))?;
    Ok(match (definition.body(), &value.0) {
        (TypeBody::Record(fields), Repr::Record(written)) => Value::Record(
            named_fields(set, &instantiated(fields, arguments), written, ty)?,
        ),
        (TypeBody::Tuple(types), Repr::Tuple(values))
            if types.len() == values.len() =>
        {
            Value::Tuple(
                values
                    .iter()
                    .zip(types)
                    .map(|(value, component)| {
                        to_neutral(set, value, &component.instantiate(arguments))
                    })
                    .collect::<Result<_, _>>()?,
            )
        }
        (TypeBody::Wrapper(representation), Repr::Wrapper(inner)) => {
            Value::Wrapper(Box::new(to_neutral(
                set,
                inner,
                &representation.instantiate(arguments),
            )?))
        }
        (TypeBody::Variant(_), Repr::Tag(case)) => {
            definition.case(case).ok_or_else(|| mismatch(ty))?;
            Value::Variant {
                case: (*case).to_owned(),
                fields: Vec::new(),
            }
        }
        (TypeBody::Variant(_), Repr::Case(case)) => {
            let declared = definition.case(case.case).ok_or_else(|| mismatch(ty))?;
            Value::Variant {
                case: case.case.to_owned(),
                fields: named_fields(
                    set,
                    &instantiated(&declared.fields, arguments),
                    &case.fields,
                    ty,
                )?,
            }
        }
        _ => return Err(mismatch(ty)),
    })
}

/// The runtime value of the neutral `value` at the closed type `ty`.
pub(crate) fn from_neutral<'a>(
    set: &CheckedModuleSet,
    value: &'a Value,
    ty: &Type,
) -> Result<Val<'a>, Halt> {
    Ok(Val(match (ty, value) {
        (Type::I8, Value::I8(n)) => Repr::I8(*n),
        (Type::I16, Value::I16(n)) => Repr::I16(*n),
        (Type::I32, Value::I32(n)) => Repr::I32(*n),
        (Type::I64, Value::I64(n)) => Repr::I64(*n),
        (Type::U8, Value::U8(n)) => Repr::U8(*n),
        (Type::U16, Value::U16(n)) => Repr::U16(*n),
        (Type::U32, Value::U32(n)) => Repr::U32(*n),
        (Type::U64, Value::U64(n)) => Repr::U64(*n),
        (Type::F32, Value::F32(bits)) => Repr::F32(*bits),
        (Type::F64, Value::F64(bits)) => Repr::F64(*bits),
        (Type::Char, Value::Char(c)) => Repr::Char(*c),
        (Type::Atom | Type::AtomSingleton(_), Value::Atom(atom)) => Repr::Atom(atom),
        (Type::Void, Value::Void) => Repr::Void,
        (Type::Any | Type::Interface(..), Value::Interface { held_type, held }) => {
            Repr::Interface(Rc::new(Held {
                held_type: held_type.clone(),
                value: from_neutral(set, held, held_type)?,
            }))
        }
        (Type::List(element), Value::List(items)) => Repr::List(Rc::new(
            items
                .iter()
                .map(|item| from_neutral(set, item, element))
                .collect::<Result<_, _>>()?,
        )),
        (Type::Tuple(types), Value::Tuple(values)) if types.len() == values.len() => {
            Repr::Tuple(Rc::new(
                values
                    .iter()
                    .zip(types)
                    .map(|(value, ty)| from_neutral(set, value, ty))
                    .collect::<Result<_, _>>()?,
            ))
        }
        (Type::Record(fields), Value::Record(written)) => {
            Repr::Record(Rc::new(neutral_fields(set, fields.entries(), written, ty)?))
        }
        (
            Type::Fn(_) | Type::Proc(_),
            Value::Function {
                function,
                type_arguments,
            },
        ) => Repr::Function(Rc::new(FunctionVal {
            function: *function,
            type_arguments: type_arguments.clone(),
        })),
        (Type::Declared(id, arguments), _) => {
            return declared_from_neutral(set, value, ty, id, arguments);
        }
        _ => return Err(mismatch(ty)),
    }))
}

fn neutral_fields<'a>(
    set: &CheckedModuleSet,
    declared: &[(String, Type)],
    written: &'a [(String, Value)],
    ty: &Type,
) -> Result<Vec<(&'a str, Val<'a>)>, Halt> {
    if declared.len() != written.len() {
        return Err(mismatch(ty));
    }
    written
        .iter()
        .map(|(name, value)| {
            let field_type = declared
                .iter()
                .find_map(|(field, field_type)| (field == name).then_some(field_type))
                .ok_or_else(|| mismatch(ty))?;
            Ok((name.as_str(), from_neutral(set, value, field_type)?))
        })
        .collect()
}

fn declared_from_neutral<'a>(
    set: &CheckedModuleSet,
    value: &'a Value,
    ty: &Type,
    id: &vibra_ir::TypeId,
    arguments: &[Type],
) -> Result<Val<'a>, Halt> {
    match (set.library().library_of(id), value) {
        (Some(Library::Bool), Value::Variant { case, fields }) if fields.is_empty() => {
            return match case.as_str() {
                "true" => Ok(Val::boolean(true)),
                "false" => Ok(Val::boolean(false)),
                _ => Err(mismatch(ty)),
            };
        }
        (Some(Library::Str), Value::Str(text)) => return Ok(Val::string(text)),
        (Some(Library::Bytes), Value::Bytes(bytes)) => {
            return Ok(Val::bytes(bytes.clone()));
        }
        (Some(Library::Dict), Value::Dict(entries)) => {
            let [key, item] = arguments else {
                return Err(mismatch(ty));
            };
            let entries = entries
                .iter()
                .map(|(k, v)| {
                    Ok((from_neutral(set, k, key)?, from_neutral(set, v, item)?))
                })
                .collect::<Result<_, Halt>>()?;
            return Ok(Val::dict(key.clone(), entries));
        }
        (Some(Library::Option), Value::Variant { case, fields }) => {
            let [item] = arguments else {
                return Err(mismatch(ty));
            };
            return match (case.as_str(), fields.as_slice()) {
                ("none", []) => Ok(Val::none()),
                ("some", [(name, inner)]) if name == "value" => {
                    Ok(Val::some(from_neutral(set, inner, item)?))
                }
                _ => Err(mismatch(ty)),
            };
        }
        (
            Some(
                Library::Bool
                | Library::Str
                | Library::Bytes
                | Library::Dict
                | Library::Option,
            ),
            _,
        ) => {
            return Err(mismatch(ty));
        }
        _ => {}
    }
    let definition = set.types().definition(id).ok_or_else(|| mismatch(ty))?;
    Ok(Val(match (definition.body(), value) {
        (TypeBody::Record(fields), Value::Record(written)) => Repr::Record(Rc::new(
            neutral_fields(set, &instantiated(fields, arguments), written, ty)?,
        )),
        (TypeBody::Tuple(types), Value::Tuple(values))
            if types.len() == values.len() =>
        {
            Repr::Tuple(Rc::new(
                values
                    .iter()
                    .zip(types)
                    .map(|(value, component)| {
                        from_neutral(set, value, &component.instantiate(arguments))
                    })
                    .collect::<Result<_, _>>()?,
            ))
        }
        (TypeBody::Wrapper(representation), Value::Wrapper(inner)) => {
            Repr::Wrapper(Rc::new(from_neutral(
                set,
                inner,
                &representation.instantiate(arguments),
            )?))
        }
        (TypeBody::Variant(_), Value::Variant { case, fields }) => {
            let declared = definition.case(case).ok_or_else(|| mismatch(ty))?;
            if fields.is_empty() && declared.fields.is_empty() {
                Repr::Tag(case)
            } else {
                Repr::Case(Rc::new(CaseVal {
                    case,
                    fields: neutral_fields(
                        set,
                        &instantiated(&declared.fields, arguments),
                        fields,
                        ty,
                    )?,
                }))
            }
        }
        _ => return Err(mismatch(ty)),
    }))
}

/// Why a value could not be observed.
#[derive(Debug)]
pub(crate) enum ObserveError {
    /// A function or closure behind an interface value: the observation
    /// error `@tool.unsupportedValue`.
    Unsupported,
    /// Anything else, which the checker rules out.
    Halt(Halt),
}

fn encoded(result: Result<String, EncodeError>) -> Result<String, ObserveError> {
    result.map_err(|error| match error {
        EncodeError::NoEncoding(_) => ObserveError::Unsupported,
        other => ObserveError::Halt(Halt::Invalid(other.to_string())),
    })
}

/// The neutral value of `value` at `ty`, and the canonical bytes of its
/// complete `@value.v2` observation.
pub(crate) fn observe(
    set: &CheckedModuleSet,
    value: &Val<'_>,
    ty: &Type,
) -> Result<(Value, String), ObserveError> {
    let neutral = to_neutral(set, value, ty).map_err(ObserveError::Halt)?;
    let document = encoded(value_document(&neutral, ty, set.types()))?;
    Ok((neutral, document))
}

/// The canonical value encoding of `value` at `ty`, which `assert.equal`
/// compares.
pub(crate) fn encoding(
    set: &CheckedModuleSet,
    value: &Val<'_>,
    ty: &Type,
) -> Result<String, ObserveError> {
    let neutral = to_neutral(set, value, ty).map_err(ObserveError::Halt)?;
    encoded(encode_value(&neutral, ty, set.types()))
}
