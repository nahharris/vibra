//! The closed compiler intrinsic registry (`docs/spec/06-runtime.md`, "M3
//! compiler intrinsic registry").
//!
//! Every operation is pure, total, deterministic, and host-event free. A
//! partial operation answers with the standard `option` or `result`. The
//! registry never fixes a library type's identity: signatures name the
//! role-playing and `@std.core` types through [`RoleTypes`].

use std::collections::BTreeMap;
use std::sync::OnceLock;

use super::{FunctionSignature, Type, TypeId};

mod vectors;

pub use vectors::{CHARS, Outcome, SCALARS, Vector};

/// The compiler registry identity used by the runtime contract.
///
/// `vibra_v1` is the version named by the v1 runtime specification. The
/// compiler registry and the host registry are separate namespaces, but share
/// this stable toolchain ABI version.
pub const REGISTRY_VERSION: &str = "vibra_v1";

/// A numeric primitive type that carries registry static methods.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NumericType {
    /// `i8`.
    I8,
    /// `i16`.
    I16,
    /// `i32`.
    I32,
    /// `i64`.
    I64,
    /// `u8`.
    U8,
    /// `u16`.
    U16,
    /// `u32`.
    U32,
    /// `u64`.
    U64,
    /// `f32`.
    F32,
    /// `f64`.
    F64,
}

impl NumericType {
    /// Every numeric type, integers first, in width order.
    pub const ALL: [Self; 10] = [
        Self::I8,
        Self::I16,
        Self::I32,
        Self::I64,
        Self::U8,
        Self::U16,
        Self::U32,
        Self::U64,
        Self::F32,
        Self::F64,
    ];

    /// The type's spelling, such as `i32`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
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

    /// The primitive type.
    #[must_use]
    pub const fn to_type(self) -> Type {
        match self {
            Self::I8 => Type::I8,
            Self::I16 => Type::I16,
            Self::I32 => Type::I32,
            Self::I64 => Type::I64,
            Self::U8 => Type::U8,
            Self::U16 => Type::U16,
            Self::U32 => Type::U32,
            Self::U64 => Type::U64,
            Self::F32 => Type::F32,
            Self::F64 => Type::F64,
        }
    }

    /// Whether the type is a fixed-width integer.
    #[must_use]
    pub const fn is_integer(self) -> bool {
        !matches!(self, Self::F32 | Self::F64)
    }

    /// Whether the type is a signed integer.
    #[must_use]
    pub const fn is_signed(self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64)
    }

    /// The bit width.
    #[must_use]
    pub const fn bits(self) -> u32 {
        match self {
            Self::I8 | Self::U8 => 8,
            Self::I16 | Self::U16 => 16,
            Self::I32 | Self::U32 | Self::F32 => 32,
            Self::I64 | Self::U64 | Self::F64 => 64,
        }
    }

    /// The inclusive integer range, as `i128`.
    #[must_use]
    pub const fn range(self) -> (i128, i128) {
        match self {
            Self::I8 => (i8::MIN as i128, i8::MAX as i128),
            Self::I16 => (i16::MIN as i128, i16::MAX as i128),
            Self::I32 => (i32::MIN as i128, i32::MAX as i128),
            Self::I64 => (i64::MIN as i128, i64::MAX as i128),
            Self::U8 => (0, u8::MAX as i128),
            Self::U16 => (0, u16::MAX as i128),
            Self::U32 => (0, u32::MAX as i128),
            Self::U64 => (0, u64::MAX as i128),
            Self::F32 | Self::F64 => (0, 0),
        }
    }
}

/// An integer static method.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntegerOp {
    /// `add-checked`.
    AddChecked,
    /// `sub-checked`.
    SubChecked,
    /// `mul-checked`.
    MulChecked,
    /// `div-checked`.
    DivChecked,
    /// `rem-checked`.
    RemChecked,
    /// `neg-checked`, signed types only.
    NegChecked,
    /// `shift-left-checked`.
    ShiftLeftChecked,
    /// `shift-right`.
    ShiftRight,
    /// `equal`.
    Equal,
    /// `compare`.
    Compare,
    /// `to-str`.
    ToStr,
    /// `parse`.
    Parse,
}

impl IntegerOp {
    const ALL: [Self; 12] = [
        Self::AddChecked,
        Self::SubChecked,
        Self::MulChecked,
        Self::DivChecked,
        Self::RemChecked,
        Self::NegChecked,
        Self::ShiftLeftChecked,
        Self::ShiftRight,
        Self::Equal,
        Self::Compare,
        Self::ToStr,
        Self::Parse,
    ];

