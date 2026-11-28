//! The closed `vibra_v1` host registry (`docs/spec/06-runtime.md`, "The host
//! registry" and "Registry versioning").
//!
//! The registry has one row for each host operation: the registry **symbol**,
//! the **owner** root, the exact **signature**, and `since`, the registry minor
//! that added it. The table is data: the emitter reads the symbols and
//! signatures, the checker binds `external: @host` declarations against the
//! rows, and the providers and the audit event of `vibra-host-v1` follow the same
//! rows. A row's audit-event shape is determined by the row.
//!
//! The registry is versioned `1.N`. Adding a row is the minor `1.N+1`; changing
//! an existing row's symbol, signature, owner, or event shape, an error enum
//! included, requires `vibra_v2`.

/// The registry minor that Milestone 4 ships: the greatest `since` of any row.
pub const HOST_REGISTRY_MINOR: u32 = 0;

/// A type that appears in a host signature, spelled as the standard library
/// spells it inside the module that declares the operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostType {
    /// `void`.
    Void,
    /// `str`.
    Str,
    /// `bytes`.
    Bytes,
    /// `i64`.
    I64,
    /// `u64`.
    U64,
    /// `path`, the wrapper of `str` that `@std.fs` declares.
    Path,
    /// `fs-metadata`, the record `@std.fs` declares.
    FsMetadata,
    /// `(array T)`.
    Array(&'static HostType),
}

impl HostType {
    /// The type as the standard library writes it in the declaring module.
    #[must_use]
    pub fn spelling(self) -> String {
        match self {
            Self::Void => "void".to_owned(),
            Self::Str => "str".to_owned(),
            Self::Bytes => "bytes".to_owned(),
            Self::I64 => "i64".to_owned(),
            Self::U64 => "u64".to_owned(),
            Self::Path => "path".to_owned(),
            Self::FsMetadata => "fs-metadata".to_owned(),
            Self::Array(element) => format!("(array {})", element.spelling()),
        }
    }
}

/// The closed error enum of a host operation. Every variant carries `void`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HostErrorType {
    /// `io-error` of `@std.io`.
    Io,
    /// `fs-error` of `@std.fs`.
    Fs,
    /// `env-error` of `@std.env`.
    Env,
    /// `time-error` of `@std.time`.
    Time,
    /// `random-error` of `@std.random`.
    Random,
}

impl HostErrorType {
    /// The enum's spelling in its module, such as `io-error`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Io => "io-error",
            Self::Fs => "fs-error",
            Self::Env => "env-error",
            Self::Time => "time-error",
            Self::Random => "random-error",
        }
    }

    /// The standard-library module that declares the enum, such as `std.io`.
    #[must_use]
    pub const fn module(self) -> &'static str {
        match self {
            Self::Io => "std.io",
            Self::Fs => "std.fs",
            Self::Env => "std.env",
            Self::Time => "std.time",
            Self::Random => "std.random",
        }
    }

    /// The variants in declaration order.
    #[must_use]
    pub const fn variants(self) -> &'static [&'static str] {
        match self {
            Self::Io => &["invalid-text", "closed", "other"],
            Self::Fs => &[
                "not-found",
                "permission-denied",
                "not-a-file",
                "not-a-directory",
                "invalid-path",
                "invalid-text",
                "other",
            ],
            Self::Env => &["not-present", "not-unicode"],
            Self::Time | Self::Random => &["unavailable"],
        }
    }
}

/// The variants of `fs-kind` in declaration order.
pub const FS_KIND_VARIANTS: [&str; 3] = ["file", "directory", "other"];

/// One parameter of a host operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostParameter {
    /// The parameter name the standard library gives it.
    pub name: &'static str,
    /// The parameter type.
    pub ty: HostType,
}

/// One row of the host registry: `(symbol, owner, signature, since)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostRow {
    /// The registry symbol, the entry's identity.
    pub symbol: &'static str,
    /// The operation's source name, such as `io.stdin.text`.
    pub operation: &'static str,
    /// The owner root, `@std.<module>.<root>`.
    pub owner: &'static str,
    /// The parameters in order.
    pub parameters: &'static [HostParameter],
    /// The type of the `ok` payload of the result.
    pub ok: HostType,
    /// The closed error enum of the `err` payload.
    pub error: HostErrorType,
    /// The registry minor that added the row.
    pub since: u32,
}

impl HostRow {
    /// The exact signature as the runtime chapter writes it, such as
    /// `(value str) -> (result void io-error)`.
    #[must_use]
    pub fn signature(&self) -> String {
        let parameters = self
            .parameters
            .iter()
            .map(|parameter| format!("{} {}", parameter.name, parameter.ty.spelling()))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "({parameters}) -> (result {} {})",
            self.ok.spelling(),
            self.error.name()
        )
    }
}

const fn param(name: &'static str, ty: HostType) -> HostParameter {
    HostParameter { name, ty }
}

