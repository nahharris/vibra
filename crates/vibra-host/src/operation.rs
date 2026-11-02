//! The value-access trait and the typed host operations.
//!
//! `docs/spec/06-runtime.md`, "WebAssembly boundary" and "The audit trace": each
//! host operation is written once in Rust, behind a value-access trait. The
//! reference interpreter implements [`HostValues`] over its own values, and the
//! Wasm runner implements it through the exported accessors and builders of the
//! module, never through the instance's linear memory. A [`HostCall`] is the
//! operation read from the arguments of one call, [`perform`] runs it on a
//! provider, and a [`HostReply`] is built back into the backend's value, so the
//! two backends cannot differ in what an operation does.
//!
//! Builders are bottom-up: a child is built, and so sealed, before the value
//! that holds it, which is the discipline of the seven builders of the Wasm
//! boundary.

use std::fmt;

use vibra_ir::external::host::{HostErrorType, HostRow, host_row};

use crate::provider::{
    EnvError, Failure, FsError, HostProvider, IoError, Metadata, RandomError, TimeError,
};

/// An enum type that a host operation builds, in the order of the registry's
/// declarations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostEnum {
    /// `result` of `@std.result`: `ok` carries the value, `err` the error.
    Result,
    /// `io-error`.
    IoError,
    /// `fs-error`.
    FsError,
    /// `fs-kind`.
    FsKind,
    /// `env-error`.
    EnvError,
    /// `time-error`.
    TimeError,
    /// `random-error`.
    RandomError,
}

/// A record type that a host operation builds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostRecord {
    /// `fs-metadata`: the fields `kind` and `size`, in that order.
    FsMetadata,
}

/// The variant index of `ok` and `err` in `result`.
const OK: u32 = 0;
const ERR: u32 = 1;

/// The value-access interface each backend implements.
///
/// A value is an owned handle in the backend's representation. Reads borrow the
/// handle; builders take sealed children and answer a sealed value.
pub trait HostValues {
    /// A backend value.
    type Value;

    /// The scalars of a `str` as text.
    fn read_text(&mut self, value: &Self::Value) -> Result<String, ReadError>;
    /// The bytes of a `bytes`.
    fn read_bytes(&mut self, value: &Self::Value) -> Result<Vec<u8>, ReadError>;
    /// The text of a `path`, the wrapper of `str` that `@std.fs` declares.
    fn read_path(&mut self, value: &Self::Value) -> Result<String, ReadError>;
    /// A `u64`.
    fn read_u64(&mut self, value: &Self::Value) -> Result<u64, ReadError>;

    /// The `void` value.
    fn build_void(&mut self) -> Self::Value;
    /// A `str`.
    fn build_text(&mut self, text: &str) -> Self::Value;
    /// A `bytes`.
    fn build_bytes(&mut self, bytes: &[u8]) -> Self::Value;
    /// An `i64`.
    fn build_i64(&mut self, value: i64) -> Self::Value;
    /// A `u64`.
    fn build_u64(&mut self, value: u64) -> Self::Value;
    /// An `array` of sealed elements.
    fn build_array(&mut self, elements: Vec<Self::Value>) -> Self::Value;
    /// A value of enum `ty` at `variant`, with the sealed payload of a non-`void`
    /// slot.
    fn build_enum(
        &mut self,
        ty: HostEnum,
        variant: u32,
        payload: Option<Self::Value>,
    ) -> Self::Value;
    /// A record of type `ty` with its sealed fields in declaration order.
    fn build_record(&mut self, ty: HostRecord, fields: Vec<Self::Value>)
    -> Self::Value;
}

/// An argument that could not be read as a host call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The symbol names no row of the registry.
    UnknownSymbol(String),
    /// The call has the wrong number of arguments for its row.
    Arity {
        /// The row's parameter count.
        expected: usize,
        /// The count supplied.
        found: usize,
    },
    /// The backend could not read an argument as the type of its parameter.
    Value,
}

