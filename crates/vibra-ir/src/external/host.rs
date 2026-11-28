//! The closed host registry of major version 2 and the closed test-world
//! registry (`docs/spec/06-runtime.md`, "Host registry", "Error types", and
//! "Registry versioning"; `docs/spec/03-effects.md`, "Standard operation
//! inventory" and "Delivery and typed outcomes").
//!
//! The host registry has one row for each of the 17 standard operations: the
//! wire **symbol**, the [`HostOp`] it names, its **owner**, the
//! [`HostInterface`] whose one host-bound member it is, the parameters after
//! the grant, the `Ok` type and the error type of its result, `since`, the
//! registry minor that added it, and its [`Delivery`] class. The first
//! parameter of every operation is the grant, of the owner's type.
//!
//! The six members of `TestWorld` form a second closed table,
//! [`TEST_WORLD_ROWS`], owned by `TestWorld` in `@std.test`. [`host_row`]
//! never returns one of its rows, and [`test_world_row`] never returns a host
//! row. A test-world row is a row like any other: it has a `since` and a
//! delivery class, and it counts toward the registry minor. Every row of both
//! tables has `since` 0 and delivery `@once`.
//!
//! Both tables are frozen in `registry/host-2.0.txt` beside this crate's
//! manifest; the crate's tests write the snapshot from the tables and hold
//! the versioning rule against it. The snapshot is one line per entry, its
//! fields separated by tabs:
//!
//! - `registry host 2.N`, the major and the minor, first;
//! - `row`, the symbol, the owner's path, the exact signature, `since`, and
//!   the delivery atom, for each host row in the chapter's order;
//! - `testWorld`, the same fields, for each test-world row in the chapter's
//!   order;
//! - `type`, the name, the declaring module, and the cases or the fields, for
//!   each error type, `FsKind`, and `FsMetadata`.
//!
//! A signature is the type of the bound member in source spelling, with the
//! owner's type first: `Proc({StdinText U64} Result(Str IoError)
//! {StdinText})`.

/// The major version of the host registry.
pub const HOST_REGISTRY_MAJOR: u32 = 2;

/// The minor version of the host registry: the greatest `since` of any row
/// of either table.
pub const HOST_REGISTRY_MINOR: u32 = 0;

/// The delivery class of a host-bound operation (`docs/spec/03-effects.md`,
/// "Delivery and typed outcomes").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Delivery {
    /// `@once`: the runtime never dispatches one invocation twice.
    Once,
    /// `@repeatable`: the runtime may retry, only under the duplicate-safety
    /// contract the operation states.
    Repeatable,
    /// `@unknown`: no promise; the runtime does not retry after a possible
    /// submission.
    Unknown,
}

impl Delivery {
    /// The class's atom, such as `@once`.
    #[must_use]
    pub const fn atom(self) -> &'static str {
        match self {
            Self::Once => "@once",
            Self::Repeatable => "@repeatable",
            Self::Unknown => "@unknown",
        }
    }
}

/// The owner of a registry row: a declaration of the embedded library, by
/// its module path atom and its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Owner {
    /// The module path atom, such as `@std.io`.
    pub module: &'static str,
    /// The declaration's name, such as `StdinText`.
    pub name: &'static str,
}

impl Owner {
    /// The owner's path atom, such as `@std.io.StdinText`.
    #[must_use]
    pub fn path(self) -> String {
        format!("{}.{}", self.module, self.name)
    }
}

/// One of the 17 standard operation interfaces, each the owner of one host
/// row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HostInterface {
    /// `StdinText` of `@std.io`.
    StdinText,
    /// `StdinBytes` of `@std.io`.
    StdinBytes,
    /// `StdoutText` of `@std.io`.
    StdoutText,
    /// `StdoutBytes` of `@std.io`.
    StdoutBytes,
    /// `StderrText` of `@std.io`.
    StderrText,
    /// `StderrBytes` of `@std.io`.
    StderrBytes,
    /// `DirReadBytes` of `@std.fs`.
    DirReadBytes,
    /// `DirReadText` of `@std.fs`.
    DirReadText,
    /// `DirList` of `@std.fs`.
    DirList,
    /// `DirWriteBytes` of `@std.fs`.
    DirWriteBytes,
    /// `DirWriteText` of `@std.fs`.
    DirWriteText,
    /// `DirStat` of `@std.fs`.
    DirStat,
    /// `EnvReadVar` of `@std.env`.
    EnvReadVar,
    /// `EnvReadArgs` of `@std.env`.
    EnvReadArgs,
    /// `WallClock` of `@std.time`.
    WallClock,
    /// `MonotonicClock` of `@std.time`.
    MonotonicClock,
    /// `RandomBytes` of `@std.random`.
    RandomBytes,
}