    /// The member name, such as `add-checked`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::AddChecked => "add-checked",
            Self::SubChecked => "sub-checked",
            Self::MulChecked => "mul-checked",
            Self::DivChecked => "div-checked",
            Self::RemChecked => "rem-checked",
            Self::NegChecked => "neg-checked",
            Self::ShiftLeftChecked => "shift-left-checked",
            Self::ShiftRight => "shift-right",
            Self::Equal => "equal",
            Self::Compare => "compare",
            Self::ToStr => "to-str",
            Self::Parse => "parse",
        }
    }
}

/// A floating-point static method.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FloatOp {
    /// `add`.
    Add,
    /// `sub`.
    Sub,
    /// `mul`.
    Mul,
    /// `div`.
    Div,
    /// `neg`.
    Neg,
    /// `equal`.
    Equal,
    /// `compare-total`.
    CompareTotal,
    /// `to-str`.
    ToStr,
    /// `parse`.
    Parse,
}

impl FloatOp {
    const ALL: [Self; 9] = [
        Self::Add,
        Self::Sub,
        Self::Mul,
        Self::Div,
        Self::Neg,
        Self::Equal,
        Self::CompareTotal,
        Self::ToStr,
        Self::Parse,
    ];

    /// The member name, such as `compare-total`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
            Self::Neg => "neg",
            Self::Equal => "equal",
            Self::CompareTotal => "compare-total",
            Self::ToStr => "to-str",
            Self::Parse => "parse",
        }
    }
}

/// The semantic operation implemented by a compiler intrinsic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SemanticIdentity {
    /// One integer method, identical across integer types up to their range.
    Integer(IntegerOp),
    /// One IEEE 754 method, identical across widths up to their format.
    Float(FloatOp),
    /// An integer as another integer type: its value when the target holds
    /// it, and `out-of-range` otherwise.
    IntegerConversion,
    /// A `char`'s Unicode scalar value.
    CharToScalar,
    /// The `char` of a Unicode scalar value, or `none`.
    CharFromScalar,
    /// Concatenate Unicode scalar sequences in order.
    UnicodeScalarConcatenation,
    /// Count Unicode scalars, rather than UTF-8 bytes.
    UnicodeScalarLength,
    /// Equal scalar sequences.
    UnicodeScalarEquality,
    /// Lexicographic order by scalar value.
    UnicodeScalarOrder,
    /// Scalars in a half-open range, or `none`.
    UnicodeScalarSlice,
    /// A string's scalars as an array.
    UnicodeScalarsToArray,
    /// A string of the scalars of an array.
    UnicodeScalarsFromArray,
    /// A string's UTF-8 encoding.
    Utf8Encoding,
    /// A string decoded from well-formed UTF-8.
    Utf8Decoding,
    /// Count bytes.
    ByteLength,
    /// Concatenate byte sequences in order.
    ByteConcatenation,
    /// Equal byte sequences.
    ByteEquality,
    /// Lexicographic order by byte.
    ByteOrder,
    /// Bytes in a half-open range, or `none`.
    ByteSlice,
    /// Bytes as an array of `u8`.
    BytesToArray,
    /// Bytes of an array of `u8`.
    BytesFromArray,
    /// Build an array from its packed variadic elements.
    ArrayConstruction,
    /// Build a dict from its packed variadic entries in canonical key order, a
    /// later entry replacing an equal key.
    DictConstruction,
    /// A dict's entries as an array of key-value tuples, in key order.
    DictEntries,
    /// Count array elements.
    ArrayLength,
    /// A new array with one trailing element.
    ArrayAppend,
    /// A new array of the first array's elements, then the second's.
    ArrayConcatenation,
    /// Elements in a half-open range, or `none` when it is out of range.
    ArraySlice,
    /// A left fold of an array through a step function.
    ArrayFold,
}

