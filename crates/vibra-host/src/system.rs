//! The drivers over the process: the standard streams, the environment, the
//! arguments, the clocks, the random source, and the scoped directories.
//!
//! `docs/spec/06-runtime.md`, "Closed operation semantics", fixes what each
//! adapter does, and `docs/spec/04-programs-and-packages.md`, "Startup
//! order", what acquiring its resource answers. [`SystemDriver`] is the one
//! [`Driver`] over the process; the broker of [`crate::broker::Context`]
//! runs it exactly as it runs the driver of a host world.
//!
//! Every read of the process is in the five submodules: `streams`, `env`,
//! `clock`, `random`, and `fs`. This module names no ambient API; it
//! dispatches a call to the adapter of the resource it holds.
//!
//! **Construction reads nothing.** [`SystemDriver::new`] keeps its
//! [`SystemInputs`]; a stream of the process is opened when its provider
//! first acquires it, a monotonic clock takes its origin when it is
//! acquired, and a directory is opened once, when it is acquired.
//!
//! **Directories on Windows only.** `@host.fs.directory` is the file-scope
//! adapter of `vibra-fs-scope`, which exists on Windows. On every other
//! platform [`Driver::identity`] answers `None` for it, so admission refuses
//! a target that binds it as `@tool.unavailable` before anything is
//! acquired.
//!
//! **No adapter ends the process.** An operating-system error becomes a typed
//! reply, a refusal, or a failure to acquire; no adapter calls an exit
//! function, aborts, or panics.

mod clock;
mod env;
mod fs;
mod random;
mod streams;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::rc::Rc;

use vibra_fs_scope::ScopedDir;
use vibra_ir::authority::{HostErrorCase, HostOk, HostReply};
use vibra_ir::external::host::HostOp;

use crate::driver::{
    Acquire, Action, Completion, Driver, DriverIdentity, Gate, Measured, Progress,
    Resolved, ResourceId,
};
use crate::recipe::Provider;
use crate::world::DRIVER_VERSION;

use self::fs as directory;

pub use streams::{InputSource, OutputSink, WriteFailure};

/// Where the standard input comes from.
pub enum Input {
    /// The process's own standard input.
    Process,
    /// The process has no standard input: acquiring it is refused.
    Absent,
    /// A replacement, for the tests.
    Replaced(Box<dyn InputSource>),
}

impl fmt::Debug for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Process => "Process",
            Self::Absent => "Absent",
            Self::Replaced(_) => "Replaced(..)",
        })
    }
}

/// Where the standard output or the standard error goes.
pub enum Output {
    /// The process's own stream.
    Process,
    /// The process has no such stream: acquiring it is refused.
    Absent,
    /// A replacement, for the tests.
    Replaced(Box<dyn OutputSink>),
}

impl fmt::Debug for Output {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Process => "Process",
            Self::Absent => "Absent",
            Self::Replaced(_) => "Replaced(..)",
        })
    }
}

/// The environment variables an `EnvReadVar` grant reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Variables {
    /// The process's environment.
    Process,
    /// A fixed set, for the tests and the dual run: each name and its value;
    /// the first entry of a name wins.
    Fixed(Vec<(OsString, OsString)>),
}

/// What a [`SystemDriver`] acts on: the program's arguments, and the three
/// streams and the variable set, each the process's own unless replaced.
#[derive(Debug)]
pub struct SystemInputs {
    /// The program's arguments, in order, without the executable's path.
    pub arguments: Vec<OsString>,
    /// The standard input.
    pub stdin: Input,
    /// The standard output.
    pub stdout: Output,
    /// The standard error.
    pub stderr: Output,
    /// The environment variables.
    pub variables: Variables,
}

impl SystemInputs {
    /// The process's streams and environment, with `arguments` as the
    /// program's arguments.
    #[must_use]
    pub const fn process(arguments: Vec<OsString>) -> Self {
        Self {
            arguments,
            stdin: Input::Process,
            stdout: Output::Process,
            stderr: Output::Process,
            variables: Variables::Process,
        }
    }
}

/// One standard stream: the process's own until it is first acquired, then
/// the opened stream.
enum Stream<T> {
    /// Not opened yet: the process's own stream when it is acquired.
    Process,
    /// The process has no such stream.
    Absent,
    /// Opened, or given as a replacement.
    Open(T),
}

/// What one acquired resource is.
#[derive(Clone, Debug)]
enum Held {
    Stdin,
    Stdout,
    Stderr,
    Variables,
    Arguments,
    Wall,
    Monotonic(clock::Monotonic),
    Random,
    /// A held directory: the root handle, closed when the resource is
    /// released.
    Directory(Rc<ScopedDir>),
}