impl HostInterface {
    /// Every interface, in the order of the chapter's table.
    pub const ALL: [Self; 17] = [
        Self::StdinText,
        Self::StdinBytes,
        Self::StdoutText,
        Self::StdoutBytes,
        Self::StderrText,
        Self::StderrBytes,
        Self::DirReadBytes,
        Self::DirReadText,
        Self::DirList,
        Self::DirWriteBytes,
        Self::DirWriteText,
        Self::DirStat,
        Self::EnvReadVar,
        Self::EnvReadArgs,
        Self::WallClock,
        Self::MonotonicClock,
        Self::RandomBytes,
    ];

    /// The interface's name, such as `StdinText`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::StdinText => "StdinText",
            Self::StdinBytes => "StdinBytes",
            Self::StdoutText => "StdoutText",
            Self::StdoutBytes => "StdoutBytes",
            Self::StderrText => "StderrText",
            Self::StderrBytes => "StderrBytes",
            Self::DirReadBytes => "DirReadBytes",
            Self::DirReadText => "DirReadText",
            Self::DirList => "DirList",
            Self::DirWriteBytes => "DirWriteBytes",
            Self::DirWriteText => "DirWriteText",
            Self::DirStat => "DirStat",
            Self::EnvReadVar => "EnvReadVar",
            Self::EnvReadArgs => "EnvReadArgs",
            Self::WallClock => "WallClock",
            Self::MonotonicClock => "MonotonicClock",
            Self::RandomBytes => "RandomBytes",
        }
    }

    /// The module path atom that declares the interface, such as `@std.io`.
    #[must_use]
    pub const fn module(self) -> &'static str {
        match self {
            Self::StdinText
            | Self::StdinBytes
            | Self::StdoutText
            | Self::StdoutBytes
            | Self::StderrText
            | Self::StderrBytes => "@std.io",
            Self::DirReadBytes
            | Self::DirReadText
            | Self::DirList
            | Self::DirWriteBytes
            | Self::DirWriteText
            | Self::DirStat => "@std.fs",
            Self::EnvReadVar | Self::EnvReadArgs => "@std.env",
            Self::WallClock | Self::MonotonicClock => "@std.time",
            Self::RandomBytes => "@std.random",
        }
    }

    /// The interface as the owner of its row.
    #[must_use]
    pub const fn owner(self) -> Owner {
        Owner {
            module: self.module(),
            name: self.name(),
        }
    }

    /// The interface whose owner has the path `path`, such as
    /// `@std.io.StdinText`.
    #[must_use]
    pub fn by_owner_path(path: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|interface| interface.owner().path() == path)
    }

    /// The name of the interface's host-bound member, such as `read`.
    #[must_use]
    pub const fn member(self) -> &'static str {
        match self {
            Self::StdinText
            | Self::StdinBytes
            | Self::DirReadBytes
            | Self::DirReadText
            | Self::EnvReadVar
            | Self::EnvReadArgs
            | Self::WallClock
            | Self::MonotonicClock => "read",
            Self::StdoutText
            | Self::StdoutBytes
            | Self::StderrText
            | Self::StderrBytes
            | Self::DirWriteBytes
            | Self::DirWriteText => "write",
            Self::DirList => "entries",
            Self::DirStat => "stat",
            Self::RandomBytes => "generate",
        }
    }

    /// The one operation the interface owns.
    #[must_use]
    pub const fn operation(self) -> HostOp {
        match self {
            Self::StdinText => HostOp::IoStdinText,
            Self::StdinBytes => HostOp::IoStdinBytes,
            Self::StdoutText => HostOp::IoStdoutText,
            Self::StdoutBytes => HostOp::IoStdoutBytes,
            Self::StderrText => HostOp::IoStderrText,
            Self::StderrBytes => HostOp::IoStderrBytes,
            Self::DirReadBytes => HostOp::FsReadFile,
            Self::DirReadText => HostOp::FsReadText,
            Self::DirList => HostOp::FsReadEntries,
            Self::DirWriteBytes => HostOp::FsWriteFile,
            Self::DirWriteText => HostOp::FsWriteText,
            Self::DirStat => HostOp::FsMetadataStat,
            Self::EnvReadVar => HostOp::EnvReadVar,
            Self::EnvReadArgs => HostOp::EnvReadArgs,
            Self::WallClock => HostOp::TimeNowWall,
            Self::MonotonicClock => HostOp::TimeNowMonotonic,
            Self::RandomBytes => HostOp::RandomGenerateBytes,
        }
    }
}

