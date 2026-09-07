//! The small, backend independent IR admitted by M2 Step 7.
//!
//! The type checker is the only workspace phase that constructs a
//! [`CheckedProgram`].  The interpreter consumes that type, rather than a
//! parsed syntax tree, which makes the checked-program boundary explicit.
//! This IR slice contains primitive values, immutable bindings, literal
//! sequences, conditionals, first-class function paths, owned closures, and
//! fixed/labelled calls; effects and collections belong to later steps.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use vibra_diagnostics::ByteSpan;

/// The closed compiler intrinsic registry admitted by M2.
pub mod external {
    use super::{FunctionSignature, PrimitiveType};

    /// The closed M2 compiler registry identity used by the runtime contract.
    ///
    /// `vibra_v1` is the version named by the v1 runtime specification.  The
    /// compiler registry and the host registry are separate namespaces, but
    /// share this stable toolchain ABI version.
    pub const REGISTRY_VERSION: &str = "vibra_v1";

    /// The semantic operation implemented by a compiler intrinsic.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum SemanticIdentity {
        /// Concatenate Unicode scalar sequences in order.
        UnicodeScalarConcatenation,
        /// Count Unicode scalars, rather than UTF-8 bytes.
        UnicodeScalarLength,
    }

    /// One pure, compiler-owned operation.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum CompilerIntrinsic {
        /// Concatenate two Unicode scalar sequences.
        TextConcat,
        /// Count Unicode scalars in a string.
        TextLength,
    }

    impl CompilerIntrinsic {
        /// The closed registry version for this operation.
        #[must_use]
        pub const fn registry_version(self) -> &'static str {
            REGISTRY_VERSION
        }

        /// The semantic contract implemented by this operation.
        #[must_use]
        pub const fn semantic_identity(self) -> SemanticIdentity {
            match self {
                Self::TextConcat => SemanticIdentity::UnicodeScalarConcatenation,
                Self::TextLength => SemanticIdentity::UnicodeScalarLength,
            }
        }

        /// The stable registry symbol.
        #[must_use]
        pub const fn symbol(self) -> &'static str {
            match self {
                Self::TextConcat => "text.concat",
                Self::TextLength => "text.length",
            }
        }

        /// The exact checked signature.
        #[must_use]
        pub fn signature(self) -> FunctionSignature {
            match self {
                Self::TextConcat => FunctionSignature::new(
                    vec![PrimitiveType::Str, PrimitiveType::Str],
                    PrimitiveType::Str,
                ),
                Self::TextLength => {
                    FunctionSignature::new(vec![PrimitiveType::Str], PrimitiveType::U64)
                }
            }
        }

        /// Resolves only a symbol in the closed registry.
        #[must_use]
        pub fn from_symbol(symbol: &str) -> Option<Self> {
            match symbol {
                "text.concat" => Some(Self::TextConcat),
                "text.length" => Some(Self::TextLength),
                _ => None,
            }
        }

        /// Every compiler intrinsic in canonical registry order.
        pub const ALL: [Self; 2] = [Self::TextConcat, Self::TextLength];
    }
}

/// One of the primitive types admitted by the M2 literal profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrimitiveType {
    /// Boolean values.
    Bool,
    /// The single successful-completion value.
    Void,
    /// Unicode scalar values.
    Char,
    /// Immutable Unicode scalar strings.
    Str,
    /// Immutable byte sequences.
    Bytes,
    /// Interned atom values.
    Atom,
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
}

impl PrimitiveType {
    /// The canonical source/type spelling.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Void => "void",
            Self::Char => "char",
            Self::Str => "str",
            Self::Bytes => "bytes",
            Self::Atom => "atom",
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
            _ => self == other,
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

impl fmt::Display for PrimitiveType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One labelled slot in a function value's call contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelledParameter {
    name: String,
    value_type: PrimitiveType,
    default: Option<Value>,
}

impl LabelledParameter {
    /// Creates a labelled slot.  Function type expressions use `None`; a
    /// declaration signature carries the typed literal default.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        value_type: PrimitiveType,
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
    pub fn value_type(&self) -> PrimitiveType {
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
    parameters: Vec<PrimitiveType>,
    labelled: Vec<LabelledParameter>,
    result: PrimitiveType,
}

impl FunctionSignature {
    /// Creates a signature from its checked primitive slots.
    #[must_use]
    pub fn new(parameters: Vec<PrimitiveType>, result: PrimitiveType) -> Self {
        Self {
            parameters,
            labelled: Vec::new(),
            result,
        }
    }

    /// Creates a signature with declaration-order labelled slots.
    #[must_use]
    pub fn with_labelled(
        parameters: Vec<PrimitiveType>,
        labelled: Vec<LabelledParameter>,
        result: PrimitiveType,
    ) -> Self {
        Self {
            parameters,
            labelled,
            result,
        }
    }

    /// Required positional parameter types in written order.
    #[must_use]
    pub fn parameters(&self) -> &[PrimitiveType] {
        &self.parameters
    }

    /// Labelled slots in declaration order.
    #[must_use]
    pub fn labelled(&self) -> &[LabelledParameter] {
        &self.labelled
    }

    /// Total fixed slots after defaults have been materialized.
    #[must_use]
    pub fn fixed_parameter_count(&self) -> usize {
        self.parameters.len().saturating_add(self.labelled.len())
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
    }

    /// The declared result type.
    #[must_use]
    pub fn result(&self) -> PrimitiveType {
        self.result.clone()
    }
}

/// A source identity and span carried by checked operands.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceOrigin {
    source_id: String,
    span: ByteSpan,
}

