# Step 14c — contract laws and the role check

Prerequisite: Step 14b merged. Stage 3B behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Nominal declarations** (the key
  contracts), **Iteration**, **Language core and standard library** (roles).
- [Decision ledger](decision-ledger.md) rows D17.2, D18.2, and D22.3.

## Scope

1. **Laws.** The type chapter states the algebraic laws of `equatable`,
   `ordered`, and `iter`, each with a conformance example. `hashable` has none,
   since v1 does not declare it. The toolchain cannot check a law; the chapter
   says what a program that breaks one may still rely on.
2. **The role check.** Every role of the closed table is now claimed, `@iter`
   by a `defint`, so the loader's missing-role check, deferred since Step 4b,
   is enabled for every role: a standard library that leaves a role unclaimed
   is rejected, as an unknown or repeated role already is.

## Test matrix

- Positive: `V1-RUNTIME-workspace-test-contract-laws`, one test per law:
  `equatable`'s three, `ordered`'s three, their agreement, and for `iter` that
  `next` is a function of its iterator, `collect` is the walk, `map` preserves
  structure, `filter` selects in order, and `take` and `skip` split. Laziness
  is `take` over an infinite iterator in the Step 14b cases.
- Negative: a standard library that leaves a role unclaimed, beside the
  existing unknown and repeated roles, in the loader's host tests.

## Done

The iteration coverage clause of the conformance chapter maps to the Step 14
cases, and validation passes.
