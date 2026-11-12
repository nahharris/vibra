//! Admission of a [`VibraProgram`]: the whole descriptor is validated, and
//! copied into owned, safe data, before any step function is called.
//!
//! After [`admit`] returns, the runtime holds no pointer into the descriptor:
//! the tables are copied, and the only raw pointers that are read later are the
//! operand lists generated code passes to the routines.

use std::fmt;

use vibra_ir::external::host::{HostRow, host_row};
use vibra_nexus::{
    AdmitError, MAX_FRAME_SLOTS, Requirements, admit as admit_requirements,
};

use crate::abi::{
    CONSTANT_ATOM, CONSTANT_BYTES, CONSTANT_STR, VibraProgram, VibraStep, VibraTable,
};

/// The most functions, module values, constants, symbols, or origins a
/// descriptor may declare.
pub const MAX_TABLE_LEN: u64 = 1 << 24;
/// The most bytes one string or byte constant of a descriptor may have.
pub const MAX_DATA_LEN: u64 = 1 << 30;

/// What is wrong with a descriptor that is not a mismatch of versions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Malformed {
    /// A reserved field is not zero.
    Reserved,
    /// The function table is null or empty.
    NoFunctions,
    /// A table is longer than the runtime admits.
    TableTooLong(&'static str),
    /// A table with elements has a null address.
    NullTable(&'static str),
    /// A table address is not aligned for its elements.
    Misaligned(&'static str),
    /// A function has no step function.
    NullStep(u32),
    /// A function's frame is larger than [`MAX_FRAME_SLOTS`].
    FrameTooLarge(u32),
    /// A function's frame cannot hold its parameters and environment.
    FrameTooSmall(u32),
    /// A function's environment flag is neither `0` nor `1`.
    BadEnvFlag(u32),
    /// The entry is not a function of the program.
    EntryOutOfRange,
    /// The entry takes operands or an environment.
    EntryTakesOperands,
    /// A module value's initializer is not a function of the program.
    InitializerOutOfRange(u32),
    /// A module value's initializer takes operands or an environment.
    InitializerTakesOperands(u32),
    /// A constant has a kind the ABI does not define.
    BadConstantKind(u32),
    /// A string that must be UTF-8 is not.
    NotUtf8(&'static str),
    /// An origin ends before it starts.
    BadOrigin(u32),
}

impl fmt::Display for Malformed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reserved => write!(formatter, "a reserved field is not zero"),
            Self::NoFunctions => write!(formatter, "the program has no functions"),
            Self::TableTooLong(table) => {
                write!(formatter, "the {table} table is too long")
            }
            Self::NullTable(table) => write!(formatter, "the {table} table is null"),
            Self::Misaligned(table) => {
                write!(formatter, "the {table} table is misaligned")
            }
            Self::NullStep(function) => {
                write!(formatter, "function {function} has no step function")
            }
            Self::FrameTooLarge(function) => {
                write!(formatter, "the frame of function {function} is too large")
            }
            Self::FrameTooSmall(function) => write!(
                formatter,
                "the frame of function {function} cannot hold its operands"
            ),
            Self::BadEnvFlag(function) => {
                write!(formatter, "function {function} has a bad environment flag")
            }
            Self::EntryOutOfRange => write!(formatter, "the entry is not a function"),
            Self::EntryTakesOperands => {
                write!(formatter, "the entry takes operands or an environment")
            }
            Self::InitializerOutOfRange(global) => write!(
                formatter,
                "the initializer of module value {global} is not a function"
            ),
            Self::InitializerTakesOperands(global) => write!(
                formatter,
                "the initializer of module value {global} takes operands"
            ),
            Self::BadConstantKind(constant) => {
                write!(formatter, "constant {constant} has an undefined kind")
            }
            Self::NotUtf8(what) => write!(formatter, "{what} is not UTF-8"),
            Self::BadOrigin(origin) => {
                write!(formatter, "origin {origin} ends before it starts")
            }
        }
    }
}

/// Why the runtime refuses a program: it reports
/// `@runtime.incompatible-artifact` and runs nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A version, registry, or symbol requirement the runtime does not meet.
    Requirement(AdmitError),
    /// A descriptor that is not well formed.
    Malformed(Malformed),
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Requirement(AdmitError::Abi { runtime, program }) => write!(
                formatter,
                "the program needs runtime ABI {program}, this runtime implements {runtime}"
            ),
            Self::Requirement(AdmitError::RegistryMinor { runtime, program }) => {
                write!(
                    formatter,
                    "the program needs registry minor {program}, this runtime implements {runtime}"
                )
            }
            Self::Requirement(AdmitError::UnknownSymbol(symbol)) => {
                write!(
                    formatter,
                    "the program names the unknown host symbol `{symbol}`"
                )
            }
            Self::Malformed(malformed) => malformed.fmt(formatter),
        }
    }
}

