//! The host world: the `@hostWorld.v2` document and `WorldDriver` (M2 step
//! 16).
//!
//! Every host test runs through a `Context` over a `WorldDriver`, with root
//! grants issued by the test hook `root_for_tests`. `W` is the example
//! document of `docs/spec/06-runtime.md`, "Host world". A call that completes
//! returns a receipt, which these tests commit; "no receipt" means no event
//! is due.

use std::cell::RefCell;
use std::rc::Rc;

use vibra_diagnostics::DiagnosticCode;
use vibra_host::broker::{Context, RootGrant};
use vibra_host::driver::{
    Acquire, Action, Completion, Driver, DriverIdentity, Found, FoundAction, Gate,
    Measured, Progress, Refused, Reserved, Resolved, ResourceId,
};
use vibra_host::policy::{self, Policy};
use vibra_host::recipe::{Limits, PROVIDERS, Provider};
use vibra_host::trace::GrantOrigin;
use vibra_host::world::{Clock, HostWorld, WORLD_FORMAT, WorldDriver, decode_world};
use vibra_ir::authority::{
    GrantToken, HostArgument, HostContext, HostErrorCase, HostFault, HostFsKind,
    HostOk, HostReply, HostRequest,
};
use vibra_ir::external::host::{HostInterface, HostOp};

use crate::trace::header;

/// The chapter's example world.
const W: &str = r#"{
  format: @hostWorld.v2
  stdin: "x\ny"
  directories: ["/notes"]
  files: ["/notes/a.txt": "hello" "/notes/raw.bin": {kind: @bytes values: [0u8 255u8]}]
  env: ["HOME": "/home/vibra"]
  args: ["/notes"]
  clock: {wall: 1700000000000000000i64 monotonic: 0u64 step: 1000u64}
  random: {seed: 42u64}
}"#;

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn decoded(text: &str) -> HostWorld {
    decode_world(Some("world.vibon"), text.as_bytes()).expect("a valid world")
}

fn w() -> HostWorld {
    decoded(W)
}

/// The codes of a rejected document.
fn rejected(text: &str) -> Vec<DiagnosticCode> {
    decode_world(Some("world.vibon"), text.as_bytes())
        .expect_err("a rejected world")
        .iter()
        .map(|fault| fault.code())
        .collect()
}

// ---------------------------------------------------------------------------
// Resources, grants, and calls.
// ---------------------------------------------------------------------------

fn acquired(
    driver: &mut dyn Driver,
    provider: Provider,
    resolved: &Resolved,
) -> ResourceId {
    match driver.acquire(provider, resolved) {
        Acquire::Acquired(resource) => resource,
        other => panic!("{} did not acquire: {other:?}", provider.atom()),
    }
}

fn directory(driver: &mut dyn Driver, path: &str) -> ResourceId {
    acquired(
        driver,
        Provider::FsDirectory,
        &Resolved::Directory {
            path: path.to_owned(),
        },
    )
}

/// The resources of a world, one of each provider.
#[derive(Clone, Copy, Debug)]
struct Ids {
    root: ResourceId,
    notes: ResourceId,
    stdin: ResourceId,
    stdout: ResourceId,
    stderr: ResourceId,
    vars: ResourceId,
    args: ResourceId,
    wall: ResourceId,
    monotonic: ResourceId,
    random: ResourceId,
}

fn acquire_all(driver: &mut dyn Driver, notes: bool) -> Ids {
    let root = directory(driver, "/");
    let notes = if notes {
        directory(driver, "/notes")
    } else {
        root
    };
    Ids {
        root,
        notes,
        stdin: acquired(driver, Provider::IoStdin, &Resolved::ProcessStream),
        stdout: acquired(driver, Provider::IoStdout, &Resolved::ProcessStream),
        stderr: acquired(driver, Provider::IoStderr, &Resolved::ProcessStream),
        vars: acquired(
            driver,
            Provider::EnvVars,
            &Resolved::VariableSet { names: Vec::new() },
        ),
        args: acquired(driver, Provider::EnvArgs, &Resolved::ProcessArguments),
        wall: acquired(driver, Provider::TimeWall, &Resolved::SystemClock),
        monotonic: acquired(driver, Provider::TimeMonotonic, &Resolved::SystemClock),
        random: acquired(driver, Provider::Random, &Resolved::SystemRandom),
    }
}

fn policy(expression: &str) -> Policy {
    policy::decode(
        format!(
            "{{format: @scopePolicy.v1 family: @filesystem expression: {expression}}}"
        )
        .as_bytes(),
    )
    .expect("a policy")
}

/// A file rule over the whole held directory with `actions` and `test`.
fn files_rule(actions: &str, test: &str) -> Policy {
    policy(&format!(
        "{{kind: @fileRule root: @testWorld prefix: [] depth: @tree actions: [{actions}] \
         names: {test}}}"
    ))
}

const ANY_NAME: &str = "{kind: @anyName}";

fn root_grant(
    cx: &mut Context<'_>,
    interface: HostInterface,
    resource: ResourceId,
    policy: Option<Policy>,
    variables: Option<Vec<String>>,
    limits: Limits,
) -> GrantToken {
    cx.root_for_tests(RootGrant {
        interface,
        resource,
        origin: GrantOrigin::TestWorld,
        policy,
        variables,
        limits,
    })
    .expect("a root")
}

/// An unrestricted root grant.
fn grant(
    cx: &mut Context<'_>,
    interface: HostInterface,
    resource: ResourceId,
) -> GrantToken {
    root_grant(cx, interface, resource, None, None, Limits::default())
}

