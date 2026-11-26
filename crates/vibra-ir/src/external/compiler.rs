//! The closed `@compiler` registry at major 2, minor 0
//! (`docs/spec/06-runtime.md`, "Compiler intrinsic registry").
//!
//! Every row has a wire symbol `<owner>.<member>`, with a lower-case owner
//! and a camelCase member, exact generic, parameter, and result types over
//! the role-bound library types and the [narrowing types](NarrowingType),
//! a purity flag, and a totality mark. Every row is pure except the three
//! assertion rows. Only the embedded standard library binds a row.
//!
//! A row marked **total** is pure, total, and cannot fault, so a call of a
//! function bound to it is an atom of the certified fragment of guard
//! predicates (`docs/spec/02-type-system.md`, "Predicates"). The one such
//! row is `fs.validNames`, the name profile of `RelativePath`
//! ([`crate::names`]).
//!
//! The rows `grant.restrict`, `grant.sub`, and `grant.limit` are the three
//! narrowing members of every standard operation interface, generic over the
//! grant type `G`. They are rows of 2.0, with `since` 0.
//!
//! The rows are frozen in `registry/compiler-2.0.txt`, one line per row in
//! symbol order: the symbol, the generics, the parameters, the result, and
//! the marks, separated by tabs, the types in source spelling. The marks are
//! `@total` for a total row and empty otherwise.

use std::fmt::Write as _;

use super::vectors::{self, Vector};
use crate::{IrError, Library, LibraryTypes, Type};

/// The registry's major version.
pub const REGISTRY_MAJOR: u32 = 2;

/// The registry's minor version.
pub const REGISTRY_MINOR: u32 = 0;

/// An integer type that owns registry rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntegerType {
    /// `I8`.
    I8,
    /// `I16`.
    I16,
    /// `I32`.
    I32,
    /// `I64`.
    I64,
    /// `U8`.
    U8,
    /// `U16`.
    U16,
    /// `U32`.
    U32,
    /// `U64`.
    U64,
}

impl IntegerType {
    /// Every integer type, signed first, in width order.
    pub const ALL: [Self; 8] = [
        Self::I8,
        Self::I16,
        Self::I32,
        Self::I64,
        Self::U8,
        Self::U16,
        Self::U32,
        Self::U64,
    ];

    /// The lower-case owner of the type's rows, such as `i32`.
    #[must_use]
    pub const fn owner(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
        }
    }

    /// The type's source name, such as `I32`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::I8 => "I8",
            Self::I16 => "I16",
            Self::I32 => "I32",
            Self::I64 => "I64",
            Self::U8 => "U8",
            Self::U16 => "U16",
            Self::U32 => "U32",
            Self::U64 => "U64",
        }
    }

    /// The checked type.
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
        }
    }

    /// Whether the type is signed.
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
            Self::I32 | Self::U32 => 32,
            Self::I64 | Self::U64 => 64,
        }
    }

    /// The inclusive range of the type.
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
        }
    }
}

/// A floating-point type that owns registry rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FloatType {
    /// `F32`.
    F32,
    /// `F64`.
    F64,
}

impl FloatType {
    /// Both float types.
    pub const ALL: [Self; 2] = [Self::F32, Self::F64];

