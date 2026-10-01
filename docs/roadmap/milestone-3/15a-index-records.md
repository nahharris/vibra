# Step 15a — index records

Prerequisite: Step 14c merged. Stage 3B tooling step, the first of two that
Step 15 is split into; type-aware position metadata is
[Step 15b](15b-query-metadata.md).

## Read before editing

- [Tooling](../../spec/05-tooling.md): **Workspace queries**, **Index
  records** (the `@index.v1` schema, D18.3).
- [Types](../../spec/02-type-system.md): **Interfaces and methods**
  (implementation identities).
- [Decision ledger](decision-ledger.md) rows D18.3 and D23.1.

## Scope

`vibra-workspace` projects the `@index.v1` document of a snapshot. It has
three parts:

- one declaration record per module and per declaration: identity, kind,
  module, owner, visibility, source span, checked signature, effects, error
  types, applications, and formatter-normalized text;
- one implementation record per `impl` block, keyed by receiver and applied
  interface, with its members keyed by the contract, target, and receiver
  triple;
- one reference record per resolved written name.

The document is canonical VIBON with the specified sort orders, byte-identical
across hosts for an identical snapshot. A declaration that does not check keeps
its record without the checked facts. The checker exposes what the records
need: the checked type of every header, and every `impl` block with its
receiver, applied interface, and written members.

The public `query` command stays outside M3, so the projection is a library
function with a corpus operation, `index` (D23.1).

## Test matrix

- Tooling cases with the full `@index.v1` document for a project exercising
  every declaration kind M3 checks, a two-target receiver (two block identities
  and two member identities), and a reference the resolver leaves to the
  checker; and for a project whose effect root, operation, and one function do
  not check.
- A host test that indexes the same sources in two orders and compares the
  bytes.
- Schema tests: a rendered index validates, round-trips, and reads as a
  consumer expects; a malformed one is rejected.

## Done

The index schema is covered in `vibra-schema` with a JSON Schema and consumer
tests, and validation passes.