/// Calls and commits: the reply, and whether an event was due.
fn run(
    cx: &mut Context<'_>,
    op: HostOp,
    grant: GrantToken,
    arguments: &[HostArgument<'_>],
) -> Result<(HostReply, bool), HostFault> {
    let completion = cx.host(HostRequest {
        op,
        grant,
        arguments,
        invocation: 1,
        parent: None,
        origin: None,
        room: u64::MAX,
    })?;
    let due = completion.receipt.is_some();
    if let Some(receipt) = completion.receipt {
        cx.commit(receipt).expect("the pending receipt");
    }
    Ok((completion.reply, due))
}

/// The reply of a call, which must not be a fault.
fn reply(
    cx: &mut Context<'_>,
    op: HostOp,
    grant: GrantToken,
    arguments: &[HostArgument<'_>],
) -> HostReply {
    run(cx, op, grant, arguments).expect("no fault").0
}

fn at(cx: &mut Context<'_>, op: HostOp, grant: GrantToken, path: &[&str]) -> HostReply {
    let path = names(path);
    reply(cx, op, grant, &[HostArgument::Path(&path)])
}

fn write(
    cx: &mut Context<'_>,
    grant: GrantToken,
    path: &[&str],
    contents: &str,
) -> HostReply {
    let path = names(path);
    reply(
        cx,
        HostOp::FsWriteText,
        grant,
        &[HostArgument::Path(&path), HostArgument::Text(contents)],
    )
}

fn count(cx: &mut Context<'_>, op: HostOp, grant: GrantToken, count: u64) -> HostReply {
    reply(cx, op, grant, &[HostArgument::Count(count)])
}

fn none(cx: &mut Context<'_>, op: HostOp, grant: GrantToken) -> HostReply {
    reply(cx, op, grant, &[])
}

fn err(op: HostOp, case: &str) -> HostReply {
    HostReply::Err(HostErrorCase::new(op.row().error, case).expect("a case"))
}

fn text(value: &str) -> HostReply {
    HostReply::Ok(HostOk::Text(value.to_owned()))
}

fn bytes(value: &[u8]) -> HostReply {
    HostReply::Ok(HostOk::Bytes(value.to_vec()))
}

fn listing(entries: &[&str]) -> HostReply {
    HostReply::Ok(HostOk::Names(names(entries)))
}

fn metadata(kind: HostFsKind, size: u64) -> HostReply {
    HostReply::Ok(HostOk::Metadata { kind, size })
}

const VOID: HostReply = HostReply::Ok(HostOk::Void);

// ---------------------------------------------------------------------------
// A recording wrapper: the calls, the gate's questions and answers, and the
// completions, in order.
// ---------------------------------------------------------------------------

/// What the driver asked the gate, owned, and whether the gate admitted it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Asked {
    real_names: Vec<String>,
    action: FoundAction,
    size: Option<u64>,
    links: Option<u32>,
    admitted: Option<u64>,
}

#[derive(Debug, Default)]
struct Log {
    calls: usize,
    asked: Vec<Asked>,
    completions: Vec<Completion>,
}

struct RecordingGate<'g> {
    inner: &'g mut dyn Gate,
    log: Rc<RefCell<Log>>,
}

impl Gate for RecordingGate<'_> {
    fn admit(&mut self, found: &Found<'_>) -> Result<Reserved, Refused> {
        let answer = self.inner.admit(found);
        self.log.borrow_mut().asked.push(Asked {
            real_names: found.real_names.to_vec(),
            action: found.action,
            size: found.size,
            links: found.links,
            admitted: answer.as_ref().ok().map(|reserved| reserved.input),
        });
        answer
    }

    fn admit_error(&mut self, names: &[String]) -> Result<(), Refused> {
        self.inner.admit_error(names)
    }
}

struct Recording<'w> {
    inner: &'w mut WorldDriver,
    log: Rc<RefCell<Log>>,
}

impl Driver for Recording<'_> {
    fn identity(&self, provider: Provider) -> Option<DriverIdentity> {
        self.inner.identity(provider)
    }

    fn acquire(&mut self, provider: Provider, resource: &Resolved) -> Acquire {
        self.inner.acquire(provider, resource)
    }

    fn release(&mut self, resource: ResourceId) -> Result<(), String> {
        self.inner.release(resource)
    }

    fn call(
        &mut self,
        resource: ResourceId,
        action: &Action<'_>,
        gate: &mut dyn Gate,
    ) -> Completion {
        self.log.borrow_mut().calls += 1;
        let mut gate = RecordingGate {
            inner: gate,
            log: Rc::clone(&self.log),
        };
        let completion = self.inner.call(resource, action, &mut gate);
        self.log.borrow_mut().completions.push(completion.clone());
        completion
    }
}

// ---------------------------------------------------------------------------
// The document.
// ---------------------------------------------------------------------------

#[test]
fn decode_the_example() {
    let world = w();
    let notes = world.filesystem();
    assert_eq!(notes.file(&names(&["notes", "a.txt"])), Some(&b"hello"[..]));
    assert_eq!(
        notes.file(&names(&["notes", "raw.bin"])),
        Some(&[0, 255][..])
    );
    assert!(notes.is_directory(&names(&["notes"])));
    assert_eq!(world.stdin(), b"x\ny");
    assert_eq!(
        world.env().get("HOME").map(String::as_str),
        Some("/home/vibra")
    );
    assert_eq!(world.args(), names(&["/notes"]));
    assert_eq!(
        world.clock(),
        Clock {
            wall: 1_700_000_000_000_000_000,
            monotonic: 0,
            step: 1000,
        }
    );
    assert_eq!(world.seed(), 42);
}

#[test]
fn decode_the_minimum() {
    let world = decoded("{format: @hostWorld.v2}");
    assert_eq!(world, HostWorld::default());
    let filesystem = world.filesystem();
    assert!(filesystem.is_directory(&[]));
    assert_eq!(filesystem.directories().count(), 0);
    assert_eq!(filesystem.files().count(), 0);
    assert!(world.stdin().is_empty());
    assert!(world.env().is_empty());
    assert!(world.args().is_empty());
    assert_eq!(
        world.clock(),
        Clock {
            wall: 0,
            monotonic: 0,
            step: 1,
        }
    );
    assert_eq!(world.seed(), 0);
    assert_eq!(WORLD_FORMAT, "@hostWorld.v2");
}

