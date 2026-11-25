//! Startup: admission, the one order, the three outcomes, and release.
//!
//! `docs/spec/04-programs-and-packages.md`, "Startup order", fixes how the
//! leaves of a binary target are acquired; `docs/spec/06-runtime.md`,
//! "Entry outcome", how they are released; `docs/spec/05-tooling.md`,
//! "Command payloads", the [`StartupRecord`]; and `docs/spec/03-effects.md`,
//! "Target admission", what [`admit`] checks again.
//!
//! # Admission
//!
//! [`admit`] makes four checks, in this order, and returns the first that
//! fails. It acquires nothing.
//!
//! 1. The plan is valid ([`BindingPlan::validate`] again).
//! 2. Conformance: the plan's leaves are the entry's leaves, name by name,
//!    with the same interface and the same optionality.
//! 3. The ceiling: the entry's row, and the row of every startup step (the
//!    interface of the `source` leaf of an `@argumentPath` or a
//!    `@variablePath`), are contained in the target's ceiling. A leaf's
//!    interface is not a row: an entry may take a grant it never performs.
//! 4. Every provider has a driver ([`Driver::identity`] is `Some`), else
//!    the target is `@tool.unavailable`.
//!
//! # Start
//!
//! [`start`] processes the leaves in [`BindingPlan::order`]. For each leaf
//! it resolves the resource (a process read through the `source` grant for
//! an `@argumentPath` or a `@variablePath`), asks the driver to acquire it,
//! and classifies the answer by the issuer's row: acquired, refused, or
//! failed. A refused optional leaf is bound absent and startup goes on; a
//! refused required leaf stops it with `@grant.requiredUnavailable`; a
//! failure stops it with `@grant.acquisitionFailed`, for an optional leaf
//! too. One `@startup` event is appended for each processed leaf.
//!
//! A startup read is a host call on the `source` grant, phase `@startup`,
//! with no parent and its own invocation ordinal. Startup decodes the reply,
//! stores the resolved resource for the leaf, and only then commits the
//! receipt, so the read's event is appended after startup used its reply,
//! exactly as an `await` does. A refusal the issuer's row does not list is a
//! failure, and a read counts against the `source` grant's limits.
//!
//! An acquired resource is put on the teardown list of [`Started`] by the
//! statement after the driver returns it, before the grant is issued and
//! before anything else that can fail; the list's capacity for every leaf is
//! reserved before the first leaf. A fault of the broker (a defect of a
//! driver, or no room to record a grant) ends startup as that fault: no leaf
//! stopped it, so the record has no failure.
//!
//! # Release
//!
//! [`release`] releases the teardown list in reverse, each resource through
//! `Context::release`, which makes every grant on it unusable before it
//! calls the driver. Each acquired binding's status becomes `released`.
//! Dropping a [`Started`] releases the same way, so every exit path, an
//! unwinding one included, releases. Release appends no event.

use std::collections::BTreeMap;

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_ir::Row;
use vibra_ir::authority::{
    GrantToken, HostArgument, HostContext, HostFault, HostOk, HostReply, HostRequest,
};
use vibra_ir::external::host::{HostInterface, HostOp};

use crate::broker::{Context, RootGrant};
use crate::driver::{Acquire, Driver, DriverIdentity, Resolved, ResourceId};
use crate::recipe::{
    BindingFault, BindingPlan, Leaf, LeafName, Problem, Provider, Resource,
};
use crate::trace::{
    Event, FailureCode, GrantOrigin, GrantProjection, StartupFailure, StartupStatus,
};

/// One grant leaf of the checked entry: what conformance compares a recipe
/// with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryLeaf {
    /// The leaf's name, such as `@logs.reader`.
    pub name: LeafName,
    /// The operation interface of the leaf's type, or of the payload of its
    /// `Option`.
    pub interface: HostInterface,
    /// Whether the leaf's type is an `Option`.
    pub optional: bool,
}

/// What admission needs of the checked entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryShape {
    /// The entry's written `performs` row.
    pub row: Row,
    /// The entry's grant leaves.
    pub leaves: Vec<EntryLeaf>,
}

