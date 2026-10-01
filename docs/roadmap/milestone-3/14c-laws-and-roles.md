# Step 14c — contract laws and the role check

Prerequisite: Step 14b merged. Stage 3B behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Nominal declarations** (the key
  contracts), **Iteration**, **Language core and standard library** (roles).
- [Decision ledger](decision-ledger.md) rows D17.2 and D18.2.

## Scope

1. **Laws.** The type chapter states the algebraic laws of `equatable`,
   `ordered`, and `iter`, each with a conformance example. `hashable` has none,
   since v1 does not declare it.
2. **The role check.** Every role of the closed table is now claimed, so the
   loader's missing-role check, deferred since Step 4b, is enabled for every
   role: a missing or repeated role is a provenance diagnostic.

## Test matrix

- Positive: one conformance example per law.
- Negative: a standard library whose manifest or modules leave a role
  unclaimed, and one that claims a role twice.

## Done

The iteration coverage clause of the conformance chapter maps to cases, and
validation passes.