impl fmt::Display for ReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSymbol(symbol) => {
                write!(formatter, "`{symbol}` is not a host registry symbol")
            }
            Self::Arity { expected, found } => {
                write!(formatter, "expected {expected} arguments, found {found}")
            }
            Self::Value => {
                formatter.write_str("an argument is not a value of its type")
            }
        }
    }
}

impl std::error::Error for ReadError {}

/// One host call, read from its arguments: a variant for each row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostCall {
    /// `io.stdin-text`.
    StdinText,
    /// `io.stdin-bytes`.
    StdinBytes,
    /// `io.stdout-text`.
    StdoutText(String),
    /// `io.stdout-bytes`.
    StdoutBytes(Vec<u8>),
    /// `io.stderr-text`.
    StderrText(String),
    /// `io.stderr-bytes`.
    StderrBytes(Vec<u8>),
    /// `fs.read-file`.
    ReadFile(String),
    /// `fs.read-text`.
    ReadText(String),
    /// `fs.read-entries`.
    ReadEntries(String),
    /// `fs.write-file`.
    WriteFile(String, Vec<u8>),
    /// `fs.write-text`.
    WriteText(String, String),
    /// `fs.metadata-stat`.
    Stat(String),
    /// `env.read-var`.
    EnvVar(String),
    /// `env.read-args`.
    EnvArgs,
    /// `time.now-wall`.
    NowWall,
    /// `time.now-monotonic`.
    NowMonotonic,
    /// `random.generate-bytes`.
    RandomBytes(u64),
}

impl HostCall {
    /// Reads the call of registry symbol `symbol` from its `arguments`.
    pub fn read<V: HostValues>(
        symbol: &str,
        arguments: &[V::Value],
        values: &mut V,
    ) -> Result<Self, ReadError> {
        let row = host_row(symbol)
            .ok_or_else(|| ReadError::UnknownSymbol(symbol.to_owned()))?;
        if arguments.len() != row.parameters.len() {
            return Err(ReadError::Arity {
                expected: row.parameters.len(),
                found: arguments.len(),
            });
        }
        let first = arguments.first();
        let second = arguments.get(1);
        // The arity is checked, so each argument a row needs is present.
        let (Some(first), second) = (first, second) else {
            return Self::without_arguments(symbol);
        };
        Ok(match (symbol, second) {
            ("io.stdout-text", _) => Self::StdoutText(values.read_text(first)?),
            ("io.stdout-bytes", _) => Self::StdoutBytes(values.read_bytes(first)?),
            ("io.stderr-text", _) => Self::StderrText(values.read_text(first)?),
            ("io.stderr-bytes", _) => Self::StderrBytes(values.read_bytes(first)?),
            ("fs.read-file", _) => Self::ReadFile(values.read_path(first)?),
            ("fs.read-text", _) => Self::ReadText(values.read_path(first)?),
            ("fs.read-entries", _) => Self::ReadEntries(values.read_path(first)?),
            ("fs.metadata-stat", _) => Self::Stat(values.read_path(first)?),
            ("env.read-var", _) => Self::EnvVar(values.read_text(first)?),
            ("random.generate-bytes", _) => Self::RandomBytes(values.read_u64(first)?),
            ("fs.write-file", Some(contents)) => {
                Self::WriteFile(values.read_path(first)?, values.read_bytes(contents)?)
            }
            ("fs.write-text", Some(contents)) => {
                Self::WriteText(values.read_path(first)?, values.read_text(contents)?)
            }
            _ => return Err(ReadError::UnknownSymbol(symbol.to_owned())),
        })
    }

    fn without_arguments(symbol: &str) -> Result<Self, ReadError> {
        match symbol {
            "io.stdin-text" => Ok(Self::StdinText),
            "io.stdin-bytes" => Ok(Self::StdinBytes),
            "env.read-args" => Ok(Self::EnvArgs),
            "time.now-wall" => Ok(Self::NowWall),
            "time.now-monotonic" => Ok(Self::NowMonotonic),
            _ => Err(ReadError::UnknownSymbol(symbol.to_owned())),
        }
    }