/// The driver over the process for the nine providers of the streams, the
/// environment, the arguments, the clocks, random, and the directories.
pub struct SystemDriver {
    arguments: Vec<OsString>,
    variables: Variables,
    stdin: Stream<streams::Reader>,
    stdout: Stream<Box<dyn OutputSink>>,
    stderr: Stream<Box<dyn OutputSink>>,
    directories: directory::Directories,
    held: BTreeMap<ResourceId, Held>,
    next: u64,
}

impl fmt::Debug for SystemDriver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SystemDriver")
            .field("arguments", &self.arguments.len())
            .field("held", &self.held)
            .field("file_reads", &self.directories.reads)
            .finish_non_exhaustive()
    }
}

fn output(output: Output) -> Stream<Box<dyn OutputSink>> {
    match output {
        Output::Process => Stream::Process,
        Output::Absent => Stream::Absent,
        Output::Replaced(sink) => Stream::Open(sink),
    }
}

/// Opens a standard output or error the first time it is acquired.
fn open_output(
    slot: &mut Stream<Box<dyn OutputSink>>,
    which: streams::Which,
) -> Result<(), Acquire> {
    match slot {
        Stream::Open(_) => Ok(()),
        Stream::Absent => Err(Acquire::Refused),
        Stream::Process => match streams::process_output(which) {
            streams::Opened::Open(sink) => {
                *slot = Stream::Open(sink);
                Ok(())
            }
            streams::Opened::Missing => Err(Acquire::Refused),
            streams::Opened::Failed(why) => Err(Acquire::Failed(why)),
        },
    }
}

impl SystemDriver {
    /// A driver over `inputs`. It reads nothing until a resource is
    /// acquired.
    #[must_use]
    pub fn new(inputs: SystemInputs) -> Self {
        let stdin = match inputs.stdin {
            Input::Process => Stream::Process,
            Input::Absent => Stream::Absent,
            Input::Replaced(source) => Stream::Open(streams::Reader::new(source)),
        };
        Self {
            arguments: inputs.arguments,
            variables: inputs.variables,
            stdin,
            stdout: output(inputs.stdout),
            stderr: output(inputs.stderr),
            directories: directory::Directories::default(),
            held: BTreeMap::new(),
            next: 0,
        }
    }

    /// Holds `held` under a new resource; `None` when the identities are
    /// exhausted.
    fn hold(&mut self, held: Held) -> Option<ResourceId> {
        self.next = self.next.checked_add(1)?;
        let resource = ResourceId(self.next);
        self.held.insert(resource, held);
        Some(resource)
    }

    /// Opens the standard input the first time it is acquired.
    fn open_stdin(&mut self) -> Result<(), Acquire> {
        match &self.stdin {
            Stream::Open(_) => Ok(()),
            Stream::Absent => Err(Acquire::Refused),
            Stream::Process => match streams::process_stdin() {
                streams::Opened::Open(source) => {
                    self.stdin = Stream::Open(streams::Reader::new(source));
                    Ok(())
                }
                streams::Opened::Missing => Err(Acquire::Refused),
                streams::Opened::Failed(why) => Err(Acquire::Failed(why)),
            },
        }
    }

    /// The adapter call for a resource held as `held`, other than a
    /// monotonic clock.
    fn answer(
        &mut self,
        held: &Held,
        action: &Action<'_>,
        gate: &mut dyn Gate,
    ) -> Answer {
        match (held, action.op) {
            (Held::Stdin, HostOp::IoStdinText | HostOp::IoStdinBytes) => {
                match &mut self.stdin {
                    Stream::Open(reader) => reader.read(action, gate),
                    Stream::Process | Stream::Absent => {
                        Answer::NotSubmitted("unsupported")
                    }
                }
            }
            (Held::Stdout, HostOp::IoStdoutText | HostOp::IoStdoutBytes) => {
                write(&mut self.stdout, action)
            }
            (Held::Stderr, HostOp::IoStderrText | HostOp::IoStderrBytes) => {
                write(&mut self.stderr, action)
            }
            (Held::Variables, HostOp::EnvReadVar) => {
                env::read_var(&self.variables, action.operand, gate)
            }
            (Held::Arguments, HostOp::EnvReadArgs) => {
                env::read_args(&self.arguments, gate)
            }
            (Held::Wall, HostOp::TimeNowWall) => clock::wall(),
            (Held::Random, HostOp::RandomGenerateBytes) => {
                random::generate(action.operand)
            }
            (Held::Directory(root), _) => self.directories.call(root, action, gate),
            _ => Answer::NotSubmitted("unsupported"),
        }
    }
}

/// Test hooks of the directory driver.
#[cfg(feature = "testing")]
impl SystemDriver {
    /// From now on every call of the directory adapter that reports the
    /// names the filesystem gives, the acquisition of a root included,
    /// answers as a volume that does not report normalized names: the
    /// adapter's `Unsupported`, before it opens anything.
    pub fn fail_name_queries_for_tests(&mut self) {
        self.directories.names_unsupported = true;
    }

