//! The system drivers: the standard streams, the environment, the arguments,
//! the clocks, and random, through `Context` over a `SystemDriver` (M2 step
//! 30).
//!
//! Every row builds the driver from `SystemInputs` with in-memory streams, a
//! fixed variable set, and fixed arguments, so it runs the same on every
//! platform; the two clock rows and the random rows read the process.
//!
//! [`Watched`] is the driver with a recorder on top: it counts the calls
//! that reach the driver and records what each call's gate was shown and
//! what it reserved.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::rc::Rc;

use vibra_diagnostics::ByteSpan;
#[cfg(not(windows))]
use vibra_diagnostics::DiagnosticCode;
use vibra_host::broker::{Context, RootGrant};
use vibra_host::driver::{
    Acquire, Action, Completion, Driver, DriverIdentity, Found, Gate, Refused,
    Reserved, Resolved, ResourceId,
};
use vibra_host::policy;
use vibra_host::recipe::{
    self, BindingPlan, Leaf, LeafName, Limits, Provider, Resource,
};
#[cfg(not(windows))]
use vibra_host::startup::AdmissionRefusal;
use vibra_host::startup::{EntryLeaf, EntryShape, admit};
use vibra_host::system::{
    Input, InputSource, Output, OutputSink, SystemDriver, SystemInputs, Variables,
    WriteFailure,
};
use vibra_host::trace::GrantOrigin;
use vibra_ir::Row;
use vibra_ir::authority::{
    GrantToken, HostArgument, HostContext, HostFault, HostOk, HostReply, HostRequest,
};
use vibra_ir::external::host::{HostInterface, HostOp};

use crate::support::{VOID, bytes, counters, entity, err, text};
use crate::trace::header;

// --- In-memory streams ---------------------------------------------------

/// What a test input holds and what was asked of it.
#[derive(Default)]
struct Held {
    bytes: Vec<u8>,
    at: usize,
    /// Whether the input declares the size of what remains.
    sized: bool,
    /// After this many bytes, a read fails.
    fail_after: Option<usize>,
    /// The number of `read` calls.
    reads: usize,
}

#[derive(Clone, Default)]
struct Source(Rc<RefCell<Held>>);

impl Source {
    fn pipe(bytes: &[u8]) -> Self {
        Self(Rc::new(RefCell::new(Held {
            bytes: bytes.to_vec(),
            ..Held::default()
        })))
    }

    fn file(bytes: &[u8]) -> Self {
        let source = Self::pipe(bytes);
        source.0.borrow_mut().sized = true;
        source
    }

    fn reads(&self) -> usize {
        self.0.borrow().reads
    }
}

impl InputSource for Source {
    fn remaining(&mut self) -> Option<u64> {
        let held = self.0.borrow();
        held.sized.then(|| (held.bytes.len() - held.at) as u64)
    }

    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut held = self.0.borrow_mut();
        held.reads += 1;
        let mut end = held.bytes.len();
        if let Some(fail) = held.fail_after {
            if held.at >= fail {
                return Err(io::Error::other("the device failed"));
            }
            end = end.min(fail);
        }
        let count = buffer.len().min(end - held.at);
        let at = held.at;
        buffer[..count].copy_from_slice(&held.bytes[at..at + count]);
        held.at += count;
        Ok(count)
    }

    fn unread(&mut self, count: u64) -> io::Result<()> {
        let mut held = self.0.borrow_mut();
        if held.sized {
            held.at -= usize::try_from(count).expect("a small count");
        }
        Ok(())
    }
}

/// What a test sink does with one write.
#[derive(Clone, Copy, Debug)]
enum Behaviour {
    /// Delivers the whole value.
    Accept,
    /// A broken pipe, nothing written.
    Broken,
    /// Writes this many bytes and fails, reporting them.
    Partial(usize),
    /// Writes this many bytes and fails, reporting no count.
    Unknown(usize),
}

#[derive(Default)]
struct SinkState {
    held: Vec<u8>,
    script: VecDeque<Behaviour>,
}

#[derive(Clone, Default)]
struct Sink(Rc<RefCell<SinkState>>);

impl Sink {
    fn scripted(script: &[Behaviour]) -> Self {
        let sink = Self::default();
        sink.0.borrow_mut().script = script.iter().copied().collect();
        sink
    }

    fn held(&self) -> Vec<u8> {
        self.0.borrow().held.clone()
    }
}

impl OutputSink for Sink {
    fn write(&mut self, bytes: &[u8]) -> Result<(), WriteFailure> {
        let mut state = self.0.borrow_mut();
        match state.script.pop_front().unwrap_or(Behaviour::Accept) {
            Behaviour::Accept => {
                state.held.extend_from_slice(bytes);
                Ok(())
            }
            Behaviour::Broken => Err(WriteFailure {
                kind: io::ErrorKind::BrokenPipe,
                written: Some(0),
            }),
            Behaviour::Partial(count) => {
                state.held.extend_from_slice(&bytes[..count]);
                Err(WriteFailure {
                    kind: io::ErrorKind::Other,
                    written: Some(count as u64),
                })
            }
            Behaviour::Unknown(count) => {
                state.held.extend_from_slice(&bytes[..count]);
                Err(WriteFailure {
                    kind: io::ErrorKind::Other,
                    written: None,
                })
            }
        }
    }
}

