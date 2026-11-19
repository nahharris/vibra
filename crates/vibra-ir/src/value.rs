//! The neutral value model: the values a constant, an observation, and the
//! canonical value encoding talk about.
//!
//! A value carries no type of its own except where the runtime chapter's
//! encoding needs one that the static type does not give: the held type of
//! an interface value. An executor keeps its own representation, under the
//! runtime chapter's "Representation latitude", and converts to this model
//! only to observe a value.

use crate::encode::{EncodeError, value_document};
use crate::{Type, TypeTable};

/// A value.
///
/// Equality, cloning, debug formatting, and dropping walk the value with an
/// explicit work list, so that no host stack grows with its depth.
#[derive(Eq)]
pub enum Value {
    /// An `I8`.
    I8(i8),
    /// An `I16`.
    I16(i16),
    /// An `I32`.
    I32(i32),
    /// An `I64`.
    I64(i64),
    /// A `U8`.
    U8(u8),
    /// A `U16`.
    U16(u16),
    /// A `U32`.
    U32(u32),
    /// A `U64`.
    U64(u64),
    /// An `F32`, by its exact bits.
    F32(u32),
    /// An `F64`, by its exact bits.
    F64(u64),
    /// A `Char`.
    Char(char),
    /// An atom, without its `@`.
    Atom(String),
    /// `void`.
    Void,
    /// A `Str`: the value of the type playing `@str`.
    Str(String),
    /// A `Bytes`: the value of the type playing `@bytes`.
    Bytes(Vec<u8>),
    /// A tuple, structural or declared.
    Tuple(Vec<Self>),
    /// A `List`.
    List(Vec<Self>),
    /// A `Dict`, the value of the type playing `@dict`: its entries in the
    /// `Ordered` order of its keys, one per key.
    Dict(Vec<(Self, Self)>),
    /// A record, structural or declared, by field name.
    Record(Vec<(String, Self)>),
    /// A value of a variant type: its case and the case's fields.
    Variant {
        /// The case name.
        case: String,
        /// The case's fields, by name.
        fields: Vec<(String, Self)>,
    },
    /// A value of a wrapper type: its representation.
    Wrapper(Box<Self>),
    /// An interface value: the concrete value it holds and that value's
    /// closed type.
    Interface {
        /// The held value's type.
        held_type: Type,
        /// The held value.
        held: Box<Self>,
    },
    /// A module function as a value, at its type arguments.
    Function {
        /// The function's index in its module set.
        function: usize,
        /// The type arguments it was instantiated at.
        type_arguments: Vec<Type>,
    },
    /// A closure and its captured values.
    Closure {
        /// The captured values, by environment slot.
        captures: Vec<Self>,
    },
}

impl Value {
    /// The value `false` or `true` of a type playing `@bool`, whose cases
    /// are `false` and `true` with no fields.
    #[must_use]
    pub fn bool(value: bool) -> Self {
        Self::Variant {
            case: if value { "true" } else { "false" }.to_owned(),
            fields: Vec::new(),
        }
    }

    /// The `F32` of `value`.
    #[must_use]
    pub const fn f32(value: f32) -> Self {
        Self::F32(value.to_bits())
    }

    /// The `F64` of `value`.
    #[must_use]
    pub const fn f64(value: f64) -> Self {
        Self::F64(value.to_bits())
    }

    /// The compiler-owned scalar type of a scalar value, `None` for any
    /// other value.
    #[must_use]
    pub const fn scalar_type(&self) -> Option<Type> {
        Some(match self {
            Self::I8(_) => Type::I8,
            Self::I16(_) => Type::I16,
            Self::I32(_) => Type::I32,
            Self::I64(_) => Type::I64,
            Self::U8(_) => Type::U8,
            Self::U16(_) => Type::U16,
            Self::U32(_) => Type::U32,
            Self::U64(_) => Type::U64,
            Self::F32(_) => Type::F32,
            Self::F64(_) => Type::F64,
            Self::Char(_) => Type::Char,
            Self::Atom(_) => Type::Atom,
            Self::Void => Type::Void,
            _ => return None,
        })
    }

    /// Whether a function or a closure occurs in this value, at any depth,
    /// an interface value's held value included.
    #[must_use]
    pub fn holds_callable(&self) -> bool {
        let mut pending = vec![self];
        while let Some(value) = pending.pop() {
            match value {
                Self::Function { .. } | Self::Closure { .. } => return true,
                other => pending.extend(other.children()),
            }
        }
        false
    }

