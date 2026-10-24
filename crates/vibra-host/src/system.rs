//! The system provider of `run`.
//!
//! `docs/spec/06-runtime.md`, "The host world": `run` uses the real standard
//! streams, filesystem, environment, clocks, and the operating system's random
//! source. This is the one module of the toolchain's execution crates that reads
//! ambient state, so that every ambient read of a running program passes through
//! a provider and a registry row; a host test scans the other crates for these
//! APIs.

use std::ffi::OsString;
use std::io::{ErrorKind, Read, Write};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::provider::{
    Done, EnvError, Failure, FsError, FsKind, HostProvider, IoError, Metadata,
    RandomError, TimeError, try_zeroed,
};

/// The provider over the real host.
#[derive(Debug)]
pub struct SystemProvider {
    arguments: Vec<OsString>,
    origin: Instant,
    /// Where standard output and error go, chosen by the command.
    sink: Sink,
}

/// Where the system provider delivers standard output and error: the command
/// chooses, and the language has no such switch. `run` in human mode inherits
/// the process's streams; in JSON mode it captures the bytes for the envelope.
#[derive(Debug)]
enum Sink {
    Inherit,
    Capture { stdout: Vec<u8>, stderr: Vec<u8> },
}

impl Default for SystemProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemProvider {
    /// A provider with no program arguments. The monotonic clock counts from
    /// this instant, the origin the instance fixes.
    #[must_use]
    pub fn new() -> Self {
        Self::with_arguments(Vec::new())
    }

    /// A provider whose `env.read-args` returns `arguments`, which the command
    /// line supplied after `--`.
    #[must_use]
    pub fn with_arguments(arguments: Vec<OsString>) -> Self {
        Self {
            arguments,
            origin: Instant::now(),
            sink: Sink::Inherit,
        }
    }

    /// The same provider with standard output and error captured instead of
    /// delivered to the process, for a command that reports them itself.
    #[must_use]
    pub fn capturing(mut self) -> Self {
        self.sink = Sink::Capture {
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        self
    }

    /// The bytes written to standard output so far, when it is captured.
    #[must_use]
    pub fn captured_stdout(&self) -> &[u8] {
        match &self.sink {
            Sink::Capture { stdout, .. } => stdout,
            Sink::Inherit => &[],
        }
    }

    /// The bytes written to standard error so far, when it is captured.
    #[must_use]
    pub fn captured_stderr(&self) -> &[u8] {
        match &self.sink {
            Sink::Capture { stderr, .. } => stderr,
            Sink::Inherit => &[],
        }
    }
}

fn io_error(error: &std::io::Error) -> IoError {
    match error.kind() {
        ErrorKind::BrokenPipe => IoError::Closed,
        _ => IoError::Other,
    }
}

fn fs_error(error: &std::io::Error) -> FsError {
    match error.kind() {
        ErrorKind::NotFound => FsError::NotFound,
        ErrorKind::PermissionDenied => FsError::PermissionDenied,
        ErrorKind::NotADirectory => FsError::NotADirectory,
        ErrorKind::IsADirectory => FsError::NotAFile,
        ErrorKind::InvalidInput | ErrorKind::InvalidFilename => FsError::InvalidPath,
        _ => FsError::Other,
    }
}

fn path_text(at: &str) -> Result<&std::path::Path, FsError> {
    if at.contains('\0') {
        Err(FsError::InvalidPath)
    } else {
        Ok(std::path::Path::new(at))
    }
}

fn read_all(reader: &mut impl Read, length: u64) -> Result<Vec<u8>, ReadFailure> {
    let mut bytes = Vec::new();
    let reserve = usize::try_from(length).map_err(|_| ReadFailure::Memory)?;
    bytes
        .try_reserve_exact(reserve)
        .map_err(|_| ReadFailure::Memory)?;
    let mut chunk = [0_u8; 8192];
    loop {
        let count = reader.read(&mut chunk).map_err(ReadFailure::Io)?;
        let Some(read) = chunk.get(..count).filter(|read| !read.is_empty()) else {
            return Ok(bytes);
        };
        bytes
            .try_reserve(read.len())
            .map_err(|_| ReadFailure::Memory)?;
        bytes.extend_from_slice(read);
    }
}

enum ReadFailure {
    Memory,
    Io(std::io::Error),
}

impl SystemProvider {
    /// Delivers `value` to standard output, or to standard error when `error`.
    fn deliver(&mut self, error: bool, value: &[u8]) -> Done<(), IoError> {
        match &mut self.sink {
            Sink::Inherit if error => {
                Self::write_stream(&mut std::io::stderr().lock(), value)
            }
            Sink::Inherit => Self::write_stream(&mut std::io::stdout().lock(), value),
            Sink::Capture { stdout, stderr } => {
                let buffer = if error { stderr } else { stdout };
                buffer
                    .try_reserve(value.len())
                    .map_err(|_| Failure::MemoryExhausted)?;
                buffer.extend_from_slice(value);
                Ok(())
            }
        }
    }
}

impl SystemProvider {
    fn file_bytes(at: &str) -> Done<Vec<u8>, FsError> {
        let path = path_text(at)?;
        let metadata = std::fs::metadata(path).map_err(|error| fs_error(&error))?;
        if metadata.is_dir() {
            return Err(Failure::Error(FsError::NotAFile));
        }
        let mut file = std::fs::File::open(path).map_err(|error| fs_error(&error))?;
        read_all(&mut file, metadata.len()).map_err(|failure| match failure {
            ReadFailure::Memory => Failure::MemoryExhausted,
            ReadFailure::Io(error) => Failure::Error(fs_error(&error)),
        })
    }