/// How one leaf of the plan fails to conform to the entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Nonconformance {
    /// A leaf of the entry that has no recipe.
    Missing(LeafName),
    /// A recipe for a leaf the entry does not have.
    Extra(LeafName),
    /// The recipe's leaf has another interface than the entry's leaf.
    Interface {
        /// The leaf.
        leaf: LeafName,
        /// The interface of the recipe's leaf.
        plan: HostInterface,
        /// The interface of the entry's leaf.
        entry: HostInterface,
    },
    /// The recipe's leaf is optional where the entry's is not, or the other
    /// way round.
    Optionality {
        /// The leaf.
        leaf: LeafName,
        /// Whether the recipe's leaf is optional.
        plan: bool,
        /// Whether the entry's leaf is optional.
        entry: bool,
    },
}

/// A row that is not contained in the ceiling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutsideCeiling {
    /// The path of the root outside the ceiling, such as
    /// `std.io.StderrText`.
    pub root: String,
    /// The leaf whose startup read performs the root; `None` for a root of
    /// the entry's row.
    pub step: Option<LeafName>,
}

/// Why admission refused a target. Nothing was acquired.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdmissionRefusal {
    /// The plan is not valid: every fault of [`BindingPlan::validate`].
    InvalidPlan(Vec<BindingFault>),
    /// The plan's leaves are not the entry's: `@grant.invalidBinding`.
    Nonconforming(Vec<Nonconformance>),
    /// The entry's row or a startup step's row is not contained in the
    /// ceiling: `@effect.outsideCeiling`.
    OutsideCeiling(Vec<OutsideCeiling>),
    /// No driver is bound for these providers: `@tool.unavailable`.
    Unavailable(Vec<Provider>),
}

impl AdmissionRefusal {
    /// The diagnostic code of the refusal; for an invalid plan, that of its
    /// first fault: `@grant.bindingCycle` for a cycle, `@grant.invalidPolicy`
    /// for a foreign root, and `@grant.invalidBinding` otherwise.
    #[must_use]
    pub fn code(&self) -> DiagnosticCode {
        match self {
            Self::InvalidPlan(faults) => {
                match faults.first().map(|fault| &fault.problem) {
                    Some(Problem::Cycle(_)) => DiagnosticCode::GrantBindingCycle,
                    Some(Problem::ForeignRoot(_)) => DiagnosticCode::GrantInvalidPolicy,
                    _ => DiagnosticCode::GrantInvalidBinding,
                }
            }
            Self::Nonconforming(_) => DiagnosticCode::GrantInvalidBinding,
            Self::OutsideCeiling(_) => DiagnosticCode::EffectOutsideCeiling,
            Self::Unavailable(_) => DiagnosticCode::ToolUnavailable,
        }
    }
}

/// An admitted plan: the only input of [`start`]. It holds the driver
/// identity of every provider the plan names.
#[derive(Clone, Debug)]
pub struct Admission<'p> {
    plan: &'p BindingPlan,
    identities: BTreeMap<Provider, DriverIdentity>,
}

impl Admission<'_> {
    /// The admitted plan.
    #[must_use]
    pub const fn plan(&self) -> &BindingPlan {
        self.plan
    }
}

/// The path of the operation interface `interface` as a row names it, such
/// as `std.env.EnvReadArgs`.
fn interface_path(interface: HostInterface) -> String {
    interface.owner().path().trim_start_matches('@').to_owned()
}

/// The leaves of `plan` in the UTF-8 byte order of their names.
fn by_name(plan: &BindingPlan) -> BTreeMap<&LeafName, &Leaf> {
    plan.leaves.iter().map(|leaf| (&leaf.name, leaf)).collect()
}