#[test]
fn unknown_field() {
    let document = W.replace(
        "random: {seed: 42u64}",
        "random: {seed: 42u64}\n  network: []",
    );
    assert_eq!(rejected(&document), [DiagnosticCode::DataInvalidShape]);
}

#[test]
fn version_1() {
    let document = W.replace("@hostWorld.v2", "@hostWorld.v1");
    assert_eq!(rejected(&document), [DiagnosticCode::DataInvalidValue]);
}

#[test]
fn clock_with_no_member() {
    assert_eq!(
        rejected("{format: @hostWorld.v2 clock: {}}"),
        [DiagnosticCode::DataInvalidShape]
    );
    assert_eq!(
        rejected("{format: @hostWorld.v2 random: {}}"),
        [DiagnosticCode::DataInvalidShape]
    );
}

#[test]
fn clock_with_one_member() {
    let world = decoded("{format: @hostWorld.v2 clock: {step: 1000u64}}");
    assert_eq!(
        world.clock(),
        Clock {
            wall: 0,
            monotonic: 0,
            step: 1000,
        }
    );
    let mut driver = WorldDriver::new(&world);
    let ids = acquire_all(&mut driver, false);
    let mut cx = Context::new(&mut driver, header());
    let wall = grant(&mut cx, HostInterface::WallClock, ids.wall);
    let monotonic = grant(&mut cx, HostInterface::MonotonicClock, ids.monotonic);
    assert_eq!(
        none(&mut cx, HostOp::TimeNowWall, wall),
        HostReply::Ok(HostOk::I64(0))
    );
    assert_eq!(
        none(&mut cx, HostOp::TimeNowMonotonic, monotonic),
        HostReply::Ok(HostOk::U64(0))
    );
    assert_eq!(
        none(&mut cx, HostOp::TimeNowMonotonic, monotonic),
        HostReply::Ok(HostOk::U64(1000))
    );
}

#[test]
fn unsuffixed_integer() {
    for document in [
        "{format: @hostWorld.v2 random: {seed: 42}}",
        "{format: @hostWorld.v2 clock: {wall: 5}}",
        "{format: @hostWorld.v2 stdin: {kind: @bytes values: [1]}}",
        "{format: @hostWorld.v2 files: [\"/a\": {kind: @bytes values: [1u8 2]}]}",
    ] {
        assert_eq!(
            rejected(document),
            [DiagnosticCode::DataInvalidValue],
            "{document}"
        );
    }
    // A literal of another width or out of range fails decoding too.
    for document in [
        "{format: @hostWorld.v2 random: {seed: 42u32}}",
        "{format: @hostWorld.v2 stdin: {kind: @bytes values: [256u8]}}",
    ] {
        assert_eq!(
            rejected(document),
            [DiagnosticCode::DataInvalidValue],
            "{document}"
        );
    }
}

#[test]
fn path_not_canonical() {
    for path in [
        "/a/../b", "/a/./b", "a/b", "/a//b", "/a/", "", "/a\\\\b", "/C:", "/a:b",
        "/con", "/NUL.txt", "/lpt1. ",
    ] {
        assert_eq!(
            rejected(&format!(
                "{{format: @hostWorld.v2 files: [\"{path}\": \"x\"]}}"
            )),
            [DiagnosticCode::DataInvalidValue],
            "file {path:?}"
        );
        if !path.is_empty() {
            assert_eq!(
                rejected(&format!(
                    "{{format: @hostWorld.v2 directories: [\"{path}\"]}}"
                )),
                [DiagnosticCode::DataInvalidValue],
                "directory {path:?}"
            );
        }
    }
    // A content of another form is the wrong shape.
    assert_eq!(
        rejected("{format: @hostWorld.v2 files: [\"/a\": [1u8]]}"),
        [DiagnosticCode::DataInvalidShape]
    );
    assert_eq!(
        rejected("{format: @hostWorld.v2 stdin: {kind: @bytes values: [] extra: 1u8}}"),
        [DiagnosticCode::DataInvalidShape]
    );
}

#[test]
fn file_and_directory_conflict() {
    for document in [
        "{format: @hostWorld.v2 files: [\"/a\": \"x\" \"/a/b\": \"y\"]}",
        "{format: @hostWorld.v2 directories: [\"/a\"] files: [\"/a\": \"x\"]}",
        "{format: @hostWorld.v2 directories: [\"/a/b\"] files: [\"/a\": \"x\"]}",
        "{format: @hostWorld.v2 directories: [\"/a\" \"/a\"]}",
    ] {
        assert_eq!(
            rejected(document),
            [DiagnosticCode::DataInvalidValue],
            "{document}"
        );
    }
    // A file given twice is a repeated key.
    assert_eq!(
        rejected("{format: @hostWorld.v2 files: [\"/a\": \"x\" \"/a\": \"y\"]}"),
        [DiagnosticCode::DataDuplicateKey]
    );
}

#[test]
fn root_as_a_file() {
    assert_eq!(
        rejected("{format: @hostWorld.v2 files: [\"/\": \"x\"]}"),
        [DiagnosticCode::DataInvalidValue]
    );
}

