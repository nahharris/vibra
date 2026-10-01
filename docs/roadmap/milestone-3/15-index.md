# Step 15 — index records and type-aware query metadata

Prerequisite: Step 14c merged. Stage 3B tooling step.

## Read before editing

- [Tooling](../../spec/05-tooling.md): **Workspace queries**, **Index
  records** (the `@index.v1` schema, D18.3).
- [Types](../../spec/02-type-system.md): **Interfaces and methods**
  (implementation identities).

## Scope

The `@workspace` projection `--expand declarations --include index` emits the
`@index.v1` document. It has three parts:

- one declaration record per declaration: identity, kind, module, owner,
  visibility, source span, checked signature, effects, error types,
  applications, and formatter-normalized text;
- one implementation record per `impl` block, keyed by receiver and applied
  interface, with its members keyed by the contract, target, and receiver
  triple;
- one reference record per resolved written name.

Source-position queries gain type-aware metadata for the Stage 3A and 3B forms:
constructor and pattern shapes, union members, interface members and bounds,
and the destination a dispatched member resolved to. Both outputs are
canonical VIBON with the specified sort orders, byte-identical across hosts
for an identical snapshot.

## Test matrix

- Tooling cases with the full `@index.v1` document for a project exercising
  every declaration kind, a two-target receiver (two block identities and two
  member identities), a recovered declaration, and a reference the resolver
  leaves to the checker.
- Query cases at a constructor, a pattern, an `as` narrowing, an interface
  member call, and a destination-dispatched call.
- A host test that indexes the same snapshot on two orderings of the source
  map and compares the bytes.

## Done

The index schema is covered in `vibra-schema` with a JSON Schema and consumer
tests, and validation passes.
