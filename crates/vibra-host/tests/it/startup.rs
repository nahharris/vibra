//! Startup: admission, the one order, the three outcomes, the startup
//! reads, the record, and release (M2 step 28).
//!
//! `W` is a world with the directory `/notes` holding `a.txt`. `M` is the
//! plan of the guide: `@args` (`EnvReadArgs`), `@notes` (`DirReadText` at the
//! path in argument 0, read through `@args`, after `@args`, with a policy
//! reading `.txt` files below the leaf), and `@out` (optional `StdoutText`).
//! `O` is the plan `@a` (after `@b`), `@b`, `@z`, each a required
//! `StdoutText`.
//!
//! [`Rig`] is the world's driver with a script on top: it logs every
//! acquisition, release, and call, and can refuse, fail, or panic at an
//! acquisition, lose a provider's identity, answer a call with a given
//! completion, or panic at its first release.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use sha2::{Digest, Sha256};
use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_host::broker::Context;
use vibra_host::driver::{
    Acquire, Action, Completion, Driver, DriverIdentity, Gate, Progress, Resolved,
    ResourceId,
};
use vibra_host::policy::{self, Policy};
use vibra_host::recipe::{
    self, BindingPlan, Leaf, LeafName, Limits, Problem, Provider, Resource,
};
use vibra_host::startup::{
    self, AdmissionRefusal, BindingStatus, EntryLeaf, EntryShape, Nonconformance,
    OutsideCeiling, RecordStatus, StartupRecord, admit, release, start,
};
use vibra_host::trace::{
    AuditTrace, Event, FailureCode, GrantOrigin, Phase, StartupStatus,
    declaration_identity,
};
use vibra_host::world::{HostWorld, WorldDriver, decode_world};
use vibra_ir::authority::{
    GrantLimits, GrantToken, HostArgument, HostContext, HostErrorCase, HostFault,
    HostOk, HostReply, HostRequest, NarrowRequest, Narrowed,
};
use vibra_ir::external::host::{HostInterface, HostOp};
use vibra_ir::{EntityRef, Row, Value};

use crate::trace::header;

// --- The rig -------------------------------------------------------------

/// One thing the rig saw.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Seen {
    Acquire(Provider, Resolved),
    Acquired(ResourceId),
    Release(ResourceId),
    Call(HostOp),
}

/// What the rig answers at an acquisition instead of the world.
#[derive(Clone, Debug)]
enum Answer {
    Refuse,
    Fail(&'static str),
    Panic,
}

#[derive(Default)]
struct Script {
    log: Vec<Seen>,
    acquires: usize,
    /// By the number of the acquisition, from 1.
    at: BTreeMap<usize, Answer>,
    by_provider: BTreeMap<Provider, Answer>,
    no_identity: BTreeSet<Provider>,
    /// After acquiring for this provider, its identity is `None`.
    lose_identity: Option<Provider>,
    replies: BTreeMap<HostOp, Completion>,
    panic_at_release: bool,
}

struct Rig {
    world: WorldDriver,
    script: Rc<RefCell<Script>>,
}

impl Rig {
    fn new(world: &HostWorld) -> Self {
        Self {
            world: WorldDriver::new(world),
            script: Rc::default(),
        }
    }

    fn script(&self) -> Rc<RefCell<Script>> {
        Rc::clone(&self.script)
    }
}

impl Script {
    fn acquired(&self) -> Vec<ResourceId> {
        self.log
            .iter()
            .filter_map(|seen| match seen {
                Seen::Acquired(resource) => Some(*resource),
                _ => None,
            })
            .collect()
    }

    fn released(&self) -> Vec<ResourceId> {
        self.log
            .iter()
            .filter_map(|seen| match seen {
                Seen::Release(resource) => Some(*resource),
                _ => None,
            })
            .collect()
    }

    fn acquisitions(&self) -> usize {
        self.log
            .iter()
            .filter(|seen| matches!(seen, Seen::Acquire(..)))
            .count()
    }
}

impl Driver for Rig {
    fn identity(&self, provider: Provider) -> Option<DriverIdentity> {
        if self.script.borrow().no_identity.contains(&provider) {
            return None;
        }
        self.world.identity(provider)
    }

    fn acquire(&mut self, provider: Provider, resource: &Resolved) -> Acquire {
        let answer = {
            let mut script = self.script.borrow_mut();
            script.log.push(Seen::Acquire(provider, resource.clone()));
            script.acquires += 1;
            let number = script.acquires;
            script
                .at
                .get(&number)
                .or_else(|| script.by_provider.get(&provider))
                .cloned()
        };
        match answer {
            Some(Answer::Refuse) => return Acquire::Refused,
            Some(Answer::Fail(why)) => return Acquire::Failed(why.to_owned()),
            Some(Answer::Panic) => panic!("the driver panics inside `acquire`"),
            None => {}
        }
        let acquired = self.world.acquire(provider, resource);
        let mut script = self.script.borrow_mut();
        if let Acquire::Acquired(id) = acquired {
            script.log.push(Seen::Acquired(id));
        }
        if script.lose_identity == Some(provider) {
            script.no_identity.insert(provider);
        }
        acquired
    }

    fn release(&mut self, resource: ResourceId) {
        let panic = {
            let mut script = self.script.borrow_mut();
            script.log.push(Seen::Release(resource));
            std::mem::take(&mut script.panic_at_release)
        };
        assert!(!panic, "the driver panics inside `release`");
        self.world.release(resource);
    }

