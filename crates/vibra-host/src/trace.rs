//! The `@auditTrace.v2` document, its events, and the grant projections.
//!
//! `docs/spec/06-runtime.md`, "Audit trace", fixes the document: the ordered
//! list of the startup steps of a run and of its completed host calls, with
//! the drivers bound in the run and the policies its projections name. The
//! broker of [`crate::broker::Context`] builds it; nothing else appends to
//! it.
//!
//! - **The document.** [`AuditTrace`] holds `format`, `revision`, `target`,
//!   `drivers`, `policies`, `redaction`, and `events`, in that order. Its
//!   canonical bytes ([`encode`]) are one line, every optional field absent
//!   rather than empty, every number suffixed.
//! - **The events.** A [`StartupEvent`] records one processed leaf, a
//!   [`HostEvent`] one completed host call. Both are closed records whose
//!   first field is `kind`. An event of any other kind is kept as an
//!   [`OpaqueEvent`], its fields in name order.
//! - **The projections.** A [`GrantProjection`] summarizes a grant as data:
//!   where its authority began ([`GrantOrigin`]), the hash of its effective
//!   policy, the interface it permits, the bounds set along its ancestry, and
//!   the ordinal of its root. It holds no slot, no generation, and no context
//!   identity, so nothing decoded from a trace can become a token.
//! - **Values.** The `args` and `result` of a host event are the canonical
//!   value encoding of the runtime chapter, "Canonical value encoding", kept
//!   as their canonical text. Their declared types carry the identities of
//!   the embedded library's declarations, which [`StdlibIdentity`] computes.
//! - **Decoding.** [`decode`] reads the closed schema and returns data only.
//!   It reads each host event's values against the registry row its `op`
//!   names, so a value's depth is bounded by its type, never by its input.
//! - **JSON.** [`HostEvent::json`] and [`StartupEvent::json`] give each field
//!   its own member, an atom as a string keeping its `@`, and a value as its
//!   canonical text inside a string.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::{Value as Json, json};
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::authority::{HostArgument, HostFsKind, HostOk, HostReply};
use vibra_ir::external::host::{
    FS_KIND_CASES, HostErrorType, HostInterface, HostOp, HostType, host_row,
};
use vibra_ir::{EntityRef, SourceOrigin};
use vibra_syntax::{
    DataField, DataNode, DataValue, FieldSchema, IntegerSuffix, RecordSchema, Schema,
    Slot, TypedNode, TypedValue, canonical_string, decode as read, parse_data,
};

use crate::driver::DriverIdentity;
use crate::policy::{self, Policy, PolicyError};
use crate::recipe::{LeafName, Limits, Provider};

/// The format atom of an audit trace.
pub const TRACE_FORMAT: &str = "@auditTrace.v2";

/// The `redaction` of a trace to which no redaction profile was applied.
pub const NO_REDACTION: &str = "@none";

/// The atom every rule of a grant descending from a `TestWorld` directory
/// grant names as its `root`.
const TEST_WORLD_ROOT: &str = "@testWorld";

/// The declaration kind atom of a type.
const TYPE_KIND: &str = "@type";

/// The declaration kind atom of an interface.
pub(crate) const INTERFACE_KIND: &str = "@interface";

/// The identity of the declaration whose logical anchor is the package
/// `package` at `version`, the module path atom `module` (with its `@`), the
/// dotted `owner` path within the module (empty for a top-level
/// declaration), the kind atom `kind` (with its `@`), and the name `name`:
/// `docs/spec/02-type-system.md`, "Declared types".
///
/// It is `sha256:` and the lower-case hexadecimal SHA-256 of the text
/// `vibraDeclarationIdentityV2`, one zero byte, and the six strings, each
/// preceded by its length as a big-endian 64-bit integer.
#[must_use]
pub fn declaration_identity(
    package: &str,
    version: &str,
    module: &str,
    owner: &str,
    kind: &str,
    name: &str,
) -> String {
    let mut bytes = b"vibraDeclarationIdentityV2\0".to_vec();
    for part in [package, version, module, owner, kind, name] {
        let length = u64::try_from(part.len()).unwrap_or(u64::MAX);
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(part.as_bytes());
    }
    policy::sha256(&bytes)
}

/// The package of the embedded standard library, whose name and version
/// anchor the identity of every declaration it holds: the `seed` a trace
/// writes for a library type or interface.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StdlibIdentity {
    /// The package name, `std`.
    pub package: String,
    /// The package version.
    pub version: String,
}

impl StdlibIdentity {
    /// The identity of the top-level declaration `name` of kind `kind` in the
    /// library module `module`.
    #[must_use]
    pub fn seed(&self, module: &str, kind: &str, name: &str) -> String {
        declaration_identity(&self.package, &self.version, module, "", kind, name)
    }
}

/// What a run gives its trace before anything is recorded: the fields of the
/// document that no event decides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceHeader {
    /// The workspace revision of the source that runs.
    pub revision: String,
    /// The binary target's atom; `None` for a test or an evaluation.
    pub target: Option<String>,
    /// The embedded library, whose identities the values of events carry.
    pub library: StdlibIdentity,
}

/// The origin record: where a grant's authority began.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GrantOrigin {
    /// `{kind: @binding leaf: …}`: a startup leaf of a target.
    Binding {
        /// The leaf.
        leaf: LeafName,
    },
    /// `{kind: @testWorld}`: a member of a test's `TestWorld`.
    TestWorld,
}

impl GrantOrigin {
    /// The `root` of every rule a narrowing of a grant of this origin
    /// contributes: the leaf, or `@testWorld`.
    #[must_use]
    pub fn policy_root(&self) -> &str {
        match self {
            Self::Binding { leaf } => leaf.as_str(),
            Self::TestWorld => TEST_WORLD_ROOT,
        }
    }

    /// The record's canonical bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> String {
        match self {
            Self::Binding { leaf } => format!("{{kind: @binding leaf: {leaf}}}"),
            Self::TestWorld => "{kind: @testWorld}".to_owned(),
        }
    }

    fn json(&self) -> Json {
        match self {
            Self::Binding { leaf } => {
                json!({"kind": "@binding", "leaf": leaf.as_str()})
            }
            Self::TestWorld => json!({"kind": "@testWorld"}),
        }
    }
}

