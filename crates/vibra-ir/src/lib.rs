//! The small, backend independent IR admitted by M2 Step 7.
//!
//! The type checker is the only workspace phase that constructs a
//! [`CheckedProgram`].  The interpreter consumes that type, rather than a
//! parsed syntax tree, which makes the checked-program boundary explicit.
//! This IR slice contains primitive values, immutable bindings, literal
//! sequences, conditionals, first-class function paths, owned closures, and
//! fixed/labelled calls; effects and collections belong to later steps.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use vibra_diagnostics::ByteSpan;

mod nominal;
mod observed;
mod pattern;

pub use nominal::{TypeBody, TypeDefinition, TypeId, canonical_members};
use nominal::{
    type_table, validate_declared_expr, validate_declared_signature,
    validate_declared_type,
};
pub use observed::ObservedValue;
pub use pattern::{MatchArm, Pattern};

pub mod external;

/// One of the primitive types admitted by the M2 literal profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    /// Boolean values.
    Bool,
    /// The single successful-completion value.
    Void,
    /// The uninhabited type of an expression that never completes.
    Never,
    /// Unicode scalar values.
    Char,
    /// Immutable Unicode scalar strings.
    Str,
    /// Immutable byte sequences.
    Bytes,
    /// Interned atom values.
    Atom,
    /// The singleton type of one written atom, which widens to `atom`.
    AtomSingleton(String),
    /// Signed eight-bit integers.
    I8,
    /// Signed sixteen-bit integers.
    I16,
    /// Signed thirty-two-bit integers.
    I32,
    /// Signed sixty-four-bit integers.
    I64,
    /// Unsigned eight-bit integers.
    U8,
    /// Unsigned sixteen-bit integers.
    U16,
    /// Unsigned thirty-two-bit integers.
    U32,
    /// Unsigned sixty-four-bit integers.
    U64,
    /// IEEE 754 binary32 values.
    F32,
    /// IEEE 754 binary64 values.
    F64,
    /// A first-class monomorphic function value.
    Function(Box<FunctionSignature>),
    /// A declared (`deftype`) type, identified by its declaration.
    Declared(TypeId),
    /// An anonymous record type; fields are in canonical order.
    Record(Vec<(String, Type)>),
    /// An anonymous enum type; variants are in canonical order.
    Enum(Vec<(String, Type)>),
    /// A generic parameter of the enclosing declaration, rigid inside it.
    Param(String),
    /// A generic declared type applied to its complete argument list.
    Applied(TypeId, Vec<Type>),
    /// An anonymous tuple type; components are positional.
    Tuple(Vec<Type>),
    /// The builtin `(array t)` type.
    Array(Box<Type>),
    /// The builtin `(dict k v)` type.
    Dict(Box<Type>, Box<Type>),
    /// An anonymous union type; members are in canonical order.
    Union(Vec<Type>),
    /// An interface value: a value of some type that implements the declared
    /// interface applied to these arguments, reached by widening.
    Interface(TypeId, Vec<Type>),
    /// A value of the predeclared empty interface `any`, which every type
    /// satisfies and which can only be passed along.
    Any,
}

impl Type {
    /// The source spelling of a primitive or function type head.
    ///
    /// Declared and structural types have no single head word; they spell
    /// as `type`, `record`, and `enum`, and [`fmt::Display`] renders them in
    /// full.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Declared(_) | Self::Param(_) | Self::Applied(_, _) => "type",
            Self::Record(_) => "record",
            Self::Enum(_) => "enum",
            Self::Tuple(_) => "tuple",
            Self::Union(_) => "union",
            Self::Array(_) => "array",
            Self::Dict(_, _) => "dict",
            Self::Bool => "bool",
            Self::Void => "void",
            Self::Never => "never",
            Self::Char => "char",
            Self::Str => "str",
            Self::Bytes => "bytes",
            Self::Atom => "atom",
            Self::AtomSingleton(_) => "atom",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Function(_) => "fn",
            Self::Interface(_, _) => "interface",
            Self::Any => "any",
        }
    }

    /// Whether two types have the same semantic shape.
    ///
    /// Function defaults are call-site metadata rather than part of the
    /// function type, so this comparison deliberately delegates to
    /// [`FunctionSignature::same_shape`] for function values.
    #[must_use]
    pub fn same_shape(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Function(left), Self::Function(right)) => left.same_shape(right),
            (Self::Record(left), Self::Record(right))
            | (Self::Enum(left), Self::Enum(right)) => members_same_shape(left, right),
            (Self::Tuple(left), Self::Tuple(right))
            | (Self::Union(left), Self::Union(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|(left, right)| left.same_shape(right))
            }
            (Self::Array(left), Self::Array(right)) => left.same_shape(right),
            (Self::Dict(left_key, left_value), Self::Dict(right_key, right_value)) => {
                left_key.same_shape(right_key) && left_value.same_shape(right_value)
            }
            (
                Self::Applied(left, left_arguments),
                Self::Applied(right, right_arguments),
            ) => {
                left == right
                    && left_arguments.len() == right_arguments.len()
                    && left_arguments
                        .iter()
                        .zip(right_arguments)
                        .all(|(left, right)| left.same_shape(right))
            }
            _ => self == other,
        }
    }

    /// This type with every generic parameter named in `arguments` replaced.
    #[must_use]
    pub fn substitute(&self, arguments: &BTreeMap<String, Type>) -> Self {
        match self {
            Self::Param(name) => {
                arguments.get(name).cloned().unwrap_or_else(|| self.clone())
            }
            Self::Applied(id, values) => Self::Applied(
                id.clone(),
                values
                    .iter()
                    .map(|value| value.substitute(arguments))
                    .collect(),
            ),
            Self::Interface(id, values) => Self::Interface(
                id.clone(),
                values
                    .iter()
                    .map(|value| value.substitute(arguments))
                    .collect(),
            ),
            Self::Record(members) => {
                Self::Record(substitute_members(members, arguments))
            }
            Self::Enum(members) => Self::Enum(substitute_members(members, arguments)),
            Self::Union(values) => Self::Union(canonical_union(
                values
                    .iter()
                    .map(|value| value.substitute(arguments))
                    .collect(),
            )),
            Self::Tuple(values) => Self::Tuple(
                values
                    .iter()
                    .map(|value| value.substitute(arguments))
                    .collect(),
            ),
            Self::Array(element) => {
                Self::Array(Box::new(element.substitute(arguments)))
            }
            Self::Dict(key, value) => Self::Dict(
                Box::new(key.substitute(arguments)),
                Box::new(value.substitute(arguments)),
            ),
            Self::Function(signature) => {
                Self::Function(Box::new(signature.substitute(arguments)))
            }
            _ => self.clone(),
        }
    }

    /// Whether a value of static type `self` may hold a runtime value whose
    /// recorded type is `actual`. Generic parameters are erased at run time, so
    /// a parameter on either side matches any type; an interface value erases
    /// the type it holds, so an interface type admits any type, at any depth.
    #[must_use]
    pub fn admits(&self, actual: &Self) -> bool {
        match (self, actual) {
            (Self::Param(_), _) | (_, Self::Param(_)) | (_, Self::Never) => true,
            (Self::Interface(_, _) | Self::Any, _) => true,
            (
                Self::Applied(left, left_arguments),
                Self::Applied(right, right_arguments),
            ) => {
                left == right
                    && left_arguments.len() == right_arguments.len()
                    && left_arguments
                        .iter()
                        .zip(right_arguments)
                        .all(|(left, right)| left.admits(right))
            }
            (Self::Record(left), Self::Record(right))
            | (Self::Enum(left), Self::Enum(right)) => {
                left.len() == right.len()
                    && left.iter().zip(right).all(|(left, right)| {
                        left.0 == right.0 && left.1.admits(&right.1)
                    })
            }
            (Self::Tuple(left), Self::Tuple(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|(left, right)| left.admits(right))
            }
            (Self::Array(left), Self::Array(right)) => left.admits(right),
            (Self::Dict(left_key, left_value), Self::Dict(right_key, right_value)) => {
                left_key.admits(right_key) && left_value.admits(right_value)
            }
            (Self::Function(left), Self::Function(right)) => left.admits(right),
            _ => self.same_shape(actual),
        }
    }

    /// The types this type is directly built from: applied and collection
    /// arguments, tuple components, record fields, enum payloads, and a
    /// function type's parameters and result. A primitive, declared type, or
    /// generic parameter has none.
    #[must_use]
    pub fn components(&self) -> Vec<Self> {
        match self {
            Self::Applied(_, values)
            | Self::Interface(_, values)
            | Self::Tuple(values)
            | Self::Union(values) => values.clone(),
            Self::Array(element) => vec![element.as_ref().clone()],
            Self::Dict(key, value) => {
                vec![key.as_ref().clone(), value.as_ref().clone()]
            }
            Self::Record(members) | Self::Enum(members) => {
                members.iter().map(|(_, value)| value.clone()).collect()
            }
            Self::Function(signature) => {
                let mut values = signature.slot_types();
                values.push(signature.result());
                values
            }
            _ => Vec::new(),
        }
    }

    /// Whether any generic parameter occurs in this type.
    #[must_use]
    pub fn has_params(&self) -> bool {
        match self {
            Self::Param(_) => true,
            Self::Applied(_, values) | Self::Interface(_, values) => {
                values.iter().any(Self::has_params)
            }
            Self::Tuple(values) | Self::Union(values) => {
                values.iter().any(Self::has_params)
            }
            Self::Array(element) => element.has_params(),
            Self::Dict(key, value) => key.has_params() || value.has_params(),
            Self::Record(members) | Self::Enum(members) => {
                members.iter().any(|(_, value)| value.has_params())
            }
            Self::Function(signature) => signature.has_params(),
            _ => false,
        }
    }

    /// Whether this type is one of the fixed-width integer types.
    #[must_use]
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
                | Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
        )
    }

    /// Whether this type is a floating-point type.
    #[must_use]
    pub fn is_float(&self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Declared(id) => formatter.write_str(id.path()),
            Self::Param(name) => formatter.write_str(name),
            Self::Interface(id, arguments) if arguments.is_empty() => {
                formatter.write_str(id.path())
            }
            Self::Applied(id, arguments) | Self::Interface(id, arguments) => {
                write!(formatter, "({}", id.path())?;
                for argument in arguments {
                    write!(formatter, " {argument}")?;
                }
                formatter.write_str(")")
            }
            Self::Record(members) | Self::Enum(members) => {
                formatter.write_str(if matches!(self, Self::Record(_)) {
                    "(record"
                } else {
                    "(enum"
                })?;
                for (name, value) in members {
                    write!(formatter, " {name} {value}")?;
                }
                formatter.write_str(")")
            }
            Self::Tuple(values) => {
                formatter.write_str("(tuple")?;
                for value in values {
                    write!(formatter, " {value}")?;
                }
                formatter.write_str(")")
            }
            Self::Union(values) => {
                formatter.write_str("(union")?;
                for value in values {
                    write!(formatter, " {value}")?;
                }
                formatter.write_str(")")
            }
            Self::AtomSingleton(name) => write!(formatter, "@{name}"),
            Self::Array(element) => write!(formatter, "(array {element})"),
            Self::Dict(key, value) => write!(formatter, "(dict {key} {value})"),
            // Spelled like the `fn` type expression, so two function types
            // in a diagnostic are told apart by their parameters and result.
            Self::Function(signature) => {
                formatter.write_str("(fn (")?;
                for (index, parameter) in signature.parameters().iter().enumerate() {
                    if index != 0 {
                        formatter.write_str(" ")?;
                    }
                    write!(formatter, "{parameter}")?;
                }
                write!(formatter, ") {}", signature.result())?;
                if !signature.labelled().is_empty() {
                    formatter.write_str(" labelled: (")?;
                    for (index, parameter) in signature.labelled().iter().enumerate() {
                        if index != 0 {
                            formatter.write_str(" ")?;
                        }
                        write!(
                            formatter,
                            "{} {}",
                            parameter.name(),
                            parameter.value_type()
                        )?;
                    }
                    formatter.write_str(")")?;
                }
                if let Some(tail) = signature.variadic() {
                    write!(formatter, " variadic: {tail}")?;
                }
                formatter.write_str(")")
            }
            _ => formatter.write_str(self.as_str()),
        }
    }
}

/// One labelled slot in a function value's call contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelledParameter {
    name: String,
    value_type: Type,
    default: Option<Value>,
}

impl LabelledParameter {
    /// Creates a labelled slot.  Function type expressions use `None`; a
    /// declaration signature carries the typed literal default.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        value_type: Type,
        default: Option<Value>,
    ) -> Self {
        Self {
            name: name.into(),
            value_type,
            default,
        }
    }

    /// The source-level label without its trailing colon.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The labelled slot type.
    #[must_use]
    pub fn value_type(&self) -> Type {
        self.value_type.clone()
    }

    /// The declaration default, when this is a callable declaration value.
    #[must_use]
    pub const fn default(&self) -> Option<&Value> {
        self.default.as_ref()
    }
}

/// One fully checked monomorphic function signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionSignature {
    parameters: Vec<Type>,
    labelled: Vec<LabelledParameter>,
    /// The `(array t)` or `(dict k v)` type of a variadic tail. A call passes
    /// the packed tail as one final argument after the labelled slots.
    variadic: Option<Type>,
    result: Type,
}

impl FunctionSignature {
    /// Creates a signature from its checked primitive slots.
    #[must_use]
    pub fn new(parameters: Vec<Type>, result: Type) -> Self {
        Self {
            parameters,
            labelled: Vec::new(),
            variadic: None,
            result,
        }
    }

    /// Creates a signature with declaration-order labelled slots.
    #[must_use]
    pub fn with_labelled(
        parameters: Vec<Type>,
        labelled: Vec<LabelledParameter>,
        result: Type,
    ) -> Self {
        Self {
            parameters,
            labelled,
            variadic: None,
            result,
        }
    }

    /// The same signature with a variadic tail of `tail` type, an
    /// `(array t)` or a `(dict k v)`.
    #[must_use]
    pub fn with_variadic(mut self, tail: Type) -> Self {
        self.variadic = Some(tail);
        self
    }

    /// Required positional parameter types in written order.
    #[must_use]
    pub fn parameters(&self) -> &[Type] {
        &self.parameters
    }

    /// Labelled slots in declaration order.
    #[must_use]
    pub fn labelled(&self) -> &[LabelledParameter] {
        &self.labelled
    }

    /// The variadic tail type, when the signature has one.
    #[must_use]
    pub const fn variadic(&self) -> Option<&Type> {
        self.variadic.as_ref()
    }

    /// Total argument slots after defaults have been materialized and a
    /// variadic tail packed: positional, labelled, then the tail.
    #[must_use]
    pub fn fixed_parameter_count(&self) -> usize {
        self.parameters
            .len()
            .saturating_add(self.labelled.len())
            .saturating_add(usize::from(self.variadic.is_some()))
    }

    /// The type of every argument slot, in slot order.
    #[must_use]
    pub fn slot_types(&self) -> Vec<Type> {
        self.parameters
            .iter()
            .cloned()
            .chain(self.labelled.iter().map(LabelledParameter::value_type))
            .chain(self.variadic.iter().cloned())
            .collect()
    }

    /// Whether two signatures have the same callable type shape.
    ///
    /// Defaults belong to a value and are deliberately excluded from the
    /// function type comparison.
    #[must_use]
    pub fn same_shape(&self, other: &Self) -> bool {
        self.parameters.len() == other.parameters.len()
            && self
                .parameters
                .iter()
                .zip(&other.parameters)
                .all(|(left, right)| left.same_shape(right))
            && self.result.same_shape(&other.result)
            && self.labelled.len() == other.labelled.len()
            && self
                .labelled
                .iter()
                .zip(&other.labelled)
                .all(|(left, right)| {
                    left.name == right.name
                        && left.value_type.same_shape(&right.value_type)
                })
            && match (&self.variadic, &other.variadic) {
                (Some(left), Some(right)) => left.same_shape(right),
                (None, None) => true,
                _ => false,
            }
    }

    /// This signature with its generic parameters replaced; defaults are kept.
    #[must_use]
    pub fn substitute(&self, arguments: &BTreeMap<String, Type>) -> Self {
        Self {
            parameters: self
                .parameters
                .iter()
                .map(|value| value.substitute(arguments))
                .collect(),
            labelled: self
                .labelled
                .iter()
                .map(|parameter| LabelledParameter {
                    name: parameter.name.clone(),
                    value_type: parameter.value_type.substitute(arguments),
                    default: parameter.default.clone(),
                })
                .collect(),
            variadic: self
                .variadic
                .as_ref()
                .map(|tail| tail.substitute(arguments)),
            result: self.result.substitute(arguments),
        }
    }

    /// [`Type::admits`] extended over every slot of two signatures.
    #[must_use]
    pub fn admits(&self, other: &Self) -> bool {
        self.parameters.len() == other.parameters.len()
            && self
                .parameters
                .iter()
                .zip(&other.parameters)
                .all(|(left, right)| left.admits(right))
            && self.result.admits(&other.result)
            && self.labelled.len() == other.labelled.len()
            && self
                .labelled
                .iter()
                .zip(&other.labelled)
                .all(|(left, right)| {
                    left.name == right.name && left.value_type.admits(&right.value_type)
                })
            && match (&self.variadic, &other.variadic) {
                (Some(left), Some(right)) => left.admits(right),
                (None, None) => true,
                _ => false,
            }
    }

    /// Whether any slot mentions a generic parameter.
    #[must_use]
    pub fn has_params(&self) -> bool {
        self.slot_types().iter().any(Type::has_params) || self.result.has_params()
    }

    /// The declared result type.
    #[must_use]
    pub fn result(&self) -> Type {
        self.result.clone()
    }
}

/// One closed verified assertion exported only by `@std.assert` for tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TestAssertion {
    /// Require one boolean operand to be `true`.
    True,
    /// Require one boolean operand to be `false`.
    False,
    /// Require two operands of one type to have the same canonical value
    /// encoding.
    Equal,
}

impl TestAssertion {
    /// Every test assertion in canonical order.
    pub const ALL: [Self; 3] = [Self::True, Self::False, Self::Equal];

    /// The assertion member without its `@std.assert.` prefix.
    #[must_use]
    pub const fn member(self) -> &'static str {
        match self {
            Self::True => "true",
            Self::False => "false",
            Self::Equal => "equal",
        }
    }

    /// Canonical source-level assertion identity.
    #[must_use]
    pub fn symbol(self) -> String {
        format!("@std.assert.{}", self.member())
    }

    /// Resolves only a member in the closed assertion table.
    #[must_use]
    pub fn from_member(member: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|assertion| assertion.member() == member)
    }

    /// The generic parameters of the exact signature, in `where:` order.
    #[must_use]
    pub fn type_parameters(self) -> Vec<String> {
        match self {
            Self::True | Self::False => Vec::new(),
            Self::Equal => vec!["t".to_owned()],
        }
    }

    /// The exact signature: `(expected t) (actual t) -> void` for `equal`.
    #[must_use]
    pub fn signature(self) -> FunctionSignature {
        match self {
            Self::True | Self::False => {
                FunctionSignature::new(vec![Type::Bool], Type::Void)
            }
            Self::Equal => {
                let value = Type::Param("t".to_owned());
                FunctionSignature::new(vec![value.clone(), value], Type::Void)
            }
        }
    }
}

/// A source identity and span carried by checked operands.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceOrigin {
    /// Shared so that cloning an origin, which every checked node carries,
    /// never copies the source ID.
    source_id: Arc<str>,
    span: ByteSpan,
}

impl SourceOrigin {
    /// Creates an origin for one source document span.
    #[must_use]
    pub fn new(source_id: impl Into<Arc<str>>, span: ByteSpan) -> Self {
        Self {
            source_id: source_id.into(),
            span,
        }
    }

    /// The source document identity.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The half-open source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// A primitive value after lexical decoding and exact type checking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A boolean value.
    Bool(bool),
    /// The only void value.
    Void,
    /// A Unicode scalar.
    Char(char),
    /// An immutable string.
    Str(String),
    /// An immutable byte sequence.
    Bytes(Vec<u8>),
    /// An atom without its leading `@` marker.
    Atom(String),
    /// A signed eight-bit integer.
    I8(i8),
    /// A signed sixteen-bit integer.
    I16(i16),
    /// A signed thirty-two-bit integer.
    I32(i32),
    /// A signed sixty-four-bit integer.
    I64(i64),
    /// An unsigned eight-bit integer.
    U8(u8),
    /// An unsigned sixteen-bit integer.
    U16(u16),
    /// An unsigned thirty-two-bit integer.
    U32(u32),
    /// An unsigned sixty-four-bit integer.
    U64(u64),
    /// A binary32 value represented by its exact bits.
    F32(u32),
    /// A binary64 value represented by its exact bits.
    F64(u64),
}

impl Value {
    /// Returns the primitive type carried by this value.
    #[must_use]
    pub fn ty(&self) -> Type {
        match self {
            Self::Bool(_) => Type::Bool,
            Self::Void => Type::Void,
            Self::Char(_) => Type::Char,
            Self::Str(_) => Type::Str,
            Self::Bytes(_) => Type::Bytes,
            Self::Atom(name) => Type::AtomSingleton(name.clone()),
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
        }
    }

    /// Creates a binary32 value from its finite IEEE bits.
    #[must_use]
    pub fn f32(value: f32) -> Option<Self> {
        value.is_finite().then_some(Self::F32(value.to_bits()))
    }

    /// Creates a binary64 value from its finite IEEE bits.
    #[must_use]
    pub fn f64(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self::F64(value.to_bits()))
    }

    /// Reads a binary32 value from its exact bits.
    #[must_use]
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Self::F32(bits) => Some(f32::from_bits(*bits)),
            _ => None,
        }
    }

    /// Reads a binary64 value from its exact bits.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::F64(bits) => Some(f64::from_bits(*bits)),
            _ => None,
        }
    }

    /// The deterministic literal spelling used by result observations.
    #[must_use]
    pub fn canonical_vibon(&self) -> String {
        match self {
            Self::Bool(value) => value.to_string(),
            Self::Void => "void".to_owned(),
            Self::Char(value) => canonical_character(*value),
            Self::Str(value) => quote(value),
            Self::Bytes(value) => {
                let bytes = value
                    .iter()
                    .map(|byte| format!("{byte}u8"))
                    .collect::<Vec<_>>();
                format!("(record kind: @bytes values: {})", canonical_array(&bytes))
            }
            Self::Atom(value) => format!("@{value}"),
            Self::I8(value) => format!("{value}i8"),
            Self::I16(value) => format!("{value}i16"),
            Self::I32(value) => format!("{value}i32"),
            Self::I64(value) => format!("{value}i64"),
            Self::U8(value) => format!("{value}u8"),
            Self::U16(value) => format!("{value}u16"),
            Self::U32(value) => format!("{value}u32"),
            Self::U64(value) => format!("{value}u64"),
            Self::F32(bits) => {
                format!("{}f32", canonical_f32_text(f32::from_bits(*bits)))
            }
            Self::F64(bits) => {
                format!("{}f64", canonical_f64_text(f64::from_bits(*bits)))
            }
        }
    }

    /// A typed result observation suitable for a conformance snapshot.
    #[must_use]
    pub fn canonical_observation(&self) -> String {
        format!(
            "(record type: @{} value: {})\n",
            self.ty().as_str(),
            self.canonical_vibon()
        )
    }
}