impl From<Malformed> for Refusal {
    fn from(malformed: Malformed) -> Self {
        Self::Malformed(malformed)
    }
}

/// A function of an admitted program.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FunctionInfo {
    pub(crate) step: VibraStep,
    pub(crate) slots: u32,
    pub(crate) params: u32,
    pub(crate) env: bool,
}

/// A constant of an admitted program.
#[derive(Clone, Debug)]
pub(crate) enum ConstantData {
    Str(String),
    Atom(String),
    Bytes(Vec<u8>),
}

/// A source span of an admitted program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    /// The source's identity.
    pub source: String,
    /// The first byte of the span.
    pub start: u32,
    /// One past the last byte.
    pub end: u32,
}

/// A program that passed admission, as safe owned data.
#[derive(Debug)]
pub struct Admitted {
    pub(crate) functions: Vec<FunctionInfo>,
    pub(crate) entry: u32,
    pub(crate) globals: Vec<u32>,
    pub(crate) constants: Vec<ConstantData>,
    pub(crate) rows: Vec<&'static HostRow>,
    pub(crate) origins: Vec<Origin>,
}

impl Admitted {
    /// The origin of an ordinal, `None` for `0` and for an ordinal past the
    /// table.
    #[must_use]
    pub fn origin(&self, ordinal: u32) -> Option<&Origin> {
        let index = usize::try_from(ordinal).ok()?.checked_sub(1)?;
        self.origins.get(index)
    }

    /// The number of module values of the program.
    #[must_use]
    pub fn globals(&self) -> usize {
        self.globals.len()
    }
}

/// Reads a table as a slice.
///
/// # Safety
///
/// A table with `len > 0` must address `len` initialized elements that are
/// valid for reads for `'a` and are not mutated during it.
unsafe fn read<'a, T>(
    table: &VibraTable<T>,
    name: &'static str,
    limit: u64,
) -> Result<&'a [T], Malformed> {
    if table.len > limit {
        return Err(Malformed::TableTooLong(name));
    }
    let Ok(len) = usize::try_from(table.len) else {
        return Err(Malformed::TableTooLong(name));
    };
    if len == 0 {
        return Ok(&[]);
    }
    if table.ptr.is_null() {
        return Err(Malformed::NullTable(name));
    }
    if !table.ptr.is_aligned() {
        return Err(Malformed::Misaligned(name));
    }
    // SAFETY: the pointer is non-null and aligned, and the length is at most
    // `limit`, which keeps the byte size below `isize::MAX`. That the memory
    // is initialized, readable, and unchanging for `'a` is the caller's
    // contract: the code generator emitted the table as read-only data.
    Ok(unsafe { std::slice::from_raw_parts(table.ptr, len) })
}

/// Reads a table of bytes as UTF-8 text.
///
/// # Safety
///
/// As [`read`].
unsafe fn read_text<'a>(
    table: &VibraTable<u8>,
    name: &'static str,
) -> Result<&'a str, Malformed> {
    // SAFETY: forwarded; the caller's contract is that of `read`.
    let bytes = unsafe { read(table, name, MAX_DATA_LEN) }?;
    std::str::from_utf8(bytes).map_err(|_| Malformed::NotUtf8(name))
}