/// A grant as data: never a usable handle, and decoding one grants nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrantProjection {
    /// Where the grant's authority began; narrowing keeps it.
    pub issued_by: GrantOrigin,
    /// The hash of the normalized effective policy, a key of the trace's
    /// `policies`; empty for a grant whose ancestry has no policy.
    pub policy: String,
    /// The one operation interface the grant permits.
    pub rights: EntityRef,
    /// The smallest bound of each kind set along the ancestry; `None` when
    /// none is set.
    pub limits: Option<Limits>,
    /// The run-local ordinal of the root whose accounting the grant shares.
    pub root: u64,
}

impl GrantProjection {
    /// The record's canonical bytes: `issuedBy`, `policy`, `rights`,
    /// `limits` when a bound is set, then `root`.
    #[must_use]
    pub fn canonical_bytes(&self) -> String {
        let mut out = String::from("{issuedBy: ");
        out.push_str(&self.issued_by.canonical_bytes());
        out.push_str(" policy: ");
        out.push_str(&canonical_string(&self.policy));
        out.push_str(" rights: [");
        out.push_str(&entity(&self.rights));
        out.push(']');
        if let Some(limits) = &self.limits {
            out.push_str(" limits: ");
            out.push_str(&limits.canonical_bytes());
        }
        let _ = write!(out, " root: {}u64}}", self.root);
        out
    }

    /// The JSON projection: each field its own member.
    #[must_use]
    pub fn json(&self) -> Json {
        let limits = self.limits.map_or(Json::Null, |limits| {
            json!({
                "invocations": limits.invocations,
                "inputBytes": limits.input_bytes,
                "outputBytes": limits.output_bytes,
            })
        });
        json!({
            "issuedBy": self.issued_by.json(),
            "policy": self.policy,
            "rights": [{
                "path": format!("@{}", self.rights.path()),
                "seed": self.rights.seed(),
            }],
            "limits": limits,
            "root": self.root,
        })
    }
}

/// The entity reference record of `entity`.
fn entity(entity: &EntityRef) -> String {
    format!(
        "{{path: @{} seed: {}}}",
        entity.path(),
        canonical_string(entity.seed())
    )
}

/// The outcome of one processed startup leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StartupStatus {
    /// `@acquired`: the leaf holds a grant.
    Acquired,
    /// `@refused`: the issuer refused.
    Refused,
    /// `@failed`: the issuer failed.
    Failed,
}

impl StartupStatus {
    /// The status atom.
    #[must_use]
    pub const fn atom(self) -> &'static str {
        match self {
            Self::Acquired => "@acquired",
            Self::Refused => "@refused",
            Self::Failed => "@failed",
        }
    }

    fn from_atom(atom: &str) -> Option<Self> {
        [Self::Acquired, Self::Refused, Self::Failed]
            .into_iter()
            .find(|status| status.atom() == atom)
    }
}

/// The code of a leaf that stopped startup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FailureCode {
    /// `@grant.requiredUnavailable`: a required leaf was refused.
    RequiredUnavailable,
    /// `@grant.acquisitionFailed`: a leaf failed.
    AcquisitionFailed,
}

impl FailureCode {
    /// The code's atom.
    #[must_use]
    pub const fn atom(self) -> &'static str {
        match self {
            Self::RequiredUnavailable => {
                DiagnosticCode::GrantRequiredUnavailable.as_atom()
            }
            Self::AcquisitionFailed => DiagnosticCode::GrantAcquisitionFailed.as_atom(),
        }
    }

    fn from_atom(atom: &str) -> Option<Self> {
        [Self::RequiredUnavailable, Self::AcquisitionFailed]
            .into_iter()
            .find(|code| code.atom() == atom)
    }

    /// The code a leaf of `status` that stopped startup has; `None` for an
    /// acquired leaf, which never stops it.
    #[must_use]
    pub const fn of(status: StartupStatus) -> Option<Self> {
        match status {
            StartupStatus::Acquired => None,
            StartupStatus::Refused => Some(Self::RequiredUnavailable),
            StartupStatus::Failed => Some(Self::AcquisitionFailed),
        }
    }
}

/// Why a leaf stopped startup.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StartupFailure {
    /// The code.
    pub code: FailureCode,
    /// Text for a person.
    pub message: String,
}

/// One processed leaf of the startup order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupEvent {
    /// The leaf's atom.
    pub binding: LeafName,
    /// The issuer.
    pub provider: Provider,
    /// The outcome.
    pub status: StartupStatus,
    /// The projection of the grant, exactly when the leaf was acquired.
    pub grant: Option<GrantProjection>,
    /// Exactly when this leaf stopped startup.
    pub failure: Option<StartupFailure>,
}

impl StartupEvent {
    /// Whether the fields relate as the chapter says: a grant exactly when
    /// acquired; a failure only for a refused or failed leaf, with the code
    /// of its status, and always for a failed one.
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        let grant = self.grant.is_some() == (self.status == StartupStatus::Acquired);
        let failure = match (&self.failure, self.status) {
            (None, StartupStatus::Acquired | StartupStatus::Refused) => true,
            (Some(failure), status) => FailureCode::of(status) == Some(failure.code),
            (None, StartupStatus::Failed) => false,
        };
        grant && failure
    }

    /// The record's canonical bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> String {
        let mut out = format!(
            "{{kind: @startup binding: {} provider: {} status: {}",
            self.binding,
            self.provider.atom(),
            self.status.atom()
        );
        if let Some(grant) = &self.grant {
            out.push_str(" grant: ");
            out.push_str(&grant.canonical_bytes());
        }
        if let Some(failure) = &self.failure {
            let _ = write!(
                out,
                " failure: {{code: {} message: {}}}",
                failure.code.atom(),
                canonical_string(&failure.message)
            );
        }
        out.push('}');
        out
    }

    /// The JSON projection: each field its own member, `null` for an absent
    /// one.
    #[must_use]
    pub fn json(&self) -> Json {
        json!({
            "kind": "@startup",
            "binding": self.binding.as_str(),
            "provider": self.provider.atom(),
            "status": self.status.atom(),
            "grant": self.grant.as_ref().map_or(Json::Null, GrantProjection::json),
            "failure": self.failure.as_ref().map_or(Json::Null, |failure| {
                json!({"code": failure.code.atom(), "message": failure.message})
            }),
        })
    }
}

/// Whether a host call was a startup read or ran under the entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    /// `@startup`: a read a recipe performs.
    Startup,
    /// `@entry`: every other call.
    Entry,
}