    fn call(
        &mut self,
        resource: ResourceId,
        action: &Action<'_>,
        gate: &mut dyn Gate,
    ) -> Completion {
        let reply = {
            let mut script = self.script.borrow_mut();
            script.log.push(Seen::Call(action.op));
            script.replies.remove(&action.op)
        };
        reply.unwrap_or_else(|| self.world.call(resource, action, gate))
    }
}

// --- Worlds, plans, and shapes -------------------------------------------

/// `W` with `args` and the variables `env`.
fn world(args: &[&str], env: &[(&str, &str)]) -> HostWorld {
    let mut text = String::from(
        "{format: @hostWorld.v2 directories: [\"/notes\"] files: [\"/notes/a.txt\": \"hello\"]",
    );
    if !env.is_empty() {
        let entries = env
            .iter()
            .map(|(name, value)| format!("\"{name}\": \"{value}\""))
            .collect::<Vec<_>>();
        text.push_str(&format!(" env: [{}]", entries.join(" ")));
    }
    if !args.is_empty() {
        let args = args
            .iter()
            .map(|arg| format!("\"{arg}\""))
            .collect::<Vec<_>>();
        text.push_str(&format!(" args: [{}]", args.join(" ")));
    }
    text.push('}');
    decode_world(Some("world.vibon"), text.as_bytes()).expect("a valid world")
}

fn w() -> HostWorld {
    world(&["/notes"], &[])
}

fn name(atom: &str) -> LeafName {
    LeafName::new(atom)
}

/// The policy of a directory leaf `root`: `.txt` files below it, read.
fn notes_policy(root: &str) -> Policy {
    policy::decode(
        format!(
            "{{format: @scopePolicy.v1 family: @filesystem expression: {{kind: @fileRule \
             root: {root} prefix: [] depth: @tree actions: [@read] names: {{kind: @suffix \
             value: \".txt\"}}}}}}"
        )
        .as_bytes(),
    )
    .expect("a policy")
}

#[expect(clippy::too_many_arguments, reason = "one field of a recipe each")]
fn leaf(
    atom: &str,
    interface: HostInterface,
    optional: bool,
    provider: Provider,
    resource: Resource,
    policy: Option<Policy>,
    limits: Limits,
    after: &[&str],
) -> Leaf {
    let after = after.iter().map(|atom| name(atom)).collect::<Vec<_>>();
    let recipe_identity =
        recipe::recipe_identity(provider, &resource, policy.as_ref(), &limits, &after);
    // Each recipe has its own span: the leaf's spelling's length, after its
    // first byte, which no two leaves of a test share.
    let start = atom.bytes().map(usize::from).sum::<usize>();
    Leaf {
        name: name(atom),
        interface,
        optional,
        provider,
        resource,
        policy,
        limits,
        after,
        recipe_identity,
        span: ByteSpan::new(start, start + atom.len()),
    }
}

fn args_leaf(optional: bool, limits: Limits) -> Leaf {
    leaf(
        "@args",
        HostInterface::EnvReadArgs,
        optional,
        Provider::EnvArgs,
        Resource::ProcessArguments,
        None,
        limits,
        &[],
    )
}

/// A `DirReadText` leaf at the path in argument `index`, read through
/// `@args`.
fn argument_leaf(atom: &str, optional: bool, index: u64) -> Leaf {
    leaf(
        atom,
        HostInterface::DirReadText,
        optional,
        Provider::FsDirectory,
        Resource::ArgumentPath {
            index,
            source: name("@args"),
        },
        Some(notes_policy(atom)),
        Limits::default(),
        &["@args"],
    )
}

fn notes_leaf(optional: bool) -> Leaf {
    argument_leaf("@notes", optional, 0)
}

fn stdout_leaf(atom: &str, optional: bool, after: &[&str]) -> Leaf {
    leaf(
        atom,
        HostInterface::StdoutText,
        optional,
        Provider::IoStdout,
        Resource::ProcessStream,
        None,
        Limits::default(),
        after,
    )
}

fn plan(leaves: Vec<Leaf>) -> BindingPlan {
    BindingPlan {
        target: "@app".to_owned(),
        leaves,
    }
}

/// `M`, with `@notes` optional or not and the limits of `@args`.
fn m_with(notes_optional: bool, args_limits: Limits) -> BindingPlan {
    plan(vec![
        args_leaf(false, args_limits),
        notes_leaf(notes_optional),
        stdout_leaf("@out", true, &[]),
    ])
}

fn m() -> BindingPlan {
    m_with(false, Limits::default())
}

/// `M` with `@notes` read from the variable `NOTES` through `@vars`.
fn m_variable(notes_optional: bool) -> BindingPlan {
    plan(vec![
        leaf(
            "@vars",
            HostInterface::EnvReadVar,
            false,
            Provider::EnvVars,
            Resource::VariableSet {
                names: vec!["NOTES".to_owned()],
            },
            None,
            Limits::default(),
            &[],
        ),
        leaf(
            "@notes",
            HostInterface::DirReadText,
            notes_optional,
            Provider::FsDirectory,
            Resource::VariablePath {
                name: "NOTES".to_owned(),
                source: name("@vars"),
            },
            Some(notes_policy("@notes")),
            Limits::default(),
            &["@vars"],
        ),
        stdout_leaf("@out", true, &[]),
    ])
}

fn o_leaves() -> Vec<Leaf> {
    vec![
        stdout_leaf("@a", false, &["@b"]),
        stdout_leaf("@b", false, &[]),
        stdout_leaf("@z", false, &[]),
    ]
}

fn o() -> BindingPlan {
    plan(o_leaves())
}

fn entity(interface: HostInterface) -> EntityRef {
    let owner = interface.owner();
    EntityRef::new(
        owner.path().trim_start_matches('@'),
        declaration_identity(
            "std",
            "0.0.2",
            owner.module,
            "",
            "@interface",
            owner.name,
        ),
    )
    .expect("an entity reference")
}

fn row(interfaces: &[HostInterface]) -> Row {
    Row::new(interfaces.iter().copied().map(entity)).expect("a row")
}

/// The shape of an entry that takes exactly the plan's leaves and performs
/// each leaf's interface.
fn shape(plan: &BindingPlan) -> EntryShape {
    EntryShape {
        row: row(&plan
            .leaves
            .iter()
            .map(|leaf| leaf.interface)
            .collect::<Vec<_>>()),
        leaves: plan
            .leaves
            .iter()
            .map(|leaf| EntryLeaf {
                name: leaf.name.clone(),
                interface: leaf.interface,
                optional: leaf.optional,
            })
            .collect(),
    }
}

/// The ceiling of the three interfaces of `M`.
fn m_ceiling() -> Row {
    row(&[
        HostInterface::EnvReadArgs,
        HostInterface::DirReadText,
        HostInterface::StdoutText,
    ])
}

fn every_interface() -> Row {
    row(&HostInterface::ALL)
}

// --- Running -------------------------------------------------------------

/// What one start and release left.
struct Ran {
    started: StartupRecord,
    bindings: Option<Vec<(LeafName, Option<GrantToken>)>>,
    next_invocation: u64,
    fault: Option<HostFault>,
    released: StartupRecord,
    trace: AuditTrace,
    uncommitted: usize,
}

/// Admits `plan` with its own shape under every interface, starts it on
/// `rig`, and releases it.
fn run(plan: &BindingPlan, rig: &mut Rig) -> Ran {
    let shape = shape(plan);
    let admission = admit(plan, &shape, &every_interface(), &*rig).expect("admitted");
    let mut cx = Context::new(rig, header());
    let started = start(admission, &mut cx);
    let (record, bindings, next_invocation, fault) = (
        started.record.clone(),
        started.bindings.clone(),
        started.next_invocation,
        started.fault.clone(),
    );
    let released = release(started);
    Ran {
        started: record,
        bindings,
        next_invocation,
        fault,
        released,
        trace: cx.trace().clone(),
        uncommitted: cx.uncommitted(),
    }
}

/// Each event, in one line: `@startup <leaf> <status>` or
/// `@host <invocation> <parent> <phase> <op>`.
fn summary(trace: &AuditTrace) -> Vec<String> {
    trace
        .events
        .iter()
        .map(|event| match event {
            Event::Startup(event) => {
                format!("@startup {} {}", event.binding, event.status.atom())
            }
            Event::Host(event) => format!(
                "@host {} {:?} {} {}",
                event.invocation,
                event.parent,
                event.phase.atom(),
                event.op.symbol()
            ),
            Event::Opaque(event) => event.kind().to_owned(),
        })
        .collect()
}

fn startup_event<'t>(
    trace: &'t AuditTrace,
    leaf: &str,
) -> &'t vibra_host::trace::StartupEvent {
    trace
        .events
        .iter()
        .find_map(|event| match event {
            Event::Startup(event) if event.binding.as_str() == leaf => Some(event),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a startup event for {leaf}"))
}

fn has_startup_event(trace: &AuditTrace, leaf: &str) -> bool {
    trace.events.iter().any(|event| {
        matches!(event, Event::Startup(event) if event.binding.as_str() == leaf)
    })
}

fn processed(record: &StartupRecord) -> Vec<String> {
    record
        .bindings
        .iter()
        .map(|binding| binding.name.as_str().to_owned())
        .collect()
}

fn status_of(record: &StartupRecord, leaf: &str) -> BindingStatus {
    record
        .bindings
        .iter()
        .find(|binding| binding.name.as_str() == leaf)
        .map(|binding| binding.status)
        .unwrap_or_else(|| panic!("a binding record for {leaf}"))
}

fn token(
    bindings: Option<&[(LeafName, Option<GrantToken>)]>,
    leaf: &str,
) -> GrantToken {
    bindings
        .expect("startup acquired")
        .iter()
        .find(|(name, _)| name.as_str() == leaf)
        .and_then(|(_, grant)| *grant)
        .unwrap_or_else(|| panic!("a grant for {leaf}"))
}

fn request<'v>(
    op: HostOp,
    grant: GrantToken,
    arguments: &'v [HostArgument<'v>],
    invocation: u64,
) -> HostRequest<'v> {
    HostRequest {
        op,
        grant,
        arguments,
        invocation,
        parent: Some(invocation - 1),
        origin: None,
        room: u64::MAX,
    }
}

