//! The grant table of a run: narrowing, the counters, and `Context::host`.
//!
//! `docs/spec/06-runtime.md`, "Host calls", and `docs/spec/03-effects.md`,
//! "Capability values", "Narrowing", and "Limits", fix what [`Context`]
//! does. It implements [`HostContext`], the one boundary every host
//! operation passes.
//!
//! # The grant table
//!
//! A [`GrantToken`] is an index into the table, not authority: every use
//! validates it. A slot of the table holds its interface, its parent slot
//! (none for a root), the one constraint it adds (a root's scope and limits,
//! a policy, a prefix, or limits with their counters), and its
//! [`Lineage`]: the root it descends from. A lineage holds the resource, the
//! origin record, the root ordinal, a generation, and whether it is live. A
//! slot has no liveness of its own, so killing a lineage kills every grant
//! narrowed from its root at once, and no slot is ever written again after
//! it is added. Every walk from a slot to its root is a loop over parent
//! slots, so an ancestry of any depth costs no host stack.
//!
//! # A host call
//!
//! [`Context::host`] is steps 1 to 4 of the runtime chapter, in one
//! function, with nothing decided ahead of the call:
//!
//! 1. validate the token: its context, its slot, the liveness and the
//!    generation of the slot's lineage, the operation's owner, and that no
//!    event is pending;
//! 2. derive the action from the operands: the names, rebased below the
//!    prefix of every `sub`, the actions the operation may turn out to
//!    perform, and for `env.readVar` the name against the root's set;
//! 3. evaluate every policy of the ancestry, then reserve one invocation and
//!    the bytes the operands fix on every counter of the ancestry, all or
//!    none;
//! 4. call the driver with a gate, which decides the policy on the real
//!    names and the found action, the hard-link rule, and the input bytes of
//!    a read, and then settle every reservation from the driver's
//!    [`Progress`].
//!
//! Step 5 is split with the caller: a completed call leaves its event
//! pending and returns a [`Receipt`]; the caller advances its continuation
//! and then calls [`HostContext::commit`]. Nothing is appended inside
//! `host`.
//!
//! # The trace
//!
//! The context keeps the run's [`AuditTrace`]. A root records the driver
//! bound for its provider. A completed call (a typed denial of the gate
//! included) builds its [`HostEvent`] and keeps it pending, with the
//! policies its projection names; `commit` of that call's receipt appends
//! both, once. A refusal of steps 1 to 3, and a size or `unsupported`
//! refusal of the gate, have no receipt and record nothing. A pending event
//! that is never committed is dropped with the context
//! ([`Context::uncommitted`]).
//!
//! # The test world
//!
//! A context made by [`Context::for_world`] is over a fake world, and
//! [`Context::test_root`] issues its one test root: the `TestWorld` grant a
//! test receives. The root is no slot of the table, so no host operation
//! and no narrowing accepts it. [`HostContext::test_world`] accepts only
//! it, runs the member on the world (`crate::test_world`), and issues the
//! member's roots; it appends no event.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use vibra_ir::authority::{
    GrantLimits, GrantToken, HostArgument, HostCompletion, HostContext, HostErrorCase,
    HostFault, HostReply, HostRequest, Issued, NarrowRequest, Narrowed, Receipt,
    ScopeErrorCase,
};
use vibra_ir::external::host::{HostErrorType, HostInterface, HostOp, TestWorldOp};
use vibra_ir::{EntityRef, Value};

use crate::driver::{
    Action, Completion, Driver, DriverIdentity, Found, FoundAction, Gate, Measured,
    Operand, Progress, Refused, Reserved, ResourceId,
};
use crate::path::RelativeNames;
use crate::policy::{
    self, DirAction, DirDepth, DirectoryRule, Expr, FileAction, FileDepth, FileRule,
    Join, NameTest, Policy, Request,
};
use crate::recipe::{LeafName, Limits, PROVIDERS, Provider};
use crate::test_world::{self, Made, Root};
use crate::trace::{
    self, AuditTrace, Event, GrantOrigin, GrantProjection, HostEvent, Phase,
    StartupEvent, StartupFailure, StartupStatus, StdlibIdentity, TraceHeader,
};
use crate::world::WorldDriver;

/// The identity of the next context; each [`Context::new`] takes one.
static NEXT_CONTEXT: AtomicU64 = AtomicU64::new(1);

/// A root grant as its issuer hands it to `Context::issue`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootGrant {
    /// The operation interface the grant permits.
    pub interface: HostInterface,
    /// The resource the issuer acquired.
    pub resource: ResourceId,
    /// Who issued it.
    pub origin: GrantOrigin,
    /// The directory policy, in its normal form; `None` for a grant that is
    /// not of a directory, or one whose root adds no policy.
    pub policy: Option<Policy>,
    /// The variables an `EnvReadVar` grant may read; `None` permits every
    /// name.
    pub variables: Option<Vec<String>>,
    /// The recipe's limits.
    pub limits: Limits,
}

/// The root a slot descends from: what every grant of one root shares, and
/// the one thing `Context::release` writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lineage {
    resource: ResourceId,
    origin: GrantOrigin,
    driver: DriverIdentity,
    root: u64,
    generation: u32,
    live: bool,
}

impl Lineage {
    /// The driver bound for the root's provider: the `driver` of every
    /// event of the lineage.
    #[must_use]
    pub const fn driver(&self) -> &DriverIdentity {
        &self.driver
    }

    /// The resource the root's issuer acquired.
    #[must_use]
    pub const fn resource(&self) -> ResourceId {
        self.resource
    }

    /// The origin record of the root.
    #[must_use]
    pub const fn origin(&self) -> &GrantOrigin {
        &self.origin
    }

    /// The run-local ordinal of the root, from 1 in issue order.
    #[must_use]
    pub const fn root(&self) -> u64 {
        self.root
    }

    /// The generation every live token of the lineage carries.
    #[must_use]
    pub const fn generation(&self) -> u32 {
        self.generation
    }

    /// Whether the resource is still held.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        self.live
    }
}

/// One counter: a bound, what was charged, and what is reserved by the call
/// in progress.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Counter {
    bound: Option<u64>,
    used: u64,
    reserved: u64,
}

/// Why a counter cannot take an amount.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shortfall {
    /// The counter would overflow: the call is `denied`.
    Overflow,
    /// The counter would pass its bound: the call is `limitExceeded`.
    Exceeded,
}

impl Counter {
    const fn bounded(bound: Option<u64>) -> Self {
        Self {
            bound,
            used: 0,
            reserved: 0,
        }
    }

    /// The bound; `None` is unbounded.
    #[must_use]
    pub const fn bound(&self) -> Option<u64> {
        self.bound
    }

    /// What was charged.
    #[must_use]
    pub const fn used(&self) -> u64 {
        self.used
    }

    /// What is reserved and not yet settled: zero between calls.
    #[must_use]
    pub const fn reserved(&self) -> u64 {
        self.reserved
    }

    fn admits(&self, amount: u64) -> Result<(), Shortfall> {
        let total = self
            .used
            .checked_add(self.reserved)
            .and_then(|taken| taken.checked_add(amount))
            .ok_or(Shortfall::Overflow)?;
        match self.bound {
            Some(bound) if total > bound => Err(Shortfall::Exceeded),
            _ => Ok(()),
        }
    }

    /// What the counter can still take before it passes its bound or
    /// overflows.
    fn headroom(&self) -> u64 {
        self.bound
            .unwrap_or(u64::MAX)
            .saturating_sub(self.used)
            .saturating_sub(self.reserved)
    }

    fn reserve(&mut self, amount: u64) {
        self.reserved = self.reserved.saturating_add(amount);
    }

    /// Settles `reserved` of the reservation, charging `charged` of it.
    fn settle(&mut self, reserved: u64, charged: u64) {
        self.reserved = self.reserved.saturating_sub(reserved);
        self.used = self.used.saturating_add(charged.min(reserved));
    }
}