#[test]
fn implied_ancestors() {
    let world = decoded("{format: @hostWorld.v2 files: [\"/a/b/c.txt\": \"x\"]}");
    let mut driver = WorldDriver::new(&world);
    let ids = acquire_all(&mut driver, false);
    let mut cx = Context::new(&mut driver, header());
    let stat = grant(&mut cx, HostInterface::DirStat, ids.root);
    assert_eq!(
        at(&mut cx, HostOp::FsMetadataStat, stat, &["a", "b"]),
        metadata(HostFsKind::Directory, 0)
    );
    assert_eq!(
        at(&mut cx, HostOp::FsMetadataStat, stat, &["a"]),
        metadata(HostFsKind::Directory, 0)
    );
}

// ---------------------------------------------------------------------------
// The filesystem.
// ---------------------------------------------------------------------------

#[test]
fn read_text() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let read = grant(&mut cx, HostInterface::DirReadText, ids.notes);
    assert_eq!(
        at(&mut cx, HostOp::FsReadText, read, &["a.txt"]),
        text("hello")
    );
}

#[test]
fn read_invalid_text() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let read = grant(&mut cx, HostInterface::DirReadText, ids.notes);
    assert_eq!(
        at(&mut cx, HostOp::FsReadText, read, &["raw.bin"]),
        err(HostOp::FsReadText, "invalidText")
    );
}

#[test]
fn read_bytes() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let read = grant(&mut cx, HostInterface::DirReadBytes, ids.notes);
    assert_eq!(
        at(&mut cx, HostOp::FsReadFile, read, &["raw.bin"]),
        bytes(&[0, 255])
    );
}

#[test]
fn absent_file() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let read = grant(&mut cx, HostInterface::DirReadText, ids.notes);
    assert_eq!(
        at(&mut cx, HostOp::FsReadText, read, &["no.txt"]),
        err(HostOp::FsReadText, "notFound")
    );
    // A file where a directory is required on the way is not a directory.
    assert_eq!(
        at(&mut cx, HostOp::FsReadText, read, &["a.txt", "b"]),
        err(HostOp::FsReadText, "notADirectory")
    );
}

#[test]
fn read_a_directory() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let read = grant(&mut cx, HostInterface::DirReadText, ids.notes);
    assert_eq!(
        at(&mut cx, HostOp::FsReadText, read, &[]),
        err(HostOp::FsReadText, "notAFile")
    );
}

#[test]
fn list() {
    let world = decoded(
        "{format: @hostWorld.v2 files: [\"/notes/b\": \"\" \"/notes/A\": \"\" \"/notes/a\": \"\"]}",
    );
    let mut driver = WorldDriver::new(&world);
    let notes = directory(&mut driver, "/notes");
    let mut cx = Context::new(&mut driver, header());
    let list = grant(&mut cx, HostInterface::DirList, notes);
    assert_eq!(
        at(&mut cx, HostOp::FsReadEntries, list, &[]),
        listing(&["A", "a", "b"])
    );
}

#[test]
fn list_a_file() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let list = grant(&mut cx, HostInterface::DirList, ids.notes);
    assert_eq!(
        at(&mut cx, HostOp::FsReadEntries, list, &["a.txt"]),
        err(HostOp::FsReadEntries, "notADirectory")
    );
}

#[test]
fn stat() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let stat = grant(&mut cx, HostInterface::DirStat, ids.notes);
    assert_eq!(
        at(&mut cx, HostOp::FsMetadataStat, stat, &["a.txt"]),
        metadata(HostFsKind::File, 5)
    );
    assert_eq!(
        at(&mut cx, HostOp::FsMetadataStat, stat, &[]),
        metadata(HostFsKind::Directory, 0)
    );
}

#[test]
fn write_creates() {
    let world = w();
    let mut driver = WorldDriver::new(&world);
    let ids = acquire_all(&mut driver, true);
    {
        let mut cx = Context::new(&mut driver, header());
        let writer = grant(&mut cx, HostInterface::DirWriteText, ids.notes);
        let read = grant(&mut cx, HostInterface::DirReadText, ids.notes);
        assert_eq!(write(&mut cx, writer, &["new.txt"], "fresh"), VOID);
        assert_eq!(
            at(&mut cx, HostOp::FsReadText, read, &["new.txt"]),
            text("fresh")
        );
    }
    // A write changes only the copy: another driver of the same world, and
    // the world itself, do not have it.
    assert_eq!(world.filesystem().file(&names(&["notes", "new.txt"])), None);
    let mut second = WorldDriver::new(&world);
    let notes = directory(&mut second, "/notes");
    let mut cx = Context::new(&mut second, header());
    let read = grant(&mut cx, HostInterface::DirReadText, notes);
    assert_eq!(
        at(&mut cx, HostOp::FsReadText, read, &["new.txt"]),
        err(HostOp::FsReadText, "notFound")
    );
}

#[test]
fn write_replaces() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let writer = grant(&mut cx, HostInterface::DirWriteText, ids.notes);
    let read = grant(&mut cx, HostInterface::DirReadText, ids.notes);
    assert_eq!(write(&mut cx, writer, &["a.txt"], "x"), VOID);
    assert_eq!(at(&mut cx, HostOp::FsReadText, read, &["a.txt"]), text("x"));
}

#[test]
fn write_without_a_parent() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let writer = grant(&mut cx, HostInterface::DirWriteText, ids.notes);
    let list = grant(&mut cx, HostInterface::DirList, ids.notes);
    assert_eq!(
        write(&mut cx, writer, &["no", "a.txt"], "x"),
        err(HostOp::FsWriteText, "notFound")
    );
    // No parent directory was made.
    assert_eq!(
        at(&mut cx, HostOp::FsReadEntries, list, &[]),
        listing(&["a.txt", "raw.bin"])
    );
}

