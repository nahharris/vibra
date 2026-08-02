---
title: Plan — Handle lifecycle via in-danger propagation
category: plans
status: proposed
updated: 2026-08-02
issue: 255
---

# Plan: handle lifecycle (#255)

**Wave 5. Starts as a spike, not an implementation.** The plan's first
deliverable is a go/no-go recommendation.

## Why

Host handles are copyable, closeable, and their lifecycle is unenforced.
Use-after-close and double-close are runtime errors at best. The effect-system
contract explicitly defers affine ownership.

## Design decisions

**Do not adopt linearity.** *Typestate via Revocable Capabilities* shows
capabilities need not be linear: state is an abstract type member, capabilities
are path-dependent types, revocation is a compile-time-only operation extending
a killed-set, and safety comes from requiring each use's reachability qualifier
to be transitively disjoint from that set. Closing one handle invalidates what
may reach it while leaving unrelated handles usable.

Linearity would break Vibra's copyable-handle model and impose exactly the
annotation burden the rest of this roadmap is reducing. That is a bad trade for
an LLM-authored language.

**Do not adopt path-dependent typestate either — yet.** The paper is a design
paper: no benchmarks, no mutable-field support, and it assumes a DOT-style host
with path-dependent types Vibra does not have. Its important structural insight
(path-dependence is what supplies resource *identity*; reachability or
capturing types alone cannot, since they only express "may refer to") is worth
carrying, but building the type theory to exploit it is a research project, not
a roadmap item.

**Take the cheap approximation: in-danger propagation.** Montenegro, Peña and
Segura statically taint everything sharing structure with a destroyed value.
This gives borrow-checker-like safety without linearity and without new type
theory.

**Scope strictly to host-backed endpoints.** Not general ownership, borrowing,
or lifetimes for ordinary values. Endpoints are unforgeable nominal newtypes
with a closed set of constructors, which is what makes the analysis tractable
here and intractable in general.

## The honest cost warning

The source paper is explicit that the **sharing analysis, not the typing rules,
is the expensive part**. Vibra's position is favourable — value semantics and
unforgeable nominal endpoints mean far less aliasing than the ML-family setting
the paper targets — but that is a hypothesis, not a fact, and this plan treats
it as one.

## Phases

1. **Spike (deliverable: a written recommendation).** Determine whether the
   sharing analysis is tractable over Vibra's value semantics. Concretely,
   answer: can a handle be reached from a record field, an array element, a map
   value, a tuple, a closure capture, or a `task` capture? Each yes is a
   sharing edge the analysis must track. Enumerate them against the actual type
   system rather than reasoning abstractly.
   **The spike may recommend stopping.** If handles turn out to be reachable
   through most aggregate positions, the analysis approaches whole-program
   alias analysis and the cost/benefit inverts. Recommending no-go is a
   successful outcome, not a failed one.
2. **If go — analysis.** Implement in-danger propagation over endpoint values.
3. **Diagnostics** for use-after-close and double-close, registered in
   `schemas/linter-codes.json`, each naming the closing site as a related span.
4. **Corpus check**; `examples/fs-roundtrip.vib` closes handles in some match
   arms and not others, and is a useful real test of arm-sensitivity.
5. **Contract update** to `decisions/effect-system.md`, replacing the deferred-
   ownership note with what was actually adopted.

## Testing

Rust tests: use after close; double close; close in one `match` arm only (must
be diagnosed on the path where it is closed, not globally); handle passed to a
function and closed there; unrelated handles still usable after a close (this
is the whole point of non-linearity and is the primary acceptance gate); handle
stored in a record then closed through the record.

## Risks

Beyond tractability: false positives are worse here than missed detections. A
model that receives a spurious use-after-close error will restructure working
code to appease it. If the analysis cannot be made precise enough for
arm-sensitive closing, ship it as a warning rather than an error, and say so
in the contract.

## Definition of done

Either a written no-go recommendation with the enumerated sharing edges that
justify it, or: use-after-close and double-close diagnosed, unrelated handles
unaffected, arm-sensitive closing handled, and both suites pass.
