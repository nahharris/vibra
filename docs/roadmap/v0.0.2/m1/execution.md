# v0.0.2 M1 execution plan

Status: planning draft; design and contract approval pending.

**Goal:** Import a checked local project into a durable content-addressed
codebase, publish a second revision safely, and retrieve either revision's
definitions and dependencies after a restart.

**Architecture proposal:** The workspace service projects immutable records
from the existing resolver and checker. A storage boundary persists canonical
VIBON payloads and derived indexes in a project-local SQLite database. CLI
operations adapt that service; later MCP and LSP adapters use the same service.

**Technology proposal:** Existing Rust workspace, shared VIBON reader and
formatter, SHA-256, and SQLite. Select the Rust SQLite binding and native
build configuration during the contract review; no new dependency is selected
by this draft.

For execution, use the repository's [milestone execution model](../../execution.md)
and the executing-plans skill task by task. This draft authorizes no code work.
Detailed code-level tasks follow the approved Step 1 contracts rather than
inventing APIs or schema spellings in advance.

## Starting point

The inspected planning base is `c81a2836` on local `main`. The working tree
also contains untracked `examples/m4-demo/target/`; preserve it. Before
execution, refresh `origin/main`, verify the preceding milestone's actual
merge and [exit evidence](../../v0.0.1/m4/exit-evidence.md), and establish a
fresh baseline. No remote branch, PR, or test result is asserted by this plan.

The release's newer scope supersedes old v0.0.1 notes assigning dependency
delivery, build products, or optimization to v0.0.2 M1–M3. M1 admits one
offline package, multiple local modules and targets, tests, and the trusted
embedded stdlib. Ordinary dependency delivery stays deferred.

## Approaches considered

| Approach | Benefit | Cost or limitation |
| --- | --- | --- |
| Definition graph with one SQLite container; recommended | Fits the M1 addressed-definition gate and keeps objects, indexes, and head changes within one transaction | Requires a precise graph encoding and a reviewed VIBON/container boundary |
| Whole-project snapshot objects | Simpler initial serialization | Does not satisfy definition addressing and component deduplication without the graph model as well |
| Immutable blob directory with a separate index | Objects are individually inspectable files | Adds coordination, crash recovery, and backup across multiple stores |

The roadmap already prefers SQLite. Step 1 should confirm the container
interpretation against the charter explicitly, then preserve a portable
record format independent of the database layout.

## Proposed design decisions for review

These are recommendations, not normative rules. Step 1 records accepted
decisions in the affected specification chapters and the milestone README.

1. **Keep entity identity and object identity separate.** Preserve package
   provenance, unit, module, owner, kind, and name from the resolver. A
   definition hash pins one checked version of that entity; equal bodies do
   not collapse distinct nominal declarations. Do not add UUID-based nominal
   identity or rename-stable identity in M1.
2. **Address the resolved checked graph.** Encode language semantics rather
   than Rust memory layouts or the existing diagnostic/index snapshots.
   Normalize lexical binders by scope position; retain observable names such
   as labels and record fields. Pin external references to dependency object
   addresses. Group cycles as strongly connected components with members
   ordered by canonical entity identity; represent internal references by
   that deterministic member order. Specify component and member address
   derivation before freezing vectors.
3. **Use versioned canonical payloads and domain separation.** Recommend
   SHA-256 over a specified domain/version prefix and canonical VIBON bytes.
   Do not include insertion order, database IDs, absolute checkout paths,
   timestamps, source spans, or runtime arena IDs in checked object identity.
   Different dependency bodies must change the affected graph's addresses.
4. **Retain complete source documents.** Attach exact UTF-8 module bytes and
   project data to revisions, with declaration spans into those documents.
   Hash exact bytes separately from normalized checked payloads. This retains
   inter-declaration comments and whitespace and makes source-preserving
   export possible. Define how unchecked drafts are held without publishing
   them as checked objects.
5. **Make a revision a complete admitted snapshot.** Its contract includes
   entity-to-object membership, source attachments/origins, project/target
   data, and trusted input provenance. A named head selects a revision.
   Close whether lineage is part of revision identity in Step 1. Do not reuse
   the existing file workspace revision as a codebase revision.
6. **Check before acquiring the writer transaction.** Publication verifies
   the expected head and writes all required objects, sources, membership,
   and indexes with the new head in one transaction. Stale and busy are
   distinct outcomes. No step may temporarily expose partial publication.
7. **Derive indexes from the workspace facts.** Persist revision-scoped
   declarations, origins, references, and calls; rebuilding them must reproduce
   the same observations. Keep SQL and row IDs private to the storage boundary.
8. **Keep import and export explicit.** File projects remain usable through
   existing commands. Codebase reads use a selected published revision and
   never silently refresh from files. Export previews a destination-checked
   file plan. The single-file formatter is a precedent for confinement and
   stale checks, not proof of a multi-file transaction.

## Code boundaries to qualify

Existing entry points below were inspected. New file names are proposals.

