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

/// A child still to convert, at its closed type.
type Child<'v, V> = (&'v V, Rc<Type>);

/// What converting one node gives: a finished leaf, or a shape to build
/// once its children, in order, are converted.
enum Visited<'v, V, R, S> {
    Leaf(R),
    Node(S, Vec<Child<'v, V>>),
}

enum Work<'v, V, S> {
    Visit(&'v V, Rc<Type>),
    Build(S, usize),
}

/// Converts `root` at `ty` with an explicit work list, so that no host
/// stack grows with the depth of the value: `visit` takes one node apart
/// and `build` puts one together from its converted children.
fn convert<'v, V, R, S>(
    root: &'v V,
    ty: &Type,
    mut visit: impl FnMut(&'v V, &Type) -> Result<Visited<'v, V, R, S>, Halt>,
    mut build: impl FnMut(S, Vec<R>) -> Result<R, Halt>,
) -> Result<R, Halt> {
    let mut work = vec![Work::Visit(root, Rc::new(ty.clone()))];
    let mut done = Vec::new();
    while let Some(item) = work.pop() {
        match item {
            Work::Visit(value, ty) => match visit(value, &ty)? {
                Visited::Leaf(result) => done.push(result),
                Visited::Node(shape, children) => {
                    work.push(Work::Build(shape, children.len()));
                    work.extend(
                        children
                            .into_iter()
                            .rev()
                            .map(|(child, ty)| Work::Visit(child, ty)),
                    );
                }
            },
            Work::Build(shape, count) => {
                let at = done.len().checked_sub(count).ok_or_else(unbalanced)?;
                let parts = done.split_off(at);
                done.push(build(shape, parts)?);
            }
        }
    }
    match (done.pop(), done.is_empty()) {
        (Some(result), true) => Ok(result),
        _ => Err(unbalanced()),
    }
}

fn unbalanced() -> Halt {
    Halt::Invalid("an unbalanced value conversion".to_owned())
}

/// The one part of a node with one child.
fn single<R>(parts: Vec<R>) -> Result<R, Halt> {
    parts.into_iter().next().ok_or_else(unbalanced)
}

/// The parts of a dict, alternately keys and values, as entries.
fn entries<R>(parts: Vec<R>) -> Vec<(R, R)> {
    let mut parts = parts.into_iter();
    let mut entries = Vec::new();
    while let (Some(key), Some(value)) = (parts.next(), parts.next()) {
        entries.push((key, value));
    }
    entries
}

/// A node of a neutral value, before its children are converted.
enum NeutralShape {
    Interface(Type),
    List,
    Tuple,
    Record(Vec<String>),
    Variant(String, Vec<String>),
    Wrapper,
    Dict,
    Closure,
}

/// The written fields of a record or a case in declared order, each at its
/// declared type.
fn named_fields<'v, 'a>(
    declared: &[(String, Type)],
    written: &'v [(&'a str, Val<'a>)],
    ty: &Type,
) -> Result<(Vec<String>, Vec<Child<'v, Val<'a>>>), Halt> {
    if declared.len() != written.len() {
        return Err(mismatch(ty));
    }
    let mut names = Vec::with_capacity(declared.len());
    let mut children = Vec::with_capacity(declared.len());
    for (name, field_type) in declared {
        let value = written
            .iter()
            .find_map(|(field, value)| (*field == name.as_str()).then_some(value))
            .ok_or_else(|| mismatch(ty))?;
        names.push(name.clone());
        children.push((value, Rc::new(field_type.clone())));
    }
    Ok((names, children))
}

type ToNeutral<'v, 'a> = Visited<'v, Val<'a>, Value, NeutralShape>;