/// One pure, compiler-owned operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompilerIntrinsic {
    /// An integer static method of a numeric primitive type.
    Integer(NumericType, IntegerOp),
    /// A floating-point static method of `f32` or `f64`.
    Float(NumericType, FloatOp),
    /// `S.to-T`: an integer of the first type as the second, distinct,
    /// integer type.
    Convert(NumericType, NumericType),
    /// `char.to-u32`.
    CharToU32,
    /// `char.from-u32`.
    CharFromU32,
    /// `text.concat`.
    TextConcat,
    /// `text.length`.
    TextLength,
    /// `text.equal`.
    TextEqual,
    /// `text.compare`.
    TextCompare,
    /// `text.slice`.
    TextSlice,
    /// `text.to-chars`.
    TextToChars,
    /// `text.from-chars`.
    TextFromChars,
    /// `text.to-utf8`.
    TextToUtf8,
    /// `text.from-utf8`.
    TextFromUtf8,
    /// `bytes.length`.
    BytesLength,
    /// `bytes.concat`.
    BytesConcat,
    /// `bytes.equal`.
    BytesEqual,
    /// `bytes.compare`.
    BytesCompare,
    /// `bytes.slice`.
    BytesSlice,
    /// `bytes.to-array`.
    BytesToArray,
    /// `bytes.from-array`.
    BytesFromArray,
    /// `array.of`.
    ArrayOf,
    /// `dict.of`.
    DictOf,
    /// `dict.entries`.
    DictEntries,
    /// `array.length`.
    ArrayLength,
    /// `array.append`.
    ArrayAppend,
    /// `array.concat`.
    ArrayConcat,
    /// `array.slice`.
    ArraySlice,
    /// `array.fold`.
    ArrayFold,
}

/// The standard-library types a registry signature names: the types playing
/// the `@option` and `@result` roles, and the ordering and error types of
/// `@std.core`. A type nothing declares is `void`, which no declaration can
/// match.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoleTypes {
    option: Option<TypeId>,
    result: Option<TypeId>,
    ordering: Option<TypeId>,
    arithmetic_error: Option<TypeId>,
    conversion_error: Option<TypeId>,
}

impl RoleTypes {
    /// Roles bound to the type playing `@option`, when one does.
    #[must_use]
    pub const fn new(option: Option<TypeId>) -> Self {
        Self {
            option,
            result: None,
            ordering: None,
            arithmetic_error: None,
            conversion_error: None,
        }
    }

    /// The same roles with the type playing `@result`.
    #[must_use]
    pub fn with_result(mut self, result: Option<TypeId>) -> Self {
        self.result = result;
        self
    }

    /// The same roles with `@std.core`'s `ordering`, `arithmetic-error`, and
    /// `conversion-error`.
    #[must_use]
    pub fn with_core(
        mut self,
        ordering: Option<TypeId>,
        arithmetic_error: Option<TypeId>,
        conversion_error: Option<TypeId>,
    ) -> Self {
        self.ordering = ordering;
        self.arithmetic_error = arithmetic_error;
        self.conversion_error = conversion_error;
        self
    }

    /// `(option value)`.
    #[must_use]
    pub fn option_of(&self, value: Type) -> Type {
        self.option
            .as_ref()
            .map_or(Type::Void, |id| Type::Applied(id.clone(), vec![value]))
    }

    /// `(result value error)`.
    #[must_use]
    pub fn result_of(&self, value: Type, error: Type) -> Type {
        self.result.as_ref().map_or(Type::Void, |id| {
            Type::Applied(id.clone(), vec![value, error])
        })
    }

    /// `ordering`.
    #[must_use]
    pub fn ordering(&self) -> Type {
        declared(self.ordering.as_ref())
    }

    /// `arithmetic-error`.
    #[must_use]
    pub fn arithmetic_error(&self) -> Type {
        declared(self.arithmetic_error.as_ref())
    }

    /// `conversion-error`.
    #[must_use]
    pub fn conversion_error(&self) -> Type {
        declared(self.conversion_error.as_ref())
    }
}

fn declared(id: Option<&TypeId>) -> Type {
    id.map_or(Type::Void, |id| Type::Declared(id.clone()))
}

fn param(name: &str) -> Type {
    Type::Param(name.to_owned())
}

fn array_of(element: Type) -> Type {
    Type::Array(Box::new(element))
}

fn function(parameters: Vec<Type>, result: Type) -> FunctionSignature {
    FunctionSignature::new(parameters, result)
}

impl CompilerIntrinsic {
    /// Whether every value of the integer type `source` is a value of the
    /// integer type `target`, so `source.to-target` cannot fail.
    #[must_use]
    pub const fn conversion_is_total(source: NumericType, target: NumericType) -> bool {
        let (source_low, source_high) = source.range();
        let (target_low, target_high) = target.range();
        target_low <= source_low && source_high <= target_high
    }

