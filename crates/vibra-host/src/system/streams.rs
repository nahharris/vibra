//! The stream adapters: `io.stdinText`, `io.stdinBytes`, and the four
//! writes to the standard output and error.
//!
//! `docs/spec/06-runtime.md`, "Standard input" and "Standard output and
//! error", fix what a read and a write do.
//!
//! **A read** reports the remaining size to the gate before it takes a byte:
//! `Some` when the input is a regular file, `None` otherwise (a pipe, a
//! console, or a replacement that declares no size). It then takes at most
//! the count the gate reserved, which is never more than `max`. Progress is
//! the bytes the read consumed, whatever the reply: the bytes of a read that
//! fails part-way, of an undecodable window, and of a value all count.
//!
//! A text read returns whole scalars. When its window ends inside a scalar
//! whose bytes are valid so far, before the end of input, it returns the
//! scalars before that one and consumes exactly their bytes: the adapter
//! keeps the bytes it took and did not consume, and the next read starts
//! with them. To tell "before the end of input" from "at the end" for an
//! input of unknown size, the adapter takes one more byte, which it keeps
//! the same way and does not charge. When the grant is released, the kept
//! bytes are given back: a regular file is positioned just after the bytes
//! delivered, and a pipe or a console cannot take them back.
//!
//! **A write** delivers the whole value. A broken pipe is `closed`, and any
//! other failure is `other`, both completed calls: a failed write may have
//! sent a prefix. The progress of a failed write is `Known(n)` when the
//! operating system reported the `n` bytes written before the failure and
//! `Uncertain` when it did not:
//!
//! - to a file or a pipe on a POSIX system, the bytes of the `write` calls
//!   that succeeded: a `write` that fails wrote nothing;
//! - to a file or a pipe on Windows, `Uncertain`: a failed `WriteFile` can
//!   have written bytes it does not report;
//! - to a terminal, `Uncertain`: the standard library buffers the stream,
//!   and its count is of the buffer, not of the terminal.

use std::fs::File;
use std::io::{self, IsTerminal, Read, Seek, Write};

use vibra_ir::authority::HostOk;
use vibra_ir::external::host::HostOp;

use super::{Answer, length, moved};
use crate::driver::{Action, Found, FoundAction, Gate, Measured, Operand, Progress};

/// The length of the longest scalar in UTF-8: the least `max` of a text
/// read.
const LONGEST_SCALAR: u64 = 4;

/// The most bytes a read asks of its source at once: a read of a large
/// `max` from a short input allocates no more than this past what it took.
const CHUNK: usize = 64 * 1024;

/// A standard input the adapter reads.
pub trait InputSource {
    /// The bytes that remain, when the source knows them: the rest of a
    /// regular file. `None` for a pipe or a console.
    fn remaining(&mut self) -> Option<u64>;

    /// Reads at most `buffer.len()` bytes into `buffer`; `Ok(0)` is the end
    /// of input.
    ///
    /// # Errors
    ///
    /// The input cannot be read. [`io::ErrorKind::Interrupted`] is retried.
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize>;

    /// Gives back the last `count` bytes read, which the adapter took and
    /// did not deliver, when the grant is released: a regular file is
    /// positioned `count` bytes back. A pipe or a console cannot take bytes
    /// back and does nothing, an accepted limit of the platform (06
    /// "Standard input").
    ///
    /// # Errors
    ///
    /// The position cannot be changed.
    fn unread(&mut self, count: u64) -> io::Result<()>;
}

/// Why a whole-value write failed, and what the system reported of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteFailure {
    /// The kind of the failure: [`io::ErrorKind::BrokenPipe`] is a closed
    /// stream.
    pub kind: io::ErrorKind,
    /// The bytes written before the failure, when the system reported
    /// them; `None` when it did not.
    pub written: Option<u64>,
}

