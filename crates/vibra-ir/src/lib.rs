//! The small, backend independent IR admitted by M2 Step 5.
//!
//! The type checker is the only workspace phase that constructs a
//! [`CheckedProgram`].  The interpreter consumes that type, rather than a
//! parsed syntax tree, which makes the checked-program boundary explicit.
//! This IR slice contains primitive values, immutable bindings, literal
//! sequences, conditionals, fixed positional calls, and function signatures;
//! effects and collections belong to later steps.

use std::fmt;

use vibra_diagnostics::ByteSpan;

/// One of the primitive types admitted by the M2 literal profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
}

impl PrimitiveType {
    /// The canonical source/type spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
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
        }
    }

    /// Whether this type is one of the fixed-width integer types.
    #[must_use]
    pub const fn is_integer(self) -> bool {
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
    pub const fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }
}

impl fmt::Display for PrimitiveType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One fully checked monomorphic function signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionSignature {
    parameters: Vec<PrimitiveType>,
    result: PrimitiveType,
}

impl FunctionSignature {
    /// Creates a signature from its checked primitive slots.
    #[must_use]
    pub fn new(parameters: Vec<PrimitiveType>, result: PrimitiveType) -> Self {
        Self { parameters, result }
    }

    /// Required positional parameter types in written order.
    #[must_use]
    pub fn parameters(&self) -> &[PrimitiveType] {
        &self.parameters
    }

    /// The declared result type.
    #[must_use]
    pub const fn result(&self) -> PrimitiveType {
        self.result
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
    pub const fn ty(&self) -> PrimitiveType {
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
        /// The statically checked result type.
        result: PrimitiveType,
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
            result,
            origin,
        }
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
            | Self::Sequence { origin, .. }
            | Self::Variable { origin, .. }
            | Self::Global { origin, .. }
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
            Self::Sequence { expressions, .. } => expressions
                .last()
                .map_or(PrimitiveType::Void, Self::result_type),
            Self::Variable { value_type, .. } | Self::Global { value_type, .. } => {
                *value_type
            }
            Self::Let { body, .. } => body.result_type(),
            Self::If { then_branch, .. } => then_branch.result_type(),
            Self::Call { result, .. } => *result,
        }
    }

    /// The sequence members, or an empty slice for a literal.
    #[must_use]
    pub fn expressions(&self) -> &[Self] {
        match self {
            Self::Literal { .. }
            | Self::Variable { .. }
            | Self::Global { .. }
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
            Self::Sequence { .. }
            | Self::Variable { .. }
            | Self::Global { .. }
            | Self::Let { .. }
            | Self::If { .. }
            | Self::Call { .. } => None,
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
}

impl CheckedGlobal {
    /// Creates a checked global after validating its initializer type.
    pub fn new(
        name: impl Into<String>,
        value_type: PrimitiveType,
        initializer: Expr,
        origin: SourceOrigin,
    ) -> Result<Self, IrError> {
        if initializer.result_type() != value_type {
            return Err(IrError::ResultTypeMismatch {
                expected: value_type,
                actual: initializer.result_type(),
            });
        }
        Ok(Self {
            name: name.into(),
            value_type,
            initializer,
            origin,
        })
    }

    /// Global name in its owning module.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The checked global type.
    #[must_use]
    pub const fn value_type(&self) -> PrimitiveType {
        self.value_type
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
}

/// One checked module-level function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedFunction {
    name: String,
    signature: FunctionSignature,
    body: Expr,
    origin: SourceOrigin,
    slot_count: usize,
}

impl CheckedFunction {
    /// Creates a checked function, rejecting a body whose final type differs
    /// from its written result type.
    pub fn new(
        name: impl Into<String>,
        signature: FunctionSignature,
        body: Expr,
        origin: SourceOrigin,
    ) -> Result<Self, IrError> {
        let slot_count = signature.parameters().len();
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
        let name = name.into();
        if slot_count < signature.parameters().len() {
            return Err(IrError::InvalidSlotCount {
                function: name,
                slots: slot_count,
                parameters: signature.parameters().len(),
            });
        }
        if body.result_type() != signature.result() {
            return Err(IrError::ResultTypeMismatch {
                expected: signature.result(),
                actual: body.result_type(),
            });
        }
        Ok(Self {
            name,
            signature,
            body,
            origin,
            slot_count,
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
}

/// A complete immutable program that crossed the checker boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedProgram {
    globals: Vec<CheckedGlobal>,
    functions: Vec<CheckedFunction>,
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
            if function.body.result_type() != function.signature.result() {
                return Err(IrError::ResultTypeMismatch {
                    expected: function.signature.result(),
                    actual: function.body.result_type(),
                });
            }
        }
        for global in &globals {
            if global.initializer.result_type() != global.value_type {
                return Err(IrError::ResultTypeMismatch {
                    expected: global.value_type,
                    actual: global.initializer.result_type(),
                });
            }
        }
        Ok(Self {
            globals,
            functions,
            entry,
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
                    "    (record name: @{} type: @{} body: {})\n",
                    global.name,
                    global.value_type.as_str(),
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
                .map(|parameter| format!("@{}", parameter.as_str()))
                .collect::<Vec<_>>();
            let body = canonical_expr(function.body());
            output.push_str(&format!(
                "    (record name: @{} params: {} result: @{} body: {})\n",
                function.name,
                canonical_array(&parameters),
                function.signature.result().as_str(),
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
        }
    }
}

impl std::error::Error for IrError {}

fn canonical_expr(expression: &Expr) -> String {
    match expression {
        Expr::Literal { value, .. } => format!(
            "(record kind: @literal type: @{} value: {})",
            value.ty().as_str(),
            value.canonical_vibon()
        ),
        Expr::Sequence { expressions, .. } => {
            let values = expressions.iter().map(canonical_expr).collect::<Vec<_>>();
            format!(
                "(record kind: @sequence type: @{} values: {})",
                expression.result_type().as_str(),
                canonical_array(&values)
            )
        }
        Expr::Variable {
            slot, value_type, ..
        } => format!(
            "(record kind: @variable slot: {}u64 type: @{})",
            slot,
            value_type.as_str()
        ),
        Expr::Global {
            index, value_type, ..
        } => format!(
            "(record kind: @global index: {}u64 type: @{})",
            index,
            value_type.as_str()
        ),
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
            ..
        } => {
            let values = arguments.iter().map(canonical_expr).collect::<Vec<_>>();
            format!(
                "(record kind: @call function: {}u64 result: @{} arguments: {})",
                function,
                result.as_str(),
                canonical_array(&values)
            )
        }
    }
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
