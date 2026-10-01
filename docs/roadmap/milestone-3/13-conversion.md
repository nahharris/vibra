# Step 13 — destination dispatch and conversion

Prerequisite: Step 12 merged. Stage 3B behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Interfaces and methods**
  (destination dispatch), **Conversion**.
- [Runtime](../../spec/06-runtime.md): the `conversion-error` enum and the
  integer `to-U` registry family.
- [Decision ledger](decision-ledger.md) rows D21.1 and D21.2.

## Scope

1. **Selection among implementations.** A destination-dispatched member, a
   member of a generic interface, and a variadic member select their
   implementation at the call. A destination-dispatched member unifies the
   written expected type with its result type and takes `self` from it, with
   `@type.ambiguous-destination` when no written expected type reaches the
   call. The candidates are the implementations whose receiver covers that
   type; each is tried against the operands, exactly one must fit, and several
   are `@type.ambiguous-implementation`.
2. **Conversion contracts.** `@std.core` declares `from` and `try-from`. The
   builtin integer types conform through a closed registry (D21.1): the new
   `to-U` registry family, total exactly when the destination holds every
   source value. `@type.redundant-conversion` rejects a receiver converting
   from one source through both contracts.
3. **Factories.** A member such as `(defn empty () self)` is selected the same
   way, with or without a variadic tail.
4. **Text bodies.** The Step 8c text bodies move between `u8` and `u32` through
   the registry conversions, not through decimal text.

Member generics, labelled operands, a dict variadic tail, and a generic
interface dispatched through a bounded generic or an interface value arrive
with Step 14. Floating-point conversions are not in v1's registry.

## Test matrix

Follow the conformance chapter's conversion and destination-dispatch
paragraphs:

- Positive: a `from` implementation on a destination `deftype`; a `try-from`
  returning `conversion-error`; destination selection through a written
  parameter type, a result type, and `as`; one receiver implementing
  `(from i16)` and `(from i8)`; a member naming `self` in its result and a
  variadic tail; a factory selected from an expected type; every integer pair
  at its bounds.
- Negative: a bare call with no expected type (`@type.ambiguous-destination`);
  a `from`/`try-from` pair on one source and `(from t)` with `(try-from i32)`
  (`@type.redundant-conversion`); an unsuffixed literal matching both targets
  (`@type.ambiguous-implementation`); `(from t)` with `(from i32)`
  (`@type.overlapping-implementation`); a destination with no matching
  conversion; the two undispatchable members.

## Done

The conversion cases are in the corpus and validation passes. The query and
index records of the two-target receiver's block and member identities are
Step 15's.