/// The three counters of one root or one `limit`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Counters {
    invocations: Counter,
    input_bytes: Counter,
    output_bytes: Counter,
}

impl Counters {
    fn new(limits: Limits) -> Self {
        Self {
            invocations: Counter::bounded(limits.invocations),
            input_bytes: Counter::bounded(limits.input_bytes),
            output_bytes: Counter::bounded(limits.output_bytes),
        }
    }

    /// The invocation counter.
    #[must_use]
    pub const fn invocations(&self) -> &Counter {
        &self.invocations
    }

    /// The input-byte counter.
    #[must_use]
    pub const fn input_bytes(&self) -> &Counter {
        &self.input_bytes
    }

    /// The output-byte counter.
    #[must_use]
    pub const fn output_bytes(&self) -> &Counter {
        &self.output_bytes
    }

    fn admits(&self, amount: Charge) -> Result<(), Shortfall> {
        self.invocations.admits(amount.invocations)?;
        self.input_bytes.admits(amount.input)?;
        self.output_bytes.admits(amount.output)
    }

    fn reserve(&mut self, amount: Charge) {
        self.invocations.reserve(amount.invocations);
        self.input_bytes.reserve(amount.input);
        self.output_bytes.reserve(amount.output);
    }

    fn settle(&mut self, reserved: Charge, charged: Charge) {
        self.invocations
            .settle(reserved.invocations, charged.invocations);
        self.input_bytes.settle(reserved.input, charged.input);
        self.output_bytes.settle(reserved.output, charged.output);
    }
}

/// An amount on each of the three counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Charge {
    invocations: u64,
    input: u64,
    output: u64,
}

impl Charge {
    const NONE: Self = Self {
        invocations: 0,
        input: 0,
        output: 0,
    };
}

/// The one constraint a slot adds to its parent.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Constraint {
    /// A root: its scope and its recipe's counters.
    Root {
        policy: Option<Policy>,
        variables: Option<Vec<String>>,
        counters: Counters,
    },
    /// `restrict`: the policy, re-rooted into the coordinates of the root's
    /// directory.
    Policy(Policy),
    /// `sub`: the whole prefix in root coordinates, and the policy it
    /// contributes to the effective policy.
    Prefix {
        names: RelativeNames,
        policy: Policy,
    },
    /// `limit`: counters of its own.
    Limits(Counters),
}

impl Constraint {
    /// The policy this constraint contributes, if any.
    const fn policy(&self) -> Option<&Policy> {
        match self {
            Self::Root { policy, .. } => policy.as_ref(),
            Self::Policy(policy) | Self::Prefix { policy, .. } => Some(policy),
            Self::Limits(_) => None,
        }
    }

    /// The counters this constraint holds, if any.
    const fn counters(&self) -> Option<&Counters> {
        match self {
            Self::Root { counters, .. } | Self::Limits(counters) => Some(counters),
            Self::Policy(_) | Self::Prefix { .. } => None,
        }
    }

    const fn counters_mut(&mut self) -> Option<&mut Counters> {
        match self {
            Self::Root { counters, .. } | Self::Limits(counters) => Some(counters),
            Self::Policy(_) | Self::Prefix { .. } => None,
        }
    }

    /// Whether this is a `restrict` or a `sub`, which add a level to the
    /// depth of the effective policy.
    const fn is_narrowing_policy(&self) -> bool {
        matches!(self, Self::Policy(_) | Self::Prefix { .. })
    }
}

/// One entry of the grant table. Written once, when it is added.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Slot {
    interface: HostInterface,
    parent: Option<u32>,
    constraint: Constraint,
    lineage: usize,
}

/// The slots from one slot to its root, found by a loop over parent slots.
struct Ancestry<'t> {
    slots: &'t [Slot],
    next: Option<u32>,
}

impl<'t> Iterator for Ancestry<'t> {
    type Item = &'t Slot;

    fn next(&mut self) -> Option<&'t Slot> {
        let slot = self.slots.get(usize::try_from(self.next?).ok()?)?;
        self.next = slot.parent;
        Some(slot)
    }
}

fn ancestry(slots: &[Slot], slot: u32) -> Ancestry<'_> {
    Ancestry {
        slots,
        next: Some(slot),
    }
}

/// Applies `apply` to the counters of every slot from `slot` to its root.
fn each_counters(slots: &mut [Slot], slot: u32, mut apply: impl FnMut(&mut Counters)) {
    let mut next = Some(slot);
    while let Some(index) = next {
        let Some(slot) = usize::try_from(index)
            .ok()
            .and_then(|index| slots.get_mut(index))
        else {
            return;
        };
        if let Some(counters) = slot.constraint.counters_mut() {
            apply(counters);
        }
        next = slot.parent;
    }
}

/// Reserves `amount` on every counter of the ancestry of `slot`, all or
/// none.
fn reserve(slots: &mut [Slot], slot: u32, amount: Charge) -> Result<(), Shortfall> {
    // An overflow anywhere denies before a bound anywhere is exceeded.
    let mut exceeded = false;
    for counters in ancestry(slots, slot).filter_map(|slot| slot.constraint.counters())
    {
        match counters.admits(amount) {
            Ok(()) => {}
            Err(Shortfall::Overflow) => return Err(Shortfall::Overflow),
            Err(Shortfall::Exceeded) => exceeded = true,
        }
    }
    if exceeded {
        return Err(Shortfall::Exceeded);
    }
    each_counters(slots, slot, |counters| counters.reserve(amount));
    Ok(())
}

/// The input bytes every counter of the ancestry of `slot` can still take.
fn input_headroom(slots: &[Slot], slot: u32) -> u64 {
    ancestry(slots, slot)
        .filter_map(|slot| slot.constraint.counters())
        .map(|counters| counters.input_bytes.headroom())
        .fold(u64::MAX, u64::min)
}

/// Whether every policy of the ancestry of `slot` permits `request`.
fn ancestry_permits(slots: &[Slot], slot: u32, request: &Request<'_>) -> bool {
    ancestry(slots, slot)
        .filter_map(|slot| slot.constraint.policy())
        .all(|policy| policy::permits(policy.expression(), request))
}

/// The policy action a found action is decided as.
const fn policy_action(found: FoundAction) -> policy::Action {
    match found {
        FoundAction::Read => policy::Action::File(FileAction::Read),
        FoundAction::Replace => policy::Action::File(FileAction::Replace),
        FoundAction::Create => policy::Action::File(FileAction::Create),
        FoundAction::StatFile => policy::Action::File(FileAction::Stat),
        FoundAction::List => policy::Action::Directory(DirAction::List),
        FoundAction::StatDirectory => policy::Action::Directory(DirAction::Stat),
    }
}

/// What steps 2 and 3 derive from an operation and its operands.
struct Derived<'r> {
    /// The names of a filesystem request, in root coordinates.
    names: Vec<String>,
    operand: Operand<'r>,
    /// The actions a filesystem operation may turn out to perform; empty for
    /// every other operation.
    may: &'static [FoundAction],
    /// The bytes the operands fix.
    fixed: Charge,
    /// The `max` of a stream read.
    max: Option<u64>,
    /// Whether the stream read is a text read.
    text: bool,
    /// The name of an `env.readVar`.
    variable: Option<&'r str>,
    /// Whether the operation reads the filesystem, so that its policy
    /// applies.
    filesystem: bool,
}

const READ: &[FoundAction] = &[FoundAction::Read];
const LIST: &[FoundAction] = &[FoundAction::List];
const WRITE: &[FoundAction] = &[FoundAction::Create, FoundAction::Replace];
const STAT: &[FoundAction] = &[FoundAction::StatFile, FoundAction::StatDirectory];
const NOTHING: &[FoundAction] = &[];