fn completed(reply: HostReply) -> Completion {
    Completion::Completed {
        reply,
        progress: Progress::NONE,
    }
}

fn err(op: HostOp, case: &str) -> HostReply {
    HostReply::Err(HostErrorCase::new(op.row().error, case).expect("a case of the row"))
}

fn invalid<T>(result: &Result<T, HostFault>) -> bool {
    matches!(result, Err(HostFault::InvalidHostValue(_)))
}

// --- Start ---------------------------------------------------------------

#[test]
fn all_acquired() {
    let mut rig = Rig::new(&w());
    let ran = run(&m(), &mut rig);
    assert_eq!(ran.started.status, RecordStatus::Acquired);
    assert_eq!(ran.started.failure, None);
    assert_eq!(ran.fault, None);
    assert_eq!(
        summary(&ran.trace),
        [
            "@startup @args @acquired",
            "@host 1 None @startup env.readArgs",
            "@startup @notes @acquired",
            "@startup @out @acquired",
        ]
    );
    assert_eq!(
        ran.started.events, ran.trace.events,
        "the record holds them"
    );
    let bindings = ran.bindings.expect("arguments");
    assert!(bindings.iter().all(|(_, grant)| grant.is_some()));
    let Some(Event::Host(read)) = ran.trace.events.get(1) else {
        panic!("a host event");
    };
    assert_eq!(read.phase, Phase::Startup);
    assert_eq!(read.parent, None);
    assert_eq!(read.origin, None);
}

#[test]
fn order_by_spelling() {
    let plan = plan(vec![
        stdout_leaf("@b", false, &[]),
        stdout_leaf("@a", false, &[]),
        stdout_leaf("@logs.reader", false, &[]),
    ]);
    let ran = run(&plan, &mut Rig::new(&w()));
    assert_eq!(processed(&ran.started), ["@a", "@b", "@logs.reader"]);
}

