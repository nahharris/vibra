//! The small, backend independent IR admitted by M2 Step 5.
//!
//! The type checker is the only workspace phase that constructs a
//! [`CheckedProgram`].  The interpreter consumes that type, rather than a
//! parsed syntax tree, which makes the checked-program boundary explicit.
//! This IR slice contains primitive values, immutable bindings, literal
//! sequences, conditionals, fixed positional calls, and function signatures;
//! effects and collections belong to later steps.

use std::collections::BTreeSet;
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

    /// Number of immutable activation slots required by this expression.
    #[must_use]
    pub fn slot_count(&self) -> usize {
        match self {
            Self::Literal { .. } | Self::Global { .. } => 0,
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
            Self::Call { arguments, .. } => {
                arguments.iter().map(Self::slot_count).max().unwrap_or(0)
            }
        }
    }

    fn validate_shape(
        &self,
        slots: &mut [Option<PrimitiveType>],
    ) -> Result<PrimitiveType, IrError> {
        match self {
            Self::Literal { value, .. } => Ok(value.ty()),
            Self::Sequence { expressions, .. } => {
                let mut result = PrimitiveType::Void;
                for expression in expressions {
                    result = expression.validate_shape(slots)?;
                }
                Ok(result)
            }
            Self::Variable {
                slot, value_type, ..
            } => match slots.get(*slot).copied().flatten() {
                Some(actual) if actual == *value_type => Ok(*value_type),
                Some(actual) => Err(IrError::InvalidExpression(format!(
                    "variable slot {slot} has type {actual}, expression declares {value_type}"
                ))),
                None => Err(IrError::InvalidExpression(format!(
                    "variable slot {slot} is not bound"
                ))),
            },
            Self::Global { value_type, .. } => Ok(*value_type),
            Self::Let {
                slot, value, body, ..
            } => {
                let value_type = value.validate_shape(slots)?;
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
                body.validate_shape(&mut body_slots)
            }
            Self::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let condition_type = condition.validate_shape(slots)?;
                if condition_type != PrimitiveType::Bool {
                    return Err(IrError::InvalidExpression(format!(
                        "if condition has type {condition_type}, expected bool"
                    )));
                }
                let mut then_slots = slots.to_vec();
                let mut else_slots = slots.to_vec();
                let then_type = then_branch.validate_shape(&mut then_slots)?;
                let else_type = else_branch.validate_shape(&mut else_slots)?;
                if then_type != else_type {
                    return Err(IrError::InvalidExpression(format!(
                        "if branches have types {then_type} and {else_type}"
                    )));
                }
                Ok(then_type)
            }
            Self::Call {
                arguments, result, ..
            } => {
                for argument in arguments {
                    argument.validate_shape(slots)?;
                }
                Ok(*result)
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
        let slot_count = initializer.slot_count();
        let mut slots = vec![None; slot_count];
        let actual = initializer.validate_shape(&mut slots)?;
        if actual != value_type {
            return Err(IrError::ResultTypeMismatch {
                expected: value_type,
                actual,
            });
        }
        Ok(Self {
            name: name.into(),
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
        let slot_count = signature.parameters().len().max(body.slot_count());
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
        let mut slots = vec![None; slot_count];
        for (slot, value_type) in signature.parameters().iter().enumerate() {
            if let Some(bound) = slots.get_mut(slot) {
                *bound = Some(*value_type);
            }
        }
        let actual = body.validate_shape(&mut slots)?;
        if actual != signature.result() {
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
            let mut slots = vec![None; function.slot_count];
            for (slot, value_type) in function.signature.parameters().iter().enumerate()
            {
                if let Some(bound) = slots.get_mut(slot) {
                    *bound = Some(*value_type);
                }
            }
            let actual = function.body.validate_shape(&mut slots)?;
            if actual != function.signature.result() {
                return Err(IrError::ResultTypeMismatch {
                    expected: function.signature.result(),
                    actual,
                });
            }
        }
        for global in &globals {
            let mut slots = vec![None; global.slot_count];
            let actual = global.initializer.validate_shape(&mut slots)?;
            if actual != global.value_type {
                return Err(IrError::ResultTypeMismatch {
                    expected: global.value_type,
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
        reject_recursive_calls(&calls)?;
        reject_global_initializer_cycles(&dependencies, globals.len())?;
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
        Expr::Literal { .. } | Expr::Variable { .. } => {}
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
            if global.value_type() != *value_type {
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
            ..
        } => {
            let Some(callee) = functions.get(*function) else {
                return Err(IrError::InvalidExpression(format!(
                    "function index {function} is outside the program"
                )));
            };
            if arguments.len() != callee.signature().parameters().len() {
                return Err(IrError::InvalidExpression(format!(
                    "call to `{}` has {} arguments, expected {}",
                    callee.name(),
                    arguments.len(),
                    callee.signature().parameters().len()
                )));
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
            for (argument, expected) in
                arguments.iter().zip(callee.signature().parameters())
            {
                let actual = argument.result_type();
                if actual != *expected {
                    return Err(IrError::InvalidExpression(format!(
                        "call argument has type {actual}, expected {expected}"
                    )));
                }
            }
            if *result != callee.signature().result() {
                return Err(IrError::InvalidExpression(format!(
                    "call result declares {result}, callee returns {}",
                    callee.signature().result()
                )));
            }
            if let Some(owner) = owner {
                let Some(edges) = dependencies.get_mut(owner.node_index(globals.len()))
                else {
                    return Err(IrError::InvalidExpression(format!(
                        "dependency owner {owner:?} is outside the program"
                    )));
                };
                edges.insert(DependencyNode::Function(*function));
                if let DependencyNode::Function(owner) = owner {
                    let Some(edges) = calls.get_mut(owner) else {
                        return Err(IrError::InvalidExpression(format!(
                            "function owner index {owner} is outside the program"
                        )));
                    };
                    edges.insert(*function);
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallState {
    Unvisited,
    Visiting,
    Done,
}

fn reject_recursive_calls(calls: &[BTreeSet<usize>]) -> Result<(), IrError> {
    let mut states = vec![CallState::Unvisited; calls.len()];
    for index in 0..calls.len() {
        visit_call_graph(index, calls, &mut states)?;
    }
    Ok(())
}

fn visit_call_graph(
    index: usize,
    calls: &[BTreeSet<usize>],
    states: &mut [CallState],
) -> Result<(), IrError> {
    match states.get(index).copied() {
        Some(CallState::Done) => return Ok(()),
        Some(CallState::Visiting) => {
            return Err(IrError::RecursiveCall(format!(
                "function index {index} is part of a recursive group"
            )));
        }
        Some(CallState::Unvisited) => {}
        None => {
            return Err(IrError::InvalidExpression(format!(
                "call graph references function index {index}"
            )));
        }
    }
    let Some(state) = states.get_mut(index) else {
        return Err(IrError::InvalidExpression(format!(
            "call graph references function index {index}"
        )));
    };
    *state = CallState::Visiting;
    let Some(dependencies) = calls.get(index) else {
        return Err(IrError::InvalidExpression(format!(
            "call graph has no node for function index {index}"
        )));
    };
    for dependency in dependencies {
        visit_call_graph(*dependency, calls, states)?;
    }
    if let Some(state) = states.get_mut(index) {
        *state = CallState::Done;
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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{
        ByteSpan, CheckedFunction, CheckedProgram, Expr, FunctionSignature, IrError,
        PrimitiveType, SourceOrigin, Value,
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
    fn program_constructor_rejects_recursive_calls() {
        let origin = origin();
        let function = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            Expr::call(0, Vec::new(), PrimitiveType::I32, origin.clone()),
            origin,
        )
        .expect("call shape is valid before program binding");
        let result = CheckedProgram::try_new(vec![function], 0);
        assert!(matches!(result, Err(IrError::RecursiveCall(_))));
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
}