/// Takes apart one runtime node at the closed type `ty`.
fn to_visit<'v, 'a>(
    set: &CheckedModuleSet,
    value: &'v Val<'a>,
    ty: &Type,
) -> Result<ToNeutral<'v, 'a>, Halt> {
    use Visited::{Leaf, Node};
    Ok(match (ty, &value.0) {
        (Type::I8, Repr::I8(n)) => Leaf(Value::I8(*n)),
        (Type::I16, Repr::I16(n)) => Leaf(Value::I16(*n)),
        (Type::I32, Repr::I32(n)) => Leaf(Value::I32(*n)),
        (Type::I64, Repr::I64(n)) => Leaf(Value::I64(*n)),
        (Type::U8, Repr::U8(n)) => Leaf(Value::U8(*n)),
        (Type::U16, Repr::U16(n)) => Leaf(Value::U16(*n)),
        (Type::U32, Repr::U32(n)) => Leaf(Value::U32(*n)),
        (Type::U64, Repr::U64(n)) => Leaf(Value::U64(*n)),
        (Type::F32, Repr::F32(bits)) => Leaf(Value::F32(canonical_f32(*bits))),
        (Type::F64, Repr::F64(bits)) => Leaf(Value::F64(canonical_f64(*bits))),
        (Type::Char, Repr::Char(c)) => Leaf(Value::Char(*c)),
        (Type::Atom | Type::AtomSingleton(_), Repr::Atom(atom)) => {
            Leaf(Value::Atom((*atom).to_owned()))
        }
        (Type::Void, Repr::Void) => Leaf(Value::Void),
        (Type::Any | Type::Interface(..), Repr::Interface(held)) => Node(
            NeutralShape::Interface(held.held_type.clone()),
            vec![(&held.value, Rc::new(held.held_type.clone()))],
        ),
        (Type::List(element), Repr::List(items)) => {
            let element = Rc::new(Type::clone(element));
            Node(
                NeutralShape::List,
                items
                    .iter()
                    .map(|item| (item, Rc::clone(&element)))
                    .collect(),
            )
        }
        (Type::Tuple(types), Repr::Tuple(values)) if types.len() == values.len() => {
            Node(
                NeutralShape::Tuple,
                values
                    .iter()
                    .zip(types)
                    .map(|(value, ty)| (value, Rc::new(ty.clone())))
                    .collect(),
            )
        }
        (Type::Record(fields), Repr::Record(written)) => {
            let (names, children) = named_fields(fields.entries(), written, ty)?;
            Node(NeutralShape::Record(names), children)
        }
        (Type::Fn(_) | Type::Proc(_), Repr::Function(function)) => {
            Leaf(Value::Function {
                function: function.function,
                type_arguments: function.type_arguments.clone(),
            })
        }
        (Type::Fn(_) | Type::Proc(_), Repr::Closure(closure)) => Node(
            NeutralShape::Closure,
            closure
                .captures
                .iter()
                .zip(&closure.body.captures)
                .map(|(value, ty)| (value, Rc::new(closure.env.apply(ty))))
                .collect(),
        ),
        (Type::Declared(id, arguments), _) => {
            declared_to_visit(set, value, ty, id, arguments)?
        }
        _ => return Err(mismatch(ty)),
    })
}

