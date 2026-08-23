# Handle Lifecycle Review Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the local same-binding handle lifecycle lint scope-correct, reset it after assignment, exempt borrowed standard streams, and document/test the shared `stream.error.resource-closed` contract without changing the whole-program in-danger NO-GO finding.

**Architecture:** Keep the analysis as a warning-only, function-local direct-binding pass. Resolve references through the same lexical scope boundaries as `src/ast/scope.rs`, key lifecycle state by each binder's source identity, intersect state at branch joins, and skip aliases, aggregates, calls, generics, anonymous functions, and tasks. The runtime remains responsible for the authoritative typed lifecycle result.

**Tech Stack:** Rust AST tooling, SARIF integration tests, Vibra standard-library fixtures, Markdown contract documentation, Cargo test runner.

---

### Task 1: Complete scope-resolved lifecycle state

**Files:**
- Modify: `src/sexpr_tooling.rs:617-980`
- Test: `src/sexpr_tooling.rs:1539-1706`

- [ ] **Step 1: Preserve the failing reproduction evidence.** The existing attempted patch must first fail to compile because its recursive lifecycle calls still use the old signature and its state map still receives string keys. The focused command is `cargo test --lib sexpr_tooling`; the expected failure includes `expected &(usize, usize), found &String` and missing `&mut LifecycleScopes` arguments.

- [ ] **Step 2: Resolve direct references by lexical binding ID.** Keep `BindingId = (name.span.start, name.span.end)`, resolve each `ExprKind::Reference(name)` through the innermost scope, and use the resolved ID for `LifecycleState.closed` and `LifecycleState.borrowed`. `let` must visit its value before `scopes.bind(name)`, `set` must visit its value then remove the resolved target from both state sets, and close diagnostics must retain the reference spelling for their message and related span.

- [ ] **Step 3: Mirror compiler scope boundaries.** Push/pop scopes around `if`, `while`, `for`, and each match arm; bind `for` names and recursively bind `PatternKind::Bind` names; leave `do` in its current scope; skip `AnonymousFunction`, `Task`, `Spawn`, and `Join`. Pass `&mut LifecycleScopes` through every recursive call. Merge `closed` and `borrowed` by intersection so only facts true on every branch survive.

- [ ] **Step 4: Run the focused Rust tests.** Run `cargo test --lib sexpr_tooling`. Expected result: the existing same-binding, double-close, path-local, sibling-scope, reassignment, borrowed-stream, and non-alias tests pass.

### Task 2: Lock the borrowed endpoint and error contract

**Files:**
- Modify: `docs/decisions/effect-system.md:166-169`
- Modify: `docs/reference/wasm-abi.md:69-74`
- Do not modify: `docs/plans/risk-findings/255-handle-reachability.md`

- [ ] **Step 1: Keep borrowed standard streams non-revocable in the linter.** Mark only direct zero-argument calls to `io.stdin.open`, `io.stdout.open`, and `io.stderr.open` as borrowed at their fresh `let` binding. A close on such a binding must not create closed state or a double-close warning.

- [ ] **Step 2: Correct the normative error name.** Replace prose that calls lifecycle failures `fs-error.resource-closed` with `stream.error.resource-closed`, while retaining provider-specific `fs-error` conversion where the filesystem adapter explicitly maps the shared stream error. Leave the whole-program NO-GO rationale/report byte-for-byte unchanged.

### Task 3: Add focused boundary and SARIF coverage

**Files:**
- Modify: `tests/integration.rs:6022-6065`
- Test: `src/sexpr_tooling.rs:1539-1706`

- [ ] **Step 1: Assert SARIF metadata and related closing location.** Keep the existing `W-HANDLE-001` SARIF test and assert its rule ID, warning level, related location ID, physical location, and registered summary.

- [ ] **Step 2: Assert the runtime boundary contract.** Add a hermetic integration fixture that opens a real file, closes it, and matches both duplicate close and later write as `result.err (stream.error.resource-closed)`. Add a borrowed stdout close/write fixture or retain the focused linter test proving borrowed endpoint close is a no-op.

- [ ] **Step 3: Check the diff.** Run `git diff --check` and verify the NO-GO report is unchanged with `git diff -- docs/plans/risk-findings/255-handle-reachability.md` producing no output.

### Task 4: Verify and commit

**Files:**
- Verify: all changed files in the worktree

- [ ] **Step 1: Format/check.** Run `cargo fmt --all -- --check` and `git diff --check`; both must exit 0.

- [ ] **Step 2: Run both required suites.** Run `cargo test` and `cargo run -- test`; both must exit 0 with no failing tests.

- [ ] **Step 3: Commit the finalized worktree.** Stage the lifecycle source, focused tests, and corrected contract documentation, then create one focused commit with message `fix: tighten direct handle lifecycle lint`. Do not push, merge, or open a pull request.