/// One of the 17 operations of the host registry, named after its wire
/// symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HostOp {
    /// `io.stdinText`.
    IoStdinText,
    /// `io.stdinBytes`.
    IoStdinBytes,
    /// `io.stdoutText`.
    IoStdoutText,
    /// `io.stdoutBytes`.
    IoStdoutBytes,
    /// `io.stderrText`.
    IoStderrText,
    /// `io.stderrBytes`.
    IoStderrBytes,
    /// `fs.readFile`.
    FsReadFile,
    /// `fs.readText`.
    FsReadText,
    /// `fs.readEntries`.
    FsReadEntries,
    /// `fs.writeFile`.
    FsWriteFile,
    /// `fs.writeText`.
    FsWriteText,
    /// `fs.metadataStat`.
    FsMetadataStat,
    /// `env.readVar`.
    EnvReadVar,
    /// `env.readArgs`.
    EnvReadArgs,
    /// `time.nowWall`.
    TimeNowWall,
    /// `time.nowMonotonic`.
    TimeNowMonotonic,
    /// `random.generateBytes`.
    RandomGenerateBytes,
}

impl HostOp {
    /// Every operation, in the order of the chapter's table.
    pub const ALL: [Self; 17] = [
        Self::IoStdinText,
        Self::IoStdinBytes,
        Self::IoStdoutText,
        Self::IoStdoutBytes,
        Self::IoStderrText,
        Self::IoStderrBytes,
        Self::FsReadFile,
        Self::FsReadText,
        Self::FsReadEntries,
        Self::FsWriteFile,
        Self::FsWriteText,
        Self::FsMetadataStat,
        Self::EnvReadVar,
        Self::EnvReadArgs,
        Self::TimeNowWall,
        Self::TimeNowMonotonic,
        Self::RandomGenerateBytes,
    ];

    /// The operation's row.
    #[must_use]
    pub const fn row(self) -> HostRow {
        use HostErrorType::{Env, Fs, Io, Random, Time};
        use HostType::{Bytes, FsMetadata, I64, List, Str, U64, Void};
        match self {
            Self::IoStdinText => row(self, "io.stdinText", MAX, Str, Io),
            Self::IoStdinBytes => row(self, "io.stdinBytes", MAX, Bytes, Io),
            Self::IoStdoutText => row(self, "io.stdoutText", VALUE_STR, Void, Io),
            Self::IoStdoutBytes => row(self, "io.stdoutBytes", VALUE_BYTES, Void, Io),
            Self::IoStderrText => row(self, "io.stderrText", VALUE_STR, Void, Io),
            Self::IoStderrBytes => row(self, "io.stderrBytes", VALUE_BYTES, Void, Io),
            Self::FsReadFile => row(self, "fs.readFile", AT, Bytes, Fs),
            Self::FsReadText => row(self, "fs.readText", AT, Str, Fs),
            Self::FsReadEntries => row(self, "fs.readEntries", AT, List(&Str), Fs),
            Self::FsWriteFile => row(self, "fs.writeFile", AT_BYTES, Void, Fs),
            Self::FsWriteText => row(self, "fs.writeText", AT_STR, Void, Fs),
            Self::FsMetadataStat => row(self, "fs.metadataStat", AT, FsMetadata, Fs),
            Self::EnvReadVar => row(self, "env.readVar", NAME, Str, Env),
            Self::EnvReadArgs => row(self, "env.readArgs", NONE, List(&Str), Env),
            Self::TimeNowWall => row(self, "time.nowWall", NONE, I64, Time),
            Self::TimeNowMonotonic => row(self, "time.nowMonotonic", NONE, U64, Time),
            Self::RandomGenerateBytes => {
                row(self, "random.generateBytes", COUNT, Bytes, Random)
            }
        }
    }