fn declared_to_visit<'v, 'a>(
    set: &CheckedModuleSet,
    value: &'v Val<'a>,
    ty: &Type,
    id: &vibra_ir::TypeId,
    arguments: &[Type],
) -> Result<ToNeutral<'v, 'a>, Halt> {
    use Visited::{Leaf, Node};
    match (set.library().library_of(id), &value.0) {
        (Some(Library::Bool), Repr::Bool(b)) => return Ok(Leaf(Value::bool(*b))),
        (Some(Library::Str), Repr::Str(text)) => {
            return Ok(Leaf(Value::Str(text.to_string())));
        }
        (Some(Library::Bytes), Repr::Bytes(bytes)) => {
            return Ok(Leaf(Value::Bytes(bytes.to_vec())));
        }
        (Some(Library::Dict), Repr::Dict(dict)) => {
            let [key, item] = arguments else {
                return Err(mismatch(ty));
            };
            let (key, item) = (Rc::new(key.clone()), Rc::new(item.clone()));
            return Ok(Node(
                NeutralShape::Dict,
                dict.entries
                    .iter()
                    .flat_map(|(k, v)| [(k, Rc::clone(&key)), (v, Rc::clone(&item))])
                    .collect(),
            ));
        }
        (Some(Library::Option), Repr::Opt(inner)) => {
            let [item] = arguments else {
                return Err(mismatch(ty));
            };
            return Ok(match inner {
                None => Leaf(Value::Variant {
                    case: "none".to_owned(),
                    fields: Vec::new(),
                }),
                Some(inner) => Node(
                    NeutralShape::Variant("some".to_owned(), vec!["value".to_owned()]),
                    vec![(&**inner, Rc::new(item.clone()))],
                ),
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
        (TypeBody::Record(fields), Repr::Record(written)) => {
            let (names, children) =
                named_fields(&instantiated(fields, arguments), written, ty)?;
            Node(NeutralShape::Record(names), children)
        }
        (TypeBody::Tuple(types), Repr::Tuple(values))
            if types.len() == values.len() =>
        {
            Node(
                NeutralShape::Tuple,
                values
                    .iter()
                    .zip(types)
                    .map(|(value, component)| {
                        (value, Rc::new(component.instantiate(arguments)))
                    })
                    .collect(),
            )
        }
        (TypeBody::Wrapper(representation), Repr::Wrapper(inner)) => Node(
            NeutralShape::Wrapper,
            vec![(&**inner, Rc::new(representation.instantiate(arguments)))],
        ),
        (TypeBody::Variant(_), Repr::Tag(case)) => {
            definition.case(case).ok_or_else(|| mismatch(ty))?;
            Leaf(Value::Variant {
                case: (*case).to_owned(),
                fields: Vec::new(),
            })
        }
        (TypeBody::Variant(_), Repr::Case(case)) => {
            let declared = definition.case(case.case).ok_or_else(|| mismatch(ty))?;
            let (names, children) = named_fields(
                &instantiated(&declared.fields, arguments),
                &case.fields,
                ty,
            )?;
            Node(NeutralShape::Variant(case.case.to_owned(), names), children)
        }
        _ => return Err(mismatch(ty)),
    })
}

/// Puts one neutral node together from its converted children.
fn to_build(shape: NeutralShape, parts: Vec<Value>) -> Result<Value, Halt> {
    Ok(match shape {
        NeutralShape::Interface(held_type) => Value::Interface {
            held_type,
            held: Box::new(single(parts)?),
        },
        NeutralShape::List => Value::List(parts),
        NeutralShape::Tuple => Value::Tuple(parts),
        NeutralShape::Closure => Value::Closure { captures: parts },
        NeutralShape::Record(names) => {
            Value::Record(names.into_iter().zip(parts).collect())
        }
        NeutralShape::Variant(case, names) => Value::Variant {
            case,
            fields: names.into_iter().zip(parts).collect(),
        },
        NeutralShape::Wrapper => Value::Wrapper(Box::new(single(parts)?)),
        NeutralShape::Dict => Value::Dict(entries(parts)),
    })
}

/// The neutral value of `value` at the closed type `ty`.
pub(crate) fn to_neutral(
    set: &CheckedModuleSet,
    value: &Val<'_>,
    ty: &Type,
) -> Result<Value, Halt> {
    convert(value, ty, |value, ty| to_visit(set, value, ty), to_build)
}

/// A node of a runtime value, before its children are converted.
enum RuntimeShape<'a> {
    Interface(Type),
    List,
    Tuple,
    Record(Vec<&'a str>),
    Case(&'a str, Vec<&'a str>),
    Wrapper,
    Dict(Type),
    Some,
}

/// The written fields of a neutral record or case in written order, each
/// at its declared type.
fn neutral_fields<'a>(
    declared: &[(String, Type)],
    written: &'a [(String, Value)],
    ty: &Type,
) -> Result<(Vec<&'a str>, Vec<Child<'a, Value>>), Halt> {
    if declared.len() != written.len() {
        return Err(mismatch(ty));
    }
    let mut names = Vec::with_capacity(written.len());
    let mut children = Vec::with_capacity(written.len());
    for (name, value) in written {
        let field_type = declared
            .iter()
            .find_map(|(field, field_type)| (field == name).then_some(field_type))
            .ok_or_else(|| mismatch(ty))?;
        names.push(name.as_str());
        children.push((value, Rc::new(field_type.clone())));
    }
    Ok((names, children))
}

type FromNeutral<'a> = Visited<'a, Value, Val<'a>, RuntimeShape<'a>>;

/// Takes apart one neutral node at the closed type `ty`.
fn from_visit<'a>(
    set: &CheckedModuleSet,
    value: &'a Value,
    ty: &Type,
) -> Result<FromNeutral<'a>, Halt> {
    use Visited::{Leaf, Node};
    let leaf = |repr| Ok(Leaf(Val(repr)));
    match (ty, value) {
        (Type::I8, Value::I8(n)) => leaf(Repr::I8(*n)),
        (Type::I16, Value::I16(n)) => leaf(Repr::I16(*n)),
        (Type::I32, Value::I32(n)) => leaf(Repr::I32(*n)),
        (Type::I64, Value::I64(n)) => leaf(Repr::I64(*n)),
        (Type::U8, Value::U8(n)) => leaf(Repr::U8(*n)),
        (Type::U16, Value::U16(n)) => leaf(Repr::U16(*n)),
        (Type::U32, Value::U32(n)) => leaf(Repr::U32(*n)),
        (Type::U64, Value::U64(n)) => leaf(Repr::U64(*n)),
        (Type::F32, Value::F32(bits)) => leaf(Repr::F32(*bits)),
        (Type::F64, Value::F64(bits)) => leaf(Repr::F64(*bits)),
        (Type::Char, Value::Char(c)) => leaf(Repr::Char(*c)),
        (Type::Atom | Type::AtomSingleton(_), Value::Atom(atom)) => {
            leaf(Repr::Atom(atom))
        }
        (Type::Void, Value::Void) => leaf(Repr::Void),
        (Type::Any | Type::Interface(..), Value::Interface { held_type, held }) => {
            Ok(Node(
                RuntimeShape::Interface(held_type.clone()),
                vec![(&**held, Rc::new(held_type.clone()))],
            ))
        }
        (Type::List(element), Value::List(items)) => {
            let element = Rc::new(Type::clone(element));
            Ok(Node(
                RuntimeShape::List,
                items
                    .iter()
                    .map(|item| (item, Rc::clone(&element)))
                    .collect(),
            ))
        }
        (Type::Tuple(types), Value::Tuple(values)) if types.len() == values.len() => {
            Ok(Node(
                RuntimeShape::Tuple,
                values
                    .iter()
                    .zip(types)
                    .map(|(value, ty)| (value, Rc::new(ty.clone())))
                    .collect(),
            ))
        }
        (Type::Record(fields), Value::Record(written)) => {
            let (names, children) = neutral_fields(fields.entries(), written, ty)?;
            Ok(Node(RuntimeShape::Record(names), children))
        }
        (
            Type::Fn(_) | Type::Proc(_),
            Value::Function {
                function,
                type_arguments,
            },
        ) => leaf(Repr::Function(Rc::new(FunctionVal {
            function: *function,
            type_arguments: type_arguments.clone(),
        }))),
        (Type::Declared(id, arguments), _) => {
            declared_from_visit(set, value, ty, id, arguments)
        }
        _ => Err(mismatch(ty)),
    }
}