/// One executable expression in the checked primitive/binding subset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    /// A typed primitive literal.
    Literal {
        /// The value to return or discard.
        value: Value,
        /// The source origin of the literal.
        origin: SourceOrigin,
    },
    /// A call to one admitted pure compiler intrinsic.
    External {
        /// The closed compiler registry operation.
        intrinsic: external::CompilerIntrinsic,
        /// Checked operands in the registry's declaration order.
        arguments: Vec<Self>,
        /// The checked result type, with any language role bound to the
        /// standard-library type that plays it.
        result: Type,
        /// The source origin of the intrinsic call.
        origin: SourceOrigin,
    },
    /// A labelled argument whose default is resolved from the runtime callee.
    Default {
        /// The statically checked labelled slot type.
        value_type: Type,
        /// The source origin of the omitted argument.
        origin: SourceOrigin,
    },
    /// A strict left-to-right sequence.
    Sequence {
        /// Expressions in evaluation order.
        expressions: Vec<Self>,
        /// The source origin of the sequence form.
        origin: SourceOrigin,
    },
    /// A value stored in the current function activation.
    Variable {
        /// The immutable activation slot.
        slot: usize,
        /// The statically checked value type.
        value_type: Type,
        /// The source origin of the name use.
        origin: SourceOrigin,
    },
    /// A module-level immutable value.
    Global {
        /// The program-global index.
        index: usize,
        /// The statically checked value type.
        value_type: Type,
        /// The source origin of the name use.
        origin: SourceOrigin,
    },
    /// A module-level function path reified as a function value.
    Function {
        /// The function index in the containing checked program.
        function: usize,
        /// The resolved callable signature.
        signature: FunctionSignature,
        /// The source origin of the path.
        origin: SourceOrigin,
    },
    /// A lambda with an immutable, owned closure environment.
    Closure {
        /// The lambda's checked signature.
        signature: FunctionSignature,
        /// Lambda parameters/body, checked in a separate lexical activation.
        parameters: Vec<Type>,
        /// Expressions evaluated once when the closure is created.
        captures: Vec<Self>,
        /// Static types of the closure-environment slots.
        capture_types: Vec<Type>,
        /// The lambda body, whose free names use [`Self::Captured`].
        ///
        /// Shared so that creating a closure value, or summarizing one during
        /// call-flow analysis, never copies the body tree.
        body: Arc<Self>,
        /// Activation slots needed by the lambda body.
        slot_count: usize,
        /// The source origin of the lambda form.
        origin: SourceOrigin,
    },
    /// A value captured into a closure environment.
    Captured {
        /// The closure-environment slot.
        slot: usize,
        /// The statically checked value type.
        value_type: Type,
        /// The source origin of the captured name use.
        origin: SourceOrigin,
    },
    /// An immutable binding followed by its body.
    Let {
        /// The slot receiving the checked initializer, or `None` for a discard.
        slot: Option<usize>,
        /// The initializer.
        value: Box<Self>,
        /// The body sequence after the binding.
        body: Box<Self>,
        /// The source origin of the complete form.
        origin: SourceOrigin,
    },
    /// A `match`: the scrutinee is evaluated once and the first arm whose
    /// pattern matches it is selected.
    Match {
        /// The scrutinee.
        scrutinee: Box<Self>,
        /// Arms in source order.
        arms: Vec<MatchArm>,
        /// The common result type of every arm.
        value_type: Type,
        /// The source origin of the complete form.
        origin: SourceOrigin,
    },
    /// One widening at a written expected type: an atom singleton to `atom`,
    /// which is erased, or a member value to a union, which attaches the
    /// member's discriminant.
    Widen {
        /// The widened value.
        value: Box<Self>,
        /// The written target type.
        value_type: Type,
        /// The discriminant, the member's index in the union's member
        /// list; `None` for atom widening.
        member: Option<usize>,
        /// The source origin of the widened operand.
        origin: SourceOrigin,
    },
    /// `try`: the success payload of an `option` or `result` operand, or an
    /// early exit from the innermost function or `lambda` with its absence
    /// or error rebuilt at that function's result type.
    Try {
        /// The `option` or `result` operand, never in tail position.
        value: Box<Self>,
        /// The success payload type.
        value_type: Type,
        /// The enclosing function's result type, which an early exit returns.
        exit_type: Type,
        /// The source origin of the `try` form.
        origin: SourceOrigin,
    },
    /// `return`: leaves the innermost enclosing function activation with the
    /// operand's value. It has type `never`.
    Return {
        /// The returned value, in tail position of the activation it exits.
        value: Box<Self>,
        /// The source origin of the `return` form.
        origin: SourceOrigin,
    },
    /// A boolean conditional with two already checked branches.
    If {
        /// The boolean condition.
        condition: Box<Self>,
        /// The branch selected for `true`.
        then_branch: Box<Self>,
        /// The branch selected for `false`.
        else_branch: Box<Self>,
        /// The source origin of the complete form.
        origin: SourceOrigin,
    },
    /// A fixed positional call.
    Call {
        /// What the call invokes.
        target: CallTarget,
        /// Arguments in declaration order.
        arguments: Vec<Self>,
        /// The statically checked result type.
        result: Type,
        /// Whether this call is an explicit tail transfer.
        ///
        /// The checker sets this for a call in an activation-relative tail
        /// position of a function or `lambda` body when it can reach source
        /// code: a statically known source function (not a compiler
        /// intrinsic wrapper), a closure, or a target set that is not
        /// statically bounded. Checked-program validation accepts the marker
        /// only in a tail position. At run time a transfer reuses the current
        /// activation whatever the evaluated callee is; only a callee that
        /// creates no language activation, such as an intrinsic wrapper, is
        /// invoked as an ordinary call.
        tail: bool,
        /// The source origin of the complete application.
        origin: SourceOrigin,
    },
    /// A declared record constructor or an anonymous `recordof`.
    Record {
        /// The declared or anonymous record type being built.
        value_type: Type,
        /// Field operands in evaluation order: declaration order for a
        /// declared constructor, written order for `recordof`.
        fields: Vec<(String, Self)>,
        /// The source origin of the complete form.
        origin: SourceOrigin,
    },
    /// An enum variant constructor or an anonymous `enumof`.
    Variant {
        /// The declared or anonymous enum type being built.
        value_type: Type,
        /// The selected variant.
        variant: String,
        /// The payload operand; `None` for a `void` payload slot.
        payload: Option<Box<Self>>,
        /// The source origin of the complete form.
        origin: SourceOrigin,
    },
    /// A wrapper-type constructor.
    Wrap {
        /// The declared or applied wrapper type.
        value_type: Type,
        /// The representation operand.
        value: Box<Self>,
        /// The source origin of the complete form.
        origin: SourceOrigin,
    },
    /// A record projection with a compile-time field selector.
    Project {
        /// The record operand, evaluated once.
        record: Box<Self>,
        /// The selected field.
        field: String,
        /// The statically checked field type.
        value_type: Type,
        /// The source origin of the complete application.
        origin: SourceOrigin,
    },
    /// A declared tuple constructor or an anonymous `tupleof`.
    Tuple {
        /// The declared or anonymous tuple type being built.
        value_type: Type,
        /// Component operands in order.
        components: Vec<Self>,
        /// The source origin of the complete form.
        origin: SourceOrigin,
    },
    /// A tuple value applied to one tuple-index literal.
    TupleProject {
        /// The tuple operand, evaluated once.
        tuple: Box<Self>,
        /// The checked component index.
        index: usize,
        /// The statically checked component type.
        value_type: Type,
        /// The source origin of the complete application.
        origin: SourceOrigin,
    },
    /// An array built from element operands, such as a variadic array tail.
    Array {
        /// The `(array t)` type being built.
        value_type: Type,
        /// Element operands in order.
        elements: Vec<Self>,
        /// The source origin of the elements.
        origin: SourceOrigin,
    },
    /// A dict built from key and value operands, such as a variadic dict tail.
    /// A later entry replaces an earlier entry with an equal key.
    Dict {
        /// The `(dict k v)` type being built.
        value_type: Type,
        /// Key and value operands, evaluated key then value, in order.
        entries: Vec<(Self, Self)>,
        /// The `ordered` interface whose implementations order the keys, when
        /// the key type is not closed; canonical key order otherwise.
        key_order: Option<TypeId>,
        /// The source origin of the entries.
        origin: SourceOrigin,
    },
    /// An array, dict, `str`, or `bytes` value applied to one key, returning
    /// the standard `option`.
    Lookup {
        /// The collection operand, evaluated first.
        collection: Box<Self>,
        /// The index or key operand.
        key: Box<Self>,
        /// For a dict, the `ordered` interface whose implementations order its
        /// keys, when the key type is not closed.
        key_order: Option<TypeId>,
        /// The `(option t)` result type.
        value_type: Type,
        /// The source origin of the complete application.
        origin: SourceOrigin,
    },
}

/// What a checked call invokes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallTarget {
    /// A module-level function of the containing program, by index.
    Direct(usize),
    /// A function value computed by evaluating `callee` exactly once.
    Indirect {
        /// The callee expression, of function type.
        callee: Box<Expr>,
        /// The one module function the callee is statically known to denote,
        /// when it denotes exactly one and no closure. Validation re-derives
        /// the target set from `callee` and rejects a hint that disagrees.
        hint: Option<usize>,
    },
    /// A contract member dispatched at run time from the runtime type of the
    /// operand at `receiver`: the one function whose [`Implements`] names
    /// this interface and member and admits that type.
    Contract {
        /// The interface identity.
        interface: TypeId,
        /// The contract member name.
        member: String,
        /// The operand that selects the implementation.
        receiver: usize,
        /// The interface's type arguments at this call, for a generic
        /// interface: one receiver may implement it at several.
        arguments: Vec<Type>,
        /// For a member selected by its destination, the type that selects
        /// the implementation in place of the operand at `receiver`: a
        /// generic parameter bounded by the interface, instantiated at run
        /// time.
        destination: Option<Type>,
        /// The member's signature at this call.
        signature: Box<FunctionSignature>,
        /// The toolchain conformance the call falls back to when no
        /// implementation covers the receiver.s runtime type.
        closed: Option<ClosedContract>,
    },
}

/// A closed toolchain conformance of a standard contract: the `ordered` and
/// `equatable` members of the closed key types
/// (`docs/spec/02-type-system.md`, "Nominal declarations"), answered by
/// canonical key order, and `iter.next` of the builtin constructor types
/// ("Closed builtin conformance").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClosedContract {
    /// `ordered.compare`.
    KeyCompare,
    /// `equatable.equal`.
    KeyEqual,
    /// `iter.next` of an `(array t)`, `(dict k v)`, `str`, or `(option t)`.
    IterNext,
}

impl ClosedContract {
    /// The canonical spelling of the conformance.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::KeyCompare => "key.compare",
            Self::KeyEqual => "key.equal",
            Self::IterNext => "iter.next",
        }
    }
}

impl CallTarget {
    /// The indirect callee expression, if this is an indirect call.
    #[must_use]
    pub fn callee(&self) -> Option<&Expr> {
        match self {
            Self::Direct(_) | Self::Contract { .. } => None,
            Self::Indirect { callee, .. } => Some(callee),
        }
    }

    /// The statically known function hint of an indirect call.
    #[must_use]
    pub const fn hint(&self) -> Option<usize> {
        match self {
            Self::Direct(_) | Self::Contract { .. } => None,
            Self::Indirect { hint, .. } => *hint,
        }
    }
}

impl Expr {
    /// Creates a checked literal expression.
    #[must_use]
    pub fn literal(value: Value, origin: SourceOrigin) -> Self {
        Self::Literal { value, origin }
    }

    /// Creates a checked compiler intrinsic call.
    #[must_use]
    pub fn external(
        intrinsic: external::CompilerIntrinsic,
        arguments: Vec<Self>,
        origin: SourceOrigin,
    ) -> Self {
        let result = intrinsic
            .signature(&external::RoleTypes::default())
            .result();
        Self::external_with_result(intrinsic, arguments, result, origin)
    }

    /// Creates a registry call whose checked result binds language roles.
    #[must_use]
    pub fn external_with_result(
        intrinsic: external::CompilerIntrinsic,
        arguments: Vec<Self>,
        result: Type,
        origin: SourceOrigin,
    ) -> Self {
        Self::External {
            intrinsic,
            arguments,
            result,
            origin,
        }
    }

    /// Creates an omitted labelled argument resolved by the selected callable.
    #[must_use]
    pub fn default_value(value_type: Type, origin: SourceOrigin) -> Self {
        Self::Default { value_type, origin }
    }

    /// Creates a checked activation-slot reference.
    #[must_use]
    pub fn variable(slot: usize, value_type: Type, origin: SourceOrigin) -> Self {
        Self::Variable {
            slot,
            value_type,
            origin,
        }
    }

    /// Creates a checked module-global reference.
    #[must_use]
    pub fn global(index: usize, value_type: Type, origin: SourceOrigin) -> Self {
        Self::Global {
            index,
            value_type,
            origin,
        }
    }

    /// Creates a first-class module function value.
    #[must_use]
    pub fn function(
        function: usize,
        signature: FunctionSignature,
        origin: SourceOrigin,
    ) -> Self {
        Self::Function {
            function,
            signature,
            origin,
        }
    }

    /// Creates a closure with explicit capture expressions and body slots.
    #[must_use]
    pub fn closure(
        signature: FunctionSignature,
        parameters: Vec<Type>,
        captures: Vec<Self>,
        body: Self,
        slot_count: usize,
        origin: SourceOrigin,
    ) -> Self {
        let capture_types = captures.iter().map(Self::result_type).collect::<Vec<_>>();
        Self::Closure {
            signature,
            parameters,
            captures,
            capture_types,
            body: Arc::new(body),
            slot_count,
            origin,
        }
    }

    /// Creates a closure-environment reference.
    #[must_use]
    pub fn captured(slot: usize, value_type: Type, origin: SourceOrigin) -> Self {
        Self::Captured {
            slot,
            value_type,
            origin,
        }
    }

    /// Creates a checked immutable binding.
    #[must_use]
    pub fn let_binding(
        slot: Option<usize>,
        value: Self,
        body: Self,
        origin: SourceOrigin,
    ) -> Self {
        Self::Let {
            slot,
            value: Box::new(value),
            body: Box::new(body),
            origin,
        }
    }

    /// Creates a checked conditional.
    #[must_use]
    pub fn if_expression(
        condition: Self,
        then_branch: Self,
        else_branch: Self,
        origin: SourceOrigin,
    ) -> Self {
        Self::If {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
            origin,
        }
    }

    /// Creates a checked fixed positional call.
    #[must_use]
    pub fn call(
        function: usize,
        arguments: Vec<Self>,
        result: Type,
        origin: SourceOrigin,
    ) -> Self {
        Self::Call {
            target: CallTarget::Direct(function),
            arguments,
            result,
            tail: false,
            origin,
        }
    }

    /// Creates a direct tail transfer to a checked function.
    #[must_use]
    pub fn tail_call(
        function: usize,
        arguments: Vec<Self>,
        result: Type,
        origin: SourceOrigin,
    ) -> Self {
        Self::Call {
            target: CallTarget::Direct(function),
            arguments,
            result,
            tail: true,
            origin,
        }
    }

    /// Creates an indirect call whose callee is evaluated exactly once.
    #[must_use]
    pub fn indirect_call(
        callee: Self,
        function_hint: Option<usize>,
        arguments: Vec<Self>,
        result: Type,
        origin: SourceOrigin,
    ) -> Self {
        Self::Call {
            target: CallTarget::Indirect {
                callee: Box::new(callee),
                hint: function_hint,
            },
            arguments,
            result,
            tail: false,
            origin,
        }
    }

    /// Creates an indirect tail transfer with a statically known target.
    #[must_use]
    pub fn indirect_tail_call(
        callee: Self,
        function_hint: usize,
        arguments: Vec<Self>,
        result: Type,
        origin: SourceOrigin,
    ) -> Self {
        Self::indirect_tail_call_with_hint(
            callee,
            Some(function_hint),
            arguments,
            result,
            origin,
        )
    }

    /// Creates an indirect tail transfer whose statically bounded target set
    /// may contain more than one module function.  The optional hint is only
    /// a compact singleton observation; checked-program validation derives
    /// the actual target set from the callee expression.
    #[must_use]
    pub fn indirect_tail_call_with_hint(
        callee: Self,
        function_hint: Option<usize>,
        arguments: Vec<Self>,
        result: Type,
        origin: SourceOrigin,
    ) -> Self {
        Self::Call {
            target: CallTarget::Indirect {
                callee: Box::new(callee),
                hint: function_hint,
            },
            arguments,
            result,
            tail: true,
            origin,
        }
    }

    /// The same expression as an ordinary call: a tail transfer whose result
    /// the caller still has to use, such as a call it widens, is not one.
    #[must_use]
    pub fn without_tail(mut self) -> Self {
        if let Self::Call { tail, .. } = &mut self {
            *tail = false;
        }
        self
    }

    /// The same call marked as a tail transfer: it is the final result of its
    /// activation, which it reuses.
    #[must_use]
    pub fn with_tail(mut self) -> Self {
        if let Self::Call { tail, .. } = &mut self {
            *tail = true;
        }
        self
    }

    /// Reports whether this expression is an explicit checked tail transfer.
    #[must_use]
    pub const fn is_tail_call(&self) -> bool {
        matches!(self, Self::Call { tail: true, .. })
    }

    /// Creates a checked sequence expression.
    #[must_use]
    pub fn sequence(expressions: Vec<Self>, origin: SourceOrigin) -> Self {
        Self::Sequence {
            expressions,
            origin,
        }
    }

    /// The source origin of this expression.
    #[must_use]
    pub const fn origin(&self) -> &SourceOrigin {
        match self {
            Self::Literal { origin, .. }
            | Self::External { origin, .. }
            | Self::Default { origin, .. }
            | Self::Sequence { origin, .. }
            | Self::Variable { origin, .. }
            | Self::Global { origin, .. }
            | Self::Function { origin, .. }
            | Self::Closure { origin, .. }
            | Self::Captured { origin, .. }
            | Self::Let { origin, .. }
            | Self::Match { origin, .. }
            | Self::If { origin, .. }
            | Self::Call { origin, .. }
            | Self::Record { origin, .. }
            | Self::Variant { origin, .. }
            | Self::Wrap { origin, .. }
            | Self::Widen { origin, .. }
            | Self::Try { origin, .. }
            | Self::Return { origin, .. }
            | Self::Project { origin, .. }
            | Self::Tuple { origin, .. }
            | Self::TupleProject { origin, .. }
            | Self::Array { origin, .. }
            | Self::Dict { origin, .. }
            | Self::Lookup { origin, .. } => origin,
        }
    }

    /// The statically known result type of this expression.
    #[must_use]
    pub fn result_type(&self) -> Type {
        match self {
            Self::Literal { value, .. } => value.ty(),
            Self::External { result, .. } => result.clone(),
            Self::Default { value_type, .. } => value_type.clone(),
            Self::Sequence { expressions, .. } => {
                expressions.last().map_or(Type::Void, Self::result_type)
            }
            Self::Variable { value_type, .. } | Self::Global { value_type, .. } => {
                value_type.clone()
            }
            Self::Function { signature, .. } | Self::Closure { signature, .. } => {
                Type::Function(Box::new(signature.clone()))
            }
            Self::Captured { value_type, .. } => value_type.clone(),
            Self::Let { body, .. } => body.result_type(),
            Self::Match { value_type, .. } => value_type.clone(),
            Self::If {
                then_branch,
                else_branch,
                ..
            } => match then_branch.result_type() {
                Type::Never => else_branch.result_type(),
                value_type => value_type,
            },
            Self::Return { .. } => Type::Never,
            Self::Call { result, .. } => result.clone(),
            Self::Record { value_type, .. }
            | Self::Variant { value_type, .. }
            | Self::Project { value_type, .. } => value_type.clone(),
            Self::Wrap { value_type, .. }
            | Self::Widen { value_type, .. }
            | Self::Try { value_type, .. }
            | Self::Tuple { value_type, .. }
            | Self::TupleProject { value_type, .. }
            | Self::Array { value_type, .. }
            | Self::Dict { value_type, .. }
            | Self::Lookup { value_type, .. } => value_type.clone(),
        }
    }

    /// The operands of a construction or projection, in evaluation order.
    /// Every other expression kind returns an empty list.
    #[must_use]
    pub fn data_operands(&self) -> Vec<&Self> {
        match self {
            Self::Record { fields, .. } => {
                fields.iter().map(|(_, value)| value).collect()
            }
            Self::Variant { payload, .. } => {
                payload.iter().map(|payload| &**payload).collect()
            }
            Self::Wrap { value, .. }
            | Self::Try { value, .. }
            | Self::Return { value, .. }
            | Self::Widen { value, .. } => vec![value],
            Self::Project { record, .. } => vec![record],
            Self::Tuple { components, .. } => components.iter().collect(),
            Self::TupleProject { tuple, .. } => vec![tuple],
            Self::Array { elements, .. } => elements.iter().collect(),
            Self::Dict { entries, .. } => entries
                .iter()
                .flat_map(|(key, value)| [key, value])
                .collect(),
            Self::Lookup {
                collection, key, ..
            } => vec![collection, key],
            _ => Vec::new(),
        }
    }

    /// The sequence members, or an empty slice for a literal.
    #[must_use]
    pub fn expressions(&self) -> &[Self] {
        match self {
            Self::Literal { .. }
            | Self::External { .. }
            | Self::Default { .. }
            | Self::Variable { .. }
            | Self::Global { .. }
            | Self::Function { .. }
            | Self::Closure { .. }
            | Self::Captured { .. }
            | Self::Let { .. }
            | Self::Match { .. }
            | Self::If { .. }
            | Self::Call { .. }
            | Self::Record { .. }
            | Self::Variant { .. }
            | Self::Wrap { .. }
            | Self::Widen { .. }
            | Self::Try { .. }
            | Self::Return { .. }
            | Self::Project { .. }
            | Self::Tuple { .. }
            | Self::TupleProject { .. }
            | Self::Array { .. }
            | Self::Dict { .. }
            | Self::Lookup { .. } => &[],
            Self::Sequence { expressions, .. } => expressions,
        }
    }