    /// The lower-case owner of the type's rows.
    #[must_use]
    pub const fn owner(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    /// The checked type.
    #[must_use]
    pub const fn to_type(self) -> Type {
        match self {
            Self::F32 => Type::F32,
            Self::F64 => Type::F64,
        }
    }
}

/// An integer row's member.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntegerOp {
    /// `addChecked`: the exact sum, or `overflow`.
    AddChecked,
    /// `subChecked`: the exact difference, or `overflow`.
    SubChecked,
    /// `mulChecked`: the exact product, or `overflow`.
    MulChecked,
    /// `divChecked`: the quotient truncated toward zero, `divisionByZero`,
    /// or `overflow` for the signed minimum divided by -1.
    DivChecked,
    /// `remChecked`: the remainder with the dividend's sign, or
    /// `divisionByZero`.
    RemChecked,
    /// `negChecked`, signed types only: the exact negation, or `overflow`.
    NegChecked,
    /// `shiftLeftChecked`: `invalidShift`, the exact product, or `overflow`.
    ShiftLeftChecked,
    /// `shiftRightChecked`: `invalidShift`, or floor division.
    ShiftRightChecked,
    /// `add`: the sum, clamped to the bounds.
    Add,
    /// `sub`: the difference, clamped to the bounds.
    Sub,
    /// `mul`: the product, clamped to the bounds.
    Mul,
    /// `neg`, signed types only: the negation, clamped to the bounds.
    Neg,
    /// `shiftLeft`: `value * 2^amount`, clamped, for every amount.
    ShiftLeft,
    /// `shiftRight`: floor division by `2^amount`, for every amount.
    ShiftRight,
    /// `equal`: numeric equality.
    Equal,
    /// `compare`: numeric order.
    Compare,
    /// `toStr`: the shortest decimal digits.
    ToStr,
    /// `parse`: the decimal grammar of the chapter.
    Parse,
}

impl IntegerOp {
    /// Every integer member.
    pub const ALL: [Self; 18] = [
        Self::AddChecked,
        Self::SubChecked,
        Self::MulChecked,
        Self::DivChecked,
        Self::RemChecked,
        Self::NegChecked,
        Self::ShiftLeftChecked,
        Self::ShiftRightChecked,
        Self::Add,
        Self::Sub,
        Self::Mul,
        Self::Neg,
        Self::ShiftLeft,
        Self::ShiftRight,
        Self::Equal,
        Self::Compare,
        Self::ToStr,
        Self::Parse,
    ];

    /// The camelCase member name.
    #[must_use]
    pub const fn member(self) -> &'static str {
        match self {
            Self::AddChecked => "addChecked",
            Self::SubChecked => "subChecked",
            Self::MulChecked => "mulChecked",
            Self::DivChecked => "divChecked",
            Self::RemChecked => "remChecked",
            Self::NegChecked => "negChecked",
            Self::ShiftLeftChecked => "shiftLeftChecked",
            Self::ShiftRightChecked => "shiftRightChecked",
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Neg => "neg",
            Self::ShiftLeft => "shiftLeft",
            Self::ShiftRight => "shiftRight",
            Self::Equal => "equal",
            Self::Compare => "compare",
            Self::ToStr => "toStr",
            Self::Parse => "parse",
        }
    }

    /// Whether the member exists only for signed types.
    #[must_use]
    pub const fn signed_only(self) -> bool {
        matches!(self, Self::Neg | Self::NegChecked)
    }
}

/// A floating-point row's member.
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
    /// `equal`: IEEE equality.
    Equal,
    /// `compareTotal`: IEEE `totalOrder` over canonicalized values.
    CompareTotal,
    /// `toStr`: the canonical float text.
    ToStr,
    /// `parse`: the unsuffixed decimal float-literal grammar.
    Parse,
}

impl FloatOp {
    /// Every float member.
    pub const ALL: [Self; 9] = [
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

    /// The camelCase member name.
    #[must_use]
    pub const fn member(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
            Self::Neg => "neg",
            Self::Equal => "equal",
            Self::CompareTotal => "compareTotal",
            Self::ToStr => "toStr",
            Self::Parse => "parse",
        }
    }
}

