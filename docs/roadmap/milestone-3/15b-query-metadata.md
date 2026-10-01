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

## Test matrix

- Query cases at a constructor, a pattern, an `as` narrowing, an interface
  member call, and a destination-dispatched call.

## Done

The envelope's schema and consumer tests cover the new facts, and validation
passes.