#[test]
fn v2_grant_startup_order_permuted() {
    let orders = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for order in orders {
        let leaves = o_leaves();
        let permuted = order.iter().map(|index| leaves[*index].clone()).collect();
        let ran = run(&plan(permuted), &mut Rig::new(&w()));
        assert_eq!(processed(&ran.started), ["@b", "@a", "@z"], "{order:?}");
        assert_eq!(
            summary(&ran.trace),
            [
                "@startup @b @acquired",
                "@startup @a @acquired",
                "@startup @z @acquired"
            ],
            "{order:?}"
        );
    }
}

#[test]
fn refused_optional() {
    let mut rig = Rig::new(&w());
    rig.script
        .borrow_mut()
        .by_provider
        .insert(Provider::IoStdout, Answer::Refuse);
    let ran = run(&m(), &mut rig);
    assert_eq!(ran.started.status, RecordStatus::Acquired);
    assert_eq!(ran.started.failure, None);
    let out = startup_event(&ran.trace, "@out");
    assert_eq!(out.status, StartupStatus::Refused);
    assert_eq!(out.failure, None);
    assert_eq!(out.grant, None);
    assert_eq!(status_of(&ran.started, "@out"), BindingStatus::Refused);
    let bindings = ran.bindings.expect("startup continued");
    assert!(
        bindings.contains(&(name("@out"), None)),
        "the argument is absent"
    );
}

#[test]
fn refused_required() {
    let mut rig = Rig::new(&world(&["/missing"], &[]));
    let plan = m();
    let ran = run(&plan, &mut rig);
    assert_eq!(ran.started.status, RecordStatus::Failed);
    let failure = ran.started.failure.as_ref().expect("a failure");
    assert_eq!(failure.binding, name("@notes"));
    assert_eq!(failure.code, FailureCode::RequiredUnavailable);
    assert_eq!(failure.span, plan.leaves[1].span, "the recipe's span");
    let notes = startup_event(&ran.trace, "@notes");
    assert_eq!(notes.status, StartupStatus::Refused);
    assert_eq!(
        notes.failure.as_ref().map(|failure| failure.code),
        Some(FailureCode::RequiredUnavailable)
    );
    assert!(!has_startup_event(&ran.trace, "@out"), "never reached");
    assert_eq!(ran.bindings, None);
}

#[test]
fn each_directory_refusal() {
    // The path names nothing; names a file; the index is beyond the
    // arguments.
    for args in [&["/missing"][..], &["/notes/a.txt"], &[]] {
        let ran = run(
            &m_with(true, Limits::default()),
            &mut Rig::new(&world(args, &[])),
        );
        let notes = startup_event(&ran.trace, "@notes");
        assert_eq!(notes.status, StartupStatus::Refused, "{args:?}");
        assert_eq!(notes.failure, None, "{args:?}");
        assert_eq!(ran.started.status, RecordStatus::Acquired, "{args:?}");
    }
    // The variable is not present.
    let ran = run(&m_variable(true), &mut Rig::new(&world(&[], &[])));
    assert_eq!(
        startup_event(&ran.trace, "@notes").status,
        StartupStatus::Refused
    );
    assert!(
        summary(&ran.trace).contains(&"@host 1 None @startup env.readVar".to_owned()),
        "the completed read is recorded"
    );
}

#[test]
fn v2_grant_startup_failed_optional() {
    let mut rig = Rig::new(&w());
    rig.script
        .borrow_mut()
        .by_provider
        .insert(Provider::FsDirectory, Answer::Fail("io"));
    let ran = run(&m_with(true, Limits::default()), &mut rig);
    assert_eq!(ran.started.status, RecordStatus::Failed);
    let failure = ran.started.failure.as_ref().expect("a failure");
    assert_eq!(failure.code, FailureCode::AcquisitionFailed);
    assert_eq!(failure.binding, name("@notes"));
    let notes = startup_event(&ran.trace, "@notes");
    assert_eq!(notes.status, StartupStatus::Failed);
    assert_eq!(
        notes
            .failure
            .as_ref()
            .map(|failure| failure.message.as_str()),
        Some("io")
    );
}

#[test]
fn atk_startup_failure_not_none() {
    let mut rig = Rig::new(&w());
    rig.script
        .borrow_mut()
        .by_provider
        .insert(Provider::FsDirectory, Answer::Fail("io"));
    let ran = run(&m_with(true, Limits::default()), &mut rig);
    assert_eq!(ran.bindings, None, "no entry argument was built");
    assert_eq!(status_of(&ran.started, "@notes"), BindingStatus::Failed);
    assert!(!has_startup_event(&ran.trace, "@out"));
}

#[test]
fn a_refusal_of_a_never_refusing_issuer_is_a_failure() {
    let mut rig = Rig::new(&w());
    rig.script
        .borrow_mut()
        .by_provider
        .insert(Provider::EnvArgs, Answer::Refuse);
    let plan = plan(vec![
        args_leaf(true, Limits::default()),
        stdout_leaf("@out", true, &[]),
    ]);
    let ran = run(&plan, &mut rig);
    assert_eq!(
        startup_event(&ran.trace, "@args").status,
        StartupStatus::Failed
    );
    assert_eq!(
        ran.started.failure.map(|failure| failure.code),
        Some(FailureCode::AcquisitionFailed)
    );
}