    /// The registry symbol of the call's row.
    #[must_use]
    pub const fn symbol(&self) -> &'static str {
        match self {
            Self::StdinText => "io.stdin-text",
            Self::StdinBytes => "io.stdin-bytes",
            Self::StdoutText(_) => "io.stdout-text",
            Self::StdoutBytes(_) => "io.stdout-bytes",
            Self::StderrText(_) => "io.stderr-text",
            Self::StderrBytes(_) => "io.stderr-bytes",
            Self::ReadFile(_) => "fs.read-file",
            Self::ReadText(_) => "fs.read-text",
            Self::ReadEntries(_) => "fs.read-entries",
            Self::WriteFile(..) => "fs.write-file",
            Self::WriteText(..) => "fs.write-text",
            Self::Stat(_) => "fs.metadata-stat",
            Self::EnvVar(_) => "env.read-var",
            Self::EnvArgs => "env.read-args",
            Self::NowWall => "time.now-wall",
            Self::NowMonotonic => "time.now-monotonic",
            Self::RandomBytes(_) => "random.generate-bytes",
        }
    }

    /// The registry row of the call.
    #[must_use]
    pub fn row(&self) -> Option<&'static HostRow> {
        host_row(self.symbol())
    }
}

/// What a completed host call answered when it succeeded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostOk {
    /// `void`.
    Void,
    /// A `str`.
    Text(String),
    /// A `bytes`.
    Bytes(Vec<u8>),
    /// An `(array str)`.
    Entries(Vec<String>),
    /// An `fs-metadata`.
    Metadata(Metadata),
    /// An `i64`.
    Integer(i64),
    /// A `u64`.
    Count(u64),
}

/// The closed error an operation answered, of the enum its row declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationError {
    /// An `io-error`.
    Io(IoError),
    /// An `fs-error`.
    Fs(FsError),
    /// An `env-error`.
    Env(EnvError),
    /// A `time-error`.
    Time(TimeError),
    /// A `random-error`.
    Random(RandomError),
}

impl OperationError {
    /// The enum the error is a variant of.
    #[must_use]
    pub const fn enum_type(self) -> HostEnum {
        match self {
            Self::Io(_) => HostEnum::IoError,
            Self::Fs(_) => HostEnum::FsError,
            Self::Env(_) => HostEnum::EnvError,
            Self::Time(_) => HostEnum::TimeError,
            Self::Random(_) => HostEnum::RandomError,
        }
    }

    /// The registry's description of the enum.
    #[must_use]
    pub const fn error_type(self) -> HostErrorType {
        match self {
            Self::Io(_) => HostErrorType::Io,
            Self::Fs(_) => HostErrorType::Fs,
            Self::Env(_) => HostErrorType::Env,
            Self::Time(_) => HostErrorType::Time,
            Self::Random(_) => HostErrorType::Random,
        }
    }

    /// The variant's atom spelling without `@`.
    #[must_use]
    pub const fn variant_name(self) -> &'static str {
        match self {
            Self::Io(error) => error.name(),
            Self::Fs(error) => error.name(),
            Self::Env(error) => error.name(),
            Self::Time(error) => error.name(),
            Self::Random(error) => error.name(),
        }
    }

    /// The variant's position in its enum.
    #[must_use]
    pub const fn variant_index(self) -> u32 {
        match self {
            Self::Io(error) => error.index(),
            Self::Fs(error) => error.index(),
            Self::Env(error) => error.index(),
            Self::Time(error) => error.index(),
            Self::Random(error) => error.index(),
        }
    }
}

/// The `(result t e)` a completed call answered.
pub type HostReply = Result<HostOk, OperationError>;

/// The host event of memory exhaustion: the call did not complete, and it reaches
/// no program and appends no audit event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryExhausted;

