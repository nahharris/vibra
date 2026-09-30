# Step 13 — destination dispatch and conversion

Prerequisite: Step 12 merged. Stage 3B behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Interfaces and methods**
  (destination dispatch), **Conversion**.
- [Runtime](../../spec/06-runtime.md): the `conversion-error` enum.

## Scope

Destination-dispatched contract members: the checker unifies the written
expected type with the member's result type and takes `self` from it, with
`@type.ambiguous-destination` when no written expected type reaches the call.
The standard `from` and `try-from` interfaces and their numeric
implementations, returning `conversion-error` for `try-from`;
`@type.redundant-conversion` for a `from`/`try-from` pair on one source;
`@type.ambiguous-implementation` when two implementations of one interface
remain candidates at a call; and factory members such as `(defn empty ()
self)` selected the same way. This step also removes the decimal-text
round trips the Step 8c text bodies use to move between `u8` and `u32`.

## Test matrix

Follow the conformance chapter's conversion and destination-dispatch
paragraphs:

- Positive: a `from` implementation on a destination `deftype`; a `try-from`
  returning `conversion-error`; destination selection through a written
  parameter type, a result type, and `as`; one receiver implementing
  `(from i16)` and `(from i8)`; a member naming `self` in its result and a
  variadic tail; a factory selected from an expected type.
- Negative: a bare call with no expected type (`@type.ambiguous-destination`);
  a `from`/`try-from` pair on one source and `(from t)` with `(try-from i32)`
  (`@type.redundant-conversion`); an unsuffixed literal matching both targets
  (`@type.ambiguous-implementation`); `(from t)` with `(from i32)`
  (`@type.overlapping-implementation`); the two undispatchable members.

## Done

The query and index records show the two-target receiver's two block identities
and two `convert` member identities. Validation passes.