    /// The number of times the directory driver read a file's contents.
    #[must_use]
    pub const fn file_reads_for_tests(&self) -> u64 {
        self.directories.reads
    }
}

/// A write to an opened standard output or error.
fn write(slot: &mut Stream<Box<dyn OutputSink>>, action: &Action<'_>) -> Answer {
    match slot {
        Stream::Open(sink) => streams::write(sink.as_mut(), action.operand),
        Stream::Process | Stream::Absent => Answer::NotSubmitted("unsupported"),
    }
}

/// What an adapter answers, before it is a [`Completion`] of the row's error
/// type.
#[derive(Debug)]
enum Answer {
    /// The gate refused.
    Refused,
    /// Known not submitted, with a refusal case.
    NotSubmitted(&'static str),
    /// Completed with a value.
    Ok(HostOk, Progress),
    /// Completed with a case of the row's error type.
    Err(&'static str, Progress),
}

/// The progress of an adapter that took `input` bytes and sent `output`.
const fn moved(input: u64, output: u64) -> Progress {
    Progress {
        input: Measured::Known(input),
        output: Measured::Known(output),
    }
}

/// A byte count as a `u64`.
fn length(bytes: usize) -> u64 {
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

impl Driver for SystemDriver {
    fn identity(&self, provider: Provider) -> Option<DriverIdentity> {
        if provider == Provider::FsDirectory && !directory::SUPPORTED {
            return None;
        }
        Some(DriverIdentity {
            provider: provider.atom().to_owned(),
            version: DRIVER_VERSION.to_owned(),
        })
    }

    fn acquire(&mut self, provider: Provider, resource: &Resolved) -> Acquire {
        let opened = match (provider, resource) {
            (Provider::IoStdin, Resolved::ProcessStream) => {
                self.open_stdin().map(|()| Held::Stdin)
            }
            (Provider::IoStdout, Resolved::ProcessStream) => {
                open_output(&mut self.stdout, streams::Which::Stdout)
                    .map(|()| Held::Stdout)
            }
            (Provider::IoStderr, Resolved::ProcessStream) => {
                open_output(&mut self.stderr, streams::Which::Stderr)
                    .map(|()| Held::Stderr)
            }
            (Provider::EnvVars, Resolved::VariableSet { .. }) => Ok(Held::Variables),
            (Provider::EnvArgs, Resolved::ProcessArguments) => Ok(Held::Arguments),
            (Provider::TimeWall, Resolved::SystemClock) => Ok(Held::Wall),
            (Provider::TimeMonotonic, Resolved::SystemClock) => {
                Ok(Held::Monotonic(clock::Monotonic::start()))
            }
            (Provider::Random, Resolved::SystemRandom) => random::probe()
                .map(|()| Held::Random)
                .map_err(Acquire::Failed),
            (Provider::FsDirectory, Resolved::Directory { path }) => self
                .directories
                .open_root(path)
                .map(|root| Held::Directory(Rc::new(root))),
            (provider, _) => Err(Acquire::Failed(format!(
                "the system driver does not issue `{}` for this resource",
                provider.atom()
            ))),
        };
        let held = match opened {
            Ok(held) => held,
            Err(answer) => return answer,
        };
        match self.hold(held) {
            Some(resource) => Acquire::Acquired(resource),
            None => {
                Acquire::Failed("the driver has no resource identity left".to_owned())
            }
        }
    }

    fn release(&mut self, resource: ResourceId) {
        self.held.remove(&resource);
    }

    fn call(
        &mut self,
        resource: ResourceId,
        action: &Action<'_>,
        gate: &mut dyn Gate,
    ) -> Completion {
        let answer = match self.held.get_mut(&resource) {
            None => Answer::NotSubmitted("unsupported"),
            Some(Held::Monotonic(clock)) => match action.op {
                HostOp::TimeNowMonotonic => clock.read(),
                _ => Answer::NotSubmitted("unsupported"),
            },
            Some(held) => {
                let held = held.clone();
                self.answer(&held, action, gate)
            }
        };
        let error = action.op.row().error;
        // Every case an adapter names is a refusal case of every host error
        // type or a case of the row's own type; `unsupported` is a case of
        // all five.
        let case = |name: &str| {
            HostErrorCase::new(error, name)
                .or_else(|| HostErrorCase::new(error, "unsupported"))
        };
        let completion = match answer {
            Answer::Refused => Some(Completion::Refused),
            Answer::NotSubmitted(name) => case(name).map(Completion::NotSubmitted),
            Answer::Ok(value, progress) => Some(Completion::Completed {
                reply: HostReply::Ok(value),
                progress,
            }),
            Answer::Err(name, progress) => {
                case(name).map(|case| Completion::Completed {
                    reply: HostReply::Err(case),
                    progress,
                })
            }
        };
        // Unreachable: every error type has `unsupported`. The broker reports
        // a refusal its gate did not make as a defect of the driver.
        completion.unwrap_or(Completion::Refused)
    }
}