/// Check 2: the plan's leaves are the entry's, name by name, with the same
/// interface and optionality.
fn conformance(plan: &BindingPlan, entry: &EntryShape) -> Vec<Nonconformance> {
    let recipes = by_name(plan);
    let leaves = entry
        .leaves
        .iter()
        .map(|leaf| (&leaf.name, leaf))
        .collect::<BTreeMap<_, _>>();
    let mut faults = Vec::new();
    for (name, leaf) in &leaves {
        let Some(recipe) = recipes.get(name) else {
            faults.push(Nonconformance::Missing((*name).clone()));
            continue;
        };
        if recipe.interface != leaf.interface {
            faults.push(Nonconformance::Interface {
                leaf: (*name).clone(),
                plan: recipe.interface,
                entry: leaf.interface,
            });
        }
        if recipe.optional != leaf.optional {
            faults.push(Nonconformance::Optionality {
                leaf: (*name).clone(),
                plan: recipe.optional,
                entry: leaf.optional,
            });
        }
    }
    for name in recipes.keys() {
        if !leaves.contains_key(name) {
            faults.push(Nonconformance::Extra((*name).clone()));
        }
    }
    faults
}

/// Check 3: the entry's row and every startup step's row are contained in
/// `ceiling`.
fn outside_ceiling(
    plan: &BindingPlan,
    entry: &EntryShape,
    ceiling: &Row,
) -> Vec<OutsideCeiling> {
    let mut outside = entry
        .row
        .roots()
        .iter()
        .filter(|root| !ceiling.roots().contains(root))
        .map(|root| OutsideCeiling {
            root: root.path().to_owned(),
            step: None,
        })
        .collect::<Vec<_>>();
    let recipes = by_name(plan);
    for leaf in recipes.values() {
        let Some(source) = leaf
            .resource
            .source()
            .and_then(|source| recipes.get(source))
        else {
            continue;
        };
        let root = interface_path(source.interface);
        if !ceiling.roots().iter().any(|known| known.path() == root) {
            outside.push(OutsideCeiling {
                root,
                step: Some(leaf.name.clone()),
            });
        }
    }
    outside
}

/// Admits `plan` for an entry of shape `entry` under the target's ceiling
/// `ceiling`, on `driver`. It acquires nothing.
///
/// # Errors
///
/// The first of the four checks that fails, in the order of the module's
/// documentation.
pub fn admit<'p>(
    plan: &'p BindingPlan,
    entry: &EntryShape,
    ceiling: &Row,
    driver: &dyn Driver,
) -> Result<Admission<'p>, AdmissionRefusal> {
    plan.validate().map_err(AdmissionRefusal::InvalidPlan)?;
    let nonconforming = conformance(plan, entry);
    if !nonconforming.is_empty() {
        return Err(AdmissionRefusal::Nonconforming(nonconforming));
    }
    let outside = outside_ceiling(plan, entry, ceiling);
    if !outside.is_empty() {
        return Err(AdmissionRefusal::OutsideCeiling(outside));
    }
    let mut identities = BTreeMap::new();
    let mut unavailable = Vec::new();
    for leaf in by_name(plan).values() {
        if identities.contains_key(&leaf.provider)
            || unavailable.contains(&leaf.provider)
        {
            continue;
        }
        match driver.identity(leaf.provider) {
            Some(identity) => {
                identities.insert(leaf.provider, identity);
            }
            None => unavailable.push(leaf.provider),
        }
    }
    if !unavailable.is_empty() {
        return Err(AdmissionRefusal::Unavailable(unavailable));
    }
    Ok(Admission { plan, identities })
}

/// The status of a startup record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RecordStatus {
    /// `notStarted`: startup did not begin, as when admission refused.
    NotStarted,
    /// `acquired`: every leaf was processed.
    Acquired,
    /// `failed`: a leaf stopped startup, or a fault ended it.
    Failed,
}

impl RecordStatus {
    /// The status as the record spells it.
    #[must_use]
    pub const fn spelling(self) -> &'static str {
        match self {
            Self::NotStarted => "notStarted",
            Self::Acquired => "acquired",
            Self::Failed => "failed",
        }
    }
}

/// The status of one processed leaf in the record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BindingStatus {
    /// The leaf holds a grant. Only until [`release`]: every acquired
    /// binding of a finished command is `released`.
    Acquired,
    /// `refused`.
    Refused,
    /// `failed`.
    Failed,
    /// `released`: the leaf was acquired, and its grant was released.
    Released,
}