/// 04 "Startup order": no source provider can refuse, so a source leaf is
/// never bound absent. An optional `@args` whose issuer refuses anyway is a
/// failure that stops startup there: the optional `@notes` that reads its
/// path through `@args` is never processed, so it is neither refused nor
/// bound absent.
#[test]
fn a_source_leaf_is_never_bound_absent() {
    let mut rig = Rig::new(&w());
    rig.script
        .borrow_mut()
        .by_provider
        .insert(Provider::EnvArgs, Answer::Refuse);
    let plan = plan(vec![args_leaf(true, Limits::default()), notes_leaf(true)]);
    let ran = run(&plan, &mut rig);
    assert_eq!(status_of(&ran.started, "@args"), BindingStatus::Failed);
    assert!(!has_startup_event(&ran.trace, "@notes"));
    assert_eq!(processed(&ran.started), ["@args"]);
    let failure = ran.started.failure.expect("a failure");
    assert_eq!(failure.binding, name("@args"));
    assert_eq!(failure.code, FailureCode::AcquisitionFailed);
    assert_eq!(ran.bindings, None);
}

#[test]
fn unlisted_read_error() {
    let mut rig = Rig::new(&world(&[], &[("NOTES", "/notes")]));
    rig.script.borrow_mut().replies.insert(
        HostOp::EnvReadVar,
        completed(err(HostOp::EnvReadVar, "outcomeUnknown")),
    );
    let ran = run(&m_variable(true), &mut rig);
    assert_eq!(
        startup_event(&ran.trace, "@notes").status,
        StartupStatus::Failed
    );
    assert_eq!(
        ran.started.failure.map(|failure| failure.code),
        Some(FailureCode::AcquisitionFailed)
    );
}

#[test]
fn a_variable_that_is_not_unicode_is_a_refusal() {
    // 04 lists "the path is not Unicode" as a refusal of the directory row.
    let mut rig = Rig::new(&world(&[], &[("NOTES", "/notes")]));
    rig.script.borrow_mut().replies.insert(
        HostOp::EnvReadVar,
        completed(err(HostOp::EnvReadVar, "notUnicode")),
    );
    let ran = run(&m_variable(true), &mut rig);
    assert_eq!(
        startup_event(&ran.trace, "@notes").status,
        StartupStatus::Refused
    );
    assert_eq!(ran.started.status, RecordStatus::Acquired);
}

#[test]
fn arguments_that_are_not_unicode_are_a_failure() {
    let mut rig = Rig::new(&w());
    rig.script.borrow_mut().replies.insert(
        HostOp::EnvReadArgs,
        completed(err(HostOp::EnvReadArgs, "notUnicode")),
    );
    let ran = run(&m_with(true, Limits::default()), &mut rig);
    assert_eq!(
        startup_event(&ran.trace, "@notes").status,
        StartupStatus::Failed
    );
}

#[test]
fn v2_grant_startup_read_counts() {
    let plan = m_with(
        false,
        Limits {
            invocations: Some(1),
            ..Limits::default()
        },
    );
    let shape = shape(&plan);
    let mut rig = Rig::new(&w());
    let admission = admit(&plan, &shape, &m_ceiling(), &rig).expect("admitted");
    let mut cx = Context::new(&mut rig, header());
    let mut started = start(admission, &mut cx);
    assert_eq!(started.record.status, RecordStatus::Acquired);
    let args = token(started.bindings.as_deref(), "@args");
    let next = started.next_invocation;
    let reply = started
        .context()
        .host(request(HostOp::EnvReadArgs, args, &[], next + 1))
        .expect("a reply");
    assert_eq!(reply.reply, err(HostOp::EnvReadArgs, "limitExceeded"));
    assert!(reply.receipt.is_none());
    let _ = release(started);
}

#[test]
fn v2_grant_startup_read_limit_fails() {
    let plan = plan(vec![
        args_leaf(
            false,
            Limits {
                invocations: Some(1),
                ..Limits::default()
            },
        ),
        argument_leaf("@logs", false, 0),
        notes_leaf(false),
        stdout_leaf("@out", true, &[]),
    ]);
    let ran = run(&plan, &mut Rig::new(&w()));
    assert_eq!(
        summary(&ran.trace),
        [
            "@startup @args @acquired",
            "@host 1 None @startup env.readArgs",
            "@startup @logs @acquired",
            "@startup @notes @failed",
        ]
    );
    let failure = ran.started.failure.expect("a failure");
    assert_eq!(failure.binding, name("@notes"));
    assert_eq!(failure.code, FailureCode::AcquisitionFailed);
}

#[test]
fn startup_invocations() {
    let plan = plan(vec![
        args_leaf(false, Limits::default()),
        argument_leaf("@logs", false, 0),
        notes_leaf(false),
        stdout_leaf("@out", true, &[]),
    ]);
    let ran = run(&plan, &mut Rig::new(&w()));
    let reads = ran
        .trace
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Host(event) => Some((event.invocation, event.parent, event.phase)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        reads,
        [(1, None, Phase::Startup), (2, None, Phase::Startup)]
    );
    assert_eq!(ran.next_invocation, 3);
}

#[test]
fn projection_of_a_leaf() {
    let ran = run(&m(), &mut Rig::new(&w()));
    let notes = startup_event(&ran.trace, "@notes");
    let grant = notes.grant.as_ref().expect("a projection");
    assert_eq!(
        grant.issued_by,
        GrantOrigin::Binding {
            leaf: name("@notes")
        }
    );
    let policy = notes_policy("@notes");
    assert_eq!(grant.policy, policy::hash(&policy));
    assert_eq!(ran.trace.policies.get(&grant.policy), Some(&policy));
    assert_eq!(grant.rights, entity(HostInterface::DirReadText));
    assert_eq!(grant.limits, None);
    assert_eq!(grant.root, 2, "@args is root 1, @notes root 2");
    assert_eq!(
        startup_event(&ran.trace, "@out")
            .grant
            .as_ref()
            .map(|g| g.root),
        Some(3)
    );
    let record = ran
        .started
        .bindings
        .iter()
        .find(|binding| binding.name == name("@notes"))
        .expect("a binding");
    assert_eq!(record.grant_projection.as_ref(), Some(grant));
}