fn invalid(reason: impl Into<String>) -> HostFault {
    HostFault::InvalidHostValue(reason.into())
}

fn byte_length(bytes: usize) -> u64 {
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

/// Step 2: the names, rebased below `prefix`, and the operands of `op`,
/// checked against the row's parameters.
fn derive<'r>(
    op: HostOp,
    arguments: &[HostArgument<'r>],
    prefix: &RelativeNames,
) -> Result<Derived<'r>, HostFault> {
    let mut derived = Derived {
        names: Vec::new(),
        operand: Operand::None,
        may: NOTHING,
        fixed: Charge::NONE,
        max: None,
        text: false,
        variable: None,
        filesystem: false,
    };
    let rebase = |names: &[String]| -> Result<Vec<String>, HostFault> {
        RelativeNames::new(names.iter().cloned()).map_err(|fault| {
            invalid(format!(
                "the path operand is not a `RelativePath`: {}",
                fault.error
            ))
        })?;
        Ok(prefix
            .names()
            .iter()
            .map(|name| name.as_str().to_owned())
            .chain(names.iter().cloned())
            .collect())
    };
    let wrong = || {
        invalid(format!(
            "the operands of `{}` are not its row's",
            op.symbol()
        ))
    };
    match (op, arguments) {
        (HostOp::IoStdinText | HostOp::IoStdinBytes, [HostArgument::Count(max)]) => {
            derived.may = READ;
            derived.max = Some(*max);
            derived.text = op == HostOp::IoStdinText;
            derived.operand = Operand::Count(*max);
        }
        (HostOp::IoStdoutText | HostOp::IoStderrText, [HostArgument::Text(value)]) => {
            derived.fixed.output = byte_length(value.len());
            derived.operand = Operand::Text(value);
        }
        (
            HostOp::IoStdoutBytes | HostOp::IoStderrBytes,
            [HostArgument::Bytes(value)],
        ) => {
            derived.fixed.output = byte_length(value.len());
            derived.operand = Operand::Bytes(value);
        }
        (
            HostOp::FsReadFile
            | HostOp::FsReadText
            | HostOp::FsReadEntries
            | HostOp::FsMetadataStat,
            [HostArgument::Path(names)],
        ) => {
            derived.names = rebase(names)?;
            derived.filesystem = true;
            derived.may = match op {
                HostOp::FsReadEntries => LIST,
                HostOp::FsMetadataStat => STAT,
                _ => READ,
            };
        }
        (
            HostOp::FsWriteFile,
            [HostArgument::Path(names), HostArgument::Bytes(value)],
        ) => {
            derived.names = rebase(names)?;
            derived.filesystem = true;
            derived.may = WRITE;
            derived.fixed.output = byte_length(value.len());
            derived.operand = Operand::Bytes(value);
        }
        (
            HostOp::FsWriteText,
            [HostArgument::Path(names), HostArgument::Text(value)],
        ) => {
            derived.names = rebase(names)?;
            derived.filesystem = true;
            derived.may = WRITE;
            derived.fixed.output = byte_length(value.len());
            derived.operand = Operand::Text(value);
        }
        (HostOp::EnvReadVar, [HostArgument::Text(name)]) => {
            derived.may = READ;
            derived.variable = Some(name);
            derived.operand = Operand::Text(name);
        }
        (HostOp::EnvReadArgs, []) => derived.may = READ,
        (HostOp::TimeNowWall | HostOp::TimeNowMonotonic, []) => {}
        (HostOp::RandomGenerateBytes, [HostArgument::Count(count)]) => {
            derived.fixed.input = *count;
            derived.operand = Operand::Count(*count);
        }
        _ => return Err(wrong()),
    }
    Ok(derived)
}

/// Why the gate refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reason {
    /// The policy on the real names or the found action, or the hard-link
    /// rule: a completed `denied`.
    Denied,
    /// A counter would overflow: `denied`, not a completed call.
    Overflow,
    /// The size is over a bound: `limitExceeded`.
    LimitExceeded,
    /// A fact the rule needs is not reported: `unsupported`.
    Unsupported,
    /// The size is over the caller's room.
    MemoryExhausted,
    /// The driver asked the gate something its operation cannot ask.
    Violation(&'static str),
}

/// The gate of one call: it decides what only the opened object decides,
/// against the same ancestry step 3 decided.
struct Admission<'c> {
    slots: &'c mut [Slot],
    slot: u32,
    may: &'static [FoundAction],
    filesystem: bool,
    max: Option<u64>,
    text: bool,
    room: u64,
    /// The gate's answer, once it gave one.
    answer: Option<Result<u64, Reason>>,
    /// Whether the driver asked twice.
    twice: bool,
}

impl Admission<'_> {
    fn decide(&mut self, found: &Found<'_>) -> Result<u64, Reason> {
        if !self.may.contains(&found.action) {
            return Err(Reason::Violation(
                "the driver found an action its operation cannot perform",
            ));
        }
        if self.filesystem {
            let request = Request {
                names: found.real_names,
                action: policy_action(found.action),
            };
            if !ancestry_permits(self.slots, self.slot, &request) {
                return Err(Reason::Denied);
            }
        }
        if found.action == FoundAction::Replace {
            match found.links {
                None => return Err(Reason::Unsupported),
                Some(links) if links > 1 => return Err(Reason::Denied),
                Some(_) => {}
            }
        }
        if found.action != FoundAction::Read {
            return Ok(0);
        }
        let input = match (self.max, found.size) {
            // A stream of known size: the smaller of `max` and the size.
            (Some(max), Some(size)) => max.min(size),
            // A stream of unknown size: the smaller of `max` and what remains
            // of every bound of the ancestry.
            (Some(max), None) => {
                let remaining = input_headroom(self.slots, self.slot);
                if max > 0 && remaining == 0 {
                    return Err(Reason::LimitExceeded);
                }
                let input = max.min(remaining);
                if self.text && max >= 4 && input < 4 {
                    return Err(Reason::LimitExceeded);
                }
                input
            }
            (None, Some(size)) => size,
            (None, None) => {
                return Err(Reason::Violation(
                    "the driver reads an object whose size it does not report",
                ));
            }
        };
        let amount = Charge {
            input,
            ..Charge::NONE
        };
        for counters in ancestry(self.slots, self.slot)
            .filter_map(|slot| slot.constraint.counters())
        {
            match counters.admits(amount) {
                Ok(()) => {}
                Err(Shortfall::Overflow) => return Err(Reason::Overflow),
                Err(Shortfall::Exceeded) => return Err(Reason::LimitExceeded),
            }
        }
        if input > self.room {
            return Err(Reason::MemoryExhausted);
        }
        each_counters(self.slots, self.slot, |counters| counters.reserve(amount));
        Ok(input)
    }
}

impl Gate for Admission<'_> {
    fn admit(&mut self, found: &Found<'_>) -> Result<Reserved, Refused> {
        if self.answer.is_some() {
            self.twice = true;
            return Err(Refused::new());
        }
        let answer = self.decide(found);
        self.answer = Some(answer);
        answer
            .map(|input| Reserved { input })
            .map_err(|_| Refused::new())
    }
}

/// The case `case` of `error`.
fn case(error: HostErrorType, case: &str) -> Result<HostErrorCase, HostFault> {
    HostErrorCase::new(error, case)
        .ok_or_else(|| invalid(format!("`{}` has no case `{case}`", error.name())))
}

/// The bytes one byte reservation charges, from what the adapter measured.
/// The second value is whether the measure was above the reservation.
const fn charged(
    reserved: u64,
    measured: Measured,
    outcome_unknown: bool,
) -> (u64, bool) {
    if outcome_unknown {
        return (reserved, false);
    }
    match measured {
        Measured::Known(bytes) if bytes <= reserved => (bytes, false),
        Measured::Known(_) => (reserved, true),
        Measured::Uncertain => (reserved, false),
    }
}