impl BindingStatus {
    /// The status as the record spells it.
    #[must_use]
    pub const fn spelling(self) -> &'static str {
        match self {
            Self::Acquired => "acquired",
            Self::Refused => "refused",
            Self::Failed => "failed",
            Self::Released => "released",
        }
    }
}

/// One processed leaf of the startup record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingRecord {
    /// The leaf.
    pub name: LeafName,
    /// The recipe identity of the leaf's recipe.
    pub recipe_identity: String,
    /// The driver bound for the leaf's provider.
    pub provider_identity: DriverIdentity,
    /// The leaf's outcome.
    pub status: BindingStatus,
    /// The projection of the leaf's grant, exactly when it was acquired.
    pub grant_projection: Option<GrantProjection>,
}

/// The leaf that stopped startup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordFailure {
    /// The leaf.
    pub binding: LeafName,
    /// `@grant.requiredUnavailable` or `@grant.acquisitionFailed`.
    pub code: FailureCode,
    /// The leaf's recipe in the project file.
    pub span: ByteSpan,
}

/// The startup record of `docs/spec/05-tooling.md`, "Command payloads".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupRecord {
    /// The status.
    pub status: RecordStatus,
    /// One per processed leaf, in processing order.
    pub bindings: Vec<BindingRecord>,
    /// The events startup appended: the `@startup` events and the `@host`
    /// events of phase `@startup`, in order.
    pub events: Vec<Event>,
    /// The leaf that stopped startup; `None` when none did, as when a fault
    /// ended it.
    pub failure: Option<RecordFailure>,
}

impl StartupRecord {
    /// The record of a startup that did not begin.
    #[must_use]
    pub const fn not_started() -> Self {
        Self {
            status: RecordStatus::NotStarted,
            bindings: Vec::new(),
            events: Vec::new(),
            failure: None,
        }
    }
}

/// One resource on the teardown list, and the index its leaf's binding
/// record has once it is reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Held {
    resource: ResourceId,
    binding: usize,
}

/// The teardown list: every resource startup acquired, in acquisition order.
/// Dropping it releases what is left on it, in reverse.
#[derive(Debug)]
struct Teardown<'c, 'd> {
    context: &'c mut Context<'d>,
    held: Vec<Held>,
}

impl Drop for Teardown<'_, '_> {
    fn drop(&mut self) {
        while let Some(held) = self.held.pop() {
            self.context.release(held.resource);
        }
    }
}

/// A startup that ran: its record, the entry's bindings, the ordinal of the
/// entry's invocation, and the teardown list. Dropping it releases every
/// acquired resource; [`release`] does so and reports it.
#[derive(Debug)]
pub struct Started<'c, 'd> {
    /// The startup record.
    pub record: StartupRecord,
    /// Each leaf and what it is bound to, in the order of their names;
    /// `None` when startup failed, so no entry argument was built.
    pub bindings: Option<Vec<(LeafName, Option<GrantToken>)>>,
    /// One more than the number of startup reads: the ordinal of the
    /// launcher's run of the entry.
    pub next_invocation: u64,
    /// The fault that ended startup, when one did: a defect of a driver,
    /// `@runtime.invalidHostValue`, or memory exhaustion.
    pub fault: Option<HostFault>,
    teardown: Teardown<'c, 'd>,
}

impl<'d> Started<'_, 'd> {
    /// The context the grants live in, to run the entry.
    pub fn context(&mut self) -> &mut Context<'d> {
        &mut *self.teardown.context
    }
}

/// The outcome of one attempt.
enum Outcome {
    Acquired(GrantToken),
    Refused(String),
    Failed(String),
}

/// What a startup read resolved.
enum Read {
    Path(String),
    Refused(String),
    Failed(String),
}

/// Why startup stopped early.
enum Stop {
    /// A leaf stopped it.
    Leaf,
    /// The broker faulted.
    Fault(HostFault),
}

fn invalid(reason: impl Into<String>) -> HostFault {
    HostFault::InvalidHostValue(reason.into())
}