    /// The closed registry version for this operation.
    #[must_use]
    pub const fn registry_version(self) -> &'static str {
        REGISTRY_VERSION
    }

    /// The semantic contract implemented by this operation.
    #[must_use]
    pub const fn semantic_identity(self) -> SemanticIdentity {
        match self {
            Self::Integer(_, op) => SemanticIdentity::Integer(op),
            Self::Float(_, op) => SemanticIdentity::Float(op),
            Self::Convert(..) => SemanticIdentity::IntegerConversion,
            Self::CharToU32 => SemanticIdentity::CharToScalar,
            Self::CharFromU32 => SemanticIdentity::CharFromScalar,
            Self::TextConcat => SemanticIdentity::UnicodeScalarConcatenation,
            Self::TextLength => SemanticIdentity::UnicodeScalarLength,
            Self::TextEqual => SemanticIdentity::UnicodeScalarEquality,
            Self::TextCompare => SemanticIdentity::UnicodeScalarOrder,
            Self::TextSlice => SemanticIdentity::UnicodeScalarSlice,
            Self::TextToChars => SemanticIdentity::UnicodeScalarsToArray,
            Self::TextFromChars => SemanticIdentity::UnicodeScalarsFromArray,
            Self::TextToUtf8 => SemanticIdentity::Utf8Encoding,
            Self::TextFromUtf8 => SemanticIdentity::Utf8Decoding,
            Self::BytesLength => SemanticIdentity::ByteLength,
            Self::BytesConcat => SemanticIdentity::ByteConcatenation,
            Self::BytesEqual => SemanticIdentity::ByteEquality,
            Self::BytesCompare => SemanticIdentity::ByteOrder,
            Self::BytesSlice => SemanticIdentity::ByteSlice,
            Self::BytesToArray => SemanticIdentity::BytesToArray,
            Self::BytesFromArray => SemanticIdentity::BytesFromArray,
            Self::ArrayOf => SemanticIdentity::ArrayConstruction,
            Self::DictOf => SemanticIdentity::DictConstruction,
            Self::DictEntries => SemanticIdentity::DictEntries,
            Self::ArrayLength => SemanticIdentity::ArrayLength,
            Self::ArrayAppend => SemanticIdentity::ArrayAppend,
            Self::ArrayConcat => SemanticIdentity::ArrayConcatenation,
            Self::ArraySlice => SemanticIdentity::ArraySlice,
            Self::ArrayFold => SemanticIdentity::ArrayFold,
        }
    }

    /// The stable registry symbol, such as `i32.add-checked` or
    /// `text.concat`.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Integer(..) | Self::Float(..) | Self::Convert(..) => {
                numeric_symbols().get(&self).map_or("", String::as_str)
            }
            Self::CharToU32 => "char.to-u32",
            Self::CharFromU32 => "char.from-u32",
            Self::TextConcat => "text.concat",
            Self::TextLength => "text.length",
            Self::TextEqual => "text.equal",
            Self::TextCompare => "text.compare",
            Self::TextSlice => "text.slice",
            Self::TextToChars => "text.to-chars",
            Self::TextFromChars => "text.from-chars",
            Self::TextToUtf8 => "text.to-utf8",
            Self::TextFromUtf8 => "text.from-utf8",
            Self::BytesLength => "bytes.length",
            Self::BytesConcat => "bytes.concat",
            Self::BytesEqual => "bytes.equal",
            Self::BytesCompare => "bytes.compare",
            Self::BytesSlice => "bytes.slice",
            Self::BytesToArray => "bytes.to-array",
            Self::BytesFromArray => "bytes.from-array",
            Self::ArrayOf => "array.of",
            Self::DictOf => "dict.of",
            Self::DictEntries => "dict.entries",
            Self::ArrayLength => "array.length",
            Self::ArrayAppend => "array.append",
            Self::ArrayConcat => "array.concat",
            Self::ArraySlice => "array.slice",
            Self::ArrayFold => "array.fold",
        }
    }

    /// Whether this entry is a native implementation of a standard-library
    /// function that keeps its Vibra body, named with `native:`, rather than a
    /// bodiless primitive operation named with `external:`.
    #[must_use]
    pub const fn is_native(self) -> bool {
        !matches!(
            self,
            Self::Integer(..)
                | Self::Float(..)
                | Self::Convert(..)
                | Self::CharToU32
                | Self::CharFromU32
                | Self::DictEntries
                | Self::ArrayLength
                | Self::ArrayAppend
                | Self::ArrayConcat
                | Self::ArraySlice
        )
    }

    /// The sample vectors of this row: operands, and the outcome the
    /// specification gives them, computed once from the language's own integer
    /// types and independent of any backend. Every backend that lowers the row
    /// is held to them. A row without vectors has none yet: the vectors cover
    /// the integer rows but `to-str` and `parse`, the integer conversions, and
    /// the `char` rows.
    #[must_use]
    pub fn vectors(self) -> &'static [Vector] {
        vectors::of(self)
    }

    /// The generic parameters of the exact signature, in `where:` order.
    #[must_use]
    pub fn type_parameters(self) -> Vec<String> {
        match self {
            Self::DictOf | Self::DictEntries => vec!["k".to_owned(), "v".to_owned()],
            Self::ArrayFold => vec!["t".to_owned(), "a".to_owned()],
            Self::ArrayOf
            | Self::ArrayLength
            | Self::ArrayAppend
            | Self::ArrayConcat
            | Self::ArraySlice => vec!["t".to_owned()],
            _ => Vec::new(),
        }
    }

    /// The exact checked signature, with the library types bound by `roles`.
    #[must_use]
    pub fn signature(self, roles: &RoleTypes) -> FunctionSignature {
        let items = || array_of(param("t"));
        let checked = |value: Type| roles.result_of(value, roles.arithmetic_error());
        let converted = |value: Type| roles.result_of(value, roles.conversion_error());
        match self {
            Self::Convert(source, target) => function(
                vec![source.to_type()],
                if Self::conversion_is_total(source, target) {
                    target.to_type()
                } else {
                    converted(target.to_type())
                },
            ),
            Self::Integer(numeric, op) => {
                let value = numeric.to_type();
                match op {
                    IntegerOp::AddChecked
                    | IntegerOp::SubChecked
                    | IntegerOp::MulChecked
                    | IntegerOp::DivChecked
                    | IntegerOp::RemChecked => {
                        function(vec![value.clone(), value.clone()], checked(value))
                    }
                    IntegerOp::NegChecked => {
                        function(vec![value.clone()], checked(value))
                    }
                    IntegerOp::ShiftLeftChecked | IntegerOp::ShiftRight => {
                        function(vec![value.clone(), Type::U32], checked(value))
                    }
                    IntegerOp::Equal => {
                        function(vec![value.clone(), value], Type::Bool)
                    }
                    IntegerOp::Compare => {
                        function(vec![value.clone(), value], roles.ordering())
                    }
                    IntegerOp::ToStr => function(vec![value], Type::Str),
                    IntegerOp::Parse => function(vec![Type::Str], converted(value)),
                }
            }
            Self::Float(numeric, op) => {
                let value = numeric.to_type();
                match op {
                    FloatOp::Add | FloatOp::Sub | FloatOp::Mul | FloatOp::Div => {
                        function(vec![value.clone(), value.clone()], value)
                    }
                    FloatOp::Neg => function(vec![value.clone()], value),
                    FloatOp::Equal => function(vec![value.clone(), value], Type::Bool),
                    FloatOp::CompareTotal => {
                        function(vec![value.clone(), value], roles.ordering())
                    }
                    FloatOp::ToStr => function(vec![value], Type::Str),
                    FloatOp::Parse => function(vec![Type::Str], converted(value)),
                }
            }
            Self::CharToU32 => function(vec![Type::Char], Type::U32),
            Self::CharFromU32 => function(vec![Type::U32], roles.option_of(Type::Char)),
            Self::TextConcat => function(vec![Type::Str, Type::Str], Type::Str),
            Self::TextLength => function(vec![Type::Str], Type::U64),
            Self::TextEqual => function(vec![Type::Str, Type::Str], Type::Bool),
            Self::TextCompare => function(vec![Type::Str, Type::Str], roles.ordering()),
            Self::TextSlice => function(
                vec![Type::Str, Type::U64, Type::U64],
                roles.option_of(Type::Str),
            ),
            Self::TextToChars => function(vec![Type::Str], array_of(Type::Char)),
            Self::TextFromChars => function(vec![array_of(Type::Char)], Type::Str),
            Self::TextToUtf8 => function(vec![Type::Str], Type::Bytes),
            Self::TextFromUtf8 => function(vec![Type::Bytes], converted(Type::Str)),
            Self::BytesLength => function(vec![Type::Bytes], Type::U64),
            Self::BytesConcat => function(vec![Type::Bytes, Type::Bytes], Type::Bytes),
            Self::BytesEqual => function(vec![Type::Bytes, Type::Bytes], Type::Bool),
            Self::BytesCompare => {
                function(vec![Type::Bytes, Type::Bytes], roles.ordering())
            }
            Self::BytesSlice => function(
                vec![Type::Bytes, Type::U64, Type::U64],
                roles.option_of(Type::Bytes),
            ),
            Self::BytesToArray => function(vec![Type::Bytes], array_of(Type::U8)),
            Self::BytesFromArray => function(vec![array_of(Type::U8)], Type::Bytes),
            Self::ArrayOf => function(Vec::new(), items()).with_variadic(items()),
            Self::DictOf => {
                let dict = Type::Dict(Box::new(param("k")), Box::new(param("v")));
                function(Vec::new(), dict.clone()).with_variadic(dict)
            }
            Self::DictEntries => function(
                vec![Type::Dict(Box::new(param("k")), Box::new(param("v")))],
                array_of(Type::Tuple(vec![param("k"), param("v")])),
            ),
            Self::ArrayLength => function(vec![items()], Type::U64),
            Self::ArrayAppend => function(vec![items(), param("t")], items()),
            Self::ArrayConcat => function(vec![items(), items()], items()),
            Self::ArraySlice => function(
                vec![items(), Type::U64, Type::U64],
                roles.option_of(items()),
            ),
            Self::ArrayFold => {
                let step = Type::Function(Box::new(function(
                    vec![param("a"), param("t")],
                    param("a"),
                )));
                function(vec![items(), param("a"), step], param("a"))
            }
        }
    }

    /// Resolves only a symbol in the closed registry.
    #[must_use]
    pub fn from_symbol(symbol: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|intrinsic| intrinsic.symbol() == symbol)
    }

    /// Every compiler intrinsic in canonical registry order: the numeric
    /// methods by type, then the module rows, then the collection rows.
    #[must_use]
    pub fn all() -> Vec<Self> {
        let mut all = Vec::new();
        for numeric in NumericType::ALL {
            if numeric.is_integer() {
                all.extend(
                    IntegerOp::ALL
                        .into_iter()
                        .filter(|op| {
                            *op != IntegerOp::NegChecked || numeric.is_signed()
                        })
                        .map(|op| Self::Integer(numeric, op)),
                );
            } else {
                all.extend(FloatOp::ALL.into_iter().map(|op| Self::Float(numeric, op)));
            }
        }
        for source in NumericType::ALL {
            for target in NumericType::ALL {
                if source != target && source.is_integer() && target.is_integer() {
                    all.push(Self::Convert(source, target));
                }
            }
        }
        all.extend([
            Self::CharToU32,
            Self::CharFromU32,
            Self::TextConcat,
            Self::TextLength,
            Self::TextEqual,
            Self::TextCompare,
            Self::TextSlice,
            Self::TextToChars,
            Self::TextFromChars,
            Self::TextToUtf8,
            Self::TextFromUtf8,
            Self::BytesLength,
            Self::BytesConcat,
            Self::BytesEqual,
            Self::BytesCompare,
            Self::BytesSlice,
            Self::BytesToArray,
            Self::BytesFromArray,
            Self::ArrayOf,
            Self::DictOf,
            Self::DictEntries,
            Self::ArrayLength,
            Self::ArrayAppend,
            Self::ArrayConcat,
            Self::ArraySlice,
            Self::ArrayFold,
        ]);
        all
    }
}

/// `T.<name>` for every numeric method, built once.
fn numeric_symbols() -> &'static BTreeMap<CompilerIntrinsic, String> {
    static SYMBOLS: OnceLock<BTreeMap<CompilerIntrinsic, String>> = OnceLock::new();
    SYMBOLS.get_or_init(|| {
        let mut symbols = BTreeMap::new();
        for numeric in NumericType::ALL {
            for op in IntegerOp::ALL {
                symbols.insert(
                    CompilerIntrinsic::Integer(numeric, op),
                    format!("{}.{}", numeric.name(), op.name()),
                );
            }
            for op in FloatOp::ALL {
                symbols.insert(
                    CompilerIntrinsic::Float(numeric, op),
                    format!("{}.{}", numeric.name(), op.name()),
                );
            }
            for target in NumericType::ALL {
                symbols.insert(
                    CompilerIntrinsic::Convert(numeric, target),
                    format!("{}.to-{}", numeric.name(), target.name()),
                );
            }
        }
        symbols
    })
}