    /// The wire symbol, such as `io.stdinText`.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        self.row().symbol
    }

    /// The interface that owns the operation.
    #[must_use]
    pub const fn interface(self) -> HostInterface {
        match self {
            Self::IoStdinText => HostInterface::StdinText,
            Self::IoStdinBytes => HostInterface::StdinBytes,
            Self::IoStdoutText => HostInterface::StdoutText,
            Self::IoStdoutBytes => HostInterface::StdoutBytes,
            Self::IoStderrText => HostInterface::StderrText,
            Self::IoStderrBytes => HostInterface::StderrBytes,
            Self::FsReadFile => HostInterface::DirReadBytes,
            Self::FsReadText => HostInterface::DirReadText,
            Self::FsReadEntries => HostInterface::DirList,
            Self::FsWriteFile => HostInterface::DirWriteBytes,
            Self::FsWriteText => HostInterface::DirWriteText,
            Self::FsMetadataStat => HostInterface::DirStat,
            Self::EnvReadVar => HostInterface::EnvReadVar,
            Self::EnvReadArgs => HostInterface::EnvReadArgs,
            Self::TimeNowWall => HostInterface::WallClock,
            Self::TimeNowMonotonic => HostInterface::MonotonicClock,
            Self::RandomGenerateBytes => HostInterface::RandomBytes,
        }
    }
}

/// A type that appears in a registry signature, as the standard library
/// spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HostType {
    /// `Void`.
    Void,
    /// `Str`.
    Str,
    /// `Bytes`.
    Bytes,
    /// `I64`.
    I64,
    /// `U64`.
    U64,
    /// `RelativePath`, declared in `@std.fs`.
    RelativePath,
    /// `FsKind`, the variant type `@std.fs` declares.
    FsKind,
    /// `FsMetadata`, the record `@std.fs` declares.
    FsMetadata,
    /// `List(T)`.
    List(&'static Self),
    /// `Dict(K V)`.
    Dict(&'static Self, &'static Self),
    /// A standard operation interface, as the type of its grants.
    Interface(HostInterface),
    /// `TestWorld`, declared in `@std.test`.
    TestWorld,
    /// A structural record, its fields in the chapter's order.
    Record(&'static [HostParameter]),
}

impl HostType {
    /// The type in source spelling, such as `List(Str)` or
    /// `{stdout: Bytes stderr: Bytes}`.
    #[must_use]
    pub fn spelling(self) -> String {
        match self {
            Self::Void => "Void".to_owned(),
            Self::Str => "Str".to_owned(),
            Self::Bytes => "Bytes".to_owned(),
            Self::I64 => "I64".to_owned(),
            Self::U64 => "U64".to_owned(),
            Self::RelativePath => "RelativePath".to_owned(),
            Self::FsKind => "FsKind".to_owned(),
            Self::FsMetadata => "FsMetadata".to_owned(),
            Self::List(element) => format!("List({})", element.spelling()),
            Self::Dict(key, value) => {
                format!("Dict({} {})", key.spelling(), value.spelling())
            }
            Self::Interface(interface) => interface.name().to_owned(),
            Self::TestWorld => TEST_WORLD_OWNER.name.to_owned(),
            Self::Record(fields) => format!("{{{}}}", labelled(fields)),
        }
    }
}

/// The error type of a host row's result: a nominal library declaration
/// with fieldless cases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HostErrorType {
    /// `IoError` of `@std.io`.
    Io,
    /// `FsError` of `@std.fs`.
    Fs,
    /// `EnvError` of `@std.env`.
    Env,
    /// `TimeError` of `@std.time`.
    Time,
    /// `RandomError` of `@std.random`.
    Random,
}

