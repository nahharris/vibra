//! The host provider of the Vibra runtime over the major-1 host rows.
//!
//! `docs/spec/06-runtime.md`, "Host world" and "Audit trace". A run is given
//! one **host provider**, and a host operation is an operation of that
//! provider:
//!
//! - [`HostProvider`] is the one trait, with a method for each row of the
//!   major-1 registry in `vibra_ir::external::host`: the [`SystemProvider`]
//!   reads the process and the [`WorldProvider`] a world.
//! - [`HostWorld`] is the decoded `@hostWorld.v1` document and
//!   [`WorldProvider::new`] makes a fresh copy of it for each test and each
//!   backend, so a world is a value that nothing shares.
//! - [`HostValues`] is the value-access trait that a backend implements over
//!   its own values. With [`HostCall`] it is the one place a host operation
//!   reads its arguments and builds its result, so the operations are written
//!   once.
//! - [`AuditEvent`] is the event of the `@auditTrace.v1` trace, built by one
//!   function from the canonical data of the call, and its data and JSON
//!   codecs.
//!
//! # Position in the architecture
//!
//! No crate of the language pipeline depends on this one: it reads the
//! registry of `vibra-ir`, the data grammar of `vibra-syntax`, and the
//! diagnostics of `vibra-diagnostics`, and nothing above them. This crate and
//! the tooling crates that read a project, a command line, or a corpus are
//! the only places that read ambient filesystem, environment, clock, or
//! random state.

mod event;
mod operation;
mod provider;
mod random;
mod system;
mod world;

pub use event::{
    AuditEvent, HostEvent, OpaqueEvent, TRACE_FORMAT, decode_trace, trace_to_vibon,
};
pub use operation::{
    HostCall, HostEnum, HostOk, HostRecord, HostReply, HostValues, MemoryExhausted,
    OperationError, ReadError, build_reply, perform,
};
pub use provider::{
    Done, EnvError, Failure, FsError, FsKind, HostProvider, IoError, Metadata,
    RandomError, TimeError,
};
pub use random::SplitMix64;
pub use system::SystemProvider;
pub use world::{HostWorld, WORLD_FORMAT, WorldProvider};
