//! The `vibra-conformance` integration tests, one test target.
//!
//! Every file in `tests/` would be its own binary, and each links the
//! WebAssembly engine, so the tests live here as modules of one binary that
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
mod ambient_reads;
mod architecture_boundary;
mod arena;
mod arena_emitted;
mod arena_runtime;
mod bindings;
mod call_flow;
mod calls;
mod collections;
mod contract_members;
mod core_lowering;
mod corpus;
mod diagnostic_registry;
mod differential;
mod effects;
mod entry_outcome;
mod evidence;
mod examples_backends;
mod functions;
mod host_boundary;
mod host_builders;
mod host_documents_fmt;
mod host_single_source;
mod host_world_corpus;
mod initializer_calls;
mod interfaces_matrix;
#[cfg(windows)]
mod linked_exe;
#[cfg(windows)]
mod linked_exe_programs;
#[cfg(windows)]
mod linked_exe_support;
mod lowering_slice;
mod lowers_every_form;
mod native_bodies;
mod native_differential;
mod native_parity_inventory;
mod natives;
mod numbers;
mod patterns;
mod performance_baseline;
mod primitives;
mod project;
mod reader;
mod resolved_oracle;
mod return_tail_call;
mod semantic_queries;
mod tail_calls;
mod tests_traps;
mod trap_origins;
mod try_tail_loop;
mod typed_ir_identity;
mod typed_literals;
mod wasm_skeleton;
mod workspace_semantic;
mod workspace_test;