/// Whether the issuer's row lists any refusal: the filesystem and the
/// streams do; the environment, the clocks, and random never refuse.
const fn row_refuses(provider: Provider) -> bool {
    matches!(
        provider,
        Provider::FsDirectory
            | Provider::IoStdin
            | Provider::IoStdout
            | Provider::IoStderr
    )
}

/// The resolved resource of a recipe whose resource is not read from the
/// process.
fn fixed(resource: &Resource) -> Option<Resolved> {
    Some(match resource {
        Resource::FixedPath { path } => Resolved::Directory { path: path.clone() },
        Resource::ProcessStream => Resolved::ProcessStream,
        Resource::VariableSet { names } => Resolved::VariableSet {
            names: names.clone(),
        },
        Resource::ProcessArguments => Resolved::ProcessArguments,
        Resource::SystemClock => Resolved::SystemClock,
        Resource::SystemRandom => Resolved::SystemRandom,
        Resource::ArgumentPath { .. } | Resource::VariablePath { .. } => return None,
    })
}

/// Classifies the reply of a startup read: the path it read, or a refusal
/// the directory row lists, or a failure. `None` for a reply of another
/// shape than the row's.
fn classify(op: HostOp, reply: &HostReply, index: Option<u64>) -> Option<Read> {
    Some(match (op, reply) {
        (HostOp::EnvReadArgs, HostReply::Ok(HostOk::Names(arguments))) => {
            let argument = index
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| arguments.get(index));
            match argument {
                Some(path) => Read::Path(path.clone()),
                None => Read::Refused(
                    "the argument index is at or beyond the end of the arguments"
                        .to_owned(),
                ),
            }
        }
        (HostOp::EnvReadVar, HostReply::Ok(HostOk::Text(path))) => {
            Read::Path(path.clone())
        }
        (HostOp::EnvReadVar, HostReply::Err(case)) if case.case() == "notPresent" => {
            Read::Refused("the variable is not present".to_owned())
        }
        // The variable's value is the path, so the path is not Unicode.
        (HostOp::EnvReadVar, HostReply::Err(case)) if case.case() == "notUnicode" => {
            Read::Refused("the path is not Unicode".to_owned())
        }
        (HostOp::EnvReadArgs | HostOp::EnvReadVar, HostReply::Err(case)) => {
            Read::Failed(format!("the startup read returned `{}`", case.case()))
        }
        _ => return None,
    })
}

/// One run of startup.
struct Run<'a, 'c, 'd> {
    admission: &'a Admission<'a>,
    teardown: Teardown<'c, 'd>,
    record: StartupRecord,
    bound: BTreeMap<LeafName, Option<GrantToken>>,
    reads: u64,
}

