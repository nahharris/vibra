# Capability grants Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make nominal declared effects enforceable runtime capability grants seeded from `project.vib`, with operation-time checks, monotone scope attenuation, and no authority amplification.

**Architecture:** Keep `CapabilityGrant` as the runtime authority value with its two axes: canonical effect-root domain and hierarchical `resource_prefix`. Extend the scheduler with a reusable operation-boundary check and effect-domain scope attenuation. Carry manifest grants into `RunConfig`, seed the root scope before execution, open/close child scopes at function and spawn boundaries, and re-check concrete host requirements immediately before each host operation in both interpreter and Wasm host execution.

**Tech Stack:** Rust 2021, `anyhow`, serde, the existing `Scheduler`/`RunConfig` runtime, the lowered `EffectRow`/`FunctionSig` IR, the S-expression `project.vib` parser, JSON schemas, Rust integration tests, and `tests/*.vib`.

---

### Task 1: Prove the runtime operation-boundary contract

**Files:**
- Modify: `src/async_runtime.rs` in the existing capability test module
- Test: `src/async_runtime.rs` capability unit tests

- [ ] **Step 1: Write the failing test**

Add a test named `operation_boundary_rechecks_grants` using the existing `grant` helper. It must construct a root scheduler with `fs.read:/safe`, assert that `fs.read:/safe/data.txt` is accepted, and assert that `fs.read:/outside` is rejected with the existing capability-denial error. Update the existing attenuation fixture domains to `fs.read` and `net.connect` so the test vocabulary matches the accepted effect-root contract.

- [ ] **Step 2: Run the focused test to verify it fails**

Run `cargo test async_runtime::tests::operation_boundary_rechecks_grants -- --exact` from the worktree. Expected: compilation failure because the operation-boundary grant check is not yet exposed by `Scheduler`.

- [ ] **Step 3: Implement the minimal scheduler check**

Expose a scheduler method that checks a requested `CapabilityGrant` against the current scope's held grants using the existing containment relation, returning the existing runtime denial variant for a miss. Keep the check independent of `ScopeLimits` and do not add #251 resource-budget behavior.

- [ ] **Step 4: Run the focused test to verify it passes**

Run `cargo test async_runtime::tests::operation_boundary_rechecks_grants -- --exact`. Expected: one passing test.

### Task 2: Add manifest authority and runtime configuration

**Files:**
- Modify: `src/project.rs` manifest types/parser/tests
- Modify: `src/runtime/wasi_env.rs` `RunConfig`
- Modify: `schemas/project-manifest.schema.json`
- Modify: `docs/reference/project-layout.md`
- Test: `tests/project_cli.rs` or `tests/integration.rs` manifest parsing/runtime setup tests

- [ ] **Step 1: Write the failing manifest test**

Add a parser test for a project containing `(authority (grant fs.read "/safe") (grant net.connect "example.com:443"))`. Assert that both grants are present with their exact domains and prefixes, and add a companion case where the authority section is omitted and the parsed authority is empty/explicitly absent.

- [ ] **Step 2: Run the focused test to verify it fails**

Run the exact new project-parser test with `cargo test project::tests::<test-name> -- --exact`. Expected: failure because `authority` is currently an unknown project form.

- [ ] **Step 3: Implement the manifest contract**

Add a typed manifest authority/grant representation, parse the canonical `(authority ...)` form with duplicate/shape validation, expose it through `ProjectManifest`, and make `RunConfig` carry the root grant set. Preserve the distinction between an explicit empty authority and legacy direct-module execution so callers can choose fail-closed project execution without forcing unrelated pure unit tests to invent a manifest. Update the project-manifest schema description/definitions and the project-layout reference with the exact syntax.

- [ ] **Step 4: Run focused parser/schema tests**

Run the new parser test and the existing `project::tests` target. Expected: all pass, including omission and malformed-authority cases.

### Task 3: Seed and lifecycle the root/child scopes

**Files:**
- Modify: `src/execute.rs` source execution state and call/spawn paths
- Modify: `src/wasm_backend.rs` `HostExecution` and host-call path
- Modify: `src/runtime/wasi_env.rs` configuration cloning/serialization helpers as needed
- Test: `tests/integration.rs` focused Rust runtime tests

- [ ] **Step 1: Write the failing lifecycle tests**

Add focused tests for a satisfied grant, denied grant, attenuation, amplification rejection at a spawn boundary, and a child scope that cannot retain a grant omitted from its parent. Add a runtime test that executes a host operation through a manifest-seeded root and proves the denied path fails before the host function mutates state.

- [ ] **Step 2: Run the focused tests to verify they fail**

Run the exact new integration tests with `cargo test --test integration <test-name>`. Expected: the current empty `Scheduler::new([])`/unwired scope lifecycle fails the grant-satisfied and spawn-boundary cases, while denied cases may currently execute ambiently.

- [ ] **Step 3: Implement reusable scope state**

Create one runtime authority state shared by interpreter and Wasm host execution. Seed `Scheduler::new` from the configured manifest grants, track the active scope, open a child scope by filtering parent grants to the callee's declared effect-root set, reject amplification, and close the child scope after the call. Make source `spawn` open a child scope through the same helper and retain its join result without adding budgets or changing #251’s limits.