impl Phase {
    /// The phase atom.
    #[must_use]
    pub const fn atom(self) -> &'static str {
        match self {
            Self::Startup => "@startup",
            Self::Entry => "@entry",
        }
    }

    fn from_atom(atom: &str) -> Option<Self> {
        [Self::Startup, Self::Entry]
            .into_iter()
            .find(|phase| phase.atom() == atom)
    }
}

/// One completed host call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEvent {
    /// The ordinal of the `await` that ran the host-bound member.
    pub invocation: u64,
    /// The ordinal of the invocation whose body performed that `await`;
    /// `None` for a startup read.
    pub parent: Option<u64>,
    /// The phase.
    pub phase: Phase,
    /// The operation, by its registered wire symbol.
    pub op: HostOp,
    /// The bound driver.
    pub driver: DriverIdentity,
    /// The source origin of the `await`; `None` when none is known.
    pub origin: Option<SourceOrigin>,
    /// The grant the call went through.
    pub grant: GrantProjection,
    /// The canonical text of each argument after the grant, in parameter
    /// order; a path as the program wrote it.
    pub args: Vec<String>,
    /// The canonical text of the operation's complete `Result` value.
    pub result: String,
}

impl HostEvent {
    /// The record's canonical bytes, its fields in the order of the
    /// chapter's table.
    #[must_use]
    pub fn canonical_bytes(&self) -> String {
        let mut out = format!("{{kind: @host invocation: {}u64", self.invocation);
        if let Some(parent) = self.parent {
            let _ = write!(out, " parent: {parent}u64");
        }
        let _ = write!(
            out,
            " phase: {} op: {} driver: {}",
            self.phase.atom(),
            canonical_string(self.op.symbol()),
            driver_bytes(&self.driver)
        );
        if let Some(origin) = &self.origin {
            out.push_str(" origin: ");
            out.push_str(&origin_bytes(origin));
        }
        out.push_str(" grant: ");
        out.push_str(&self.grant.canonical_bytes());
        out.push_str(" args: [");
        out.push_str(&self.args.join(" "));
        out.push_str("] result: ");
        out.push_str(&self.result);
        out.push('}');
        out
    }

    /// The JSON projection: each field its own member, an atom a string
    /// keeping its `@`, an argument and the result their canonical text
    /// inside a string. `span` writes a known origin as the `SpanDocument`
    /// of the diagnostics chapter, whose positions need the source text the
    /// trace does not hold; an unknown origin and an absent `parent` are
    /// `null`.
    #[must_use]
    pub fn json(&self, span: &dyn Fn(&SourceOrigin) -> Json) -> Json {
        json!({
            "kind": "@host",
            "invocation": self.invocation,
            "parent": self.parent,
            "phase": self.phase.atom(),
            "op": self.op.symbol(),
            "driver": {"provider": self.driver.provider, "version": self.driver.version},
            "origin": self.origin.as_ref().map_or(Json::Null, span),
            "grant": self.grant.json(),
            "args": self.args,
            "result": self.result,
        })
    }
}

fn driver_bytes(driver: &DriverIdentity) -> String {
    format!(
        "{{provider: {} version: {}}}",
        driver.provider,
        canonical_string(&driver.version)
    )
}

fn origin_bytes(origin: &SourceOrigin) -> String {
    format!(
        "{{sourceId: {} start: {}u64 end: {}u64}}",
        canonical_string(origin.source_id()),
        origin.span().start(),
        origin.span().end()
    )
}

/// An event of a kind this format does not define, kept as opaque data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpaqueEvent {
    kind: String,
    bytes: String,
}

impl OpaqueEvent {
    /// The event's `kind` atom.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// The canonical bytes of opaque data: fields in the byte order of their
    /// names, numbers as written, nested dict entries in the byte order of
    /// their keys.
    #[must_use]
    pub fn canonical_bytes(&self) -> &str {
        &self.bytes
    }
}

/// One event of a trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// `@startup`.
    Startup(StartupEvent),
    /// `@host`.
    Host(HostEvent),
    /// Any other kind.
    Opaque(OpaqueEvent),
}

impl Event {
    /// The event's canonical bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> String {
        match self {
            Self::Startup(event) => event.canonical_bytes(),
            Self::Host(event) => event.canonical_bytes(),
            Self::Opaque(event) => event.bytes.clone(),
        }
    }

    /// The JSON projection of a startup or host event; `None` for an event of
    /// an unknown kind, whose fields no JSON projection is defined for.
    #[must_use]
    pub fn json(&self, span: &dyn Fn(&SourceOrigin) -> Json) -> Option<Json> {
        match self {
            Self::Startup(event) => Some(event.json()),
            Self::Host(event) => Some(event.json(span)),
            Self::Opaque(_) => None,
        }
    }
}

/// An `@auditTrace.v2` document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditTrace {
    /// The workspace revision of the source that ran.
    pub revision: String,
    /// The binary target's atom; `None` for a test or an evaluation.
    pub target: Option<String>,
    /// Every driver bound in the run, sorted by `provider`, each once.
    pub drivers: Vec<DriverIdentity>,
    /// Every policy a projection refers to, by its hash, in its normal form.
    pub policies: BTreeMap<String, Policy>,
    /// `@none`, or the redaction profile applied.
    pub redaction: String,
    /// The events, in completion order.
    pub events: Vec<Event>,
}

impl AuditTrace {
    /// The empty trace of a run with `header`, unredacted.
    #[must_use]
    pub fn new(header: TraceHeader) -> Self {
        Self {
            revision: header.revision,
            target: header.target,
            drivers: Vec::new(),
            policies: BTreeMap::new(),
            redaction: NO_REDACTION.to_owned(),
            events: Vec::new(),
        }
    }

    /// Adds `driver` to `drivers` unless it is there, keeping them sorted by
    /// the bytes of the provider and then of the version.
    pub fn bind_driver(&mut self, driver: DriverIdentity) {
        if let Err(position) = self
            .drivers
            .binary_search_by(|known| driver_key(known).cmp(&driver_key(&driver)))
        {
            self.drivers.insert(position, driver);
        }
    }
}

/// The order of `drivers`: the provider's bytes, then the version's.
fn driver_key(driver: &DriverIdentity) -> (&[u8], &[u8]) {
    (driver.provider.as_bytes(), driver.version.as_bytes())
}