impl Run<'_, '_, '_> {
    /// Reads the path of `leaf` through its `source` grant.
    fn read(&mut self, leaf: &Leaf) -> Result<Read, HostFault> {
        let (op, index, name, source) = match &leaf.resource {
            Resource::ArgumentPath { index, source } => {
                (HostOp::EnvReadArgs, Some(*index), None, source)
            }
            Resource::VariablePath { name, source } => {
                (HostOp::EnvReadVar, None, Some(name.as_str()), source)
            }
            _ => return Err(invalid("the resource is not read from the process")),
        };
        let token = match self.bound.get(source) {
            Some(Some(token)) => *token,
            Some(None) => {
                return Ok(Read::Refused(format!(
                    "the source leaf `{source}` was bound absent"
                )));
            }
            None => return Err(invalid("the source leaf was not processed first")),
        };
        self.reads = self.reads.saturating_add(1);
        let arguments = name.map(HostArgument::Text);
        let arguments = arguments.as_slice();
        let completion = self.teardown.context.host(HostRequest {
            op,
            grant: token,
            arguments,
            invocation: self.reads,
            parent: None,
            origin: None,
            room: u64::MAX,
        })?;
        // Decode the reply; a reply of another shape drops its receipt.
        let read = classify(op, &completion.reply, index).ok_or_else(|| {
            invalid("the startup read's reply is not of its row's type")
        })?;
        // Store what the read resolved for the leaf, then commit.
        let stored = read;
        #[cfg(feature = "testing")]
        stored_for_tests(
            &leaf.name,
            &stored,
            self.teardown.context.trace().events.len(),
        );
        if let Some(receipt) = completion.receipt {
            self.teardown.context.commit(receipt)?;
        }
        Ok(stored)
    }

    /// Resolves, acquires, and issues one leaf.
    fn attempt(&mut self, leaf: &Leaf) -> Result<Outcome, HostFault> {
        let resolved = match fixed(&leaf.resource) {
            Some(resolved) => resolved,
            None => match self.read(leaf)? {
                Read::Path(path) => Resolved::Directory { path },
                Read::Refused(why) => return Ok(Outcome::Refused(why)),
                Read::Failed(why) => return Ok(Outcome::Failed(why)),
            },
        };
        let resource = match self.teardown.context.acquire(leaf.provider, &resolved) {
            Acquire::Acquired(resource) => {
                self.teardown.held.push(Held {
                    resource,
                    binding: self.record.bindings.len(),
                });
                resource
            }
            Acquire::Refused if row_refuses(leaf.provider) => {
                return Ok(Outcome::Refused(
                    "the issuer refused the resource".to_owned(),
                ));
            }
            Acquire::Refused => {
                return Ok(Outcome::Failed(
                    "the issuer refused, which its row never does".to_owned(),
                ));
            }
            Acquire::Failed(why) => return Ok(Outcome::Failed(why)),
        };
        let variables = match &leaf.resource {
            Resource::VariableSet { names } => Some(names.clone()),
            _ => None,
        };
        let token = self.teardown.context.issue(RootGrant {
            interface: leaf.interface,
            resource,
            origin: GrantOrigin::Binding {
                leaf: leaf.name.clone(),
            },
            policy: leaf.policy.clone(),
            variables,
            limits: leaf.limits,
        })?;
        Ok(Outcome::Acquired(token))
    }

    /// Appends the leaf's `@startup` event and its binding record.
    fn report(
        &mut self,
        leaf: &Leaf,
        status: StartupStatus,
        grant: Option<GrantToken>,
        failure: Option<StartupFailure>,
    ) -> Result<(), HostFault> {
        let code = failure.as_ref().map(|failure| failure.code);
        self.teardown.context.startup_event(
            leaf.name.clone(),
            leaf.provider,
            status,
            grant,
            failure,
        )?;
        let grant_projection = match self.teardown.context.trace().events.last() {
            Some(Event::Startup(event)) => event.grant.clone(),
            _ => None,
        };
        let provider_identity = self
            .admission
            .identities
            .get(&leaf.provider)
            .cloned()
            .ok_or_else(|| invalid("the provider was not admitted"))?;
        self.record.bindings.push(BindingRecord {
            name: leaf.name.clone(),
            recipe_identity: leaf.recipe_identity.clone(),
            provider_identity,
            status: match status {
                StartupStatus::Acquired => BindingStatus::Acquired,
                StartupStatus::Refused => BindingStatus::Refused,
                StartupStatus::Failed => BindingStatus::Failed,
            },
            grant_projection,
        });
        if let Some(code) = code {
            self.record.failure = Some(RecordFailure {
                binding: leaf.name.clone(),
                code,
                span: leaf.span,
            });
        }
        Ok(())
    }

    /// Processes one leaf; `Err` when it stopped startup.
    fn leaf(&mut self, leaf: &Leaf) -> Result<(), Stop> {
        let stop = |message: String, code| StartupFailure { code, message };
        match self.attempt(leaf).map_err(Stop::Fault)? {
            Outcome::Acquired(token) => {
                self.report(leaf, StartupStatus::Acquired, Some(token), None)
                    .map_err(Stop::Fault)?;
                self.bound.insert(leaf.name.clone(), Some(token));
                Ok(())
            }
            Outcome::Refused(_) if leaf.optional => {
                self.report(leaf, StartupStatus::Refused, None, None)
                    .map_err(Stop::Fault)?;
                self.bound.insert(leaf.name.clone(), None);
                Ok(())
            }
            Outcome::Refused(why) => {
                let failure = stop(why, FailureCode::RequiredUnavailable);
                self.report(leaf, StartupStatus::Refused, None, Some(failure))
                    .map_err(Stop::Fault)?;
                Err(Stop::Leaf)
            }
            Outcome::Failed(why) => {
                let failure = stop(why, FailureCode::AcquisitionFailed);
                self.report(leaf, StartupStatus::Failed, None, Some(failure))
                    .map_err(Stop::Fault)?;
                Err(Stop::Leaf)
            }
        }
    }

    /// Processes every leaf in the one startup order, until one stops it.
    fn all(&mut self) -> Result<(), Stop> {
        let plan = self.admission.plan;
        self.teardown
            .held
            .try_reserve_exact(plan.leaves.len())
            .map_err(|_| Stop::Fault(HostFault::MemoryExhausted))?;
        for leaf in plan.order() {
            self.leaf(leaf)?;
        }
        Ok(())
    }
}