#[test]
fn a_refused_create_leaves_the_copy() {
    let mut driver = WorldDriver::new(&w());
    let notes = directory(&mut driver, "/notes");
    let log = Rc::new(RefCell::new(Log::default()));
    let mut recording = Recording {
        inner: &mut driver,
        log: Rc::clone(&log),
    };
    let mut cx = Context::new(&mut recording, header());
    let list = grant(&mut cx, HostInterface::DirList, notes);
    let before = at(&mut cx, HostOp::FsReadEntries, list, &[]);
    let writer = root_grant(
        &mut cx,
        HostInterface::DirWriteText,
        notes,
        Some(files_rule("@replace", ANY_NAME)),
        None,
        Limits::default(),
    );
    let (result, due) = run(
        &mut cx,
        HostOp::FsWriteText,
        writer,
        &[
            HostArgument::Path(&names(&["new.txt"])),
            HostArgument::Text("fresh"),
        ],
    )
    .expect("no fault");
    assert_eq!(result, err(HostOp::FsWriteText, "denied"));
    assert!(
        due,
        "a refusal after the adapter opened its parent is completed"
    );
    assert_eq!(at(&mut cx, HostOp::FsReadEntries, list, &[]), before);
    let asked = log.borrow().asked.clone();
    let create = asked
        .iter()
        .find(|asked| asked.action == FoundAction::Create)
        .expect("the gate saw a create");
    assert_eq!(create.real_names, names(&["new.txt"]));
    assert_eq!(create.admitted, None);
}

#[test]
fn a_refused_replace_leaves_the_copy() {
    let mut driver = WorldDriver::new(&w());
    let notes = directory(&mut driver, "/notes");
    let log = Rc::new(RefCell::new(Log::default()));
    let mut recording = Recording {
        inner: &mut driver,
        log: Rc::clone(&log),
    };
    let mut cx = Context::new(&mut recording, header());
    let read = grant(&mut cx, HostInterface::DirReadText, notes);
    let writer = root_grant(
        &mut cx,
        HostInterface::DirWriteText,
        notes,
        Some(files_rule("@create", ANY_NAME)),
        None,
        Limits::default(),
    );
    assert_eq!(
        write(&mut cx, writer, &["a.txt"], "x"),
        err(HostOp::FsWriteText, "denied")
    );
    assert_eq!(
        at(&mut cx, HostOp::FsReadText, read, &["a.txt"]),
        text("hello")
    );
    let asked = log.borrow().asked.clone();
    assert!(
        asked
            .iter()
            .any(|asked| asked.action == FoundAction::Replace
                && asked.links == Some(1)
                && asked.admitted.is_none()),
        "{asked:?}"
    );
}

// ---------------------------------------------------------------------------
// The standard streams.
// ---------------------------------------------------------------------------

#[test]
fn stdin_once() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let stdin = grant(&mut cx, HostInterface::StdinText, ids.stdin);
    assert_eq!(count(&mut cx, HostOp::IoStdinText, stdin, 16), text("x\ny"));
    assert_eq!(count(&mut cx, HostOp::IoStdinText, stdin, 16), text(""));
}

#[test]
fn stdin_invalid_text() {
    let world =
        decoded("{format: @hostWorld.v2 stdin: {kind: @bytes values: [255u8]}}");
    let mut driver = WorldDriver::new(&world);
    let ids = acquire_all(&mut driver, false);
    let mut cx = Context::new(&mut driver, header());
    let stdin = grant(&mut cx, HostInterface::StdinText, ids.stdin);
    assert_eq!(
        count(&mut cx, HostOp::IoStdinText, stdin, 16),
        err(HostOp::IoStdinText, "invalidText")
    );
    assert_eq!(count(&mut cx, HostOp::IoStdinText, stdin, 16), text(""));
}

#[test]
fn stdin_reports_its_size() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let log = Rc::new(RefCell::new(Log::default()));
    let mut recording = Recording {
        inner: &mut driver,
        log: Rc::clone(&log),
    };
    let mut cx = Context::new(&mut recording, header());
    // A grant whose input bound is below the size: the gate refuses at the
    // size the driver reported, and the stream yielded nothing.
    let small = root_grant(
        &mut cx,
        HostInterface::StdinBytes,
        ids.stdin,
        None,
        None,
        Limits {
            input_bytes: Some(2),
            ..Limits::default()
        },
    );
    assert_eq!(
        count(&mut cx, HostOp::IoStdinBytes, small, 16),
        err(HostOp::IoStdinBytes, "limitExceeded")
    );
    let stdin = grant(&mut cx, HostInterface::StdinBytes, ids.stdin);
    assert_eq!(
        count(&mut cx, HostOp::IoStdinBytes, stdin, 16),
        bytes(b"x\ny")
    );
    let log = log.borrow();
    let sizes = log
        .asked
        .iter()
        .map(|asked| (asked.action, asked.size, asked.admitted))
        .collect::<Vec<_>>();
    assert_eq!(
        sizes,
        [
            (FoundAction::Read, Some(3), None),
            (FoundAction::Read, Some(3), Some(3)),
        ]
    );
    assert_eq!(log.completions.first(), Some(&Completion::Refused));
}

#[test]
fn stdin_by_count() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let stdin = grant(&mut cx, HostInterface::StdinBytes, ids.stdin);
    assert_eq!(
        count(&mut cx, HostOp::IoStdinBytes, stdin, 2),
        bytes(b"x\n")
    );
    assert_eq!(count(&mut cx, HostOp::IoStdinBytes, stdin, 2), bytes(b"y"));
    assert_eq!(count(&mut cx, HostOp::IoStdinBytes, stdin, 2), bytes(b""));
}

#[test]
fn stdin_text_below_a_scalar() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let stdin = grant(&mut cx, HostInterface::StdinText, ids.stdin);
    let (result, due) = run(
        &mut cx,
        HostOp::IoStdinText,
        stdin,
        &[HostArgument::Count(3)],
    )
    .expect("no fault");
    assert_eq!(result, err(HostOp::IoStdinText, "unsupported"));
    assert!(!due, "nothing was attempted");
    assert_eq!(count(&mut cx, HostOp::IoStdinText, stdin, 16), text("x\ny"));
}