/// The four cases every host error type has after its own.
pub const SHARED_ERROR_CASES: [&str; 4] =
    ["denied", "limitExceeded", "unsupported", "outcomeUnknown"];

impl HostErrorType {
    /// Every error type, in the order of the chapter's table.
    pub const ALL: [Self; 5] =
        [Self::Io, Self::Fs, Self::Env, Self::Time, Self::Random];

    /// The type's name, such as `IoError`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Io => "IoError",
            Self::Fs => "FsError",
            Self::Env => "EnvError",
            Self::Time => "TimeError",
            Self::Random => "RandomError",
        }
    }

    /// The module path atom that declares the type, such as `@std.io`.
    #[must_use]
    pub const fn module(self) -> &'static str {
        match self {
            Self::Io => "@std.io",
            Self::Fs => "@std.fs",
            Self::Env => "@std.env",
            Self::Time => "@std.time",
            Self::Random => "@std.random",
        }
    }

    /// The type's own cases, in declaration order.
    #[must_use]
    pub const fn own_cases(self) -> &'static [&'static str] {
        match self {
            Self::Io => &["invalidText", "closed", "other"],
            Self::Fs => &[
                "notFound",
                "permissionDenied",
                "notAFile",
                "notADirectory",
                "invalidPath",
                "invalidText",
                "other",
            ],
            Self::Env => &["notPresent", "notUnicode"],
            Self::Time | Self::Random => &["unavailable"],
        }
    }

    /// Every case in declaration order: the type's own, then the four
    /// [shared ones](SHARED_ERROR_CASES).
    #[must_use]
    pub fn cases(self) -> Vec<&'static str> {
        self.own_cases()
            .iter()
            .chain(&SHARED_ERROR_CASES)
            .copied()
            .collect()
    }
}

/// The cases of `FsKind` of `@std.fs`, in declaration order.
pub const FS_KIND_CASES: [&str; 3] = ["file", "directory", "other"];

/// The fields of the record `FsMetadata` of `@std.fs`, in declaration order.
pub const FS_METADATA_FIELDS: &[HostParameter] = &[
    param("kind", HostType::FsKind),
    param("size", HostType::U64),
];

/// One parameter of a registry row, or one field of a record type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HostParameter {
    /// The name the chapter gives it.
    pub name: &'static str,
    /// Its type.
    pub ty: HostType,
}

/// One row of the host registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostRow {
    /// The wire symbol, the row's identity.
    pub symbol: &'static str,
    /// The operation the row names.
    pub op: HostOp,
    /// The interface that owns the operation.
    pub owner: HostInterface,
    /// The parameters after the grant, in order.
    pub parameters: &'static [HostParameter],
    /// The type of the `Ok` payload of the result.
    pub ok: HostType,
    /// The error type of the result.
    pub error: HostErrorType,
    /// The registry minor that added the row.
    pub since: u32,
    /// The delivery class.
    pub delivery: Delivery,
}

impl HostRow {
    /// The exact signature in source spelling, the grant first, such as
    /// `Proc({StdoutText Str} Result(Void IoError) {StdoutText})`.
    #[must_use]
    pub fn signature(&self) -> String {
        let owner = self.owner.name();
        let parameters = std::iter::once(owner.to_owned())
            .chain(
                self.parameters
                    .iter()
                    .map(|parameter| parameter.ty.spelling()),
            )
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "Proc({{{parameters}}} Result({} {}) {{{owner}}})",
            self.ok.spelling(),
            self.error.name()
        )
    }
}

const fn param(name: &'static str, ty: HostType) -> HostParameter {
    HostParameter { name, ty }
}

const MAX: &[HostParameter] = &[param("max", HostType::U64)];
const VALUE_STR: &[HostParameter] = &[param("value", HostType::Str)];
const VALUE_BYTES: &[HostParameter] = &[param("value", HostType::Bytes)];
const AT: &[HostParameter] = &[param("at", HostType::RelativePath)];
const AT_BYTES: &[HostParameter] = &[
    param("at", HostType::RelativePath),
    param("contents", HostType::Bytes),
];
const AT_STR: &[HostParameter] = &[
    param("at", HostType::RelativePath),
    param("contents", HostType::Str),
];
const NAME: &[HostParameter] = &[param("name", HostType::Str)];
const COUNT: &[HostParameter] = &[param("count", HostType::U64)];
const NONE: &[HostParameter] = &[];