const VALUE_STR: &[HostParameter] = &[param("value", HostType::Str)];
const VALUE_BYTES: &[HostParameter] = &[param("value", HostType::Bytes)];
const AT: &[HostParameter] = &[param("at", HostType::Path)];
const AT_CONTENTS_BYTES: &[HostParameter] = &[
    param("at", HostType::Path),
    param("contents", HostType::Bytes),
];
const AT_CONTENTS_STR: &[HostParameter] = &[
    param("at", HostType::Path),
    param("contents", HostType::Str),
];
const NAME: &[HostParameter] = &[param("name", HostType::Str)];
const COUNT: &[HostParameter] = &[param("count", HostType::U64)];
const NONE: &[HostParameter] = &[];

const fn row(
    symbol: &'static str,
    operation: &'static str,
    owner: &'static str,
    parameters: &'static [HostParameter],
    ok: HostType,
    error: HostErrorType,
) -> HostRow {
    HostRow {
        symbol,
        operation,
        owner,
        parameters,
        ok,
        error,
        since: 0,
    }
}

/// The 17 rows of registry `1.0`, in the order of the runtime chapter.
pub const HOST_ROWS: [HostRow; 17] = [
    row(
        "io.stdin-text",
        "io.stdin.text",
        "@std.io.stdin",
        NONE,
        HostType::Str,
        HostErrorType::Io,
    ),
    row(
        "io.stdin-bytes",
        "io.stdin.bytes",
        "@std.io.stdin",
        NONE,
        HostType::Bytes,
        HostErrorType::Io,
    ),
    row(
        "io.stdout-text",
        "io.stdout.text",
        "@std.io.stdout",
        VALUE_STR,
        HostType::Void,
        HostErrorType::Io,
    ),
    row(
        "io.stdout-bytes",
        "io.stdout.bytes",
        "@std.io.stdout",
        VALUE_BYTES,
        HostType::Void,
        HostErrorType::Io,
    ),
    row(
        "io.stderr-text",
        "io.stderr.text",
        "@std.io.stderr",
        VALUE_STR,
        HostType::Void,
        HostErrorType::Io,
    ),
    row(
        "io.stderr-bytes",
        "io.stderr.bytes",
        "@std.io.stderr",
        VALUE_BYTES,
        HostType::Void,
        HostErrorType::Io,
    ),
    row(
        "fs.read-file",
        "fs.read.file",
        "@std.fs.read",
        AT,
        HostType::Bytes,
        HostErrorType::Fs,
    ),
    row(
        "fs.read-text",
        "fs.read.text",
        "@std.fs.read",
        AT,
        HostType::Str,
        HostErrorType::Fs,
    ),
    row(
        "fs.read-entries",
        "fs.read.entries",
        "@std.fs.read",
        AT,
        HostType::Array(&HostType::Str),
        HostErrorType::Fs,
    ),
    row(
        "fs.write-file",
        "fs.write.file",
        "@std.fs.write",
        AT_CONTENTS_BYTES,
        HostType::Void,
        HostErrorType::Fs,
    ),
    row(
        "fs.write-text",
        "fs.write.text",
        "@std.fs.write",
        AT_CONTENTS_STR,
        HostType::Void,
        HostErrorType::Fs,
    ),
    row(
        "fs.metadata-stat",
        "fs.metadata.stat",
        "@std.fs.metadata",
        AT,
        HostType::FsMetadata,
        HostErrorType::Fs,
    ),
    row(
        "env.read-var",
        "env.read.var",
        "@std.env.read",
        NAME,
        HostType::Str,
        HostErrorType::Env,
    ),
    row(
        "env.read-args",
        "env.read.args",
        "@std.env.read",
        NONE,
        HostType::Array(&HostType::Str),
        HostErrorType::Env,
    ),
    row(
        "time.now-wall",
        "time.now.wall",
        "@std.time.now",
        NONE,
        HostType::I64,
        HostErrorType::Time,
    ),
    row(
        "time.now-monotonic",
        "time.now.monotonic",
        "@std.time.now",
        NONE,
        HostType::U64,
        HostErrorType::Time,
    ),
    row(
        "random.generate-bytes",
        "random.generate.bytes",
        "@std.random.generate",
        COUNT,
        HostType::Bytes,
        HostErrorType::Random,
    ),
];

/// The row with registry symbol `symbol`.
#[must_use]
pub fn host_row(symbol: &str) -> Option<&'static HostRow> {
    HOST_ROWS.iter().find(|row| row.symbol == symbol)
}

/// The derived registry minor of a set of symbols: the greatest `since` among
/// them, and `0` when the set is empty. A symbol outside the registry has no
/// minor, so the answer is `None`.
#[must_use]
pub fn derived_registry_minor<'a>(
    symbols: impl IntoIterator<Item = &'a str>,
) -> Option<u32> {
    let mut minor = 0;
    for symbol in symbols {
        minor = minor.max(host_row(symbol)?.since);
    }
    Some(minor)
}