#[test]
fn stdin_text_takes_whole_scalars() {
    // Q30: U+20AC then `a`, read with a count of 4, then again.
    let world = decoded("{format: @hostWorld.v2 stdin: \"\u{20ac}a\"}");
    let mut driver = WorldDriver::new(&world);
    let ids = acquire_all(&mut driver, false);
    let mut cx = Context::new(&mut driver, header());
    let stdin = grant(&mut cx, HostInterface::StdinText, ids.stdin);
    assert_eq!(
        count(&mut cx, HostOp::IoStdinText, stdin, 4),
        text("\u{20ac}a")
    );
    assert_eq!(count(&mut cx, HostOp::IoStdinText, stdin, 4), text(""));
    // A count that ends inside a scalar returns the whole scalars before it.
    let world = decoded("{format: @hostWorld.v2 stdin: \"ab\u{20ac}\"}");
    let mut driver = WorldDriver::new(&world);
    let ids = acquire_all(&mut driver, false);
    let mut cx = Context::new(&mut driver, header());
    let stdin = grant(&mut cx, HostInterface::StdinText, ids.stdin);
    assert_eq!(count(&mut cx, HostOp::IoStdinText, stdin, 4), text("ab"));
    assert_eq!(
        count(&mut cx, HostOp::IoStdinText, stdin, 4),
        text("\u{20ac}")
    );
}

#[test]
fn stdout_order() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    {
        let mut cx = Context::new(&mut driver, header());
        let text_out = grant(&mut cx, HostInterface::StdoutText, ids.stdout);
        let bytes_out = grant(&mut cx, HostInterface::StdoutBytes, ids.stdout);
        let text_err = grant(&mut cx, HostInterface::StderrText, ids.stderr);
        assert_eq!(
            reply(
                &mut cx,
                HostOp::IoStdoutText,
                text_out,
                &[HostArgument::Text("a")]
            ),
            VOID
        );
        assert_eq!(
            reply(
                &mut cx,
                HostOp::IoStdoutBytes,
                bytes_out,
                &[HostArgument::Bytes(&[98])]
            ),
            VOID
        );
        assert_eq!(
            reply(
                &mut cx,
                HostOp::IoStderrText,
                text_err,
                &[HostArgument::Text("e")]
            ),
            VOID
        );
    }
    let output = driver.output();
    assert_eq!(output.stdout, b"ab");
    assert_eq!(output.stderr, b"e");
}

// ---------------------------------------------------------------------------
// The environment and the arguments.
// ---------------------------------------------------------------------------

#[test]
fn variable_present_absent() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let vars = root_grant(
        &mut cx,
        HostInterface::EnvReadVar,
        ids.vars,
        None,
        Some(names(&["HOME", "NOPE"])),
        Limits::default(),
    );
    assert_eq!(
        reply(
            &mut cx,
            HostOp::EnvReadVar,
            vars,
            &[HostArgument::Text("HOME")]
        ),
        text("/home/vibra")
    );
    assert_eq!(
        reply(
            &mut cx,
            HostOp::EnvReadVar,
            vars,
            &[HostArgument::Text("NOPE")]
        ),
        err(HostOp::EnvReadVar, "notPresent")
    );
}

#[test]
fn unholdable_name() {
    // The world holds a variable no environment can: it is still absent.
    let world = decoded("{format: @hostWorld.v2 env: [\"A=B\": \"v\" \"\": \"w\"]}");
    let mut driver = WorldDriver::new(&world);
    let ids = acquire_all(&mut driver, false);
    let mut cx = Context::new(&mut driver, header());
    let vars = root_grant(
        &mut cx,
        HostInterface::EnvReadVar,
        ids.vars,
        None,
        Some(names(&["", "A=B"])),
        Limits::default(),
    );
    for name in ["", "A=B"] {
        assert_eq!(
            reply(
                &mut cx,
                HostOp::EnvReadVar,
                vars,
                &[HostArgument::Text(name)]
            ),
            err(HostOp::EnvReadVar, "notPresent"),
            "{name:?}"
        );
    }
}

#[test]
fn arguments() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let args = grant(&mut cx, HostInterface::EnvReadArgs, ids.args);
    assert_eq!(
        none(&mut cx, HostOp::EnvReadArgs, args),
        listing(&["/notes"])
    );
}

// ---------------------------------------------------------------------------
// The clocks and the random source.
// ---------------------------------------------------------------------------

#[test]
fn wall_clock() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let wall = grant(&mut cx, HostInterface::WallClock, ids.wall);
    for _ in 0..2 {
        assert_eq!(
            none(&mut cx, HostOp::TimeNowWall, wall),
            HostReply::Ok(HostOk::I64(1_700_000_000_000_000_000))
        );
    }
}

#[test]
fn monotonic_clock() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let monotonic = grant(&mut cx, HostInterface::MonotonicClock, ids.monotonic);
    for expected in [0, 1000, 2000] {
        assert_eq!(
            none(&mut cx, HostOp::TimeNowMonotonic, monotonic),
            HostReply::Ok(HostOk::U64(expected))
        );
    }
}

#[test]
fn monotonic_overflow() {
    let world = decoded(
        "{format: @hostWorld.v2 clock: {monotonic: 18446744073709551615u64 step: 1u64}}",
    );
    let mut driver = WorldDriver::new(&world);
    let ids = acquire_all(&mut driver, false);
    let mut cx = Context::new(&mut driver, header());
    let monotonic = grant(&mut cx, HostInterface::MonotonicClock, ids.monotonic);
    assert_eq!(
        none(&mut cx, HostOp::TimeNowMonotonic, monotonic),
        HostReply::Ok(HostOk::U64(u64::MAX))
    );
    for _ in 0..2 {
        assert_eq!(
            none(&mut cx, HostOp::TimeNowMonotonic, monotonic),
            err(HostOp::TimeNowMonotonic, "unavailable")
        );
    }
}