- [ ] **Step 4: Run focused lifecycle tests**

Run the new integration tests plus the existing `async_runtime` target. Expected: satisfied grants pass, denied operations fail, attenuation remains possible, amplification is rejected before child admission, and balanced spawn/join leaves no live child scope.

### Task 4: Re-check every host operation at the boundary

**Files:**
- Modify: `src/execute.rs` host dispatch and resource-prefix extraction
- Modify: `src/wasm_backend.rs` host execution state wiring
- Modify: `src/host_abi.rs` effect-root/resource metadata only where the existing registry needs a canonical mapping
- Test: `tests/integration.rs` grant satisfied/denied and operation-time enforcement tests

- [ ] **Step 1: Write the failing operation-time tests**

Add tests that call `fs.read` and `net.connect` with matching and non-matching resource prefixes, and a test that changes/observes host state so a denied operation demonstrably fails at the host boundary rather than only at an earlier scope-entry check. Cover both `run_lowered_interpreted` and the Wasm backend where the existing test helpers support both.

- [ ] **Step 2: Run the focused tests to verify they fail**

Run the exact integration tests. Expected: current host calls ignore `RunConfig` grants and therefore the denied cases fail their assertions.

- [ ] **Step 3: Implement the minimal boundary enforcement**

Map each registry effect pair to a canonical grant domain such as `fs.read` or `net.connect`; derive a concrete `resource_prefix` from the operation’s path/endpoint/name argument when the ABI exposes one; and invoke the active scheduler check immediately before the real filesystem/network/process/environment operation. Keep pure operations and test intrinsics ungated. If an operation has no concrete resource argument, check the canonical domain against any held prefix without inventing a filesystem/network prefix. Preserve `resource_prefix` as an independent axis and record the check cost with a focused benchmark/timing test or a documented constant-time-per-effect note.

- [ ] **Step 4: Run the focused operation tests**

Run the new tests and the relevant existing host/resource tests. Expected: matching grants reach the host operation, non-matching grants fail before side effects, and both execution paths agree.

### Task 5: Enforce manifest omission before execution and add Vibra coverage

**Files:**
- Modify: `src/main.rs`, `src/package.rs`, and project-run routing so project manifests seed `RunConfig` before execution
- Modify: `src/lower.rs`/`src/effect_semantics.rs` only if the lowered entry effect row needs a runtime requirement artifact; preserve #249’s under-declaration errors
- Create or modify: `tests/lang-capability-grants.vib`
- Test: `tests/integration.rs` manifest-omission-before-execution case

- [ ] **Step 1: Write the failing omission and language tests**

Add a Rust test whose project manifest declares no `fs.read` grant while its entry code declares/performs `fs.read`; assert execution fails before the host operation or program-visible side effect. Add one `.vib` case that exercises the denied authority path with an expected runtime failure and keeps its symbols/test names kebab-case.

- [ ] **Step 2: Run the focused tests to verify they fail**

Run the exact Rust test and `cargo run -- test --filter <case>`. Expected: the manifest omission test currently executes ambiently, and the new language case is not yet supported.

- [ ] **Step 3: Wire project execution and entry requirements**

Load the containing `project.vib` for source/package execution, seed the configured root grants before any top-level statement, and validate the inferred `main` effect row against the manifest authority before invoking the backend. Keep #249’s declared/inferred distinction and `E-EFFECT-001` under-declaration behavior intact; do not move resource budgets into this issue.

- [ ] **Step 4: Run focused omission/language tests**

Run the new Rust test and the filtered Vibra case. Expected: omitted authority fails before execution and the case demonstrates the same denial through the language runner.

### Task 6: Update accepted contracts and finish verification

**Files:**
- Modify: `docs/decisions/effect-system.md`
- Modify: `docs/decisions/philosophy.md`
- Modify: `docs/reference/project-layout.md`
- Modify: `README.md` only if the user-facing run/manifest contract changes
- Modify: `docs/index.md` only if a new canonical document is added

- [ ] **Step 1: Update contracts**

Replace statements that effects are erased or host operations are unconditionally available with the runtime-grant contract, including manifest-readable authority, operation-time re-checking, resource-prefix scoping, monotone attenuation, amplification rejection, and the explicit limitation that #251 budgets are not part of this change.

- [ ] **Step 2: Run formatting and both complete suites**

Run `cargo fmt --all -- --check`, `cargo test`, and `cargo run -- test`. Also run `cargo run -- fmt tests/lang-capability-grants.vib --check` and `cargo run -- lint tests/lang-capability-grants.vib --deny-warnings` when the new Vibra test is present. Expected: all required commands exit successfully; if the stdlib submodule remains unavailable, report the exact setup blocker rather than hiding it.

- [ ] **Step 3: Review the diff and commit locally**

Run `git diff --check`, `git status --short`, and inspect the final diff for scope creep. Commit the implementation on `codex/issue-253-capability-grants` with a focused message. Do not push, open a PR, merge, or remove the requested worktree.