    /// The values directly inside this one.
    fn children(&self) -> Vec<&Self> {
        match self {
            Self::Tuple(values)
            | Self::List(values)
            | Self::Closure { captures: values } => values.iter().collect(),
            Self::Dict(entries) => entries
                .iter()
                .flat_map(|(key, value)| [key, value])
                .collect(),
            Self::Record(fields) | Self::Variant { fields, .. } => {
                fields.iter().map(|(_, value)| value).collect()
            }
            Self::Wrapper(value) | Self::Interface { held: value, .. } => vec![value],
            _ => Vec::new(),
        }
    }

    /// Whether this value and `other` agree on everything but their
    /// children, whose pairs are added to `pending`.
    fn same_node<'v>(
        &'v self,
        other: &'v Self,
        pending: &mut Vec<(&'v Self, &'v Self)>,
    ) -> bool {
        match (self, other) {
            (Self::I8(left), Self::I8(right)) => left == right,
            (Self::I16(left), Self::I16(right)) => left == right,
            (Self::I32(left), Self::I32(right)) => left == right,
            (Self::I64(left), Self::I64(right)) => left == right,
            (Self::U8(left), Self::U8(right)) => left == right,
            (Self::U16(left), Self::U16(right)) => left == right,
            (Self::U32(left), Self::U32(right))
            | (Self::F32(left), Self::F32(right)) => left == right,
            (Self::U64(left), Self::U64(right))
            | (Self::F64(left), Self::F64(right)) => left == right,
            (Self::Char(left), Self::Char(right)) => left == right,
            (Self::Atom(left), Self::Atom(right))
            | (Self::Str(left), Self::Str(right)) => left == right,
            (Self::Void, Self::Void) => true,
            (Self::Bytes(left), Self::Bytes(right)) => left == right,
            (Self::Tuple(left), Self::Tuple(right))
            | (Self::List(left), Self::List(right))
            | (Self::Closure { captures: left }, Self::Closure { captures: right }) => {
                left.len() == right.len() && {
                    pending.extend(left.iter().zip(right));
                    true
                }
            }
            (Self::Dict(left), Self::Dict(right)) => {
                left.len() == right.len() && {
                    pending.extend(left.iter().zip(right).flat_map(
                        |((left_key, left_value), (right_key, right_value))| {
                            [(left_key, right_key), (left_value, right_value)]
                        },
                    ));
                    true
                }
            }
            (Self::Record(left), Self::Record(right)) => {
                same_fields(left, right, pending)
            }
            (
                Self::Variant {
                    case: left_case,
                    fields: left,
                },
                Self::Variant {
                    case: right_case,
                    fields: right,
                },
            ) => left_case == right_case && same_fields(left, right, pending),
            (Self::Wrapper(left), Self::Wrapper(right)) => {
                pending.push((left, right));
                true
            }
            (
                Self::Interface {
                    held_type: left_type,
                    held: left,
                },
                Self::Interface {
                    held_type: right_type,
                    held: right,
                },
            ) => {
                left_type == right_type && {
                    pending.push((left, right));
                    true
                }
            }
            (
                Self::Function {
                    function: left,
                    type_arguments: left_arguments,
                },
                Self::Function {
                    function: right,
                    type_arguments: right_arguments,
                },
            ) => left == right && left_arguments == right_arguments,
            _ => false,
        }
    }

    /// Moves the values directly inside this one to `pending`.
    fn take_children(&mut self, pending: &mut Vec<Self>) {
        match self {
            Self::Tuple(values)
            | Self::List(values)
            | Self::Closure { captures: values } => {
                pending.append(values);
            }
            Self::Dict(entries) => {
                pending.extend(entries.drain(..).flat_map(|(key, value)| [key, value]));
            }
            Self::Record(fields) | Self::Variant { fields, .. } => {
                pending.extend(fields.drain(..).map(|(_, value)| value));
            }
            Self::Wrapper(value) | Self::Interface { held: value, .. } => {
                pending.push(std::mem::replace(&mut **value, Self::Void));
            }
            _ => {}
        }
    }
}

