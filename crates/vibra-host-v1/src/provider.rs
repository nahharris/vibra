//! The host provider trait and the typed answers of the 17 registry rows.
//!
//! `docs/spec/06-runtime.md`, "The host registry" and "The host world": a run is
//! given one host provider, and a host operation is an operation of that
//! provider. One trait serves the system provider (`run`) and the world provider
//! (`test`), so the language has no mode switch. Every operation answers a typed
//! value or the closed error enum its row declares; the one other way a call can
//! end is the host event of memory exhaustion, which is never a registry error.

/// How a provider call failed: an error of the operation's own closed enum, which
/// the program sees as the `err` of its result, or memory exhaustion, which is
/// the host event of "Activations and memory" and reaches no program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure<E> {
    /// The operation's own error.
    Error(E),
    /// The host could not allocate the answer.
    MemoryExhausted,
}

impl<E> From<E> for Failure<E> {
    fn from(error: E) -> Self {
        Self::Error(error)
    }
}

/// The answer of a provider call.
pub type Done<T, E> = Result<T, Failure<E>>;

macro_rules! error_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $name {
            $(
                #[doc = concat!("The variant `", $text, "`.")]
                $variant,
            )+
        }

        impl $name {
            /// Every variant, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The variant's atom spelling without `@`.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $text,)+
                }
            }

            /// The variant's position in the declaration.
            #[must_use]
            pub const fn index(self) -> u32 {
                self as u32
            }
        }
    };
}

error_enum! {
    /// `io-error` of `@std.io`.
    IoError { InvalidText = "invalid-text", Closed = "closed", Other = "other" }
}

error_enum! {
    /// `fs-error` of `@std.fs`.
    FsError {
        NotFound = "not-found",
        PermissionDenied = "permission-denied",
        NotAFile = "not-a-file",
        NotADirectory = "not-a-directory",
        InvalidPath = "invalid-path",
        InvalidText = "invalid-text",
        Other = "other",
    }
}

error_enum! {
    /// `env-error` of `@std.env`.
    EnvError { NotPresent = "not-present", NotUnicode = "not-unicode" }
}

error_enum! {
    /// `time-error` of `@std.time`.
    TimeError { Unavailable = "unavailable" }
}

error_enum! {
    /// `random-error` of `@std.random`.
    RandomError { Unavailable = "unavailable" }
}

error_enum! {
    /// `fs-kind` of `@std.fs`.
    FsKind { File = "file", Directory = "directory", Other = "other" }
}

/// `fs-metadata` of `@std.fs`: `(record kind fs-kind size u64)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metadata {
    /// What the path names.
    pub kind: FsKind,
    /// The byte length of a file's contents and `0` for every other kind.
    pub size: u64,
}

/// The host provider: one method for each row of the registry.
///
/// A method does exactly what its row's meaning in "The host registry" says and
/// adds no behavior. A provider MUST NOT read ambient state in any other place:
/// every ambient read of a running program passes through a provider and a
/// registry entry.
pub trait HostProvider {
    /// `io.stdin-text`: the rest of standard input as text.
    fn stdin_text(&mut self) -> Done<String, IoError>;
    /// `io.stdin-bytes`: the rest of standard input.
    fn stdin_bytes(&mut self) -> Done<Vec<u8>, IoError>;
    /// `io.stdout-text`.
    fn stdout_text(&mut self, value: &str) -> Done<(), IoError>;
    /// `io.stdout-bytes`.
    fn stdout_bytes(&mut self, value: &[u8]) -> Done<(), IoError>;
    /// `io.stderr-text`.
    fn stderr_text(&mut self, value: &str) -> Done<(), IoError>;
    /// `io.stderr-bytes`.
    fn stderr_bytes(&mut self, value: &[u8]) -> Done<(), IoError>;
    /// `fs.read-file`.
    fn read_file(&mut self, at: &str) -> Done<Vec<u8>, FsError>;
    /// `fs.read-text`.
    fn read_text(&mut self, at: &str) -> Done<String, FsError>;
    /// `fs.read-entries`: the sorted names of a directory's entries.
    fn read_entries(&mut self, at: &str) -> Done<Vec<String>, FsError>;
    /// `fs.write-file`.
    fn write_file(&mut self, at: &str, contents: &[u8]) -> Done<(), FsError>;
    /// `fs.write-text`.
    fn write_text(&mut self, at: &str, contents: &str) -> Done<(), FsError>;
    /// `fs.metadata-stat`.
    fn stat(&mut self, at: &str) -> Done<Metadata, FsError>;
    /// `env.read-var`.
    fn env_var(&mut self, name: &str) -> Done<String, EnvError>;
    /// `env.read-args`.
    fn env_args(&mut self) -> Done<Vec<String>, EnvError>;
    /// `time.now-wall`: nanoseconds since the Unix epoch.
    fn now_wall(&mut self) -> Done<i64, TimeError>;
    /// `time.now-monotonic`: nanoseconds from an origin the instance fixes.
    fn now_monotonic(&mut self) -> Done<u64, TimeError>;
    /// `random.generate-bytes`: exactly `count` bytes.
    fn random_bytes(&mut self, count: u64) -> Done<Vec<u8>, RandomError>;
}

/// Allocates a zero-filled vector of `count` bytes, or reports that the host
/// cannot.
pub(crate) fn try_zeroed(count: u64) -> Option<Vec<u8>> {
    let count = usize::try_from(count).ok()?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(count).ok()?;
    bytes.resize(count, 0);
    Some(bytes)
}

/// Copies `source` into a vector, or reports that the host cannot allocate it.
pub(crate) fn try_copy(source: &[u8]) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(source.len()).ok()?;
    bytes.extend_from_slice(source);
    Some(bytes)
}
