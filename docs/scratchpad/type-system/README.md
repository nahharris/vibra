# Type system

These proposals explore type-system extensions against the
[active type system specification](../../spec/02-type-system.md).

## Generic implementations

**Current restriction.** An `impl` target must be closed, so `(impl (from t) ...)`
over a generic receiver parameter is unwritable, and associated types are
excluded.

Generic implementations follow the existing ownership and bound-agnostic
overlap rules unchanged: an implementation over `(box t)` overlaps with one
over `(box i32)`, and overlap is still decided at the declaration. The proposal
needs no new identity kind.

## Associated types and per-implementation errors

An associated type has one identity per implementation, so no atom path
addresses it. That conflicts with the namespace rule that every path resolves
to at most one entity and would add a fifth kind to the flat addressable
member namespace. `try-from` currently fixes the error to `conversion-error`.
A richer error can be expressed with an ordinary `defn`.

The proposal reopens if the fixed error type proves costly in user code.
Generalizing the contract could require existing `impl` blocks to declare the
associated type unless the contract supplies a default. A contract-level type
parameter with a default is a candidate that keeps every identity addressable.
Any such break would follow the charter's compatibility policy.

## Interface narrowing and runtime type tests

Narrowing an interface value needs a runtime type tag on every such value.
That conflicts with the current rules for runtime type tests and erasure of
`any`. A declared union provides discrimination under the existing rules.

## Type-scoped constants

A `def` inside a `deftype` would give a type-scoped constant one identity
and one atom path, fitting the existing member namespace. It is independent
of unions and conversion.