/// The canonical bytes of `trace`: one line, the fields in the chapter's
/// order, an absent `target` omitted.
#[must_use]
pub fn encode(trace: &AuditTrace) -> String {
    let mut out = format!(
        "{{format: {TRACE_FORMAT} revision: {}",
        canonical_string(&trace.revision)
    );
    if let Some(target) = &trace.target {
        out.push_str(" target: ");
        out.push_str(target);
    }
    out.push_str(" drivers: [");
    let drivers = trace.drivers.iter().map(driver_bytes).collect::<Vec<_>>();
    out.push_str(&drivers.join(" "));
    out.push_str("] policies: [");
    if trace.policies.is_empty() {
        out.push(':');
    }
    for (index, (hash, policy)) in trace.policies.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push_str(&canonical_string(hash));
        out.push_str(": ");
        out.push_str(&policy.canonical_bytes());
    }
    let _ = write!(out, "] redaction: {} events: [", trace.redaction);
    let events = trace
        .events
        .iter()
        .map(Event::canonical_bytes)
        .collect::<Vec<_>>();
    out.push_str(&events.join(" "));
    out.push_str("]}");
    out
}

// --- Values -------------------------------------------------------------

/// A type a registry row's argument or result names, as the trace encodes
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ty {
    Void,
    Str,
    Bytes,
    I64,
    U64,
    ListStr,
    RelativePath,
    FsKind,
    FsMetadata,
    Error(HostErrorType),
}

impl Ty {
    /// The type of a registry slot; `None` for a type no host row passes or
    /// returns.
    fn of(ty: HostType) -> Option<Self> {
        match ty {
            HostType::Void => Some(Self::Void),
            HostType::Str => Some(Self::Str),
            HostType::Bytes => Some(Self::Bytes),
            HostType::I64 => Some(Self::I64),
            HostType::U64 => Some(Self::U64),
            HostType::List(&HostType::Str) => Some(Self::ListStr),
            HostType::RelativePath => Some(Self::RelativePath),
            HostType::FsKind => Some(Self::FsKind),
            HostType::FsMetadata => Some(Self::FsMetadata),
            HostType::List(_)
            | HostType::Dict(..)
            | HostType::Interface(_)
            | HostType::TestWorld
            | HostType::Record(_) => None,
        }
    }

    /// The scalar atom of a compiler-owned type.
    const fn scalar(self) -> Option<&'static str> {
        match self {
            Self::Void => Some("@void"),
            Self::I64 => Some("@i64"),
            Self::U64 => Some("@u64"),
            _ => None,
        }
    }

    /// The module and name of a declared library type.
    const fn declared(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Str => Some(("@std.str", "Str")),
            Self::Bytes => Some(("@std.bytes", "Bytes")),
            Self::RelativePath => Some(("@std.fs", "RelativePath")),
            Self::FsKind => Some(("@std.fs", "FsKind")),
            Self::FsMetadata => Some(("@std.fs", "FsMetadata")),
            Self::Error(error) => Some((error.module(), error.name())),
            Self::Void | Self::I64 | Self::U64 | Self::ListStr => None,
        }
    }
}

/// The module and name of `Result`.
const RESULT: (&str, &str) = ("@std.result", "Result");

/// The declared type record of `module.name` with `seed` and the encoded
/// `arguments`.
fn declared_bytes(
    module: &str,
    name: &str,
    seed: &str,
    arguments: &[String],
) -> String {
    format!(
        "{{kind: @declared path: {module}.{name} seed: {} arguments: [{}]}}",
        canonical_string(seed),
        arguments.join(" ")
    )
}

/// The type encoding of `ty`, with the library's identities.
fn type_bytes(ty: Ty, library: &StdlibIdentity) -> String {
    if let Some(atom) = ty.scalar() {
        return atom.to_owned();
    }
    if let Some((module, name)) = ty.declared() {
        return declared_bytes(
            module,
            name,
            &library.seed(module, TYPE_KIND, name),
            &[],
        );
    }
    format!(
        "{{kind: @list arguments: [{}]}}",
        type_bytes(Ty::Str, library)
    )
}

fn string_list_bytes(names: &[String]) -> String {
    let names = names
        .iter()
        .map(|name| canonical_string(name))
        .collect::<Vec<_>>();
    format!("{{kind: @list values: [{}]}}", names.join(" "))
}

fn bytes_bytes(bytes: &[u8]) -> String {
    let values = bytes
        .iter()
        .map(|byte| format!("{byte}u8"))
        .collect::<Vec<_>>();
    format!("{{kind: @bytes values: [{}]}}", values.join(" "))
}

/// The canonical value encoding of one argument after the grant: a `Str` as
/// its literal, a `Bytes` as the bytes record, a path as the
/// `RelativePath` wrapper of the names the program wrote, a count as its
/// `U64` literal.
#[must_use]
pub fn argument_bytes(argument: &HostArgument<'_>, library: &StdlibIdentity) -> String {
    match argument {
        HostArgument::Text(text) => canonical_string(text),
        HostArgument::Bytes(bytes) => bytes_bytes(bytes),
        HostArgument::Path(names) => format!(
            "{{kind: @wrapper typeInfo: {} value: {}}}",
            type_bytes(Ty::RelativePath, library),
            string_list_bytes(names)
        ),
        HostArgument::Count(count) => format!("{count}u64"),
    }
}

/// The variant record of a fieldless case of the declared type `ty`.
fn case_bytes(ty: &str, case: &str) -> String {
    format!("{{kind: @enum typeInfo: {ty} variant: @{case}}}")
}