    fn write(at: &str, contents: &[u8]) -> Done<(), FsError> {
        let path = path_text(at)?;
        if std::fs::metadata(path).is_ok_and(|metadata| metadata.is_dir()) {
            return Err(Failure::Error(FsError::NotAFile));
        }
        std::fs::write(path, contents).map_err(|error| Failure::Error(fs_error(&error)))
    }

    fn write_stream(stream: &mut impl Write, value: &[u8]) -> Done<(), IoError> {
        stream
            .write_all(value)
            .and_then(|()| stream.flush())
            .map_err(|error| Failure::Error(io_error(&error)))
    }

    fn stdin_all() -> Done<Vec<u8>, IoError> {
        read_all(&mut std::io::stdin().lock(), 0).map_err(|failure| match failure {
            ReadFailure::Memory => Failure::MemoryExhausted,
            ReadFailure::Io(_) => Failure::Error(IoError::Other),
        })
    }
}

impl HostProvider for SystemProvider {
    fn stdin_text(&mut self) -> Done<String, IoError> {
        String::from_utf8(Self::stdin_all()?)
            .map_err(|_| Failure::Error(IoError::InvalidText))
    }

    fn stdin_bytes(&mut self) -> Done<Vec<u8>, IoError> {
        Self::stdin_all()
    }

    fn stdout_text(&mut self, value: &str) -> Done<(), IoError> {
        self.deliver(false, value.as_bytes())
    }

    fn stdout_bytes(&mut self, value: &[u8]) -> Done<(), IoError> {
        self.deliver(false, value)
    }

    fn stderr_text(&mut self, value: &str) -> Done<(), IoError> {
        self.deliver(true, value.as_bytes())
    }

    fn stderr_bytes(&mut self, value: &[u8]) -> Done<(), IoError> {
        self.deliver(true, value)
    }

    fn read_file(&mut self, at: &str) -> Done<Vec<u8>, FsError> {
        Self::file_bytes(at)
    }

    fn read_text(&mut self, at: &str) -> Done<String, FsError> {
        String::from_utf8(Self::file_bytes(at)?)
            .map_err(|_| Failure::Error(FsError::InvalidText))
    }

    fn read_entries(&mut self, at: &str) -> Done<Vec<String>, FsError> {
        let path = path_text(at)?;
        let entries = std::fs::read_dir(path).map_err(|error| fs_error(&error))?;
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| fs_error(&error))?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| FsError::InvalidText)?;
            names.try_reserve(1).map_err(|_| Failure::MemoryExhausted)?;
            names.push(name);
        }
        // Sorted by Unicode scalar order, which is the byte order of UTF-8, so
        // the result does not depend on the order the host lists them in.
        names.sort();
        Ok(names)
    }

    fn write_file(&mut self, at: &str, contents: &[u8]) -> Done<(), FsError> {
        Self::write(at, contents)
    }

    fn write_text(&mut self, at: &str, contents: &str) -> Done<(), FsError> {
        Self::write(at, contents.as_bytes())
    }

    fn stat(&mut self, at: &str) -> Done<Metadata, FsError> {
        let path = path_text(at)?;
        // `metadata` follows symbolic links.
        let metadata = std::fs::metadata(path).map_err(|error| fs_error(&error))?;
        Ok(if metadata.is_file() {
            Metadata {
                kind: FsKind::File,
                size: metadata.len(),
            }
        } else if metadata.is_dir() {
            Metadata {
                kind: FsKind::Directory,
                size: 0,
            }
        } else {
            Metadata {
                kind: FsKind::Other,
                size: 0,
            }
        })
    }

    fn env_var(&mut self, name: &str) -> Done<String, EnvError> {
        // A name that no environment can hold is not present, and the standard
        // library would reject it.
        if name.is_empty() || name.contains('=') || name.contains('\0') {
            return Err(Failure::Error(EnvError::NotPresent));
        }
        let value = std::env::var_os(name).ok_or(EnvError::NotPresent)?;
        value
            .into_string()
            .map_err(|_| Failure::Error(EnvError::NotUnicode))
    }

    fn env_args(&mut self) -> Done<Vec<String>, EnvError> {
        let mut arguments = Vec::new();
        for argument in &self.arguments {
            let text = argument.to_str().ok_or(EnvError::NotUnicode)?;
            arguments
                .try_reserve(1)
                .map_err(|_| Failure::MemoryExhausted)?;
            arguments.push(text.to_owned());
        }
        Ok(arguments)
    }

    fn now_wall(&mut self) -> Done<i64, TimeError> {
        let nanoseconds = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(after) => i128::try_from(after.as_nanos()),
            Err(before) => i128::try_from(before.duration().as_nanos()).map(|n| -n),
        };
        nanoseconds
            .ok()
            .and_then(|value| i64::try_from(value).ok())
            .ok_or(Failure::Error(TimeError::Unavailable))
    }

    fn now_monotonic(&mut self) -> Done<u64, TimeError> {
        u64::try_from(self.origin.elapsed().as_nanos())
            .map_err(|_| Failure::Error(TimeError::Unavailable))
    }

    fn random_bytes(&mut self, count: u64) -> Done<Vec<u8>, RandomError> {
        let mut bytes = try_zeroed(count).ok_or(Failure::MemoryExhausted)?;
        getrandom::fill(&mut bytes).map_err(|_| RandomError::Unavailable)?;
        Ok(bytes)
    }
}
