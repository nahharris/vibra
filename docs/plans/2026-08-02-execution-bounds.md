---
title: Plan — Bounded execution
category: plans
status: proposed
updated: 2026-08-02
issue: 251
---

# Plan: fuel, memory ceiling, and arena reclamation (#251)

**Wave 3.** Independent of the language-surface work; can start any time, but
sequenced here because Tier 1 surface changes deliver value sooner.

## Why

The Principles pipeline ends at a sandboxed deterministic runtime with bounded
execution. Vibra has deadlines and nothing else. Worse, the value arena in
`src/wasm_backend.rs` appends and never frees, so any memory bound is
unenforceable regardless of how it is specified.

## Design decisions

**Fix reclamation first.** Shipping a memory ceiling against a non-reclaiming
arena would produce a limit that every long-running program hits for reasons
unrelated to its actual working set. The ceiling is meaningless until the leak
is gone, so the order is not negotiable.

**Reclamation strategy: NOT scope-tied regions.** An earlier version of this
plan chose scope-tied regions on the reasoning that they reuse
`src/async_runtime.rs`'s scopes. Pre-implementation investigation refuted that
(see `risk-findings/251-escape-analysis.md`), on two independent grounds:

1. **There is no scope construct to tie a region to.** The language has `task`,
   `spawn`, and `join`, and no `scope` form. `open_scope`/`close_scope` have
   zero call sites outside `src/async_runtime.rs` and its own tests, and
   `docs/reference/async-structured-concurrency.md:190` states milestones 1–5
   are not implementation-complete. The scope machinery is a well-tested
   standalone model with nothing plugged into it.
2. **Values escape by every available route**, not as an edge case but as the
   mainstream path: returning out of a function (`src/wasm_backend.rs:1297`),
   `join` handing the parent the child's own arena handle (`:1397`, `:1405`),
   and `task` — whose `captures:` clause is **dropped entirely at codegen**,
   with the body inlined into the parent frame (`:1396`). `Rc`-aliased mutable
   cells are worse than an escape: they make reclamation silently *partial*,
   which would leave the phase-2 memory ceiling enforced against a number that
   does not track reality.

**Revised sequence**, in descending value-per-risk:

- **1a. Generation-tagged arena handles.** Handles are currently un-tagged
  absolute 1-based indices (`src/wasm_backend.rs:331`), so any reclamation
  makes stale handles alias freshly allocated values — wrong answers rather
  than crashes. A generation tag on the `OperationToken` model
  (`src/async_runtime.rs:26`) is a hard prerequisite for *every* reclamation
  strategy and is worth landing on its own merits.
- **1b. Function-frame reclamation, not scope regions.** Function boundaries do
  exist in the executed language (`src/wasm_backend.rs:1061`), unlike scopes.
  The only escape at that boundary is the single returned handle, which can be
  copied forward into the caller's region — a one-value escape analysis rather
  than the general problem. This addresses the motivating case directly, since
  the unbounded loop growth is frame-local. **State the limitation up front:
  `Rc`-aliased values still leak under this scheme.**
- **1c. Full scope-tied regions.** Blocked on a scope construct existing in the
  executed language *and* on real escape analysis. Its own issue, not this one.

Reference counting adds per-value overhead and cycle problems; a collector is a
large subsystem with nondeterministic timing that would undermine the
determinism goal. Frame reclamation remains the cheapest option that is also
implementable against what exists today.

**Open uncertainty, not yet settled:** whether frame-tied reclamation is sound
against the `Rc`-aliasing route (`src/lower.rs:103`) and the `frames` /
`bindings` side tables (`src/wasm_backend.rs:374`, `:557`). This needs a
dedicated pass before 1b is committed to.

**Coarse fuel accounting.** SandCell's finding is that the dominant sandboxing
cost is boundary-crossing frequency and data volume, not the enforcement
primitive — allocating cross-boundary data in a shared region up front took one
benchmark from 89% overhead to 3%. Per-instruction fuel buys precision nobody
needs at a cost the evidence says is the wrong thing to spend. Account per call
and per loop back-edge, which bounds non-termination while staying cheap.

**Exhaustion is an uncatchable scope abort.** This is the deliberate choice
against ergonomics. A catchable failure is more useful to application authors,
but a budget a program can catch and ignore is not a security boundary, and the
whole point is to give embedders something they can rely on. Cleanup still
runs; the scope simply cannot continue.

**Budget declared in `project.vib`.** An embedder must be able to read a
program's maximum resource envelope without compiling or running it. This is
the same reasoning that puts the authority declaration there in #253, and the
two should share a manifest section.

**Monotone narrowing, matching grants and deadlines.** A child scope may
request less, never more. Vibra already enforces exactly this for
`CapabilityGrant`; fuel and memory join the same lattice rather than inventing
a parallel mechanism.

## Phases

1. **Arena reclamation** with scope-tied regions; a test with a long-running
   loop that currently grows without bound.
2. **Memory ceiling** on `ScopeLimits`, inheriting monotone narrowing.
3. **Fuel budget**, accounted per call and loop back-edge.
4. **Manifest surface** in `project.vib` plus project-manifest schema update.
5. **Reference docs**: `reference/async-structured-concurrency.md` and
   `reference/project-layout.md`.

## Testing

Rust tests: fuel exhaustion; memory-ceiling exhaustion; monotone-narrowing
rejection; inheritance through nested scopes; interaction with deadline
cancellation (deadline and fuel racing must produce a deterministic winner —
`src/async_runtime.rs` already has a precedent test for deadline versus
completion); cleanup runs on abort; and arena reclamation under sustained
allocation.

## Risks

Reclamation is the risky part, not the limits — and the investigation has
already converted the largest risk into a known constraint rather than an open
question. What remains:

- **The `Rc` aliasing route defeats accounting, not just reclamation.** A
  memory ceiling enforced against a partially-reclaiming arena measures the
  wrong thing. Settle 1b's soundness against `src/lower.rs:103` before the
  ceiling in phase 2 is trusted.
- **Fuel and memory ceilings inherit the same wiring gap as #253.**
  `ScopeLimits` inheritance lives in `open_scope_with_limits`, which real
  execution never calls. Limits added there are inert until scope lifecycle is
  wired into `src/execute.rs`. **This issue and #253 share that work**;
  whichever lands first should make it reusable rather than duplicating it.
- **Host handles are orthogonal and must not be conflated.** `FileTable`
  (`src/execute.rs:138`) lives for the whole program run with no scope
  association, and `Scheduler::open_resource` mints synthetic IDs unconnected
  to any file descriptor. Reclaiming an arena slot holding a handle orphans the
  OS resource rather than closing it. Handle lifetime is #255's subject.

## Definition of done

The arena reclaims, fuel and memory ceilings are enforced and inherited,
budgets are declarable, exhaustion aborts cleanly, and both suites pass.
