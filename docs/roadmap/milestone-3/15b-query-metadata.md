# Step 15b — type-aware query metadata

Prerequisite: Step 15a merged. Stage 3B tooling step.

## Read before editing

- [Tooling](../../spec/05-tooling.md): **Workspace queries**, **M2 workspace
  position envelope**.
- [Decision ledger](decision-ledger.md) row D23.1.

## Scope

Source-position queries gain type-aware metadata for the Stage 3A and 3B forms:
constructor and pattern shapes, union members, interface members and bounds,
and the destination a dispatched member resolved to. The output stays
deterministic JSON under the workspace position envelope, byte-identical across
hosts for an identical snapshot.

The checked facts now come from the workspace check against the standard
library, so a module that imports `std` or another module keeps them; a
workspace that does not check falls back to the queried source alone. The
envelope gains (D23.2):

- type kinds for every checked type, with a declared type named rather than
  expanded;
- application kinds `@constructor` and `@contract`, the latter with a
  `dispatch` naming the interface, the member, the receiver type, the
  selection (`static`, `default`, `dynamic`, `closed`), and whether the
  destination selected it;
- a `pattern` fact for the `match` arm pattern at the position, with the
  narrowed member and the union's members on an `as` pattern.

Bodies of methods, interface defaults, and `impl` members are now walked for
roles, contexts, and scopes.

Left unavailable, by name: a fact for a subpattern, an application fact for
projection, lookup, effect operations, and lambda calls, an expected type the
source writes as a non-primitive type, and interface bounds as a separate
fact (a bounded parameter is reported as its `param` type, and a call through
it as a `dynamic` dispatch).

## Test matrix

- Query cases at a record and a wrapper constructor, a variant and a record
  pattern, an `as` narrowing, and contract member calls resolved statically,
  to a default, dynamically through an interface value and through `self`, by
  the destination, and by a closed conversion.
- Schema tests: every one of those queries validates and round-trips, a
  consumer reads the constructor, narrowing, and dispatch facts, and malformed
  metadata is rejected by both the JSON Schema and the typed reader.

## Done

The envelope's schema and consumer tests cover the new facts, and validation
passes.
