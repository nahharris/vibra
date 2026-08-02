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

## The cost is lower than it looks

`src/async_runtime.rs` already carries a per-scope `CapabilityGrant` set, and
`open_scope_with_limits` already rejects a child requesting a grant its parent
lacks — a monotone-narrowing authority lattice, implemented and tested. #213
removed the source-language authority type, not the runtime. What is missing is
a surface syntax and a propagation rule.

## Design decisions

**Re-check at the boundary; do not trust the compiler.** FINE's contribution is
that a type-preserving lowering lets the enforcement point re-verify policy
rather than assume the compiler got it right. Plain Wasm validation cannot
express this, so the check lives in Vibra's host boundary. Concretely: the
runtime checks the grant at the operation, not only at scope entry — scope
entry checks are an optimization, not the guarantee.

**Grants are per effect root, not per operation.** The root inventory
(`fs.read`, `net.connect`, `process.spawn`, …) is already the right
granularity: coarse enough that a human embedder can read a program's authority
at a glance, fine enough to be meaningful. Per-operation grants would produce
authority lists as unreadable as the effect ceilings #249 exists to remove.

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