fn same_fields<'v>(
    left: &'v [(String, Value)],
    right: &'v [(String, Value)],
    pending: &mut Vec<(&'v Value, &'v Value)>,
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|((left_name, _), (right_name, _))| left_name == right_name)
        && {
            pending.extend(
                left.iter()
                    .zip(right)
                    .map(|((_, left_value), (_, right_value))| {
                        (left_value, right_value)
                    }),
            );
            true
        }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        let mut pending = vec![(self, other)];
        while let Some((left, right)) = pending.pop() {
            if !left.same_node(right, &mut pending) {
                return false;
            }
        }
        true
    }
}

impl Value {
    /// This value rebuilt from its own non-value parts and `children`, the
    /// copies of its children in [`Self::children`] order.
    fn rebuilt(&self, children: Vec<Self>) -> Self {
        let mut children = children.into_iter();
        let mut fields = |fields: &[(String, Self)]| {
            fields
                .iter()
                .map(|(name, _)| (name.clone(), children.next().unwrap_or(Self::Void)))
                .collect::<Vec<_>>()
        };
        match self {
            Self::I8(n) => Self::I8(*n),
            Self::I16(n) => Self::I16(*n),
            Self::I32(n) => Self::I32(*n),
            Self::I64(n) => Self::I64(*n),
            Self::U8(n) => Self::U8(*n),
            Self::U16(n) => Self::U16(*n),
            Self::U32(n) => Self::U32(*n),
            Self::U64(n) => Self::U64(*n),
            Self::F32(bits) => Self::F32(*bits),
            Self::F64(bits) => Self::F64(*bits),
            Self::Char(c) => Self::Char(*c),
            Self::Atom(atom) => Self::Atom(atom.clone()),
            Self::Void => Self::Void,
            Self::Str(text) => Self::Str(text.clone()),
            Self::Bytes(bytes) => Self::Bytes(bytes.clone()),
            Self::Tuple(_) => Self::Tuple(children.collect()),
            Self::List(_) => Self::List(children.collect()),
            Self::Closure { .. } => Self::Closure {
                captures: children.collect(),
            },
            Self::Dict(entries) => Self::Dict(
                entries
                    .iter()
                    .map(|_| {
                        let key = children.next().unwrap_or(Self::Void);
                        (key, children.next().unwrap_or(Self::Void))
                    })
                    .collect(),
            ),
            Self::Record(written) => Self::Record(fields(written)),
            Self::Variant {
                case,
                fields: written,
            } => Self::Variant {
                case: case.clone(),
                fields: fields(written),
            },
            Self::Wrapper(_) => {
                Self::Wrapper(Box::new(children.next().unwrap_or(Self::Void)))
            }
            Self::Interface { held_type, .. } => Self::Interface {
                held_type: held_type.clone(),
                held: Box::new(children.next().unwrap_or(Self::Void)),
            },
            Self::Function {
                function,
                type_arguments,
            } => Self::Function {
                function: *function,
                type_arguments: type_arguments.clone(),
            },
        }
    }
}