/// `sha256:` and the SHA-256 of the recipe identity's tag, a zero byte, and
/// `recipe`, written by hand.
fn identity_of(recipe: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"vibraRecipeIdentityV2\0");
    hasher.update(recipe.as_bytes());
    let digest = hasher.finalize();
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hex}")
}

#[test]
fn recipe_identity_of_each_binding() {
    let plan = m();
    let ran = run(&plan, &mut Rig::new(&w()));
    let identity = |leaf: &str| {
        ran.started
            .bindings
            .iter()
            .find(|binding| binding.name.as_str() == leaf)
            .map(|binding| binding.recipe_identity.clone())
            .expect("a binding")
    };
    assert_eq!(
        identity("@args"),
        identity_of(
            "{provider: @host.env.args resource: {kind: @processArguments} after: []}"
        )
    );
    assert_eq!(
        identity("@out"),
        identity_of(
            "{provider: @host.io.stdout resource: {kind: @processStream} after: []}"
        )
    );
    let notes = &plan.leaves[1];
    assert_eq!(
        identity("@notes"),
        recipe::recipe_identity(
            notes.provider,
            &notes.resource,
            notes.policy.as_ref(),
            &notes.limits,
            &notes.after
        )
    );
    for binding in &ran.started.bindings {
        assert_eq!(
            binding.provider_identity,
            DriverIdentity {
                provider: "@hostWorld.v2".to_owned(),
                version: "0.0.2".to_owned()
            }
        );
    }
}

// --- Admission -----------------------------------------------------------

#[test]
fn atk_admit_entry_row_outside_ceiling() {
    let plan = m();
    let mut shape = shape(&plan);
    shape.row = row(&[
        HostInterface::EnvReadArgs,
        HostInterface::DirReadText,
        HostInterface::StdoutText,
        HostInterface::StderrText,
    ]);
    let rig = Rig::new(&w());
    let refusal = admit(&plan, &shape, &m_ceiling(), &rig).expect_err("refused");
    assert_eq!(
        refusal,
        AdmissionRefusal::OutsideCeiling(vec![OutsideCeiling {
            root: "std.io.StderrText".to_owned(),
            step: None,
        }])
    );
    assert_eq!(refusal.code(), DiagnosticCode::EffectOutsideCeiling);
    assert_eq!(rig.script.borrow().acquisitions(), 0);
}

#[test]
fn atk_admit_startup_read_outside_ceiling() {
    let plan = m();
    let mut shape = shape(&plan);
    let lacking = row(&[HostInterface::DirReadText, HostInterface::StdoutText]);
    shape.row = lacking.clone();
    let rig = Rig::new(&w());
    let refusal = admit(&plan, &shape, &lacking, &rig).expect_err("refused");
    assert_eq!(
        refusal,
        AdmissionRefusal::OutsideCeiling(vec![OutsideCeiling {
            root: "std.env.EnvReadArgs".to_owned(),
            step: Some(name("@notes")),
        }])
    );
    assert_eq!(rig.script.borrow().acquisitions(), 0);
}

#[test]
fn an_unused_grant_is_admitted() {
    let plan = plan(vec![stdout_leaf("@out", false, &[])]);
    let mut shape = shape(&plan);
    shape.row = Row::empty();
    let rig = Rig::new(&w());
    assert!(admit(&plan, &shape, &Row::empty(), &rig).is_ok());
}

#[test]
fn atk_admit_leaf_interface_mismatch() {
    let plan = m();
    let mut shape = shape(&plan);
    shape.leaves[1].interface = HostInterface::DirWriteText;
    let rig = Rig::new(&w());
    let refusal = admit(&plan, &shape, &m_ceiling(), &rig).expect_err("refused");
    assert_eq!(
        refusal,
        AdmissionRefusal::Nonconforming(vec![Nonconformance::Interface {
            leaf: name("@notes"),
            plan: HostInterface::DirReadText,
            entry: HostInterface::DirWriteText,
        }])
    );
    assert_eq!(refusal.code(), DiagnosticCode::GrantInvalidBinding);
    assert_eq!(rig.script.borrow().acquisitions(), 0);
}

#[test]
fn admission_order() {
    let plan = m();
    let mut shape = shape(&plan);
    shape.leaves[1].interface = HostInterface::DirWriteText;
    shape.row = row(&[
        HostInterface::EnvReadArgs,
        HostInterface::DirReadText,
        HostInterface::StdoutText,
        HostInterface::StderrText,
    ]);
    let rig = Rig::new(&w());
    let refusal = admit(&plan, &shape, &m_ceiling(), &rig).expect_err("refused");
    assert!(
        matches!(refusal, AdmissionRefusal::Nonconforming(_)),
        "{refusal:?}"
    );
}

#[test]
fn admission_finds_missing_extra_and_optionality() {
    let plan = m();
    let mut shape = shape(&plan);
    shape.leaves[2].optional = false;
    shape.leaves[0].name = name("@argv");
    let rig = Rig::new(&w());
    let refusal = admit(&plan, &shape, &m_ceiling(), &rig).expect_err("refused");
    assert_eq!(
        refusal,
        AdmissionRefusal::Nonconforming(vec![
            Nonconformance::Missing(name("@argv")),
            Nonconformance::Optionality {
                leaf: name("@out"),
                plan: true,
                entry: false,
            },
            Nonconformance::Extra(name("@args")),
        ])
    );
}

#[test]
fn atk_plan_provider_mismatch() {
    let mut plan = m();
    plan.leaves[1].provider = Provider::IoStdout;
    let shape = shape(&plan);
    let rig = Rig::new(&w());
    let refusal = admit(&plan, &shape, &m_ceiling(), &rig).expect_err("refused");
    let AdmissionRefusal::InvalidPlan(faults) = &refusal else {
        panic!("an invalid plan: {refusal:?}");
    };
    assert!(
        faults.iter().any(|fault| fault.leaf == name("@notes")
            && fault.problem == Problem::TypeNotIssued),
        "{faults:?}"
    );
    assert_eq!(refusal.code(), DiagnosticCode::GrantInvalidBinding);
    assert_eq!(rig.script.borrow().acquisitions(), 0);
}