/// One row of the registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompilerIntrinsic {
    /// An integer row.
    Integer(IntegerType, IntegerOp),
    /// A floating-point row.
    Float(FloatType, FloatOp),
    /// `S.toT`: an integer of the first type as the second, distinct, type.
    Convert(IntegerType, IntegerType),
    /// `char.toU32`.
    CharToU32,
    /// `char.fromU32`.
    CharFromU32,
    /// `str.concat`.
    StrConcat,
    /// `str.length`.
    StrLength,
    /// `str.equal`.
    StrEqual,

    /// `str.slice`.
    StrSlice,
    /// `str.toChars`.
    StrToChars,
    /// `str.fromChars`.
    StrFromChars,
    /// `str.toUtf8`.
    StrToUtf8,
    /// `str.fromUtf8`.
    StrFromUtf8,
    /// `bytes.length`.
    BytesLength,
    /// `bytes.concat`.
    BytesConcat,
    /// `bytes.equal`.
    BytesEqual,

    /// `bytes.slice`.
    BytesSlice,
    /// `bytes.toList`.
    BytesToList,
    /// `bytes.fromList`.
    BytesFromList,
    /// `dict.entries`.
    DictEntries,
    /// `dict.fromList`: the dict of a list of entries, in canonical key
    /// order, where a later duplicate key replaces the earlier value.
    DictFromList,
    /// `dict.next`: the first entry in key order and the remainder.
    DictNext,
    /// `list.length`.
    ListLength,
    /// `list.append`.
    ListAppend,
    /// `list.concat`.
    ListConcat,
    /// `list.slice`.
    ListSlice,
    /// `list.fold`.
    ListFold,
    /// `list.next`: the first element and the remainder.
    ListNext,
    /// `assert.true`.
    AssertTrue,
    /// `assert.false`.
    AssertFalse,
    /// `assert.equal`, expected then actual.
    AssertEqual,
    /// `grant.restrict`: a sealed grant narrowed by a policy.
    GrantRestrict,
    /// `grant.sub`: a sealed grant narrowed below a path prefix.
    GrantSub,
    /// `grant.limit`: a sealed grant narrowed by limits.
    GrantLimit,
    /// `fs.validNames`: whether every name of a list satisfies the name
    /// profile of `RelativePath` ([`crate::names::valid_names`]). Total.
    FsValidNames,
}

/// A declaration of the embedded library that plays no role and that the
/// grant rows name (`docs/spec/03-effects.md`, "Narrowing").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NarrowingType {
    /// `FsPolicy` of `@std.fs`.
    FsPolicy,
    /// `RelativePath` of `@std.fs`.
    RelativePath,
    /// `ScopeError` of `@std.policy`.
    ScopeError,
    /// `GrantLimits` of `@std.policy`.
    GrantLimits,
}

impl NarrowingType {
    /// The declaration's name, such as `FsPolicy`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::FsPolicy => "FsPolicy",
            Self::RelativePath => "RelativePath",
            Self::ScopeError => "ScopeError",
            Self::GrantLimits => "GrantLimits",
        }
    }

    /// The module path atom that declares it, such as `@std.fs`.
    #[must_use]
    pub const fn module(self) -> &'static str {
        match self {
            Self::FsPolicy | Self::RelativePath => "@std.fs",
            Self::ScopeError | Self::GrantLimits => "@std.policy",
        }
    }
}

/// The type language of registry signatures: compiler-owned types, the
/// row's generics, the library types, which a module set binds, and the
/// narrowing types.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RowType {
    /// A compiler-owned scalar type.
    Core(Type),
    /// The row's generic at this position.
    Generic(usize),
    /// `List(T)`.
    List(Box<Self>),
    /// A structural tuple.
    Tuple(Vec<Self>),
    /// `Fn({…} R)` without labels.
    Fn(Vec<Self>, Box<Self>),
    /// A library type applied to its arguments.
    Library(Library, Vec<Self>),
    /// A narrowing type, which takes no arguments.
    Narrowing(NarrowingType),
}