impl Clone for Value {
    fn clone(&self) -> Self {
        enum Task<'v> {
            Enter(&'v Value),
            Build(&'v Value, usize),
        }
        let mut tasks = vec![Task::Enter(self)];
        let mut built: Vec<Self> = Vec::new();
        while let Some(task) = tasks.pop() {
            match task {
                Task::Enter(value) => {
                    let children = value.children();
                    tasks.push(Task::Build(value, children.len()));
                    tasks.extend(children.into_iter().rev().map(Task::Enter));
                }
                Task::Build(value, count) => {
                    let children = built.split_off(built.len().saturating_sub(count));
                    built.push(value.rebuilt(children));
                }
            }
        }
        built.pop().unwrap_or(Self::Void)
    }
}

/// The derived debug form, `{:#?}` included: `Tuple([I8(1), Void])`,
/// `Variant { case: "some", fields: [("value", U8(2))] }`.
impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        enum Piece<'v> {
            Text(&'static str),
            Name(&'v str),
            Value(&'v Value),
        }
        fn list<'v>(
            pieces: &mut Vec<Piece<'v>>,
            open: &'static str,
            items: impl IntoIterator<Item = Vec<Piece<'v>>>,
            close: &'static str,
        ) {
            let mut out = vec![Piece::Text(open)];
            for (index, item) in items.into_iter().enumerate() {
                if index > 0 {
                    out.push(Piece::Text(", "));
                }
                out.extend(item);
            }
            out.push(Piece::Text(close));
            pieces.extend(out.into_iter().rev());
        }
        fn named<'v>(fields: &'v [(String, Value)]) -> Vec<Vec<Piece<'v>>> {
            fields
                .iter()
                .map(|(name, value)| {
                    vec![
                        Piece::Text("("),
                        Piece::Name(name),
                        Piece::Text(", "),
                        Piece::Value(value),
                        Piece::Text(")"),
                    ]
                })
                .collect()
        }
        let mut pieces = vec![Piece::Value(self)];
        while let Some(piece) = pieces.pop() {
            let value = match piece {
                Piece::Text(text) => {
                    f.write_str(text)?;
                    continue;
                }
                Piece::Name(name) => {
                    write!(f, "{name:?}")?;
                    continue;
                }
                Piece::Value(value) => value,
            };
            match value {
                Self::I8(n) => write!(f, "I8({n:?})")?,
                Self::I16(n) => write!(f, "I16({n:?})")?,
                Self::I32(n) => write!(f, "I32({n:?})")?,
                Self::I64(n) => write!(f, "I64({n:?})")?,
                Self::U8(n) => write!(f, "U8({n:?})")?,
                Self::U16(n) => write!(f, "U16({n:?})")?,
                Self::U32(n) => write!(f, "U32({n:?})")?,
                Self::U64(n) => write!(f, "U64({n:?})")?,
                Self::F32(bits) => write!(f, "F32({bits:?})")?,
                Self::F64(bits) => write!(f, "F64({bits:?})")?,
                Self::Char(c) => write!(f, "Char({c:?})")?,
                Self::Atom(atom) => write!(f, "Atom({atom:?})")?,
                Self::Void => f.write_str("Void")?,
                Self::Str(text) => write!(f, "Str({text:?})")?,
                Self::Bytes(bytes) => write!(f, "Bytes({bytes:?})")?,
                Self::Tuple(values) => list(
                    &mut pieces,
                    "Tuple([",
                    values.iter().map(|value| vec![Piece::Value(value)]),
                    "])",
                ),
                Self::List(values) => list(
                    &mut pieces,
                    "List([",
                    values.iter().map(|value| vec![Piece::Value(value)]),
                    "])",
                ),
                Self::Closure { captures } => list(
                    &mut pieces,
                    "Closure { captures: [",
                    captures.iter().map(|value| vec![Piece::Value(value)]),
                    "] }",
                ),
                Self::Dict(entries) => list(
                    &mut pieces,
                    "Dict([",
                    entries.iter().map(|(key, value)| {
                        vec![
                            Piece::Text("("),
                            Piece::Value(key),
                            Piece::Text(", "),
                            Piece::Value(value),
                            Piece::Text(")"),
                        ]
                    }),
                    "])",
                ),
                Self::Record(fields) => {
                    list(&mut pieces, "Record([", named(fields), "])")
                }
                Self::Variant { case, fields } => {
                    write!(f, "Variant {{ case: {case:?}, ")?;
                    list(&mut pieces, "fields: [", named(fields), "] }");
                }
                Self::Wrapper(inner) => {
                    list(&mut pieces, "Wrapper(", [vec![Piece::Value(inner)]], ")");
                }
                Self::Interface { held_type, held } => {
                    write!(f, "Interface {{ held_type: {held_type:?}, ")?;
                    list(&mut pieces, "held: ", [vec![Piece::Value(held)]], " }");
                }
                Self::Function {
                    function,
                    type_arguments,
                } => write!(
                    f,
                    "Function {{ function: {function:?}, type_arguments: {type_arguments:?} }}"
                )?,
            }
        }
        Ok(())
    }
}

impl Drop for Value {
    fn drop(&mut self) {
        let mut pending = Vec::new();
        self.take_children(&mut pending);
        while let Some(mut value) = pending.pop() {
            value.take_children(&mut pending);
        }
    }
}

/// A value observed at its closed type: what the canonical value encoding
/// writes as one `@value.v2` document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedValue {
    value_type: Type,
    value: Value,
}

impl ObservedValue {
    /// The observation of `value` at `value_type`.
    #[must_use]
    pub const fn new(value_type: Type, value: Value) -> Self {
        Self { value_type, value }
    }

    /// The observed type.
    #[must_use]
    pub const fn value_type(&self) -> &Type {
        &self.value_type
    }

    /// The observed value.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// The canonical bytes of the `@value.v2` document of this observation.
    ///
    /// # Errors
    ///
    /// The value has no encoding, or it does not fit its type.
    pub fn encode(&self, types: &TypeTable) -> Result<String, EncodeError> {
        value_document(&self.value, &self.value_type, types)
    }
}