impl SourceOrigin {
    /// Creates an origin for one source document span.
    #[must_use]
    pub fn new(source_id: impl Into<String>, span: ByteSpan) -> Self {
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
    pub fn ty(&self) -> PrimitiveType {
        match self {
            Self::Bool(_) => PrimitiveType::Bool,
            Self::Void => PrimitiveType::Void,
            Self::Char(_) => PrimitiveType::Char,
            Self::Str(_) => PrimitiveType::Str,
            Self::Bytes(_) => PrimitiveType::Bytes,
            Self::Atom(_) => PrimitiveType::Atom,
            Self::I8(_) => PrimitiveType::I8,
            Self::I16(_) => PrimitiveType::I16,
            Self::I32(_) => PrimitiveType::I32,
            Self::I64(_) => PrimitiveType::I64,
            Self::U8(_) => PrimitiveType::U8,
            Self::U16(_) => PrimitiveType::U16,
            Self::U32(_) => PrimitiveType::U32,
            Self::U64(_) => PrimitiveType::U64,
            Self::F32(_) => PrimitiveType::F32,
            Self::F64(_) => PrimitiveType::F64,
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
            Self::F32(bits) => format_float(f32::from_bits(*bits), "f32"),
            Self::F64(bits) => format_float(f64::from_bits(*bits), "f64"),
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
        /// The source origin of the intrinsic call.
        origin: SourceOrigin,
    },
    /// A labelled argument whose default is resolved from the runtime callee.
    Default {
        /// The statically checked labelled slot type.
        value_type: PrimitiveType,
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
        value_type: PrimitiveType,
        /// The source origin of the name use.
        origin: SourceOrigin,
    },
    /// A module-level immutable value.
    Global {
        /// The program-global index.
        index: usize,
        /// The statically checked value type.
        value_type: PrimitiveType,
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
        parameters: Vec<PrimitiveType>,
        /// Expressions evaluated once when the closure is created.
        captures: Vec<Self>,
        /// Static types of the closure-environment slots.
        capture_types: Vec<PrimitiveType>,
        /// The lambda body, whose free names use [`Self::Captured`].
        body: Box<Self>,
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
        value_type: PrimitiveType,
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
    /// A fixed positional call to a checked function.
    Call {
        /// The function index in the containing checked program.
        function: usize,
        /// Arguments in declaration order.
        arguments: Vec<Self>,
        /// An indirect callee expression.  `None` preserves the compact
        /// direct-call representation used by the M2 Step 6 IR.
        callee: Option<Box<Self>>,
        /// A statically known target for an indirect call, when one exists.
        /// This preserves recursive-call dependency checking through a local
        /// function alias without treating an arbitrary lambda as a call to
        /// function zero.
        function_hint: Option<usize>,
        /// The statically checked result type.
        result: PrimitiveType,
        /// Whether this call is an explicit tail transfer in the checked IR.
        ///
        /// The checker sets this only when every statically bounded target is
        /// in the current module function's recursive group and the call is
        /// in a syntactic tail position.
        tail: bool,
        /// The source origin of the complete application.
        origin: SourceOrigin,
    },
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
        Self::External {
            intrinsic,
            arguments,
            origin,
        }
    }

    /// Creates an omitted labelled argument resolved by the selected callable.
    #[must_use]
    pub fn default_value(value_type: PrimitiveType, origin: SourceOrigin) -> Self {
        Self::Default { value_type, origin }
    }

    /// Creates a checked activation-slot reference.
    #[must_use]
    pub fn variable(
        slot: usize,
        value_type: PrimitiveType,
        origin: SourceOrigin,
    ) -> Self {
        Self::Variable {
            slot,
            value_type,
            origin,
        }
    }

    /// Creates a checked module-global reference.
    #[must_use]
    pub fn global(
        index: usize,
        value_type: PrimitiveType,
        origin: SourceOrigin,
    ) -> Self {
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
        parameters: Vec<PrimitiveType>,
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
            body: Box::new(body),
            slot_count,
            origin,
        }
    }

    /// Creates a closure-environment reference.
    #[must_use]
    pub fn captured(
        slot: usize,
        value_type: PrimitiveType,
        origin: SourceOrigin,
    ) -> Self {
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
        result: PrimitiveType,
        origin: SourceOrigin,
    ) -> Self {
        Self::Call {
            function,
            arguments,
            callee: None,
            function_hint: None,
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
        result: PrimitiveType,
        origin: SourceOrigin,
    ) -> Self {
        Self::Call {
            function,
            arguments,
            callee: None,
            function_hint: None,
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
        result: PrimitiveType,
        origin: SourceOrigin,
    ) -> Self {
        Self::Call {
            function: function_hint.unwrap_or_default(),
            arguments,
            callee: Some(Box::new(callee)),
            function_hint,
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
        result: PrimitiveType,
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
        result: PrimitiveType,
        origin: SourceOrigin,
    ) -> Self {
        Self::Call {
            function: function_hint.unwrap_or_default(),
            arguments,
            callee: Some(Box::new(callee)),
            function_hint,
            result,
            tail: true,
            origin,
        }
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
            | Self::If { origin, .. }
            | Self::Call { origin, .. } => origin,
        }
    }

    /// The statically known result type of this expression.
    #[must_use]
    pub fn result_type(&self) -> PrimitiveType {
        match self {
            Self::Literal { value, .. } => value.ty(),
            Self::External { intrinsic, .. } => intrinsic.signature().result(),
            Self::Default { value_type, .. } => value_type.clone(),
            Self::Sequence { expressions, .. } => expressions
                .last()
                .map_or(PrimitiveType::Void, Self::result_type),
            Self::Variable { value_type, .. } | Self::Global { value_type, .. } => {
                value_type.clone()
            }
            Self::Function { signature, .. } | Self::Closure { signature, .. } => {
                PrimitiveType::Function(Box::new(signature.clone()))
            }
            Self::Captured { value_type, .. } => value_type.clone(),
            Self::Let { body, .. } => body.result_type(),
            Self::If { then_branch, .. } => then_branch.result_type(),
            Self::Call { result, .. } => result.clone(),
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
            | Self::If { .. }
            | Self::Call { .. } => &[],
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
            | Self::If { .. }
            | Self::Call { .. } => None,
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
            Self::Call {
                arguments, callee, ..
            } => callee
                .as_deref()
                .map_or(0, Self::slot_count)
                .max(arguments.iter().map(Self::slot_count).max().unwrap_or(0)),
            Self::Closure { captures, .. } => {
                captures.iter().map(Self::slot_count).max().unwrap_or(0)
            }
        }
    }

    fn validate_shape(
        &self,
        slots: &mut [Option<PrimitiveType>],
    ) -> Result<PrimitiveType, IrError> {
        self.validate_shape_with_captures(slots, &[])
    }

    fn validate_shape_with_captures(
        &self,
        slots: &mut [Option<PrimitiveType>],
        capture_types: &[PrimitiveType],
    ) -> Result<PrimitiveType, IrError> {
        match self {
            Self::Literal { value, .. } => Ok(value.ty()),
            Self::External {
                intrinsic,
                arguments,
                ..
            } => {
                let signature = intrinsic.signature();
                if arguments.len() != signature.parameters().len() {
                    return Err(IrError::InvalidExpression(format!(
                        "{} expects {} arguments, got {}",
                        intrinsic.symbol(),
                        signature.parameters().len(),
                        arguments.len()
                    )));
                }
                for (argument, expected) in arguments.iter().zip(signature.parameters())
                {
                    let actual =
                        argument.validate_shape_with_captures(slots, capture_types)?;
                    if !actual.same_shape(expected) {
                        return Err(IrError::InvalidExpression(format!(
                            "{} argument has type {actual}, expected {expected}",
                            intrinsic.symbol()
                        )));
                    }
                }
                Ok(signature.result())
            }
            Self::Default { .. } => Err(IrError::InvalidExpression(
                "default argument marker is only valid as a call operand".to_owned(),
            )),
            Self::Function { signature, .. } => {
                Ok(PrimitiveType::Function(Box::new(signature.clone())))
            }
            Self::Captured {
                slot, value_type, ..
            } => {
                let Some(actual) = capture_types.get(*slot) else {
                    return Err(IrError::InvalidExpression(format!(
                        "capture slot {slot} is outside the closure environment"
                    )));
                };
                if !actual.same_shape(value_type) {
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
                    if !actual.same_shape(expected) {
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
                for (offset, parameter) in signature.labelled().iter().enumerate() {
                    if let Some(bound) =
                        closure_slots.get_mut(parameters.len().saturating_add(offset))
                    {
                        *bound = Some(parameter.value_type());
                    }
                }
                let actual = body.validate_shape_with_captures(
                    &mut closure_slots,
                    closure_capture_types,
                )?;
                if !actual.same_shape(&signature.result()) {
                    return Err(IrError::ResultTypeMismatch {
                        expected: signature.result(),
                        actual,
                    });
                }
                Ok(PrimitiveType::Function(Box::new(signature.clone())))
            }
            Self::Sequence { expressions, .. } => {
                let mut result = PrimitiveType::Void;
                for expression in expressions {
                    result = expression
                        .validate_shape_with_captures(slots, capture_types)?;
                }
                Ok(result)
            }
            Self::Variable {
                slot, value_type, ..
            } => match slots.get(*slot).cloned().flatten() {
                Some(actual) if actual.same_shape(value_type) => Ok(value_type.clone()),
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
            Self::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let condition_type =
                    condition.validate_shape_with_captures(slots, capture_types)?;
                if !condition_type.same_shape(&PrimitiveType::Bool) {
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
                if !then_type.same_shape(&else_type) {
                    return Err(IrError::InvalidExpression(format!(
                        "if branches have types {then_type} and {else_type}"
                    )));
                }
                Ok(then_type)
            }
            Self::Call {
                arguments,
                result,
                callee,
                ..
            } => {
                if let Some(callee) = callee {
                    let callee_type =
                        callee.validate_shape_with_captures(slots, capture_types)?;
                    if !matches!(callee_type, PrimitiveType::Function(_)) {
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
        }
    }
}

/// One checked module-level immutable value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedGlobal {
    name: String,
    value_type: PrimitiveType,
    initializer: Expr,
    origin: SourceOrigin,
    slot_count: usize,
}

impl CheckedGlobal {
    /// Creates a checked global after validating its initializer type.
    pub fn new(
        name: impl Into<String>,
        value_type: PrimitiveType,
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
                expected: value_type,
                actual,
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
    pub fn value_type(&self) -> PrimitiveType {
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
}

impl CheckedFunction {
    /// Creates a checked function backed by a closed compiler intrinsic.
    pub fn new_external(
        name: impl Into<String>,
        signature: FunctionSignature,
        intrinsic: external::CompilerIntrinsic,
        origin: SourceOrigin,
    ) -> Result<Self, IrError> {
        if signature != intrinsic.signature() {
            return Err(IrError::InvalidExpression(format!(
                "external {} has a mismatched declaration signature",
                intrinsic.symbol()
            )));
        }
        let arguments = signature
            .parameters()
            .iter()
            .enumerate()
            .map(|(slot, value_type)| {
                Expr::variable(slot, value_type.clone(), origin.clone())
            })
            .collect();
        let body = Expr::external(intrinsic, arguments, origin.clone());
        Self::with_slots_and_external(
            name,
            signature,
            body,
            origin,
            intrinsic.signature().fixed_parameter_count(),
            true,
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
        Self::with_slots_and_external(name, signature, body, origin, slot_count, false)
    }

    fn with_slots_and_external(
        name: impl Into<String>,
        signature: FunctionSignature,
        body: Expr,
        origin: SourceOrigin,
        slot_count: usize,
        external_wrapper: bool,
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
        for (slot, value_type) in signature.parameters().iter().enumerate() {
            if let Some(bound) = slots.get_mut(slot) {
                *bound = Some(value_type.clone());
            }
        }
        for (offset, parameter) in signature.labelled().iter().enumerate() {
            if let Some(bound) = slots.get_mut(signature.parameters().len() + offset) {
                *bound = Some(parameter.value_type());
            }
        }
        let actual = body.validate_shape(&mut slots)?;
        if !actual.same_shape(&signature.result()) {
            return Err(IrError::ResultTypeMismatch {
                expected: signature.result(),
                actual,
            });
        }
        Ok(Self {
            name,
            signature,
            body,
            origin,
            slot_count,
            external_wrapper,
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
}

/// A complete immutable program that crossed the checker boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedProgram {
    globals: Vec<CheckedGlobal>,
    functions: Vec<CheckedFunction>,
    entry: usize,
    recursive_groups: Vec<Vec<usize>>,
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
        if functions.is_empty() {
            return Err(IrError::NoFunctions);
        }
        if entry >= functions.len() {
            return Err(IrError::InvalidEntry(entry));
        }
        for (left, function) in functions.iter().enumerate() {
            if functions
                .iter()
                .enumerate()
                .any(|(right, other)| left != right && function.name == other.name)
            {
                return Err(IrError::DuplicateFunction(function.name.clone()));
            }
            if let Err(message) = validate_signature_shape(function.signature()) {
                return Err(IrError::InvalidExpression(format!(
                    "function `{}` has an invalid signature: {message}",
                    function.name
                )));
            }
            let mut slots = vec![None; function.slot_count];
            for (slot, value_type) in function.signature.parameters().iter().enumerate()
            {
                if let Some(bound) = slots.get_mut(slot) {
                    *bound = Some(value_type.clone());
                }
            }
            for (offset, parameter) in function.signature.labelled().iter().enumerate()
            {
                if let Some(bound) =
                    slots.get_mut(function.signature.parameters().len() + offset)
                {
                    *bound = Some(parameter.value_type());
                }
            }
            let actual = function.body.validate_shape(&mut slots)?;
            if !actual.same_shape(&function.signature.result()) {
                return Err(IrError::ResultTypeMismatch {
                    expected: function.signature.result(),
                    actual,
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
                    expected: global.value_type.clone(),
                    actual,
                });
            }
        }
        let mut calls = vec![BTreeSet::new(); functions.len()];
        let mut dependencies = vec![BTreeSet::new(); globals.len() + functions.len()];
        for (index, global) in globals.iter().enumerate() {
            validate_program_expr(
                global.initializer(),
                &globals,
                &functions,
                Some(DependencyNode::Global(index)),
                &mut calls,
                &mut dependencies,
            )?;
        }
        for (index, function) in functions.iter().enumerate() {
            validate_program_expr(
                function.body(),
                &globals,
                &functions,
                Some(DependencyNode::Function(index)),
                &mut calls,
                &mut dependencies,
            )?;
        }
        let (calls, flow_dependencies) =
            analyze_call_flow(&globals, &functions, entry)?;
        for (dependencies, flow_edges) in dependencies.iter_mut().zip(flow_dependencies)
        {
            dependencies.extend(flow_edges);
        }
        let recursive_groups = find_recursive_groups(&calls, &functions);
        for global in &globals {
            validate_tail_calls(
                global.initializer(),
                false,
                None,
                &recursive_groups,
                &globals,
                &functions,
                &BTreeMap::new(),
            )?;
        }
        for (index, function) in functions.iter().enumerate() {
            validate_tail_calls(
                function.body(),
                true,
                Some(index),
                &recursive_groups,
                &globals,
                &functions,
                &BTreeMap::new(),
            )?;
        }
        reject_global_initializer_cycles(&dependencies, globals.len())?;
        Ok(Self {
            globals,
            functions,
            entry,
            recursive_groups,
        })
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

    /// Same-module recursive groups, in deterministic function-index order.
    #[must_use]
    pub fn recursive_groups(&self) -> &[Vec<usize>] {
        &self.recursive_groups
    }

    /// Returns the recursive group containing `function`, if any.
    #[must_use]
    pub fn recursive_group(&self, function: usize) -> Option<&[usize]> {
        self.recursive_groups.get(function).map(Vec::as_slice)
    }

    /// The selected entry function.
    #[must_use]
    #[allow(clippy::indexing_slicing)]
    pub fn entry(&self) -> &CheckedFunction {
        // `try_new` proves this index is below the immutable function count.
        &self.functions[self.entry]
    }

    /// Canonical typed-program observation used by static-v1.
    #[must_use]
    pub fn canonical_vibon(&self) -> String {
        let mut output = String::from("(record\n  format: @types.v1\n");
        if !self.globals.is_empty() {
            output.push_str("  globals: (array\n");
            for global in &self.globals {
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
        for function in &self.functions {
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
        expected: PrimitiveType,
        /// The body result type.
        actual: PrimitiveType,
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
    /// The function-call graph contains a recursive group outside the admitted subset.
    RecursiveCall(String),
    /// A module initializer dependency graph contains a cycle.
    GlobalInitializerCycle(String),
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
            Self::GlobalInitializerCycle(message) => {
                write!(formatter, "global initializer cycle: {message}")
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
    calls: &mut [BTreeSet<usize>],
    dependencies: &mut [BTreeSet<DependencyNode>],
) -> Result<(), IrError> {
    match expression {
        Expr::Literal { .. }
        | Expr::Default { .. }
        | Expr::Variable { .. }
        | Expr::Captured { .. } => {}
        Expr::External { arguments, .. } => {
            for argument in arguments {
                validate_program_expr(
                    argument,
                    globals,
                    functions,
                    owner,
                    calls,
                    dependencies,
                )?;
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
            if callee.signature() != signature {
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
                    calls,
                    dependencies,
                )?;
            }
            validate_program_expr(
                body,
                globals,
                functions,
                owner,
                calls,
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
                    calls,
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
            validate_program_expr(
                value,
                globals,
                functions,
                owner,
                calls,
                dependencies,
            )?;
            validate_program_expr(
                body,
                globals,
                functions,
                owner,
                calls,
                dependencies,
            )?;
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            validate_program_expr(
                condition,
                globals,
                functions,
                owner,
                calls,
                dependencies,
            )?;
            validate_program_expr(
                then_branch,
                globals,
                functions,
                owner,
                calls,
                dependencies,
            )?;
            validate_program_expr(
                else_branch,
                globals,
                functions,
                owner,
                calls,
                dependencies,
            )?;
        }
        Expr::Call {
            function,
            arguments,
            result,
            callee: callee_expression,
            function_hint,
            ..
        } => {
            let signature = if let Some(callee_expression) = callee_expression {
                validate_program_expr(
                    callee_expression,
                    globals,
                    functions,
                    owner,
                    calls,
                    dependencies,
                )?;
                match callee_expression.result_type() {
                    PrimitiveType::Function(signature) => *signature,
                    actual => {
                        return Err(IrError::InvalidExpression(format!(
                            "indirect callee has non-function type {actual}"
                        )));
                    }
                }
            } else {
                let Some(callee) = functions.get(*function) else {
                    return Err(IrError::InvalidExpression(format!(
                        "function index {function} is outside the program"
                    )));
                };
                callee.signature().clone()
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
                    calls,
                    dependencies,
                )?;
            }
            let expected_parameters = signature
                .parameters()
                .iter()
                .cloned()
                .chain(
                    signature
                        .labelled()
                        .iter()
                        .map(|parameter| parameter.value_type()),
                )
                .collect::<Vec<_>>();
            for (argument, expected) in arguments.iter().zip(expected_parameters) {
                let actual = argument.result_type();
                if !actual.same_shape(&expected) {
                    return Err(IrError::InvalidExpression(format!(
                        "call argument has type {actual}, expected {expected}"
                    )));
                }
            }
            if !result.same_shape(&signature.result()) {
                return Err(IrError::InvalidExpression(format!(
                    "call result declares {result}, callee returns {}",
                    signature.result()
                )));
            }
            let mut targets = BTreeSet::new();
            if let Some(callee_expression) = callee_expression {
                let summary = possible_function_targets(
                    callee_expression,
                    &BTreeMap::new(),
                    globals,
                    functions,
                    &mut BTreeSet::new(),
                    &mut BTreeSet::new(),
                );
                if let Some(function_hint) = function_hint {
                    if summary.has_closure
                        || (!summary.known.is_empty()
                            && (summary.unknown
                                || summary.known != BTreeSet::from([*function_hint])))
                    {
                        return Err(IrError::InvalidExpression(
                            "function hint does not match indirect callee".to_owned(),
                        ));
                    }
                    targets.insert(*function_hint);
                } else {
                    targets.extend(summary.known);
                }
            } else {
                targets.insert(*function);
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
                    if let DependencyNode::Function(owner) = owner {
                        let Some(edges) = calls.get_mut(owner) else {
                            return Err(IrError::InvalidExpression(format!(
                                "function owner index {owner} is outside the program"
                            )));
                        };
                        edges.insert(dependency_target);
                    }
                }
            }
        }
    }
    Ok(())
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
        Expr::Function { function, .. } => FunctionTargetSummary::known(*function),
        // A closure is a distinct runtime callable, even when its body
        // returns a module function.  It therefore cannot be summarized as
        // that function's identity for a module-level tail transfer.
        Expr::Closure { .. } => FunctionTargetSummary::closure(),
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
                unknown: value_targets.unknown || body_targets.unknown,
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
            function, callee, ..
        } => {
            let target_summary = if let Some(callee) = callee {
                possible_function_targets(
                    callee,
                    aliases,
                    globals,
                    functions,
                    visiting,
                    visiting_globals,
                )
            } else {
                FunctionTargetSummary::known(*function)
            };
            if target_summary.unknown || target_summary.has_closure {
                return target_summary;
            }
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
                let returned = possible_function_targets(
                    function.body(),
                    &BTreeMap::new(),
                    globals,
                    functions,
                    visiting,
                    visiting_globals,
                );
                result.known.extend(returned.known);
                result.unknown |= returned.unknown;
                result.has_closure |= returned.has_closure;
                visiting.remove(&target);
            }
            result
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
    closures: Vec<Self>,
    closure_defaults: Vec<Vec<bool>>,
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
        self.closures.extend(other.closures.iter().cloned());
        self.closure_defaults
            .extend(other.closure_defaults.iter().cloned());
    }
}

struct CallFlow<'a> {
    globals: &'a [CheckedGlobal],
    functions: &'a [CheckedFunction],
    global_returns: Vec<FlowTargetSummary>,
    function_returns: Vec<FlowTargetSummary>,
    parameter_targets: Vec<Vec<FlowTargetSummary>>,
    parameter_sources: Vec<Vec<bool>>,
    calls: Vec<BTreeSet<usize>>,
    dependencies: Vec<BTreeSet<DependencyNode>>,
    unresolved: bool,
}

type CallAnalysis = (Vec<BTreeSet<usize>>, Vec<BTreeSet<DependencyNode>>);

fn analyze_call_flow(
    globals: &[CheckedGlobal],
    functions: &[CheckedFunction],
    entry: usize,
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
        calls: vec![BTreeSet::new(); functions.len()],
        dependencies: vec![BTreeSet::new(); globals.len() + functions.len()],
        unresolved: false,
    };

    if let Some(function) = functions.get(entry) {
        for (slot, value_type) in
            function_signature_types(function.signature()).enumerate()
        {
            if matches!(value_type, PrimitiveType::Function(_))
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

    let iteration_limit = functions
        .len()
        .saturating_mul(functions.len().saturating_add(1))
        .saturating_add(globals.len())
        .saturating_add(8);
    for _ in 0..iteration_limit.max(1) {
        let previous_parameters = flow.parameter_targets.clone();
        let previous_sources = flow.parameter_sources.clone();
        let returns_changed = flow.refresh_returns();
        flow.calls = vec![BTreeSet::new(); functions.len()];
        flow.dependencies = vec![BTreeSet::new(); globals.len() + functions.len()];
        flow.unresolved = false;
        for (global_index, global) in globals.iter().enumerate() {
            flow.collect_expr(
                global.initializer(),
                Some(DependencyNode::Global(global_index)),
                &BTreeMap::new(),
                &[],
            )?;
        }
        for (index, function) in functions.iter().enumerate() {
            let environment = flow.parameter_environment(index);
            flow.collect_expr(
                function.body(),
                Some(DependencyNode::Function(index)),
                &environment,
                &[],
            )?;
        }
        let parameters_changed = previous_parameters != flow.parameter_targets
            || previous_sources != flow.parameter_sources;
        if !returns_changed && !parameters_changed {
            if flow.unresolved {
                return Err(IrError::RecursiveCall(
                    "indirect call target is not statically bounded before Step 9"
                        .to_owned(),
                ));
            }
            return Ok((flow.calls, flow.dependencies));
        }
    }
    if flow.unresolved {
        return Err(IrError::RecursiveCall(
            "indirect call target analysis did not reach a bounded result".to_owned(),
        ));
    }
    Ok((flow.calls, flow.dependencies))
}

fn function_signature_types(
    signature: &FunctionSignature,
) -> impl Iterator<Item = PrimitiveType> + '_ {
    signature.parameters().iter().cloned().chain(
        signature
            .labelled()
            .iter()
            .map(LabelledParameter::value_type),
    )
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
            if matches!(value_type, PrimitiveType::Function(_)) {
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

    fn refresh_returns(&mut self) -> bool {
        let new_globals = self
            .globals
            .iter()
            .map(|global| {
                self.summary_expr(global.initializer(), &BTreeMap::new(), &[])
            })
            .collect::<Vec<_>>();
        let new_functions = self
            .functions
            .iter()
            .enumerate()
            .map(|(index, function)| {
                let environment = self.parameter_environment(index);
                self.summary_expr(function.body(), &environment, &[])
            })
            .collect::<Vec<_>>();
        let changed = new_globals != self.global_returns
            || new_functions != self.function_returns;
        self.global_returns = new_globals;
        self.function_returns = new_functions;
        changed
    }

    fn summary_expr(
        &self,
        expression: &Expr,
        environment: &BTreeMap<usize, FlowTargetSummary>,
        captures: &[FlowTargetSummary],
    ) -> FlowTargetSummary {
        match expression {
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
                    .map(|capture| self.summary_expr(capture, environment, captures))
                    .collect::<Vec<_>>();
                let mut closure_environment = BTreeMap::new();
                for (slot, value_type) in
                    function_signature_types(signature).enumerate()
                {
                    if matches!(value_type, PrimitiveType::Function(_)) {
                        closure_environment
                            .insert(slot, FlowTargetSummary::unknown_function());
                    }
                }
                FlowTargetSummary {
                    is_function: true,
                    closures: vec![self.summary_expr(
                        body,
                        &closure_environment,
                        &closure_capture_summaries,
                    )],
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
                if !matches!(value_type, PrimitiveType::Function(_)) {
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
                if !matches!(value_type, PrimitiveType::Function(_)) {
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
                if !matches!(value_type, PrimitiveType::Function(_)) {
                    return FlowTargetSummary::default();
                }
                self.global_returns
                    .get(*index)
                    .cloned()
                    .unwrap_or_else(FlowTargetSummary::unknown_function)
            }
            Expr::Let {
                slot, value, body, ..
            } => {
                let value_summary = self.summary_expr(value, environment, captures);
                let mut nested = environment.clone();
                if let Some(slot) = slot {
                    nested.insert(*slot, value_summary);
                }
                self.summary_expr(body, &nested, captures)
            }
            Expr::If {
                then_branch,
                else_branch,
                ..
            } => {
                let mut summary = self.summary_expr(then_branch, environment, captures);
                summary.union(&self.summary_expr(else_branch, environment, captures));
                summary
            }
            Expr::Sequence { expressions, .. } => expressions
                .last()
                .map_or_else(FlowTargetSummary::default, |expression| {
                    self.summary_expr(expression, environment, captures)
                }),
            Expr::Call {
                function,
                callee,
                function_hint,
                result,
                ..
            } => {
                if !matches!(result, PrimitiveType::Function(_)) {
                    return FlowTargetSummary::default();
                }
                let callee_summary = callee.as_deref().map_or_else(
                    || FlowTargetSummary::known_function(*function),
                    |callee| self.summary_expr(callee, environment, captures),
                );
                let mut targets = callee_summary.clone();
                if let Some(function_hint) = function_hint {
                    targets.known.insert(*function_hint);
                    targets.is_function = true;
                }
                let mut summary = FlowTargetSummary {
                    is_function: true,
                    unknown: targets.unknown,
                    ..FlowTargetSummary::default()
                };
                for closure in &targets.closures {
                    summary.union(closure);
                }
                for target in targets.known {
                    if let Some(returned) = self.function_returns.get(target) {
                        summary.union(returned);
                    } else {
                        summary.unknown = true;
                    }
                }
                summary
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
            | Expr::Global { .. }
            | Expr::Function { .. }
            | Expr::Captured { .. } => {}
            Expr::External { arguments, .. } => {
                for argument in arguments {
                    self.collect_expr(argument, owner, environment, captures)?;
                }
            }
            Expr::Closure {
                captures: closure_captures,
                body,
                signature,
                ..
            } => {
                for capture in closure_captures {
                    self.collect_expr(capture, owner, environment, captures)?;
                }
                let closure_capture_summaries = closure_captures
                    .iter()
                    .map(|capture| self.summary_expr(capture, environment, captures))
                    .collect::<Vec<_>>();
                let mut closure_environment = BTreeMap::new();
                for (slot, value_type) in
                    function_signature_types(signature).enumerate()
                {
                    if matches!(value_type, PrimitiveType::Function(_)) {
                        closure_environment
                            .insert(slot, FlowTargetSummary::unknown_function());
                    }
                }
                self.collect_expr(
                    body,
                    owner,
                    &closure_environment,
                    &closure_capture_summaries,
                )?;
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
                let mut nested = environment.clone();
                if let Some(slot) = slot {
                    nested.insert(*slot, value_summary);
                }
                self.collect_expr(body, owner, &nested, captures)?;
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
                function,
                arguments,
                callee,
                function_hint,
                ..
            } => {
                if let Some(callee) = callee {
                    self.collect_expr(callee, owner, environment, captures)?;
                }
                for argument in arguments {
                    self.collect_expr(argument, owner, environment, captures)?;
                }
                let callee_summary = callee.as_deref().map_or_else(
                    || FlowTargetSummary::known_function(*function),
                    |callee| self.summary_expr(callee, environment, captures),
                );
                let mut target_summary = callee_summary;
                if let Some(function_hint) = function_hint {
                    target_summary.known.insert(*function_hint);
                    target_summary.is_function = true;
                }
                if target_summary.unknown {
                    self.unresolved = true;
                }
                let argument_summaries = arguments
                    .iter()
                    .map(|argument| self.summary_expr(argument, environment, captures))
                    .collect::<Vec<_>>();
                for closure in &target_summary.closures {
                    if closure.unknown {
                        self.unresolved = true;
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
                        for target in &closure.known {
                            dependencies.insert(DependencyNode::Function(*target));
                        }
                        if let DependencyNode::Function(owner) = owner {
                            let Some(edges) = self.calls.get_mut(owner) else {
                                return Err(IrError::InvalidExpression(format!(
                                    "function owner index {owner} is outside the program"
                                )));
                            };
                            edges.extend(&closure.known);
                        }
                    }
                }
                if !target_summary.closure_defaults.is_empty()
                    && let Some(callee) = callee.as_deref()
                    && let PrimitiveType::Function(signature) = callee.result_type()
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
                        if let DependencyNode::Function(owner) = owner {
                            let Some(edges) = self.calls.get_mut(owner) else {
                                return Err(IrError::InvalidExpression(format!(
                                    "function owner index {owner} is outside the program"
                                )));
                            };
                            edges.insert(target);
                        }
                    }
                    for (slot, (argument, value_type)) in argument_summaries
                        .iter()
                        .zip(function_signature_types(callee.signature()))
                        .enumerate()
                    {
                        if !matches!(value_type, PrimitiveType::Function(_)) {
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
                    }
                }
            }
        }
        Ok(())
    }
}

fn validate_type_shape(value_type: &PrimitiveType) -> Result<(), String> {
    if let PrimitiveType::Function(signature) = value_type {
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
        if let Some(default) = parameter.default()
            && !default.ty().same_shape(&parameter.value_type())
        {
            return Err(format!(
                "default for labelled parameter `{}` has type {}, expected {}",
                parameter.name(),
                default.ty(),
                parameter.value_type()
            ));
        }
    }
    validate_type_shape(&signature.result())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallState {
    Unvisited,
    Visiting,
    Done,
}

fn find_recursive_groups(
    calls: &[BTreeSet<usize>],
    functions: &[CheckedFunction],
) -> Vec<Vec<usize>> {
    (0..calls.len())
        .map(|start| {
            if !is_module_definition(functions, start) {
                return Vec::new();
            }
            reachable_functions(start, calls)
                .into_iter()
                .filter(|target| is_module_definition(functions, *target))
                .collect()
        })
        .collect()
}

fn is_module_definition(functions: &[CheckedFunction], index: usize) -> bool {
    functions
        .get(index)
        .is_some_and(|function| !function.is_external_wrapper())
}

fn reachable_functions(start: usize, calls: &[BTreeSet<usize>]) -> BTreeSet<usize> {
    let mut reached = BTreeSet::new();
    let mut pending = vec![start];
    while let Some(index) = pending.pop() {
        if !reached.insert(index) {
            continue;
        }
        if let Some(dependencies) = calls.get(index) {
            pending.extend(dependencies.iter().copied());
        }
    }
    reached
}

fn validate_tail_calls(
    expression: &Expr,
    tail_position: bool,
    current_function: Option<usize>,
    recursive_groups: &[Vec<usize>],
    globals: &[CheckedGlobal],
    functions: &[CheckedFunction],
    aliases: &BTreeMap<usize, FunctionTargetSummary>,
) -> Result<(), IrError> {
    match expression {
        Expr::Literal { .. }
        | Expr::Default { .. }
        | Expr::Variable { .. }
        | Expr::Global { .. }
        | Expr::Function { .. }
        | Expr::Captured { .. } => {}
        Expr::External { arguments, .. } => {
            for argument in arguments {
                validate_tail_calls(
                    argument,
                    false,
                    current_function,
                    recursive_groups,
                    globals,
                    functions,
                    aliases,
                )?;
            }
        }
        Expr::Closure { captures, body, .. } => {
            for capture in captures {
                validate_tail_calls(
                    capture,
                    false,
                    current_function,
                    recursive_groups,
                    globals,
                    functions,
                    aliases,
                )?;
            }
            validate_tail_calls(
                body,
                true,
                None,
                recursive_groups,
                globals,
                functions,
                &BTreeMap::new(),
            )?;
        }
        Expr::Sequence { expressions, .. } => {
            for (index, expression) in expressions.iter().enumerate() {
                validate_tail_calls(
                    expression,
                    tail_position && index + 1 == expressions.len(),
                    current_function,
                    recursive_groups,
                    globals,
                    functions,
                    aliases,
                )?;
            }
        }
        Expr::Let {
            value, body, slot, ..
        } => {
            validate_tail_calls(
                value,
                false,
                current_function,
                recursive_groups,
                globals,
                functions,
                aliases,
            )?;
            let value_targets = possible_function_targets(
                value,
                aliases,
                globals,
                functions,
                &mut BTreeSet::new(),
                &mut BTreeSet::new(),
            );
            let mut nested_aliases = aliases.clone();
            if let Some(slot) = slot {
                nested_aliases.insert(*slot, value_targets);
            }
            validate_tail_calls(
                body,
                tail_position,
                current_function,
                recursive_groups,
                globals,
                functions,
                &nested_aliases,
            )?;
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            validate_tail_calls(
                condition,
                false,
                current_function,
                recursive_groups,
                globals,
                functions,
                aliases,
            )?;
            validate_tail_calls(
                then_branch,
                tail_position,
                current_function,
                recursive_groups,
                globals,
                functions,
                aliases,
            )?;
            validate_tail_calls(
                else_branch,
                tail_position,
                current_function,
                recursive_groups,
                globals,
                functions,
                aliases,
            )?;
        }
        Expr::Call {
            function,
            arguments,
            callee,
            function_hint,
            tail,
            ..
        } => {
            if let Some(callee) = callee {
                validate_tail_calls(
                    callee,
                    false,
                    current_function,
                    recursive_groups,
                    globals,
                    functions,
                    aliases,
                )?;
            }
            for argument in arguments {
                validate_tail_calls(
                    argument,
                    false,
                    current_function,
                    recursive_groups,
                    globals,
                    functions,
                    aliases,
                )?;
            }
            if !tail {
                return Ok(());
            }
            if !tail_position {
                return Err(IrError::InvalidExpression(
                    "tail call is outside an activation-relative tail position"
                        .to_owned(),
                ));
            }
            let Some(current_function) = current_function else {
                return Err(IrError::InvalidExpression(
                    "tail call has no module-level function activation".to_owned(),
                ));
            };
            let targets = if let Some(callee) = callee {
                possible_function_targets(
                    callee,
                    aliases,
                    globals,
                    functions,
                    &mut BTreeSet::new(),
                    &mut BTreeSet::new(),
                )
            } else {
                FunctionTargetSummary::known(*function)
            };
            if targets.unknown || targets.has_closure || targets.known.is_empty() {
                return Err(IrError::InvalidExpression(
                    "tail call target is not statically bounded".to_owned(),
                ));
            }
            if let Some(function_hint) = function_hint
                && targets.known != BTreeSet::from([*function_hint])
            {
                return Err(IrError::InvalidExpression(
                    "tail call hint does not match indirect callee".to_owned(),
                ));
            }
            let Some(group) = recursive_groups.get(current_function) else {
                return Err(IrError::InvalidExpression(format!(
                    "tail call owner {current_function} is outside the program"
                )));
            };
            for target in targets.known {
                if functions.get(target).is_none() {
                    return Err(IrError::InvalidExpression(format!(
                        "tail call target {target} is outside the program"
                    )));
                }
                if !group.contains(&target) {
                    return Err(IrError::InvalidExpression(format!(
                        "tail call target {target} is outside function {current_function}'s recursive group"
                    )));
                }
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
        )?;
    }
    Ok(())
}

fn visit_dependency_graph(
    node: DependencyNode,
    dependencies: &[BTreeSet<DependencyNode>],
    global_count: usize,
    states: &mut [CallState],
) -> Result<(), IrError> {
    let index = node.node_index(global_count);
    match states.get(index).copied() {
        Some(CallState::Done) => return Ok(()),
        Some(CallState::Visiting) => {
            return Err(IrError::GlobalInitializerCycle(format!(
                "dependency node {node:?} is part of a module initializer cycle"
            )));
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
    let Some(edges) = dependencies.get(index) else {
        return Err(IrError::InvalidExpression(format!(
            "dependency graph has no node for {node:?}"
        )));
    };
    for dependency in edges {
        visit_dependency_graph(*dependency, dependencies, global_count, states)?;
    }
    if let Some(state) = states.get_mut(index) {
        *state = CallState::Done;
    }
    Ok(())
}

fn canonical_expr(expression: &Expr) -> String {
    match expression {
        Expr::Literal { value, .. } => format!(
            "(record kind: @literal type: @{} value: {})",
            value.ty().as_str(),
            value.canonical_vibon()
        ),
        Expr::External {
            intrinsic,
            arguments,
            ..
        } => format!(
            "(record kind: @external symbol: \"{}\" result: {} arguments: {})",
            intrinsic.symbol(),
            canonical_type(&intrinsic.signature().result()),
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
            function,
            arguments,
            result,
            callee,
            function_hint,
            tail,
            ..
        } => {
            let values = arguments.iter().map(canonical_expr).collect::<Vec<_>>();
            let tail_field = if *tail { " tail: true" } else { "" };
            match callee {
                Some(callee) => {
                    let function_field = function_hint
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
                None => format!(
                    "(record kind: @call function: {}u64{} result: {} arguments: {})",
                    function,
                    tail_field,
                    canonical_type(result),
                    canonical_array(&values)
                ),
            }
        }
    }
}

fn canonical_type(value: &PrimitiveType) -> String {
    match value {
        PrimitiveType::Function(signature) => canonical_function_signature(signature),
        _ => format!("@{}", value.as_str()),
    }
}

fn canonical_function_signature(signature: &FunctionSignature) -> String {
    let parameters = signature
        .parameters()
        .iter()
        .map(canonical_type)
        .collect::<Vec<_>>();
    let mut output = format!(
        "(fn {} {}",
        canonical_array(&parameters),
        canonical_type(&signature.result())
    );
    if !signature.labelled().is_empty() {
        let labelled = signature
            .labelled()
            .iter()
            .map(|parameter| {
                format!(
                    "(record name: @{} type: {}",
                    parameter.name(),
                    canonical_type(&parameter.value_type())
                ) + ")"
            })
            .collect::<Vec<_>>();
        output.push_str(&format!(" labelled: {}", canonical_array(&labelled)));
    }
    output.push(')');
    output
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

fn format_float<T: fmt::Display>(value: T, suffix: &str) -> String {
    format!("{value}{suffix}")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{
        ByteSpan, CheckedFunction, CheckedGlobal, CheckedProgram, Expr,
        FunctionSignature, IrError, PrimitiveType, SourceOrigin, Value,
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
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
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
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            Expr::variable(0, PrimitiveType::I32, origin.clone()),
            origin,
        );
        assert!(matches!(result, Err(IrError::InvalidExpression(_))));
    }

    #[test]
    fn program_constructor_records_recursive_group() {
        let origin = origin();
        let function = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            Expr::call(0, Vec::new(), PrimitiveType::I32, origin.clone()),
            origin,
        )
        .expect("call shape is valid before program binding");
        let program = CheckedProgram::try_new(vec![function], 0)
            .expect("recursive calls are admitted for tail-call analysis");
        assert_eq!(program.recursive_groups(), &[vec![0]]);
    }

    #[test]
    fn program_constructor_records_each_function_reachability_group() {
        let origin = origin();
        let signature = FunctionSignature::new(Vec::new(), PrimitiveType::I32);
        let answer = CheckedFunction::new(
            "answer",
            signature.clone(),
            Expr::call(1, Vec::new(), PrimitiveType::I32, origin.clone()),
            origin.clone(),
        )
        .expect("answer function");
        let leaf = CheckedFunction::new(
            "leaf",
            signature,
            Expr::literal(Value::I32(7), origin.clone()),
            origin,
        )
        .expect("leaf function");
        let program = CheckedProgram::try_new(vec![answer, leaf], 0)
            .expect("reachable groups are valid");
        assert_eq!(program.recursive_groups(), &[vec![0, 1], vec![1]]);
        assert_eq!(program.recursive_group(0), Some(&[0, 1][..]));
        assert_eq!(program.recursive_group(1), Some(&[1][..]));
    }

    #[test]
    fn program_constructor_records_recursive_function_value_group() {
        let origin = origin();
        let signature = FunctionSignature::new(Vec::new(), PrimitiveType::I32);
        let function_value = Expr::function(0, signature.clone(), origin.clone());
        let body = Expr::indirect_call(
            function_value,
            None,
            Vec::new(),
            PrimitiveType::I32,
            origin.clone(),
        );
        let function = CheckedFunction::new("answer", signature, body, origin)
            .expect("indirect call shape is valid before program binding");
        let program = CheckedProgram::try_new(vec![function], 0)
            .expect("recursive function values are admitted for tail-call analysis");
        assert_eq!(program.recursive_groups(), &[vec![0]]);
    }

    #[test]
    fn tail_call_is_explicit_in_checked_ir() {
        let origin = origin();
        let expression =
            Expr::tail_call(3, Vec::new(), PrimitiveType::I32, origin.clone());
        assert!(expression.is_tail_call());
        assert!(super::canonical_expr(&expression).contains("tail: true"));
        let normal = Expr::call(3, Vec::new(), PrimitiveType::I32, origin);
        assert!(!super::canonical_expr(&normal).contains("tail: true"));
    }

    #[test]
    fn program_constructor_rejects_tail_call_outside_sequence_tail_position() {
        let origin = origin();
        let signature = FunctionSignature::new(Vec::new(), PrimitiveType::I32);
        let body = Expr::sequence(
            vec![
                Expr::tail_call(0, Vec::new(), PrimitiveType::I32, origin.clone()),
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
        let leaf_signature =
            FunctionSignature::new(vec![PrimitiveType::I32], PrimitiveType::I32);
        let leaf = CheckedFunction::new(
            "leaf",
            leaf_signature,
            Expr::variable(0, PrimitiveType::I32, origin.clone()),
            origin.clone(),
        )
        .expect("leaf function");
        let caller_signature = FunctionSignature::new(Vec::new(), PrimitiveType::I32);
        let caller_body = Expr::call(
            0,
            vec![Expr::tail_call(
                1,
                Vec::new(),
                PrimitiveType::I32,
                origin.clone(),
            )],
            PrimitiveType::I32,
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
        let signature = FunctionSignature::new(Vec::new(), PrimitiveType::Bool);
        let body = Expr::if_expression(
            Expr::tail_call(0, Vec::new(), PrimitiveType::Bool, origin.clone()),
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
    fn program_constructor_rejects_tail_call_in_a_closure_activation() {
        let origin = origin();
        let closure_signature = FunctionSignature::new(Vec::new(), PrimitiveType::I32);
        let outer_signature = FunctionSignature::new(
            Vec::new(),
            PrimitiveType::Function(Box::new(closure_signature.clone())),
        );
        let closure = Expr::closure(
            closure_signature,
            Vec::new(),
            Vec::new(),
            Expr::tail_call(0, Vec::new(), PrimitiveType::I32, origin.clone()),
            0,
            origin.clone(),
        );
        let callee = CheckedFunction::new(
            "callee",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            Expr::literal(Value::I32(7), origin.clone()),
            origin.clone(),
        )
        .expect("callee");
        let function = CheckedFunction::new("answer", outer_signature, closure, origin)
            .expect("closure shape is valid before tail-position validation");
        let error = CheckedProgram::try_new(vec![callee, function], 1)
            .expect_err("closure tail markers must use their own activation");
        assert!(
            error
                .to_string()
                .contains("module-level function activation"),
            "{error}"
        );
    }

    #[test]
    fn program_constructor_rejects_tail_call_in_a_global_initializer() {
        let origin = origin();
        let function = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            Expr::literal(Value::I32(1), origin.clone()),
            origin.clone(),
        )
        .expect("function");
        let global = CheckedGlobal::new(
            "value",
            PrimitiveType::I32,
            Expr::tail_call(0, Vec::new(), PrimitiveType::I32, origin.clone()),
            origin.clone(),
        )
        .expect("global shape is valid before tail-position validation");
        let error =
            CheckedProgram::try_new_with_globals(vec![global], vec![function], 0)
                .expect_err("global initializers do not have tail-call activations");
        assert!(error.to_string().contains("tail position"));
    }

    #[test]
    fn program_constructor_rejects_tail_call_to_an_external_wrapper() {
        let origin = origin();
        let intrinsic = super::external::CompilerIntrinsic::TextLength;
        let external = CheckedFunction::new_external(
            "text.length",
            intrinsic.signature(),
            intrinsic,
            origin.clone(),
        )
        .expect("external wrapper");
        let caller = CheckedFunction::new(
            "caller",
            FunctionSignature::new(Vec::new(), PrimitiveType::U64),
            Expr::tail_call(
                0,
                vec![Expr::literal(Value::Str("x".to_owned()), origin.clone())],
                PrimitiveType::U64,
                origin.clone(),
            ),
            origin,
        )
        .expect("caller shape is valid before tail-target validation");
        let error = CheckedProgram::try_new(vec![external, caller], 1).expect_err(
            "external wrappers are not module-level recursive-group members",
        );
        assert!(error.to_string().contains("outside function"));
    }

    #[test]
    fn program_constructor_keeps_source_functions_with_external_operands_in_groups() {
        let origin = origin();
        let signature = FunctionSignature::new(Vec::new(), PrimitiveType::Str);
        let recursive_operand =
            Expr::call(0, Vec::new(), PrimitiveType::Str, origin.clone());
        let body = Expr::external(
            super::external::CompilerIntrinsic::TextConcat,
            vec![
                recursive_operand,
                Expr::literal(Value::Str(String::new()), origin.clone()),
            ],
            origin.clone(),
        );
        let function = CheckedFunction::new("source", signature, body, origin)
            .expect("source function with an intrinsic operand");
        let program = CheckedProgram::try_new(vec![function], 0)
            .expect("recursive external operands are valid");
        assert_eq!(program.recursive_groups(), &[vec![0]]);
    }

    #[test]
    fn program_constructor_rejects_tail_transfers_through_closures() {
        let origin = origin();
        let signature = FunctionSignature::new(Vec::new(), PrimitiveType::I32);
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
            Expr::indirect_tail_call(
                callee,
                0,
                Vec::new(),
                PrimitiveType::I32,
                origin.clone(),
            ),
            origin,
        )
        .expect("caller shape");
        let error = CheckedProgram::try_new(vec![target, caller], 1)
            .expect_err("a closure is not a module-level tail target");
        assert!(
            error.to_string().contains("statically bounded")
                || error.to_string().contains("function hint")
        );
    }

    #[test]
    fn program_constructor_rejects_tail_transfers_with_a_closure_branch() {
        let origin = origin();
        let signature = FunctionSignature::new(Vec::new(), PrimitiveType::I32);
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
                Some(0),
                Vec::new(),
                PrimitiveType::I32,
                origin.clone(),
            ),
            origin,
        )
        .expect("caller shape");
        let error = CheckedProgram::try_new(vec![target, caller], 1)
            .expect_err("a closure branch is not a module-level tail target");
        assert!(error.to_string().contains("function hint"));
    }

    #[test]
    fn program_constructor_rejects_tail_transfers_from_closures_returning_functions() {
        let origin = origin();
        let target_signature = FunctionSignature::new(Vec::new(), PrimitiveType::I32);
        let target = CheckedFunction::new(
            "target",
            target_signature.clone(),
            Expr::literal(Value::I32(1), origin.clone()),
            origin.clone(),
        )
        .expect("target");
        let closure_signature = FunctionSignature::new(
            Vec::new(),
            PrimitiveType::Function(Box::new(target_signature.clone())),
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
            Expr::indirect_tail_call(
                callee,
                0,
                Vec::new(),
                PrimitiveType::I32,
                origin.clone(),
            ),
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
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            Expr::default_value(PrimitiveType::I32, origin.clone()),
            origin,
        );
        assert!(matches!(result, Err(IrError::InvalidExpression(_))));
    }

    #[test]
    fn program_constructor_rejects_forged_indirect_function_hints() {
        let origin = origin();
        let signature = FunctionSignature::new(Vec::new(), PrimitiveType::I32);
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
            PrimitiveType::I32,
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
        let signature = FunctionSignature::new(Vec::new(), PrimitiveType::I32);
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
        let caller_body = Expr::indirect_call(
            callee,
            Some(1),
            Vec::new(),
            PrimitiveType::I32,
            origin.clone(),
        );
        let caller = CheckedFunction::new("caller", signature, caller_body, origin)
            .expect("caller function");
        let result = CheckedProgram::try_new(vec![first, second, caller], 2);
        assert!(matches!(result, Err(IrError::InvalidExpression(_))));
    }

    #[test]
    fn program_constructor_does_not_trust_hints_for_unknown_parameters() {
        let origin = origin();
        let called_signature = FunctionSignature::new(Vec::new(), PrimitiveType::I32);
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
            vec![PrimitiveType::Function(Box::new(called_signature.clone()))],
            PrimitiveType::I32,
        );
        let caller_body = Expr::indirect_call(
            Expr::variable(
                0,
                PrimitiveType::Function(Box::new(called_signature)),
                origin.clone(),
            ),
            Some(1),
            Vec::new(),
            PrimitiveType::I32,
            origin.clone(),
        );
        let caller =
            CheckedFunction::new("caller", caller_signature, caller_body, origin)
                .expect("caller function");
        let result = CheckedProgram::try_new(vec![first, second, caller], 2);
        assert!(matches!(result, Err(IrError::RecursiveCall(_))));
    }

    #[test]
    fn program_constructor_rejects_default_markers_in_positional_slots() {
        let origin = origin();
        let callee_signature = FunctionSignature::with_labelled(
            vec![PrimitiveType::I32],
            vec![super::LabelledParameter::new(
                "value",
                PrimitiveType::I32,
                Some(Value::I32(7)),
            )],
            PrimitiveType::I32,
        );
        let callee = CheckedFunction::new(
            "callee",
            callee_signature.clone(),
            Expr::variable(0, PrimitiveType::I32, origin.clone()),
            origin.clone(),
        )
        .expect("callee function");
        let caller_signature = FunctionSignature::new(Vec::new(), PrimitiveType::I32);
        let caller_body = Expr::call(
            0,
            vec![
                Expr::default_value(PrimitiveType::I32, origin.clone()),
                Expr::literal(Value::I32(8), origin.clone()),
            ],
            PrimitiveType::I32,
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
            vec![super::LabelledParameter::new(
                "value",
                PrimitiveType::I32,
                None,
            )],
            PrimitiveType::I32,
        );
        let callee = CheckedFunction::new(
            "callee",
            signature.clone(),
            Expr::variable(0, PrimitiveType::I32, origin.clone()),
            origin.clone(),
        )
        .expect("callee");
        let caller = CheckedFunction::new(
            "caller",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            Expr::indirect_call(
                Expr::function(0, signature, origin.clone()),
                None,
                vec![Expr::default_value(PrimitiveType::I32, origin.clone())],
                PrimitiveType::I32,
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
            vec![super::LabelledParameter::new(
                "value",
                PrimitiveType::I32,
                None,
            )],
            PrimitiveType::I32,
        );
        let closure = Expr::closure(
            signature.clone(),
            Vec::new(),
            Vec::new(),
            Expr::variable(0, PrimitiveType::I32, origin.clone()),
            1,
            origin.clone(),
        );
        let caller = CheckedFunction::new(
            "caller",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            Expr::indirect_call(
                closure,
                None,
                vec![Expr::default_value(PrimitiveType::I32, origin.clone())],
                PrimitiveType::I32,
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
        let signature =
            FunctionSignature::new(vec![PrimitiveType::I32], PrimitiveType::Str);
        let closure = Expr::closure(
            signature.clone(),
            vec![PrimitiveType::Str],
            Vec::new(),
            Expr::variable(0, PrimitiveType::Str, origin.clone()),
            1,
            origin.clone(),
        );
        let result = CheckedFunction::new(
            "entry",
            FunctionSignature::new(
                Vec::new(),
                PrimitiveType::Function(Box::new(signature)),
            ),
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
                PrimitiveType::I32,
                Some(Value::Str("wrong".to_owned())),
            )],
            PrimitiveType::I32,
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
            FunctionSignature::new(
                Vec::new(),
                PrimitiveType::Function(Box::new(signature)),
            ),
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
            FunctionSignature::new(vec![PrimitiveType::I32], PrimitiveType::I32),
            Expr::variable(0, PrimitiveType::I32, origin.clone()),
            origin.clone(),
        )
        .expect("callee");
        let caller = CheckedFunction::new(
            "caller",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            Expr::call(0, Vec::new(), PrimitiveType::I32, origin.clone()),
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
            PrimitiveType::I32,
            Expr::global(0, PrimitiveType::I32, origin.clone()),
            origin.clone(),
        )
        .expect("global shape");
        let function = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
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
            PrimitiveType::I32,
            Expr::call(0, Vec::new(), PrimitiveType::I32, origin.clone()),
            origin.clone(),
        )
        .expect("global shape");
        let function = CheckedFunction::new(
            "read",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            Expr::global(0, PrimitiveType::I32, origin.clone()),
            origin.clone(),
        )
        .expect("function");
        let result =
            CheckedProgram::try_new_with_globals(vec![global], vec![function], 0);
        assert!(matches!(result, Err(IrError::GlobalInitializerCycle(_))));
    }

    #[test]
    fn higher_order_global_cycles_reach_initializer_validation() {
        let origin = origin();
        let read_signature = FunctionSignature::new(Vec::new(), PrimitiveType::I32);
        let apply_signature = FunctionSignature::new(
            vec![PrimitiveType::Function(Box::new(read_signature.clone()))],
            PrimitiveType::I32,
        );
        let global = super::CheckedGlobal::new(
            "value",
            PrimitiveType::I32,
            Expr::call(
                0,
                vec![Expr::function(1, read_signature.clone(), origin.clone())],
                PrimitiveType::I32,
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
                    PrimitiveType::Function(Box::new(read_signature.clone())),
                    origin.clone(),
                ),
                None,
                Vec::new(),
                PrimitiveType::I32,
                origin.clone(),
            ),
            origin.clone(),
        )
        .expect("apply");
        let read = CheckedFunction::new(
            "read",
            read_signature,
            Expr::global(0, PrimitiveType::I32, origin.clone()),
            origin.clone(),
        )
        .expect("read");
        let answer = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            Expr::global(0, PrimitiveType::I32, origin.clone()),
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
}
