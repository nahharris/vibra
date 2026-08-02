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

**Reclamation strategy: scope-tied regions.** Of the three candidates
(reference counting, region reclamation, a collector), regions align with
machinery Vibra already has — `src/async_runtime.rs` scopes with monotone
narrowing and deterministic teardown. Values allocated within a scope are
released at its close. This reuses an existing, tested lifetime notion instead
of introducing a second one, and it composes with the `try` early-return path
from #248, which must run the same teardown.

Reference counting adds per-value overhead and cycle problems; a collector is a
large subsystem with nondeterministic timing that would undermine the
determinism goal. Regions are the cheapest option that is also the most
consistent with the language's existing structure.

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

Reclamation is the risky part, not the limits. Scope-tied regions require
knowing that no value outlives its allocating scope. Vibra's value semantics
make that plausible, but escaping values — a value returned from a scope, or
captured by a `task` — are the counterexample class. Establish whether escape
analysis is needed **before** committing to regions; if it is, that is a
material scope increase and should be split into its own issue rather than
absorbed silently.

## Definition of done

The arena reclaims, fuel and memory ceilings are enforced and inherited,
budgets are declarable, exhaustion aborts cleanly, and both suites pass.