const fn row(
    op: HostOp,
    symbol: &'static str,
    parameters: &'static [HostParameter],
    ok: HostType,
    error: HostErrorType,
) -> HostRow {
    HostRow {
        symbol,
        op,
        owner: op.interface(),
        parameters,
        ok,
        error,
        since: 0,
        delivery: Delivery::Once,
    }
}

/// The 17 rows of registry 2.0, in the order of the chapter's table.
pub const HOST_ROWS: [HostRow; 17] = [
    HostOp::IoStdinText.row(),
    HostOp::IoStdinBytes.row(),
    HostOp::IoStdoutText.row(),
    HostOp::IoStdoutBytes.row(),
    HostOp::IoStderrText.row(),
    HostOp::IoStderrBytes.row(),
    HostOp::FsReadFile.row(),
    HostOp::FsReadText.row(),
    HostOp::FsReadEntries.row(),
    HostOp::FsWriteFile.row(),
    HostOp::FsWriteText.row(),
    HostOp::FsMetadataStat.row(),
    HostOp::EnvReadVar.row(),
    HostOp::EnvReadArgs.row(),
    HostOp::TimeNowWall.row(),
    HostOp::TimeNowMonotonic.row(),
    HostOp::RandomGenerateBytes.row(),
];

/// The owner of every test-world row: `TestWorld` of `@std.test`.
pub const TEST_WORLD_OWNER: Owner = Owner {
    module: "@std.test",
    name: "TestWorld",
};

/// One of the six members of `TestWorld`, named after its wire symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TestWorldOp {
    /// `testWorld.directory`.
    Directory,
    /// `testWorld.streams`.
    Streams,
    /// `testWorld.environment`.
    Environment,
    /// `testWorld.clocks`.
    Clocks,
    /// `testWorld.random`.
    Random,
    /// `testWorld.output`.
    Output,
}

const DIRECTORY_GRANTS: &[HostParameter] = &[
    param(
        "readBytes",
        HostType::Interface(HostInterface::DirReadBytes),
    ),
    param("readText", HostType::Interface(HostInterface::DirReadText)),
    param("list", HostType::Interface(HostInterface::DirList)),
    param(
        "writeBytes",
        HostType::Interface(HostInterface::DirWriteBytes),
    ),
    param(
        "writeText",
        HostType::Interface(HostInterface::DirWriteText),
    ),
    param("stat", HostType::Interface(HostInterface::DirStat)),
];
const STREAM_GRANTS: &[HostParameter] = &[
    param("stdinText", HostType::Interface(HostInterface::StdinText)),
    param("stdinBytes", HostType::Interface(HostInterface::StdinBytes)),
    param("stdoutText", HostType::Interface(HostInterface::StdoutText)),
    param(
        "stdoutBytes",
        HostType::Interface(HostInterface::StdoutBytes),
    ),
    param("stderrText", HostType::Interface(HostInterface::StderrText)),
    param(
        "stderrBytes",
        HostType::Interface(HostInterface::StderrBytes),
    ),
];
const ENVIRONMENT_GRANTS: &[HostParameter] = &[
    param("variables", HostType::Interface(HostInterface::EnvReadVar)),
    param("arguments", HostType::Interface(HostInterface::EnvReadArgs)),
];
const CLOCK_GRANTS: &[HostParameter] = &[
    param("wall", HostType::Interface(HostInterface::WallClock)),
    param(
        "monotonic",
        HostType::Interface(HostInterface::MonotonicClock),
    ),
];
const OUTPUT: &[HostParameter] = &[
    param("stdout", HostType::Bytes),
    param("stderr", HostType::Bytes),
];
const FILES: &[HostParameter] = &[param(
    "files",
    HostType::Dict(&HostType::RelativePath, &HostType::Str),
)];
const STDIN: &[HostParameter] = &[param("stdin", HostType::Str)];
const ENVIRONMENT: &[HostParameter] = &[
    param("variables", HostType::Dict(&HostType::Str, &HostType::Str)),
    param("arguments", HostType::List(&HostType::Str)),
];

