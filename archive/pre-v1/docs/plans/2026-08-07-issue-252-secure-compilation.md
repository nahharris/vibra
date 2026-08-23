---
title: Implementation plan — secure compilation constraints (#252)
category: plans
status: completed
updated: 2026-08-07
issue: 252
---

# Secure compilation constraints implementation plan

> **For agentic workers:** Execute this plan task-by-task with TDD and verify
> both repository test suites before committing.

**Goal:** Enforce the accepted scalar-only `vibra_v1` boundary, make the
hardening-last ordering executable and tested, and align the ABI references
with the narrower security claims.

**Architecture:** The guest-facing `vibra_v1` Wasm imports remain numeric and
use host-owned arena indices; a shared ABI predicate rejects reference-like
source types before generic host values can cross the boundary. Static Wasm
FFI remains a separately documented host→foreign-module boundary with its
call-scoped memory mitigations. The backend exposes its ordered compilation
stages and validates that hardening is a terminal suffix.

**Tech Stack:** Rust, Wasmer, existing integration tests, Markdown decision
contracts, and the repository’s Vibra test runner.

---

### Task 1: Add focused red tests

**Files:**
- Modify: `tests/integration.rs`
- Modify: `src/wasm_backend.rs`
- Modify: `tests/documentation_contract.rs`

- [x] Add an ABI regression test with a `vibra_v1.array_append` wrapper whose
  argument is `(ref int64)`; it must fail lowering with `E-WASM-003`.
- [x] Change the import validation test so `wasi_snapshot_preview1.fd_write`
  is rejected, pinning the stale whitelist removal.
- [x] Add a documentation contract test requiring the secure-compilation
  decision to state both semantic-preservation and attack-prevention limits.
- [x] Add the pass-ordering invariant test against the backend’s ordered pass
  list.
- [x] Run each focused test and record the expected failures before writing
  production enforcement.

### Task 2: Publish the accepted contract and correct references

**Files:**
- Create: `docs/decisions/secure-compilation.md`
- Modify: `docs/index.md`
- Modify: `docs/reference/wasm-abi.md`
- Modify: `docs/reference/static-wasm-ffi.md`
- Delete: `src/wasm_abi.rs`
- Modify: `src/lib.rs`
- Modify: `tests/integration.rs`

- [x] State the scalar-only `vibra_v1` rule, opaque host-handle index rule,
  no-reference/no-pointer rule, hardening-last rule, and the distinction
  between semantic preservation and attack prevention.
- [x] Record the clean `vibra_v1` registry audit and explicitly scope the
  static FFI exception to host-owned memory, with fresh store/memory/instance,
  import allowlisting, and call-scoped non-retained pointers as mitigations.
- [x] Replace the normative pointer-layout paragraph in the v1 ABI reference
  with the live arena-index contract and remove the dead pointer-layout module
  and its tests.

### Task 3: Enforce the ABI boundary and audit the registry

**Files:**
- Modify: `src/lower.rs`
- Modify: `src/host_abi.rs`
- Modify: `src/wasm_backend.rs`
- Modify: `src/wasm_backend.rs` tests

- [x] Add a recursive reference-like-type guard used by `ValueKind::Any`,
  `OptionAny`, and `ResultAny` matching so `Mutable`, `Reference`, and
  `FnType` cannot cross the `vibra_v1` value boundary, including through
  aliases and aggregate payloads.
- [x] Add a registry audit asserting all `vibra_v1` entries remain in the
  numeric/arena-index model and contain no pointer/reference ABI kinds.
- [x] Remove the obsolete `WASI_IMPORTS` allowance and require unknown and
  known WASI imports alike to fail validation.

### Task 4: Make hardening order an executable invariant

**Files:**
- Create: `src/compilation_pipeline.rs`
- Modify: `src/lib.rs`
- Modify: `src/wasm_backend.rs`

- [x] Define the backend pass stages and terminal hardening stage in one
  ordered list.
- [x] Validate the list at the actual compilation entry point and expose a
  focused test that fails if any non-hardening stage follows hardening.
- [x] Document that no mechanized proof or attack-prevention claim is implied
  by this ordering check.

### Task 5: Verify and commit

**Files:** all implementation files above

- [x] Run `cargo fmt --all -- --check`.
- [x] Run `cargo test`.
- [x] Run `cargo run -- test`.
- [x] Re-audit `vibra_v1`, inspect the final diff/status, and commit the
  implementation without pushing, opening a PR, or merging.

### Verification record

- `cargo fmt --all -- --check` passed.
- Focused ABI, registry, import-validation, pass-order, and documentation
  tests passed.
- `cargo test` passed: 287 unit/library tests plus all integration targets.
- `cargo run -- test` passed: 97 of 97 Vibra-language tests.
- `schemas/host-abi.json` required no edit: the registry wire shapes are
  unchanged, and the existing schema synchronization test passed.