/// Runs `call` on `provider`.
pub fn perform(
    call: &HostCall,
    provider: &mut dyn HostProvider,
) -> Result<HostReply, MemoryExhausted> {
    fn settle<T, E>(
        answer: Result<T, Failure<E>>,
        ok: impl FnOnce(T) -> HostOk,
        err: impl FnOnce(E) -> OperationError,
    ) -> Result<HostReply, MemoryExhausted> {
        match answer {
            Ok(value) => Ok(Ok(ok(value))),
            Err(Failure::Error(error)) => Ok(Err(err(error))),
            Err(Failure::MemoryExhausted) => Err(MemoryExhausted),
        }
    }
    let void = |()| HostOk::Void;
    match call {
        HostCall::StdinText => {
            settle(provider.stdin_text(), HostOk::Text, OperationError::Io)
        }
        HostCall::StdinBytes => {
            settle(provider.stdin_bytes(), HostOk::Bytes, OperationError::Io)
        }
        HostCall::StdoutText(value) => {
            settle(provider.stdout_text(value), void, OperationError::Io)
        }
        HostCall::StdoutBytes(value) => {
            settle(provider.stdout_bytes(value), void, OperationError::Io)
        }
        HostCall::StderrText(value) => {
            settle(provider.stderr_text(value), void, OperationError::Io)
        }
        HostCall::StderrBytes(value) => {
            settle(provider.stderr_bytes(value), void, OperationError::Io)
        }
        HostCall::ReadFile(at) => {
            settle(provider.read_file(at), HostOk::Bytes, OperationError::Fs)
        }
        HostCall::ReadText(at) => {
            settle(provider.read_text(at), HostOk::Text, OperationError::Fs)
        }
        HostCall::ReadEntries(at) => settle(
            provider.read_entries(at),
            HostOk::Entries,
            OperationError::Fs,
        ),
        HostCall::WriteFile(at, contents) => {
            settle(provider.write_file(at, contents), void, OperationError::Fs)
        }
        HostCall::WriteText(at, contents) => {
            settle(provider.write_text(at, contents), void, OperationError::Fs)
        }
        HostCall::Stat(at) => {
            settle(provider.stat(at), HostOk::Metadata, OperationError::Fs)
        }
        HostCall::EnvVar(name) => {
            settle(provider.env_var(name), HostOk::Text, OperationError::Env)
        }
        HostCall::EnvArgs => {
            settle(provider.env_args(), HostOk::Entries, OperationError::Env)
        }
        HostCall::NowWall => {
            settle(provider.now_wall(), HostOk::Integer, OperationError::Time)
        }
        HostCall::NowMonotonic => settle(
            provider.now_monotonic(),
            HostOk::Count,
            OperationError::Time,
        ),
        HostCall::RandomBytes(count) => settle(
            provider.random_bytes(*count),
            HostOk::Bytes,
            OperationError::Random,
        ),
    }
}

/// Builds the `(result t e)` value of a completed call in the backend's own
/// representation, children before parents.
pub fn build_reply<V: HostValues>(reply: &HostReply, values: &mut V) -> V::Value {
    match reply {
        Ok(ok) => {
            let payload = match ok {
                HostOk::Void => None,
                HostOk::Text(text) => Some(values.build_text(text)),
                HostOk::Bytes(bytes) => Some(values.build_bytes(bytes)),
                HostOk::Entries(names) => {
                    let elements = names
                        .iter()
                        .map(|name| values.build_text(name))
                        .collect::<Vec<_>>();
                    Some(values.build_array(elements))
                }
                HostOk::Metadata(metadata) => {
                    let kind = values.build_enum(
                        HostEnum::FsKind,
                        metadata.kind.index(),
                        None,
                    );
                    let size = values.build_u64(metadata.size);
                    Some(values.build_record(HostRecord::FsMetadata, vec![kind, size]))
                }
                HostOk::Integer(value) => Some(values.build_i64(*value)),
                HostOk::Count(value) => Some(values.build_u64(*value)),
            };
            values.build_enum(HostEnum::Result, OK, payload)
        }
        Err(error) => {
            let payload =
                values.build_enum(error.enum_type(), error.variant_index(), None);
            values.build_enum(HostEnum::Result, ERR, Some(payload))
        }
    }
}