    /// The literal value, when this is a literal expression.
    #[must_use]
    pub const fn literal_value(&self) -> Option<&Value> {
        match self {
            Self::Literal { value, .. } => Some(value),
            Self::Default { .. }
            | Self::External { .. }
            | Self::Sequence { .. }
            | Self::Variable { .. }
            | Self::Global { .. }
            | Self::Function { .. }
            | Self::Closure { .. }
            | Self::Captured { .. }
            | Self::Let { .. }
            | Self::Match { .. }
            | Self::If { .. }
            | Self::Call { .. }
            | Self::Record { .. }
            | Self::Variant { .. }
            | Self::Wrap { .. }
            | Self::Widen { .. }
            | Self::Try { .. }
            | Self::Return { .. }
            | Self::Project { .. }
            | Self::Tuple { .. }
            | Self::TupleProject { .. }
            | Self::Array { .. }
            | Self::Dict { .. }
            | Self::Lookup { .. } => None,
        }
    }

    /// Number of immutable activation slots required by this expression.
    #[must_use]
    pub fn slot_count(&self) -> usize {
        match self {
            Self::Literal { .. }
            | Self::Default { .. }
            | Self::Global { .. }
            | Self::Function { .. }
            | Self::Captured { .. } => 0,
            Self::External { arguments, .. } => {
                arguments.iter().map(Self::slot_count).max().unwrap_or(0)
            }
            Self::Sequence { expressions, .. } => {
                expressions.iter().map(Self::slot_count).max().unwrap_or(0)
            }
            Self::Variable { slot, .. } => slot.saturating_add(1),
            Self::Let {
                slot, value, body, ..
            } => value
                .slot_count()
                .max(body.slot_count())
                .max(slot.map_or(0, |slot| slot.saturating_add(1))),
            Self::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => condition
                .slot_count()
                .max(then_branch.slot_count())
                .max(else_branch.slot_count()),
            Self::Match {
                scrutinee, arms, ..
            } => arms.iter().fold(scrutinee.slot_count(), |count, arm| {
                arm.pattern
                    .bindings()
                    .iter()
                    .map(|(slot, _)| slot.saturating_add(1))
                    .fold(count.max(arm.body.slot_count()), usize::max)
            }),
            Self::Call {
                arguments, target, ..
            } => target
                .callee()
                .map_or(0, Self::slot_count)
                .max(arguments.iter().map(Self::slot_count).max().unwrap_or(0)),
            Self::Closure { captures, .. } => {
                captures.iter().map(Self::slot_count).max().unwrap_or(0)
            }
            Self::Record { fields, .. } => fields
                .iter()
                .map(|(_, value)| value.slot_count())
                .max()
                .unwrap_or(0),
            Self::Variant { payload, .. } => {
                payload.as_deref().map_or(0, Self::slot_count)
            }
            Self::Wrap { value, .. }
            | Self::Widen { value, .. }
            | Self::Return { value, .. }
            | Self::Try { value, .. } => value.slot_count(),
            Self::Project { record, .. } => record.slot_count(),
            Self::Tuple { .. }
            | Self::TupleProject { .. }
            | Self::Array { .. }
            | Self::Dict { .. }
            | Self::Lookup { .. } => self
                .data_operands()
                .into_iter()
                .map(Self::slot_count)
                .max()
                .unwrap_or(0),
        }
    }

    fn validate_shape(&self, slots: &mut [Option<Type>]) -> Result<Type, IrError> {
        self.validate_shape_with_captures(slots, &[])
    }

    fn validate_shape_with_captures(
        &self,
        slots: &mut [Option<Type>],
        capture_types: &[Type],
    ) -> Result<Type, IrError> {
        match self {
            Self::Literal { value, .. } => Ok(value.ty()),
            Self::External {
                intrinsic,
                arguments,
                result,
                ..
            } => {
                // Registry operands never name a role; only the result may.
                let signature = intrinsic.signature(&external::RoleTypes::default());
                if arguments.len() != signature.fixed_parameter_count() {
                    return Err(IrError::InvalidExpression(format!(
                        "{} expects {} arguments, got {}",
                        intrinsic.symbol(),
                        signature.fixed_parameter_count(),
                        arguments.len()
                    )));
                }
                for (argument, expected) in arguments.iter().zip(signature.slot_types())
                {
                    let actual =
                        argument.validate_shape_with_captures(slots, capture_types)?;
                    if !expected.admits(&actual) {
                        return Err(IrError::InvalidExpression(format!(
                            "{} argument has type {actual}, expected {expected}",
                            intrinsic.symbol()
                        )));
                    }
                }
                Ok(result.clone())
            }
            Self::Default { .. } => Err(IrError::InvalidExpression(
                "default argument marker is only valid as a call operand".to_owned(),
            )),
            Self::Function { signature, .. } => {
                Ok(Type::Function(Box::new(signature.clone())))
            }
            Self::Captured {
                slot, value_type, ..
            } => {
                let Some(actual) = capture_types.get(*slot) else {
                    return Err(IrError::InvalidExpression(format!(
                        "capture slot {slot} is outside the closure environment"
                    )));
                };
                if !actual.admits(value_type) {
                    return Err(IrError::InvalidExpression(format!(
                        "capture slot {slot} has type {actual}, expression declares {value_type}"
                    )));
                }
                Ok(value_type.clone())
            }
            Self::Closure {
                signature,
                parameters,
                captures,
                capture_types: closure_capture_types,
                body,
                slot_count,
                ..
            } => {
                if let Err(message) = validate_signature_shape(signature) {
                    return Err(IrError::InvalidExpression(format!(
                        "closure has an invalid signature: {message}"
                    )));
                }
                if captures.len() != closure_capture_types.len() {
                    return Err(IrError::InvalidExpression(
                        "closure capture metadata length does not match captures"
                            .to_owned(),
                    ));
                }
                if parameters.len() != signature.parameters().len() {
                    return Err(IrError::InvalidExpression(
                        "closure parameter metadata does not match its signature"
                            .to_owned(),
                    ));
                }
                for (index, (actual, expected)) in
                    parameters.iter().zip(signature.parameters()).enumerate()
                {
                    if !actual.same_shape(expected) {
                        return Err(IrError::InvalidExpression(format!(
                            "closure parameter {index} has type {actual}, expected {expected}"
                        )));
                    }
                }
                if *slot_count < signature.fixed_parameter_count()
                    || *slot_count < body.slot_count()
                {
                    return Err(IrError::InvalidExpression(
                        "closure activation slot count is smaller than its body"
                            .to_owned(),
                    ));
                }
                for (capture, expected) in captures.iter().zip(closure_capture_types) {
                    let actual =
                        capture.validate_shape_with_captures(slots, capture_types)?;
                    if !expected.admits(&actual) {
                        return Err(IrError::InvalidExpression(format!(
                            "closure capture has type {actual}, expected {expected}"
                        )));
                    }
                }
                let mut closure_slots =
                    vec![
                        None;
                        signature.fixed_parameter_count().max(body.slot_count())
                    ];
                for (slot, value_type) in parameters.iter().enumerate() {
                    if let Some(bound) = closure_slots.get_mut(slot) {
                        *bound = Some(value_type.clone());
                    }
                }
                // Labelled slots, then a variadic tail, follow the positional ones.
                for (offset, value_type) in signature
                    .slot_types()
                    .into_iter()
                    .skip(parameters.len())
                    .enumerate()
                {
                    if let Some(bound) =
                        closure_slots.get_mut(parameters.len().saturating_add(offset))
                    {
                        *bound = Some(value_type);
                    }
                }
                let actual = body.validate_shape_with_captures(
                    &mut closure_slots,
                    closure_capture_types,
                )?;
                if !signature.result().admits(&actual) {
                    return Err(IrError::ResultTypeMismatch {
                        expected: Box::new(signature.result()),
                        actual: Box::new(actual),
                    });
                }
                Ok(Type::Function(Box::new(signature.clone())))
            }
            Self::Sequence { expressions, .. } => {
                let mut result = Type::Void;
                for expression in expressions {
                    result = expression
                        .validate_shape_with_captures(slots, capture_types)?;
                }
                Ok(result)
            }
            Self::Variable {
                slot, value_type, ..
            } => match slots.get(*slot).cloned().flatten() {
                Some(actual) if actual.admits(value_type) => Ok(value_type.clone()),
                Some(actual) => Err(IrError::InvalidExpression(format!(
                    "variable slot {slot} has type {actual}, expression declares {value_type}"
                ))),
                None => Err(IrError::InvalidExpression(format!(
                    "variable slot {slot} is not bound"
                ))),
            },
            Self::Global { value_type, .. } => Ok(value_type.clone()),
            Self::Let {
                slot, value, body, ..
            } => {
                let value_type =
                    value.validate_shape_with_captures(slots, capture_types)?;
                let mut body_slots = slots.to_vec();
                if let Some(slot) = slot {
                    let Some(bound) = body_slots.get_mut(*slot) else {
                        return Err(IrError::InvalidExpression(format!(
                            "let binding slot {slot} is outside the activation"
                        )));
                    };
                    if bound.is_some() {
                        return Err(IrError::InvalidExpression(format!(
                            "let binding slot {slot} shadows an active slot"
                        )));
                    }
                    *bound = Some(value_type);
                }
                body.validate_shape_with_captures(&mut body_slots, capture_types)
            }
            Self::Match {
                scrutinee,
                arms,
                value_type,
                ..
            } => {
                let scrutinee_type =
                    scrutinee.validate_shape_with_captures(slots, capture_types)?;
                if arms.is_empty() {
                    return Err(IrError::InvalidExpression(
                        "match has no arm".to_owned(),
                    ));
                }
                for arm in arms {
                    let mut arm_slots = slots.to_vec();
                    validate_pattern(
                        &arm.pattern,
                        Some(&scrutinee_type),
                        &mut arm_slots,
                    )?;
                    let actual = arm
                        .body
                        .validate_shape_with_captures(&mut arm_slots, capture_types)?;
                    if !value_type.admits(&actual) {
                        return Err(IrError::InvalidExpression(format!(
                            "match arm has type {actual}, expected {value_type}"
                        )));
                    }
                }
                Ok(value_type.clone())
            }
            Self::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let condition_type =
                    condition.validate_shape_with_captures(slots, capture_types)?;
                if !condition_type.same_shape(&Type::Bool) {
                    return Err(IrError::InvalidExpression(format!(
                        "if condition has type {condition_type}, expected bool"
                    )));
                }
                let mut then_slots = slots.to_vec();
                let mut else_slots = slots.to_vec();
                let then_type = then_branch
                    .validate_shape_with_captures(&mut then_slots, capture_types)?;
                let else_type = else_branch
                    .validate_shape_with_captures(&mut else_slots, capture_types)?;
                if then_type == Type::Never {
                    return Ok(else_type);
                }
                if else_type != Type::Never && !then_type.same_shape(&else_type) {
                    return Err(IrError::InvalidExpression(format!(
                        "if branches have types {then_type} and {else_type}"
                    )));
                }
                Ok(then_type)
            }
            Self::Call {
                arguments,
                result,
                target,
                ..
            } => {
                if let Some(callee) = target.callee() {
                    let callee_type =
                        callee.validate_shape_with_captures(slots, capture_types)?;
                    if !matches!(callee_type, Type::Function(_)) {
                        return Err(IrError::InvalidExpression(
                            "indirect callee is not a function".to_owned(),
                        ));
                    }
                }
                for argument in arguments {
                    match argument {
                        Self::Default { value_type, .. } => {
                            validate_type_shape(value_type).map_err(|message| {
                                IrError::InvalidExpression(format!(
                                    "default argument marker has an invalid type: {message}"
                                ))
                            })?;
                        }
                        _ => {
                            argument
                                .validate_shape_with_captures(slots, capture_types)?;
                        }
                    }
                }
                Ok(result.clone())
            }
            Self::Record {
                value_type, fields, ..
            } => {
                let mut names = BTreeSet::new();
                let mut actual = Vec::with_capacity(fields.len());
                for (name, value) in fields {
                    if !names.insert(name.as_str()) {
                        return Err(IrError::InvalidExpression(format!(
                            "record construction repeats field `{name}`"
                        )));
                    }
                    let field_type =
                        value.validate_shape_with_captures(slots, capture_types)?;
                    actual.push((name.clone(), field_type));
                }
                match value_type {
                    // A declared record is checked against its definition by
                    // the program-level pass, which owns the type table.
                    Type::Declared(_) | Type::Applied(_, _) => {}
                    Type::Record(expected) => {
                        let actual = canonical_members(actual);
                        if !members_same_shape(expected, &actual) {
                            return Err(IrError::InvalidExpression(format!(
                                "anonymous record fields do not match {value_type}"
                            )));
                        }
                    }
                    _ => {
                        return Err(IrError::InvalidExpression(format!(
                            "record construction produces non-record type {value_type}"
                        )));
                    }
                }
                Ok(value_type.clone())
            }
            Self::Variant {
                value_type,
                variant,
                payload,
                ..
            } => {
                let payload_type = payload
                    .as_deref()
                    .map(|payload| {
                        payload.validate_shape_with_captures(slots, capture_types)
                    })
                    .transpose()?
                    .unwrap_or(Type::Void);
                match value_type {
                    Type::Declared(_) | Type::Applied(_, _) => {}
                    // `bool` is represented directly; its variants are its values.
                    Type::Bool
                        if payload.is_none()
                            && matches!(variant.as_str(), "false" | "true") => {}
                    Type::Enum(variants) => {
                        let declared = variants
                            .iter()
                            .find(|(name, _)| name == variant)
                            .ok_or_else(|| {
                                IrError::InvalidExpression(format!(
                                    "{value_type} has no variant `{variant}`"
                                ))
                            })?;
                        if !declared.1.same_shape(&payload_type) {
                            return Err(IrError::InvalidExpression(format!(
                                "variant `{variant}` payload has type {payload_type}, expected {}",
                                declared.1
                            )));
                        }
                    }
                    _ => {
                        return Err(IrError::InvalidExpression(format!(
                            "variant construction produces non-enum type {value_type}"
                        )));
                    }
                }
                Ok(value_type.clone())
            }
            Self::Wrap {
                value_type, value, ..
            } => {
                let representation =
                    value.validate_shape_with_captures(slots, capture_types)?;
                // `str` and `bytes` are represented directly over their scalars
                // and bytes.
                let expected = match value_type {
                    Type::Str => Some(Type::Array(Box::new(Type::Char))),
                    Type::Bytes => Some(Type::Array(Box::new(Type::U8))),
                    _ => None,
                };
                if expected
                    .is_some_and(|expected| !expected.same_shape(&representation))
                {
                    return Err(IrError::InvalidExpression(format!(
                        "{value_type} is not written over {representation}"
                    )));
                }
                Ok(value_type.clone())
            }
            Self::Try {
                value, value_type, ..
            } => {
                let container =
                    value.validate_shape_with_captures(slots, capture_types)?;
                if !matches!(container, Type::Applied(_, _) | Type::Param(_)) {
                    return Err(IrError::InvalidExpression(format!(
                        "`try` over non-container {container}"
                    )));
                }
                Ok(value_type.clone())
            }
            Self::Return { value, .. } => {
                value.validate_shape_with_captures(slots, capture_types)?;
                Ok(Type::Never)
            }
            Self::Widen {
                value_type,
                value,
                member,
                ..
            } => {
                let actual =
                    value.validate_shape_with_captures(slots, capture_types)?;
                let widens = match value_type {
                    Type::Atom => {
                        member.is_none() && matches!(actual, Type::AtomSingleton(_))
                    }
                    Type::Union(members) => member
                        .and_then(|index| members.get(index))
                        .is_some_and(|found| found.admits(&actual)),
                    // A declared union's members are checked with its definition.
                    Type::Declared(_) | Type::Applied(_, _) => true,
                    // An interface value holds a concrete value, never an
                    // atom singleton or another interface value: widening
                    // does not chain. The checker proved conformance.
                    Type::Interface(_, _) | Type::Any => {
                        member.is_none()
                            && !matches!(
                                actual,
                                Type::AtomSingleton(_)
                                    | Type::Interface(_, _)
                                    | Type::Any
                            )
                    }
                    _ => false,
                };
                if !widens {
                    return Err(IrError::InvalidExpression(format!(
                        "{actual} does not widen to {value_type}"
                    )));
                }
                Ok(value_type.clone())
            }
            Self::Project {
                record,
                field,
                value_type,
                ..
            } => {
                let record_type =
                    record.validate_shape_with_captures(slots, capture_types)?;
                match &record_type {
                    Type::Declared(_) | Type::Applied(_, _) => {}
                    Type::Record(fields) => {
                        let found = fields.iter().find(|(name, _)| name == field);
                        if !found.is_some_and(|(_, found)| found.same_shape(value_type))
                        {
                            return Err(IrError::InvalidExpression(format!(
                                "{record_type} has no field `{field}` of type {value_type}"
                            )));
                        }
                    }
                    _ => {
                        return Err(IrError::InvalidExpression(format!(
                            "projection of `{field}` from non-record type {record_type}"
                        )));
                    }
                }
                Ok(value_type.clone())
            }
            Self::Tuple {
                value_type,
                components,
                ..
            } => {
                let actual = components
                    .iter()
                    .map(|component| {
                        component.validate_shape_with_captures(slots, capture_types)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                match value_type {
                    // A declared tuple is checked by the program-level pass.
                    Type::Declared(_) | Type::Applied(_, _) => {}
                    Type::Tuple(expected) => {
                        if !admits_all(expected, &actual) {
                            return Err(IrError::InvalidExpression(format!(
                                "tuple components do not match {value_type}"
                            )));
                        }
                    }
                    _ => {
                        return Err(IrError::InvalidExpression(format!(
                            "tuple construction produces non-tuple type {value_type}"
                        )));
                    }
                }
                Ok(value_type.clone())
            }
            Self::TupleProject {
                tuple,
                index,
                value_type,
                ..
            } => {
                let tuple_type =
                    tuple.validate_shape_with_captures(slots, capture_types)?;
                match &tuple_type {
                    Type::Declared(_) | Type::Applied(_, _) => {}
                    Type::Tuple(components) => {
                        if !components
                            .get(*index)
                            .is_some_and(|found| found.admits(value_type))
                        {
                            return Err(IrError::InvalidExpression(format!(
                                "{tuple_type} has no component {index} of type {value_type}"
                            )));
                        }
                    }
                    _ => {
                        return Err(IrError::InvalidExpression(format!(
                            "projection of component {index} from non-tuple type {tuple_type}"
                        )));
                    }
                }
                Ok(value_type.clone())
            }
            Self::Array {
                value_type,
                elements,
                ..
            } => {
                let Type::Array(element) = value_type else {
                    return Err(IrError::InvalidExpression(format!(
                        "array construction produces non-array type {value_type}"
                    )));
                };
                for value in elements {
                    let actual =
                        value.validate_shape_with_captures(slots, capture_types)?;
                    if !element.admits(&actual) {
                        return Err(IrError::InvalidExpression(format!(
                            "array element has type {actual}, expected {element}"
                        )));
                    }
                }
                Ok(value_type.clone())
            }
            Self::Dict {
                value_type,
                entries,
                ..
            } => {
                let Type::Dict(key_type, entry_type) = value_type else {
                    return Err(IrError::InvalidExpression(format!(
                        "dict construction produces non-dict type {value_type}"
                    )));
                };
                for (key, value) in entries {
                    let actual_key =
                        key.validate_shape_with_captures(slots, capture_types)?;
                    let actual_value =
                        value.validate_shape_with_captures(slots, capture_types)?;
                    if !key_type.admits(&actual_key)
                        || !entry_type.admits(&actual_value)
                    {
                        return Err(IrError::InvalidExpression(format!(
                            "dict entry has types {actual_key} and {actual_value}, expected {value_type}"
                        )));
                    }
                }
                Ok(value_type.clone())
            }
            Self::Lookup {
                collection,
                key,
                value_type,
                ..
            } => {
                let collection_type =
                    collection.validate_shape_with_captures(slots, capture_types)?;
                let key_type =
                    key.validate_shape_with_captures(slots, capture_types)?;
                let (expected_key, element) = match &collection_type {
                    Type::Array(element) => (Type::U64, element.as_ref().clone()),
                    Type::Dict(key, value) => {
                        (key.as_ref().clone(), value.as_ref().clone())
                    }
                    Type::Str => (Type::U64, Type::Char),
                    Type::Bytes => (Type::U64, Type::U8),
                    _ => {
                        return Err(IrError::InvalidExpression(format!(
                            "lookup into non-collection type {collection_type}"
                        )));
                    }
                };
                let option_of = match value_type {
                    Type::Applied(_, arguments) if arguments.len() == 1 => {
                        arguments.first()
                    }
                    _ => None,
                };
                if !expected_key.admits(&key_type)
                    || !option_of.is_some_and(|found| found.admits(&element))
                {
                    return Err(IrError::InvalidExpression(format!(
                        "lookup into {collection_type} with {key_type} cannot produce {value_type}"
                    )));
                }
                Ok(value_type.clone())
            }
        }
    }
}

/// Checks a pattern against its expected type and binds its slots. Inside a
/// declared type the component types belong to the definition, which the
/// program-level pass owns, so `expected` is `None` and only binders are
/// recorded.
fn validate_pattern(
    pattern: &Pattern,
    expected: Option<&Type>,
    slots: &mut [Option<Type>],
) -> Result<(), IrError> {
    let invalid = |message: String| Err(IrError::InvalidExpression(message));
    let declared = matches!(expected, Some(Type::Declared(_) | Type::Applied(_, _)));
    match pattern {
        Pattern::Wildcard => Ok(()),
        Pattern::Bind { slot, value_type } => {
            if let Some(expected) = expected
                && !expected.admits(value_type)
            {
                return invalid(format!(
                    "pattern binder has type {value_type}, expected {expected}"
                ));
            }
            let Some(bound) = slots.get_mut(*slot) else {
                return invalid(format!(
                    "pattern slot {slot} is outside the activation"
                ));
            };
            if bound.is_some() {
                return invalid(format!("pattern slot {slot} shadows an active slot"));
            }
            *bound = Some(value_type.clone());
            Ok(())
        }
        Pattern::Literal(value) => match expected {
            Some(Type::Atom) if matches!(value, Value::Atom(_)) => Ok(()),
            Some(expected) if !expected.admits(&value.ty()) => invalid(format!(
                "literal pattern has type {}, expected {expected}",
                value.ty()
            )),
            _ => Ok(()),
        },
        Pattern::Variant { variant, payload } => {
            let payload_type = match expected {
                Some(Type::Enum(variants)) => {
                    let Some((_, payload_type)) =
                        variants.iter().find(|(name, _)| name == variant)
                    else {
                        return invalid(format!(
                            "pattern names unknown variant `{variant}`"
                        ));
                    };
                    Some(payload_type)
                }
                None | Some(Type::Declared(_) | Type::Applied(_, _)) => None,
                Some(expected) => {
                    return invalid(format!(
                        "variant pattern for non-enum type {expected}"
                    ));
                }
            };
            match payload {
                Some(payload) => validate_pattern(payload, payload_type, slots),
                None => Ok(()),
            }
        }
        Pattern::Record(fields) => {
            for (name, field) in fields {
                let field_type = match expected {
                    Some(Type::Record(members)) => {
                        let Some((_, field_type)) =
                            members.iter().find(|(member, _)| member == name)
                        else {
                            return invalid(format!(
                                "pattern names unknown field `{name}`"
                            ));
                        };
                        Some(field_type)
                    }
                    None | Some(Type::Declared(_) | Type::Applied(_, _)) => None,
                    Some(expected) => {
                        return invalid(format!(
                            "record pattern for non-record type {expected}"
                        ));
                    }
                };
                validate_pattern(field, field_type, slots)?;
            }
            Ok(())
        }
        Pattern::Tuple(items) => {
            let components = match expected {
                Some(Type::Tuple(components)) if components.len() == items.len() => {
                    Some(components)
                }
                None | Some(Type::Declared(_) | Type::Applied(_, _)) => None,
                Some(expected) => {
                    return invalid(format!("tuple pattern does not fit {expected}"));
                }
            };
            for (index, item) in items.iter().enumerate() {
                validate_pattern(
                    item,
                    components.and_then(|components| components.get(index)),
                    slots,
                )?;
            }
            Ok(())
        }
        Pattern::Wrap(inner) => match expected {
            Some(Type::Str) => {
                validate_pattern(inner, Some(&Type::Array(Box::new(Type::Char))), slots)
            }
            Some(Type::Bytes) => {
                validate_pattern(inner, Some(&Type::Array(Box::new(Type::U8))), slots)
            }
            _ if declared || expected.is_none() => validate_pattern(inner, None, slots),
            _ => invalid("wrapper pattern for a non-declared type".to_owned()),
        },
        Pattern::Member {
            index,
            member,
            pattern,
        } => {
            match expected {
                Some(Type::Union(members))
                    if !members
                        .get(*index)
                        .is_some_and(|found| found.same_shape(member)) =>
                {
                    return invalid(format!("{member} is not a member of the union"));
                }
                Some(Type::Union(_) | Type::Declared(_) | Type::Applied(_, _))
                | None => {}
                Some(expected) => {
                    return invalid(format!(
                        "member pattern for non-union type {expected}"
                    ));
                }
            }
            validate_pattern(pattern, Some(member), slots)
        }
        Pattern::Array(items) => {
            let element = match expected {
                Some(Type::Array(element)) => Some(element.as_ref()),
                None => None,
                Some(expected) => {
                    return invalid(format!(
                        "array pattern for non-array type {expected}"
                    ));
                }
            };
            for item in items {
                validate_pattern(item, element, slots)?;
            }
            Ok(())
        }
    }
}

/// Whether each expected type admits the actual type at the same position.
fn admits_all(expected: &[Type], actual: &[Type]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(expected, actual)| expected.admits(actual))
}

