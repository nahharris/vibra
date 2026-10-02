# Step 14b — iteration defaults and adapters

Prerequisite: Step 14a merged. Stage 3B behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Iteration** (default members,
  adapter types).
- [Decision ledger](decision-ledger.md) rows D18.1, D22.1, and D22.2.

## Scope

1. **Defaults.** `@std.iter` adds `map`, `filter`, `skip`, `take`, and
   `collect`, written in Vibra. `map` has its own generic parameter `out`, so
   it can change the element type (D18.1). `collect` drains through a module
   helper at constant depth.
2. **Adapters.** `mapped-iter`, `filtered-iter`, `skipped-iter`, and
   `taken-iter` are ordinary standard-library `deftype`s with nested `impl`
   blocks supplying only `next`.
3. **Calling a default.** A default is never redeclared, so the call is a
   direct call of its one function: `self` is the receiver's type, the
   interface's parameters are the arguments at which the receiver conforms,
   and the member's own generics are inferred (D22.2).
4. **Both check paths.** The workspace path loads `@std.iter` as a module. A
   single-source run that imports it checks that module's functions, defaults,
   and `impl` members under its own source identity and names.

Default callbacks require `effects: ()`; a nonempty effect row stays
`@tool.unavailable` until M4.

## Test matrix

- Positive: each default member over each builtin conformance and over a user
  `deftype` implementing `(iter item)`; `map` changing `i32` to `str`; laziness
  shown by `take` over an infinite user iterator; `collect` past the non-tail
  activation limit; all in one source and in a workspace.
- Negative: a redeclared default body; a callback with a nonempty effect row;
  a default over a value that is not an iterator.

## Done

The cases are in the corpus and validation passes.