/// SplitMix64, written from its definition: add the golden gamma to the
/// state, then mix the state with two multiply-xorshift rounds.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The first two outputs from seed 42, derived by hand in the report.
const SEED_42: [u64; 2] = [0xBDD7_3226_2FEB_6E95, 0x28EF_E333_B266_F103];

#[test]
fn random_vector() {
    let mut state = 42;
    assert_eq!([splitmix64(&mut state), splitmix64(&mut state)], SEED_42);
    let [first, second] = SEED_42.map(u64::to_le_bytes);
    assert_eq!(first[..3], [149, 110, 235]);
    assert_eq!(second, [3, 241, 102, 178, 51, 227, 239, 40]);
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let random = grant(&mut cx, HostInterface::RandomBytes, ids.random);
    // 3 bytes take one output and discard its other 5 bytes; 8 take the next.
    assert_eq!(
        count(&mut cx, HostOp::RandomGenerateBytes, random, 3),
        bytes(&first[..3])
    );
    assert_eq!(
        count(&mut cx, HostOp::RandomGenerateBytes, random, 8),
        bytes(&second)
    );
}

#[test]
fn random_zero() {
    let mut driver = WorldDriver::new(&w());
    let ids = acquire_all(&mut driver, true);
    let mut cx = Context::new(&mut driver, header());
    let random = grant(&mut cx, HostInterface::RandomBytes, ids.random);
    assert_eq!(
        count(&mut cx, HostOp::RandomGenerateBytes, random, 0),
        bytes(&[])
    );
    assert_eq!(
        count(&mut cx, HostOp::RandomGenerateBytes, random, 8),
        bytes(&SEED_42[0].to_le_bytes())
    );
}

// ---------------------------------------------------------------------------
// Recipes under a world, and the driver's identity.
// ---------------------------------------------------------------------------

#[test]
fn fixed_path_absent() {
    let mut driver = WorldDriver::new(&w());
    for path in ["/missing", "/notes/a.txt", "missing"] {
        assert_eq!(
            driver.acquire(
                Provider::FsDirectory,
                &Resolved::Directory {
                    path: path.to_owned()
                }
            ),
            Acquire::Refused,
            "{path}"
        );
    }
}

#[test]
fn fixed_path_relative() {
    let mut driver = WorldDriver::new(&w());
    let notes = directory(&mut driver, "notes");
    let mut cx = Context::new(&mut driver, header());
    let read = grant(&mut cx, HostInterface::DirReadText, notes);
    assert_eq!(
        at(&mut cx, HostOp::FsReadText, read, &["a.txt"]),
        text("hello")
    );
}

#[test]
fn the_worlds_identity() {
    let driver = WorldDriver::new(&w());
    assert_eq!(PROVIDERS.len(), 9);
    for row in PROVIDERS {
        assert_eq!(
            driver.identity(row.provider),
            Some(DriverIdentity {
                provider: "@hostWorld.v2".to_owned(),
                version: "0.0.2".to_owned(),
            }),
            "{}",
            row.provider.atom()
        );
    }
}