fn substitute_members(
    members: &[(String, Type)],
    arguments: &BTreeMap<String, Type>,
) -> Vec<(String, Type)> {
    members
        .iter()
        .map(|(name, value)| (name.clone(), value.substitute(arguments)))
        .collect()
}

/// Whether two canonical member lists have equal names and same-shape types.
fn members_same_shape(left: &[(String, Type)], right: &[(String, Type)]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.0 == right.0 && left.1.same_shape(&right.1))
}

/// One checked module-level immutable value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedGlobal {
    name: String,
    value_type: Type,
    initializer: Expr,
    origin: SourceOrigin,
    slot_count: usize,
}

impl CheckedGlobal {
    /// Creates a checked global after validating its initializer type.
    pub fn new(
        name: impl Into<String>,
        value_type: Type,
        initializer: Expr,
        origin: SourceOrigin,
    ) -> Result<Self, IrError> {
        let name = name.into();
        if let Err(message) = validate_type_shape(&value_type) {
            return Err(IrError::InvalidExpression(format!(
                "global `{}` has an invalid type: {message}",
                name
            )));
        }
        let slot_count = initializer.slot_count();
        let mut slots = vec![None; slot_count];
        let actual = initializer.validate_shape(&mut slots)?;
        if !actual.same_shape(&value_type) {
            return Err(IrError::ResultTypeMismatch {
                expected: Box::new(value_type),
                actual: Box::new(actual),
            });
        }
        Ok(Self {
            name,
            value_type,
            initializer,
            origin,
            slot_count,
        })
    }

    /// Global name in its owning module.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The checked global type.
    #[must_use]
    pub fn value_type(&self) -> Type {
        self.value_type.clone()
    }

    /// The checked initializer.
    #[must_use]
    pub const fn initializer(&self) -> &Expr {
        &self.initializer
    }

    /// The declaration origin.
    #[must_use]
    pub const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }

    /// Number of immutable activation slots required by the initializer.
    #[must_use]
    pub const fn slot_count(&self) -> usize {
        self.slot_count
    }
}

/// One checked module-level function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedFunction {
    name: String,
    signature: FunctionSignature,
    body: Expr,
    origin: SourceOrigin,
    slot_count: usize,
    external_wrapper: bool,
    test_assertion: Option<TestAssertion>,
    implements: Option<Implements>,
}

/// The contract member a function implements for one receiver type, which
/// a [`CallTarget::Contract`] dispatches to from the receiver's runtime type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Implements {
    /// The interface identity.
    pub interface: TypeId,
    /// The contract member name.
    pub member: String,
    /// The receiver type; its generic parameters match any argument.
    pub receiver: Type,
    /// The interface's type arguments this implementation is for, over the
    /// same generic parameters as `receiver`. A default member names the
    /// interface's own parameters.
    pub arguments: Vec<Type>,
}

impl CheckedFunction {
    /// Creates a checked function backed by a closed compiler intrinsic.
    pub fn new_external(
        name: impl Into<String>,
        signature: FunctionSignature,
        intrinsic: external::CompilerIntrinsic,
        origin: SourceOrigin,
    ) -> Result<Self, IrError> {
        // The checker binds the result's roles; every operand slot is fixed
        // by the registry itself.
        let registry = intrinsic.signature(&external::RoleTypes::default());
        let operands_agree = signature.fixed_parameter_count()
            == registry.fixed_parameter_count()
            && signature
                .slot_types()
                .iter()
                .zip(registry.slot_types())
                .all(|(declared, registered)| declared.same_shape(&registered));
        if !operands_agree {
            return Err(IrError::InvalidExpression(format!(
                "external {} has a mismatched declaration signature",
                intrinsic.symbol()
            )));
        }
        let arguments = signature
            .slot_types()
            .into_iter()
            .enumerate()
            .map(|(slot, value_type)| Expr::variable(slot, value_type, origin.clone()))
            .collect();
        let body = Expr::external_with_result(
            intrinsic,
            arguments,
            signature.result(),
            origin.clone(),
        );
        let slot_count = registry.fixed_parameter_count();
        Self::with_slots_and_external(
            name, signature, body, origin, slot_count, true, None,
        )
    }

    /// Creates a checked function, rejecting a body whose final type differs
    /// from its written result type.
    pub fn new(
        name: impl Into<String>,
        signature: FunctionSignature,
        body: Expr,
        origin: SourceOrigin,
    ) -> Result<Self, IrError> {
        let slot_count = signature.fixed_parameter_count().max(body.slot_count());
        Self::with_slots(name, signature, body, origin, slot_count)
    }

    /// Creates a checked function with explicit immutable activation slots.
    pub fn with_slots(
        name: impl Into<String>,
        signature: FunctionSignature,
        body: Expr,
        origin: SourceOrigin,
        slot_count: usize,
    ) -> Result<Self, IrError> {
        Self::with_slots_and_external(
            name, signature, body, origin, slot_count, false, None,
        )
    }

    /// Creates a verified member of the closed test assertion table.
    pub fn new_test_assertion(
        name: impl Into<String>,
        assertion: TestAssertion,
        origin: SourceOrigin,
    ) -> Result<Self, IrError> {
        let signature = assertion.signature();
        let slot_count = signature.fixed_parameter_count();
        let body = Expr::literal(Value::Void, origin.clone());
        Self::with_slots_and_external(
            name,
            signature,
            body,
            origin,
            slot_count,
            false,
            Some(assertion),
        )
    }

    fn with_slots_and_external(
        name: impl Into<String>,
        signature: FunctionSignature,
        body: Expr,
        origin: SourceOrigin,
        slot_count: usize,
        external_wrapper: bool,
        test_assertion: Option<TestAssertion>,
    ) -> Result<Self, IrError> {
        let name = name.into();
        if let Err(message) = validate_signature_shape(&signature) {
            return Err(IrError::InvalidExpression(format!(
                "function `{name}` has an invalid signature: {message}"
            )));
        }
        if slot_count < signature.fixed_parameter_count() {
            return Err(IrError::InvalidSlotCount {
                function: name,
                slots: slot_count,
                parameters: signature.fixed_parameter_count(),
            });
        }
        let mut slots = vec![None; slot_count];
        for (slot, value_type) in signature.slot_types().into_iter().enumerate() {
            if let Some(bound) = slots.get_mut(slot) {
                *bound = Some(value_type);
            }
        }
        let actual = body.validate_shape(&mut slots)?;
        // A body of type `never` is admitted at any written result type.
        if actual != Type::Never && !actual.same_shape(&signature.result()) {
            return Err(IrError::ResultTypeMismatch {
                expected: Box::new(signature.result()),
                actual: Box::new(actual),
            });
        }
        Ok(Self {
            name,
            signature,
            body,
            origin,
            slot_count,
            external_wrapper,
            test_assertion,
            implements: None,
        })
    }

    /// Function name in its owning module.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The checked signature.
    #[must_use]
    pub const fn signature(&self) -> &FunctionSignature {
        &self.signature
    }

    /// The checked body.
    #[must_use]
    pub const fn body(&self) -> &Expr {
        &self.body
    }

    /// The declaration origin.
    #[must_use]
    pub const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }

    /// Number of immutable activation slots allocated by the checker.
    #[must_use]
    pub const fn slot_count(&self) -> usize {
        self.slot_count
    }

    /// Whether this function is a synthetic wrapper around a compiler
    /// intrinsic rather than a source-level module definition.
    #[must_use]
    pub const fn is_external_wrapper(&self) -> bool {
        self.external_wrapper
    }

    /// Whether this is one of the closed verified test assertion functions.
    #[must_use]
    pub const fn test_assertion(&self) -> Option<TestAssertion> {
        self.test_assertion
    }

    /// The same function marked as implementing a contract member.
    #[must_use]
    pub fn with_implements(mut self, implements: Implements) -> Self {
        self.implements = Some(implements);
        self
    }

    /// The contract member this function implements, if any.
    #[must_use]
    pub const fn implements(&self) -> Option<&Implements> {
        self.implements.as_ref()
    }
}

/// Validates entry-independent IR structure and module initializer cycles.
///
/// A program entry affects indirect call-flow analysis, so callers that do not
/// have an executable entry can still validate structural references and
/// initializer dependencies without choosing an arbitrary function. An
/// initializer that reaches an unbounded indirect call returns
/// [`IrError::RecursiveCall`].
pub fn validate_global_initializer_cycles(
    globals: &[CheckedGlobal],
    functions: &[CheckedFunction],
) -> Result<(), IrError> {
    let dependencies = static_program_edges(globals, functions)?;
    reject_entry_free_initializer_cycles(globals, functions, dependencies)
}

/// Outgoing dependency edges of each global and function node.
type DependencyEdges = Vec<BTreeSet<DependencyNode>>;

/// Direct dependency edges from every global and function body.
fn static_program_edges(
    globals: &[CheckedGlobal],
    functions: &[CheckedFunction],
) -> Result<DependencyEdges, IrError> {
    let mut dependencies = vec![BTreeSet::new(); globals.len() + functions.len()];
    for (index, global) in globals.iter().enumerate() {
        validate_program_expr(
            global.initializer(),
            globals,
            functions,
            Some(DependencyNode::Global(index)),
            &mut dependencies,
        )?;
    }
    for (index, function) in functions.iter().enumerate() {
        validate_program_expr(
            function.body(),
            globals,
            functions,
            Some(DependencyNode::Function(index)),
            &mut dependencies,
        )?;
    }
    Ok(dependencies)
}

fn reject_entry_free_initializer_cycles(
    globals: &[CheckedGlobal],
    functions: &[CheckedFunction],
    mut dependencies: DependencyEdges,
) -> Result<(), IrError> {
    let flow = analyze_initializer_call_flow(globals, functions)?;
    for (dependencies, flow_edges) in dependencies.iter_mut().zip(flow.dependencies) {
        dependencies.extend(flow_edges);
    }
    reject_global_initializer_cycles(&dependencies, globals.len())
}

/// Module values and functions validated once and shared by the program of
/// every entry and test in one checking scope.
///
/// Construction checks everything that does not depend on an entry: names,
/// signature and body shapes, static references, and initializer cycles.
/// [`CheckedProgram::for_entry`] then adds only the entry-rooted call-flow
/// analysis, so a scope with many entries is neither deep-copied nor
/// revalidated per entry.
#[derive(Debug, PartialEq, Eq)]
pub struct CheckedModuleSet {
    types: Vec<TypeDefinition>,
    globals: Vec<CheckedGlobal>,
    functions: Vec<CheckedFunction>,
    static_dependencies: DependencyEdges,
}

impl CheckedModuleSet {
    /// Validates a complete set of checked globals and functions.
    pub fn try_new(
        globals: Vec<CheckedGlobal>,
        functions: Vec<CheckedFunction>,
    ) -> Result<Arc<Self>, IrError> {
        Self::try_new_with_types(Vec::new(), globals, functions)
    }

    /// Validates a complete set of declared types, globals, and functions.
    ///
    /// Every declared type named by a signature, global, or expression must
    /// have exactly one definition, and every declared construction and
    /// projection must agree with it.
    pub fn try_new_with_types(
        types: Vec<TypeDefinition>,
        globals: Vec<CheckedGlobal>,
        functions: Vec<CheckedFunction>,
    ) -> Result<Arc<Self>, IrError> {
        let definitions = type_table(&types)?;
        if functions.is_empty() {
            return Err(IrError::NoFunctions);
        }
        let mut names = BTreeSet::new();
        for function in &functions {
            if !names.insert(function.name.as_str()) {
                return Err(IrError::DuplicateFunction(function.name.clone()));
            }
            if let Err(message) = validate_signature_shape(function.signature()) {
                return Err(IrError::InvalidExpression(format!(
                    "function `{}` has an invalid signature: {message}",
                    function.name
                )));
            }
            let mut slots = vec![None; function.slot_count];
            for (slot, value_type) in
                function.signature.slot_types().into_iter().enumerate()
            {
                if let Some(bound) = slots.get_mut(slot) {
                    *bound = Some(value_type);
                }
            }
            let actual = function.body.validate_shape(&mut slots)?;
            if actual != Type::Never && !actual.same_shape(&function.signature.result())
            {
                return Err(IrError::ResultTypeMismatch {
                    expected: Box::new(function.signature.result()),
                    actual: Box::new(actual),
                });
            }
        }
        for global in &globals {
            if let Err(message) = validate_type_shape(&global.value_type) {
                return Err(IrError::InvalidExpression(format!(
                    "global `{}` has an invalid type: {message}",
                    global.name
                )));
            }
            let mut slots = vec![None; global.slot_count];
            let actual = global.initializer.validate_shape(&mut slots)?;
            if !actual.same_shape(&global.value_type) {
                return Err(IrError::ResultTypeMismatch {
                    expected: Box::new(global.value_type.clone()),
                    actual: Box::new(actual),
                });
            }
        }
        for function in &functions {
            validate_declared_signature(function.signature(), &definitions)?;
            validate_declared_expr(function.body(), &definitions)?;
        }
        for global in &globals {
            validate_declared_type(&global.value_type, &definitions)?;
            validate_declared_expr(global.initializer(), &definitions)?;
        }
        for global in &globals {
            validate_tail_calls(global.initializer(), false)?;
        }
        for function in &functions {
            validate_tail_calls(function.body(), true)?;
        }
        let static_dependencies = static_program_edges(&globals, &functions)?;
        reject_entry_free_initializer_cycles(
            &globals,
            &functions,
            static_dependencies.clone(),
        )?;
        Ok(Arc::new(Self {
            types,
            globals,
            functions,
            static_dependencies,
        }))
    }

    /// Declared type definitions in deterministic checked order.
    #[must_use]
    pub fn types(&self) -> &[TypeDefinition] {
        &self.types
    }

    /// The definition of one declared type.
    #[must_use]
    pub fn type_definition(&self, id: &TypeId) -> Option<&TypeDefinition> {
        self.types.iter().find(|definition| definition.id() == id)
    }

    /// Immutable module values in deterministic checked order.
    #[must_use]
    pub fn globals(&self) -> &[CheckedGlobal] {
        &self.globals
    }

    /// Functions in deterministic source order.
    #[must_use]
    pub fn functions(&self) -> &[CheckedFunction] {
        &self.functions
    }
}

/// A complete immutable program that crossed the checker boundary: a shared
/// [`CheckedModuleSet`] and one validated entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedProgram {
    set: Arc<CheckedModuleSet>,
    entry: usize,
}

impl CheckedProgram {
    /// Creates a checked program from already checked functions.
    ///
    /// This constructor accepts semantic IR only. It does not accept a
    /// parsed AST, and it revalidates entry and function-body invariants so a
    /// caller cannot accidentally execute an arbitrary syntax tree.
    pub fn try_new(
        functions: Vec<CheckedFunction>,
        entry: usize,
    ) -> Result<Self, IrError> {
        Self::try_new_with_globals(Vec::new(), functions, entry)
    }

    /// Creates a checked program containing immutable module values.
    pub fn try_new_with_globals(
        globals: Vec<CheckedGlobal>,
        functions: Vec<CheckedFunction>,
        entry: usize,
    ) -> Result<Self, IrError> {
        Self::for_entry(CheckedModuleSet::try_new(globals, functions)?, entry)
    }

    /// Creates a checked program containing declared types and module values.
    pub fn try_new_with_types(
        types: Vec<TypeDefinition>,
        globals: Vec<CheckedGlobal>,
        functions: Vec<CheckedFunction>,
        entry: usize,
    ) -> Result<Self, IrError> {
        Self::for_entry(
            CheckedModuleSet::try_new_with_types(types, globals, functions)?,
            entry,
        )
    }

    /// Selects one entry of an already validated module set.
    ///
    /// Only entry-dependent analysis runs here: indirect call flow rooted at
    /// the globals and `entry`, and initializer cycles through that flow.
    pub fn for_entry(
        set: Arc<CheckedModuleSet>,
        entry: usize,
    ) -> Result<Self, IrError> {
        if entry >= set.functions.len() {
            return Err(IrError::InvalidEntry(entry));
        }
        let globals = &set.globals;
        let functions = &set.functions;
        let mut dependencies = set.static_dependencies.clone();
        let flow = analyze_call_flow(globals, functions, entry)?;
        for (dependencies, flow_edges) in dependencies.iter_mut().zip(flow.dependencies)
        {
            dependencies.extend(flow_edges);
        }
        reject_global_initializer_cycles(&dependencies, globals.len())?;
        Ok(Self { set, entry })
    }

    /// The validated module set this program's entry selects from.
    #[must_use]
    pub const fn module_set(&self) -> &Arc<CheckedModuleSet> {
        &self.set
    }

    /// Declared type definitions in deterministic checked order.
    #[must_use]
    pub fn types(&self) -> &[TypeDefinition] {
        &self.set.types
    }

    /// Immutable module values in deterministic checked order.
    #[must_use]
    pub fn globals(&self) -> &[CheckedGlobal] {
        &self.set.globals
    }

    /// Functions in deterministic source order.
    #[must_use]
    pub fn functions(&self) -> &[CheckedFunction] {
        &self.set.functions
    }

    /// The validated entry function index.
    #[must_use]
    pub const fn entry_index(&self) -> usize {
        self.entry
    }

    /// The selected entry function.
    #[must_use]
    #[allow(clippy::indexing_slicing)]
    pub fn entry(&self) -> &CheckedFunction {
        // `for_entry` proves this index is below the immutable function count.
        &self.set.functions[self.entry]
    }

    /// Canonical typed-program observation used by static-v1.
    #[must_use]
    pub fn canonical_vibon(&self) -> String {
        let mut output = String::from("(record\n  format: @types.v1\n");
        if !self.set.globals.is_empty() {
            output.push_str("  globals: (array\n");
            for global in &self.set.globals {
                output.push_str(&format!(
                    "    (record name: @{} type: {} body: {})\n",
                    global.name,
                    canonical_type(&global.value_type),
                    canonical_expr(global.initializer()),
                ));
            }
            output.push_str("  )\n");
        }
        output.push_str("  functions: (array\n");
        for function in &self.set.functions {
            let parameters = function
                .signature
                .parameters()
                .iter()
                .map(canonical_type)
                .collect::<Vec<_>>();
            let body = canonical_expr(function.body());
            output.push_str(&format!(
                "    (record name: @{} params: {} result: {}{} body: {})\n",
                function.name,
                canonical_array(&parameters),
                canonical_type(&function.signature.result()),
                canonical_labelled_field(&function.signature),
                body
            ));
        }
        output.push_str("  )\n)\n");
        output
    }
}

/// An invariant violation while constructing checked semantic IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IrError {
    /// A program had no executable functions.
    NoFunctions,
    /// The entry index did not identify a function.
    InvalidEntry(usize),
    /// Two module-level functions share a name.
    DuplicateFunction(String),
    /// A function body result differs from its checked signature.
    ResultTypeMismatch {
        /// The written result type.
        expected: Box<Type>,
        /// The body result type.
        actual: Box<Type>,
    },
    /// A function did not allocate slots for all fixed parameters.
    InvalidSlotCount {
        /// Function name associated with the invalid count.
        function: String,
        /// Allocated slots.
        slots: usize,
        /// Required parameter slots.
        parameters: usize,
    },
    /// A public expression constructor produced an invalid checked shape.
    InvalidExpression(String),
    /// An indirect call target is not statically bounded.
    RecursiveCall(String),
    /// A module initializer dependency graph contains a cycle.
    GlobalInitializerCycle(usize),
    /// Indirect call-flow analysis reached its iteration bound without a
    /// fixed point; its call and dependency sets would be unsound.
    CallFlowDidNotConverge,
}

impl fmt::Display for IrError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoFunctions => {
                formatter.write_str("checked program has no functions")
            }
            Self::InvalidEntry(entry) => {
                write!(formatter, "checked entry {entry} is invalid")
            }
            Self::DuplicateFunction(name) => {
                write!(formatter, "checked program repeats function `{name}`")
            }
            Self::ResultTypeMismatch { expected, actual } => {
                write!(formatter, "body has type {actual}, expected {expected}")
            }
            Self::InvalidSlotCount {
                function,
                slots,
                parameters,
            } => write!(
                formatter,
                "function `{function}` allocates {slots} slots for {parameters} parameters"
            ),
            Self::InvalidExpression(message) => {
                write!(formatter, "invalid checked expression: {message}")
            }
            Self::RecursiveCall(message) => {
                write!(formatter, "recursive call graph: {message}")
            }
            Self::GlobalInitializerCycle(global) => {
                write!(
                    formatter,
                    "global initializer cycle involving global {global}"
                )
            }
            Self::CallFlowDidNotConverge => {
                formatter.write_str("indirect call-flow analysis did not converge")
            }
        }
    }
}

