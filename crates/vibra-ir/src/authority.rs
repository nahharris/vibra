//! The one boundary between an executor and the host broker
//! (`docs/spec/06-runtime.md`, "Host calls"): [`HostContext`] and the data
//! it carries. Nothing here implements it.
//!
//! The data is typed by the registry, not by neutral values, so the broker
//! never interprets a program's types. A [`GrantToken`] and a [`Receipt`]
//! are indices, not authority: the broker validates each on every use. A
//! typed refusal (`denied`, `limitExceeded`, `unsupported`,
//! `outcomeUnknown`) is a [`HostReply::Err`]; a [`HostFault`] is only a
//! defect or a host event, never a program result.

use crate::external::host::{HostErrorType, HostOp, TestWorldOp};
use crate::{SourceOrigin, Value};

/// An index into one context's grant table. It is not authority: the broker
/// validates it on every use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantToken {
    context: u64,
    slot: u32,
    generation: u32,
}

impl GrantToken {
    /// The token of `slot` at `generation` in the context `context`.
    #[must_use]
    pub const fn new(context: u64, slot: u32, generation: u32) -> Self {
        Self {
            context,
            slot,
            generation,
        }
    }

    /// The identity of the context whose table the slot is in.
    #[must_use]
    pub const fn context(self) -> u64 {
        self.context
    }

    /// The slot of the grant table.
    #[must_use]
    pub const fn slot(self) -> u32 {
        self.slot
    }

    /// The generation the slot's lineage had when the token was made.
    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

/// One argument of a host operation after its grant, in registry terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostArgument<'v> {
    /// A `Str`.
    Text(&'v str),
    /// A `Bytes`.
    Bytes(&'v [u8]),
    /// A `RelativePath`, by its names, as the program wrote it.
    Path(&'v [String]),
    /// A `U64` count.
    Count(u64),
}

/// One host call: the operation, its grant and arguments, the invocation
/// that performs it, and where it was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostRequest<'v> {
    /// The operation.
    pub op: HostOp,
    /// The grant, the operation's receiver.
    pub grant: GrantToken,
    /// The arguments after the grant, in parameter order.
    pub arguments: &'v [HostArgument<'v>],
    /// The ordinal of the `await` that runs the host-bound member.
    pub invocation: u64,
    /// The ordinal of the invocation whose body performed that `await`;
    /// `None` for a startup read.
    pub parent: Option<u64>,
    /// The source origin of the `await`; `None` when none is known, as
    /// for a startup read.
    pub origin: Option<&'v SourceOrigin>,
    /// The most result bytes the caller's memory budget can still account.
    pub room: u64,
}

/// The kind of a filesystem entry, the cases of `FsKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HostFsKind {
    /// `file`.
    File,
    /// `directory`.
    Directory,
    /// `other`.
    Other,
}

/// The `Ok` payload of an operation's result, one form for each `Ok` type
/// of the registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostOk {
    /// `Void`.
    Void,
    /// `Str`.
    Text(String),
    /// `Bytes`.
    Bytes(Vec<u8>),
    /// `List(Str)`: entry names, or the program's arguments.
    Names(Vec<String>),
    /// `FsMetadata`: the entry's kind and its byte length.
    Metadata {
        /// The kind.
        kind: HostFsKind,
        /// The byte length of a regular file; zero for any other kind.
        size: u64,
    },
    /// `I64`.
    I64(i64),
    /// `U64`.
    U64(u64),
}

/// One case of a host error type: the type, and the name of one of its
/// cases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostErrorCase {
    error: HostErrorType,
    case: &'static str,
}

impl HostErrorCase {
    /// The case named `case` of `error`; `None` when `error` has no such
    /// case.
    #[must_use]
    pub fn new(error: HostErrorType, case: &str) -> Option<Self> {
        error
            .cases()
            .into_iter()
            .find(|known| *known == case)
            .map(|case| Self { error, case })
    }

    /// The error type.
    #[must_use]
    pub const fn error(self) -> HostErrorType {
        self.error
    }

    /// The case's name, such as `denied`.
    #[must_use]
    pub const fn case(self) -> &'static str {
        self.case
    }
}

/// The complete `Result` of one operation, in registry terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostReply {
    /// `Result.ok` with its payload.
    Ok(HostOk),
    /// `Result.err` with a case of the row's error type.
    Err(HostErrorCase),
}

/// A pending audit event. Neither `Clone` nor `Copy`: committing consumes
/// it. Like a token it is an index, and the broker validates it.
#[derive(Debug, PartialEq, Eq)]
pub struct Receipt {
    context: u64,
    serial: u64,
}