fn declared_from_visit<'a>(
    set: &CheckedModuleSet,
    value: &'a Value,
    ty: &Type,
    id: &vibra_ir::TypeId,
    arguments: &[Type],
) -> Result<FromNeutral<'a>, Halt> {
    use Visited::{Leaf, Node};
    match (set.library().library_of(id), value) {
        (Some(Library::Bool), Value::Variant { case, fields }) if fields.is_empty() => {
            return match case.as_str() {
                "true" => Ok(Leaf(Val::boolean(true))),
                "false" => Ok(Leaf(Val::boolean(false))),
                _ => Err(mismatch(ty)),
            };
        }
        (Some(Library::Str), Value::Str(text)) => return Ok(Leaf(Val::string(text))),
        (Some(Library::Bytes), Value::Bytes(bytes)) => {
            return Ok(Leaf(Val::bytes(bytes.clone())));
        }
        (Some(Library::Dict), Value::Dict(entries)) => {
            let [key, item] = arguments else {
                return Err(mismatch(ty));
            };
            let (key_type, item) = (Rc::new(key.clone()), Rc::new(item.clone()));
            return Ok(Node(
                RuntimeShape::Dict(key.clone()),
                entries
                    .iter()
                    .flat_map(|(k, v)| {
                        [(k, Rc::clone(&key_type)), (v, Rc::clone(&item))]
                    })
                    .collect(),
            ));
        }
        (Some(Library::Option), Value::Variant { case, fields }) => {
            let [item] = arguments else {
                return Err(mismatch(ty));
            };
            return match (case.as_str(), fields.as_slice()) {
                ("none", []) => Ok(Leaf(Val::none())),
                ("some", [(name, inner)]) if name == "value" => Ok(Node(
                    RuntimeShape::Some,
                    vec![(inner, Rc::new(item.clone()))],
                )),
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
    Ok(match (definition.body(), value) {
        (TypeBody::Record(fields), Value::Record(written)) => {
            let (names, children) =
                neutral_fields(&instantiated(fields, arguments), written, ty)?;
            Node(RuntimeShape::Record(names), children)
        }
        (TypeBody::Tuple(types), Value::Tuple(values))
            if types.len() == values.len() =>
        {
            Node(
                RuntimeShape::Tuple,
                values
                    .iter()
                    .zip(types)
                    .map(|(value, component)| {
                        (value, Rc::new(component.instantiate(arguments)))
                    })
                    .collect(),
            )
        }
        (TypeBody::Wrapper(representation), Value::Wrapper(inner)) => Node(
            RuntimeShape::Wrapper,
            vec![(&**inner, Rc::new(representation.instantiate(arguments)))],
        ),
        (TypeBody::Variant(_), Value::Variant { case, fields }) => {
            let declared = definition.case(case).ok_or_else(|| mismatch(ty))?;
            if fields.is_empty() && declared.fields.is_empty() {
                Leaf(Val(Repr::Tag(case)))
            } else {
                let (names, children) = neutral_fields(
                    &instantiated(&declared.fields, arguments),
                    fields,
                    ty,
                )?;
                Node(RuntimeShape::Case(case, names), children)
            }
        }
        _ => return Err(mismatch(ty)),
    })
}

/// Puts one runtime node together from its converted children.
fn from_build<'a>(
    shape: RuntimeShape<'a>,
    parts: Vec<Val<'a>>,
) -> Result<Val<'a>, Halt> {
    Ok(match shape {
        RuntimeShape::Interface(held_type) => Val(Repr::Interface(Rc::new(Held {
            held_type,
            value: single(parts)?,
        }))),
        RuntimeShape::List => Val(Repr::List(Rc::new(parts))),
        RuntimeShape::Tuple => Val(Repr::Tuple(Rc::new(parts))),
        RuntimeShape::Record(names) => Val(Repr::Record(Rc::new(
            names.into_iter().zip(parts).collect(),
        ))),
        RuntimeShape::Case(case, names) => Val(Repr::Case(Rc::new(CaseVal {
            case,
            fields: names.into_iter().zip(parts).collect(),
        }))),
        RuntimeShape::Wrapper => Val(Repr::Wrapper(Rc::new(single(parts)?))),
        RuntimeShape::Dict(key) => Val::dict(key, entries(parts)),
        RuntimeShape::Some => Val::some(single(parts)?),
    })
}

/// The runtime value of the neutral `value` at the closed type `ty`.
pub(crate) fn from_neutral<'a>(
    set: &CheckedModuleSet,
    value: &'a Value,
    ty: &Type,
) -> Result<Val<'a>, Halt> {
    convert(
        value,
        ty,
        |value, ty| from_visit(set, value, ty),
        from_build,
    )
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