// --- The watched driver --------------------------------------------------

/// What one call's gate was shown and what it answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Asked {
    size: Option<u64>,
    reserved: Option<u64>,
}

#[derive(Default)]
struct Watch {
    acquires: usize,
    calls: usize,
    asked: Vec<Asked>,
}

struct Watched {
    driver: SystemDriver,
    watch: Rc<RefCell<Watch>>,
}

struct Recording<'g> {
    gate: &'g mut dyn Gate,
    watch: Rc<RefCell<Watch>>,
}

impl Gate for Recording<'_> {
    fn admit(&mut self, found: &Found<'_>) -> Result<Reserved, Refused> {
        let answer = self.gate.admit(found);
        self.watch.borrow_mut().asked.push(Asked {
            size: found.size,
            reserved: answer.ok().map(|reserved| reserved.input),
        });
        answer
    }

    fn admit_error(&mut self, names: &[String]) -> Result<(), Refused> {
        self.gate.admit_error(names)
    }
}

impl Driver for Watched {
    fn identity(&self, provider: Provider) -> Option<DriverIdentity> {
        self.driver.identity(provider)
    }

    fn acquire(&mut self, provider: Provider, resource: &Resolved) -> Acquire {
        self.watch.borrow_mut().acquires += 1;
        self.driver.acquire(provider, resource)
    }

    fn release(&mut self, resource: ResourceId) -> Result<(), String> {
        self.driver.release(resource)
    }

    fn call(
        &mut self,
        resource: ResourceId,
        action: &Action<'_>,
        gate: &mut dyn Gate,
    ) -> Completion {
        self.watch.borrow_mut().calls += 1;
        let mut gate = Recording {
            gate,
            watch: Rc::clone(&self.watch),
        };
        self.driver.call(resource, action, &mut gate)
    }
}

// --- Building and running ------------------------------------------------

fn inputs() -> SystemInputs {
    SystemInputs {
        arguments: Vec::new(),
        stdin: Input::Absent,
        stdout: Output::Absent,
        stderr: Output::Absent,
        variables: Variables::Fixed(Vec::new()),
    }
}

fn with_stdin(source: &Source) -> SystemInputs {
    SystemInputs {
        stdin: Input::Replaced(Box::new(source.clone())),
        ..inputs()
    }
}

fn with_stdout(sink: &Sink) -> SystemInputs {
    SystemInputs {
        stdout: Output::Replaced(Box::new(sink.clone())),
        ..inputs()
    }
}

fn watched(inputs: SystemInputs) -> (Watched, Rc<RefCell<Watch>>) {
    let watch = Rc::<RefCell<Watch>>::default();
    let driver = Watched {
        driver: SystemDriver::new(inputs),
        watch: Rc::clone(&watch),
    };
    (driver, watch)
}

fn resolved(provider: Provider) -> Resolved {
    match provider {
        Provider::IoStdin | Provider::IoStdout | Provider::IoStderr => {
            Resolved::ProcessStream
        }
        Provider::EnvVars => Resolved::VariableSet { names: Vec::new() },
        Provider::EnvArgs => Resolved::ProcessArguments,
        Provider::TimeWall | Provider::TimeMonotonic => Resolved::SystemClock,
        Provider::Random => Resolved::SystemRandom,
        Provider::FsDirectory => Resolved::Directory {
            path: "/".to_owned(),
        },
    }
}

fn acquire(driver: &mut impl Driver, provider: Provider) -> ResourceId {
    match driver.acquire(provider, &resolved(provider)) {
        Acquire::Acquired(resource) => resource,
        other => panic!("{} not acquired: {other:?}", provider.atom()),
    }
}

fn limits(input: Option<u64>, output: Option<u64>) -> Limits {
    Limits {
        invocations: None,
        input_bytes: input,
        output_bytes: output,
    }
}

fn grant(
    cx: &mut Context<'_>,
    interface: HostInterface,
    resource: ResourceId,
    limits: Limits,
) -> GrantToken {
    cx.root_for_tests(RootGrant {
        interface,
        resource,
        origin: GrantOrigin::Binding {
            leaf: LeafName::new("@leaf"),
        },
        policy: None,
        variables: None,
        limits,
    })
    .expect("a root grant")
}

