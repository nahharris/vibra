//! The driver trait between the broker and the world, and the gate.
//!
//! [`Driver`] is the one seam between the broker of
//! [`crate::broker::Context`] and whatever acts for a run: the host world of
//! a test or a conformance case, or the process. Policy, counters, and the
//! order of a call stay above the seam, in the broker, so every driver is run
//! by the same code (`docs/spec/06-runtime.md`, "Host calls").
//!
//! A driver is handed a [`ResourceId`] the broker resolved from a validated
//! grant, never a token, and an [`Action`] in the coordinates of the resource:
//! a filesystem request already carries the prefix of every `sub` above it.
//!
//! **The gate.** Step 4 of a host call: what only the opened object decides.
//! A driver that opens something calls [`Gate::admit`] with what it
//! [`Found`], before it reads, creates, or changes anything. The gate decides
//! the policy on the names the filesystem reports and the action found, then
//! the hard-link rule, then reserves the input bytes the driver may read, and
//! answers [`Reserved`] or [`Refused`]. A driver whose gate refused returns
//! [`Completion::Refused`] and does nothing more; any other completion after
//! a refusal is a defect of the driver, which the broker reports as
//! `@runtime.invalidHostValue`.
//!
//! **Progress.** A completion that is due an event carries the [`Progress`]
//! the adapter measured, apart from the typed reply: the broker charges the
//! counters from it, never from the reply.

use vibra_ir::authority::{HostErrorCase, HostReply};
use vibra_ir::external::host::HostOp;

use crate::recipe::Provider;

/// One resource a driver acquired: a held directory, a stream, a variable
/// set, the arguments, a clock, or a random source. It is the driver's
/// index, not authority: the broker passes it only after it validated a
/// grant whose lineage holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceId(pub u64);

/// What a driver reports about itself for a provider: the `driver` record
/// of the audit trace.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DriverIdentity {
    /// The provider atom the driver reports, such as `@hostWorld.v2` or
    /// `@host.io.stdout`.
    pub provider: String,
    /// The driver's version text.
    pub version: String,
}

/// A recipe's resource after startup resolved it: what a driver acquires.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// A directory, by the path the recipe fixed or startup read.
    Directory {
        /// The path, as the recipe or the startup read gave it.
        path: String,
    },
    /// `@processStream`.
    ProcessStream,
    /// `@variableSet`: the variables an `EnvReadVar` grant may read.
    VariableSet {
        /// The exact names of the variables.
        names: Vec<String>,
    },
    /// `@processArguments`.
    ProcessArguments,
    /// `@systemClock`.
    SystemClock,
    /// `@systemRandom`.
    SystemRandom,
}

/// What [`Driver::acquire`] answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Acquire {
    /// The resource is held.
    Acquired(ResourceId),
    /// The issuer refused: the resource does not exist or is not of the
    /// kind the provider needs.
    Refused,
    /// The issuer failed, for the reason given.
    Failed(String),
}

/// The operands of an [`Action`] besides its names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operand<'a> {
    /// No further operand.
    None,
    /// A `Str`: a text to write, or a variable's name.
    Text(&'a str),
    /// A `Bytes` to write.
    Bytes(&'a [u8]),
    /// A `U64`: the `max` of a stream read, or the `count` of random bytes.
    Count(u64),
}

/// One operation the broker asks a driver to perform, after steps 1 to 3 of
/// the host call admitted it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Action<'a> {
    /// The operation.
    pub op: HostOp,
    /// For a filesystem operation, the names of the entry from the held
    /// directory: the prefix of every `sub` above the grant, then the names
    /// the program wrote. Empty for every other operation.
    pub names: &'a [String],
    /// The other operands.
    pub operand: Operand<'a>,
}

/// The action a driver found it would perform on the object it opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FoundAction {
    /// Reading a file, a stream, a variable, or the arguments.
    Read,
    /// Replacing an existing file.
    Replace,
    /// Creating a new file in an opened parent.
    Create,
    /// Listing a directory.
    List,
    /// The metadata of an entry that is not a directory.
    StatFile,
    /// The metadata of a directory.
    StatDirectory,
}

/// What a driver found after opening, and before it reads, creates, or
/// changes anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Found<'n> {
    /// The names as the filesystem reports them, from the held directory.
    /// For a create: the names of the parent that was opened and is held,
    /// then the proposed leaf. Empty for an operation that is not on the
    /// filesystem.
    pub real_names: &'n [String],
    /// The action found.
    pub action: FoundAction,
    /// The input bytes the read would take, when the driver knows them: the
    /// size of a file, of what remains of an input, of a variable's value, or
    /// of the argument text. `None` for an input of unknown size.
    pub size: Option<u64>,
    /// The link count of the opened file; `None` when the platform reports
    /// none.
    pub links: Option<u32>,
}

/// What the gate reserved: the most input bytes the driver may read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Reserved {
    /// The input bytes reserved, which the driver does not exceed.
    pub input: u64,
}

/// The gate said no. The broker keeps the reason; the driver stops and
/// returns [`Completion::Refused`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Refused(());

impl Refused {
    /// The refusal a gate answers.
    pub(crate) const fn new() -> Self {
        Self(())
    }
}

/// The answer, inside a driver's call, to what only the opened object
/// decides.
pub trait Gate {
    /// Admits or refuses what the driver found. A driver calls it at most
    /// once per call, before it reads, creates, or changes anything.
    ///
    /// # Errors
    ///
    /// [`Refused`]: the driver stops and returns [`Completion::Refused`].
    fn admit(&mut self, found: &Found<'_>) -> Result<Reserved, Refused>;
}

/// What the adapter measured, per charged byte resource, apart from the
/// typed reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Progress {
    /// The input bytes taken from the resource for this call.
    pub input: Measured,
    /// The output bytes sent for this call.
    pub output: Measured,
}

impl Progress {
    /// No byte moved, as the adapter knows.
    pub const NONE: Self = Self {
        input: Measured::Known(0),
        output: Measured::Known(0),
    };
}

/// One measured amount.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Measured {
    /// Exactly this many bytes.
    Known(u64),
    /// The adapter does not know how many.
    Uncertain,
}

/// How a driver's call ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Completion {
    /// The gate refused. Nothing was read, created, or changed.
    Refused,
    /// The adapter knows the action was not submitted: no event.
    NotSubmitted(HostErrorCase),
    /// The adapter call completed: an event is due.
    Completed {
        /// The operation's result.
        reply: HostReply,
        /// What the adapter moved across the boundary.
        progress: Progress,
    },
}

/// The world a run acts on.
pub trait Driver {
    /// The driver bound for `provider`; `None` when the platform has no such
    /// driver, which admission reports as `@tool.unavailable`.
    fn identity(&self, provider: Provider) -> Option<DriverIdentity>;

    /// Acquires the resource of one recipe at startup.
    fn acquire(&mut self, provider: Provider, resource: &Resolved) -> Acquire;

    /// Releases a resource. The broker calls it only after every lineage on
    /// the resource is dead.
    fn release(&mut self, resource: ResourceId);

    /// Step 4 of a host call. `gate` answers what only the opened object
    /// decides, and reserves a size learned after opening.
    fn call(
        &mut self,
        resource: ResourceId,
        action: &Action<'_>,
        gate: &mut dyn Gate,
    ) -> Completion;
}
