# Step 14 — iteration

Prerequisite: Step 13 merged. Stage 3B behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Iteration** (the `iter` contract,
  default members, adapter types, closed builtin conformance).
- [Decision ledger](decision-ledger.md) row D18.1.

## Scope

`@std.iter` declares `(defint iter where: (item any) …)` claiming `@iter`, with
`next` abstract and the defaults `map`, `filter`, `skip`, `take`, and `collect`
written in Vibra. `map` has its own generic parameter `out`, so it can change
the element type (D18.1). The adapter types `mapped-iter`, `filtered-iter`,
`skipped-iter`, and `taken-iter` are ordinary standard-library `deftype`s
with nested `impl` blocks. Closed builtin conformance covers `(array t)`,
`(map k v)`, `str`, and `(option t)`. Default callbacks require `effects: ()`,
and effectful walks are tail-recursive functions over `iter.next`. This step
also writes the algebraic laws of `equatable`, `ordered`, and `iter` into the
type chapter, each with a conformance example; `hashable` has none, since v1
does not declare it.

## Test matrix

- Positive: each default member over each builtin conformance and over a user
  `deftype` implementing `(iter item)`; `map` changing `i32` to `str`; laziness
  shown by `take` over an infinite user iterator; an effectful tail-recursive
  walk at constant depth.
- Negative: bare `iter` in an `impl` target; an `item` not declared in the
  owner's `where:`; a `next` whose result names another element type; a
  redeclared default body; a callback with a nonempty effect row;
  `(result t e)` iterated.

## Done

The iteration coverage clause of the conformance chapter maps to cases, and
`@iter` joins the claimed roles, so the loader's missing-role check (deferred
since Step 4b) is enabled for every role. Validation passes.
