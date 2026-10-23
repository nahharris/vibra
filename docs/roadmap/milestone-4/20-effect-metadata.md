# Step 20 — effect metadata

Prerequisite: Step 19 merged. Stage 4B behavior step: the performed, enclosing, and
target rows at a source position, the effect-operation witnesses, and the effect
fields of the index. A library projection, as in M3; the public `query` command stays
M6's.

## Read before editing

- [Tooling](../../spec/05-tooling.md): **Workspace queries**, **Index records**,
  **M2 workspace position envelope** (the type object, the application fact, and the
  **effect fact**).
- [Effects](../../spec/03-effects.md): **Query metadata**, **The witness**,
  **Function effect rows**.
- [Runtime](../../spec/06-runtime.md): the `fn` encoding with `effects: R`.
- [Ledger](decision-ledger.md) rows D25.1, D28.1, D23.2.

## Scope

1. The workspace position envelope gains the wrapper field `effects` with
   `enclosing`, `application`, `target`, and `witnesses` (only under
   `--expand effect-witnesses`), as a `{status, value}` fact.
2. The function type object gains `effects` (a row record), in the schema and the
   producer.
3. The application fact of an effect-operation call: kind `@function`, callee the
   operation, and its complete row (the contradiction of the tooling chapter,
   fixed in Step 13).
4. `@index.v1` gains `ceiling`, the row form of `effects`, and `external`, in the
   VIBON document and in the JSON wire form; the effect roots and operations are
   indexed (`@effect-root` and `@operation` kinds carry facts instead of being
   unavailable).
5. One call graph: the position and index results are read from the checker's own
   result and never recomputed.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-schema/schemas/v1/workspace-position-query.json`, `index.json`, and `src/semantic_query.rs`, `src/index.rs` | The documents and schemas; the `effects` field becomes required together with its producer |
| `crates/vibra-workspace/src/query.rs`, `semantic.rs`, `index.rs` | The producers; `index.rs` writes `effects: (array)` today and treats `effect-root` and `operation` as unavailable |
| `crates/vibra-types/src/effects.rs` (Step 14) | The graph, the rows, and the witness chain the query reads |
| `conformance/cases/V1-TOOL-*` | The snapshots that change; `V1-TOOL-index-unavailable` is replaced by cases for the effect root and operation |
| `crates/vibra-schema/tests/` | Consumer tests for the new fields |

## Ordered tasks

1. Extend the position-query schema and documents with the type-object row and the
   `effects` fact; update every `V1-TOOL-workspace-position-*` snapshot and review
   that the only change is the new field.
2. Compute the fact from the checker's result: the enclosing function's ceiling and
   performed row, the application's complete row, the target's array when the source
   is reached from a binary's entry, and the witnesses for each performed root.
3. Extend the index producer and schema with `ceiling`, the row record, and
   `external`; give effect roots and operations their facts; update the
   `V1-TOOL-index-*` snapshots and the `fn` signatures (Step 14 already added the
   type row).
4. Replace `V1-TOOL-index-unavailable` with cases for a recovered declaration and
   for an effect root and its operations.
5. Producer and consumer tests for each document.

Invariants preserved: a query reads the snapshot's revision only; compact queries
omit witnesses; the checker and the query share one graph; an unavailable fact stays
`unavailable` and is never guessed.

## Test matrix

- Positive: a position in an operation body, in a caller, in a `lambda`, in a test,
  and in a binary's entry module; `--expand effect-witnesses` on the example of the
  effects chapter (the two-hop chain); the index of a module with a `deffect`.
- Negative: a position outside every function (`enclosing: null`, `witnesses`
  empty); a position in a source not reached from any entry (`target: null`); a
  malformed document rejected by the schema (an unknown field in a row).
- Recovery: a recovered declaration keeps `external` and omits `ceiling`; a broken
  neighbour does not change an exact `effects` fact elsewhere.
- Boundary: a row of many roots sorted by identity; a witness of many hops; a
  position at EOF.
- Formatter: none.

## Diagnostic and schema changes

The workspace-position and index schemas change in place (major version 1 is kept,
and the earlier shapes are not preserved); the consumer tests change with them.

## Validation

The [pre-merge list](validation.md#before-merging-each-step) and `cargo test
--locked --offline -p vibra-schema -p vibra-workspace -p vibra-conformance`.

## Excluded

The public `query` command and MCP (M6), edit plans, row variables.

## Completion evidence

The exit clause on effect metadata: performed, enclosing, and target rows at a source
position and the witness of each root, through the library projection.
