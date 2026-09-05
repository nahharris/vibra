# Step 10 — structural source-position facts

Prerequisite: Steps 8–9 and the metadata contract decision in
[implementation.md](implementation.md). Read tooling **Workspace queries** and its
source-position requirements, plus diagnostics span/recovery rules.

## Implementation sequence

1. Define a small syntax-owned query result containing containing-node span,
   grammar category, permitted child forms/labels, and fact availability.
   Derive these from the contextual AST tables, not a second syntax parser.
   Result types and exact wire fields need contract review before publication.
2. Validate byte offsets without slicing inside a UTF-8 scalar or panicking.
   Find the smallest containing node. Specify/test how EOF, trivia, zero-width
   recovery nodes, and ties select a node before relying on those choices.
3. Use ancestors to determine the slot: type, pattern, expression, declaration
   attribute, effect row, or data field. Distinguish lexically similar atoms
   whose grammar slots assign different roles. Do not resolve atom paths.
4. Report structural facts for incomplete files where justified. Mark facts
   recovered or unavailable explicitly; an empty list of labels is not a
   substitute for unavailable knowledge. Keep neighboring valid nodes exact.
5. Adapt results in `vibra-schema`, with producer serialization and independent
   schema consumer tests. Record any deliberate architecture dependency change
   in the README and architecture test. Do not make syntax depend on serde JSON
   wire types, CLI commands, a filesystem workspace service, or MCP.
6. Extend the corpus observation contract for structural results if required;
   the current manifest has no dedicated structural-query expectation. Update
   manifest validation, real handler, comparison logic, and synthetic harness
   tests together. Do not smuggle structural facts into `resolved` identities.

## Required matrix and completion

Query the head, operand, label, type, pattern, comment/trivia, and EOF of a
small valid document. Repeat with Unicode before the queried position and a
recovered neighbor. Exercise negative/out-of-range offsets through the chosen
public API, interior UTF-8 offsets, zero-width missing children, and nested
nodes. Assert both the selected span and the exact allowed-category/label set.
Check deterministic ordering and producer/consumer schema agreement.

No names-in-scope, inferred types, resolved application kinds, inferred effects,
or canonical identities are invented in M1. Later milestones fill those facts.
Run [validation](validation.md), adding schema contract tests and real corpus
coverage for this surface. Shipping a library and schema is completion of this
step; shipping a query CLI or MCP server is not part of M1.