#[test]
fn admission_without_a_driver() {
    let plan = m();
    let shape = shape(&plan);
    let rig = Rig::new(&w());
    rig.script
        .borrow_mut()
        .no_identity
        .insert(Provider::FsDirectory);
    let refusal = admit(&plan, &shape, &m_ceiling(), &rig).expect_err("refused");
    assert_eq!(
        refusal,
        AdmissionRefusal::Unavailable(vec![Provider::FsDirectory])
    );
    assert_eq!(refusal.code(), DiagnosticCode::ToolUnavailable);
    assert_eq!(rig.script.borrow().acquisitions(), 0);
}

// --- Release -------------------------------------------------------------

#[test]
fn v2_grant_release_order() {
    let mut rig = Rig::new(&w());
    let script = rig.script();
    let ran = run(&o(), &mut rig);
    assert_eq!(processed(&ran.started), ["@b", "@a", "@z"]);
    let script = script.borrow();
    let mut acquired = script.acquired();
    assert_eq!(acquired.len(), 3);
    acquired.reverse();
    assert_eq!(script.released(), acquired, "@z, @a, @b");
}

#[test]
fn release_after_a_failed_startup() {
    let mut rig = Rig::new(&w());
    rig.script.borrow_mut().at.insert(3, Answer::Fail("io"));
    let script = rig.script();
    let ran = run(&o(), &mut rig);
    assert_eq!(ran.started.status, RecordStatus::Failed);
    let script = script.borrow();
    let mut acquired = script.acquired();
    assert_eq!(acquired.len(), 2);
    acquired.reverse();
    assert_eq!(script.released(), acquired);
    assert_eq!(status_of(&ran.released, "@b"), BindingStatus::Released);
    assert_eq!(status_of(&ran.released, "@a"), BindingStatus::Released);
    assert_eq!(status_of(&ran.released, "@z"), BindingStatus::Failed);
}

#[test]
fn v2_grant_release_status() {
    let ran = run(&m(), &mut Rig::new(&w()));
    assert!(
        ran.started
            .bindings
            .iter()
            .all(|binding| binding.status == BindingStatus::Acquired)
    );
    assert!(
        ran.released
            .bindings
            .iter()
            .all(|binding| binding.status == BindingStatus::Released)
    );
    // The `@startup` events keep `@acquired`.
    assert!(ran.trace.events.iter().all(|event| match event {
        Event::Startup(event) => event.status == StartupStatus::Acquired,
        _ => true,
    }));
}

#[test]
fn atk_released_grant() {
    let plan = m();
    let shape = shape(&plan);
    let mut rig = Rig::new(&w());
    let admission = admit(&plan, &shape, &m_ceiling(), &rig).expect("admitted");
    let mut cx = Context::new(&mut rig, header());
    let started = start(admission, &mut cx);
    let out = token(started.bindings.as_deref(), "@out");
    let _ = release(started);
    let used = cx.host(request(
        HostOp::IoStdoutText,
        out,
        &[HostArgument::Text("x")],
        3,
    ));
    assert!(invalid(&used), "{used:?}");
}

fn unit(case: &str) -> Value {
    Value::Variant {
        case: case.to_owned(),
        fields: Vec::new(),
    }
}

/// The source policy `files {prefix: [] depth: tree actions: [read] names:
/// anyName}`.
fn read_anything() -> Value {
    Value::Variant {
        case: "files".to_owned(),
        fields: vec![
            (
                "prefix".to_owned(),
                Value::Wrapper(Box::new(Value::List(Vec::new()))),
            ),
            ("depth".to_owned(), unit("tree")),
            ("actions".to_owned(), Value::List(vec![unit("read")])),
            ("names".to_owned(), unit("anyName")),
        ],
    }
}

fn narrowed(result: Result<Narrowed, HostFault>) -> GrantToken {
    match result.expect("a narrowing") {
        Narrowed::Grant(token) => token,
        Narrowed::Refused(refusal) => panic!("refused: {refusal:?}"),
    }
}

#[test]
fn atk_released_derived_grant() {
    let plan = m();
    let shape = shape(&plan);
    let mut rig = Rig::new(&w());
    let script = rig.script();
    let admission = admit(&plan, &shape, &m_ceiling(), &rig).expect("admitted");
    let mut cx = Context::new(&mut rig, header());
    let mut started = start(admission, &mut cx);
    let notes = token(started.bindings.as_deref(), "@notes");
    let policy = read_anything();
    let prefix = vec!["sub".to_owned()];
    let context = started.context();
    let derived = [
        narrowed(context.narrow(NarrowRequest::Restrict {
            grant: notes,
            policy: &policy,
        })),
        narrowed(context.narrow(NarrowRequest::Sub {
            grant: notes,
            prefix: &prefix,
        })),
        narrowed(context.narrow(NarrowRequest::Limit {
            grant: notes,
            limits: GrantLimits {
                invocations: Some(5),
                ..GrantLimits::default()
            },
        })),
    ];
    let _ = release(started);
    let at = vec!["a.txt".to_owned()];
    for grant in derived {
        let used = cx.host(request(
            HostOp::FsReadText,
            grant,
            &[HostArgument::Path(&at)],
            3,
        ));
        assert!(invalid(&used), "{used:?}");
    }
    let log = script.borrow().log.clone();
    let first_release = log
        .iter()
        .position(|seen| matches!(seen, Seen::Release(_)))
        .expect("a release");
    assert!(
        log.iter()
            .skip(first_release)
            .all(|seen| !matches!(seen, Seen::Call(_))),
        "the driver saw no call after its release: {log:?}"
    );
}