/// A standard output or error the adapter writes.
pub trait OutputSink {
    /// Delivers the whole of `bytes`.
    ///
    /// # Errors
    ///
    /// The failure, with what the system reported of the bytes written.
    fn write(&mut self, bytes: &[u8]) -> Result<(), WriteFailure>;
}

/// The result of opening a stream of the process.
pub(super) enum Opened<T> {
    /// The stream, held.
    Open(T),
    /// The process has no such stream.
    Missing,
    /// The stream exists and could not be opened, for the reason given.
    Failed(String),
}

/// Which output stream of the process.
#[derive(Clone, Copy, Debug)]
pub(super) enum Which {
    Stdout,
    Stderr,
}

impl Which {
    const fn name(self) -> &'static str {
        match self {
            Self::Stdout => "standard output",
            Self::Stderr => "standard error",
        }
    }
}

/// Whether `error`, from duplicating a standard handle, says that the
/// process has no such stream: a closed descriptor, or an invalid handle.
fn missing(error: &io::Error) -> bool {
    // `EBADF` on Linux and macOS; `ERROR_INVALID_HANDLE` on Windows.
    const NO_STREAM: i32 = if cfg!(windows) { 6 } else { 9 };
    error.raw_os_error() == Some(NO_STREAM)
}

/// A duplicate of a standard handle as a file, which reads and writes the
/// operating system's stream directly, with no buffer of the standard
/// library in between.
#[cfg(unix)]
fn duplicate(stream: &impl std::os::fd::AsFd) -> Opened<File> {
    match stream.as_fd().try_clone_to_owned() {
        Ok(owned) => Opened::Open(File::from(owned)),
        Err(error) if missing(&error) => Opened::Missing,
        Err(error) => Opened::Failed(error.to_string()),
    }
}

/// A duplicate of a standard handle as a file, which reads and writes the
/// operating system's stream directly, with no buffer of the standard
/// library in between.
#[cfg(windows)]
fn duplicate(
    stream: &(impl std::os::windows::io::AsHandle + std::os::windows::io::AsRawHandle),
) -> Opened<File> {
    // A process with no such stream has a null standard handle.
    if stream.as_raw_handle().is_null() {
        return Opened::Missing;
    }
    match stream.as_handle().try_clone_to_owned() {
        Ok(owned) => Opened::Open(File::from(owned)),
        Err(error) if missing(&error) => Opened::Missing,
        Err(error) => Opened::Failed(error.to_string()),
    }
}

/// The process's standard input: a terminal through the standard library,
/// which decodes a console's input; any other input through a duplicate of
/// its handle.
pub(super) fn process_stdin() -> Opened<Box<dyn InputSource>> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return Opened::Open(Box::new(TerminalInput));
    }
    match duplicate(&stdin) {
        Opened::Open(file) => Opened::Open(Box::new(FileInput::new(file))),
        Opened::Missing => Opened::Missing,
        Opened::Failed(why) => {
            Opened::Failed(format!("the standard input could not be opened: {why}"))
        }
    }
}

/// The process's standard output or error: a terminal through the standard
/// library, which encodes for a console; any other stream through a
/// duplicate of its handle.
pub(super) fn process_output(which: Which) -> Opened<Box<dyn OutputSink>> {
    let duplicated = match which {
        Which::Stdout if io::stdout().is_terminal() => {
            return Opened::Open(Box::new(TerminalOutput(which)));
        }
        Which::Stderr if io::stderr().is_terminal() => {
            return Opened::Open(Box::new(TerminalOutput(which)));
        }
        Which::Stdout => duplicate(&io::stdout()),
        Which::Stderr => duplicate(&io::stderr()),
    };
    match duplicated {
        Opened::Open(file) => Opened::Open(Box::new(FileOutput(file))),
        Opened::Missing => Opened::Missing,
        Opened::Failed(why) => {
            Opened::Failed(format!("the {} could not be opened: {why}", which.name()))
        }
    }
}

/// A standard input that is a terminal.
struct TerminalInput;