impl std::error::Error for IrError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum DependencyNode {
    Global(usize),
    Function(usize),
}

impl DependencyNode {
    fn node_index(self, global_count: usize) -> usize {
        match self {
            Self::Global(index) => index,
            Self::Function(index) => global_count.saturating_add(index),
        }
    }
}

fn validate_program_expr(
    expression: &Expr,
    globals: &[CheckedGlobal],
    functions: &[CheckedFunction],
    owner: Option<DependencyNode>,
    dependencies: &mut [BTreeSet<DependencyNode>],
) -> Result<(), IrError> {
    match expression {
        Expr::Record { .. }
        | Expr::Variant { .. }
        | Expr::Wrap { .. }
        | Expr::Widen { .. }
        | Expr::Try { .. }
        | Expr::Return { .. }
        | Expr::Project { .. }
        | Expr::Tuple { .. }
        | Expr::TupleProject { .. }
        | Expr::Array { .. }
        | Expr::Dict { .. }
        | Expr::Lookup { .. } => {
            for operand in expression.data_operands() {
                validate_program_expr(
                    operand,
                    globals,
                    functions,
                    owner,
                    dependencies,
                )?;
            }
            if let Some(owner) = owner {
                for target in key_order_targets(expression, functions) {
                    let Some(edges) =
                        dependencies.get_mut(owner.node_index(globals.len()))
                    else {
                        return Err(IrError::InvalidExpression(format!(
                            "dependency owner {owner:?} is outside the program"
                        )));
                    };
                    edges.insert(DependencyNode::Function(target));
                }
            }
        }
        Expr::Literal { .. }
        | Expr::Default { .. }
        | Expr::Variable { .. }
        | Expr::Captured { .. } => {}
        Expr::External {
            intrinsic,
            arguments,
            ..
        } => {
            for argument in arguments {
                validate_program_expr(
                    argument,
                    globals,
                    functions,
                    owner,
                    dependencies,
                )?;
            }
            // `array.fold` invokes its step operand, so the step's targets
            // are this owner's dependencies, as for an indirect call.
            if *intrinsic == external::CompilerIntrinsic::ArrayFold
                && let [_, _, step] = arguments.as_slice()
            {
                let summary = possible_function_targets(
                    step,
                    &BTreeMap::new(),
                    globals,
                    functions,
                    &mut BTreeSet::new(),
                    &mut BTreeSet::new(),
                );
                for target in summary.known {
                    if functions.get(target).is_none() {
                        return Err(IrError::InvalidExpression(format!(
                            "function index {target} is outside the program"
                        )));
                    }
                    if let Some(owner) = owner {
                        let Some(edges) =
                            dependencies.get_mut(owner.node_index(globals.len()))
                        else {
                            return Err(IrError::InvalidExpression(format!(
                                "dependency owner {owner:?} is outside the program"
                            )));
                        };
                        edges.insert(DependencyNode::Function(target));
                    }
                }
            }
        }
        Expr::Function {
            function,
            signature,
            ..
        } => {
            let Some(callee) = functions.get(*function) else {
                return Err(IrError::InvalidExpression(format!(
                    "function index {function} is outside the program"
                )));
            };
            if !callee.signature().admits(signature) {
                return Err(IrError::InvalidExpression(format!(
                    "function value index {function} carries a mismatched signature"
                )));
            }
        }
        Expr::Closure { captures, body, .. } => {
            for capture in captures {
                validate_program_expr(
                    capture,
                    globals,
                    functions,
                    owner,
                    dependencies,
                )?;
            }
            validate_program_expr(
                body,
                globals,
                functions,
                // Validate the body structurally, but its effects and reads
                // belong to an invocation site, not closure creation.
                None,
                dependencies,
            )?;
        }
        Expr::Sequence { expressions, .. } => {
            for expression in expressions {
                validate_program_expr(
                    expression,
                    globals,
                    functions,
                    owner,
                    dependencies,
                )?;
            }
        }
        Expr::Global {
            index, value_type, ..
        } => {
            let Some(global) = globals.get(*index) else {
                return Err(IrError::InvalidExpression(format!(
                    "global index {index} is outside the program"
                )));
            };
            if !global.value_type().same_shape(value_type) {
                return Err(IrError::InvalidExpression(format!(
                    "global index {index} has type {}, expression declares {value_type}",
                    global.value_type()
                )));
            }
            if let Some(owner) = owner {
                let Some(edges) = dependencies.get_mut(owner.node_index(globals.len()))
                else {
                    return Err(IrError::InvalidExpression(format!(
                        "dependency owner {owner:?} is outside the program"
                    )));
                };
                edges.insert(DependencyNode::Global(*index));
            }
        }
        Expr::Let { value, body, .. } => {
            validate_program_expr(value, globals, functions, owner, dependencies)?;
            validate_program_expr(body, globals, functions, owner, dependencies)?;
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            validate_program_expr(scrutinee, globals, functions, owner, dependencies)?;
            for arm in arms {
                validate_program_expr(
                    &arm.body,
                    globals,
                    functions,
                    owner,
                    dependencies,
                )?;
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            validate_program_expr(condition, globals, functions, owner, dependencies)?;
            validate_program_expr(
                then_branch,
                globals,
                functions,
                owner,
                dependencies,
            )?;
            validate_program_expr(
                else_branch,
                globals,
                functions,
                owner,
                dependencies,
            )?;
        }
        Expr::Call {
            target,
            arguments,
            result,
            ..
        } => {
            let callee_expression = target.callee();
            let signature = match target {
                CallTarget::Direct(function) => {
                    let Some(callee) = functions.get(*function) else {
                        return Err(IrError::InvalidExpression(format!(
                            "function index {function} is outside the program"
                        )));
                    };
                    callee.signature().clone()
                }
                CallTarget::Contract { signature, .. } => signature.as_ref().clone(),
                CallTarget::Indirect { callee, .. } => {
                    validate_program_expr(
                        callee,
                        globals,
                        functions,
                        owner,
                        dependencies,
                    )?;
                    match callee.result_type() {
                        Type::Function(signature) => *signature,
                        actual => {
                            return Err(IrError::InvalidExpression(format!(
                                "indirect callee has non-function type {actual}"
                            )));
                        }
                    }
                }
            };
            if arguments.len() != signature.fixed_parameter_count() {
                return Err(IrError::InvalidExpression(format!(
                    "call has {} arguments, expected {}",
                    arguments.len(),
                    signature.fixed_parameter_count()
                )));
            }
            for (index, argument) in arguments.iter().enumerate() {
                let Expr::Default { .. } = argument else {
                    continue;
                };
                if index < signature.parameters().len() {
                    return Err(IrError::InvalidExpression(
                        "default argument marker is only valid for a labelled parameter"
                            .to_owned(),
                    ));
                }
                if callee_expression.is_none()
                    && signature
                        .labelled()
                        .get(index.saturating_sub(signature.parameters().len()))
                        .is_some_and(|parameter| parameter.default().is_none())
                {
                    return Err(IrError::InvalidExpression(
                        "direct call default marker has no declaration default"
                            .to_owned(),
                    ));
                }
                if let Some(callee_expression) = callee_expression {
                    let summary = possible_function_targets(
                        callee_expression,
                        &BTreeMap::new(),
                        globals,
                        functions,
                        &mut BTreeSet::new(),
                        &mut BTreeSet::new(),
                    );
                    let labelled_index =
                        index.saturating_sub(signature.parameters().len());
                    if summary.known.iter().any(|target| {
                        functions
                            .get(*target)
                            .and_then(|function| {
                                function.signature().labelled().get(labelled_index)
                            })
                            .is_some_and(|parameter| parameter.default().is_none())
                    }) {
                        return Err(IrError::InvalidExpression(
                            "indirect call default marker has no declaration default"
                                .to_owned(),
                        ));
                    }
                }
            }
            for argument in arguments {
                validate_program_expr(
                    argument,
                    globals,
                    functions,
                    owner,
                    dependencies,
                )?;
            }
            let expected_parameters = signature.slot_types();
            // A generic callee's parameters are erased: the checker fixed one
            // instantiation, and each slot must admit what it receives.
            for (argument, expected) in arguments.iter().zip(expected_parameters) {
                let actual = argument.result_type();
                if !expected.admits(&actual) {
                    return Err(IrError::InvalidExpression(format!(
                        "call argument has type {actual}, expected {expected}"
                    )));
                }
            }
            if !signature.result().admits(result) {
                return Err(IrError::InvalidExpression(format!(
                    "call result declares {result}, callee returns {}",
                    signature.result()
                )));
            }
            let mut targets = BTreeSet::new();
            if let CallTarget::Direct(function) = target {
                targets.insert(*function);
            } else if let CallTarget::Contract {
                interface, member, ..
            } = target
            {
                targets.extend(contract_targets(functions, interface, member));
            } else if let Some(callee_expression) = callee_expression {
                let summary = possible_function_targets(
                    callee_expression,
                    &BTreeMap::new(),
                    globals,
                    functions,
                    &mut BTreeSet::new(),
                    &mut BTreeSet::new(),
                );
                if let Some(function_hint) = target.hint() {
                    if summary.has_closure
                        || (!summary.known.is_empty()
                            && (summary.unknown
                                || summary.known != BTreeSet::from([function_hint])))
                    {
                        return Err(IrError::InvalidExpression(
                            "function hint does not match indirect callee".to_owned(),
                        ));
                    }
                    targets.insert(function_hint);
                } else {
                    targets.extend(summary.known);
                }
            }
            for dependency_target in targets {
                if functions.get(dependency_target).is_none() {
                    return Err(IrError::InvalidExpression(format!(
                        "function index {dependency_target} is outside the program"
                    )));
                }
                if let Some(owner) = owner {
                    let Some(edges) =
                        dependencies.get_mut(owner.node_index(globals.len()))
                    else {
                        return Err(IrError::InvalidExpression(format!(
                            "dependency owner {owner:?} is outside the program"
                        )));
                    };
                    edges.insert(DependencyNode::Function(dependency_target));
                }
            }
        }
    }
    Ok(())
}

/// Whether a slot of this type may hold a function value: a function type,
/// or a generic parameter, which a call may instantiate to one.
fn holds_function(value_type: &Type) -> bool {
    matches!(value_type, Type::Function(_) | Type::Param(_))
}

/// Whether a body leaves its activation through a `return` of a value that
/// holds a function. Such a value reaches the caller by a path the result
/// summaries do not follow, so the body's result is unknown. A closure body
/// has its own activation, which a `return` inside it does not leave.
fn returns_function_value(expression: &Expr) -> bool {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        match expression {
            Expr::Return { value, .. } => {
                if holds_function(&value.result_type()) {
                    return true;
                }
                pending.push(value);
            }
            Expr::Closure { captures, .. } => pending.extend(captures),
            _ => pending.extend(nominal::children(expression)),
        }
    }
    false
}

#[derive(Clone, Debug, Default)]
struct FunctionTargetSummary {
    known: BTreeSet<usize>,
    unknown: bool,
    has_closure: bool,
}

impl FunctionTargetSummary {
    fn known(function: usize) -> Self {
        Self {
            known: BTreeSet::from([function]),
            unknown: false,
            has_closure: false,
        }
    }

    fn unknown() -> Self {
        Self {
            known: BTreeSet::new(),
            unknown: true,
            has_closure: false,
        }
    }

    fn closure() -> Self {
        Self {
            known: BTreeSet::new(),
            unknown: false,
            has_closure: true,
        }
    }

    fn union(&mut self, other: Self) {
        self.known.extend(other.known);
        self.unknown |= other.unknown;
        self.has_closure |= other.has_closure;
    }
}

