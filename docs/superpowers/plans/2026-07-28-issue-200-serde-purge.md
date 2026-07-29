# Issue 200 serde purge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the remaining YAML/`serde_yaml` compiler bridge while preserving the S-expression command surface.

**Architecture:** Production commands must load `frontend::SurfaceProgram` and lower it through `typed_program`, rather than adapting it through `serde_yaml::Value`. Typed tooling replaces legacy YAML diagnostics and annotations. Once every caller moves, delete `load`, `surface_adapter`, legacy lowering, dead macro expansion, and the dependency; documentation states that YAML embeds are rejected rather than supported.

**Tech Stack:** Rust, typed S-expression frontend, `typed_program`, Rust integration tests, Cargo.

---

### Task 1: Establish zero-YAML acceptance tests

**Files:**
- Modify: `tests/frontend_cutover.rs`
- Test: `tests/frontend_cutover.rs`

- [ ] Add assertions that `Cargo.toml` and all `src/*.rs` files contain no `serde_yaml`, no `load_legacy_yaml`, and no `surface_adapter` production path.
- [ ] Run `cargo test --test frontend_cutover`; confirm red because the current adapter remains.
- [ ] Keep the existing `.vibra.yaml` rejection and deleted-editor assertions.

### Task 2: Route real module commands through typed lowering

**Files:**
- Modify: `src/main.rs`, `src/project.rs`, `src/package.rs`, `src/test_runner.rs`
- Modify: `src/typed_program.rs`
- Test: `tests/sexpr_public_commands_cli.rs`, `tests/project_cli.rs`, `tests/integration.rs`

- [ ] Add typed entry helpers returning the existing `LoweredProgram`/test discovery results directly from `SurfaceProgram`.
- [ ] Replace each `load_legacy_yaml_program` caller with the typed helper; preserve flags, conditional parts, imports, macros, embeds, and diagnostic contexts.
- [ ] Add focused S-expression command tests for run, check, test, build, package, docs, expand, and effects.
- [ ] Run focused command tests, then `cargo test`.

### Task 3: Replace typed inline execution and lint diagnostics

**Files:**
- Modify: `src/main.rs`, `src/tooling.rs`, `src/typed_program.rs`
- Delete: `src/annotations.rs`, `src/macro_expand.rs`
- Test: `tests/integration.rs`, `tests/sexpr_public_commands_cli.rs`

- [ ] Implement typed `exec` expression/import lowering without synthetic YAML mappings.
- [ ] Route lint compile diagnostics through typed frontend/lowering; delete YAML `=lint` suppression scanning and obsolete YAML diagnostic labels.
- [ ] Add red/green tests for typed inline calls/imports and lint diagnostics from S-expression buffers/files.
- [ ] Run tooling and CLI test targets.

### Task 4: Delete adapter lowering and dependency

**Files:**
- Delete: `src/load.rs`, `src/surface_adapter.rs`, legacy `serde_yaml` lowerer paths
- Modify: `src/lib.rs`, `Cargo.toml`, `Cargo.lock`, remaining callers/tests
- Test: `tests/frontend_cutover.rs`

- [ ] Remove the adapter only after Tasks 2–3 have no production caller.
- [ ] Delete legacy YAML-only tests and migrate any remaining assertions to typed AST or runtime behavior.
- [ ] Remove `serde_yaml`; run `cargo tree -i serde_yaml` and require no dependency path.
- [ ] Run the zero-YAML acceptance test green.

### Task 5: Reconcile public contract and finish

**Files:**
- Modify: `README.md`, `docs/s-expression-migration-status.md`, `docs/superpowers/specs/2026-07-25-s-expression-language-design.md`
- Test: repository source audit

- [ ] State that YAML embedding is rejected; remove contradictory current claims while preserving explicitly historical records.
- [ ] Update migration status only after all removal checks pass.
- [ ] Run `cargo fmt -- --check`, `cargo test`, `cargo run -- test`, focused CLI/MCP/LSP tests, `rg serde_yaml src Cargo.toml`, and `git diff --check`.
- [ ] Merge each passing PR, verify remote `main`, then close Issue #200 with exact evidence.