#[test]
fn progress_is_measured() {
    let mut driver = WorldDriver::new(&w());
    let notes = directory(&mut driver, "/notes");
    let log = Rc::new(RefCell::new(Log::default()));
    let mut recording = Recording {
        inner: &mut driver,
        log: Rc::clone(&log),
    };
    let mut cx = Context::new(&mut recording, header());
    let read = grant(&mut cx, HostInterface::DirReadText, notes);
    let writer = grant(&mut cx, HostInterface::DirWriteText, notes);
    assert_eq!(
        at(&mut cx, HostOp::FsReadText, read, &["a.txt"]),
        text("hello")
    );
    assert_eq!(write(&mut cx, writer, &["b.txt"], "abc"), VOID);
    let progress = log
        .borrow()
        .completions
        .iter()
        .map(|completion| match completion {
            Completion::Completed { progress, .. } => *progress,
            other => panic!("not completed: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        progress,
        [
            Progress {
                input: Measured::Known(5),
                output: Measured::Known(0),
            },
            Progress {
                input: Measured::Known(0),
                output: Measured::Known(3),
            },
        ]
    );
}

// ---------------------------------------------------------------------------
// Attacks.
// ---------------------------------------------------------------------------

/// What one error row's call returned: the reply, and the completion the
/// driver gave the broker.
fn error_row(
    cx: &mut Context<'_>,
    log: &Rc<RefCell<Log>>,
    op: HostOp,
    grant: GrantToken,
    arguments: &[HostArgument<'_>],
) -> (HostReply, Completion) {
    let result = run(cx, op, grant, arguments);
    let Ok((reply, _)) = result else {
        panic!("`{}` became a fault: {result:?}", op.symbol());
    };
    let completion = log
        .borrow()
        .completions
        .last()
        .cloned()
        .expect("the driver was called");
    (reply, completion)
}

#[test]
fn atk_driver_error_not_fault() {
    let world = decoded(
        "{format: @hostWorld.v2 stdin: {kind: @bytes values: [255u8]} \
         directories: [\"/notes\"] \
         files: [\"/notes/a.txt\": \"hello\" \"/notes/raw.bin\": {kind: @bytes values: [0u8 255u8]}] \
         clock: {monotonic: 18446744073709551615u64}}",
    );
    let mut driver = WorldDriver::new(&world);
    let ids = acquire_all(&mut driver, true);
    let log = Rc::new(RefCell::new(Log::default()));
    let mut recording = Recording {
        inner: &mut driver,
        log: Rc::clone(&log),
    };
    let mut cx = Context::new(&mut recording, header());
    let read = grant(&mut cx, HostInterface::DirReadText, ids.notes);
    let list = grant(&mut cx, HostInterface::DirList, ids.notes);
    let writer = grant(&mut cx, HostInterface::DirWriteText, ids.notes);
    let no_create = root_grant(
        &mut cx,
        HostInterface::DirWriteText,
        ids.notes,
        Some(files_rule("@replace", ANY_NAME)),
        None,
        Limits::default(),
    );
    let no_replace = root_grant(
        &mut cx,
        HostInterface::DirWriteText,
        ids.notes,
        Some(files_rule("@create", ANY_NAME)),
        None,
        Limits::default(),
    );
    let stdin = grant(&mut cx, HostInterface::StdinText, ids.stdin);
    let vars = grant(&mut cx, HostInterface::EnvReadVar, ids.vars);
    let monotonic = grant(&mut cx, HostInterface::MonotonicClock, ids.monotonic);
    let path = |path: &[&str]| names(path);
    // Each row: the operation, the grant, the path, the text written, the
    // case.
    let rows = [
        (
            HostOp::FsReadText,
            read,
            path(&["raw.bin"]),
            None,
            "invalidText",
        ),
        (
            HostOp::FsReadText,
            read,
            path(&["no.txt"]),
            None,
            "notFound",
        ),
        (HostOp::FsReadText, read, path(&[]), None, "notAFile"),
        (
            HostOp::FsReadText,
            read,
            path(&["a.txt", "b"]),
            None,
            "notADirectory",
        ),
        (
            HostOp::FsReadEntries,
            list,
            path(&["a.txt"]),
            None,
            "notADirectory",
        ),
        (
            HostOp::FsWriteText,
            writer,
            path(&["no", "a.txt"]),
            Some("x"),
            "notFound",
        ),
        (
            HostOp::FsWriteText,
            no_create,
            path(&["new.txt"]),
            Some("x"),
            "denied",
        ),
        (
            HostOp::FsWriteText,
            no_replace,
            path(&["a.txt"]),
            Some("x"),
            "denied",
        ),
    ];
    for (op, grant, at, contents, case) in &rows {
        let mut arguments = vec![HostArgument::Path(at)];
        if let Some(contents) = contents {
            arguments.push(HostArgument::Text(contents));
        }
        let (reply, completion) = error_row(&mut cx, &log, *op, *grant, &arguments);
        assert_eq!(reply, err(*op, case), "{at:?}");
        match *case {
            "denied" => assert_eq!(completion, Completion::Refused),
            _ => assert!(matches!(completion, Completion::Completed { .. })),
        }
    }
    let (reply, completion) = error_row(
        &mut cx,
        &log,
        HostOp::IoStdinText,
        stdin,
        &[HostArgument::Count(3)],
    );
    assert_eq!(reply, err(HostOp::IoStdinText, "unsupported"));
    assert!(matches!(completion, Completion::NotSubmitted(_)));
    let (reply, completion) = error_row(
        &mut cx,
        &log,
        HostOp::IoStdinText,
        stdin,
        &[HostArgument::Count(16)],
    );
    assert_eq!(reply, err(HostOp::IoStdinText, "invalidText"));
    assert!(matches!(completion, Completion::Completed { .. }));
    for name in ["NOPE", "", "A=B"] {
        let (reply, completion) = error_row(
            &mut cx,
            &log,
            HostOp::EnvReadVar,
            vars,
            &[HostArgument::Text(name)],
        );
        assert_eq!(reply, err(HostOp::EnvReadVar, "notPresent"), "{name:?}");
        assert!(matches!(completion, Completion::Completed { .. }));
    }
    let _ = error_row(&mut cx, &log, HostOp::TimeNowMonotonic, monotonic, &[]);
    let (reply, completion) =
        error_row(&mut cx, &log, HostOp::TimeNowMonotonic, monotonic, &[]);
    assert_eq!(reply, err(HostOp::TimeNowMonotonic, "unavailable"));
    assert!(matches!(completion, Completion::Completed { .. }));
}

#[test]
fn atk_no_redispatch_on_error() {
    let mut driver = WorldDriver::new(&w());
    let notes = directory(&mut driver, "/notes");
    let log = Rc::new(RefCell::new(Log::default()));
    let mut recording = Recording {
        inner: &mut driver,
        log: Rc::clone(&log),
    };
    let mut cx = Context::new(&mut recording, header());
    let read = grant(&mut cx, HostInterface::DirReadText, notes);
    assert_eq!(
        at(&mut cx, HostOp::FsReadText, read, &["no.txt"]),
        err(HostOp::FsReadText, "notFound")
    );
    assert_eq!(log.borrow().calls, 1);
}

#[test]
fn gate_before_content() {
    let mut driver = WorldDriver::new(&w());
    let notes = directory(&mut driver, "/notes");
    let log = Rc::new(RefCell::new(Log::default()));
    let mut recording = Recording {
        inner: &mut driver,
        log: Rc::clone(&log),
    };
    let mut cx = Context::new(&mut recording, header());
    let read = root_grant(
        &mut cx,
        HostInterface::DirReadBytes,
        notes,
        Some(files_rule("@read", "{kind: @suffix value: \".txt\"}")),
        None,
        Limits::default(),
    );
    let (result, due) = run(
        &mut cx,
        HostOp::FsReadFile,
        read,
        &[HostArgument::Path(&names(&["raw.bin"]))],
    )
    .expect("no fault");
    assert_eq!(result, err(HostOp::FsReadFile, "denied"));
    assert!(!due, "a refusal of step 3 appends nothing");
    let log = log.borrow();
    assert_eq!(log.calls, 0, "the driver was not called");
    assert!(log.asked.is_empty(), "the gate was not called");
    assert!(log.completions.is_empty(), "the virtual file was not read");
}