impl InputSource for TerminalInput {
    fn remaining(&mut self) -> Option<u64> {
        None
    }

    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        io::stdin().lock().read(buffer)
    }

    fn unread(&mut self, _count: u64) -> io::Result<()> {
        Ok(())
    }
}

/// A standard input that is a file or a pipe.
struct FileInput {
    file: File,
    regular: bool,
}

impl FileInput {
    /// The input `file`, whose size is known only when it is a regular
    /// file. On Windows the standard library reports a pipe as a file whose
    /// length is what the pipe holds now, so the handle's file type decides.
    fn new(file: File) -> Self {
        let regular = vibra_fs_scope::is_regular_file(&file);
        Self { file, regular }
    }
}

impl InputSource for FileInput {
    fn remaining(&mut self) -> Option<u64> {
        if !self.regular {
            return None;
        }
        let size = self.file.metadata().ok()?.len();
        let position = self.file.stream_position().ok()?;
        Some(size.saturating_sub(position))
    }

    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.file.read(buffer)
    }

    fn unread(&mut self, count: u64) -> io::Result<()> {
        if !self.regular || count == 0 {
            return Ok(());
        }
        let back = i64::try_from(count).map_err(io::Error::other)?;
        self.file.seek(io::SeekFrom::Current(-back)).map(|_| ())
    }
}

/// A standard output or error that is a terminal.
struct TerminalOutput(Which);

impl OutputSink for TerminalOutput {
    fn write(&mut self, bytes: &[u8]) -> Result<(), WriteFailure> {
        let written = match self.0 {
            Which::Stdout => {
                let mut stdout = io::stdout().lock();
                stdout.write_all(bytes).and_then(|()| stdout.flush())
            }
            Which::Stderr => {
                let mut stderr = io::stderr().lock();
                stderr.write_all(bytes).and_then(|()| stderr.flush())
            }
        };
        written.map_err(|error| WriteFailure {
            kind: error.kind(),
            written: None,
        })
    }
}

/// A standard output or error that is a file or a pipe.
struct FileOutput(File);

impl OutputSink for FileOutput {
    fn write(&mut self, bytes: &[u8]) -> Result<(), WriteFailure> {
        let mut sent: usize = 0;
        while let Some(rest) = bytes.get(sent..).filter(|rest| !rest.is_empty()) {
            match self.0.write(rest) {
                Ok(0) => {
                    return Err(failure(io::ErrorKind::WriteZero, sent));
                }
                Ok(count) => sent = sent.saturating_add(count),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(failure(error.kind(), sent)),
            }
        }
        Ok(())
    }
}

/// The failure of a write to a file or a pipe after `sent` bytes went out.
fn failure(kind: io::ErrorKind, sent: usize) -> WriteFailure {
    WriteFailure {
        kind,
        // A POSIX `write` that fails wrote nothing, so the bytes of the calls
        // that succeeded are exact. A failed `WriteFile` may have written
        // bytes it does not report.
        written: cfg!(unix).then(|| length(sent)),
    }
}

/// A write of the operand of `action` to `sink`.
pub(super) fn write(sink: &mut dyn OutputSink, operand: Operand<'_>) -> Answer {
    let bytes = match operand {
        Operand::Text(text) => text.as_bytes(),
        Operand::Bytes(bytes) => bytes,
        Operand::None | Operand::Count(_) => {
            return Answer::NotSubmitted("unsupported");
        }
    };
    match sink.write(bytes) {
        Ok(()) => Answer::Ok(HostOk::Void, moved(0, length(bytes.len()))),
        Err(failure) => {
            let case = if failure.kind == io::ErrorKind::BrokenPipe {
                "closed"
            } else {
                "other"
            };
            let output = failure.written.map_or(Measured::Uncertain, Measured::Known);
            Answer::Err(
                case,
                Progress {
                    input: Measured::Known(0),
                    output,
                },
            )
        }
    }
}