/// Calls with `room`, and commits: the reply, and whether an event was due.
fn call_in(
    cx: &mut Context<'_>,
    op: HostOp,
    grant: GrantToken,
    arguments: &[HostArgument<'_>],
    room: u64,
) -> Result<(HostReply, bool), HostFault> {
    let completion = cx.host(HostRequest {
        op,
        grant,
        arguments,
        invocation: 1,
        parent: None,
        origin: None,
        room,
    })?;
    let due = completion.receipt.is_some();
    if let Some(receipt) = completion.receipt {
        cx.commit(receipt).expect("the pending receipt");
    }
    Ok((completion.reply, due))
}

fn call(
    cx: &mut Context<'_>,
    op: HostOp,
    grant: GrantToken,
    arguments: &[HostArgument<'_>],
) -> (HostReply, bool) {
    call_in(cx, op, grant, arguments, u64::MAX).expect("no fault")
}

fn read(cx: &mut Context<'_>, op: HostOp, grant: GrantToken, max: u64) -> HostReply {
    call(cx, op, grant, &[HostArgument::Count(max)]).0
}

/// Runs `body` on a context over a driver whose standard input is
/// `source`, with the acquired standard input; what each call's gate was
/// shown and reserved.
fn on_stdin(
    source: &Source,
    body: impl FnOnce(&mut Context<'_>, ResourceId),
) -> Vec<Asked> {
    let (mut driver, watch) = watched(with_stdin(source));
    let stdin = acquire(&mut driver, Provider::IoStdin);
    {
        let mut cx = Context::new(&mut driver, header());
        body(&mut cx, stdin);
    }
    watch.borrow().asked.clone()
}

// --- Standard input ------------------------------------------------------

/// Row "stdin text".
#[test]
fn stdin_text() {
    let source = Source::pipe(b"x\ny");
    on_stdin(&source, |cx, stdin| {
        let input = grant(cx, HostInterface::StdinText, stdin, Limits::default());
        assert_eq!(read(cx, HostOp::IoStdinText, input, 16), text("x\ny"));
        assert_eq!(read(cx, HostOp::IoStdinText, input, 16), text(""));
    });
}

/// Row "stdin invalid text".
#[test]
fn stdin_invalid_text() {
    let source = Source::pipe(&[255]);
    on_stdin(&source, |cx, stdin| {
        let input = grant(cx, HostInterface::StdinText, stdin, limits(Some(16), None));
        assert_eq!(
            call(cx, HostOp::IoStdinText, input, &[HostArgument::Count(16)]),
            (err(HostOp::IoStdinText, "invalidText"), true)
        );
        // The window was consumed and charged (Q28).
        assert_eq!(counters(cx, input).input_bytes().used(), 1);
        assert_eq!(read(cx, HostOp::IoStdinText, input, 16), text(""));
    });
}

/// Row "stdin read error" (Q28): the three bytes the reader yielded before
/// it failed are charged, so the next read is reserved at two.
#[test]
fn stdin_read_error() {
    let source = Source::pipe(b"abcde");
    source.0.borrow_mut().fail_after = Some(3);
    let asked = on_stdin(&source, |cx, stdin| {
        let input = grant(cx, HostInterface::StdinBytes, stdin, limits(Some(5), None));
        assert_eq!(
            call(cx, HostOp::IoStdinBytes, input, &[HostArgument::Count(5)]),
            (err(HostOp::IoStdinBytes, "other"), true)
        );
        assert_eq!(counters(cx, input).input_bytes().used(), 3);
        let _ = read(cx, HostOp::IoStdinBytes, input, 5);
    });
    assert_eq!(
        asked,
        [
            Asked {
                size: None,
                reserved: Some(5)
            },
            Asked {
                size: None,
                reserved: Some(2)
            },
        ]
    );
}

/// Row "stdin past the limit, a pipe" (Q10).
#[test]
fn stdin_past_the_limit_a_pipe() {
    let source = Source::pipe(b"abc");
    on_stdin(&source, |cx, stdin| {
        let bounded =
            grant(cx, HostInterface::StdinBytes, stdin, limits(Some(2), None));
        assert_eq!(read(cx, HostOp::IoStdinBytes, bounded, 3), bytes(b"ab"));
        let reads = source.reads();
        assert_eq!(
            call(cx, HostOp::IoStdinBytes, bounded, &[HostArgument::Count(3)]),
            (err(HostOp::IoStdinBytes, "limitExceeded"), false)
        );
        assert_eq!(source.reads(), reads, "the reader was not read again");
        let unbounded = grant(cx, HostInterface::StdinBytes, stdin, Limits::default());
        assert_eq!(read(cx, HostOp::IoStdinBytes, unbounded, 3), bytes(b"c"));
    });
}

/// Row "stdin within the limit, a pipe" (Q10).
#[test]
fn stdin_within_the_limit_a_pipe() {
    let source = Source::pipe(b"abc");
    on_stdin(&source, |cx, stdin| {
        let input = grant(cx, HostInterface::StdinBytes, stdin, limits(Some(3), None));
        assert_eq!(read(cx, HostOp::IoStdinBytes, input, 3), bytes(b"abc"));
    });
}

/// Row "stdin by count, a pipe" (Q10).
#[test]
fn stdin_by_count_a_pipe() {
    let source = Source::pipe(b"abc");
    on_stdin(&source, |cx, stdin| {
        let input = grant(cx, HostInterface::StdinBytes, stdin, Limits::default());
        assert_eq!(read(cx, HostOp::IoStdinBytes, input, 2), bytes(b"ab"));
        assert_eq!(read(cx, HostOp::IoStdinBytes, input, 2), bytes(b"c"));
        assert_eq!(read(cx, HostOp::IoStdinBytes, input, 2), bytes(b""));
    });
}

/// F5 of the drivers review, 06 "Standard input": when the grant is
/// released, a regular file is positioned just after the bytes delivered;
/// a pipe cannot take back the bytes the adapter kept, an accepted limit.
#[test]
fn stdin_release_gives_back_the_kept_bytes() {
    for (source, at) in [
        (Source::file("abc\u{20AC}d".as_bytes()), 3),
        (Source::pipe("abc\u{20AC}d".as_bytes()), 5),
    ] {
        on_stdin(&source, |cx, stdin| {
            let input = grant(cx, HostInterface::StdinText, stdin, Limits::default());
            assert_eq!(read(cx, HostOp::IoStdinText, input, 4), text("abc"));
            assert_eq!(cx.release_for_tests(stdin), Ok(()));
        });
        assert_eq!(source.0.borrow().at, at, "the position after release");
    }
}

/// Row "stdin, a file of known size" (Q10).
#[test]
fn stdin_a_file_of_known_size() {
    let source = Source::file(b"abc");
    let asked = on_stdin(&source, |cx, stdin| {
        let input = grant(cx, HostInterface::StdinBytes, stdin, limits(Some(3), None));
        assert_eq!(read(cx, HostOp::IoStdinBytes, input, 10), bytes(b"abc"));
    });
    assert_eq!(
        asked,
        [Asked {
            size: Some(3),
            reserved: Some(3)
        }]
    );
}

/// Row "stdin, an empty file at zero" (Q10).
#[test]
fn stdin_an_empty_file_at_zero() {
    let source = Source::file(b"");
    on_stdin(&source, |cx, stdin| {
        let input = grant(cx, HostInterface::StdinBytes, stdin, limits(Some(0), None));
        assert_eq!(read(cx, HostOp::IoStdinBytes, input, 1), bytes(b""));
    });
}

/// Row "stdin text below a scalar" (Q30).
#[test]
fn stdin_text_below_a_scalar() {
    let source = Source::pipe(b"abc");
    let asked = on_stdin(&source, |cx, stdin| {
        let input = grant(cx, HostInterface::StdinText, stdin, Limits::default());
        assert_eq!(
            call(cx, HostOp::IoStdinText, input, &[HostArgument::Count(3)]),
            (err(HostOp::IoStdinText, "unsupported"), false)
        );
    });
    assert_eq!(source.reads(), 0, "the reader was not read");
    assert!(asked.is_empty(), "nor was the gate asked");
}

/// Row "stdin text, unsized, below one scalar" (Q31).
#[test]
fn stdin_text_unsized_below_one_scalar() {
    let source = Source::pipe(b"abc");
    on_stdin(&source, |cx, stdin| {
        let bounded = grant(cx, HostInterface::StdinText, stdin, limits(Some(2), None));
        assert_eq!(
            call(cx, HostOp::IoStdinText, bounded, &[HostArgument::Count(16)]),
            (err(HostOp::IoStdinText, "limitExceeded"), false)
        );
        assert_eq!(source.reads(), 0, "the reader was not read");
        let unbounded = grant(cx, HostInterface::StdinText, stdin, Limits::default());
        assert_eq!(read(cx, HostOp::IoStdinText, unbounded, 16), text("abc"));
    });
}

/// Row "stdin text at a scalar boundary" (Q30), from a pipe and from a file.
#[test]
fn stdin_text_at_a_scalar_boundary() {
    for source in [
        Source::pipe("\u{20AC}a".as_bytes()),
        Source::file("\u{20AC}a".as_bytes()),
    ] {
        on_stdin(&source, |cx, stdin| {
            let input = grant(cx, HostInterface::StdinText, stdin, Limits::default());
            assert_eq!(read(cx, HostOp::IoStdinText, input, 4), text("\u{20AC}a"));
            assert_eq!(read(cx, HostOp::IoStdinText, input, 4), text(""));
        });
    }
}

/// 06 Standard input: a window that ends inside a scalar before the end of
/// input returns the whole scalars before it and consumes exactly their
/// bytes; the next read starts with the rest of the scalar.
#[test]
fn stdin_text_window_inside_a_scalar() {
    for source in [
        Source::pipe("ab\u{20AC}".as_bytes()),
        Source::file("ab\u{20AC}".as_bytes()),
    ] {
        on_stdin(&source, |cx, stdin| {
            let input =
                grant(cx, HostInterface::StdinText, stdin, limits(Some(100), None));
            assert_eq!(read(cx, HostOp::IoStdinText, input, 4), text("ab"));
            assert_eq!(counters(cx, input).input_bytes().used(), 2);
            assert_eq!(read(cx, HostOp::IoStdinText, input, 4), text("\u{20AC}"));
            assert_eq!(counters(cx, input).input_bytes().used(), 5);
        });
    }
}

/// 06 Standard input: a scalar cut off by the end of input is
/// `invalidText`, and the whole window is consumed, whether the window ends
/// before the bound or exactly at it.
#[test]
fn stdin_text_scalar_cut_off_by_the_end() {
    let cut = [b'a', b'b', b'c', 0xE2, 0x82];
    for (source, max) in [
        (Source::pipe(&cut), 16),
        (Source::pipe(&cut), 5),
        (Source::file(&cut), 5),
    ] {
        on_stdin(&source, |cx, stdin| {
            let input =
                grant(cx, HostInterface::StdinText, stdin, limits(Some(100), None));
            assert_eq!(
                call(cx, HostOp::IoStdinText, input, &[HostArgument::Count(max)]),
                (err(HostOp::IoStdinText, "invalidText"), true),
                "max {max}"
            );
            assert_eq!(counters(cx, input).input_bytes().used(), 5);
            assert_eq!(read(cx, HostOp::IoStdinText, input, 16), text(""));
        });
    }
}

// --- Standard output and error -------------------------------------------

/// A grant of `interface` with `limits` on the replaced standard output.
fn on_stdout(sink: &Sink, body: impl FnOnce(&mut Context<'_>, ResourceId)) {
    let (mut driver, _) = watched(with_stdout(sink));
    let stdout = acquire(&mut driver, Provider::IoStdout);
    let mut cx = Context::new(&mut driver, header());
    body(&mut cx, stdout);
}

fn write_bytes(
    cx: &mut Context<'_>,
    grant: GrantToken,
    value: &[u8],
) -> (HostReply, bool) {
    call(
        cx,
        HostOp::IoStdoutBytes,
        grant,
        &[HostArgument::Bytes(value)],
    )
}

/// Row "stdout whole value".
#[test]
fn stdout_whole_value() {
    let sink = Sink::default();
    on_stdout(&sink, |cx, stdout| {
        let text = grant(cx, HostInterface::StdoutText, stdout, Limits::default());
        let raw = grant(cx, HostInterface::StdoutBytes, stdout, Limits::default());
        assert_eq!(
            call(cx, HostOp::IoStdoutText, text, &[HostArgument::Text("a")]),
            (VOID, true)
        );
        assert_eq!(write_bytes(cx, raw, &[98]), (VOID, true));
    });
    assert_eq!(sink.held(), b"ab");
}

/// Row "stdout closed".
#[test]
fn stdout_closed() {
    let sink = Sink::scripted(&[Behaviour::Broken]);
    on_stdout(&sink, |cx, stdout| {
        let out = grant(cx, HostInterface::StdoutBytes, stdout, Limits::default());
        assert_eq!(
            write_bytes(cx, out, b"abc"),
            (err(HostOp::IoStdoutBytes, "closed"), true),
            "a completed call"
        );
        assert_eq!(cx.trace().events.len(), 1);
    });
}

/// Row "stdout partial failure, known": the two bytes the system reported
/// are charged, and the remainder of the reservation refunded.
#[test]
fn stdout_partial_failure_known() {
    let sink = Sink::scripted(&[Behaviour::Partial(2)]);
    on_stdout(&sink, |cx, stdout| {
        let out = grant(
            cx,
            HostInterface::StdoutBytes,
            stdout,
            limits(None, Some(10)),
        );
        assert_eq!(
            write_bytes(cx, out, b"hello"),
            (err(HostOp::IoStdoutBytes, "other"), true)
        );
        assert_eq!(counters(cx, out).output_bytes().used(), 2);
        assert_eq!(write_bytes(cx, out, b"12345678"), (VOID, true));
        assert_eq!(
            write_bytes(cx, out, b"x"),
            (err(HostOp::IoStdoutBytes, "limitExceeded"), false)
        );
    });
    assert_eq!(sink.held(), b"he12345678");
}

/// Row "stdout partial failure, unknown": a count the system did not report
/// charges the whole reservation.
#[test]
fn stdout_partial_failure_unknown() {
    let sink = Sink::scripted(&[Behaviour::Unknown(1)]);
    on_stdout(&sink, |cx, stdout| {
        let out = grant(
            cx,
            HostInterface::StdoutBytes,
            stdout,
            limits(None, Some(10)),
        );
        assert_eq!(
            write_bytes(cx, out, b"hello"),
            (err(HostOp::IoStdoutBytes, "other"), true)
        );
        assert_eq!(counters(cx, out).output_bytes().used(), 5);
        assert_eq!(
            write_bytes(cx, out, b"sixsix"),
            (err(HostOp::IoStdoutBytes, "limitExceeded"), false)
        );
        assert_eq!(write_bytes(cx, out, b"fives"), (VOID, true));
    });
}

/// Row "stream absent", for each of the three streams.
#[test]
fn stream_absent() {
    let mut driver = SystemDriver::new(inputs());
    for provider in [Provider::IoStdin, Provider::IoStdout, Provider::IoStderr] {
        assert_eq!(
            driver.acquire(provider, &Resolved::ProcessStream),
            Acquire::Refused,
            "{}",
            provider.atom()
        );
    }
}

// --- Environment and arguments -------------------------------------------

#[cfg(unix)]
fn not_unicode() -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(vec![b'f', 0xFF])
}

#[cfg(windows)]
fn not_unicode() -> OsString {
    use std::os::windows::ffi::OsStringExt;
    OsString::from_wide(&[u16::from(b'f'), 0xD800])
}

fn variables(entries: Vec<(OsString, OsString)>) -> SystemInputs {
    SystemInputs {
        variables: Variables::Fixed(entries),
        ..inputs()
    }
}

fn read_var(inputs: SystemInputs, name: &str) -> (HostReply, bool) {
    let mut driver = SystemDriver::new(inputs);
    let vars = acquire(&mut driver, Provider::EnvVars);
    let mut cx = Context::new(&mut driver, header());
    let grant = grant(&mut cx, HostInterface::EnvReadVar, vars, Limits::default());
    call(
        &mut cx,
        HostOp::EnvReadVar,
        grant,
        &[HostArgument::Text(name)],
    )
}

fn os(text: &str) -> OsString {
    OsString::from(text)
}

/// Row "variable present".
#[test]
fn variable_present() {
    assert_eq!(
        read_var(variables(vec![(os("HOME"), os("/h"))]), "HOME"),
        (text("/h"), true)
    );
}

/// Row "variable absent"; a name that differs only in case is another name.
#[test]
fn variable_absent() {
    let set = || variables(vec![(os("HOME"), os("/h"))]);
    for name in ["NOPE", "home"] {
        assert_eq!(
            read_var(set(), name),
            (err(HostOp::EnvReadVar, "notPresent"), true),
            "{name}"
        );
    }
}

/// Row "variable name unholdable": even a set that holds the name does not
/// answer it.
#[test]
fn variable_name_unholdable() {
    for name in ["", "A=B", "A\0B"] {
        let set = variables(vec![(os(name), os("x"))]);
        assert_eq!(
            read_var(set, name),
            (err(HostOp::EnvReadVar, "notPresent"), true),
            "{name:?}"
        );
    }
}

/// Row "variable not Unicode".
#[test]
fn variable_not_unicode() {
    assert_eq!(
        read_var(variables(vec![(os("BAD"), not_unicode())]), "BAD"),
        (err(HostOp::EnvReadVar, "notUnicode"), true)
    );
}

fn read_args(arguments: Vec<OsString>) -> (HostReply, bool, u64) {
    let mut driver = SystemDriver::new(SystemInputs {
        arguments,
        ..inputs()
    });
    let args = acquire(&mut driver, Provider::EnvArgs);
    let mut cx = Context::new(&mut driver, header());
    let grant = grant(&mut cx, HostInterface::EnvReadArgs, args, Limits::default());
    let (reply, due) = call(&mut cx, HostOp::EnvReadArgs, grant, &[]);
    (reply, due, counters(&cx, grant).input_bytes().used())
}

/// Row "arguments exact": in order, nothing dropped or parsed; the charge is
/// the sum of the UTF-8 lengths.
#[test]
fn arguments_exact() {
    let arguments = ["a", "--", "-b", ""];
    assert_eq!(
        read_args(arguments.iter().map(|argument| os(argument)).collect()),
        (
            HostReply::Ok(HostOk::Names(
                arguments
                    .iter()
                    .map(|argument| (*argument).to_owned())
                    .collect()
            )),
            true,
            5
        )
    );
}

/// Row "argument not Unicode".
#[test]
fn argument_not_unicode() {
    assert_eq!(
        read_args(vec![os("a"), not_unicode()]),
        (err(HostOp::EnvReadArgs, "notUnicode"), true, 0)
    );
}

// --- Clocks and random ---------------------------------------------------

fn clock_reads(
    provider: Provider,
    interface: HostInterface,
    op: HostOp,
    times: usize,
) -> Vec<HostReply> {
    let mut driver = SystemDriver::new(inputs());
    let clock = acquire(&mut driver, provider);
    let mut cx = Context::new(&mut driver, header());
    let grant = grant(&mut cx, interface, clock, Limits::default());
    (0..times)
        .map(|_| call(&mut cx, op, grant, &[]).0)
        .collect()
}

/// Row "wall clock, process".
#[test]
fn wall_clock_process() {
    // 2020-01-01 and 2100-01-01, 00:00 UTC.
    const AFTER: i64 = 1_577_836_800_000_000_000;
    const BEFORE: i64 = 4_102_444_800_000_000_000;
    let replies = clock_reads(
        Provider::TimeWall,
        HostInterface::WallClock,
        HostOp::TimeNowWall,
        1,
    );
    match replies.as_slice() {
        [HostReply::Ok(HostOk::I64(now))] => {
            assert!((AFTER..BEFORE).contains(now), "{now}");
        }
        other => panic!("not a time: {other:?}"),
    }
}

/// Row "monotonic clock, process".
#[test]
fn monotonic_clock_process() {
    let replies = clock_reads(
        Provider::TimeMonotonic,
        HostInterface::MonotonicClock,
        HostOp::TimeNowMonotonic,
        1_000,
    );
    let values = replies
        .iter()
        .map(|reply| match reply {
            HostReply::Ok(HostOk::U64(now)) => *now,
            other => panic!("not a time: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 1_000);
    assert!(values.windows(2).all(|pair| pair[0] <= pair[1]));
}

/// Calls `random.generateBytes` for each count on one grant with `limits`
/// and `room`; the results and the number of calls that reached the driver.
fn generate(
    counts: &[u64],
    limits: Limits,
    room: u64,
) -> (Vec<Result<HostReply, HostFault>>, usize) {
    let (mut driver, watch) = watched(inputs());
    let random = acquire(&mut driver, Provider::Random);
    let mut cx = Context::new(&mut driver, header());
    let grant = grant(&mut cx, HostInterface::RandomBytes, random, limits);
    let replies = counts
        .iter()
        .map(|count| {
            call_in(
                &mut cx,
                HostOp::RandomGenerateBytes,
                grant,
                &[HostArgument::Count(*count)],
                room,
            )
            .map(|(reply, _)| reply)
        })
        .collect();
    drop(cx);
    let calls = watch.borrow().calls;
    (replies, calls)
}

/// Row "random count".
#[test]
fn random_count() {
    let (replies, _) = generate(&[0, 1, 4096], Limits::default(), u64::MAX);
    let lengths = replies
        .iter()
        .map(|reply| match reply {
            Ok(HostReply::Ok(HostOk::Bytes(bytes))) => bytes.len(),
            other => panic!("not bytes: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(lengths, [0, 1, 4096]);
}

/// Row "random over the limit".
#[test]
fn random_over_the_limit() {
    let (replies, calls) = generate(&[9], limits(Some(8), None), u64::MAX);
    assert_eq!(
        replies,
        [Ok(err(HostOp::RandomGenerateBytes, "limitExceeded"))]
    );
    assert_eq!(calls, 0);
}

/// Row "random huge count": the generator was not called, so no buffer was
/// allocated.
#[test]
fn random_huge_count() {
    let (replies, calls) = generate(&[u64::MAX], Limits::default(), 1024);
    assert_eq!(replies, [Err(HostFault::MemoryExhausted)]);
    assert_eq!(calls, 0, "the driver was not called");
}

/// F6 of the drivers review, 06 "Memory": an allocation the operating system
/// refuses inside the driver, for a reply the broker admitted, is memory
/// exhaustion, never an error case of the operation, and no event claims
/// the call completed.
#[test]
fn random_allocation_refused_is_memory_exhaustion() {
    let (replies, calls) = generate(&[1 << 62], Limits::default(), u64::MAX);
    assert_eq!(replies, [Err(HostFault::MemoryExhausted)]);
    assert_eq!(calls, 1, "the driver was called and could not allocate");
}

// --- Identities and admission --------------------------------------------

const SYSTEM_PROVIDERS: [Provider; 8] = [
    Provider::IoStdin,
    Provider::IoStdout,
    Provider::IoStderr,
    Provider::EnvVars,
    Provider::EnvArgs,
    Provider::TimeWall,
    Provider::TimeMonotonic,
    Provider::Random,
];

/// Row "identities" (Q24); the directory provider has a driver on Windows
/// only (step 31).
#[test]
fn identities() {
    let driver = SystemDriver::new(inputs());
    for provider in SYSTEM_PROVIDERS {
        assert_eq!(
            driver.identity(provider),
            Some(DriverIdentity {
                provider: provider.atom().to_owned(),
                version: "0.0.2".to_owned(),
            })
        );
    }
    let directory = cfg!(windows).then(|| DriverIdentity {
        provider: "@host.fs.directory".to_owned(),
        version: "0.0.2".to_owned(),
    });
    assert_eq!(driver.identity(Provider::FsDirectory), directory);
}

/// Nothing is read at construction: a driver over the process's own
/// streams and environment is built and dropped, and acquires nothing.
#[test]
fn construction_reads_nothing() {
    let driver = SystemDriver::new(SystemInputs::process(vec![os("a")]));
    assert!(format!("{driver:?}").contains("held: {}"));
}

fn leaf(
    atom: &str,
    interface: HostInterface,
    provider: Provider,
    resource: Resource,
) -> Leaf {
    let policy = (provider == Provider::FsDirectory).then(|| {
        policy::decode(
            format!(
                "{{format: @scopePolicy.v1 family: @filesystem expression: {{kind: @fileRule \
                 root: {atom} prefix: [] depth: @tree actions: [@read] names: {{kind: \
                 @anyName}}}}}}"
            )
            .as_bytes(),
        )
        .expect("a policy")
    });
    let limits = Limits::default();
    let recipe_identity =
        recipe::recipe_identity(provider, &resource, policy.as_ref(), &limits, &[]);
    Leaf {
        name: LeafName::new(atom),
        interface,
        optional: false,
        provider,
        resource,
        policy,
        limits,
        after: Vec::new(),
        recipe_identity,
        span: ByteSpan::new(0, atom.len()),
    }
}

/// A target that binds a directory and the standard output, its entry
/// shape, and its row, which is also its ceiling.
fn directory_target() -> (BindingPlan, EntryShape, Row) {
    let plan = BindingPlan {
        target: "@app".to_owned(),
        leaves: vec![
            leaf(
                "@notes",
                HostInterface::DirReadText,
                Provider::FsDirectory,
                Resource::FixedPath {
                    path: "/notes".to_owned(),
                },
            ),
            leaf(
                "@out",
                HostInterface::StdoutText,
                Provider::IoStdout,
                Resource::ProcessStream,
            ),
        ],
    };
    let interfaces = [HostInterface::DirReadText, HostInterface::StdoutText];
    let row = Row::new(interfaces.iter().copied().map(entity)).expect("a row");
    let shape = EntryShape {
        row: row.clone(),
        leaves: plan
            .leaves
            .iter()
            .map(|leaf| EntryLeaf {
                name: leaf.name.clone(),
                interface: leaf.interface,
                optional: leaf.optional,
            })
            .collect(),
    };
    (plan, shape, row)
}

/// Row `ATK-driver-absent-at-admission`: where the platform has no
/// directory driver, admission refuses a target that binds
/// `@host.fs.directory` as `@tool.unavailable`. `admit` holds the driver
/// shared, so it cannot acquire; the watch confirms no call either. From
/// step 31 this is Linux and macOS only.
#[cfg(not(windows))]
#[test]
fn atk_driver_absent_at_admission() {
    let (plan, shape, row) = directory_target();
    let (driver, watch) = watched(inputs());
    let refusal = admit(&plan, &shape, &row, &driver).expect_err("refused");
    assert_eq!(
        refusal,
        AdmissionRefusal::Unavailable(vec![Provider::FsDirectory])
    );
    assert_eq!(refusal.code(), DiagnosticCode::ToolUnavailable);
    assert_eq!(watch.borrow().acquires, 0, "nothing acquired");
    assert_eq!(watch.borrow().calls, 0);
}

/// On Windows the directory driver exists, so the same target passes the
/// drivers check of admission; admission still acquires nothing.
#[cfg(windows)]
#[test]
fn the_directory_driver_passes_admission_on_windows() {
    let (plan, shape, row) = directory_target();
    let (driver, watch) = watched(inputs());
    assert!(admit(&plan, &shape, &row, &driver).is_ok());
    assert_eq!(watch.borrow().acquires, 0, "nothing acquired");
    assert_eq!(watch.borrow().calls, 0);
}

// --- Architecture --------------------------------------------------------

/// What no source of `vibra-host` names outside its test modules: an exit
/// or an abort of the process, a panic, and an `unwrap` or `expect` of a
/// result, an operating system's included.
const ENDS_THE_PROCESS: &[&str] = &[
    "process::exit",
    "process::abort",
    "std::process",
    "panic!",
    "unreachable!",
    "todo!",
    "unimplemented!",
    ".unwrap()",
    ".expect(",
    "unwrap_unchecked",
    "unsafe ",
    "unsafe{",
];

/// The lines of `source`, numbered from one, outside `#[cfg(test)]`
/// modules, with comments removed.
fn code_lines(source: &str) -> Vec<(usize, String)> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut kept = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim_start();
        if line.starts_with("#[cfg(test)]") {
            let mut item = index + 1;
            while lines
                .get(item)
                .is_some_and(|text| text.trim_start().starts_with("#["))
            {
                item += 1;
            }
            let opens = lines.get(item).is_some_and(|text| {
                let text = text.trim_start();
                text.starts_with("mod ") || text.starts_with("pub mod ")
            });
            if opens {
                let mut depth = 0_i64;
                let mut end = item;
                while let Some(text) = lines.get(end) {
                    depth += text.matches('{').count() as i64;
                    depth -= text.matches('}').count() as i64;
                    end += 1;
                    if (depth <= 0 && text.contains('}'))
                        || (depth == 0 && text.trim_end().ends_with(';'))
                    {
                        break;
                    }
                }
                index = end;
                continue;
            }
        }
        let code = lines[index]
            .split("//")
            .next()
            .unwrap_or_default()
            .to_owned();
        kept.push((index + 1, code));
        index += 1;
    }
    kept
}

fn sources(directory: &Path, found: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("a source directory") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

fn ending(source: &str) -> Vec<String> {
    code_lines(source)
        .into_iter()
        .flat_map(|(number, code)| {
            ENDS_THE_PROCESS
                .iter()
                .filter(move |pattern| code.contains(**pattern))
                .map(move |pattern| format!("{number}: `{pattern}`"))
        })
        .collect()
}

/// Row `ATK-no-process-exit`: no source of the crate ends the process.
#[test]
fn atk_no_process_exit() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&root, &mut files);
    assert!(files.iter().any(|path| path.ends_with("system/streams.rs")));
    let found = files
        .iter()
        .flat_map(|path| {
            let source = std::fs::read_to_string(path).expect("a readable source");
            ending(&source)
                .into_iter()
                .map(move |line| format!("{}:{line}", path.display()))
        })
        .collect::<Vec<_>>();
    assert!(
        found.is_empty(),
        "the crate can end the process:\n{}",
        found.join("\n")
    );
}

/// The scan of `atk_no_process_exit` finds each form, and skips comments and
/// test modules only.
#[test]
fn the_process_end_scan_finds_each_form() {
    assert_eq!(
        ending("fn f() { std::process::exit(1) }\n"),
        ["1: `process::exit`", "1: `std::process`"]
    );
    assert_eq!(ending("let x = read().unwrap();\n"), ["1: `.unwrap()`"]);
    assert_eq!(ending("panic!(\"no\");\n"), ["1: `panic!`"]);
    assert!(ending("// std::process::abort()\nlet x = 1; // .unwrap()\n").is_empty());
    assert!(
        ending("#[cfg(test)]\nmod tests {\n    fn t() { x.unwrap(); }\n}\n").is_empty()
    );
    assert_eq!(
        ending(
            "#[cfg(test)]\nmod tests {\n    fn t() {}\n}\nfn after() { x.unwrap(); }\n"
        ),
        ["5: `.unwrap()`"]
    );
    assert_eq!(
        ending("#[cfg(test)]\nfn hook() {}\nfn after() { panic!() }\n"),
        ["3: `panic!`"]
    );
}