/// Validates the whole descriptor and copies it.
///
/// The version is checked first, and nothing else is read from a program of
/// another version: its layout may differ.
///
/// # Errors
///
/// The first thing the runtime does not admit.
///
/// # Safety
///
/// Every table the descriptor addresses (and every string those tables
/// address) must be initialized memory that is valid for reads and is not
/// mutated for the duration of the call. The code generator guarantees it by
/// emitting the descriptor and its tables as read-only data of the program.
/// The step functions are not called here, and are not required to be sound
/// until they are.
pub unsafe fn admit(program: &VibraProgram) -> Result<Admitted, Refusal> {
    admit_requirements(&Requirements {
        abi: program.abi,
        registry_minor: 0,
        host_symbols: Vec::new(),
    })
    .map_err(Refusal::Requirement)?;
    if program.reserved != 0 {
        return Err(Malformed::Reserved.into());
    }
    // SAFETY: the caller's contract covers every table of the descriptor.
    let functions = unsafe { read(&program.functions, "function", MAX_TABLE_LEN) }?;
    if functions.is_empty() {
        return Err(Malformed::NoFunctions.into());
    }
    let mut infos = Vec::with_capacity(functions.len());
    for (index, function) in (0_u32..).zip(functions) {
        let Some(step) = function.step else {
            return Err(Malformed::NullStep(index).into());
        };
        if function.env > 1 {
            return Err(Malformed::BadEnvFlag(index).into());
        }
        if function.reserved != 0 {
            return Err(Malformed::Reserved.into());
        }
        if function.slots > MAX_FRAME_SLOTS {
            return Err(Malformed::FrameTooLarge(index).into());
        }
        if u64::from(function.params) + u64::from(function.env)
            > u64::from(function.slots)
        {
            return Err(Malformed::FrameTooSmall(index).into());
        }
        infos.push(FunctionInfo {
            step,
            slots: function.slots,
            params: function.params,
            env: function.env == 1,
        });
    }
    let takes_nothing = |function: u32| {
        infos
            .get(usize::try_from(function).unwrap_or(usize::MAX))
            .map(|info| info.params == 0 && !info.env)
    };
    match takes_nothing(program.entry) {
        None => return Err(Malformed::EntryOutOfRange.into()),
        Some(false) => return Err(Malformed::EntryTakesOperands.into()),
        Some(true) => {}
    }

    // SAFETY: the caller's contract covers every table of the descriptor.
    let globals = unsafe { read(&program.globals, "module value", MAX_TABLE_LEN) }?;
    for (global, initializer) in (0_u32..).zip(globals) {
        match takes_nothing(*initializer) {
            None => return Err(Malformed::InitializerOutOfRange(global).into()),
            Some(false) => {
                return Err(Malformed::InitializerTakesOperands(global).into());
            }
            Some(true) => {}
        }
    }

    // SAFETY: the caller's contract covers every table of the descriptor.
    let constants = unsafe { read(&program.constants, "constant", MAX_TABLE_LEN) }?;
    let mut data = Vec::with_capacity(constants.len());
    for (index, constant) in (0_u32..).zip(constants) {
        if constant.reserved != 0 {
            return Err(Malformed::Reserved.into());
        }
        data.push(match constant.kind {
            CONSTANT_STR => {
                // SAFETY: the caller's contract covers every string.
                let text = unsafe { read_text(&constant.data, "a str constant") }?;
                ConstantData::Str(text.to_owned())
            }
            CONSTANT_ATOM => {
                // SAFETY: the caller's contract covers every string.
                let text = unsafe { read_text(&constant.data, "an atom constant") }?;
                ConstantData::Atom(text.to_owned())
            }
            CONSTANT_BYTES => {
                // SAFETY: the caller's contract covers every byte constant.
                let bytes =
                    unsafe { read(&constant.data, "a bytes constant", MAX_DATA_LEN) }?;
                ConstantData::Bytes(bytes.to_vec())
            }
            _ => return Err(Malformed::BadConstantKind(index).into()),
        });
    }

    // SAFETY: the caller's contract covers every table of the descriptor.
    let symbols = unsafe { read(&program.symbols, "symbol", MAX_TABLE_LEN) }?;
    let mut names = Vec::with_capacity(symbols.len());
    for symbol in symbols {
        // SAFETY: the caller's contract covers every string.
        names.push(unsafe { read_text(symbol, "a host symbol") }?);
    }
    admit_requirements(&Requirements {
        abi: program.abi,
        registry_minor: program.registry_minor,
        host_symbols: names.clone(),
    })
    .map_err(Refusal::Requirement)?;
    let rows = names
        .iter()
        .map(|name| host_row(name))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            Refusal::Requirement(AdmitError::UnknownSymbol("(unresolved)".to_owned()))
        })?;

    // SAFETY: the caller's contract covers every table of the descriptor.
    let origins = unsafe { read(&program.origins, "origin", MAX_TABLE_LEN) }?;
    let mut table = Vec::with_capacity(origins.len());
    for (index, origin) in (0_u32..).zip(origins) {
        if origin.end < origin.start {
            return Err(Malformed::BadOrigin(index + 1).into());
        }
        // SAFETY: the caller's contract covers every string.
        let source = unsafe { read_text(&origin.source, "an origin source") }?;
        table.push(Origin {
            source: source.to_owned(),
            start: origin.start,
            end: origin.end,
        });
    }

    Ok(Admitted {
        functions: infos,
        entry: program.entry,
        globals: globals.to_vec(),
        constants: data,
        rows,
        origins: table,
    })
}