/// The canonical value encoding of `op`'s complete `Result` value `reply`.
///
/// # Errors
///
/// The reply is not of the row's result type: its `Ok` payload is of
/// another type, or its error of another error type.
pub fn result_bytes(
    op: HostOp,
    reply: &HostReply,
    library: &StdlibIdentity,
) -> Result<String, String> {
    let row = op.row();
    let ok = Ty::of(row.ok)
        .ok_or_else(|| "the row's result type has no encoding".to_owned())?;
    let error = Ty::Error(row.error);
    let (module, name) = RESULT;
    let result_type = declared_bytes(
        module,
        name,
        &library.seed(module, TYPE_KIND, name),
        &[type_bytes(ok, library), type_bytes(error, library)],
    );
    let wrong = || format!("the reply is not a value of `{}`'s result", op.symbol());
    let (variant, payload) = match reply {
        HostReply::Err(case) if case.error() == row.error => (
            "err",
            format!(
                "error: {}",
                case_bytes(&type_bytes(error, library), case.case())
            ),
        ),
        HostReply::Err(_) => return Err(wrong()),
        HostReply::Ok(value) => {
            let value = match (ok, value) {
                (Ty::Void, HostOk::Void) => "void".to_owned(),
                (Ty::Str, HostOk::Text(text)) => canonical_string(text),
                (Ty::Bytes, HostOk::Bytes(bytes)) => bytes_bytes(bytes),
                (Ty::ListStr, HostOk::Names(names)) => string_list_bytes(names),
                (Ty::I64, HostOk::I64(value)) => format!("{value}i64"),
                (Ty::U64, HostOk::U64(value)) => format!("{value}u64"),
                (Ty::FsMetadata, HostOk::Metadata { kind, size }) => {
                    let kind = match kind {
                        HostFsKind::File => "file",
                        HostFsKind::Directory => "directory",
                        HostFsKind::Other => "other",
                    };
                    format!(
                        "{{kind: @record typeInfo: {} fields: {{kind: {} size: {size}u64}}}}",
                        type_bytes(Ty::FsMetadata, library),
                        case_bytes(&type_bytes(Ty::FsKind, library), kind)
                    )
                }
                _ => return Err(wrong()),
            };
            ("ok", format!("value: {value}"))
        }
    };
    Ok(format!(
        "{{kind: @enum typeInfo: {result_type} variant: @{variant} payload: {{{payload}}}}}"
    ))
}

// --- Decoding -----------------------------------------------------------

/// One fault of a document: `@data.invalidShape` or `@data.invalidValue` at
/// a span.
#[derive(Debug)]
struct Fault {
    shape: bool,
    span: ByteSpan,
    message: String,
}

fn shape(span: ByteSpan, message: impl Into<String>) -> Fault {
    Fault {
        shape: true,
        span,
        message: message.into(),
    }
}

fn value(span: ByteSpan, message: impl Into<String>) -> Fault {
    Fault {
        shape: false,
        span,
        message: message.into(),
    }
}

/// The fields of a closed record, found by name.
struct Fields<'a> {
    node: &'a DataNode,
    fields: &'a [DataField],
}

impl<'a> Fields<'a> {
    fn get(&self, name: &str) -> Option<&'a DataNode> {
        self.fields
            .iter()
            .find(|field| field.name() == name)
            .map(DataField::value)
    }

    fn required(&self, name: &str) -> Result<&'a DataNode, Fault> {
        self.get(name).ok_or_else(|| {
            shape(
                self.node.span(),
                format!("the required field `{name}` is missing"),
            )
        })
    }
}

/// A closed record of the `required` and `optional` fields: no other field
/// is admitted, and every required one must be written.
fn record<'a>(
    node: &'a DataNode,
    required: &[&str],
    optional: &[&str],
) -> Result<Fields<'a>, Fault> {
    let DataValue::Record(fields) = node.value() else {
        return Err(shape(node.span(), "the value is a record"));
    };
    if let Some(extra) = fields.iter().find(|field| {
        !required.contains(&field.name()) && !optional.contains(&field.name())
    }) {
        return Err(shape(
            extra.name_span(),
            format!("the record has no field `{}`", extra.name()),
        ));
    }
    let found = Fields { node, fields };
    for name in required {
        found.required(name)?;
    }
    Ok(found)
}

fn atom(node: &DataNode) -> Result<&str, Fault> {
    match node.value() {
        DataValue::Atom(atom) => Ok(atom),
        _ => Err(shape(node.span(), "the value is an atom")),
    }
}

/// An atom that must be `expected`.
fn atom_is(node: &DataNode, expected: &str) -> Result<(), Fault> {
    if atom(node)? == expected {
        Ok(())
    } else {
        Err(value(node.span(), format!("the value is `{expected}`")))
    }
}

fn string(node: &DataNode) -> Result<&str, Fault> {
    match node.value() {
        DataValue::String(text) => Ok(text),
        _ => Err(shape(node.span(), "the value is a string")),
    }
}

fn list(node: &DataNode) -> Result<&[DataNode], Fault> {
    match node.value() {
        DataValue::List(elements) => Ok(elements),
        _ => Err(shape(node.span(), "the value is a list")),
    }
}

/// An integer of the type `suffix`, written with that suffix or none.
fn integer<T: std::str::FromStr>(
    node: &DataNode,
    suffix: IntegerSuffix,
) -> Result<T, Fault> {
    let DataValue::Integer(literal) = node.value() else {
        return Err(shape(node.span(), "the value is an integer"));
    };
    if literal.suffix().is_some_and(|written| written != suffix) {
        return Err(value(node.span(), "the integer has another width"));
    }
    literal
        .body()
        .parse()
        .map_err(|_| value(node.span(), "the integer is out of range"))
}

fn u64_of(node: &DataNode) -> Result<u64, Fault> {
    integer(node, IntegerSuffix::U64)
}