impl Receipt {
    /// The receipt of the pending event `serial` of the context `context`.
    #[must_use]
    pub const fn new(context: u64, serial: u64) -> Self {
        Self { context, serial }
    }

    /// The identity of the context that holds the pending event.
    #[must_use]
    pub const fn context(&self) -> u64 {
        self.context
    }

    /// The serial of the pending event.
    #[must_use]
    pub const fn serial(&self) -> u64 {
        self.serial
    }
}

/// What [`HostContext::host`] returns: the reply, and the audit event that
/// is due for it.
#[derive(Debug, PartialEq, Eq)]
pub struct HostCompletion {
    /// The reply.
    pub reply: HostReply,
    /// `None` when no event is due: a refusal before the adapter, or a call
    /// known not submitted.
    pub receipt: Option<Receipt>,
}

/// The bounds a `limit` narrowing sets, the fields of `GrantLimits`; `None`
/// leaves a bound as it was.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GrantLimits {
    /// `invocations`.
    pub invocations: Option<u64>,
    /// `inputBytes`.
    pub input_bytes: Option<u64>,
    /// `outputBytes`.
    pub output_bytes: Option<u64>,
}

/// One narrowing of a sealed grant (`docs/spec/03-effects.md`, "Narrowing").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NarrowRequest<'v> {
    /// `restrict`: the grant and the `FsPolicy` value.
    Restrict {
        /// The grant narrowed.
        grant: GrantToken,
        /// The policy.
        policy: &'v Value,
    },
    /// `sub`: the grant and the names of the prefix.
    Sub {
        /// The grant narrowed.
        grant: GrantToken,
        /// The prefix.
        prefix: &'v [String],
    },
    /// `limit`: the grant and the bounds.
    Limit {
        /// The grant narrowed.
        grant: GrantToken,
        /// The bounds.
        limits: GrantLimits,
    },
}

/// A case of `ScopeError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeErrorCase {
    /// `invalidPolicy`.
    InvalidPolicy,
    /// `unsupportedVersion`.
    UnsupportedVersion,
    /// `policyTooComplex`.
    PolicyTooComplex,
    /// `foreignRoot`.
    ForeignRoot,
}

/// What a narrowing gives: the narrower grant, or the typed refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Narrowed {
    /// The narrower grant.
    Grant(GrantToken),
    /// `Result.err` with a case of `ScopeError`.
    Refused(ScopeErrorCase),
}

/// What a member of `TestWorld` gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Issued {
    /// One grant.
    Grant(GrantToken),
    /// A record of grants, by field name.
    Bundle(Vec<(&'static str, GrantToken)>),
    /// The bytes the test wrote to its standard output and error.
    Output {
        /// Standard output.
        stdout: Vec<u8>,
        /// Standard error.
        stderr: Vec<u8>,
    },
}

/// A defect or a host event; never a typed refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostFault {
    /// A token, a receipt, or a reply the broker cannot accept:
    /// `@runtime.invalidHostValue`.
    InvalidHostValue(String),
    /// The caller's memory budget cannot hold the result.
    MemoryExhausted,
}

/// The dispatch boundary every host operation passes, in the interpreter
/// and in native execution alike.
pub trait HostContext {
    /// Steps 1 to 4 of a host call. The event of a completed call is
    /// pending, not appended, when this returns.
    ///
    /// # Errors
    ///
    /// A token the broker cannot accept, or a result the caller's budget
    /// cannot hold.
    fn host(&mut self, request: HostRequest<'_>) -> Result<HostCompletion, HostFault>;

    /// Appends the event of `receipt`. The caller calls it exactly once,
    /// after it has accepted the reply and advanced its continuation.
    ///
    /// # Errors
    ///
    /// A receipt that is not the pending one.
    fn commit(&mut self, receipt: Receipt) -> Result<(), HostFault>;

    /// Narrows a sealed grant.
    ///
    /// # Errors
    ///
    /// A token the broker cannot accept.
    fn narrow(&mut self, request: NarrowRequest<'_>) -> Result<Narrowed, HostFault>;

    /// Runs the member `op` of the test world `world`, with its labelled
    /// arguments, as the invocation `invocation`.
    ///
    /// # Errors
    ///
    /// A token that is not a test world's, or arguments the member does not
    /// take.
    fn test_world(
        &mut self,
        op: TestWorldOp,
        world: GrantToken,
        arguments: &[Value],
        invocation: u64,
    ) -> Result<Issued, HostFault>;
}