/// The event of a completed call, kept until its receipt is committed.
#[derive(Debug)]
struct Pending {
    serial: u64,
    event: HostEvent,
    /// The effective policy the event's projection names, by its hash.
    policy: Option<(String, Policy)>,
}

/// The provider that issues grants of `interface`: the provider table's
/// grant types are disjoint.
fn provider_of(interface: HostInterface) -> Option<Provider> {
    PROVIDERS
        .iter()
        .find(|row| row.grant_types.contains(&interface))
        .map(|row| row.provider)
}

/// The slot index of a test root: no slot of the table has it, since
/// `Context::add` refuses it.
const TEST_ROOT: u32 = u32::MAX;

/// The driver a context calls: any driver, or the fake world of a test,
/// whose `TestWorld` members only the test root reaches.
enum Bound<'d> {
    Any(&'d mut dyn Driver),
    World(&'d mut WorldDriver),
}

impl Bound<'_> {
    fn get(&self) -> &dyn Driver {
        match self {
            Self::Any(driver) => &**driver,
            Self::World(world) => &**world,
        }
    }

    fn get_mut(&mut self) -> &mut dyn Driver {
        match self {
            Self::Any(driver) => &mut **driver,
            Self::World(world) => &mut **world,
        }
    }
}

/// The run's broker: the grant table, the counters, the pending event, and
/// the trace, over one driver.
pub struct Context<'d> {
    identity: u64,
    driver: Bound<'d>,
    library: StdlibIdentity,
    slots: Vec<Slot>,
    lineages: Vec<Lineage>,
    roots: u64,
    pending: Option<Pending>,
    serial: u64,
    trace: AuditTrace,
    /// Whether the test root of the world was issued.
    test_root: bool,
}

impl fmt::Debug for Context<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Context")
            .field("identity", &self.identity)
            .field("slots", &self.slots.len())
            .field("lineages", &self.lineages)
            .field("pending", &self.pending)
            .field("trace", &self.trace)
            .finish_non_exhaustive()
    }
}

impl<'d> Context<'d> {
    /// A context over `driver`, with an empty grant table, an identity no
    /// other context of the process has, and the empty trace of `header`.
    pub fn new(driver: &'d mut dyn Driver, header: TraceHeader) -> Self {
        Self::over(Bound::Any(driver), header)
    }

    fn over(driver: Bound<'d>, header: TraceHeader) -> Self {
        Self {
            identity: NEXT_CONTEXT.fetch_add(1, Ordering::Relaxed),
            driver,
            library: header.library.clone(),
            slots: Vec::new(),
            lineages: Vec::new(),
            roots: 0,
            pending: None,
            serial: 0,
            trace: AuditTrace::new(header),
            test_root: false,
        }
    }

    /// A context over the fake world `world`, as a test runs in: as
    /// [`Context::new`], and the test root can be issued.
    pub fn for_world(world: &'d mut WorldDriver, header: TraceHeader) -> Self {
        Self::over(Bound::World(world), header)
    }

    /// Issues the test root: the sealed `TestWorld` grant a test that
    /// takes its world receives. It is no slot of the table, has no
    /// ordinal, and reaches only the members of `TestWorld`.
    ///
    /// # Errors
    ///
    /// `InvalidHostValue` for a context that is not over a fake world, or
    /// one whose test root was already issued.
    pub fn test_root(&mut self) -> Result<GrantToken, HostFault> {
        if !matches!(self.driver, Bound::World(_)) {
            return Err(invalid("only a context over a fake world has a test root"));
        }
        if self.test_root {
            return Err(invalid("the test root was already issued"));
        }
        self.test_root = true;
        Ok(GrantToken::new(self.identity, TEST_ROOT, 0))
    }

    /// The run's trace: every committed event, in completion order.
    #[must_use]
    pub const fn trace(&self) -> &AuditTrace {
        &self.trace
    }

    /// The pending events not committed: the event of a completed call
    /// whose receipt the caller has not committed. It is dropped with the
    /// context, and the trace never holds it.
    #[must_use]
    pub fn uncommitted(&self) -> usize {
        usize::from(self.pending.is_some())
    }

    /// Issues a root grant. Only startup and the test world call it.
    ///
    /// The driver bound for the root's provider joins the trace's
    /// `drivers`.
    ///
    /// # Errors
    ///
    /// [`HostFault::MemoryExhausted`] when the table cannot grow;
    /// `InvalidHostValue` when the driver has no identity for the provider
    /// of the root's interface.
    pub(crate) fn issue(&mut self, root: RootGrant) -> Result<GrantToken, HostFault> {
        let driver = provider_of(root.interface)
            .and_then(|provider| self.driver.get().identity(provider))
            .ok_or_else(|| {
                invalid(format!(
                    "no driver is bound for a `{}` grant",
                    root.interface.name()
                ))
            })?;
        self.lineages
            .try_reserve(1)
            .map_err(|_| HostFault::MemoryExhausted)?;
        let lineage = self.lineages.len();
        let constraint = Constraint::Root {
            policy: root.policy,
            variables: root.variables,
            counters: Counters::new(root.limits),
        };
        let token = self.add(root.interface, None, constraint, lineage, 0)?;
        self.roots = self.roots.saturating_add(1);
        self.trace.bind_driver(driver.clone());
        self.lineages.push(Lineage {
            resource: root.resource,
            origin: root.origin,
            driver,
            root: self.roots,
            generation: 0,
            live: true,
        });
        Ok(token)
    }