/// The standard input as the adapter reads it: the source, and the bytes it
/// took from the source and has not consumed yet.
pub(super) struct Reader {
    source: Box<dyn InputSource>,
    /// The bytes the next read starts with.
    kept: Vec<u8>,
}

/// How filling a window ended.
enum Filled {
    /// The window holds the count asked for, or the input ended first.
    Window { bytes: Vec<u8>, ended: bool },
    /// The source failed after the adapter took this many bytes.
    Failed(u64),
    /// The operating system refused the window; nothing was consumed.
    NoRoom,
}

impl Reader {
    pub(super) fn new(source: Box<dyn InputSource>) -> Self {
        Self {
            source,
            kept: Vec::new(),
        }
    }

    /// The bytes that remain, when the source knows them.
    fn remaining(&mut self) -> Option<u64> {
        self.source
            .remaining()
            .map(|rest| rest.saturating_add(length(self.kept.len())))
    }

    /// Takes the next `count` bytes: the kept bytes first, then the source.
    fn fill(&mut self, count: usize) -> Filled {
        let mut window = Vec::new();
        if window.try_reserve(count.min(CHUNK)).is_err() {
            return Filled::NoRoom;
        }
        let kept = self.kept.len().min(count);
        window.extend(self.kept.drain(..kept));
        while window.len() < count {
            let start = window.len();
            let want = (count - start).min(CHUNK);
            if window.try_reserve(want).is_err() {
                // Give back what was taken: nothing is consumed.
                window.append(&mut self.kept);
                self.kept = window;
                return Filled::NoRoom;
            }
            window.resize(start.saturating_add(want), 0);
            let read = match window.get_mut(start..) {
                Some(space) => self.source.read(space),
                None => Ok(0),
            };
            match read {
                Ok(0) => {
                    window.truncate(start);
                    return Filled::Window {
                        bytes: window,
                        ended: true,
                    };
                }
                Ok(read) => window.truncate(start.saturating_add(read.min(want))),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    window.truncate(start);
                }
                Err(_) => {
                    window.truncate(start);
                    return Filled::Failed(length(window.len()));
                }
            }
        }
        Filled::Window {
            bytes: window,
            ended: false,
        }
    }

    /// Whether the input ends after the bytes taken so far, for an input of
    /// unknown size: takes one more byte and keeps it.
    fn ends_here(&mut self) -> bool {
        if !self.kept.is_empty() {
            return false;
        }
        let mut byte = [0_u8; 1];
        loop {
            match self.source.read(&mut byte) {
                Ok(0) => return true,
                Ok(_) => {
                    self.kept.extend_from_slice(&byte);
                    return false;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                // The next read reports the error.
                Err(_) => return false,
            }
        }
    }

    /// Gives the kept bytes back to the source when the grant is released,
    /// so a later reader of a regular file starts just after the bytes
    /// delivered.
    ///
    /// # Errors
    ///
    /// The source could not be positioned back.
    pub(super) fn release(&mut self) -> io::Result<()> {
        let kept = length(self.kept.len());
        self.kept.clear();
        self.source.unread(kept)
    }

    /// Puts `bytes` back in front of the kept bytes.
    fn keep(&mut self, mut bytes: Vec<u8>) {
        bytes.append(&mut self.kept);
        self.kept = bytes;
    }

    /// `io.stdinText` or `io.stdinBytes`.
    pub(super) fn read(&mut self, action: &Action<'_>, gate: &mut dyn Gate) -> Answer {
        let Operand::Count(max) = action.operand else {
            return Answer::NotSubmitted("unsupported");
        };
        let text = action.op == HostOp::IoStdinText;
        // A text count below the longest scalar is refused before the stream
        // is touched (Q30).
        if text && max < LONGEST_SCALAR {
            return Answer::NotSubmitted("unsupported");
        }
        let size = self.remaining();
        let found = Found {
            real_names: &[],
            action: FoundAction::Read,
            size,
            links: None,
        };
        let Ok(reserved) = gate.admit(&found) else {
            return Answer::Refused;
        };
        let count = usize::try_from(reserved.input.min(max)).unwrap_or(usize::MAX);
        let (window, ended) = match self.fill(count) {
            Filled::Window { bytes, ended } => (bytes, ended),
            Filled::Failed(taken) => return Answer::Err("other", moved(taken, 0)),
            // The operating system refused the window the broker admitted:
            // memory exhaustion (06 "Memory"). The bytes taken are kept, so
            // none was delivered.
            Filled::NoRoom => return Answer::MemoryExhausted(Progress::NONE),
        };
        let taken = length(window.len());
        if !text {
            return Answer::Ok(HostOk::Bytes(window), moved(taken, 0));
        }
        let error = match String::from_utf8(window) {
            Ok(decoded) => return Answer::Ok(HostOk::Text(decoded), moved(taken, 0)),
            Err(error) => error,
        };
        let utf8 = error.utf8_error();
        let window = error.into_bytes();
        // Any invalid sequence in the window: it is consumed whole.
        if utf8.error_len().is_some() {
            return Answer::Err("invalidText", moved(taken, 0));
        }
        // The window ends inside a scalar whose bytes are valid so far.
        let at_end = ended
            || match size {
                Some(size) => taken >= size,
                None => self.ends_here(),
            };
        if at_end {
            // A scalar cut off by the end of input.
            return Answer::Err("invalidText", moved(taken, 0));
        }
        let whole = utf8.valid_up_to();
        if whole == 0 {
            // A reservation below one scalar, which the gate never gives a
            // text read: nothing is consumed.
            self.keep(window);
            return Answer::NotSubmitted("limitExceeded");
        }
        let mut whole_bytes = window;
        let rest = whole_bytes.split_off(whole);
        self.keep(rest);
        match String::from_utf8(whole_bytes) {
            Ok(decoded) => Answer::Ok(HostOk::Text(decoded), moved(length(whole), 0)),
            // Unreachable: the bytes before `valid_up_to` are valid.
            Err(error) => {
                self.keep(error.into_bytes());
                Answer::NotSubmitted("unsupported")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "tests assert by panicking on the fixtures they build"
    )]

    use std::fs::File;
    use std::io::{self, Write as _};

    use super::{FileInput, InputSource};

    /// The reading end of an anonymous pipe, as the file a duplicated
    /// standard handle is.
    fn pipe_file(reader: io::PipeReader) -> File {
        #[cfg(windows)]
        let owned = std::os::windows::io::OwnedHandle::from(reader);
        #[cfg(unix)]
        let owned = std::os::fd::OwnedFd::from(reader);
        File::from(owned)
    }

    /// 06 "Standard input": a pipe has no known size, whether it is empty or
    /// holds bytes; only a regular file reports what remains (F1 of the
    /// drivers review, probe 26).
    #[test]
    fn a_pipe_has_no_known_size() {
        let (reader, mut writer) = io::pipe().expect("a pipe");
        let mut input = FileInput::new(pipe_file(reader));
        assert_eq!(input.remaining(), None, "an empty pipe");
        writer.write_all(b"abcde").expect("written");
        assert_eq!(input.remaining(), None, "a pipe holding five bytes");
        let mut two = [0_u8; 2];
        let read = input.read(&mut two).expect("read");
        assert_eq!(read, 2);
        assert_eq!(input.remaining(), None, "a pipe after a read");
    }

    #[test]
    fn a_regular_file_reports_what_remains() {
        let path = std::env::temp_dir()
            .join(format!("vibra-host-streams-regular-{}", std::process::id()));
        std::fs::write(&path, b"abcde").expect("the file");
        let file = File::open(&path).expect("opened");
        let mut input = FileInput::new(file);
        assert_eq!(input.remaining(), Some(5));
        let mut two = [0_u8; 2];
        assert_eq!(input.read(&mut two).expect("read"), 2);
        assert_eq!(input.remaining(), Some(3));
        drop(input);
        let _ = std::fs::remove_file(&path);
    }
}
