# Step 14b — iteration defaults and adapters

Prerequisite: Step 14a merged. Stage 3B behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Iteration** (default members,
  adapter types).
- [Decision ledger](decision-ledger.md) rows D18.1 and D22.1.

## Scope

`@std.iter` adds the defaults `map`, `filter`, `skip`, `take`, and `collect`,
written in Vibra. `map` has its own generic parameter `out`, so it can change
the element type (D18.1), which needs contract members with their own
generics. The adapter types `mapped-iter`, `filtered-iter`, `skipped-iter`,
and `taken-iter` are ordinary standard-library `deftype`s with nested `impl`
blocks. Default callbacks require `effects: ()`. A program that calls a
default runs standard-library Vibra, so the defaults and adapters must join
the programs of both check paths.

## Test matrix

- Positive: each default member over each builtin conformance and over a user
  `deftype` implementing `(iter item)`; `map` changing `i32` to `str`; laziness
  shown by `take` over an infinite user iterator.
- Negative: a redeclared default body; a callback with a nonempty effect row.

## Done

The cases are in the corpus and validation passes.