    /// Appends the `@startup` event of one processed leaf: its issuer, its
    /// outcome, the projection of `grant` exactly when acquired, and the
    /// failure exactly when the leaf stopped startup.
    ///
    /// # Errors
    ///
    /// `InvalidHostValue` for a grant this context cannot accept, an event
    /// pending, or fields that do not match the status.
    #[cfg_attr(
        not(feature = "testing"),
        expect(dead_code, reason = "startup (step 28) calls it")
    )]
    pub(crate) fn startup_event(
        &mut self,
        leaf: LeafName,
        provider: Provider,
        status: StartupStatus,
        grant: Option<GrantToken>,
        failure: Option<StartupFailure>,
    ) -> Result<(), HostFault> {
        if self.pending.is_some() {
            return Err(invalid("the event of the previous call is not committed"));
        }
        let (grant, policy) = match grant {
            Some(token) => {
                let slot = self.validate(token, None)?;
                let (projection, policy) = self.projection(slot)?;
                (Some(projection), policy)
            }
            None => (None, None),
        };
        let event = StartupEvent {
            binding: leaf,
            provider,
            status,
            grant,
            failure,
        };
        if !event.is_consistent() {
            return Err(invalid(
                "the startup event's grant and failure do not match its status",
            ));
        }
        self.record(Event::Startup(event), policy);
        Ok(())
    }

    /// Appends `event` and the policy its projection names.
    fn record(&mut self, event: Event, policy: Option<(String, Policy)>) {
        if let Some((hash, policy)) = policy {
            self.trace.policies.entry(hash).or_insert(policy);
        }
        self.trace.events.push(event);
    }

    /// The projection of `slot`, and its effective policy by its hash when
    /// the ancestry has one.
    fn projection(
        &self,
        slot: u32,
    ) -> Result<(GrantProjection, Option<(String, Policy)>), HostFault> {
        let lineage = self.lineage_of(slot)?;
        let interface = self.slot(slot)?.interface;
        let mut items = Vec::new();
        let mut narrowings: usize = 0;
        let mut smallest = Limits::default();
        let least = |least: Option<u64>, bound: Option<u64>| match (least, bound) {
            (Some(least), Some(bound)) => Some(least.min(bound)),
            (least, bound) => least.or(bound),
        };
        for slot in ancestry(&self.slots, slot) {
            if slot.constraint.is_narrowing_policy() {
                narrowings = narrowings.saturating_add(1);
            }
            if let Some(policy) = slot.constraint.policy() {
                items.push(policy.expression().clone());
            }
            if let Some(counters) = slot.constraint.counters() {
                smallest = Limits {
                    invocations: least(
                        smallest.invocations,
                        counters.invocations.bound,
                    ),
                    input_bytes: least(
                        smallest.input_bytes,
                        counters.input_bytes.bound,
                    ),
                    output_bytes: least(
                        smallest.output_bytes,
                        counters.output_bytes.bound,
                    ),
                };
            }
        }
        let policy = if items.is_empty() {
            None
        } else {
            let effective = Policy::new(&Expr::AllOf(items), narrowings)
                .map_err(|_| invalid("the effective policy is over the limits"))?;
            Some((policy::hash(&effective), effective))
        };
        let owner = interface.owner();
        let rights = EntityRef::new(
            owner.path().trim_start_matches('@'),
            self.library
                .seed(owner.module, trace::INTERFACE_KIND, owner.name),
        )
        .map_err(|fault| invalid(fault.to_string()))?;
        let projection = GrantProjection {
            issued_by: lineage.origin.clone(),
            policy: policy
                .as_ref()
                .map_or_else(String::new, |(hash, _)| hash.clone()),
            rights,
            limits: (!smallest.is_unbounded()).then_some(smallest),
            root: lineage.root,
        };
        Ok((projection, policy))
    }

    /// Makes every grant that descends from a root on `resource` unusable,
    /// and only then releases the resource.
    #[cfg_attr(
        not(feature = "testing"),
        expect(dead_code, reason = "startup (step 28) calls it")
    )]
    pub(crate) fn release(&mut self, resource: ResourceId) {
        for lineage in &mut self.lineages {
            if lineage.resource == resource {
                lineage.live = false;
                lineage.generation = lineage.generation.wrapping_add(1);
            }
        }
        self.driver.get_mut().release(resource);
    }

    /// Adds a slot; the token carries `generation`.
    fn add(
        &mut self,
        interface: HostInterface,
        parent: Option<u32>,
        constraint: Constraint,
        lineage: usize,
        generation: u32,
    ) -> Result<GrantToken, HostFault> {
        let index = u32::try_from(self.slots.len())
            .ok()
            .filter(|index| *index != TEST_ROOT)
            .ok_or(HostFault::MemoryExhausted)?;
        self.slots
            .try_reserve(1)
            .map_err(|_| HostFault::MemoryExhausted)?;
        self.slots.push(Slot {
            interface,
            parent,
            constraint,
            lineage,
        });
        Ok(GrantToken::new(self.identity, index, generation))
    }

    /// Step 1 of a call and of every narrowing: the token's context and
    /// slot, the liveness and generation of the slot's lineage, the
    /// operation's owner, and that no event is pending.
    fn validate(
        &self,
        token: GrantToken,
        op: Option<HostOp>,
    ) -> Result<u32, HostFault> {
        if token.context() != self.identity {
            return Err(invalid("the grant belongs to another context"));
        }
        let slot = usize::try_from(token.slot())
            .ok()
            .and_then(|index| self.slots.get(index))
            .ok_or_else(|| {
                invalid("the grant names a slot this context never issued")
            })?;
        let lineage = self
            .lineages
            .get(slot.lineage)
            .ok_or_else(|| invalid("the grant's lineage does not exist"))?;
        if !lineage.live || lineage.generation != token.generation() {
            return Err(invalid("the grant was released"));
        }
        if let Some(op) = op
            && slot.interface != op.interface()
        {
            return Err(invalid(format!(
                "a `{}` grant cannot perform `{}`",
                slot.interface.name(),
                op.symbol()
            )));
        }
        if self.pending.is_some() {
            return Err(invalid("the event of the previous call is not committed"));
        }
        Ok(token.slot())
    }

    fn slot(&self, slot: u32) -> Result<&Slot, HostFault> {
        usize::try_from(slot)
            .ok()
            .and_then(|index| self.slots.get(index))
            .ok_or_else(|| invalid("the grant names a slot this context never issued"))
    }

    fn lineage_of(&self, slot: u32) -> Result<&Lineage, HostFault> {
        let slot = self.slot(slot)?;
        self.lineages
            .get(slot.lineage)
            .ok_or_else(|| invalid("the grant's lineage does not exist"))
    }

    /// The prefix of the nearest `sub` of the ancestry, in root
    /// coordinates.
    fn prefix(&self, slot: u32) -> RelativeNames {
        ancestry(&self.slots, slot)
            .find_map(|slot| match &slot.constraint {
                Constraint::Prefix { names, .. } => Some(names.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Whether the root of the ancestry permits the variable `name`.
    fn variable_permitted(&self, slot: u32, name: &str) -> bool {
        ancestry(&self.slots, slot).all(|slot| match &slot.constraint {
            Constraint::Root {
                variables: Some(names),
                ..
            } => names.iter().any(|known| known == name),
            _ => true,
        })
    }

    /// Whether the policy of the whole ancestry permits at least one of
    /// `may` on `names`.
    fn permits_any(&self, slot: u32, names: &[String], may: &[FoundAction]) -> bool {
        may.iter().any(|action| {
            let request = Request {
                names,
                action: policy_action(*action),
            };
            ancestry_permits(&self.slots, slot, &request)
        })
    }

    /// A typed refusal of steps 1 to 3: no event is due.
    fn refusal(error: HostErrorType, name: &str) -> Result<HostCompletion, HostFault> {
        Ok(HostCompletion {
            reply: HostReply::Err(case(error, name)?),
            receipt: None,
        })
    }

    /// Builds the `@host` event of a completed call of `request` through
    /// `slot` with `reply`, and keeps it pending under a new serial.
    ///
    /// # Errors
    ///
    /// A reply that is not of the row's result type.
    fn pend(
        &mut self,
        request: &HostRequest<'_>,
        slot: u32,
        reply: HostReply,
    ) -> Result<HostCompletion, HostFault> {
        let (grant, policy) = self.projection(slot)?;
        let result =
            trace::result_bytes(request.op, &reply, &self.library).map_err(invalid)?;
        let event = HostEvent {
            invocation: request.invocation,
            parent: request.parent,
            phase: if request.parent.is_none() {
                Phase::Startup
            } else {
                Phase::Entry
            },
            op: request.op,
            driver: self.lineage_of(slot)?.driver.clone(),
            origin: request.origin.cloned(),
            grant,
            args: request
                .arguments
                .iter()
                .map(|argument| trace::argument_bytes(argument, &self.library))
                .collect(),
            result,
        };
        self.serial = self.serial.wrapping_add(1);
        self.pending = Some(Pending {
            serial: self.serial,
            event,
            policy,
        });
        Ok(HostCompletion {
            reply,
            receipt: Some(Receipt::new(self.identity, self.serial)),
        })
    }

    /// Settles every byte reservation of a call, on the slot's counters and
    /// on every ancestor's, from what the adapter measured. The reply is
    /// consulted only for whether it is `outcomeUnknown`, which forbids a
    /// refund. The invocation is charged.
    ///
    /// # Errors
    ///
    /// A measure above its reservation, which charges the whole.
    fn settle(
        &mut self,
        slot: u32,
        reserved: Charge,
        progress: Progress,
        outcome_unknown: bool,
    ) -> Result<(), HostFault> {
        let (input, input_over) =
            charged(reserved.input, progress.input, outcome_unknown);
        let (output, output_over) =
            charged(reserved.output, progress.output, outcome_unknown);
        let charge = Charge {
            invocations: reserved.invocations,
            input,
            output,
        };
        each_counters(&mut self.slots, slot, |counters| {
            counters.settle(reserved, charge);
        });
        if input_over || output_over {
            return Err(invalid(
                "the driver reported progress above the reservation",
            ));
        }
        Ok(())
    }

    /// Charges the invocation and refunds every byte: the capacity is known
    /// unused.
    fn refund_bytes(&mut self, slot: u32, reserved: Charge) {
        let charge = Charge {
            invocations: reserved.invocations,
            ..Charge::NONE
        };
        each_counters(&mut self.slots, slot, |counters| {
            counters.settle(reserved, charge);
        });
    }

    /// Releases the whole reservation, the invocation too: the call did not
    /// pass reservation.
    fn release_all(&mut self, slot: u32, reserved: Charge) {
        each_counters(&mut self.slots, slot, |counters| {
            counters.settle(reserved, Charge::NONE);
        });
    }

    /// Charges the whole reservation: the driver broke the protocol, and
    /// what it did may have happened.
    fn charge_all(&mut self, slot: u32, reserved: Charge, reason: &str) -> HostFault {
        each_counters(&mut self.slots, slot, |counters| {
            counters.settle(reserved, reserved);
        });
        invalid(reason)
    }

    /// Maps the driver's completion and the gate's answer to the reply, and
    /// settles the reservation.
    fn finish(
        &mut self,
        request: &HostRequest<'_>,
        slot: u32,
        reserved: Charge,
        answer: Option<Result<u64, Reason>>,
        completion: Completion,
    ) -> Result<HostCompletion, HostFault> {
        let error = request.op.row().error;
        match (answer, completion) {
            (Some(Err(reason)), Completion::Refused) => match reason {
                Reason::Denied => {
                    self.refund_bytes(slot, reserved);
                    self.pend(request, slot, HostReply::Err(case(error, "denied")?))
                }
                Reason::Unsupported => {
                    self.refund_bytes(slot, reserved);
                    Self::refusal(error, "unsupported")
                }
                Reason::LimitExceeded => {
                    self.release_all(slot, reserved);
                    Self::refusal(error, "limitExceeded")
                }
                Reason::Overflow => {
                    self.release_all(slot, reserved);
                    Self::refusal(error, "denied")
                }
                Reason::MemoryExhausted => {
                    self.release_all(slot, reserved);
                    Err(HostFault::MemoryExhausted)
                }
                Reason::Violation(reason) => {
                    self.release_all(slot, reserved);
                    Err(invalid(reason))
                }
            },
            (Some(Err(_)), _) => Err(self.charge_all(
                slot,
                reserved,
                "the driver acted after its gate refused",
            )),
            (_, Completion::Refused) => Err(self.charge_all(
                slot,
                reserved,
                "the driver reported a refusal its gate did not make",
            )),
            (_, Completion::NotSubmitted(case)) => {
                if case.error() != error {
                    return Err(self.charge_all(
                        slot,
                        reserved,
                        "the driver answered with a case of another error type",
                    ));
                }
                self.refund_bytes(slot, reserved);
                Ok(HostCompletion {
                    reply: HostReply::Err(case),
                    receipt: None,
                })
            }
            (_, Completion::Completed { reply, progress }) => {
                let outcome_unknown = match &reply {
                    HostReply::Err(case) if case.error() != error => {
                        return Err(self.charge_all(
                            slot,
                            reserved,
                            "the driver answered with a case of another error type",
                        ));
                    }
                    HostReply::Err(case) => case.case() == "outcomeUnknown",
                    HostReply::Ok(_) => false,
                };
                self.settle(slot, reserved, progress, outcome_unknown)?;
                self.pend(request, slot, reply)
            }
        }
    }

    /// The effective policy of `slot` with `more` added, and the number of
    /// `restrict`s and `sub`s it then counts: what `restrict` and `sub`
    /// count against the limits.
    fn counted(&self, slot: u32, more: &Expr) -> Result<(), ScopeErrorCase> {
        let mut items = Vec::new();
        let mut narrowings: usize = 1;
        for slot in ancestry(&self.slots, slot) {
            if slot.constraint.is_narrowing_policy() {
                narrowings = narrowings.saturating_add(1);
            }
            if let Some(policy) = slot.constraint.policy() {
                items.push(policy.expression().clone());
            }
        }
        items.push(more.clone());
        Policy::new(&Expr::AllOf(items), narrowings)
            .map(|_| ())
            .map_err(|_| ScopeErrorCase::PolicyTooComplex)
    }

    /// Adds a child of `slot` with `constraint`, in the same lineage.
    fn child(
        &mut self,
        slot: u32,
        constraint: Constraint,
    ) -> Result<GrantToken, HostFault> {
        let parent = self.slot(slot)?;
        let interface = parent.interface;
        let lineage = parent.lineage;
        let generation = self.lineage_of(slot)?.generation;
        self.add(interface, Some(slot), constraint, lineage, generation)
    }

    fn restrict(&mut self, slot: u32, value: &Value) -> Result<Narrowed, HostFault> {
        let root = self.lineage_of(slot)?.origin.policy_root().to_owned();
        let expression = match decode_policy(value, &root, &self.prefix(slot))? {
            Ok(expression) => expression,
            Err(refusal) => return Ok(Narrowed::Refused(refusal)),
        };
        if let Err(refusal) = self.counted(slot, &expression) {
            return Ok(Narrowed::Refused(refusal));
        }
        let Ok(policy) = Policy::new(&expression, 0) else {
            return Ok(Narrowed::Refused(ScopeErrorCase::PolicyTooComplex));
        };
        self.child(slot, Constraint::Policy(policy))
            .map(Narrowed::Grant)
    }

    fn sub(&mut self, slot: u32, prefix: &[String]) -> Result<Narrowed, HostFault> {
        let root = self.lineage_of(slot)?.origin.policy_root().to_owned();
        let names = self
            .prefix(slot)
            .names()
            .iter()
            .map(|name| name.as_str().to_owned())
            .chain(prefix.iter().cloned())
            .collect::<Vec<_>>();
        let names = RelativeNames::new(names).map_err(|fault| {
            invalid(format!(
                "the prefix is not a `RelativePath`: {}",
                fault.error
            ))
        })?;
        let expression = sub_policy(&root, &names);
        if let Err(refusal) = self.counted(slot, &expression) {
            return Ok(Narrowed::Refused(refusal));
        }
        let Ok(policy) = Policy::new(&expression, 0) else {
            return Ok(Narrowed::Refused(ScopeErrorCase::PolicyTooComplex));
        };
        self.child(slot, Constraint::Prefix { names, policy })
            .map(Narrowed::Grant)
    }

    fn limit(&mut self, slot: u32, limits: GrantLimits) -> Result<Narrowed, HostFault> {
        // The smallest bound of each kind along the ancestry.
        let mut smallest = [None::<u64>; 3];
        for counters in
            ancestry(&self.slots, slot).filter_map(|slot| slot.constraint.counters())
        {
            let bounds = [
                counters.invocations.bound,
                counters.input_bytes.bound,
                counters.output_bytes.bound,
            ];
            for (least, bound) in smallest.iter_mut().zip(bounds) {
                *least = match (*least, bound) {
                    (Some(least), Some(bound)) => Some(least.min(bound)),
                    (least, bound) => least.or(bound),
                };
            }
        }
        let [invocations, input_bytes, output_bytes] = smallest;
        let keep = |written: Option<u64>, ancestor: Option<u64>| {
            written.map(|written| {
                ancestor.map_or(written, |ancestor| written.min(ancestor))
            })
        };
        let counters = Counters::new(Limits {
            invocations: keep(limits.invocations, invocations),
            input_bytes: keep(limits.input_bytes, input_bytes),
            output_bytes: keep(limits.output_bytes, output_bytes),
        });
        self.child(slot, Constraint::Limits(counters))
            .map(Narrowed::Grant)
    }
}

/// The policy a `sub` with the whole prefix `names` contributes: a file rule
/// with all four file actions and any name, and a directory rule listing and
/// stating, both of depth `@tree` below `names`.
fn sub_policy(root: &str, names: &RelativeNames) -> Expr {
    Expr::AnyOf(vec![
        Expr::FileRule(FileRule {
            root: root.to_owned(),
            prefix: names.clone(),
            depth: FileDepth::Tree,
            actions: vec![
                FileAction::Read,
                FileAction::Replace,
                FileAction::Create,
                FileAction::Stat,
            ],
            names: NameTest::AnyName,
        }),
        Expr::DirectoryRule(DirectoryRule {
            root: root.to_owned(),
            prefix: names.clone(),
            depth: DirDepth::Tree,
            actions: vec![DirAction::List, DirAction::Stat],
        }),
    ])
}

/// The fields of a variant case, by the names its declaration gives, in
/// that order: every one required, no other admitted.
fn fields<'v, const N: usize>(
    value: &'v Value,
    names: [&'static str; N],
) -> Result<[&'v Value; N], HostFault> {
    let Value::Variant { fields, .. } = value else {
        return Err(invalid("the policy value is not a value of its type"));
    };
    if fields.len() != N {
        return Err(invalid("the policy value has the wrong fields"));
    }
    let mut found = Vec::with_capacity(N);
    for name in names {
        let value = fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, value)| value)
            .ok_or_else(|| invalid("the policy value has the wrong fields"))?;
        found.push(value);
    }
    found
        .try_into()
        .map_err(|_| invalid("the policy value has the wrong fields"))
}

/// The case of a fieldless variant.
fn fieldless(value: &Value) -> Result<&str, HostFault> {
    match value {
        Value::Variant { case, fields } if fields.is_empty() => Ok(case),
        _ => Err(invalid("the policy value is not a value of its type")),
    }
}

fn list(value: &Value) -> Result<&[Value], HostFault> {
    match value {
        Value::List(items) => Ok(items),
        _ => Err(invalid("the policy value is not a value of its type")),
    }
}

fn text(value: &Value) -> Result<&str, HostFault> {
    match value {
        Value::Str(text) => Ok(text),
        _ => Err(invalid("the policy value is not a value of its type")),
    }
}

/// A `RelativePath` of a source policy, re-rooted below `prefix`.
fn rerooted(value: &Value, prefix: &RelativeNames) -> Result<RelativeNames, HostFault> {
    let Value::Wrapper(inner) = value else {
        return Err(invalid("the policy's prefix is not a `RelativePath`"));
    };
    let own = list(inner)?
        .iter()
        .map(|name| text(name).map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    let names = prefix
        .names()
        .iter()
        .map(|name| name.as_str().to_owned())
        .chain(own);
    RelativeNames::new(names).map_err(|fault| {
        invalid(format!(
            "the policy's prefix is not a `RelativePath`: {}",
            fault.error
        ))
    })
}

/// A wrapper being decoded: its join, the items left, the items done.
struct Open<'v, T> {
    join: Join,
    rest: std::slice::Iter<'v, Value>,
    done: Vec<T>,
}

/// Decodes a tree of variants whose wrappers are `allOf` and `anyOf` with
/// `items`, without recursion; `leaf` decodes every other case.
fn decode_tree<'v, T>(
    root: &'v Value,
    wrap: fn(Join, Vec<T>) -> T,
    mut leaf: impl FnMut(&'v Value, &str) -> Result<T, HostFault>,
) -> Result<T, HostFault> {
    let mut stack: Vec<Open<'v, T>> = Vec::new();
    let mut node = root;
    loop {
        let Value::Variant { case, .. } = node else {
            return Err(invalid("the policy value is not a value of its type"));
        };
        let join = match case.as_str() {
            "allOf" => Some(Join::AllOf),
            "anyOf" => Some(Join::AnyOf),
            _ => None,
        };
        let mut value = match join {
            Some(join) => {
                let [items] = fields(node, ["items"])?;
                let items = list(items)?;
                let mut rest = items.iter();
                if let Some(first) = rest.next() {
                    stack.push(Open {
                        join,
                        rest,
                        done: Vec::with_capacity(items.len()),
                    });
                    node = first;
                    continue;
                }
                wrap(join, Vec::new())
            }
            None => leaf(node, case)?,
        };
        loop {
            let Some(mut open) = stack.pop() else {
                return Ok(value);
            };
            open.done.push(value);
            if let Some(next) = open.rest.next() {
                stack.push(open);
                node = next;
                break;
            }
            value = wrap(open.join, open.done);
        }
    }
}

const fn wrap_expr(join: Join, items: Vec<Expr>) -> Expr {
    match join {
        Join::AllOf => Expr::AllOf(items),
        Join::AnyOf => Expr::AnyOf(items),
    }
}

const fn wrap_names(join: Join, items: Vec<NameTest>) -> NameTest {
    match join {
        Join::AllOf => NameTest::AllOf(items),
        Join::AnyOf => NameTest::AnyOf(items),
    }
}

fn decode_names(value: &Value, empty_suffix: &mut bool) -> Result<NameTest, HostFault> {
    decode_tree(value, wrap_names, |node, case| match case {
        "anyName" => {
            fields(node, [])?;
            Ok(NameTest::AnyName)
        }
        "exact" => {
            let [names] = fields(node, ["names"])?;
            let names = list(names)?
                .iter()
                .map(|name| text(name).map(str::to_owned))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(NameTest::Exact(names))
        }
        "suffix" => {
            let [value] = fields(node, ["value"])?;
            let value = text(value)?;
            if value.is_empty() {
                *empty_suffix = true;
            }
            Ok(NameTest::Suffix(value.to_owned()))
        }
        _ => Err(invalid("the name test is not a value of `NameTest`")),
    })
}

fn file_action(value: &Value) -> Result<FileAction, HostFault> {
    match fieldless(value)? {
        "read" => Ok(FileAction::Read),
        "replace" => Ok(FileAction::Replace),
        "create" => Ok(FileAction::Create),
        "stat" => Ok(FileAction::Stat),
        _ => Err(invalid("the action is not a value of `FileAction`")),
    }
}

fn dir_action(value: &Value) -> Result<DirAction, HostFault> {
    match fieldless(value)? {
        "list" => Ok(DirAction::List),
        "stat" => Ok(DirAction::Stat),
        _ => Err(invalid("the action is not a value of `DirAction`")),
    }
}

/// Decodes a source `FsPolicy` value, rooted at the grant it restricts: each
/// rule's `root` is `root`, and its `prefix` is re-rooted below `prefix`.
///
/// The inner result is the typed refusal: exactly an empty suffix is
/// `invalidPolicy`.
///
/// # Errors
///
/// A value that is not of the type `FsPolicy`, which checked source cannot
/// build.
fn decode_policy(
    value: &Value,
    root: &str,
    prefix: &RelativeNames,
) -> Result<Result<Expr, ScopeErrorCase>, HostFault> {
    let mut empty_suffix = false;
    let expression = decode_tree(value, wrap_expr, |node, case| match case {
        "none" => {
            fields(node, [])?;
            Ok(Expr::None)
        }
        "files" => {
            let [at, depth, actions, names] =
                fields(node, ["prefix", "depth", "actions", "names"])?;
            let depth = match fieldless(depth)? {
                "children" => FileDepth::Children,
                "tree" => FileDepth::Tree,
                _ => return Err(invalid("the depth is not a value of `FileDepth`")),
            };
            Ok(Expr::FileRule(FileRule {
                root: root.to_owned(),
                prefix: rerooted(at, prefix)?,
                depth,
                actions: list(actions)?
                    .iter()
                    .map(file_action)
                    .collect::<Result<_, _>>()?,
                names: decode_names(names, &mut empty_suffix)?,
            }))
        }
        "directories" => {
            let [at, depth, actions] = fields(node, ["prefix", "depth", "actions"])?;
            let depth = match fieldless(depth)? {
                "self" => DirDepth::Itself,
                "children" => DirDepth::Children,
                "tree" => DirDepth::Tree,
                _ => return Err(invalid("the depth is not a value of `DirDepth`")),
            };
            Ok(Expr::DirectoryRule(DirectoryRule {
                root: root.to_owned(),
                prefix: rerooted(at, prefix)?,
                depth,
                actions: list(actions)?
                    .iter()
                    .map(dir_action)
                    .collect::<Result<_, _>>()?,
            }))
        }
        _ => Err(invalid("the policy is not a value of `FsPolicy`")),
    })?;
    if empty_suffix {
        return Ok(Err(ScopeErrorCase::InvalidPolicy));
    }
    Ok(Ok(expression))
}

impl HostContext for Context<'_> {
    fn host(&mut self, request: HostRequest<'_>) -> Result<HostCompletion, HostFault> {
        // Step 1.
        let slot = self.validate(request.grant, Some(request.op))?;
        let error = request.op.row().error;
        // Step 2.
        let derived = derive(request.op, request.arguments, &self.prefix(slot))?;
        if let Some(name) = derived.variable
            && !self.variable_permitted(slot, name)
        {
            return Self::refusal(error, "denied");
        }
        // Step 3.
        if derived.filesystem && !self.permits_any(slot, &derived.names, derived.may) {
            return Self::refusal(error, "denied");
        }
        if derived.fixed.input > request.room {
            return Err(HostFault::MemoryExhausted);
        }
        let fixed = Charge {
            invocations: 1,
            ..derived.fixed
        };
        match reserve(&mut self.slots, slot, fixed) {
            Ok(()) => {}
            Err(Shortfall::Overflow) => return Self::refusal(error, "denied"),
            Err(Shortfall::Exceeded) => return Self::refusal(error, "limitExceeded"),
        }
        // Step 4.
        let resource = self.lineage_of(slot)?.resource;
        let action = Action {
            op: request.op,
            names: &derived.names,
            operand: derived.operand,
        };
        let mut gate = Admission {
            slots: &mut self.slots,
            slot,
            may: derived.may,
            filesystem: derived.filesystem,
            max: derived.max,
            text: derived.text,
            room: request.room,
            answer: None,
            twice: false,
        };
        let completion = self.driver.get_mut().call(resource, &action, &mut gate);
        let (answer, twice) = (gate.answer, gate.twice);
        let reserved = Charge {
            input: fixed
                .input
                .saturating_add(answer.and_then(Result::ok).unwrap_or(0)),
            ..fixed
        };
        if twice {
            return Err(self.charge_all(
                slot,
                reserved,
                "the driver asked its gate twice",
            ));
        }
        self.finish(&request, slot, reserved, answer, completion)
    }

    fn commit(&mut self, receipt: Receipt) -> Result<(), HostFault> {
        let identity = self.identity;
        let pending = self
            .pending
            .take_if(|pending| {
                receipt.context() == identity && pending.serial == receipt.serial()
            })
            .ok_or_else(|| invalid("the receipt is not the pending event's"))?;
        self.record(Event::Host(pending.event), pending.policy);
        Ok(())
    }

    fn narrow(&mut self, request: NarrowRequest<'_>) -> Result<Narrowed, HostFault> {
        let directory = |interface: HostInterface| {
            matches!(
                interface,
                HostInterface::DirReadBytes
                    | HostInterface::DirReadText
                    | HostInterface::DirList
                    | HostInterface::DirWriteBytes
                    | HostInterface::DirWriteText
                    | HostInterface::DirStat
            )
        };
        match request {
            NarrowRequest::Restrict { grant, policy } => {
                let slot = self.validate(grant, None)?;
                if !directory(self.slot(slot)?.interface) {
                    return Err(invalid("only a directory grant can be restricted"));
                }
                self.restrict(slot, policy)
            }
            NarrowRequest::Sub { grant, prefix } => {
                let slot = self.validate(grant, None)?;
                if !directory(self.slot(slot)?.interface) {
                    return Err(invalid("only a directory grant has a `sub`"));
                }
                self.sub(slot, prefix)
            }
            NarrowRequest::Limit { grant, limits } => {
                let slot = self.validate(grant, None)?;
                self.limit(slot, limits)
            }
        }
    }

    fn test_world(
        &mut self,
        op: TestWorldOp,
        world: GrantToken,
        arguments: &[Value],
        _invocation: u64,
    ) -> Result<Issued, HostFault> {
        if !self.test_root || world != GrantToken::new(self.identity, TEST_ROOT, 0) {
            return Err(invalid(format!(
                "`{}` takes the test root of its own context",
                op.symbol()
            )));
        }
        if self.pending.is_some() {
            return Err(invalid("the event of the previous call is not committed"));
        }
        let Bound::World(driver) = &mut self.driver else {
            return Err(invalid("only a context over a fake world has a test root"));
        };
        match test_world::member(op, driver, arguments)? {
            Made::Roots(mut roots) => {
                roots.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
                let mut grants = Vec::with_capacity(roots.len());
                for (name, root) in roots {
                    grants.push((name, self.issue(test_grant(root))?));
                }
                Ok(Issued::Bundle(grants))
            }
            Made::Root(root) => self.issue(test_grant(root)).map(Issued::Grant),
            Made::Output { stdout, stderr } => Ok(Issued::Output { stdout, stderr }),
        }
    }
}

