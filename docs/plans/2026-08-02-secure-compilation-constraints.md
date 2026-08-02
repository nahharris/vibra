---
title: Plan — Secure-compilation constraints
category: plans
status: proposed
updated: 2026-08-02
issue: 252
---

# Plan: scalars-only ABI and hardening-last ordering (#252)

**Wave 2, time-sensitive.** Must land before the host ABI grows. This is the
cheapest issue in the roadmap today and one of the most expensive to retrofit
later, so its priority comes from the derivative, not the level.

## Why

Full verified compilation is not affordable: Monniaux measures a
layout-changing verified pass at 20–40 lines of proof per line of
implementation, and SECOMP took ~43k LoC atop CompCert while still compiling
only tiny programs. This issue explicitly does **not** attempt that.

What it does is adopt the two design constraints that make such work possible
later, both of which are nearly free now and very expensive after the fact.

## Design decisions

**Scalars only across compartment boundaries.** SECOMP's isolation theorem
holds only because the cross-compartment interface carries scalars — no pointer
passing, no shared memory — and the authors call shared pointers a major open
problem. Wasm already gives Vibra well-bracketed call/return and numeric-only
parameters, which is what SECOMP had to bolt onto RISC-V. Vibra starts nearer a
defensible design than CompCert did and should not spend that advantage.

Concretely: handles crossing a boundary are opaque indices resolved on the
owning side, never shared references. This is already close to how host-backed
newtypes behave; the change is making it a stated, tested rule rather than an
accident of current implementation.

**Hardening passes run last, and the ordering is tested.** Kruse et al. show
pass composition preserves only the *intersection* of property classes, and
only under a well-formedness side condition. Pass ordering is a security
constraint. A comment saying so decays; a test that fails on reorder does not.

**Keep the two claims separate, permanently.** Monniaux proves his
transformation preserves semantics and explicitly not that the canary stops the
attack. Vibra should claim semantic preservation and never claim attack
prevention. This is a documentation discipline that costs nothing and prevents
the single most common overclaim in this area.

**Corollary worth recording:** any security mode a caller can disable must be
re-established at every entry point, because attacker context runs between
calls. Vibra has no such mode today, which is exactly why now is the time to
write the rule down.

## Phases

1. **New decision contract** in `docs/decisions/` stating all three rules with
   their sources, linked from `docs/index.md`. This is the deliverable; the
   code changes below enforce it.
2. **Audit `vibra_v1`.** Walk `schemas/host-abi.json` and `src/host_abi.rs`
   confirming no current entry carries a reference across the boundary.
   Document any exception with a rationale rather than quietly grandfathering
   it — an undocumented exception is how the rule dies.
3. **ABI test.** A test rejecting a cross-compartment signature that would
   carry a non-scalar, so a future ABI addition fails CI rather than review.
4. **Pass-ordering test** asserting hardening passes sort last in the pipeline.
   Vibra has few passes today, which is precisely why this is cheap now.

## Testing

Rust tests: ABI signature rejection for a reference-carrying entry;
pass-ordering invariant; and a documentation test asserting no contract claims
attack prevention.

## Risks

The audit may find an existing violation — most plausibly in the typed runtime
plugin or static Wasm FFI surface, where dependency-provided modules interact
with host memory. If so, resolve it as part of this issue rather than
documenting a permanent exception, because a single grandfathered pointer-
crossing makes every future isolation claim false.

## Definition of done

The rules are an accepted contract, the ABI is audited clean or its exceptions
justified, both invariants are enforced by tests, and both suites pass.
