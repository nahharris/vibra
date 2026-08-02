---
title: Plan — Effect inference with boundary declarations
category: plans
status: proposed
updated: 2026-08-02
issue: 249
---

# Plan: effect inference at boundaries (#249)

**Wave 2, runs parallel with #250.** Blocks #253, which derives runtime grants
from declared effects.

## Why

`main` in `examples/fs-roundtrip.vib` declares seven effects that are the
transitive union of everything it reaches. Nothing in that list is derivable
from local context, which contradicts `philosophy.md`'s own rule that names and
imports make local context sufficient.

The decoding research does not justify the burden. Explicit annotations help
because they let prefix-time type search terminate at **signature positions**.
A transitive union restated at internal call sites is a derived fact, not a
signature, and buys nothing for decoding, documentation, or review.

## Design decisions

**Mandatory at the boundary, inferred inside.** Exported functions, interface
methods, and `deffect` operations keep mandatory declarations — that is where
the decoding and documentation value lives, and where a cross-module contract
actually exists. Module-private functions infer.

**Over-declaration warns; under-declaration errors.** Asymmetric on purpose. An
under-declared ceiling is unsound once #253 derives grants from it. An
over-declared one is merely imprecise, and warning rather than erroring lets an
author widen a signature deliberately for forward compatibility.

**Root subsumption is declaration-side sugar only.** Writing `fs` covers
`fs.read`, `fs.write`, `fs.metadata`. It must **not** propagate: a function
inferred to perform only `fs.read` reports `fs.read`, never `fs`. Sugar that
silently widened inferred authority would defeat the point of the system and
would hand #253 an over-broad grant set.

**`main` infers.** It is exported by definition and is the worst offender. The
program-level authority declaration belongs in `project.vib` (#253), where an
embedder can read it without compiling — which is a better place for it than a
function signature anyway.

**Effect polymorphism is out of scope; say so explicitly.** A generic
combinator taking a function argument still declares a fixed ceiling, forcing
callers to the maximum. Effect sets as type-level values fix it and belong with
#151. Recording the limitation prevents this issue from being reopened as a
bug.

## Phases

1. **Inference pass** over the module call graph in `src/effect_semantics.rs`
   (251 lines today — the machinery for call edges already exists in the
   `vibra effects` report). Compute least fixed point; the graph is finite and
   effects form a simple join semilattice, so termination is trivial, but
   recursion must be handled by iterating to a fixpoint rather than recursing.
2. **Checking**: declared ⊇ inferred at every boundary, with the asymmetric
   severity above.
3. **Root subsumption** in declaration parsing only.
4. **Reporting**: `vibra effects` gains an inferred column per function, and
   the difference from declared. This is the measurement instrument for the
   falsifiable prediction below.
5. **Contract amendment** to `decisions/effect-system.md`, in the same change.
6. **Corpus simplification**; report annotation-volume reduction in the PR.

## Testing

Rust tests: inference through a private call chain; mutual recursion; over-
declaration warns; under-declaration errors; root subsumption expands on
declaration but never on inference; inference across interface dispatch (the
implementation's ceiling must stay below the method's); `deffect` operation
owners still implicit. One `tests/*.vib` case.

## Falsifiable prediction

Effect-annotation diagnostics on LLM-authored Vibra drop substantially with no
regression in constrained-decoding acceptance. Measurable via `vibra effects`
once #254's baseline exists.

## Risks

Interface dispatch is where inference gets hard: a call through an interface
method must use the *method's* declared ceiling, not any particular
implementation's, or inference becomes whole-program and stops being modular.
Pin this with a test in which two implementations have different actual
effects.

## Definition of done

Private functions carry no effect annotations, boundaries still do, the report
shows both, the contract is amended, and both suites pass.
