# Step 14a — the iteration contract

Prerequisite: Step 13 merged. Stage 3B behavior step, the first of three that
Step 14 is split into: the contract here, its default members and adapter
types in [Step 14b](14b-iter-defaults.md), and the written laws and the
missing-role check in [Step 14c](14c-laws-and-roles.md).

## Read before editing

- [Types](../../spec/02-type-system.md): **Iteration** (the `iter` contract,
  closed builtin conformance).
- [Decision ledger](decision-ledger.md) rows D18.1 and D22.1.

## Scope

1. **The contract.** `@std.iter` declares `(defint iter where: (item any) …)`
   claiming `@iter`, with the abstract `next`. Every run declares it, and a
   module names it through `@std.iter` or `(import iter @std.iter.iter)`.
2. **Closed builtin conformance.** `(array t)`, `(dict k v)`, `str`, and
   `(option t)` conform through the closed registry: they widen to
   `(iter item)` at their item type, and `iter.next` on them is answered by the
   toolchain.
3. **Dispatch of a generic interface.** A member of a generic interface
   dispatches at run time through an `(iter item)` interface value, at the
   value's arguments, and through the `self` of a default member, at the
   interface's own.
4. **Inference through conformance.** A generic parameter typed as a generic
   interface's value, such as `(iter item)`, takes its arguments from the one
   way the operand conforms, and the operand widens to it.
5. **Bounds.** A generic interface cannot be a `where:` bound, which is one
   name; its interface value takes that role.

## Test matrix

- Positive: `iter.next` over each builtin conformance and over a user `deftype`
  implementing `(iter item)`; a tail-recursive walk over an `(iter item)`
  interface value at constant depth; the same across an import.
- Negative: bare `iter` in an `impl` target, a type, and a bound; an `item` not
  declared in the owner's `where:`; a `next` whose result names another element
  type; `(result t e)` iterated.

## Done

The cases are in the corpus and validation passes.
