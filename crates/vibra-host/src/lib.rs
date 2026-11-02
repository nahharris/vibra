//! The host provider of the Vibra runtime (Milestone 4, Step 16a).
//!
//! `docs/spec/06-runtime.md`, "External providers", "The host registry", "The
//! host world", and "The audit trace". A run is given one **host provider**, and
//! a host operation is an operation of that provider:
//!
//! - [`HostProvider`] is the one trait, with a method for each row of the
//!   registry in `vibra_ir::external::host`. The language has no mode switch:
//!   `run` uses the [`SystemProvider`] and `test` uses a [`WorldProvider`].
//! - [`HostWorld`] is the decoded `@host-world.v1` document and
//!   [`WorldProvider::new`] makes a fresh copy of it for each test and each
//!   backend, so a world is a value that nothing shares.
//! - [`HostValues`] is the value-access trait that each backend implements: the
//!   interpreter over its values, and the Wasm runner through the exported
//!   accessors and builders, never through the instance's linear memory. With
//!   [`HostCall`] it is the one place a host operation reads its arguments and
//!   builds its result, so the operations are written once.
//! - [`AuditEvent`] is the event of the audit trace, built by one function from
//!   the canonical encodings, and its VIBON and JSON codecs.
//!
//! # Position in the architecture
//!
//! Only the reference interpreter, the Wasm runner, and the hosts that hand them
//! a provider depend on this crate. The emitter never does: it needs only the
//! registry's symbols and signatures, which sit in `vibra-ir`. This crate and the
//! tooling crates that read a project, a command line, or a corpus are the only
//! places that read ambient filesystem, environment, clock, or random state, as
//! a host test checks.
//!
//! This step builds the machinery and claims no language behavior: no source
//! program can call a host operation until the `@host` external is supported.

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
pub use world::{HostWorld, WorldProvider};