impl TestWorldOp {
    /// Every member, in the order of the chapter's table.
    pub const ALL: [Self; 6] = [
        Self::Directory,
        Self::Streams,
        Self::Environment,
        Self::Clocks,
        Self::Random,
        Self::Output,
    ];

    /// The member's row.
    #[must_use]
    pub const fn row(self) -> TestWorldRow {
        let (symbol, labelled, result) = match self {
            Self::Directory => (
                "testWorld.directory",
                FILES,
                HostType::Record(DIRECTORY_GRANTS),
            ),
            Self::Streams => {
                ("testWorld.streams", STDIN, HostType::Record(STREAM_GRANTS))
            }
            Self::Environment => (
                "testWorld.environment",
                ENVIRONMENT,
                HostType::Record(ENVIRONMENT_GRANTS),
            ),
            Self::Clocks => ("testWorld.clocks", NONE, HostType::Record(CLOCK_GRANTS)),
            Self::Random => (
                "testWorld.random",
                NONE,
                HostType::Interface(HostInterface::RandomBytes),
            ),
            Self::Output => ("testWorld.output", NONE, HostType::Record(OUTPUT)),
        };
        TestWorldRow {
            symbol,
            op: self,
            labelled,
            result,
            since: 0,
            delivery: Delivery::Once,
        }
    }

    /// The wire symbol, such as `testWorld.directory`.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        self.row().symbol
    }

    /// The member's name, the part of the symbol after `testWorld.`.
    #[must_use]
    pub const fn member(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::Streams => "streams",
            Self::Environment => "environment",
            Self::Clocks => "clocks",
            Self::Random => "random",
            Self::Output => "output",
        }
    }
}

/// One row of the test-world registry. Its owner is
/// [`TEST_WORLD_OWNER`]; it takes the world as its one required parameter,
/// then its fixture data as labelled parameters, and it cannot fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TestWorldRow {
    /// The wire symbol, the row's identity.
    pub symbol: &'static str,
    /// The member the row names.
    pub op: TestWorldOp,
    /// The labelled parameters after the world, in order.
    pub labelled: &'static [HostParameter],
    /// The result of an invocation.
    pub result: HostType,
    /// The registry minor that added the row.
    pub since: u32,
    /// The delivery class of an invocation.
    pub delivery: Delivery,
}

impl TestWorldRow {
    /// The row's owner, `TestWorld` of `@std.test`.
    #[must_use]
    pub const fn owner(&self) -> Owner {
        TEST_WORLD_OWNER
    }

    /// The exact signature in source spelling, the world first, such as
    /// `Proc({TestWorld} RandomBytes {TestWorld})`.
    #[must_use]
    pub fn signature(&self) -> String {
        let owner = TEST_WORLD_OWNER.name;
        let inputs = if self.labelled.is_empty() {
            owner.to_owned()
        } else {
            format!("{owner} {}", labelled(self.labelled))
        };
        format!("Proc({{{inputs}}} {} {{{owner}}})", self.result.spelling())
    }
}

/// The six rows of the test-world registry, in the order of the chapter's
/// table.
pub const TEST_WORLD_ROWS: [TestWorldRow; 6] = [
    TestWorldOp::Directory.row(),
    TestWorldOp::Streams.row(),
    TestWorldOp::Environment.row(),
    TestWorldOp::Clocks.row(),
    TestWorldOp::Random.row(),
    TestWorldOp::Output.row(),
];

/// `name: T` for each parameter, separated by spaces.
fn labelled(parameters: &[HostParameter]) -> String {
    parameters
        .iter()
        .map(|parameter| format!("{}: {}", parameter.name, parameter.ty.spelling()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The host row with the wire symbol `symbol`. A test-world symbol has none.
#[must_use]
pub fn host_row(symbol: &str) -> Option<&'static HostRow> {
    HOST_ROWS.iter().find(|row| row.symbol == symbol)
}

/// The test-world row with the wire symbol `symbol`. A host symbol has none.
#[must_use]
pub fn test_world_row(symbol: &str) -> Option<&'static TestWorldRow> {
    TEST_WORLD_ROWS.iter().find(|row| row.symbol == symbol)
}