/// Runs startup for `admission` on `context`: acquires every leaf in the one
/// startup order, until one stops it or a fault ends it.
pub fn start<'c, 'd>(
    admission: Admission<'_>,
    context: &'c mut Context<'d>,
) -> Started<'c, 'd> {
    let first = context.trace().events.len();
    let mut run = Run {
        admission: &admission,
        teardown: Teardown {
            context,
            held: Vec::new(),
        },
        record: StartupRecord {
            status: RecordStatus::Acquired,
            bindings: Vec::new(),
            events: Vec::new(),
            failure: None,
        },
        bound: BTreeMap::new(),
        reads: 0,
    };
    let outcome = run.all();
    let Run {
        teardown,
        mut record,
        bound,
        reads,
        ..
    } = run;
    let fault = match outcome {
        Ok(()) => None,
        Err(Stop::Leaf) => {
            record.status = RecordStatus::Failed;
            None
        }
        Err(Stop::Fault(fault)) => {
            record.status = RecordStatus::Failed;
            record.failure = None;
            Some(fault)
        }
    };
    record.events = teardown
        .context
        .trace()
        .events
        .get(first..)
        .unwrap_or_default()
        .to_vec();
    let bindings =
        (record.status == RecordStatus::Acquired).then(|| bound.into_iter().collect());
    Started {
        record,
        bindings,
        next_invocation: reads.saturating_add(1),
        fault,
        teardown,
    }
}

/// Releases every resource startup acquired, in reverse acquisition order,
/// and returns the record with each acquired binding `released`. Each
/// release makes every grant on the resource unusable before the driver is
/// called. It appends no event.
#[must_use]
pub fn release(started: Started<'_, '_>) -> StartupRecord {
    let Started {
        mut record,
        mut teardown,
        ..
    } = started;
    while let Some(held) = teardown.held.pop() {
        teardown.context.release(held.resource);
        if let Some(binding) = record.bindings.get_mut(held.binding) {
            binding.status = BindingStatus::Released;
        }
    }
    record
}

/// A test hook that sees each startup read's resolution when it is stored,
/// with the number of events of the trace at that moment.
#[cfg(feature = "testing")]
pub type StoredHook = Box<dyn FnMut(&LeafName, Option<&str>, usize)>;

#[cfg(feature = "testing")]
thread_local! {
    static STORED: std::cell::RefCell<Option<StoredHook>> =
        const { std::cell::RefCell::new(None) };
}

/// Sets the hook of this thread that sees each stored startup read: the
/// leaf, the path read (`None` for a refusal or a failure), and the number
/// of events of the trace when it was stored.
#[cfg(feature = "testing")]
pub fn on_stored_for_tests(hook: Option<StoredHook>) {
    STORED.with(|slot| *slot.borrow_mut() = hook);
}

/// Calls the hook of this thread, if one is set.
#[cfg(feature = "testing")]
fn stored_for_tests(leaf: &LeafName, read: &Read, events: usize) {
    let path = match read {
        Read::Path(path) => Some(path.as_str()),
        Read::Refused(_) | Read::Failed(_) => None,
    };
    STORED.with(|slot| {
        if let Some(hook) = slot.borrow_mut().as_mut() {
            hook(leaf, path, events);
        }
    });
}