| Existing entry point | Responsibility in M1 |
| --- | --- |
| `crates/vibra-workspace/src/lib.rs`, `snapshot.rs`, `source_graph.rs` | Capture admitted immutable inputs and preserve the explicit acquisition boundary |
| `crates/vibra-resolve/src/lib.rs` (`DeclarationId`, `ResolvedSnapshot`) | Supply canonical identity and resolved references without filesystem access |
| `crates/vibra-workspace/src/semantic.rs` (`check_all`, `WorkspaceCheckResult`) | Admit checked graphs; extend immutable checked access if projection needs it |
| `crates/vibra-ir/src/` | Supply checked semantic structures; keep backend and store handles out |
| `crates/vibra-workspace/src/index.rs` | Reuse declaration, source, reference, and checked effect facts |
| `crates/vibra-workspace/src/format_plan.rs`, `confined_fs.rs` | Reuse confinement/revision principles; review multi-file export separately |
| `crates/vibra-cli/src/lib.rs` | Adapt the approved M1 grammar and result envelopes |
| `crates/vibra-schema/`, `crates/vibra-diagnostics/src/registry.rs` | Own versioned machine contracts and closed diagnostics |
| `crates/vibra-conformance/src/`, `conformance/cases/` | Exercise real operations through independently authored expectations |

Propose a pure graph-encoding unit, a SQLite storage unit, and workspace-owned
import/export orchestration. Step 1 decides module versus crate placement
after checking dependency direction. The pure encoder takes immutable checked
inputs and performs no file, database, clock, random, or network acquisition.
The storage unit cannot resolve or check language constructs.

## Step guides

Step 1 closes contracts. For each later behavior slice, first expand its guide
with exact approved schemas, diagnostics, case IDs, file placement, focused
commands, and ordered test/implementation tasks. Implementation cannot begin
from an outline with those prerequisites still open.

### Step 1 — Contract prerequisite

Use [01-contracts.md](01-contracts.md). This is an explicit specification
prerequisite exemption. It defines behavior and evidence obligations, not
implemented support. Complete its review before Step 2.

### Step 2 — Checked graph addressing

Prerequisite: approved object model, canonical encoding, identity and
provenance rules, and frozen expected vectors from Step 1.

Project checked inputs through the pure encoding unit and exercise it through
a real conformance operation. Cover functions, values, nominal declarations,
interfaces, implementations and members, effects and operations, test bodies,
and trusted stdlib references. The contract decides each object's granularity.
Encode reference and call edges without assuming those are the same relation.

Evidence: independent imports and reordered traversal yield the same
addresses; local alpha-renaming and trivia preserve checked identity;
shadowing remains distinct; label/field and semantic literal changes remain
visible. Self-recursion and mutual recursion terminate deterministically.
A dependency body change invalidates affected dependents while an unrelated
component deduplicates. Nominal identities remain distinct. Refuse malformed,
unavailable, or partially checked graphs. Canonical VIBON is idempotent and
fixed vectors are independently reviewed rather than generated and blessed.

### Step 3 — First durable revision

Prerequisite: Step 2 and the approved local-store/open/publish contract.

Add the SQLite storage boundary and workspace import orchestration, with the
approved narrow CLI operations and JSON schema. Capture and check one
multi-module, multi-target local package and its embedded stdlib provenance;
stage objects before the transaction, atomically publish, close, reopen, and
retrieve definitions and resolved edges by address.

Evidence: the recursive demo works across process restart; source attachments
and project metadata survive exactly; repeated import deduplicates; invalid
input creates no published revision. Expected-head checks, bounded busy
handling, confinement, and rollback are already required here. This step
does not defer its safety to Step 6. Run existing file-project commands as
regressions and validate successful and failed machine responses.

### Step 4 — Revision-scoped reads

Prerequisite: Step 3 and approved read selectors/authorization boundaries.

Add name-to-object lookup within a pinned revision, retained-revision reads,
and derived revision-scoped declaration/origin/reference/call indexes.
Use `index.rs` and checked facts; do not add a second resolution model.
This slice is the M1 codebase read surface; general `query` and MCP stay M2.

Evidence: old and new versions resolve independently after a second publish;
unknown head, revision, entity, and address have their specified outcomes;
cross-revision edges cannot silently resolve through the current head.
Traversal is bounded and ordered. A known hash grants no authority outside
the selected authorized codebase. Schema-check all results.

### Step 5 — Source export

Prerequisite: Step 4 and an approved multi-file publication and recovery
policy for export. Resolve the atomicity boundary before coding it.

Materialize the selected revision as an explicit file plan, preview its
changes, then revalidate the expected destination immediately before apply.
Preserve exact attachments, tests, project data, comments, and spans. Treat
untracked destination files, creates/deletes, external edits, and source-root
changes according to the Step 1 policy. Close rollback and restart recovery
for failures between replacements; a loop of single-file renames alone
cannot claim a multi-file atomic export.

