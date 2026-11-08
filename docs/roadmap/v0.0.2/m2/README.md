# Milestone 2 — Queries, edits, formatting, and lint

An agent can understand a definition, propose a structured change,
and publish it with evidence that the proposed revision checks.

## Delivery order

1. Expose the existing query and position facts through CLI and read-only
   MCP. Accept a source position, canonical entity, or addressed definition in
   a pinned revision. Return source, definition location, type, expected type,
   performed and allowed effects with witnesses, token roles, diagnostics,
   references, calls, and structured S-expression nodes with source origins.
   Distinguish exact, recovered, and unavailable facts.
   Bound and deterministically order relation expansions.
2. Start with replacing one declaration from Vibra forms. Then add typed
   plans to insert or remove a declaration, replace a selected expression,
   rename across modules, and organize imports. Identify a node by its role
   and address within the selected definition and base revision. Check the
   replacement against the grammar and semantic context. Preview source and
   definition-address diffs, affected declarations, and check diagnostics.
   Publish uses the codebase transaction. File-backed writes use the
   workspace's revision-checked file plan. Both paths revalidate their
   postconditions and reject stale, ambiguous, or escaping changes.
3. Extend the canonical formatter into format plans. Start lint with unused
   imports and one semantic rule with a safe repair, such as a redundant
   ascription proven removable by rechecking. A lint finding includes its rule,
   severity, evidence spans, explanation, and action when available. Distinguish
   validated edits from suggestions requiring judgment. Use the same plan
   engine for code actions and fixes.

## Specification and gates

Close the subject grammar, typed operations, diagnostic registry, schema
versions, and postcondition policy before implementing them. Define whether a
plan may improve an invalid draft and what it must prove before publish.
Resolve the effect identity spelling discrepancy recorded in the
[M4 evidence](../../v0.0.1/m4/exit-evidence.md#open-items-for-later) before
exposing the query schema through CLI and MCP.
Reuse the one workspace engine and versioned JSON shapes. Read-only MCP can
inspect and preview without persistent writes or execution. Apply is explicit
and separately enabled. MCP tools accept typed data and run no shell.

- **Demo.** Resolve an application to its definition by hash, inspect its type
  and effect witnesses, preview a cross-module change and explained fix, then
  apply a fresh plan after rejecting a stale one.
- **Exit.** CLI and MCP facts and action results agree for the same revision.
  Plans preserve unrelated forms and comments. Invalid or conflicting plans
  produce no partial publish. Formatting is idempotent, and every validated
  fix satisfies its advertised postconditions. Test authorization, traversal
  and symlink/junction escapes, and schema validation on positive and negative
  cases.

The [editor milestone](../m5/README.md) consumes these facts and plans
through LSP. General AST patch languages and automatic edits on read are
outside this milestone.
