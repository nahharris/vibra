//! The v0.0.1 checked IR: the types of a [`CheckedProgram`] that `vibra-lower`
//! reads, the declared-type and effect vocabulary, and the closed registries of
//! the compiler intrinsics, the native rows and the host operations.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use vibra_diagnostics_v1::ByteSpan;

mod constant;
mod effect;
mod nominal;
mod observed;
mod pattern;

pub use constant::Constant;
pub use effect::{EffectRoot, EffectRow, RowTail};
pub use nominal::{TypeBody, TypeDefinition, TypeId};
pub use observed::ObservedValue;
pub use pattern::{MatchArm, Pattern};

pub mod arena;
pub mod boundary;
pub mod external;
pub mod vocabulary;

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
                if !signature.effects().is_empty() {
                    write!(formatter, " effects: {}", signature.effects())?;
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
    default: Option<Constant>,
}

impl LabelledParameter {
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
    pub const fn default(&self) -> Option<&Constant> {
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
    /// The exact closed effect row of the function type. For a declaration
    /// it is the written or default-empty ceiling; for an effect operation it
    /// is the owner root together with the additive row.
    effects: EffectRow,
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
            effects: EffectRow::empty(),
        }
    }

    /// The exact closed effect row, which is part of the function type.
    #[must_use]
    pub const fn effects(&self) -> &EffectRow {
        &self.effects
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
            effects: self.effects.clone(),
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

    /// The numeric code of this assertion at the execution boundary: its
    /// place in [`Self::ALL`], counted from one.
    #[must_use]
    pub fn code(self) -> i32 {
        Self::ALL
            .into_iter()
            .zip(1..)
            .find_map(|(assertion, code)| (assertion == self).then_some(code))
            .unwrap_or_default()
    }

    /// Canonical source-level assertion identity.
    #[must_use]
    pub fn symbol(self) -> String {
        format!("@std.assert.{}", self.member())
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

    /// The deterministic literal spelling used by result observations.
    #[must_use]
    pub fn canonical_vibon(&self) -> String {
        match self {
            Self::Bool(value) => value.to_string(),
            Self::Void => "void".to_owned(),
            Self::Char(value) => canonical_character(*value),
            Self::Str(value) => canonical_string(value),
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
    /// A call to one host operation of the closed `vibra_v1` registry
    /// (`docs/spec/06-runtime.md`, "The host registry"). It names its row and
    /// no ID, offset, or instance.
    Host {
        /// The registry row.
        row: &'static external::host::HostRow,
        /// Checked operands in the row's parameter order.
        arguments: Vec<Self>,
        /// The checked `(result t e)` type.
        result: Type,
        /// The source origin of the call.
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
        /// The member's own type arguments at this call, one for each generic
        /// parameter of its `where:` clause in order, or none when the member
        /// is not generic. They are fixed by inference, the expected type, or
        /// a written `types:` list, and may name the enclosing activation's
        /// own generic parameters.
        member_types: Vec<Type>,
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

    /// The source origin of this expression.
    #[must_use]
    pub const fn origin(&self) -> &SourceOrigin {
        match self {
            Self::Literal { origin, .. }
            | Self::External { origin, .. }
            | Self::Host { origin, .. }
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
            Self::External { result, .. } | Self::Host { result, .. } => result.clone(),
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
            | Self::Host { .. }
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

    /// Number of immutable activation slots required by this expression.
    #[must_use]
    pub fn slot_count(&self) -> usize {
        match self {
            Self::Literal { .. }
            | Self::Default { .. }
            | Self::Global { .. }
            | Self::Function { .. }
            | Self::Captured { .. } => 0,
            Self::External { arguments, .. } | Self::Host { arguments, .. } => {
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
    native_body: Option<NativeBody>,
    test_assertion: Option<TestAssertion>,
    implements: Option<Implements>,
    effect_owner: Option<EffectRoot>,
}

/// The checked Vibra body of a native whose meaning applies a function value.
///
/// Such a native has no import, because a pure import cannot call back into
/// the module, so a module runs this body (`docs/spec/06-runtime.md`, "Native
/// implementations"). The reference interpreter keeps running its native code,
/// which the differential holds to the body. The body has the signature of the
/// wrapper that carries it, and it calls the wrapper itself to recurse.
#[derive(Clone, Debug, PartialEq, Eq)]
struct NativeBody {
    body: Expr,
    slot_count: usize,
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
    /// The generic parameter this function declares for each of the contract
    /// member's own generic parameters, in the contract's `where:` order, or
    /// `None` where the function declares none that matches. A
    /// [`CallTarget::Contract`] binds each to the call's `member_types`.
    pub member_generics: Vec<Option<String>>,
}

impl CheckedFunction {
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

    /// The checked Vibra body of a native whose meaning applies a function
    /// value, which a module runs where the interpreter runs the native. `None`
    /// for every other function.
    #[must_use]
    pub fn native_body(&self) -> Option<&Expr> {
        self.native_body.as_ref().map(|native| &native.body)
    }

    /// The body a module lowers for this function, and the number of
    /// immutable activation slots it needs: the checked Vibra body of a native
    /// that runs one, and the function's own body otherwise.
    #[must_use]
    pub fn module_body(&self) -> (&Expr, usize) {
        match &self.native_body {
            Some(native) => (&native.body, native.slot_count),
            None => (&self.body, self.slot_count),
        }
    }

    /// Whether this is one of the closed verified test assertion functions.
    #[must_use]
    pub const fn test_assertion(&self) -> Option<TestAssertion> {
        self.test_assertion
    }

    /// The contract member this function implements, if any.
    #[must_use]
    pub const fn implements(&self) -> Option<&Implements> {
        self.implements.as_ref()
    }

    /// The effect root that owns this function as an operation, if any.
    #[must_use]
    pub const fn effect_owner(&self) -> Option<&EffectRoot> {
        self.effect_owner.as_ref()
    }
}

/// Module values and functions shared by the program of every entry and test
/// in one checking scope.
#[derive(Debug, PartialEq, Eq)]
pub struct CheckedModuleSet {
    types: Vec<TypeDefinition>,
    globals: Vec<CheckedGlobal>,
    functions: Vec<CheckedFunction>,
}

impl CheckedModuleSet {
    /// The canonical `@types.v1` observation of the checked globals and
    /// functions, in deterministic checked order.
    #[must_use]
    pub fn canonical_vibon(&self) -> String {
        let mut output = String::from("(record\n  format: @types.v1\n");
        if !self.globals.is_empty() {
            output.push_str("  globals: (array\n");
            for global in &self.globals {
                output.push_str(&format!(
                    "    (record name: @{} type: {} body: {})\n",
                    declaration_path(&global.name),
                    canonical_type(&global.value_type),
                    canonical_expr(global.initializer()),
                ));
            }
            output.push_str("  )\n");
        }
        output.push_str("  functions: (array\n");
        for function in &self.functions {
            let parameters = function
                .signature
                .parameters()
                .iter()
                .map(canonical_type)
                .collect::<Vec<_>>();
            let body = canonical_expr(function.body());
            let native = function.native_body().map_or_else(String::new, |body| {
                format!(" native-body: {}", canonical_expr(body))
            });
            output.push_str(&format!(
                "    (record name: @{} params: {} result: {}{} body: {}{})\n",
                declaration_path(&function.name),
                canonical_array(&parameters),
                canonical_type(&function.signature.result()),
                canonical_labelled_field(&function.signature),
                body,
                native
            ));
        }
        output.push_str("  )\n)\n");
        output
    }

    /// Declared type definitions in deterministic checked order.
    #[must_use]
    pub fn types(&self) -> &[TypeDefinition] {
        &self.types
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

    /// Canonical typed-program observation of the static profile: the observation
    /// of the module set the entry selects from, which does not depend on the
    /// entry.
    #[must_use]
    pub fn canonical_vibon(&self) -> String {
        self.set.canonical_vibon()
    }
}

/// The dotted path of a declaration identity, as a declared type's identity
/// prints: `app.main.answer` for `@source@0.0.0/app.main.answer`. The package
/// is the checking scope's, so it is not part of what an observation names.
fn declaration_path(name: &str) -> &str {
    match name.strip_prefix('@') {
        Some(identity) => identity.split_once('/').map_or(name, |(_, path)| path),
        None => name,
    }
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
        Expr::Host {
            row,
            arguments,
            result,
            ..
        } => format!(
            "(record kind: @host symbol: \"{}\" result: {} arguments: {})",
            row.symbol,
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
                    member_types,
                    closed,
                    ..
                } => format!(
                    "(record kind: @call contract: @{} member: @{member} receiver: {receiver}u64{}{}{} result: {} arguments: {})",
                    interface.path(),
                    closed
                        .map(|closed| format!(" closed: @{}", closed.symbol()))
                        .unwrap_or_default(),
                    if member_types.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " types: {}",
                            canonical_array(
                                &member_types
                                    .iter()
                                    .map(canonical_type)
                                    .collect::<Vec<_>>()
                            )
                        )
                    },
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
        "(record type: @fn parameters: {} labelled: (record{labelled}){variadic} result: {} effects: {})",
        canonical_array(&parameters),
        canonical_type(&signature.result()),
        signature.effects().canonical_vibon()
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

/// The canonical Vibon array of already-rendered `values`.
#[must_use]
pub fn canonical_array(values: &[String]) -> String {
    if values.is_empty() {
        "(array)".to_owned()
    } else {
        format!("(array {})", values.join(" "))
    }
}

/// The canonical Vibon string literal of `value`: the quotes, the escapes of
/// `\`, `"`, and every control character.
#[must_use]
pub fn canonical_string(value: &str) -> String {
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

/// The one quiet NaN of binary32: the positive sign and a zero payload.
pub const CANONICAL_NAN_F32: u32 = 0x7FC0_0000;

/// The one quiet NaN of binary64: the positive sign and a zero payload.
pub const CANONICAL_NAN_F64: u64 = 0x7FF8_0000_0000_0000;

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

/// The canonical `key-order:` field of a dict construction or lookup.
fn canonical_key_order(key_order: Option<&TypeId>) -> String {
    key_order
        .map(|interface| format!(" key-order: @{}", interface.path()))
        .unwrap_or_default()
}
