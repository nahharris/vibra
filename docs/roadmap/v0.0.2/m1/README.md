# Milestone 1 — Content-addressed codebase

M1 imports a checked local project into a durable codebase, publishes safe immutable revisions, and lets an authorized reader find a definition by canonical identity and retrieve its exact checked version after a restart.

**Status:** active on integration branch <code>v0.0.2/m1</code>. C1–C12 decisions are approved. Local implementation, schema/registry updates, independent conformance scenarios, reviewed address vectors, measurements, and evidence are recorded in [M1 evidence](evidence.md). Delivery remains in progress until the cumulative draft merges with green CI. No release support claim is made before the milestone is merged.

This run uses focused commits and one cumulative draft PR into <code>main</code>, as explicitly requested for the full M1 delivery. That is the delivery exception to the repository's usual one-PR-per-step model. A step may have complete local evidence while the cumulative PR is open, but it is not landed or complete until that PR merges.

## Execution sequence

| Step | Slice | Status | Guide |
| --- | --- | --- | --- |
| 1 | Freeze identity, persistence, import/export, schemas, and evidence contracts | in progress | [Approved contract ledger](01-contracts.md) |
| 2 | Encode and address a checked graph deterministically | in progress | [Step 2](execution.md#step-2--checked-graph-addressing) |
| 3 | Import, publish, reopen, and retrieve a checked revision | in progress | [Step 3](execution.md#step-3--first-durable-revision) |
| 4 | Resolve names and read revision-scoped relations | in progress | [Step 4](execution.md#step-4--revision-scoped-reads) |
| 5 | Export exact source into an absent destination | in progress | [Step 5](execution.md#step-5--source-export) |
| 6 | Prove publication under contention and interruption | in progress | [Step 6](execution.md#step-6--publication-failure-paths) |
| 7 | Verify integrity, version rejection, and index rebuilding | in progress | [Step 7](execution.md#step-7--integrity-and-recovery) |
| 8 | Back up and restore an independently usable store | in progress | [Step 8](execution.md#step-8--backup-and-restore) |
| 9 | Measure the local store and select journal settings | in progress | [Step 9](execution.md#step-9--local-store-measurements) |
| 10 | Sweep the demo, exit gate, and M2 handoff | in progress | [Step 10](execution.md#step-10--exit-evidence) |

## Approved design

The workspace engine projects an immutable checked graph from its existing snapshot, resolver, and checker. The workspace-owned implementation is split into the pure <code>workspace::codebase::graph</code>, the SQLite <code>store</code>, and the explicit <code>export</code> boundary. Graph encoding performs no I/O or resolution. Persistence validates canonical records; workspace recovery replays exact attachments through the existing engine to verify and rebuild derived facts. Neither service defines a second checker or resolver.

The store is one project-local SQLite file at <code>.vibra/codebase.sqlite</code> by default. It contains exact canonical VIBON payloads and private derived indexes. Domain-separated SHA-256 addresses cover canonical checked-component records, source wrappers, and complete revision manifests. Checked objects, entity identities, raw source hashes, source-attachment addresses, codebase revisions, and file-workspace revisions are distinct.

Objects are top-level declarations, tests, and explicit implementation blocks. Nested members are owned parts with their own entity mappings and part paths. References include resolved types, interfaces, effects, values, calls, initializers, and implementation contracts. SCCs sort members by complete canonical key and encode internal references by member index. Named heads publish a revision with expected-absence or expected-address compare-and-publish. A busy store is retryable; a stale head is not silently rebased.

Import is explicit. File project commands remain file-authoritative. Codebase reads pin one head or explicit revision and never refresh from files. Export writes exact stored project and source attachments to an absent destination only. It previews a canonical plan and applies it with an exclusive no-replace directory publication. M1 does not overwrite, delete, update, or merge into an existing destination tree.

See [the accepted contract ledger](01-contracts.md) and the normative [program data](../../../spec/04-programs-and-packages.md#m1-addressed-codebase-data) and [tooling](../../../spec/05-tooling.md#m1-codebase-commands) chapters for the complete schemas, grammar, outcomes, and boundaries.

Local validation and the remaining integration evidence are tracked in [M1 evidence](evidence.md).

## Gates and exclusions

**Demo:** Import a multi-module project with mutually recursive definitions, publish a second checked revision, reopen the store, and retrieve both versions and their resolved dependency edges by address.

**Exit:** Independent imports and reordered traversal produce the same object addresses; alpha-renaming and trivia-only edits preserve checked objects while exact source addresses change; dependent addresses change with dependency bodies; unrelated components deduplicate; nominal identities remain distinct; old and new revisions read independently; stale/busy CAS behavior is distinct; export preserves exact source and refuses a destination created after preview; interrupted publication exposes no partial revision; verify/reindex and backup/restore preserve authoritative addresses.

No distributed sync, automatic merge, pruning, arbitrary SQL, general query CLI, MCP, LSP, semantic editing, REPL, ordinary dependency delivery, Wasm build products, native code generation, or optimization promise enters M1. A content address identifies an object only within an authorized opened codebase and grants no access by itself.