impl RowType {
    /// This type over `generics`, with the library types bound by `library`.
    ///
    /// # Errors
    ///
    /// A library type or a narrowing type it names is not bound, or a
    /// generic has no argument.
    pub fn instantiate(
        &self,
        library: &LibraryTypes,
        generics: &[Type],
    ) -> Result<Type, IrError> {
        let all = |values: &[Self]| {
            values
                .iter()
                .map(|value| value.instantiate(library, generics))
                .collect::<Result<Vec<_>, _>>()
        };
        Ok(match self {
            Self::Core(ty) => ty.clone(),
            Self::Generic(index) => generics.get(*index).cloned().ok_or_else(|| {
                IrError::InvalidExpression(format!(
                    "registry generic {index} has no argument"
                ))
            })?,
            Self::List(element) => Type::list(element.instantiate(library, generics)?),
            Self::Tuple(values) => Type::Tuple(all(values)?),
            Self::Fn(parameters, result) => Type::function(crate::FnType::new(
                all(parameters)?,
                result.instantiate(library, generics)?,
            )),
            Self::Library(declaration, arguments) => {
                library.declared(*declaration, all(arguments)?)?
            }
            Self::Narrowing(declaration) => {
                let id = library.narrowing(*declaration).ok_or_else(|| {
                    IrError::UnknownType(format!(
                        "{}.{}",
                        declaration.module(),
                        declaration.name()
                    ))
                })?;
                Type::Declared(id.clone(), Vec::new())
            }
        })
    }

    /// The source spelling of this type, with `generics` naming the
    /// generics.
    #[must_use]
    pub fn spelling(&self, generics: &[&str]) -> String {
        let list = |values: &[Self]| {
            values
                .iter()
                .map(|value| value.spelling(generics))
                .collect::<Vec<_>>()
                .join(" ")
        };
        match self {
            Self::Core(ty) => ty.to_string(),
            Self::Generic(index) => {
                generics.get(*index).copied().unwrap_or("?").to_owned()
            }
            Self::List(element) => format!("List({})", element.spelling(generics)),
            Self::Tuple(values) => format!("{{{}}}", list(values)),
            Self::Fn(parameters, result) => {
                format!("Fn({{{}}} {})", list(parameters), result.spelling(generics))
            }
            Self::Library(declaration, arguments) if arguments.is_empty() => {
                declaration.name().to_owned()
            }
            Self::Library(declaration, arguments) => {
                format!("{}({})", declaration.name(), list(arguments))
            }
            Self::Narrowing(declaration) => declaration.name().to_owned(),
        }
    }
}

/// Everything the registry says about one row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntrinsicRow {
    /// The wire symbol.
    pub symbol: String,
    /// The generics, in order.
    pub generics: &'static [&'static str],
    /// The bound of each bounded generic, by its position: the interface it
    /// must conform to.
    pub bounds: &'static [(usize, Library)],
    /// The parameter types, in order.
    pub parameters: Vec<RowType>,
    /// The result type.
    pub result: RowType,
    /// Whether the row is pure: every row but the assertion rows.
    pub pure: bool,
    /// Whether the row is total: pure, defined for every operand, and
    /// unable to fault.
    pub total: bool,
}

impl IntrinsicRow {
    /// The row's line in `registry/compiler-2.0.txt`: the symbol, the
    /// generics, each with its bound as `T: Encodable`, the parameters as an input vector, the result, and the
    /// marks (`@total` or nothing), separated by tabs.
    #[must_use]
    pub fn snapshot_line(&self) -> String {
        let parameters = self
            .parameters
            .iter()
            .map(|parameter| parameter.spelling(self.generics))
            .collect::<Vec<_>>()
            .join(" ");
        let mut line = String::new();
        let _ = write!(
            line,
            "{}\t{}\t{{{}}}\t{}\t{}",
            self.symbol,
            spell_generics(self.generics, self.bounds),
            parameters,
            self.result.spelling(self.generics),
            if self.total { "@total" } else { "" }
        );
        line
    }
}