/// Whether `text` is a declaration identity: `sha256:` and 64 lower-case
/// hexadecimal digits.
fn is_identity(text: &str) -> bool {
    text.strip_prefix("sha256:").is_some_and(|digits| {
        digits.len() == 64
            && digits
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

/// An identity string.
fn identity(node: &DataNode) -> Result<&str, Fault> {
    let text = string(node)?;
    if is_identity(text) {
        Ok(text)
    } else {
        Err(value(node.span(), "the value is a declaration identity"))
    }
}

/// A reader of one type argument: its canonical text.
type ReadType<'a> = dyn Fn(&DataNode) -> Result<String, Fault> + 'a;

/// The declared type record of `module.name`, written with any identity and
/// exactly `arguments.len()` arguments, each read by its function.
fn read_declared(
    node: &DataNode,
    module: &str,
    name: &str,
    arguments: &[&ReadType<'_>],
) -> Result<String, Fault> {
    let fields = record(node, &["kind", "path", "seed", "arguments"], &[])?;
    atom_is(fields.required("kind")?, "@declared")?;
    atom_is(fields.required("path")?, &format!("{module}.{name}"))?;
    let seed = identity(fields.required("seed")?)?;
    let written = fields.required("arguments")?;
    let elements = list(written)?;
    if elements.len() != arguments.len() {
        return Err(value(
            written.span(),
            "the type has another number of arguments",
        ));
    }
    let arguments = elements
        .iter()
        .zip(arguments)
        .map(|(element, read)| read(element))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(declared_bytes(module, name, seed, &arguments))
}

/// The type encoding of `ty`, as written; its canonical text.
fn read_type(node: &DataNode, ty: Ty) -> Result<String, Fault> {
    if let Some(scalar) = ty.scalar() {
        atom_is(node, scalar)?;
        return Ok(scalar.to_owned());
    }
    if let Some((module, name)) = ty.declared() {
        return read_declared(node, module, name, &[]);
    }
    let fields = record(node, &["kind", "arguments"], &[])?;
    atom_is(fields.required("kind")?, "@list")?;
    let written = fields.required("arguments")?;
    let [element] = list(written)? else {
        return Err(value(written.span(), "a list type has one argument"));
    };
    Ok(format!(
        "{{kind: @list arguments: [{}]}}",
        read_type(element, Ty::Str)?
    ))
}

/// The variant record of a fieldless case of `ty`, one of `cases`.
fn read_case(node: &DataNode, ty: Ty, cases: &[&str]) -> Result<String, Fault> {
    let fields = record(node, &["kind", "typeInfo", "variant"], &[])?;
    atom_is(fields.required("kind")?, "@enum")?;
    let type_info = read_type(fields.required("typeInfo")?, ty)?;
    let variant = fields.required("variant")?;
    let case = atom(variant)?
        .strip_prefix('@')
        .filter(|case| cases.contains(case))
        .ok_or_else(|| value(variant.span(), "the case is not one of the type's"))?;
    Ok(case_bytes(&type_info, case))
}

fn read_string_list(node: &DataNode) -> Result<String, Fault> {
    let fields = record(node, &["kind", "values"], &[])?;
    atom_is(fields.required("kind")?, "@list")?;
    let names = list(fields.required("values")?)?
        .iter()
        .map(|element| string(element).map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(string_list_bytes(&names))
}

/// The value encoding of a value of `ty`, as written; its canonical text.
fn read_value(node: &DataNode, ty: Ty) -> Result<String, Fault> {
    match ty {
        Ty::Void => match node.value() {
            DataValue::Void => Ok("void".to_owned()),
            _ => Err(shape(node.span(), "the value is `void`")),
        },
        Ty::Str => string(node).map(canonical_string),
        Ty::I64 => {
            integer::<i64>(node, IntegerSuffix::I64).map(|value| format!("{value}i64"))
        }
        Ty::U64 => u64_of(node).map(|value| format!("{value}u64")),
        Ty::Bytes => {
            let fields = record(node, &["kind", "values"], &[])?;
            atom_is(fields.required("kind")?, "@bytes")?;
            let bytes = list(fields.required("values")?)?
                .iter()
                .map(|element| integer::<u8>(element, IntegerSuffix::U8))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(bytes_bytes(&bytes))
        }
        Ty::ListStr => read_string_list(node),
        Ty::RelativePath => {
            let fields = record(node, &["kind", "typeInfo", "value"], &[])?;
            atom_is(fields.required("kind")?, "@wrapper")?;
            let type_info = read_type(fields.required("typeInfo")?, ty)?;
            let names = read_string_list(fields.required("value")?)?;
            Ok(format!(
                "{{kind: @wrapper typeInfo: {type_info} value: {names}}}"
            ))
        }
        Ty::FsKind => read_case(node, ty, &FS_KIND_CASES),
        Ty::Error(error) => read_case(node, ty, &error.cases()),
        Ty::FsMetadata => {
            let fields = record(node, &["kind", "typeInfo", "fields"], &[])?;
            atom_is(fields.required("kind")?, "@record")?;
            let type_info = read_type(fields.required("typeInfo")?, ty)?;
            let inner = record(fields.required("fields")?, &["kind", "size"], &[])?;
            let kind = read_value(inner.required("kind")?, Ty::FsKind)?;
            let size = read_value(inner.required("size")?, Ty::U64)?;
            Ok(format!(
                "{{kind: @record typeInfo: {type_info} fields: {{kind: {kind} size: {size}}}}}"
            ))
        }
    }
}

/// The complete `Result` value of `op`, as written; its canonical text.
fn read_result(node: &DataNode, op: HostOp) -> Result<String, Fault> {
    let row = op.row();
    let ok =
        Ty::of(row.ok).ok_or_else(|| value(node.span(), "the row has no encoding"))?;
    let error = Ty::Error(row.error);
    let fields = record(node, &["kind", "typeInfo", "variant", "payload"], &[])?;
    atom_is(fields.required("kind")?, "@enum")?;
    let (module, name) = RESULT;
    let type_info = read_declared(
        fields.required("typeInfo")?,
        module,
        name,
        &[&|node| read_type(node, ok), &|node| read_type(node, error)],
    )?;
    let variant = fields.required("variant")?;
    let (case, field, ty) = match atom(variant)? {
        "@ok" => ("ok", "value", ok),
        "@err" => ("err", "error", error),
        _ => return Err(value(variant.span(), "the case is `@ok` or `@err`")),
    };
    let payload = record(fields.required("payload")?, &[field], &[])?;
    let inner = read_value(payload.required(field)?, ty)?;
    Ok(format!(
        "{{kind: @enum typeInfo: {type_info} variant: @{case} payload: {{{field}: {inner}}}}}"
    ))
}

fn read_driver(node: &DataNode) -> Result<DriverIdentity, Fault> {
    let fields = record(node, &["provider", "version"], &[])?;
    Ok(DriverIdentity {
        provider: atom(fields.required("provider")?)?.to_owned(),
        version: string(fields.required("version")?)?.to_owned(),
    })
}

fn read_origin(node: &DataNode) -> Result<SourceOrigin, Fault> {
    let fields = record(node, &["sourceId", "start", "end"], &[])?;
    let source = string(fields.required("sourceId")?)?;
    let offset = |name: &str| -> Result<usize, Fault> {
        let node = fields.required(name)?;
        usize::try_from(u64_of(node)?)
            .map_err(|_| value(node.span(), "the offset is too large"))
    };
    let (start, end) = (offset("start")?, offset("end")?);
    if start > end {
        return Err(value(node.span(), "the origin starts after it ends"));
    }
    Ok(SourceOrigin::new(source, ByteSpan::new(start, end)))
}

fn read_issued_by(node: &DataNode) -> Result<GrantOrigin, Fault> {
    let DataValue::Record(written) = node.value() else {
        return Err(shape(node.span(), "the value is a record"));
    };
    let kind = written
        .iter()
        .find(|field| field.name() == "kind")
        .map(DataField::value)
        .ok_or_else(|| shape(node.span(), "the required field `kind` is missing"))?;
    match atom(kind)? {
        "@binding" => {
            let fields = record(node, &["kind", "leaf"], &[])?;
            Ok(GrantOrigin::Binding {
                leaf: LeafName::new(atom(fields.required("leaf")?)?),
            })
        }
        "@testWorld" => {
            record(node, &["kind"], &[])?;
            Ok(GrantOrigin::TestWorld)
        }
        _ => Err(value(
            kind.span(),
            "the origin's kind is `@binding` or `@testWorld`",
        )),
    }
}

fn read_limits(node: &DataNode) -> Result<Limits, Fault> {
    let fields = record(node, &[], &["invocations", "inputBytes", "outputBytes"])?;
    let bound = |name: &str| fields.get(name).map(u64_of).transpose();
    let limits = Limits {
        invocations: bound("invocations")?,
        input_bytes: bound("inputBytes")?,
        output_bytes: bound("outputBytes")?,
    };
    if limits.is_unbounded() {
        return Err(value(
            node.span(),
            "a limits record sets at least one bound",
        ));
    }
    Ok(limits)
}

/// A grant projection, and where its `policy` was written.
fn read_projection(node: &DataNode) -> Result<(GrantProjection, ByteSpan), Fault> {
    let fields = record(node, &["issuedBy", "policy", "rights", "root"], &["limits"])?;
    let policy = fields.required("policy")?;
    let policy_text = string(policy)?;
    if !policy_text.is_empty() && !is_identity(policy_text) {
        return Err(value(policy.span(), "the policy is a hash or empty"));
    }
    let rights = fields.required("rights")?;
    let [right] = list(rights)? else {
        return Err(value(rights.span(), "a projection permits one interface"));
    };
    let reference = record(right, &["path", "seed"], &[])?;
    let path = reference.required("path")?;
    let path_atom = atom(path)?;
    if !HostInterface::ALL
        .iter()
        .any(|interface| interface.owner().path() == path_atom)
    {
        return Err(value(
            path.span(),
            "the right is not an operation interface",
        ));
    }
    let seed = identity(reference.required("seed")?)?;
    let rights = EntityRef::new(path_atom.trim_start_matches('@'), seed)
        .map_err(|fault| value(right.span(), fault.to_string()))?;
    let root = fields.required("root")?;
    let ordinal = u64_of(root)?;
    if ordinal == 0 {
        return Err(value(root.span(), "roots are numbered from 1"));
    }
    let projection = GrantProjection {
        issued_by: read_issued_by(fields.required("issuedBy")?)?,
        policy: policy_text.to_owned(),
        rights,
        limits: fields.get("limits").map(read_limits).transpose()?,
        root: ordinal,
    };
    Ok((projection, policy.span()))
}

/// A decoded event, and where each projection's `policy` was written.
fn read_event(
    node: &DataNode,
    opaque: &str,
    policies: &mut Vec<(String, ByteSpan)>,
) -> Result<Event, Fault> {
    let DataValue::Record(written) = node.value() else {
        return Err(shape(node.span(), "an event is a record"));
    };
    let kind = written
        .iter()
        .find(|field| field.name() == "kind")
        .map(DataField::value)
        .ok_or_else(|| shape(node.span(), "the required field `kind` is missing"))?;
    let mut projection = |node: &DataNode| {
        let (projection, span) = read_projection(node)?;
        if !projection.policy.is_empty() {
            policies.push((projection.policy.clone(), span));
        }
        Ok::<_, Fault>(projection)
    };
    match atom(kind)? {
        "@startup" => {
            let fields = record(
                node,
                &["kind", "binding", "provider", "status"],
                &["grant", "failure"],
            )?;
            let provider = fields.required("provider")?;
            let status = fields.required("status")?;
            let failure = fields
                .get("failure")
                .map(|node| {
                    let fields = record(node, &["code", "message"], &[])?;
                    let code = fields.required("code")?;
                    Ok::<_, Fault>(StartupFailure {
                        code: FailureCode::from_atom(atom(code)?).ok_or_else(|| {
                            value(code.span(), "the code is not a startup failure's")
                        })?,
                        message: string(fields.required("message")?)?.to_owned(),
                    })
                })
                .transpose()?;
            let event = StartupEvent {
                binding: LeafName::new(atom(fields.required("binding")?)?),
                provider: Provider::from_atom(atom(provider)?).ok_or_else(|| {
                    value(provider.span(), "the provider is not registered")
                })?,
                status: StartupStatus::from_atom(atom(status)?).ok_or_else(|| {
                    value(status.span(), "the status is not a startup outcome")
                })?,
                grant: fields.get("grant").map(&mut projection).transpose()?,
                failure,
            };
            if !event.is_consistent() {
                return Err(value(
                    node.span(),
                    "the grant and the failure do not match the status",
                ));
            }
            Ok(Event::Startup(event))
        }
        "@host" => {
            let fields = record(
                node,
                &[
                    "kind",
                    "invocation",
                    "phase",
                    "op",
                    "driver",
                    "grant",
                    "args",
                    "result",
                ],
                &["parent", "origin"],
            )?;
            let op_node = fields.required("op")?;
            let op = host_row(string(op_node)?)
                .map(|row| row.op)
                .ok_or_else(|| {
                    value(op_node.span(), "the op is not a registered symbol")
                })?;
            let phase_node = fields.required("phase")?;
            let phase = Phase::from_atom(atom(phase_node)?).ok_or_else(|| {
                value(phase_node.span(), "the phase is `@startup` or `@entry`")
            })?;
            let parent = fields.get("parent").map(u64_of).transpose()?;
            if (phase == Phase::Startup) != parent.is_none() {
                return Err(value(
                    phase_node.span(),
                    "a startup read has no parent, and only a startup read",
                ));
            }
            let grant_node = fields.required("grant")?;
            let grant = projection(grant_node)?;
            if grant.rights.path()
                != op.interface().owner().path().trim_start_matches('@')
            {
                return Err(value(
                    grant_node.span(),
                    "the grant does not permit the event's operation",
                ));
            }
            let args_node = fields.required("args")?;
            let written = list(args_node)?;
            let parameters = op.row().parameters;
            if written.len() != parameters.len() {
                return Err(value(args_node.span(), "the arguments are not the row's"));
            }
            let args = written
                .iter()
                .zip(parameters)
                .map(|(argument, parameter)| {
                    let ty = Ty::of(parameter.ty).ok_or_else(|| {
                        value(argument.span(), "the parameter has no encoding")
                    })?;
                    read_value(argument, ty)
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Event::Host(HostEvent {
                invocation: u64_of(fields.required("invocation")?)?,
                parent,
                phase,
                op,
                driver: read_driver(fields.required("driver")?)?,
                origin: fields.get("origin").map(read_origin).transpose()?,
                grant,
                args,
                result: read_result(fields.required("result")?, op)?,
            }))
        }
        other => Ok(Event::Opaque(OpaqueEvent {
            kind: other.to_owned(),
            bytes: opaque.to_owned(),
        })),
    }
}

/// The schema of the document's envelope: the events and the policies are
/// opaque to it, and read by this module.
fn envelope() -> Schema {
    Schema::new(
        TRACE_FORMAT,
        vec![
            FieldSchema::required("revision", Slot::Str(None)),
            FieldSchema::optional("target", Slot::Atom(None)),
            FieldSchema::required(
                "drivers",
                Slot::list(Slot::Record(RecordSchema::new(vec![
                    FieldSchema::required("provider", Slot::Atom(None)),
                    FieldSchema::required("version", Slot::Str(None)),
                ]))),
            ),
            FieldSchema::required(
                "policies",
                Slot::dict(Slot::Str(None), Slot::Opaque),
            ),
            FieldSchema::required("redaction", Slot::Atom(None)),
            FieldSchema::required("events", Slot::list(Slot::Opaque)),
        ],
    )
}

fn raw_field<'a>(node: &'a DataNode, name: &str) -> Option<&'a DataNode> {
    match node.value() {
        DataValue::Record(fields) => fields
            .iter()
            .find(|field| field.name() == name)
            .map(DataField::value),
        _ => None,
    }
}

fn diagnostics(faults: Vec<Fault>, source_id: Option<&str>) -> Vec<Diagnostic> {
    let shape_step = faults.iter().any(|fault| fault.shape);
    let mut found = faults
        .into_iter()
        .filter(|fault| fault.shape == shape_step)
        .map(|fault| {
            let code = if fault.shape {
                DiagnosticCode::DataInvalidShape
            } else {
                DiagnosticCode::DataInvalidValue
            };
            let diagnostic = Diagnostic::new(code, fault.span, fault.message);
            match source_id {
                Some(id) => diagnostic.with_source_id(id),
                None => diagnostic,
            }
        })
        .collect::<Vec<_>>();
    found.sort_by_key(|diagnostic| {
        (
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end(),
        )
    });
    found
}

/// Reads `bytes` as an `@auditTrace.v2` document.
///
/// The envelope is read in the six-step order of "Schemas". Then each event
/// is read as the closed record of its kind, its values against the row its
/// `op` names; an event of another kind is kept as opaque data. Each policy
/// is read as a `@scopePolicy.v1` document whose key is its hash; each
/// `drivers` entry is in order; each projection's `policy` is empty or a key
/// of `policies`. The result is data: nothing decoded makes or restores a
/// grant.
///
/// # Errors
///
/// The faults of the first step that found any: the reader's codes for the
/// envelope; `@data.invalidShape` for an event's or a policy's missing or
/// unknown field or a value of the wrong form; `@data.invalidValue` for a
/// value the slot does not admit or fields that relate wrongly.
pub fn decode(
    source_id: Option<&str>,
    bytes: &[u8],
) -> Result<AuditTrace, Vec<Diagnostic>> {
    let (document, reader) = read(&envelope(), source_id, bytes).into_parts();
    let Some(document) = document.filter(|_| reader.is_empty()) else {
        return Err(reader);
    };
    let raw = parse_data(source_id, bytes);
    let Some(root) = raw.value() else {
        return Err(raw.diagnostics().to_vec());
    };
    let typed = document.root();
    let mut faults = Vec::new();
    let text = |name: &str| {
        typed
            .get(name)
            .and_then(|node| node.as_str())
            .map(str::to_owned)
    };
    let atom_text = |name: &str| {
        typed
            .get(name)
            .and_then(|node| node.as_atom())
            .map(str::to_owned)
    };

    let written = |name: &str| match raw_field(root, name).map(DataNode::value) {
        Some(DataValue::List(elements)) => elements.as_slice(),
        _ => &[],
    };

    let mut drivers: Vec<DriverIdentity> = Vec::new();
    for node in written("drivers") {
        match read_driver(node) {
            Ok(driver) => {
                if drivers
                    .last()
                    .is_some_and(|previous| driver_key(previous) >= driver_key(&driver))
                {
                    faults.push(value(
                        node.span(),
                        "the drivers are sorted by provider, each once",
                    ));
                }
                drivers.push(driver);
            }
            Err(fault) => faults.push(fault),
        }
    }

    let mut policies = BTreeMap::new();
    if let Some(DataValue::Dict(entries)) =
        raw_field(root, "policies").map(DataNode::value)
    {
        for entry in entries {
            let hash = match string(entry.key()) {
                Ok(hash) => hash,
                Err(fault) => {
                    faults.push(fault);
                    continue;
                }
            };
            match policy::decode_value(entry.value()) {
                Ok(decoded) if policy::hash(&decoded) == hash => {
                    policies.insert(hash.to_owned(), decoded);
                }
                Ok(_) => faults.push(value(
                    entry.key().span(),
                    "the key is not the hash of the policy's normal form",
                )),
                Err(PolicyError::Malformed { span, reason }) => {
                    faults.push(shape(span, reason))
                }
                Err(error) => {
                    faults.push(value(entry.value().span(), error.to_string()))
                }
            }
        }
    }

    let mut events = Vec::new();
    let mut referenced = Vec::new();
    let opaque = typed
        .get("events")
        .and_then(TypedNode::as_list)
        .unwrap_or_default();
    for (node, typed_event) in written("events").iter().zip(opaque) {
        let bytes = match typed_event.value() {
            TypedValue::Opaque(bytes) => bytes.as_str(),
            _ => "",
        };
        match read_event(node, bytes, &mut referenced) {
            Ok(event) => events.push(event),
            Err(fault) => faults.push(fault),
        }
    }
    for (hash, span) in referenced {
        if !policies.contains_key(&hash) {
            faults.push(value(span, "the policy is not a key of `policies`"));
        }
    }
    if !faults.is_empty() {
        return Err(diagnostics(faults, source_id));
    }
    Ok(AuditTrace {
        revision: text("revision").unwrap_or_default(),
        target: atom_text("target"),
        drivers,
        policies,
        redaction: atom_text("redaction").unwrap_or_default(),
        events,
    })
}