Evidence: import/export/import retains the approved identities; trivia-only
changes preserve checked addresses and change source attachment addresses;
stale, escaping, symlink/junction, malformed, or conflicting plans make no
unauthorized changes. Exercise partial-write failure and recovery. Export
does not format source implicitly; explicit formatting remains idempotent.

### Step 6 — Publication failure paths

Prerequisite: Steps 3–5 and approved fault/lock outcome contracts.

Extend the already-safe publication path with deterministic process-race,
lock contention, and interruption tests. Two processes publish different
candidates with the same expected head; only one may advance it. Distinguish
retryable busy from stale without a silent retry against a different base.

Evidence: inject failures before transaction start, after object/index writes,
before head update, before commit, and after commit before response. Reopen
and observe either the prior complete state or the committed complete state.
Define how a caller resolves an uncertain response without duplicate history
or incorrectly reporting rollback. Readers never see another writer's draft.

### Step 7 — Integrity and recovery

Prerequisite: Step 6 and approved validation/version/recovery contracts.

Implement integrity inspection and derived-index rebuilding through the
workspace service. Verify payload addresses, schemas, membership closure,
source origins, and provenance at the boundaries Step 1 requires. Refuse
unsupported versions without opportunistic migration. Define behavior for a
missing object, tampered payload, invalid source attachment, malformed record,
and damaged database. Never repair authoritative data from untrusted indexes.

Evidence: corrupt and unknown-version stores publish nothing; rejected
operations preserve original bytes as required by the contract; rebuilt
indexes equal fresh workspace facts; valid work can resume after a supported
recovery. Include an interrupted rebuild and an invalid store followed by an
independent valid operation. Record schema-upgrade policy even if version 1
offers rejection and explicit export/import only.

### Step 8 — Backup and restore

Prerequisite: Step 7 and approved backup destination/replacement policy.

Back up a consistent store and restore into a confined destination. Keep
backup format distinct from portable record interchange; a backup may contain
SQLite's private physical representation only under the approved container
rule. Never assume copying the open main database file alone is sufficient.

Evidence: overlap backup with publication and restore one complete committed
snapshot; compare heads, revisions, object addresses, sources, and indexes.
Reject corrupt input, unsupported versions, unsafe destinations, and a stale
overwrite. Interrupt backup and restore and verify the specified recovery
outcome without damaging the source store.

### Step 9 — Local-store measurements

Prerequisite: Step 8; a reviewed repeatable workload and supported filesystem
matrix. This is measurement/configuration work, not an optimization promise.

Measure cold reopen, name lookup, addressed lookup, relation reads, first and
deduplicating import, publish latency, store size, and overlapping reader/writer
contention. Run on Windows first and record host, filesystem, SQLite/binding
versions, workload size, and repetitions. Represent the later CLI/MCP overlap
as concurrent service clients; do not implement MCP in M1.

Select journal and lock settings from the measurements and rerun Step 6 and
Step 8 against the selected settings. Document supported local filesystems
and any excluded network/synchronized-drive scenarios explicitly. Record
results in canonical VIBON plus readable prose; do not persist JSON metrics.

### Step 10 — Exit evidence

Prerequisite: Steps 1–9 landed with green CI. This is an evidence exemption.

Run the multi-module recursive demo with two revisions and process restart.
Sweep the mapping below, both independent suites, schema checks, and existing
interpreter/Wasm parity. Record base and tested commits, exact commands and
exit codes, counts, fixture locations, PR merges, and limitations. Hand M2 the
approved revision selectors, publication boundary, addresses, source origins,
and error contracts. Update release/support claims only with this evidence.

## Gate coverage

| M1 obligation | Owning steps |
| --- | --- |
| Object model, canonical nominal identities, recursive graph addressing | 1, 2 |
| Independent imports, alpha-renaming/trivia vectors, dependency invalidation, deduplication | 1, 2, 3, 5 |
| Open revision, find by name, read exact old/new definition and edges after restart | 3, 4, 10 |
| Separate checked/source/codebase/file identities; retain exact source and draft boundaries | 1, 2, 3, 5 |
| Explicit file import/export, round trip, external edits and stale destinations | 1, 3, 5 |
| At most one publisher per expected head, bounded contention, interrupted transactions | 3, 6 |
| Derived indexes agree with workspace facts and can be rebuilt | 4, 7 |
| Corrupt and unknown-version inputs expose no partial revision | 3, 6, 7 |
| Local filesystem, schema policy, recovery, backup and restore | 1, 7, 8, 9 |
| Lookup/publish measurements before journal selection | 9 |
| Authorization boundary and no hash-based access grant | 1, 3, 4, 5, 8 |
| Host tests, independent corpus, schemas, and source-format regressions | Every behavior step; sweep in 10 |

## Exclusions

No distributed sync, merge, pruning, arbitrary SQL tools, ordinary dependency
delivery, public general query command, MCP server, semantic editing, REPL,
LSP, runtime redefinition, Wasm build products, native code generation, or
optimization enters this milestone. M1 provides the services M2 and M3 need;
it does not implement their adapters or behavior early.