fn possible_function_targets(
    expression: &Expr,
    aliases: &BTreeMap<usize, FunctionTargetSummary>,
    globals: &[CheckedGlobal],
    functions: &[CheckedFunction],
    visiting: &mut BTreeSet<usize>,
    visiting_globals: &mut BTreeSet<usize>,
) -> FunctionTargetSummary {
    match expression {
        // Constructions are never function values; a projected field may hold
        // any function stored into a record, so its targets are unbounded.
        Expr::Record { .. }
        | Expr::Variant { .. }
        | Expr::Wrap { .. }
        | Expr::Widen { .. }
        | Expr::Tuple { .. }
        | Expr::Array { .. }
        | Expr::Dict { .. }
        | Expr::Lookup { .. } => FunctionTargetSummary::default(),
        Expr::Project { .. }
        | Expr::TupleProject { .. }
        | Expr::Try { .. }
        | Expr::Return { .. } => FunctionTargetSummary::unknown(),
        Expr::Function { function, .. } => FunctionTargetSummary::known(*function),
        // A closure is a distinct runtime callable, even when its body
        // returns a module function.  It therefore cannot be summarized as
        // that function's identity for a module-level tail transfer.
        Expr::Closure { .. } => FunctionTargetSummary::closure(),
        Expr::Match { arms, .. } => {
            let mut summary = FunctionTargetSummary::default();
            for arm in arms {
                summary.union(possible_function_targets(
                    &arm.body,
                    aliases,
                    globals,
                    functions,
                    visiting,
                    visiting_globals,
                ));
            }
            summary
        }
        Expr::If {
            then_branch,
            else_branch,
            ..
        } => {
            let then_targets = possible_function_targets(
                then_branch,
                aliases,
                globals,
                functions,
                visiting,
                visiting_globals,
            );
            let else_targets = possible_function_targets(
                else_branch,
                aliases,
                globals,
                functions,
                visiting,
                visiting_globals,
            );
            let mut summary = then_targets;
            summary.union(else_targets);
            summary
        }
        Expr::Let {
            slot, value, body, ..
        } => {
            let value_targets = possible_function_targets(
                value,
                aliases,
                globals,
                functions,
                visiting,
                visiting_globals,
            );
            let mut nested = aliases.clone();
            if let Some(slot) = slot {
                nested.insert(*slot, value_targets.clone());
            }
            let body_targets = possible_function_targets(
                body,
                &nested,
                globals,
                functions,
                visiting,
                visiting_globals,
            );
            FunctionTargetSummary {
                known: body_targets.known,
                unknown: body_targets.unknown,
                has_closure: body_targets.has_closure,
            }
        }
        Expr::Sequence { expressions, .. } => expressions.last().map_or_else(
            FunctionTargetSummary::default,
            |expression| {
                possible_function_targets(
                    expression,
                    aliases,
                    globals,
                    functions,
                    visiting,
                    visiting_globals,
                )
            },
        ),
        Expr::Variable { slot, .. } => aliases
            .get(slot)
            .cloned()
            .unwrap_or_else(FunctionTargetSummary::unknown),
        Expr::Captured { .. } => FunctionTargetSummary::unknown(),
        Expr::Global { index, .. } => {
            let Some(global) = globals.get(*index) else {
                return FunctionTargetSummary::unknown();
            };
            if !visiting_globals.insert(*index) {
                return FunctionTargetSummary::unknown();
            }
            let result = possible_function_targets(
                global.initializer(),
                &BTreeMap::new(),
                globals,
                functions,
                visiting,
                visiting_globals,
            );
            visiting_globals.remove(index);
            result
        }
        Expr::Call {
            target, arguments, ..
        } => {
            let target_summary = match target {
                CallTarget::Indirect { callee, .. } => possible_function_targets(
                    callee,
                    aliases,
                    globals,
                    functions,
                    visiting,
                    visiting_globals,
                ),
                CallTarget::Direct(function) => FunctionTargetSummary::known(*function),
                CallTarget::Contract {
                    interface, member, ..
                } => FunctionTargetSummary {
                    known: contract_targets(functions, interface, member),
                    unknown: false,
                    has_closure: false,
                },
            };
            if target_summary.unknown || target_summary.has_closure {
                return target_summary;
            }
            let argument_targets = arguments
                .iter()
                .map(|argument| {
                    possible_function_targets(
                        argument,
                        aliases,
                        globals,
                        functions,
                        visiting,
                        visiting_globals,
                    )
                })
                .collect::<Vec<_>>();
            let mut result = FunctionTargetSummary::default();
            let targets = target_summary.known;
            for target in targets {
                if !visiting.insert(target) {
                    result.unknown = true;
                    continue;
                }
                let Some(function) = functions.get(target) else {
                    result.unknown = true;
                    continue;
                };
                let mut target_aliases = BTreeMap::new();
                for (slot, (argument, value_type)) in argument_targets
                    .iter()
                    .zip(function_signature_types(function.signature()))
                    .enumerate()
                {
                    if holds_function(&value_type) {
                        target_aliases.insert(slot, argument.clone());
                    }
                }
                let mut returned = possible_function_targets(
                    function.body(),
                    &target_aliases,
                    globals,
                    functions,
                    visiting,
                    visiting_globals,
                );
                if returns_function_value(function.body()) {
                    returned.unknown = true;
                }
                result.known.extend(returned.known);
                result.unknown |= returned.unknown;
                result.has_closure |= returned.has_closure;
                visiting.remove(&target);
            }
            result
        }
        // A registry operation that returns a function, such as `array.fold`
        // over functions, may return any function value.
        Expr::External { result, .. } if holds_function(result) => {
            FunctionTargetSummary::unknown()
        }
        Expr::Literal { .. } | Expr::External { .. } | Expr::Default { .. } => {
            FunctionTargetSummary::default()
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct FlowTargetSummary {
    known: BTreeSet<usize>,
    unknown: bool,
    is_function: bool,
    closures: Vec<FlowClosure>,
    closure_defaults: Vec<Vec<bool>>,
}

/// One closure value a callable summary may denote.
#[derive(Clone, Debug)]
struct FlowClosure {
    /// Stable identity of the closure expression: the address of its shared
    /// body, which every copy of that expression shares. Summaries compare
    /// closures by this identity, never by walking the body.
    id: usize,
    signature: FunctionSignature,
    body: Arc<Expr>,
    captures: Vec<FlowTargetSummary>,
}

impl PartialEq for FlowClosure {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.captures == other.captures
    }
}

impl Eq for FlowClosure {}

fn closure_id(body: &Arc<Expr>) -> usize {
    Arc::as_ptr(body).addr()
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FlowCallId {
    Function(usize),
    Closure(usize),
}

impl FlowTargetSummary {
    fn known_function(function: usize) -> Self {
        Self {
            known: BTreeSet::from([function]),
            unknown: false,
            is_function: true,
            closures: Vec::new(),
            closure_defaults: Vec::new(),
        }
    }

    fn unknown_function() -> Self {
        Self {
            known: BTreeSet::new(),
            unknown: true,
            is_function: true,
            closures: Vec::new(),
            closure_defaults: Vec::new(),
        }
    }

    fn union(&mut self, other: &Self) {
        self.known.extend(&other.known);
        self.unknown |= other.unknown;
        self.is_function |= other.is_function;
        for closure in &other.closures {
            if let Some(existing) = self
                .closures
                .iter_mut()
                .find(|existing| existing.id == closure.id)
            {
                for (capture, additional) in
                    existing.captures.iter_mut().zip(&closure.captures)
                {
                    capture.union(additional);
                }
            } else {
                self.closures.push(closure.clone());
            }
        }
        for defaults in &other.closure_defaults {
            if !self.closure_defaults.contains(defaults) {
                self.closure_defaults.push(defaults.clone());
            }
        }
    }
}

struct CallFlow<'a> {
    globals: &'a [CheckedGlobal],
    functions: &'a [CheckedFunction],
    global_returns: Vec<FlowTargetSummary>,
    function_returns: Vec<FlowTargetSummary>,
    parameter_targets: Vec<Vec<FlowTargetSummary>>,
    parameter_sources: Vec<Vec<bool>>,
    dependencies: Vec<BTreeSet<DependencyNode>>,
    unresolved: BTreeSet<DependencyNode>,
    active_closures: BTreeSet<usize>,
    /// Return summaries read while analyzing the current owner:
    /// `Global(g)` for a global's value and `Function(f)` for a function's
    /// result. The worklist re-analyzes an owner when one of them changes.
    reads: RefCell<BTreeSet<DependencyNode>>,
    /// Functions whose parameter summaries the current owner widened.
    widened_parameters: BTreeSet<usize>,
    /// What an unknown call target stands for: every function named as a
    /// value anywhere in the program and every closure, whose captured
    /// callables are themselves unknown. The over-approximation keeps a call
    /// through data, such as a function stored in a record or an array,
    /// callable while call edges stay sound (gap G12).
    escaping: FlowTargetSummary,
}

struct CallAnalysis {
    dependencies: Vec<BTreeSet<DependencyNode>>,
    unresolved: BTreeSet<DependencyNode>,
    reachable: BTreeSet<DependencyNode>,
}

fn analyze_call_flow(
    globals: &[CheckedGlobal],
    functions: &[CheckedFunction],
    entry: usize,
) -> Result<CallAnalysis, IrError> {
    let analysis = analyze_call_flow_with_entry(globals, functions, Some(entry))?;
    if analysis
        .unresolved
        .iter()
        .any(|dependency| analysis.reachable.contains(dependency))
    {
        return Err(IrError::RecursiveCall(
            "indirect call target is not statically bounded before Step 9".to_owned(),
        ));
    }
    Ok(analysis)
}

fn analyze_initializer_call_flow(
    globals: &[CheckedGlobal],
    functions: &[CheckedFunction],
) -> Result<CallAnalysis, IrError> {
    let analysis = analyze_call_flow_with_entry(globals, functions, None)?;
    if analysis
        .unresolved
        .iter()
        .any(|dependency| analysis.reachable.contains(dependency))
    {
        return Err(IrError::RecursiveCall(
            "module initializer call flow is not statically bounded".to_owned(),
        ));
    }
    Ok(analysis)
}

fn analyze_call_flow_with_entry(
    globals: &[CheckedGlobal],
    functions: &[CheckedFunction],
    entry: Option<usize>,
) -> Result<CallAnalysis, IrError> {
    let mut flow = CallFlow {
        globals,
        functions,
        global_returns: vec![FlowTargetSummary::default(); globals.len()],
        function_returns: vec![FlowTargetSummary::default(); functions.len()],
        parameter_targets: functions
            .iter()
            .map(|function| {
                vec![
                    FlowTargetSummary::default();
                    function.signature().fixed_parameter_count()
                ]
            })
            .collect(),
        parameter_sources: functions
            .iter()
            .map(|function| vec![false; function.signature().fixed_parameter_count()])
            .collect(),
        dependencies: vec![BTreeSet::new(); globals.len() + functions.len()],
        unresolved: BTreeSet::new(),
        active_closures: BTreeSet::new(),
        reads: RefCell::new(BTreeSet::new()),
        widened_parameters: BTreeSet::new(),
        escaping: escaping_targets(globals, functions),
    };
    let roots = (0..globals.len())
        .map(DependencyNode::Global)
        .chain(entry.map(DependencyNode::Function))
        .collect::<Vec<_>>();
    let mut reachable = dependency_closure_from_roots(
        &flow.dependencies,
        globals.len(),
        roots.iter().copied(),
    )?;

    if let Some((entry, function)) =
        entry.and_then(|entry| functions.get(entry).map(|function| (entry, function)))
    {
        for (slot, value_type) in
            function_signature_types(function.signature()).enumerate()
        {
            if holds_function(&value_type)
                && let Some(summary) = flow
                    .parameter_targets
                    .get_mut(entry)
                    .and_then(|parameters| parameters.get_mut(slot))
            {
                *summary = FlowTargetSummary::unknown_function();
                if let Some(source) = flow
                    .parameter_sources
                    .get_mut(entry)
                    .and_then(|parameters| parameters.get_mut(slot))
                {
                    *source = true;
                }
            }
        }
    }

    // Worklist over owners. An owner's return summary and its call and
    // dependency edges are recomputed only when something it read changed:
    // another owner's return summary, or its own parameter summaries.
    let owners = (0..globals.len())
        .map(DependencyNode::Global)
        .chain((0..functions.len()).map(DependencyNode::Function))
        .collect::<Vec<_>>();
    let mut dirty = owners.iter().copied().collect::<BTreeSet<_>>();
    let mut readers = BTreeMap::<DependencyNode, BTreeSet<DependencyNode>>::new();
    // Every re-analysis follows a strict widening of a finite summary; the
    // budget only guards against a defect in that argument.
    let mut budget = owners
        .len()
        .saturating_add(1)
        .saturating_mul(owners.len().saturating_add(1))
        .saturating_mul(8)
        .saturating_add(64);
    while let Some(owner) = dirty.pop_first() {
        budget = budget
            .checked_sub(1)
            .ok_or(IrError::CallFlowDidNotConverge)?;
        let (body, environment) = match owner {
            DependencyNode::Global(index) => (
                globals.get(index).map(CheckedGlobal::initializer),
                BTreeMap::new(),
            ),
            DependencyNode::Function(index) => (
                functions.get(index).map(CheckedFunction::body),
                flow.parameter_environment(index),
            ),
        };
        let Some(body) = body else {
            return Err(IrError::InvalidExpression(format!(
                "call-flow owner {owner:?} is outside the program"
            )));
        };

        flow.reads.borrow_mut().clear();
        let mut returned = flow.summary_expr(body, &environment, &[]);
        if returns_function_value(body) {
            returned.union(&FlowTargetSummary::unknown_function());
        }
        let slot = match owner {
            DependencyNode::Global(index) => flow.global_returns.get_mut(index),
            DependencyNode::Function(index) => flow.function_returns.get_mut(index),
        };
        if let Some(slot) = slot
            && *slot != returned
        {
            *slot = returned;
            dirty.extend(readers.get(&owner).into_iter().flatten().copied());
        }
        for read in flow.reads.take() {
            readers.entry(read).or_default().insert(owner);
        }

        if matches!(owner, DependencyNode::Function(_)) && !reachable.contains(&owner) {
            continue;
        }
        let row = owner.node_index(globals.len());
        let previous_dependencies = flow.dependencies.get_mut(row).map(std::mem::take);
        flow.unresolved.remove(&owner);
        flow.widened_parameters.clear();
        flow.collect_expr(body, Some(owner), &environment, &[])?;
        for read in flow.reads.take() {
            readers.entry(read).or_default().insert(owner);
        }
        dirty.extend(
            std::mem::take(&mut flow.widened_parameters)
                .into_iter()
                .map(DependencyNode::Function),
        );
        if previous_dependencies.as_ref() != flow.dependencies.get(row) {
            let next_reachable = dependency_closure_from_roots(
                &flow.dependencies,
                globals.len(),
                roots.iter().copied(),
            )?;
            dirty.extend(next_reachable.difference(&reachable).copied());
            reachable = next_reachable;
        }
    }
    Ok(CallAnalysis {
        dependencies: flow.dependencies,
        unresolved: flow.unresolved,
        reachable,
    })
}

fn dependency_closure_from_roots(
    dependencies: &[BTreeSet<DependencyNode>],
    global_count: usize,
    roots: impl IntoIterator<Item = DependencyNode>,
) -> Result<BTreeSet<DependencyNode>, IrError> {
    let mut reachable = BTreeSet::new();
    let mut pending = roots.into_iter().collect::<Vec<_>>();
    while let Some(dependency) = pending.pop() {
        if !reachable.insert(dependency) {
            continue;
        }
        let Some(edges) = dependencies.get(dependency.node_index(global_count)) else {
            return Err(IrError::InvalidExpression(format!(
                "dependency graph references node {dependency:?}"
            )));
        };
        pending.extend(edges.iter().copied());
    }
    Ok(reachable)
}

/// The environment for a `let` body. Call-flow environments track only
/// function-typed slots, and slots are never reused within an activation, so
/// a non-callable binding leaves the environment unchanged and is not copied.
fn bind_callable_slot<T>(
    environment: &BTreeMap<usize, T>,
    slot: Option<usize>,
    value: &Expr,
    summary: T,
) -> Option<BTreeMap<usize, T>>
where
    T: Clone,
{
    let slot = slot?;
    holds_function(&value.result_type()).then(|| {
        let mut nested = environment.clone();
        nested.insert(slot, summary);
        nested
    })
}

fn function_signature_types(
    signature: &FunctionSignature,
) -> impl Iterator<Item = Type> {
    signature.slot_types().into_iter()
}

fn function_argument_environment(
    signature: &FunctionSignature,
    arguments: &[FlowTargetSummary],
) -> BTreeMap<usize, FlowTargetSummary> {
    arguments
        .iter()
        .zip(function_signature_types(signature))
        .enumerate()
        .filter_map(|(slot, (argument, value_type))| {
            holds_function(&value_type).then(|| (slot, argument.clone()))
        })
        .collect()
}

impl<'a> CallFlow<'a> {
    fn parameter_environment(
        &self,
        function: usize,
    ) -> BTreeMap<usize, FlowTargetSummary> {
        let Some(checked) = self.functions.get(function) else {
            return BTreeMap::new();
        };
        let mut environment = BTreeMap::new();
        for (slot, value_type) in
            function_signature_types(checked.signature()).enumerate()
        {
            if holds_function(&value_type) {
                let summary = if self
                    .parameter_sources
                    .get(function)
                    .and_then(|parameters| parameters.get(slot))
                    .copied()
                    .unwrap_or(false)
                {
                    self.parameter_targets
                        .get(function)
                        .and_then(|parameters| parameters.get(slot))
                        .cloned()
                        .unwrap_or_else(FlowTargetSummary::unknown_function)
                } else {
                    FlowTargetSummary::unknown_function()
                };
                environment.insert(slot, summary);
            }
        }
        environment
    }

    fn summary_expr(
        &self,
        expression: &Expr,
        environment: &BTreeMap<usize, FlowTargetSummary>,
        captures: &[FlowTargetSummary],
    ) -> FlowTargetSummary {
        self.summary_expr_with_stack(
            expression,
            environment,
            captures,
            &mut BTreeSet::new(),
        )
    }

    fn summary_expr_with_stack(
        &self,
        expression: &Expr,
        environment: &BTreeMap<usize, FlowTargetSummary>,
        captures: &[FlowTargetSummary],
        visiting: &mut BTreeSet<FlowCallId>,
    ) -> FlowTargetSummary {
        match expression {
            Expr::Record { .. }
            | Expr::Variant { .. }
            | Expr::Wrap { .. }
            | Expr::Widen { .. }
            | Expr::Tuple { .. }
            | Expr::Array { .. }
            | Expr::Dict { .. }
            | Expr::Lookup { .. } => FlowTargetSummary::default(),
            Expr::Project { .. }
            | Expr::TupleProject { .. }
            | Expr::Try { .. }
            | Expr::Return { .. } => FlowTargetSummary::unknown_function(),
            Expr::Function { function, .. } => {
                FlowTargetSummary::known_function(*function)
            }
            Expr::Closure {
                signature,
                body,
                captures: closure_captures,
                ..
            } => {
                let closure_capture_summaries = closure_captures
                    .iter()
                    .map(|capture| {
                        self.summary_expr_with_stack(
                            capture,
                            environment,
                            captures,
                            visiting,
                        )
                    })
                    .collect::<Vec<_>>();
                FlowTargetSummary {
                    is_function: true,
                    closures: vec![FlowClosure {
                        id: closure_id(body),
                        signature: signature.clone(),
                        body: Arc::clone(body),
                        captures: closure_capture_summaries,
                    }],
                    closure_defaults: vec![
                        signature
                            .labelled()
                            .iter()
                            .map(|parameter| parameter.default().is_some())
                            .collect(),
                    ],
                    ..FlowTargetSummary::default()
                }
            }
            Expr::Variable {
                slot, value_type, ..
            } => {
                if !holds_function(value_type) {
                    return FlowTargetSummary::default();
                }
                environment
                    .get(slot)
                    .cloned()
                    .unwrap_or_else(FlowTargetSummary::unknown_function)
            }
            Expr::Captured {
                slot, value_type, ..
            } => {
                if !holds_function(value_type) {
                    return FlowTargetSummary::default();
                }
                captures
                    .get(*slot)
                    .cloned()
                    .unwrap_or_else(FlowTargetSummary::unknown_function)
            }
            Expr::Global {
                index, value_type, ..
            } => {
                if !holds_function(value_type) {
                    return FlowTargetSummary::default();
                }
                self.reads
                    .borrow_mut()
                    .insert(DependencyNode::Global(*index));
                self.global_returns
                    .get(*index)
                    .cloned()
                    .unwrap_or_else(FlowTargetSummary::unknown_function)
            }
            Expr::Let {
                slot, value, body, ..
            } => {
                let value_summary = self.summary_expr_with_stack(
                    value,
                    environment,
                    captures,
                    visiting,
                );
                match bind_callable_slot(environment, *slot, value, value_summary) {
                    Some(nested) => {
                        self.summary_expr_with_stack(body, &nested, captures, visiting)
                    }
                    None => self.summary_expr_with_stack(
                        body,
                        environment,
                        captures,
                        visiting,
                    ),
                }
            }
            Expr::Match { arms, .. } => {
                let mut summary = FlowTargetSummary::default();
                for arm in arms {
                    summary.union(&self.summary_expr_with_stack(
                        &arm.body,
                        environment,
                        captures,
                        visiting,
                    ));
                }
                summary
            }
            Expr::If {
                then_branch,
                else_branch,
                ..
            } => {
                let mut summary = self.summary_expr_with_stack(
                    then_branch,
                    environment,
                    captures,
                    visiting,
                );
                summary.union(&self.summary_expr_with_stack(
                    else_branch,
                    environment,
                    captures,
                    visiting,
                ));
                summary
            }
            Expr::Sequence { expressions, .. } => expressions.last().map_or_else(
                FlowTargetSummary::default,
                |expression| {
                    self.summary_expr_with_stack(
                        expression,
                        environment,
                        captures,
                        visiting,
                    )
                },
            ),
            Expr::Call {
                target,
                arguments,
                result,
                ..
            } => {
                if !holds_function(result) {
                    return FlowTargetSummary::default();
                }
                let mut targets = match target {
                    CallTarget::Direct(function) => {
                        FlowTargetSummary::known_function(*function)
                    }
                    CallTarget::Contract {
                        interface, member, ..
                    } => FlowTargetSummary {
                        known: contract_targets(self.functions, interface, member),
                        is_function: true,
                        ..FlowTargetSummary::default()
                    },
                    CallTarget::Indirect { callee, .. } => self
                        .summary_expr_with_stack(
                            callee,
                            environment,
                            captures,
                            visiting,
                        ),
                };
                if let Some(function_hint) = target.hint() {
                    targets.known.insert(function_hint);
                    targets.is_function = true;
                }
                let mut summary = FlowTargetSummary {
                    is_function: true,
                    unknown: targets.unknown,
                    ..FlowTargetSummary::default()
                };
                let argument_summaries = arguments
                    .iter()
                    .map(|argument| {
                        self.summary_expr_with_stack(
                            argument,
                            environment,
                            captures,
                            visiting,
                        )
                    })
                    .collect::<Vec<_>>();
                for closure in &targets.closures {
                    let closure_environment = function_argument_environment(
                        &closure.signature,
                        &argument_summaries,
                    );
                    let closure_id = FlowCallId::Closure(closure.id);
                    if !visiting.insert(closure_id.clone()) {
                        summary.unknown = true;
                        continue;
                    }
                    let mut returned = self.summary_expr_with_stack(
                        &closure.body,
                        &closure_environment,
                        &closure.captures,
                        visiting,
                    );
                    if returns_function_value(&closure.body) {
                        returned.union(&FlowTargetSummary::unknown_function());
                    }
                    summary.union(&returned);
                    visiting.remove(&closure_id);
                }
                for target in targets.known {
                    let function_id = FlowCallId::Function(target);
                    if !visiting.insert(function_id.clone()) {
                        self.reads
                            .borrow_mut()
                            .insert(DependencyNode::Function(target));
                        if let Some(returned) = self.function_returns.get(target) {
                            summary.union(returned);
                        } else {
                            summary.unknown = true;
                        }
                        continue;
                    }
                    let Some(function) = self.functions.get(target) else {
                        summary.unknown = true;
                        visiting.remove(&function_id);
                        continue;
                    };
                    let mut target_environment = BTreeMap::new();
                    for (slot, (argument, value_type)) in argument_summaries
                        .iter()
                        .zip(function_signature_types(function.signature()))
                        .enumerate()
                    {
                        if holds_function(&value_type) {
                            target_environment.insert(slot, argument.clone());
                        }
                    }
                    let mut returned = self.summary_expr_with_stack(
                        function.body(),
                        &target_environment,
                        &[],
                        visiting,
                    );
                    if returns_function_value(function.body()) {
                        returned.union(&FlowTargetSummary::unknown_function());
                    }
                    summary.union(&returned);
                    visiting.remove(&function_id);
                }
                summary
            }
            Expr::External { result, .. } if holds_function(result) => {
                FlowTargetSummary::unknown_function()
            }
            Expr::Literal { .. } | Expr::External { .. } | Expr::Default { .. } => {
                FlowTargetSummary::default()
            }
        }
    }

    fn collect_expr(
        &mut self,
        expression: &Expr,
        owner: Option<DependencyNode>,
        environment: &BTreeMap<usize, FlowTargetSummary>,
        captures: &[FlowTargetSummary],
    ) -> Result<(), IrError> {
        match expression {
            Expr::Literal { .. }
            | Expr::Default { .. }
            | Expr::Variable { .. }
            | Expr::Function { .. }
            | Expr::Captured { .. } => {}
            Expr::Global { index, .. } => {
                // A module read inside an invoked closure body belongs to the
                // caller: `(def x i32 (h))` depends on every global that the
                // closure stored in `h` reads. The static pass deliberately
                // skips closure bodies, so the flow pass records the edge.
                if let Some(owner) = owner {
                    let Some(dependencies) = self
                        .dependencies
                        .get_mut(owner.node_index(self.globals.len()))
                    else {
                        return Err(IrError::InvalidExpression(format!(
                            "dependency owner {owner:?} is outside the program"
                        )));
                    };
                    dependencies.insert(DependencyNode::Global(*index));
                }
            }
            Expr::External {
                intrinsic,
                arguments,
                result,
                origin,
            } => {
                for argument in arguments {
                    self.collect_expr(argument, owner, environment, captures)?;
                }
                // `array.fold` invokes its step operand. The accumulator and
                // the element are not followed, so the step's own function
                // parameters are unknown: the call is collected with no
                // operand summaries.
                if *intrinsic == external::CompilerIntrinsic::ArrayFold
                    && let [_, _, step] = arguments.as_slice()
                {
                    let invocation = Expr::Call {
                        target: CallTarget::Indirect {
                            callee: Box::new(step.clone()),
                            hint: None,
                        },
                        arguments: Vec::new(),
                        result: result.clone(),
                        tail: false,
                        origin: origin.clone(),
                    };
                    self.collect_expr(&invocation, owner, environment, captures)?;
                }
            }
            Expr::Record { .. }
            | Expr::Variant { .. }
            | Expr::Wrap { .. }
            | Expr::Widen { .. }
            | Expr::Try { .. }
            | Expr::Return { .. }
            | Expr::Project { .. }
            | Expr::Tuple { .. }
            | Expr::TupleProject { .. }
            | Expr::Array { .. }
            | Expr::Dict { .. }
            | Expr::Lookup { .. } => {
                for operand in expression.data_operands() {
                    self.collect_expr(operand, owner, environment, captures)?;
                }
                if let Some(owner) = owner {
                    for target in key_order_targets(expression, self.functions) {
                        if let Some(dependencies) = self
                            .dependencies
                            .get_mut(owner.node_index(self.globals.len()))
                        {
                            dependencies.insert(DependencyNode::Function(target));
                        }
                    }
                }
            }
            Expr::Closure {
                captures: closure_captures,
                ..
            } => {
                for capture in closure_captures {
                    self.collect_expr(capture, owner, environment, captures)?;
                }
            }
            Expr::Sequence { expressions, .. } => {
                for expression in expressions {
                    self.collect_expr(expression, owner, environment, captures)?;
                }
            }
            Expr::Let {
                slot, value, body, ..
            } => {
                self.collect_expr(value, owner, environment, captures)?;
                let value_summary = self.summary_expr(value, environment, captures);
                match bind_callable_slot(environment, *slot, value, value_summary) {
                    Some(nested) => {
                        self.collect_expr(body, owner, &nested, captures)?
                    }
                    None => self.collect_expr(body, owner, environment, captures)?,
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.collect_expr(scrutinee, owner, environment, captures)?;
                for arm in arms {
                    self.collect_expr(&arm.body, owner, environment, captures)?;
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_expr(condition, owner, environment, captures)?;
                self.collect_expr(then_branch, owner, environment, captures)?;
                self.collect_expr(else_branch, owner, environment, captures)?;
            }
            Expr::Call {
                target, arguments, ..
            } => {
                let callee = target.callee();
                if let Some(callee) = callee {
                    self.collect_expr(callee, owner, environment, captures)?;
                }
                for argument in arguments {
                    self.collect_expr(argument, owner, environment, captures)?;
                }
                let mut target_summary = match target {
                    CallTarget::Direct(function) => {
                        FlowTargetSummary::known_function(*function)
                    }
                    CallTarget::Contract {
                        interface, member, ..
                    } => FlowTargetSummary {
                        known: contract_targets(self.functions, interface, member),
                        is_function: true,
                        ..FlowTargetSummary::default()
                    },
                    CallTarget::Indirect { callee, .. } => {
                        self.summary_expr(callee, environment, captures)
                    }
                };
                if let Some(function_hint) = target.hint() {
                    target_summary.known.insert(function_hint);
                    target_summary.is_function = true;
                }
                if target_summary.unknown {
                    let escaping = self.escaping.clone();
                    target_summary.union(&escaping);
                }
                let argument_summaries = arguments
                    .iter()
                    .map(|argument| self.summary_expr(argument, environment, captures))
                    .collect::<Vec<_>>();
                for closure in &target_summary.closures {
                    let closure_id = closure.id;
                    // A closure already being walked on this path contributes
                    // its edges once; re-entering it adds none.
                    if !self.active_closures.insert(closure_id) {
                        continue;
                    }
                    let closure_environment = function_argument_environment(
                        &closure.signature,
                        &argument_summaries,
                    );
                    let closure_result = self.collect_expr(
                        &closure.body,
                        owner,
                        &closure_environment,
                        &closure.captures,
                    );
                    self.active_closures.remove(&closure_id);
                    closure_result?;
                }
                if !target_summary.closure_defaults.is_empty()
                    && let Some(callee) = callee
                    && let Type::Function(signature) = callee.result_type()
                {
                    for (index, argument) in arguments.iter().enumerate() {
                        if !matches!(argument, Expr::Default { .. }) {
                            continue;
                        }
                        let labelled_index =
                            index.saturating_sub(signature.parameters().len());
                        if target_summary.closure_defaults.iter().any(|defaults| {
                            defaults
                                .get(labelled_index)
                                .is_some_and(|has_default| !has_default)
                        }) {
                            return Err(IrError::InvalidExpression(
                                "indirect call default marker has no closure default"
                                    .to_owned(),
                            ));
                        }
                    }
                }
                for target in target_summary.known {
                    let Some(callee) = self.functions.get(target) else {
                        return Err(IrError::InvalidExpression(format!(
                            "function index {target} is outside the program"
                        )));
                    };
                    for (index, argument) in arguments.iter().enumerate() {
                        if !matches!(argument, Expr::Default { .. }) {
                            continue;
                        }
                        let labelled_index =
                            index.saturating_sub(callee.signature().parameters().len());
                        if callee
                            .signature()
                            .labelled()
                            .get(labelled_index)
                            .is_some_and(|parameter| parameter.default().is_none())
                        {
                            return Err(IrError::InvalidExpression(
                                "indirect call default marker has no declaration default"
                                    .to_owned(),
                            ));
                        }
                    }
                    if let Some(owner) = owner {
                        let Some(dependencies) = self
                            .dependencies
                            .get_mut(owner.node_index(self.globals.len()))
                        else {
                            return Err(IrError::InvalidExpression(format!(
                                "dependency owner {owner:?} is outside the program"
                            )));
                        };
                        dependencies.insert(DependencyNode::Function(target));
                    }
                    for (slot, (argument, value_type)) in argument_summaries
                        .iter()
                        .zip(function_signature_types(callee.signature()))
                        .enumerate()
                    {
                        if !holds_function(&value_type) {
                            continue;
                        }
                        let Some(target_parameters) =
                            self.parameter_targets.get_mut(target)
                        else {
                            return Err(IrError::InvalidExpression(format!(
                                "function index {target} is outside the program"
                            )));
                        };
                        let Some(target_parameter) = target_parameters.get_mut(slot)
                        else {
                            return Err(IrError::InvalidExpression(format!(
                                "function index {target} has no parameter slot {slot}"
                            )));
                        };
                        let was_source = self
                            .parameter_sources
                            .get(target)
                            .and_then(|parameters| parameters.get(slot))
                            .copied()
                            .unwrap_or(false);
                        let before = target_parameter.clone();
                        target_parameter.union(argument);
                        if let Some(source) = self
                            .parameter_sources
                            .get_mut(target)
                            .and_then(|parameters| parameters.get_mut(slot))
                        {
                            *source = true;
                        }
                        if !was_source && argument.unknown {
                            target_parameter.unknown = true;
                        }
                        if !was_source || *target_parameter != before {
                            self.widened_parameters.insert(target);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn validate_type_shape(value_type: &Type) -> Result<(), String> {
    if let Type::Function(signature) = value_type {
        validate_signature_shape(signature)?;
    }
    Ok(())
}

fn validate_signature_shape(signature: &FunctionSignature) -> Result<(), String> {
    for parameter in signature.parameters() {
        validate_type_shape(parameter)?;
    }
    let mut labels = BTreeSet::new();
    for parameter in signature.labelled() {
        if !labels.insert(parameter.name()) {
            return Err(format!(
                "function signature repeats labelled parameter `{}`",
                parameter.name()
            ));
        }
        validate_type_shape(&parameter.value_type())?;
        // An atom default has its own singleton type, which is one atom of
        // `atom`.
        if let Some(default) = parameter.default()
            && !default.ty().same_shape(&parameter.value_type())
            && !(matches!(default, Value::Atom(_))
                && parameter.value_type() == Type::Atom)
        {
            return Err(format!(
                "default for labelled parameter `{}` has type {}, expected {}",
                parameter.name(),
                default.ty(),
                parameter.value_type()
            ));
        }
    }
    if let Some(tail) = signature.variadic() {
        if !matches!(tail, Type::Array(_) | Type::Dict(_, _)) {
            return Err(format!("variadic tail type {tail} is not an array or dict"));
        }
        validate_type_shape(tail)?;
    }
    validate_type_shape(&signature.result())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallState {
    Unvisited,
    Visiting,
    Done,
}

/// Rejects a call marked as a tail transfer outside an activation-relative
/// tail position. Every call in tail position reuses the current activation
/// whatever its callee is (06-runtime), so the callee needs no analysis.
fn validate_tail_calls(expression: &Expr, tail_position: bool) -> Result<(), IrError> {
    match expression {
        Expr::Literal { .. }
        | Expr::Default { .. }
        | Expr::Variable { .. }
        | Expr::Global { .. }
        | Expr::Function { .. }
        | Expr::Captured { .. } => {}
        Expr::Record { .. }
        | Expr::Variant { .. }
        | Expr::Wrap { .. }
        | Expr::Widen { .. }
        | Expr::Try { .. }
        | Expr::Project { .. }
        | Expr::Tuple { .. }
        | Expr::TupleProject { .. }
        | Expr::Array { .. }
        | Expr::Dict { .. }
        | Expr::Lookup { .. } => {
            for operand in expression.data_operands() {
                validate_tail_calls(operand, false)?;
            }
        }
        // The operand of `return` hands its value to the caller of the
        // activation it exits, so it is in tail position there.
        Expr::Return { value, .. } => validate_tail_calls(value, true)?,
        Expr::External { arguments, .. } => {
            for argument in arguments {
                validate_tail_calls(argument, false)?;
            }
        }
        Expr::Closure { captures, body, .. } => {
            for capture in captures {
                validate_tail_calls(capture, false)?;
            }
            validate_tail_calls(body, true)?;
        }
        Expr::Sequence { expressions, .. } => {
            for (index, expression) in expressions.iter().enumerate() {
                validate_tail_calls(
                    expression,
                    tail_position && index + 1 == expressions.len(),
                )?;
            }
        }
        Expr::Let { value, body, .. } => {
            validate_tail_calls(value, false)?;
            validate_tail_calls(body, tail_position)?;
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            validate_tail_calls(scrutinee, false)?;
            for arm in arms {
                validate_tail_calls(&arm.body, tail_position)?;
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            validate_tail_calls(condition, false)?;
            validate_tail_calls(then_branch, tail_position)?;
            validate_tail_calls(else_branch, tail_position)?;
        }
        Expr::Call {
            target,
            arguments,
            tail,
            ..
        } => {
            if let Some(callee) = target.callee() {
                validate_tail_calls(callee, false)?;
            }
            for argument in arguments {
                validate_tail_calls(argument, false)?;
            }
            if *tail && !tail_position {
                return Err(IrError::InvalidExpression(
                    "tail call is outside an activation-relative tail position"
                        .to_owned(),
                ));
            }
        }
    }
    Ok(())
}

fn reject_global_initializer_cycles(
    dependencies: &[BTreeSet<DependencyNode>],
    global_count: usize,
) -> Result<(), IrError> {
    let mut states = vec![CallState::Unvisited; dependencies.len()];
    for index in 0..global_count {
        visit_dependency_graph(
            DependencyNode::Global(index),
            dependencies,
            global_count,
            &mut states,
            &mut Vec::new(),
        )?;
    }
    Ok(())
}

fn visit_dependency_graph(
    node: DependencyNode,
    dependencies: &[BTreeSet<DependencyNode>],
    global_count: usize,
    states: &mut [CallState],
    path: &mut Vec<DependencyNode>,
) -> Result<(), IrError> {
    let index = node.node_index(global_count);
    match states.get(index).copied() {
        Some(CallState::Done) => return Ok(()),
        Some(CallState::Visiting) => {
            let cycle_global = path
                .iter()
                .skip_while(|active| **active != node)
                .find_map(|active| match active {
                    DependencyNode::Global(index) => Some(*index),
                    DependencyNode::Function(_) => None,
                });
            if let Some(global) = cycle_global {
                return Err(IrError::GlobalInitializerCycle(global));
            }
            return Ok(());
        }
        Some(CallState::Unvisited) => {}
        None => {
            return Err(IrError::InvalidExpression(format!(
                "dependency graph references node {node:?}"
            )));
        }
    }
    let Some(state) = states.get_mut(index) else {
        return Err(IrError::InvalidExpression(format!(
            "dependency graph references node {node:?}"
        )));
    };
    *state = CallState::Visiting;
    path.push(node);
    let Some(edges) = dependencies.get(index) else {
        return Err(IrError::InvalidExpression(format!(
            "dependency graph has no node for {node:?}"
        )));
    };
    for dependency in edges {
        visit_dependency_graph(*dependency, dependencies, global_count, states, path)?;
    }
    path.pop();
    if let Some(state) = states.get_mut(index) {
        *state = CallState::Done;
    }
    Ok(())
}

fn canonical_expr(expression: &Expr) -> String {
    match expression {
        Expr::Try {
            value,
            value_type,
            exit_type,
            ..
        } => format!(
            "(record kind: @try type: {} exit: {} value: {})",
            canonical_type(value_type),
            canonical_type(exit_type),
            canonical_expr(value)
        ),
        Expr::Return { value, .. } => {
            format!("(record kind: @return value: {})", canonical_expr(value))
        }
        Expr::Widen {
            value_type,
            value,
            member,
            ..
        } => format!(
            "(record kind: @widen type: {}{} value: {})",
            canonical_type(value_type),
            member
                .map(|index| format!(" member: {index}u64"))
                .unwrap_or_default(),
            canonical_expr(value)
        ),
        Expr::Record {
            value_type, fields, ..
        } => format!(
            "(record kind: @record type: {} fields: (record{}))",
            canonical_type(value_type),
            fields
                .iter()
                .map(|(name, value)| format!(" {name}: {}", canonical_expr(value)))
                .collect::<String>()
        ),
        Expr::Variant {
            value_type,
            variant,
            payload,
            ..
        } => format!(
            "(record kind: @variant type: {} variant: @{variant}{})",
            canonical_type(value_type),
            payload
                .as_deref()
                .map(|payload| format!(" payload: {}", canonical_expr(payload)))
                .unwrap_or_default()
        ),
        Expr::Wrap {
            value_type, value, ..
        } => format!(
            "(record kind: @wrap type: {} value: {})",
            canonical_type(value_type),
            canonical_expr(value)
        ),
        Expr::Project {
            record,
            field,
            value_type,
            ..
        } => format!(
            "(record kind: @project field: @{field} result: {} record: {})",
            canonical_type(value_type),
            canonical_expr(record)
        ),
        Expr::Tuple {
            value_type,
            components,
            ..
        } => format!(
            "(record kind: @tuple type: {} components: (array{}))",
            canonical_type(value_type),
            canonical_operands(components.iter())
        ),
        Expr::TupleProject {
            tuple,
            index,
            value_type,
            ..
        } => format!(
            "(record kind: @tuple-project index: {index} result: {} tuple: {})",
            canonical_type(value_type),
            canonical_expr(tuple)
        ),
        Expr::Array {
            value_type,
            elements,
            ..
        } => format!(
            "(record kind: @array type: {} elements: (array{}))",
            canonical_type(value_type),
            canonical_operands(elements.iter())
        ),
        Expr::Dict {
            value_type,
            entries,
            key_order,
            ..
        } => format!(
            "(record kind: @dict type: {}{} entries: (array{}))",
            canonical_type(value_type),
            canonical_key_order(key_order.as_ref()),
            entries
                .iter()
                .map(|(key, value)| format!(
                    " (tuple {} {})",
                    canonical_expr(key),
                    canonical_expr(value)
                ))
                .collect::<String>()
        ),
        Expr::Lookup {
            collection,
            key,
            value_type,
            key_order,
            ..
        } => format!(
            "(record kind: @lookup result: {}{} collection: {} key: {})",
            canonical_type(value_type),
            canonical_key_order(key_order.as_ref()),
            canonical_expr(collection),
            canonical_expr(key)
        ),
        Expr::Literal { value, .. } => format!(
            "(record kind: @literal type: {} value: {})",
            canonical_type(&value.ty()),
            value.canonical_vibon()
        ),
        Expr::External {
            intrinsic,
            arguments,
            result,
            ..
        } => format!(
            "(record kind: @external symbol: \"{}\" result: {} arguments: {})",
            intrinsic.symbol(),
            canonical_type(result),
            canonical_array(&arguments.iter().map(canonical_expr).collect::<Vec<_>>())
        ),
        Expr::Default { value_type, .. } => format!(
            "(record kind: @default type: {})",
            canonical_type(value_type)
        ),
        Expr::Sequence { expressions, .. } => {
            let values = expressions.iter().map(canonical_expr).collect::<Vec<_>>();
            format!(
                "(record kind: @sequence type: {} values: {})",
                canonical_type(&expression.result_type()),
                canonical_array(&values)
            )
        }
        Expr::Variable {
            slot, value_type, ..
        } => format!(
            "(record kind: @variable slot: {}u64 type: {})",
            slot,
            canonical_type(value_type)
        ),
        Expr::Global {
            index, value_type, ..
        } => format!(
            "(record kind: @global index: {}u64 type: {})",
            index,
            canonical_type(value_type)
        ),
        Expr::Function {
            function,
            signature,
            ..
        } => format!(
            "(record kind: @function function: {}u64 type: {} result: {})",
            function,
            canonical_function_signature(signature),
            canonical_type(&signature.result())
        ),
        Expr::Captured {
            slot, value_type, ..
        } => format!(
            "(record kind: @captured slot: {}u64 type: {})",
            slot,
            canonical_type(value_type)
        ),
        Expr::Closure {
            signature,
            captures,
            body,
            ..
        } => {
            let capture_values =
                captures.iter().map(canonical_expr).collect::<Vec<_>>();
            format!(
                "(record kind: @closure type: {} captures: {} body: {})",
                canonical_function_signature(signature),
                canonical_array(&capture_values),
                canonical_expr(body)
            )
        }
        Expr::Let {
            slot, value, body, ..
        } => {
            let slot =
                slot.map_or_else(|| "void".to_owned(), |slot| format!("{slot}u64"));
            format!(
                "(record kind: @let slot: {slot} value: {} body: {})",
                canonical_expr(value),
                canonical_expr(body)
            )
        }
        Expr::Match {
            scrutinee,
            arms,
            value_type,
            ..
        } => format!(
            "(record kind: @match type: {} scrutinee: {} arms: (array{}))",
            canonical_type(value_type),
            canonical_expr(scrutinee),
            arms.iter()
                .map(|arm| format!(
                    " (record pattern: {} body: {})",
                    arm.pattern.canonical_vibon(),
                    canonical_expr(&arm.body)
                ))
                .collect::<String>()
        ),
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => format!(
            "(record kind: @if condition: {} then: {} else: {})",
            canonical_expr(condition),
            canonical_expr(then_branch),
            canonical_expr(else_branch)
        ),
        Expr::Call {
            target,
            arguments,
            result,
            tail,
            ..
        } => {
            let values = arguments.iter().map(canonical_expr).collect::<Vec<_>>();
            let tail_field = if *tail { " tail: true" } else { "" };
            match target {
                CallTarget::Indirect { callee, hint } => {
                    let function_field = hint
                        .map(|function| format!(" function: {function}u64"))
                        .unwrap_or_default();
                    format!(
                        "(record kind: @call callee: {}{}{} result: {} arguments: {})",
                        canonical_expr(callee),
                        function_field,
                        tail_field,
                        canonical_type(result),
                        canonical_array(&values)
                    )
                }
                CallTarget::Direct(function) => format!(
                    "(record kind: @call function: {}u64{} result: {} arguments: {})",
                    function,
                    tail_field,
                    canonical_type(result),
                    canonical_array(&values)
                ),
                CallTarget::Contract {
                    interface,
                    member,
                    receiver,
                    closed,
                    ..
                } => format!(
                    "(record kind: @call contract: @{} member: @{member} receiver: {receiver}u64{}{} result: {} arguments: {})",
                    interface.path(),
                    closed
                        .map(|closed| format!(" closed: @{}", closed.symbol()))
                        .unwrap_or_default(),
                    tail_field,
                    canonical_type(result),
                    canonical_array(&values)
                ),
            }
        }
    }
}

/// Orders anonymous union members canonically: by the bytes of their
/// canonical type encoding, removing nothing. Anonymous union identity
/// ignores written order.
#[must_use]
pub fn canonical_union(mut members: Vec<Type>) -> Vec<Type> {
    members.sort_by_cached_key(canonical_type);
    members
}

/// The canonical type encoding of `docs/spec/06-runtime.md`.
#[must_use]
pub fn canonical_type(value: &Type) -> String {
    match value {
        Type::Function(signature) => canonical_function_signature(signature),
        Type::Declared(id) => format!("@{}", id.path()),
        Type::Interface(id, arguments) if arguments.is_empty() => {
            format!("@{}", id.path())
        }
        Type::Applied(id, arguments) | Type::Interface(id, arguments) => format!(
            "(record type: @{} arguments: (array{}))",
            id.path(),
            arguments
                .iter()
                .map(|argument| format!(" {}", canonical_type(argument)))
                .collect::<String>()
        ),
        Type::Param(name) => format!("(record type: @param name: @{name})"),
        Type::Tuple(values) => canonical_builtin_applied("tuple", values.iter()),
        Type::Array(element) => {
            canonical_builtin_applied("array", [element.as_ref()].into_iter())
        }
        Type::Dict(key, value) => canonical_builtin_applied(
            "dict",
            [key.as_ref(), value.as_ref()].into_iter(),
        ),
        Type::Record(fields) => format!(
            "(record type: @record fields: (record{}))",
            canonical_type_members(fields)
        ),
        Type::Enum(variants) => format!(
            "(record type: @enum variants: (record{}))",
            canonical_type_members(variants)
        ),
        Type::Union(members) => format!(
            "(record type: @union members: (array{}))",
            members
                .iter()
                .map(|member| format!(" {}", canonical_type(member)))
                .collect::<String>()
        ),
        Type::AtomSingleton(name) => {
            format!("(record type: @atom-singleton atom: @{name})")
        }
        _ => format!("@{}", value.as_str()),
    }
}

/// `(record type: @name arguments: (array T...))` for a builtin type.
fn canonical_builtin_applied<'a>(
    name: &str,
    arguments: impl Iterator<Item = &'a Type>,
) -> String {
    format!(
        "(record type: @{name} arguments: (array{}))",
        arguments
            .map(|argument| format!(" {}", canonical_type(argument)))
            .collect::<String>()
    )
}

fn canonical_type_members(members: &[(String, Type)]) -> String {
    members
        .iter()
        .map(|(name, value)| format!(" {name}: {}", canonical_type(value)))
        .collect()
}

fn canonical_function_signature(signature: &FunctionSignature) -> String {
    let parameters = signature
        .parameters()
        .iter()
        .map(canonical_type)
        .collect::<Vec<_>>();
    // Labelled parameters bind by name, so their order is not part of the
    // type: the encoding lists them by name.
    let mut labelled = signature
        .labelled()
        .iter()
        .map(|parameter| {
            (
                parameter.name().to_owned(),
                canonical_type(&parameter.value_type()),
            )
        })
        .collect::<Vec<_>>();
    labelled.sort();
    let labelled = labelled
        .iter()
        .map(|(name, value_type)| format!(" {name}: {value_type}"))
        .collect::<String>();
    let variadic = signature
        .variadic()
        .map(|tail| format!(" variadic: {}", canonical_type(tail)))
        .unwrap_or_default();
    format!(
        "(record type: @fn parameters: {} labelled: (record{labelled}){variadic} result: {})",
        canonical_array(&parameters),
        canonical_type(&signature.result())
    )
}

fn canonical_labelled_field(signature: &FunctionSignature) -> String {
    if signature.labelled().is_empty() {
        String::new()
    } else {
        format!(" labelled: {}", canonical_labelled_array(signature))
    }
}

fn canonical_labelled_array(signature: &FunctionSignature) -> String {
    let labelled = signature
        .labelled()
        .iter()
        .map(|parameter| {
            let mut output = format!(
                "(record name: @{} type: {}",
                parameter.name(),
                canonical_type(&parameter.value_type())
            );
            if let Some(default) = parameter.default() {
                output.push_str(&format!(" default: {}", default.canonical_vibon()));
            }
            output.push(')');
            output
        })
        .collect::<Vec<_>>();
    canonical_array(&labelled)
}

fn canonical_array(values: &[String]) -> String {
    if values.is_empty() {
        "(array)".to_owned()
    } else {
        format!("(array {})", values.join(" "))
    }
}

fn quote(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            control if control <= '\u{1f}' || control == '\u{7f}' => {
                output.push_str(&format!("\\u{{{:x}}}", u32::from(control)));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn canonical_character(value: char) -> String {
    match value {
        '\n' => "\\newline".to_owned(),
        '\r' => "\\return".to_owned(),
        ' ' => "\\space".to_owned(),
        '\t' => "\\tab".to_owned(),
        _ if value.is_control() || value.is_whitespace() => {
            format!("\\u{:04X}", value as u32)
        }
        _ => format!("\\{value}"),
    }
}

/// The canonical float serialization of `docs/spec/06-runtime.md` for a
/// binary32 value, without a suffix: `nan`, `inf`, `-inf`, or the shortest
/// digits that round-trip, in decimal notation with a fraction when the
/// magnitude is zero or in `[1e-4, 1e16)` and in scientific notation otherwise.
#[must_use]
pub fn canonical_f32_text(value: f32) -> String {
    if value.is_nan() {
        "nan".to_owned()
    } else if value.is_infinite() {
        if value > 0.0 { "inf" } else { "-inf" }.to_owned()
    } else {
        format!("{value:?}")
    }
}

/// [`canonical_f32_text`] for a binary64 value.
#[must_use]
pub fn canonical_f64_text(value: f64) -> String {
    if value.is_nan() {
        "nan".to_owned()
    } else if value.is_infinite() {
        if value > 0.0 { "inf" } else { "-inf" }.to_owned()
    } else {
        format!("{value:?}")
    }
}

/// ` e...` for each operand's canonical encoding, in order.
fn canonical_operands<'a>(operands: impl Iterator<Item = &'a Expr>) -> String {
    operands
        .map(|operand| format!(" {}", canonical_expr(operand)))
        .collect()
}

/// Every function named as a value and every closure in the program: what an
/// unknown call target may denote. A closure's captured callables are
/// unknown, so a call through one widens again to this same set.
/// The functions that implement `interface`'s `member`: the candidates of a
/// contract call.
/// The canonical `key-order:` field of a dict construction or lookup.
fn canonical_key_order(key_order: Option<&TypeId>) -> String {
    key_order
        .map(|interface| format!(" key-order: @{}", interface.path()))
        .unwrap_or_default()
}

/// The `compare` implementations a dict construction or lookup orders its keys
/// by, when its key type is not closed.
fn key_order_targets(
    expression: &Expr,
    functions: &[CheckedFunction],
) -> BTreeSet<usize> {
    match expression {
        Expr::Dict {
            key_order: Some(interface),
            ..
        }
        | Expr::Lookup {
            key_order: Some(interface),
            ..
        } => contract_targets(functions, interface, "compare"),
        _ => BTreeSet::new(),
    }
}

fn contract_targets(
    functions: &[CheckedFunction],
    interface: &TypeId,
    member: &str,
) -> BTreeSet<usize> {
    // The closed `equatable.equal` of a structure holding a user key answers
    // through that key's `ordered.compare`.
    let through_compare = interface.path() == "std.core.equatable" && member == "equal";
    functions
        .iter()
        .enumerate()
        .filter(|(_, function)| {
            function.implements().is_some_and(|implements| {
                (implements.interface == *interface && implements.member == member)
                    || (through_compare
                        && implements.interface.path() == "std.core.ordered"
                        && implements.member == "compare")
            })
        })
        .map(|(index, _)| index)
        .collect()
}

fn escaping_targets(
    globals: &[CheckedGlobal],
    functions: &[CheckedFunction],
) -> FlowTargetSummary {
    let mut summary = FlowTargetSummary {
        unknown: true,
        is_function: true,
        ..FlowTargetSummary::default()
    };
    let mut pending: Vec<&Expr> = globals
        .iter()
        .map(CheckedGlobal::initializer)
        .chain(functions.iter().map(CheckedFunction::body))
        .collect();
    while let Some(expression) = pending.pop() {
        match expression {
            Expr::Function { function, .. } => {
                summary.known.insert(*function);
            }
            Expr::Closure {
                signature,
                captures,
                body,
                ..
            } => {
                let id = closure_id(body);
                if !summary.closures.iter().any(|closure| closure.id == id) {
                    summary.closures.push(FlowClosure {
                        id,
                        signature: signature.clone(),
                        body: Arc::clone(body),
                        captures: vec![
                            FlowTargetSummary::unknown_function();
                            captures.len()
                        ],
                    });
                }
            }
            _ => {}
        }
        pending.extend(nominal::children(expression));
    }
    summary
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{
        ByteSpan, CheckedFunction, CheckedGlobal, CheckedProgram, Expr,
        FunctionSignature, IrError, SourceOrigin, Type, Value,
    };

    fn origin() -> SourceOrigin {
        SourceOrigin::new("ir-test.vib", ByteSpan::new(0, 1))
    }

    #[test]
    fn function_constructor_rejects_mismatched_if_branches() {
        let origin = origin();
        let body = Expr::if_expression(
            Expr::literal(Value::Bool(true), origin.clone()),
            Expr::literal(Value::I32(1), origin.clone()),
            Expr::literal(Value::Str("wrong".to_owned()), origin.clone()),
            origin.clone(),
        );
        let result = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), Type::I32),
            body,
            origin,
        );
        assert!(matches!(result, Err(IrError::InvalidExpression(_))));
    }

    #[test]
    fn function_constructor_rejects_unbound_variables() {
        let origin = origin();
        let result = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), Type::I32),
            Expr::variable(0, Type::I32, origin.clone()),
            origin,
        );
        assert!(matches!(result, Err(IrError::InvalidExpression(_))));
    }

    #[test]
    fn tail_call_is_explicit_in_checked_ir() {
        let origin = origin();
        let expression = Expr::tail_call(3, Vec::new(), Type::I32, origin.clone());
        assert!(expression.is_tail_call());
        assert!(super::canonical_expr(&expression).contains("tail: true"));
        let normal = Expr::call(3, Vec::new(), Type::I32, origin);
        assert!(!super::canonical_expr(&normal).contains("tail: true"));
    }

    #[test]
    fn program_constructor_rejects_tail_call_outside_sequence_tail_position() {
        let origin = origin();
        let signature = FunctionSignature::new(Vec::new(), Type::I32);
        let body = Expr::sequence(
            vec![
                Expr::tail_call(0, Vec::new(), Type::I32, origin.clone()),
                Expr::literal(Value::I32(1), origin.clone()),
            ],
            origin.clone(),
        );
        let function = CheckedFunction::new("answer", signature, body, origin)
            .expect("sequence shape is valid before tail-position validation");
        let error = CheckedProgram::try_new(vec![function], 0)
            .expect_err("non-final tail markers must not cross the checked boundary");
        assert!(error.to_string().contains("tail position"));
    }

    #[test]
    fn program_constructor_rejects_tail_call_in_an_operand() {
        let origin = origin();
        let leaf_signature = FunctionSignature::new(vec![Type::I32], Type::I32);
        let leaf = CheckedFunction::new(
            "leaf",
            leaf_signature,
            Expr::variable(0, Type::I32, origin.clone()),
            origin.clone(),
        )
        .expect("leaf function");
        let caller_signature = FunctionSignature::new(Vec::new(), Type::I32);
        let caller_body = Expr::call(
            0,
            vec![Expr::tail_call(1, Vec::new(), Type::I32, origin.clone())],
            Type::I32,
            origin.clone(),
        );
        let caller =
            CheckedFunction::new("caller", caller_signature, caller_body, origin)
                .expect("operand shape is valid before tail-position validation");
        let error = CheckedProgram::try_new(vec![leaf, caller], 1)
            .expect_err("tail markers in operands must not cross the checked boundary");
        assert!(error.to_string().contains("tail position"));
    }

    #[test]
    fn program_constructor_rejects_tail_call_in_a_condition() {
        let origin = origin();
        let signature = FunctionSignature::new(Vec::new(), Type::Bool);
        let body = Expr::if_expression(
            Expr::tail_call(0, Vec::new(), Type::Bool, origin.clone()),
            Expr::literal(Value::Bool(true), origin.clone()),
            Expr::literal(Value::Bool(false), origin.clone()),
            origin.clone(),
        );
        let function = CheckedFunction::new("answer", signature, body, origin)
            .expect("conditional shape is valid before tail-position validation");
        let error = CheckedProgram::try_new(vec![function], 0).expect_err(
            "tail markers in conditions must not cross the checked boundary",
        );
        assert!(error.to_string().contains("tail position"));
    }

    #[test]
    fn program_constructor_accepts_tail_call_in_a_closure_activation() {
        let origin = origin();
        let closure_signature = FunctionSignature::new(Vec::new(), Type::I32);
        let outer_signature = FunctionSignature::new(
            Vec::new(),
            Type::Function(Box::new(closure_signature.clone())),
        );
        let closure = Expr::closure(
            closure_signature,
            Vec::new(),
            Vec::new(),
            Expr::tail_call(0, Vec::new(), Type::I32, origin.clone()),
            0,
            origin.clone(),
        );
        let callee = CheckedFunction::new(
            "callee",
            FunctionSignature::new(Vec::new(), Type::I32),
            Expr::literal(Value::I32(7), origin.clone()),
            origin.clone(),
        )
        .expect("callee");
        let function = CheckedFunction::new("answer", outer_signature, closure, origin)
            .expect("closure shape is valid before tail-position validation");
        CheckedProgram::try_new(vec![callee, function], 1)
            .expect("a closure body is its own activation with a tail position");
    }

    #[test]
    fn program_constructor_rejects_tail_call_in_a_global_initializer() {
        let origin = origin();
        let function = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), Type::I32),
            Expr::literal(Value::I32(1), origin.clone()),
            origin.clone(),
        )
        .expect("function");
        let global = CheckedGlobal::new(
            "value",
            Type::I32,
            Expr::tail_call(0, Vec::new(), Type::I32, origin.clone()),
            origin.clone(),
        )
        .expect("global shape is valid before tail-position validation");
        let error =
            CheckedProgram::try_new_with_globals(vec![global], vec![function], 0)
                .expect_err("global initializers do not have tail-call activations");
        assert!(error.to_string().contains("tail position"));
    }

    #[test]
    fn program_constructor_accepts_tail_call_to_an_external_wrapper() {
        let origin = origin();
        let intrinsic = super::external::CompilerIntrinsic::TextLength;
        let external = CheckedFunction::new_external(
            "text.length",
            intrinsic.signature(&super::external::RoleTypes::default()),
            intrinsic,
            origin.clone(),
        )
        .expect("external wrapper");
        let caller = CheckedFunction::new(
            "caller",
            FunctionSignature::new(Vec::new(), Type::U64),
            Expr::tail_call(
                0,
                vec![Expr::literal(Value::Str("x".to_owned()), origin.clone())],
                Type::U64,
                origin.clone(),
            ),
            origin,
        )
        .expect("caller shape is valid before tail-target validation");
        // The wrapper creates no language activation, so the interpreter
        // invokes it normally; checked IR places no limit on the callee.
        CheckedProgram::try_new(vec![external, caller], 1)
            .expect("a tail marker names no callee restriction");
    }

    #[test]
    fn program_constructor_rejects_a_function_hint_naming_a_closure_callee() {
        let origin = origin();
        let signature = FunctionSignature::new(Vec::new(), Type::I32);
        let target = CheckedFunction::new(
            "target",
            signature.clone(),
            Expr::literal(Value::I32(1), origin.clone()),
            origin.clone(),
        )
        .expect("target");
        let callee = Expr::closure(
            signature.clone(),
            Vec::new(),
            Vec::new(),
            Expr::literal(Value::I32(2), origin.clone()),
            0,
            origin.clone(),
        );
        let caller = CheckedFunction::new(
            "caller",
            signature.clone(),
            Expr::indirect_tail_call(callee, 0, Vec::new(), Type::I32, origin.clone()),
            origin,
        )
        .expect("caller shape");
        let error = CheckedProgram::try_new(vec![target, caller], 1)
            .expect_err("a function hint cannot name a closure callee");
        assert!(
            error.to_string().contains("statically bounded")
                || error.to_string().contains("function hint")
        );
    }

    #[test]
    fn program_constructor_allows_tail_candidates_with_a_closure_branch() {
        let origin = origin();
        let signature = FunctionSignature::new(Vec::new(), Type::I32);
        let target = CheckedFunction::new(
            "target",
            signature.clone(),
            Expr::literal(Value::I32(1), origin.clone()),
            origin.clone(),
        )
        .expect("target");
        let closure = Expr::closure(
            signature.clone(),
            Vec::new(),
            Vec::new(),
            Expr::literal(Value::I32(2), origin.clone()),
            0,
            origin.clone(),
        );
        let callee = Expr::if_expression(
            Expr::literal(Value::Bool(true), origin.clone()),
            Expr::function(0, signature.clone(), origin.clone()),
            closure,
            origin.clone(),
        );
        let caller = CheckedFunction::new(
            "caller",
            signature.clone(),
            Expr::indirect_tail_call_with_hint(
                callee,
                None,
                Vec::new(),
                Type::I32,
                origin.clone(),
            ),
            origin,
        )
        .expect("caller shape");
        CheckedProgram::try_new(vec![target, caller], 1)
            .expect("a tail call may name a function or a closure branch");
    }

    #[test]
    fn program_constructor_rejects_a_function_hint_on_a_closure_returning_a_function() {
        let origin = origin();
        let target_signature = FunctionSignature::new(Vec::new(), Type::I32);
        let target = CheckedFunction::new(
            "target",
            target_signature.clone(),
            Expr::literal(Value::I32(1), origin.clone()),
            origin.clone(),
        )
        .expect("target");
        let closure_signature = FunctionSignature::new(
            Vec::new(),
            Type::Function(Box::new(target_signature.clone())),
        );
        let callee = Expr::closure(
            closure_signature,
            Vec::new(),
            Vec::new(),
            Expr::function(0, target_signature.clone(), origin.clone()),
            0,
            origin.clone(),
        );
        let caller = CheckedFunction::new(
            "caller",
            target_signature.clone(),
            Expr::indirect_tail_call(callee, 0, Vec::new(), Type::I32, origin.clone()),
            origin,
        )
        .expect("caller shape");
        let error = CheckedProgram::try_new(vec![target, caller], 1)
            .expect_err("closure identity cannot be replaced by its return target");
        assert!(matches!(
            error,
            IrError::InvalidExpression(_) | IrError::RecursiveCall(_)
        ));
    }

    #[test]
    fn function_constructor_rejects_standalone_default_markers() {
        let origin = origin();
        let result = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), Type::I32),
            Expr::default_value(Type::I32, origin.clone()),
            origin,
        );
        assert!(matches!(result, Err(IrError::InvalidExpression(_))));
    }

    #[test]
    fn program_constructor_rejects_forged_indirect_function_hints() {
        let origin = origin();
        let signature = FunctionSignature::new(Vec::new(), Type::I32);
        let first = CheckedFunction::new(
            "first",
            signature.clone(),
            Expr::literal(Value::I32(1), origin.clone()),
            origin.clone(),
        )
        .expect("first function");
        let second = CheckedFunction::new(
            "second",
            signature.clone(),
            Expr::literal(Value::I32(2), origin.clone()),
            origin.clone(),
        )
        .expect("second function");
        let caller_body = Expr::indirect_call(
            Expr::function(0, signature.clone(), origin.clone()),
            Some(1),
            Vec::new(),
            Type::I32,
            origin.clone(),
        );
        let caller = CheckedFunction::new("caller", signature, caller_body, origin)
            .expect("caller function");
        let result = CheckedProgram::try_new(vec![first, second, caller], 2);
        assert!(matches!(result, Err(IrError::InvalidExpression(_))));
    }

    #[test]
    fn program_constructor_rejects_hints_that_drop_conditional_targets() {
        let origin = origin();
        let signature = FunctionSignature::new(Vec::new(), Type::I32);
        let first = CheckedFunction::new(
            "first",
            signature.clone(),
            Expr::literal(Value::I32(1), origin.clone()),
            origin.clone(),
        )
        .expect("first function");
        let second = CheckedFunction::new(
            "second",
            signature.clone(),
            Expr::literal(Value::I32(2), origin.clone()),
            origin.clone(),
        )
        .expect("second function");
        let callee = Expr::if_expression(
            Expr::literal(Value::Bool(true), origin.clone()),
            Expr::function(0, signature.clone(), origin.clone()),
            Expr::function(1, signature.clone(), origin.clone()),
            origin.clone(),
        );
        let caller_body =
            Expr::indirect_call(callee, Some(1), Vec::new(), Type::I32, origin.clone());
        let caller = CheckedFunction::new("caller", signature, caller_body, origin)
            .expect("caller function");
        let result = CheckedProgram::try_new(vec![first, second, caller], 2);
        assert!(matches!(result, Err(IrError::InvalidExpression(_))));
    }

    #[test]
    fn program_constructor_over_approximates_unknown_parameters_despite_hints() {
        let origin = origin();
        let called_signature = FunctionSignature::new(Vec::new(), Type::I32);
        let first = CheckedFunction::new(
            "first",
            called_signature.clone(),
            Expr::literal(Value::I32(1), origin.clone()),
            origin.clone(),
        )
        .expect("first function");
        let second = CheckedFunction::new(
            "second",
            called_signature.clone(),
            Expr::literal(Value::I32(2), origin.clone()),
            origin.clone(),
        )
        .expect("second function");
        let caller_signature = FunctionSignature::new(
            vec![Type::Function(Box::new(called_signature.clone()))],
            Type::I32,
        );
        let caller_body = Expr::indirect_call(
            Expr::variable(
                0,
                Type::Function(Box::new(called_signature)),
                origin.clone(),
            ),
            Some(1),
            Vec::new(),
            Type::I32,
            origin.clone(),
        );
        let caller =
            CheckedFunction::new("caller", caller_signature, caller_body, origin)
                .expect("caller function");
        let result = CheckedProgram::try_new(vec![first, second, caller], 2);
        // The hint names one target, but an unknown parameter stands for every
        // escaping function; neither function escapes, so the call has no
        // bounded edge to trust and the program is still well formed.
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn program_constructor_rejects_default_markers_in_positional_slots() {
        let origin = origin();
        let callee_signature = FunctionSignature::with_labelled(
            vec![Type::I32],
            vec![super::LabelledParameter::new(
                "value",
                Type::I32,
                Some(Value::I32(7)),
            )],
            Type::I32,
        );
        let callee = CheckedFunction::new(
            "callee",
            callee_signature.clone(),
            Expr::variable(0, Type::I32, origin.clone()),
            origin.clone(),
        )
        .expect("callee function");
        let caller_signature = FunctionSignature::new(Vec::new(), Type::I32);
        let caller_body = Expr::call(
            0,
            vec![
                Expr::default_value(Type::I32, origin.clone()),
                Expr::literal(Value::I32(8), origin.clone()),
            ],
            Type::I32,
            origin.clone(),
        );
        let caller = CheckedFunction::new(
            "caller",
            caller_signature,
            caller_body,
            origin.clone(),
        )
        .expect("caller function");
        let result = CheckedProgram::try_new(vec![callee, caller], 1);
        assert!(matches!(result, Err(IrError::InvalidExpression(_))));
    }

    #[test]
    fn program_constructor_rejects_unavailable_indirect_defaults() {
        let origin = origin();
        let signature = FunctionSignature::with_labelled(
            Vec::new(),
            vec![super::LabelledParameter::new("value", Type::I32, None)],
            Type::I32,
        );
        let callee = CheckedFunction::new(
            "callee",
            signature.clone(),
            Expr::variable(0, Type::I32, origin.clone()),
            origin.clone(),
        )
        .expect("callee");
        let caller = CheckedFunction::new(
            "caller",
            FunctionSignature::new(Vec::new(), Type::I32),
            Expr::indirect_call(
                Expr::function(0, signature, origin.clone()),
                None,
                vec![Expr::default_value(Type::I32, origin.clone())],
                Type::I32,
                origin.clone(),
            ),
            origin.clone(),
        )
        .expect("caller");
        let result = CheckedProgram::try_new(vec![callee, caller], 1);
        assert!(matches!(result, Err(IrError::InvalidExpression(_))));
    }

    #[test]
    fn program_constructor_rejects_unavailable_closure_defaults() {
        let origin = origin();
        let signature = FunctionSignature::with_labelled(
            Vec::new(),
            vec![super::LabelledParameter::new("value", Type::I32, None)],
            Type::I32,
        );
        let closure = Expr::closure(
            signature.clone(),
            Vec::new(),
            Vec::new(),
            Expr::variable(0, Type::I32, origin.clone()),
            1,
            origin.clone(),
        );
        let caller = CheckedFunction::new(
            "caller",
            FunctionSignature::new(Vec::new(), Type::I32),
            Expr::indirect_call(
                closure,
                None,
                vec![Expr::default_value(Type::I32, origin.clone())],
                Type::I32,
                origin.clone(),
            ),
            origin.clone(),
        )
        .expect("caller");
        let result = CheckedProgram::try_new(vec![caller], 0);
        assert!(matches!(result, Err(IrError::InvalidExpression(_))));
    }

    #[test]
    fn closure_constructor_rejects_parameter_metadata_with_wrong_types() {
        let origin = origin();
        let signature = FunctionSignature::new(vec![Type::I32], Type::Str);
        let closure = Expr::closure(
            signature.clone(),
            vec![Type::Str],
            Vec::new(),
            Expr::variable(0, Type::Str, origin.clone()),
            1,
            origin.clone(),
        );
        let result = CheckedFunction::new(
            "entry",
            FunctionSignature::new(Vec::new(), Type::Function(Box::new(signature))),
            closure,
            origin,
        );
        assert!(matches!(result, Err(IrError::InvalidExpression(_))));
    }

    #[test]
    fn closure_constructor_rejects_invalid_labelled_defaults() {
        let origin = origin();
        let signature = FunctionSignature::with_labelled(
            Vec::new(),
            vec![super::LabelledParameter::new(
                "value",
                Type::I32,
                Some(Value::Str("wrong".to_owned())),
            )],
            Type::I32,
        );
        let closure = Expr::closure(
            signature.clone(),
            Vec::new(),
            Vec::new(),
            Expr::literal(Value::I32(1), origin.clone()),
            1,
            origin.clone(),
        );
        let result = CheckedFunction::new(
            "entry",
            FunctionSignature::new(Vec::new(), Type::Function(Box::new(signature))),
            closure,
            origin,
        );
        assert!(matches!(result, Err(IrError::InvalidExpression(_))));
    }

    #[test]
    fn program_constructor_rejects_bad_call_signatures() {
        let origin = origin();
        let callee = CheckedFunction::new(
            "callee",
            FunctionSignature::new(vec![Type::I32], Type::I32),
            Expr::variable(0, Type::I32, origin.clone()),
            origin.clone(),
        )
        .expect("callee");
        let caller = CheckedFunction::new(
            "caller",
            FunctionSignature::new(Vec::new(), Type::I32),
            Expr::call(0, Vec::new(), Type::I32, origin.clone()),
            origin,
        )
        .expect("caller shape is valid before program binding");
        let result = CheckedProgram::try_new(vec![callee, caller], 1);
        assert!(matches!(result, Err(IrError::InvalidExpression(_))));
    }

    #[test]
    fn program_constructor_rejects_direct_global_cycles() {
        let origin = origin();
        let global = super::CheckedGlobal::new(
            "value",
            Type::I32,
            Expr::global(0, Type::I32, origin.clone()),
            origin.clone(),
        )
        .expect("global shape");
        let function = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), Type::I32),
            Expr::literal(Value::I32(1), origin.clone()),
            origin.clone(),
        )
        .expect("function");
        let result =
            CheckedProgram::try_new_with_globals(vec![global], vec![function], 0);
        assert!(matches!(result, Err(IrError::GlobalInitializerCycle(_))));
    }

    #[test]
    fn program_constructor_rejects_global_cycles_through_functions() {
        let origin = origin();
        let global = super::CheckedGlobal::new(
            "value",
            Type::I32,
            Expr::call(0, Vec::new(), Type::I32, origin.clone()),
            origin.clone(),
        )
        .expect("global shape");
        let function = CheckedFunction::new(
            "read",
            FunctionSignature::new(Vec::new(), Type::I32),
            Expr::global(0, Type::I32, origin.clone()),
            origin.clone(),
        )
        .expect("function");
        let result =
            CheckedProgram::try_new_with_globals(vec![global], vec![function], 0);
        assert!(matches!(result, Err(IrError::GlobalInitializerCycle(_))));
    }

    #[test]
    fn program_constructor_allows_global_call_to_terminating_recursive_helper() {
        let origin = origin();
        let helper_signature = FunctionSignature::new(vec![Type::Bool], Type::I32);
        let global = super::CheckedGlobal::new(
            "value",
            Type::I32,
            Expr::call(
                0,
                vec![Expr::literal(Value::Bool(false), origin.clone())],
                Type::I32,
                origin.clone(),
            ),
            origin.clone(),
        )
        .expect("global shape");
        let helper = CheckedFunction::new(
            "helper",
            helper_signature,
            Expr::if_expression(
                Expr::variable(0, Type::Bool, origin.clone()),
                Expr::tail_call(
                    0,
                    vec![Expr::literal(Value::Bool(false), origin.clone())],
                    Type::I32,
                    origin.clone(),
                ),
                Expr::literal(Value::I32(1), origin.clone()),
                origin.clone(),
            ),
            origin.clone(),
        )
        .expect("recursive helper");
        let answer = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), Type::I32),
            Expr::global(0, Type::I32, origin.clone()),
            origin,
        )
        .expect("answer");

        let program =
            CheckedProgram::try_new_with_globals(vec![global], vec![helper, answer], 1);

        assert!(program.is_ok(), "{program:?}");
    }

    #[test]
    fn higher_order_global_cycles_reach_initializer_validation() {
        let origin = origin();
        let read_signature = FunctionSignature::new(Vec::new(), Type::I32);
        let apply_signature = FunctionSignature::new(
            vec![Type::Function(Box::new(read_signature.clone()))],
            Type::I32,
        );
        let global = super::CheckedGlobal::new(
            "value",
            Type::I32,
            Expr::call(
                0,
                vec![Expr::function(1, read_signature.clone(), origin.clone())],
                Type::I32,
                origin.clone(),
            ),
            origin.clone(),
        )
        .expect("global shape");
        let apply = CheckedFunction::new(
            "apply",
            apply_signature,
            Expr::indirect_call(
                Expr::variable(
                    0,
                    Type::Function(Box::new(read_signature.clone())),
                    origin.clone(),
                ),
                None,
                Vec::new(),
                Type::I32,
                origin.clone(),
            ),
            origin.clone(),
        )
        .expect("apply");
        let read = CheckedFunction::new(
            "read",
            read_signature,
            Expr::global(0, Type::I32, origin.clone()),
            origin.clone(),
        )
        .expect("read");
        let answer = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), Type::I32),
            Expr::global(0, Type::I32, origin.clone()),
            origin.clone(),
        )
        .expect("answer");
        let result = CheckedProgram::try_new_with_globals(
            vec![global],
            vec![apply, read, answer],
            2,
        );
        assert!(matches!(result, Err(IrError::GlobalInitializerCycle(_))));
    }

    #[test]
    fn entries_share_one_validated_module_set() {
        let functions = ["first", "second"]
            .into_iter()
            .map(|name| {
                CheckedFunction::new(
                    name,
                    FunctionSignature::new(Vec::new(), Type::I32),
                    Expr::literal(Value::I32(0), origin()),
                    origin(),
                )
                .expect("function")
            })
            .collect::<Vec<_>>();
        let set = super::CheckedModuleSet::try_new(Vec::new(), functions).expect("set");
        let first = CheckedProgram::for_entry(std::sync::Arc::clone(&set), 0)
            .expect("first entry");
        let second = CheckedProgram::for_entry(std::sync::Arc::clone(&set), 1)
            .expect("second entry");
        assert!(std::sync::Arc::ptr_eq(
            first.module_set(),
            second.module_set()
        ));
        assert_eq!(first.entry().name(), "first");
        assert_eq!(second.entry().name(), "second");
        assert_eq!(
            CheckedProgram::for_entry(set, 2),
            Err(IrError::InvalidEntry(2))
        );
    }
}
