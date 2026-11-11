# Milestone 1 — Content-addressed codebase

An agent can open a checked codebase revision, find a definition by
name, and retrieve that exact version by content address after a restart.

Planning draft: [execution sequence and design proposals](execution.md),
[Step 1 contract review](01-contracts.md), and
[validation and handoff](validation.md). These documents propose the work;
they do not approve new contracts or claim implementation support.

## Proposed execution sequence

All steps are `not started`. The proposed integration branch is `v0.0.2/m1`;
it has not been created by this planning change. Follow the
[milestone execution model](../../execution.md) for one PR per step and a
standing draft PR into `main`.

| Step | Slice | Status | Guide |
| --- | --- | --- | --- |
| 1 | Close identity, persistence, and import/export contracts | not started | [Contract review](01-contracts.md) |
| 2 | Encode and address a checked graph deterministically | not started | [Step 2](execution.md#step-2--checked-graph-addressing) |
| 3 | Import, publish, reopen, and retrieve a checked revision | not started | [Step 3](execution.md#step-3--first-durable-revision) |
| 4 | Resolve names and read revision-scoped relations | not started | [Step 4](execution.md#step-4--revision-scoped-reads) |
| 5 | Export exact source with destination revision checks | not started | [Step 5](execution.md#step-5--source-export) |
| 6 | Prove publication under contention and interruption | not started | [Step 6](execution.md#step-6--publication-failure-paths) |
| 7 | Verify integrity, version rejection, and index rebuilding | not started | [Step 7](execution.md#step-7--integrity-and-recovery) |
| 8 | Back up and restore an independently usable store | not started | [Step 8](execution.md#step-8--backup-and-restore) |
| 9 | Measure the local store and select journal settings | not started | [Step 9](execution.md#step-9--local-store-measurements) |
| 10 | Sweep the demo, exit gate, and M2 handoff | not started | [Step 10](execution.md#step-10--exit-evidence) |

The design proposals and gate-to-step mapping live in the execution plan.
Steps 2–9 need their contract-specific detailed implementation instructions
before execution; the linked guides currently define boundaries, invariants,
and evidence. Step 1 is the first actionable planning task.

## Specification prerequisite

Define the object model and the import/export boundary in the source,
programs-and-packages, tooling, and conformance chapters. Review the type and
effect chapters for identity and provenance assumptions. Review the charter's
persistent-data rule explicitly. The proposed portable records use canonical
VIBON. An embedded database would be their private physical container.

- Store immutable checked definition objects and their resolved dependency
  edges. A revision maps canonical entities to objects and records source
  origins. A named head selects one revision. Preserve canonical nominal
  identities for types, interfaces, implementations, and effects. Matching
  structure alone cannot merge two distinct nominal declarations.
- Hash versioned, domain-separated canonical payloads. Define how local
  binders are normalized, resolved references are pinned, and mutually
  recursive components are encoded and addressed deterministically. Freeze
  vectors for recursion, nominal references, changed dependency bodies,
  alpha-renamed local binders, and formatting-only changes. A changed
  dependency must be visible in the dependent checked graph.
- Keep the checked object hash, exact source attachment hash, codebase
  revision, and existing file workspace revision distinct. Preserve comments
  and spans in source attachments. Unchecked or recovered forms can be read
  and diagnosed in a draft but cannot be published as checked objects.
- In codebase mode, the published revision is authoritative. File projects
  remain an ordinary entry point. Import captures a file snapshot explicitly.
  Export materializes a selected revision through a revision-checked file
  plan. Define round-trip behavior, external edits, and conflict diagnostics.

## Local storage

Use SQLite as the preferred first implementation. Keep immutable objects,
source attachments, revision membership, and named heads in one project-local
database. Index declarations, source origins, references, and call edges by
revision. Derive those rows from the existing workspace facts so the database
can answer joins without introducing a second checker or resolution model.
SQLite row IDs, insertion order, pages, and database bytes stay outside the
hash and interchange contracts. [SQLite transaction model](https://sqlite.org/lang_transaction.html)

Check and stage outside the write transaction. Within one transaction, verify
the expected head, persist missing objects and index rows, and publish the
new head. Bound lock contention and expose a retryable busy outcome separately
from a stale head. Specify local filesystem support, corruption diagnostics,
schema upgrade and recovery, and a consistent way to back up the store.
Measure indexed lookups and publish latency under overlapping CLI and MCP
use before choosing journal settings. A blob directory with a separate index
would need its own coordination and crash recovery across both stores.
[SQLite WAL](https://sqlite.org/wal.html) · [SQLite backup API](https://www.sqlite.org/backup.html)

## Gates

- **Demo.** Import a multi-module project with mutually recursive definitions.
  Publish a second checked revision, reopen the store, and retrieve both
  versions and their resolved dependency edges by address.
- **Exit.** Independent imports produce the same object hashes. A trivia-only
  change alters its source attachment while preserving checked object hashes.
  An unchanged component deduplicates. A nominal identity mismatch cannot be
  resolved by a matching body. Export preserves source attachments and refuses
  stale destinations. At most one publisher can advance a given head.
  An interrupted transaction and corrupt or unknown-version inputs expose
  no partial revision. Rebuilt indexes agree with the workspace facts.
  A restore preserves head and object addresses.

Distributed sync, automatic merge, arbitrary database access through tools,
and history pruning are outside this milestone. A hash identifies an object
inside an authorized codebase and grants no access on its own.