/// The root grant of a `TestWorld` member's root: origin
/// `{kind: @testWorld}`, every name of the environment, and no limits.
fn test_grant(root: Root) -> RootGrant {
    RootGrant {
        interface: root.interface,
        resource: root.resource,
        origin: GrantOrigin::TestWorld,
        policy: root.policy,
        variables: None,
        limits: Limits::default(),
    }
}

/// Test hooks: the `vibra-host` tests issue roots, release resources, and
/// read the counters through these. They exist only with the `testing`
/// feature, which no shipped build enables.
#[cfg(feature = "testing")]
impl Context<'_> {
    /// `Context::issue`, for the tests.
    ///
    /// # Errors
    ///
    /// As `issue`.
    pub fn root_for_tests(&mut self, root: RootGrant) -> Result<GrantToken, HostFault> {
        self.issue(root)
    }

    /// `Context::release`, for the tests.
    pub fn release_for_tests(&mut self, resource: ResourceId) {
        self.release(resource);
    }

    /// `Context::startup_event`, for the tests.
    ///
    /// # Errors
    ///
    /// As `startup_event`.
    pub fn startup_event_for_tests(
        &mut self,
        leaf: LeafName,
        provider: Provider,
        status: StartupStatus,
        grant: Option<GrantToken>,
        failure: Option<StartupFailure>,
    ) -> Result<(), HostFault> {
        self.startup_event(leaf, provider, status, grant, failure)
    }

    /// The counters of the slot `grant` names, when it holds counters.
    #[must_use]
    pub fn counters_for_tests(&self, grant: GrantToken) -> Option<Counters> {
        let slot = self.slots.get(usize::try_from(grant.slot()).ok()?)?;
        slot.constraint.counters().copied()
    }

    /// Sets what the counters of the slot `grant` names have charged:
    /// invocations, input bytes, output bytes.
    pub fn preset_for_tests(&mut self, grant: GrantToken, used: [u64; 3]) {
        let Some(counters) = usize::try_from(grant.slot())
            .ok()
            .and_then(|index| self.slots.get_mut(index))
            .and_then(|slot| slot.constraint.counters_mut())
        else {
            return;
        };
        let [invocations, input, output] = used;
        counters.invocations.used = invocations;
        counters.input_bytes.used = input;
        counters.output_bytes.used = output;
    }

    /// The slot `grant` names, printed, to show that nothing wrote it.
    #[must_use]
    pub fn slot_for_tests(&self, grant: GrantToken) -> Option<String> {
        let slot = self.slots.get(usize::try_from(grant.slot()).ok()?)?;
        Some(format!("{slot:?}"))
    }
}
