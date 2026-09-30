# Step 8 — core library foundation

Prerequisite: Step 7 merged. Stage 3A behavior step.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **M3 compiler intrinsic registry**,
  **Canonical value encoding**.
- [Projects](../../spec/04-programs-and-packages.md): **Toolchain
  standard-library input**, **Tests**, **M3 assertion contract**.
- [Decision ledger](decision-ledger.md) rows D4.2, D4.3, D5.2, D7.2.

## Scope

Every Stage 3A registry row, bound by trusted declarations in the listed
modules; the reviewed Vibra-written composite functions (boolean connectives,
`char` comparison and classes, text search/split/trim, array folds); and the
generic `assert.equal` replacing the five `assert.equal-*` members. Change the
**Tests** example in the projects chapter to `assert.equal` in the same PR,
refresh the example inventory, and migrate every M2 case and CLI process test
that uses a removed assertion.

Per ledger D17.1–D17.3 (Step 4a), this step also moves `bool`, `str`, `bytes`,
`ordering`, and the standard error enums into the standard library as
`deftype`s over the core, claiming `@bool`, `@str`, and `@bytes`. The text and
bytes operations become standard-library Vibra with native implementations,
and the registry keeps only primitive operations over the core. A migrated
type keeps its canonical value encoding.

## Implementation notes

The registry lives with the M2 compiler registry in `vibra-ir::external`;
each operation has an interpreter implementation and host tests at its
boundaries: minimum and maximum values, zero divisors, the signed minimum
divided or negated, shift amounts equal to and above the width, empty and
non-ASCII strings, invalid UTF-8, surrogate code points, NaN and signed zeros.
Registry signatures are checked exactly against the trusted declarations.

## Test matrix

- One interpreter case per operation family through its real module import,
  plus the boundary cases above in host tests.
- Negative: an unknown `symbol:` in the trusted package
  (`@external.unknown-symbol`); a signature that differs from the registry; a
  project module declaring `external:`.
- Tests: `assert.equal` pass and failure with compound values rendered by the
  canonical encoding; an `fn` operand (`@type.function-not-equatable`).

## Done

The M2 registry note on `integer.*` symbols is closed by the static methods of
the numeric primitive types; all registry rows have evidence; validation passes.