#[test]
fn release_invalidates_first() {
    // The driver panics inside its first `release`, so what the context
    // shows afterwards is what it was when the driver was called.
    let plan = m();
    let shape = shape(&plan);
    let mut rig = Rig::new(&w());
    let script = rig.script();
    let admission = admit(&plan, &shape, &m_ceiling(), &rig).expect("admitted");
    let mut cx = Context::new(&mut rig, header());
    let started = start(admission, &mut cx);
    let out = token(started.bindings.as_deref(), "@out");
    script.borrow_mut().panic_at_release = true;
    let released = catch_unwind(AssertUnwindSafe(move || release(started)));
    assert!(released.is_err(), "the driver panicked");
    let used = cx.host(request(
        HostOp::IoStdoutText,
        out,
        &[HostArgument::Text("x")],
        3,
    ));
    assert!(
        invalid(&used),
        "the lineage was dead before the driver was called: {used:?}"
    );
    let script = script.borrow();
    let mut acquired = script.acquired();
    acquired.reverse();
    assert_eq!(
        script.released(),
        acquired,
        "the rest were released on unwinding"
    );
}

#[test]
fn atk_acquire_then_fail_leaks_nothing() {
    // After the driver acquires `@notes`, it has no identity for the
    // directory provider, so `Context::issue` fails for it.
    let mut rig = Rig::new(&w());
    rig.script.borrow_mut().lose_identity = Some(Provider::FsDirectory);
    let script = rig.script();
    let ran = run(&m(), &mut rig);
    assert_eq!(ran.started.status, RecordStatus::Failed);
    assert!(
        matches!(ran.fault, Some(HostFault::InvalidHostValue(_))),
        "{:?}",
        ran.fault
    );
    assert_eq!(ran.started.failure, None, "no leaf stopped startup");
    let script = script.borrow();
    let acquired = script.acquired();
    assert_eq!(acquired.len(), 2, "@args, then @notes");
    assert_eq!(
        script.released(),
        [acquired[1], acquired[0]],
        "the @notes resource, then @args"
    );
}

#[test]
fn atk_startup_read_malformed_reply() {
    let mut rig = Rig::new(&w());
    rig.script.borrow_mut().replies.insert(
        HostOp::EnvReadArgs,
        completed(HostReply::Ok(HostOk::U64(1))),
    );
    let script = rig.script();
    let ran = run(&m(), &mut rig);
    assert!(
        matches!(ran.fault, Some(HostFault::InvalidHostValue(_))),
        "{:?}",
        ran.fault
    );
    assert_eq!(ran.started.status, RecordStatus::Failed);
    assert_eq!(ran.started.failure, None);
    assert!(
        ran.trace
            .events
            .iter()
            .all(|event| !matches!(event, Event::Host(_))),
        "the trace has no @host event"
    );
    assert_eq!(summary(&ran.trace), ["@startup @args @acquired"]);
    assert_eq!(ran.uncommitted, 0, "the broker kept nothing pending");
    let script = script.borrow();
    assert_eq!(script.released(), script.acquired(), "@args was released");
}

#[test]
fn a_startup_read_is_committed_after_use() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let hook = Rc::clone(&seen);
    startup::on_stored_for_tests(Some(Box::new(move |leaf, path, events| {
        hook.borrow_mut()
            .push((leaf.clone(), path.map(str::to_owned), events));
    })));
    let ran = run(&m(), &mut Rig::new(&w()));
    startup::on_stored_for_tests(None);
    assert_eq!(
        *seen.borrow(),
        [(name("@notes"), Some("/notes".to_owned()), 1)],
        "the path was stored while the trace held only `@startup @args`"
    );
    assert_eq!(
        summary(&ran.trace).get(1).map(String::as_str),
        Some("@host 1 None @startup env.readArgs"),
        "and the read's event was appended next"
    );
}

#[test]
fn the_next_invocation() {
    assert_eq!(run(&m(), &mut Rig::new(&w())).next_invocation, 2);
    assert_eq!(run(&o(), &mut Rig::new(&w())).next_invocation, 1);
}

#[test]
fn release_on_drop() {
    let plan = m();
    let shape = shape(&plan);
    let mut rig = Rig::new(&w());
    let script = rig.script();
    let admission = admit(&plan, &shape, &m_ceiling(), &rig).expect("admitted");
    let mut cx = Context::new(&mut rig, header());
    let started = start(admission, &mut cx);
    drop(started);
    let script = script.borrow();
    let mut acquired = script.acquired();
    assert_eq!(acquired.len(), 3);
    acquired.reverse();
    assert_eq!(script.released(), acquired);
}

#[test]
fn atk_trap_during_startup() {
    let plan = o();
    let shape = shape(&plan);
    let mut rig = Rig::new(&w());
    rig.script.borrow_mut().at.insert(3, Answer::Panic);
    let script = rig.script();
    let admission = admit(&plan, &shape, &every_interface(), &rig).expect("admitted");
    let mut cx = Context::new(&mut rig, header());
    let trapped = catch_unwind(AssertUnwindSafe(|| {
        let _ = start(admission, &mut cx);
    }));
    assert!(trapped.is_err(), "the driver panicked");
    let script = script.borrow();
    let mut acquired = script.acquired();
    assert_eq!(acquired.len(), 2);
    acquired.reverse();
    assert_eq!(script.released(), acquired, "@a, then @b");
}

#[test]
fn release_appends_nothing() {
    let plan = m();
    let shape = shape(&plan);
    let mut rig = Rig::new(&w());
    let admission = admit(&plan, &shape, &m_ceiling(), &rig).expect("admitted");
    let mut cx = Context::new(&mut rig, header());
    let mut started = start(admission, &mut cx);
    let before = started.context().trace().clone();
    let _ = release(started);
    assert_eq!(cx.trace(), &before);
}
