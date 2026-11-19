//! The `vibra-conformance-v1` integration tests, one test target.
//!
//! Every file in `tests/` would be its own binary, and each links the
//! native track, so the tests live here as modules of one binary that
//! share `support` instead of each carrying a copy of the helpers they need.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

mod support;

mod activations;
mod bindings;
mod call_flow;
mod contract_members;
mod corpus;
mod diagnostic_registry;
#[cfg(windows)]
mod dump_text;
mod effects;
mod entry_outcome;
mod evidence;
mod functions;
mod host_documents_fmt;
mod host_single_source;
mod host_world_corpus;
mod initializer_calls;
#[cfg(windows)]
mod linked_exe;
#[cfg(windows)]
mod linked_exe_programs;
#[cfg(windows)]
mod linked_exe_support;
mod lowering_slice;
mod m1_evidence;
mod native_bodies;
mod native_parity_inventory;
mod natives;
mod primitives;
mod project;
mod reader;
mod resolved_oracle;
mod return_tail_call;
mod semantic_queries;
mod tail_calls;
mod try_tail_loop;
mod typed_ir_identity;
mod typed_literals;
mod unchecked_integers;
mod workspace_semantic;
mod workspace_test;
