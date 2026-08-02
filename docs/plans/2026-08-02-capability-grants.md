---
title: Plan — Effects enforced as runtime grants
category: plans
status: proposed
updated: 2026-08-02
issue: 253
---

# Plan: capability grants (#253)

**Wave 4. Blocked by #249** — grants are derived from declared effects, so
inference and the declared/inferred distinction must exist first.

## Why

The Principles page's governing rule is: make invalid programs unrepresentable,
**and make dangerous authority explicit**. Vibra implements the first clause
well and the second not at all. Effects are static and erased;
`philosophy.md` concedes every host operation is unconditionally available and
that embedders must supply their own sandboxing.

LLMON supplies the general lesson: declared structure paid off only because a
runtime acted on it. Representation without enforcement is decoration, and
erased effects are exactly that.

## How much already exists — corrected

An earlier version of this plan claimed the runtime substrate was complete and
only surface syntax and a propagation rule were missing. Investigation before
implementation showed that was too optimistic; the corrected picture:

**Exists and is reusable.** `CapabilityGrant` (`src/async_runtime.rs:109`), its
containment relation `is_within` (`:116-126`), the amplification error
(`RuntimeError::CapabilityAmplification`, `:223`), and the narrowing check
(`:384-393`), with unit tests.

**Does not exist.** Any of it running in a real program. The root scope is
seeded empty at `src/execute.rs:943`, every grant-carrying construction site is
inside the `#[cfg(test)]` module beginning at `src/async_runtime.rs:1043`, and
`open_scope` is never called from `src/execute.rs` at all. The narrowing check
is unreachable in production.

So this issue has three parts, not two: seed the root from the manifest, **wire
scope lifecycle into real execution**, and add the operation-time check. The
middle part was not previously accounted for and should be scoped explicitly.

**#251 shares the wiring.** `ScopeLimits` inheritance sits in the same
never-called path, so fuel and memory ceilings will be equally inert until this
lands. Whichever issue goes first should do the wiring reusably.

## Design decisions

**Re-check at the boundary; do not trust the compiler.** FINE's contribution is
that a type-preserving lowering lets the enforcement point re-verify policy
rather than assume the compiler got it right. Plain Wasm validation cannot
express this, so the check lives in Vibra's host boundary. Concretely: the
runtime checks the grant at the operation, not only at scope entry — scope
entry checks are an optimization, not the guarantee.

**Two axes: effect-root domain, plus resource prefix.** An earlier version of
this plan chose "per effect root, not per operation" and stopped there. The
existing `CapabilityGrant` is a `{domain, resource_prefix}` pair whose
`is_within` already implements hierarchical path containment, which is a better
design than the one the plan proposed and costs nothing extra to adopt.

Use **domain** at effect-root granularity (`fs.read`, `net.connect`,
`process.spawn`) — coarse enough for an embedder to read at a glance, fine
enough to be meaningful; per-operation grants would produce authority lists as
unreadable as the ceilings #249 removes. Use **resource_prefix** to scope which
resources within that domain are reachable.

That second axis is what makes this a security feature rather than a label: the
useful embedder guarantee is not "this program touches the filesystem" but
"this program touches the filesystem only beneath this path."

**Reconcile domain naming with the effect inventory.** Existing tests use
`filesystem-read` and `network` (`src/async_runtime.rs:1163`, `:1177`); the
accepted contract uses `fs.read` and `net.connect`. The contract names win —
they are what authors write. Updating those tests is part of this work.

**Attenuation yes, amplification never.** A scope may drop authority it holds.
It may never acquire authority its parent lacks. This is the existing lattice;
the surface syntax must not offer a way to widen.

**Program authority is declared in `project.vib`.** An embedder must be able to
read a program's maximum authority without compiling or running it — that is
the property that makes this a security feature rather than a lint. Shares a
manifest section with #251's resource budget.

**Effects stop being erased.** This is the actual change. They remain static
for checking and decoding, and additionally lower to a grant requirement.

**One-shot revocable grants are deferred.** FINE identifies affine types as the
right cheap encoding, but that overlaps #255's handle-lifecycle work and should
follow it rather than duplicate it.

## Phases

1. **Manifest surface**: program grant set in `project.vib`, schema updated.
2. **Lowering**: declared effect ceiling becomes a grant requirement on the
   operation.
3. **Runtime check** at the operation in `src/execute.rs` / `src/host_abi.rs`,
   with scope-entry checking retained as the fast path.
4. **Surface syntax** for scope attenuation, recorded in
   `decisions/effect-system.md`.
5. **Contract updates**: the effect-system contract loses "erased"; the
   philosophy document's unsandboxed-host concession is replaced by what is now
   enforced.

## Testing

Rust tests: grant satisfied; grant denied at the operation; attenuation;
amplification rejected; denial across a `spawn` boundary; and a program whose
manifest omits an authority its code declares (must fail before running, not at
first use). One `tests/*.vib` case demonstrating a program failing for lack of
authority.

## Risks

**The performance question is unmeasured.** A per-operation check on every host
call is exactly the boundary-crossing frequency cost SandCell identifies as
dominant. Measure it: if per-operation checking is too expensive, the fallback
is scope-entry checking only, which is weaker but still infinitely better than
erasure. Decide with numbers, not by preference, and record the numbers in the
PR.

Second risk: under-declared effects become a soundness hole once grants derive
from them. #249's asymmetric severity — under-declaration is an error — is what
closes this, which is why the dependency order matters.

## Definition of done

Declared effects are enforced at runtime, program authority is manifest-
readable, attenuation works, amplification is impossible, the contracts no
longer claim erasure, and both suites pass.