/// The generics of a row in source spelling, each bounded one as
/// `T: Encodable`, separated by spaces.
#[must_use]
pub fn spell_generics(generics: &[&str], bounds: &[(usize, Library)]) -> String {
    generics
        .iter()
        .enumerate()
        .map(|(index, name)| {
            bounds.iter().find(|(at, _)| *at == index).map_or_else(
                || (*name).to_owned(),
                |(_, bound)| format!("{name}: {}", bound.name()),
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn core(ty: Type) -> RowType {
    RowType::Core(ty)
}

fn library(declaration: Library, arguments: Vec<RowType>) -> RowType {
    RowType::Library(declaration, arguments)
}

fn generic(index: usize) -> RowType {
    RowType::Generic(index)
}

fn list(element: RowType) -> RowType {
    RowType::List(Box::new(element))
}

fn checked(value: RowType) -> RowType {
    library(
        Library::Result,
        vec![value, library(Library::ArithmeticError, Vec::new())],
    )
}

fn converted(value: RowType) -> RowType {
    library(
        Library::Result,
        vec![value, library(Library::ConversionError, Vec::new())],
    )
}

fn option(value: RowType) -> RowType {
    library(Library::Option, vec![value])
}

fn named(declaration: Library) -> RowType {
    library(declaration, Vec::new())
}

const fn narrowing(declaration: NarrowingType) -> RowType {
    RowType::Narrowing(declaration)
}

/// `Result(value ScopeError)`.
fn scoped(value: RowType) -> RowType {
    library(
        Library::Result,
        vec![value, narrowing(NarrowingType::ScopeError)],
    )
}

fn camel_case_type(owner: &str) -> String {
    let mut characters = owner.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_uppercase().to_string() + characters.as_str()
    })
}

impl CompilerIntrinsic {
    /// Whether every value of `source` is a value of `target`, so the
    /// conversion cannot fail.
    #[must_use]
    pub const fn conversion_is_total(source: IntegerType, target: IntegerType) -> bool {
        let (source_low, source_high) = source.range();
        let (target_low, target_high) = target.range();
        target_low <= source_low && source_high <= target_high
    }

    /// Whether this value names a row of the registry: no negation of an
    /// unsigned type and no conversion of a type to itself.
    #[must_use]
    pub const fn is_row(self) -> bool {
        match self {
            Self::Integer(integer, op) => !op.signed_only() || integer.is_signed(),
            Self::Convert(source, target) => !matches!(
                (source, target),
                (IntegerType::I8, IntegerType::I8)
                    | (IntegerType::I16, IntegerType::I16)
                    | (IntegerType::I32, IntegerType::I32)
                    | (IntegerType::I64, IntegerType::I64)
                    | (IntegerType::U8, IntegerType::U8)
                    | (IntegerType::U16, IntegerType::U16)
                    | (IntegerType::U32, IntegerType::U32)
                    | (IntegerType::U64, IntegerType::U64)
            ),
            _ => true,
        }
    }

    /// The lower-case owner of the row.
    #[must_use]
    pub const fn owner(self) -> &'static str {
        match self {
            Self::Integer(integer, _) | Self::Convert(integer, _) => integer.owner(),
            Self::Float(float, _) => float.owner(),
            Self::CharToU32 | Self::CharFromU32 => "char",
            Self::StrConcat
            | Self::StrLength
            | Self::StrEqual
            | Self::StrSlice
            | Self::StrToChars
            | Self::StrFromChars
            | Self::StrToUtf8
            | Self::StrFromUtf8 => "str",
            Self::BytesLength
            | Self::BytesConcat
            | Self::BytesEqual
            | Self::BytesSlice
            | Self::BytesToList
            | Self::BytesFromList => "bytes",
            Self::DictEntries | Self::DictFromList | Self::DictNext => "dict",
            Self::ListLength
            | Self::ListAppend
            | Self::ListConcat
            | Self::ListSlice
            | Self::ListFold
            | Self::ListNext => "list",
            Self::AssertTrue | Self::AssertFalse | Self::AssertEqual => "assert",
            Self::GrantRestrict | Self::GrantSub | Self::GrantLimit => "grant",
            Self::FsValidNames => "fs",
        }
    }

    /// The camelCase member of the row.
    #[must_use]
    pub fn member(self) -> String {
        match self {
            Self::Integer(_, op) => op.member().to_owned(),
            Self::Float(_, op) => op.member().to_owned(),
            Self::Convert(_, target) => {
                format!("to{}", camel_case_type(target.owner()))
            }
            Self::CharToU32 => "toU32".to_owned(),
            Self::CharFromU32 => "fromU32".to_owned(),
            Self::StrConcat | Self::BytesConcat | Self::ListConcat => {
                "concat".to_owned()
            }
            Self::StrLength | Self::BytesLength | Self::ListLength => {
                "length".to_owned()
            }
            Self::StrEqual | Self::BytesEqual | Self::AssertEqual => "equal".to_owned(),
            Self::StrSlice | Self::BytesSlice | Self::ListSlice => "slice".to_owned(),
            Self::StrToChars => "toChars".to_owned(),
            Self::StrFromChars => "fromChars".to_owned(),
            Self::StrToUtf8 => "toUtf8".to_owned(),
            Self::StrFromUtf8 => "fromUtf8".to_owned(),
            Self::BytesToList => "toList".to_owned(),
            Self::BytesFromList | Self::DictFromList => "fromList".to_owned(),
            Self::DictEntries => "entries".to_owned(),
            Self::DictNext | Self::ListNext => "next".to_owned(),
            Self::ListAppend => "append".to_owned(),
            Self::ListFold => "fold".to_owned(),
            Self::AssertTrue => "true".to_owned(),
            Self::AssertFalse => "false".to_owned(),
            Self::GrantRestrict => "restrict".to_owned(),
            Self::GrantSub => "sub".to_owned(),
            Self::GrantLimit => "limit".to_owned(),
            Self::FsValidNames => "validNames".to_owned(),
        }
    }

    /// The wire symbol, `<owner>.<member>`, such as `i32.addChecked`.
    #[must_use]
    pub fn symbol(self) -> String {
        format!("{}.{}", self.owner(), self.member())
    }

    /// Whether the row is pure. Only the assertion rows are not.
    #[must_use]
    pub const fn is_pure(self) -> bool {
        !matches!(
            self,
            Self::AssertTrue | Self::AssertFalse | Self::AssertEqual
        )
    }

    /// Whether the row is total: pure, defined for every operand, and
    /// unable to fault. A call of a function bound to a total row is an
    /// atom of the certified fragment. Only `fs.validNames` is.
    #[must_use]
    pub const fn is_total(self) -> bool {
        matches!(self, Self::FsValidNames)
    }

    /// The generics of the row, in order.
    #[must_use]
    pub const fn generics(self) -> &'static [&'static str] {
        match self {
            Self::DictEntries | Self::DictFromList | Self::DictNext => &["K", "V"],
            Self::ListFold => &["T", "A"],
            Self::ListLength
            | Self::ListAppend
            | Self::ListConcat
            | Self::ListSlice
            | Self::ListNext
            | Self::AssertEqual => &["T"],
            Self::GrantRestrict | Self::GrantSub | Self::GrantLimit => &["G"],
            _ => &[],
        }
    }

    /// The bound of each bounded generic of the row, by its position. Only
    /// `assert.equal` has one: its `T` must be `Encodable`, so that the
    /// two operands have canonical encodings to compare.
    #[must_use]
    pub const fn bounds(self) -> &'static [(usize, Library)] {
        match self {
            Self::AssertEqual => &[(0, Library::Encodable)],
            _ => &[],
        }
    }

    /// The parameter types and the result type of the row.
    #[must_use]
    pub fn signature(self) -> (Vec<RowType>, RowType) {
        let str_type = || named(Library::Str);
        let bytes = || named(Library::Bytes);
        let boolean = || named(Library::Bool);
        let ordering = || named(Library::Ordering);
        let items = || list(generic(0));
        let dict = || library(Library::Dict, vec![generic(0), generic(1)]);
        match self {
            Self::Integer(integer, op) => {
                let value = || core(integer.to_type());
                let pair = || vec![value(), value()];
                let shift = || vec![value(), core(Type::U32)];
                match op {
                    IntegerOp::AddChecked
                    | IntegerOp::SubChecked
                    | IntegerOp::MulChecked
                    | IntegerOp::DivChecked
                    | IntegerOp::RemChecked => (pair(), checked(value())),
                    IntegerOp::NegChecked => (vec![value()], checked(value())),
                    IntegerOp::ShiftLeftChecked | IntegerOp::ShiftRightChecked => {
                        (shift(), checked(value()))
                    }
                    IntegerOp::Add | IntegerOp::Sub | IntegerOp::Mul => {
                        (pair(), value())
                    }
                    IntegerOp::Neg => (vec![value()], value()),
                    IntegerOp::ShiftLeft | IntegerOp::ShiftRight => (shift(), value()),
                    IntegerOp::Equal => (pair(), boolean()),
                    IntegerOp::Compare => (pair(), ordering()),
                    IntegerOp::ToStr => (vec![value()], str_type()),
                    IntegerOp::Parse => (vec![str_type()], converted(value())),
                }
            }
            Self::Float(float, op) => {
                let value = || core(float.to_type());
                let pair = || vec![value(), value()];
                match op {
                    FloatOp::Add | FloatOp::Sub | FloatOp::Mul | FloatOp::Div => {
                        (pair(), value())
                    }
                    FloatOp::Neg => (vec![value()], value()),
                    FloatOp::Equal => (pair(), boolean()),
                    FloatOp::CompareTotal => (pair(), ordering()),
                    FloatOp::ToStr => (vec![value()], str_type()),
                    FloatOp::Parse => (vec![str_type()], converted(value())),
                }
            }
            Self::Convert(source, target) => {
                let result = core(target.to_type());
                let result = if Self::conversion_is_total(source, target) {
                    result
                } else {
                    converted(result)
                };
                (vec![core(source.to_type())], result)
            }
            Self::CharToU32 => (vec![core(Type::Char)], core(Type::U32)),
            Self::CharFromU32 => (vec![core(Type::U32)], option(core(Type::Char))),
            Self::StrConcat => (vec![str_type(), str_type()], str_type()),
            Self::StrLength => (vec![str_type()], core(Type::U64)),
            Self::StrEqual => (vec![str_type(), str_type()], boolean()),
            Self::StrSlice => (
                vec![str_type(), core(Type::U64), core(Type::U64)],
                option(str_type()),
            ),
            Self::StrToChars => (vec![str_type()], list(core(Type::Char))),
            Self::StrFromChars => (vec![list(core(Type::Char))], str_type()),
            Self::StrToUtf8 => (vec![str_type()], bytes()),
            Self::StrFromUtf8 => (vec![bytes()], converted(str_type())),
            Self::BytesLength => (vec![bytes()], core(Type::U64)),
            Self::BytesConcat => (vec![bytes(), bytes()], bytes()),
            Self::BytesEqual => (vec![bytes(), bytes()], boolean()),
            Self::BytesSlice => (
                vec![bytes(), core(Type::U64), core(Type::U64)],
                option(bytes()),
            ),
            Self::BytesToList => (vec![bytes()], list(core(Type::U8))),
            Self::BytesFromList => (vec![list(core(Type::U8))], bytes()),
            Self::DictEntries => (
                vec![dict()],
                list(RowType::Tuple(vec![generic(0), generic(1)])),
            ),
            Self::DictFromList => (
                vec![list(RowType::Tuple(vec![generic(0), generic(1)]))],
                dict(),
            ),
            Self::DictNext => (
                vec![dict()],
                option(RowType::Tuple(vec![
                    RowType::Tuple(vec![generic(0), generic(1)]),
                    dict(),
                ])),
            ),
            Self::ListLength => (vec![items()], core(Type::U64)),
            Self::ListAppend => (vec![items(), generic(0)], items()),
            Self::ListConcat => (vec![items(), items()], items()),
            Self::ListSlice => (
                vec![items(), core(Type::U64), core(Type::U64)],
                option(items()),
            ),
            Self::ListFold => (
                vec![
                    items(),
                    generic(1),
                    RowType::Fn(vec![generic(1), generic(0)], Box::new(generic(1))),
                ],
                generic(1),
            ),
            Self::ListNext => (
                vec![items()],
                option(RowType::Tuple(vec![generic(0), items()])),
            ),
            Self::AssertTrue | Self::AssertFalse => (vec![boolean()], core(Type::Void)),
            Self::AssertEqual => (vec![generic(0), generic(0)], core(Type::Void)),
            Self::GrantRestrict => (
                vec![generic(0), narrowing(NarrowingType::FsPolicy)],
                scoped(generic(0)),
            ),
            Self::GrantSub => (
                vec![generic(0), narrowing(NarrowingType::RelativePath)],
                scoped(generic(0)),
            ),
            Self::GrantLimit => (
                vec![generic(0), narrowing(NarrowingType::GrantLimits)],
                generic(0),
            ),
            Self::FsValidNames => (vec![list(str_type())], boolean()),
        }
    }

    /// Everything the registry says about this row.
    #[must_use]
    pub fn row(self) -> IntrinsicRow {
        let (parameters, result) = self.signature();
        IntrinsicRow {
            symbol: self.symbol(),
            generics: self.generics(),
            bounds: self.bounds(),
            parameters,
            result,
            pure: self.is_pure(),
            total: self.is_total(),
        }
    }

    /// The sample vectors of the row, ported from the previous registry:
    /// operands and the outcome the specification gives them, computed from
    /// Rust's own integer, float, character, and string operations and
    /// independent of every executor. A row without vectors has none.
    #[must_use]
    pub fn vectors(self) -> &'static [Vector] {
        vectors::of(self)
    }

    /// The row whose wire symbol is `symbol`, if the registry has one.
    #[must_use]
    pub fn from_symbol(symbol: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|intrinsic| intrinsic.symbol() == symbol)
    }

    /// Every row, in the order of the UTF-8 bytes of its symbol.
    #[must_use]
    pub fn all() -> Vec<Self> {
        let mut all = Vec::new();
        for integer in IntegerType::ALL {
            for op in IntegerOp::ALL {
                all.push(Self::Integer(integer, op));
            }
            for target in IntegerType::ALL {
                all.push(Self::Convert(integer, target));
            }
        }
        for float in FloatType::ALL {
            for op in FloatOp::ALL {
                all.push(Self::Float(float, op));
            }
        }
        all.extend([
            Self::CharToU32,
            Self::CharFromU32,
            Self::StrConcat,
            Self::StrLength,
            Self::StrEqual,
            Self::StrSlice,
            Self::StrToChars,
            Self::StrFromChars,
            Self::StrToUtf8,
            Self::StrFromUtf8,
            Self::BytesLength,
            Self::BytesConcat,
            Self::BytesEqual,
            Self::BytesSlice,
            Self::BytesToList,
            Self::BytesFromList,
            Self::DictEntries,
            Self::DictFromList,
            Self::DictNext,
            Self::ListLength,
            Self::ListAppend,
            Self::ListConcat,
            Self::ListSlice,
            Self::ListFold,
            Self::ListNext,
            Self::AssertTrue,
            Self::AssertFalse,
            Self::AssertEqual,
            Self::GrantRestrict,
            Self::GrantSub,
            Self::GrantLimit,
            Self::FsValidNames,
        ]);
        all.retain(|intrinsic| intrinsic.is_row());
        all.sort_by_cached_key(|intrinsic| intrinsic.symbol());
        all
    }
}

/// Whether `symbol` is an assertion row: one of the three rows whose owner
/// is `assert`. It is the registry's one question about them.
#[must_use]
pub fn is_assertion(symbol: &str) -> bool {
    CompilerIntrinsic::from_symbol(symbol).is_some_and(|intrinsic| !intrinsic.is_pure())
}
