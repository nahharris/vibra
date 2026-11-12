# v0.0.2 M1 execution plan

Status: active; C1–C12 contracts are approved for implementation. Schema evidence, reviewed address vectors, implementation, measurements, and exit evidence remain in progress.

**Goal:** Import a checked local project into a durable content-addressed codebase, publish a second revision safely, and retrieve either revision's definitions and dependencies after a restart.

**Architecture contract:** The workspace service projects an immutable checked graph from its existing snapshot, resolver, and checker. Workspace modules <code>codebase::graph</code>, <code>codebase::store</code>, and <code>codebase::export</code> own pure graph encoding, SQLite persistence, and explicit export orchestration. CLI operations adapt that service; later adapters reuse it without adding another resolver or checker.

**Technology contract:** Existing Rust workspace, shared VIBON reader and formatter, SHA-256, and SQLite through the selected bundled Rust binding with backup support. Native configuration and supported local-filesystem publication primitives are fixed by the owning implementation guide.

For execution, use the repository's [milestone execution model](../../execution.md) and the executing-plans skill task by task. The approved specification chapters define APIs and schemas; implementation proceeds against them. This milestone run produces one cumulative draft PR into main with focused commits, as explicitly requested for the full M1 delivery. This is the delivery exception to the usual one-PR-per-step model; completed but unmerged steps remain in progress until merge.
## Starting point

The approved base is c81a2836 on main; origin/main has been verified at that same commit. The integration branch v0.0.2/m1 is active. Preserve all unrelated untracked and uncommitted work. The earlier planning tree and its untracked examples/m4-demo/target/ are not implementation inputs.

The release scope admits one offline package, multiple local modules and targets, tests, and the trusted embedded stdlib. Ordinary dependency delivery stays deferred. The user's request for one cumulative draft PR for the full milestone is recorded above; completed but unmerged steps remain in progress until that PR merges.
## Approaches considered

| Approach | Benefit | Cost or limitation |
| --- | --- | --- |
| Definition graph with one SQLite container; recommended | Fits the M1 addressed-definition gate and keeps objects, indexes, and head changes within one transaction | Requires a precise graph encoding and a reviewed VIBON/container boundary |
| Whole-project snapshot objects | Simpler initial serialization | Does not satisfy definition addressing and component deduplication without the graph model as well |
| Immutable blob directory with a separate index | Objects are individually inspectable files | Adds coordination, crash recovery, and backup across multiple stores |

Step 1 resolved the SQLite exception with the charter: authoritative payloads are canonical VIBON records in a private SQLite physical container, while database bytes and layout remain private. Canonical VIBON record schemas are independent of the database layout.

## Approved design decisions

The normative decisions are recorded in [01-contracts.md](01-contracts.md) and the charter, type/effect, programs-and-packages, codebase-encoding, and tooling chapters.

1. **Entity and object identity are separate.** Use the resolver's full canonical identity, including package name/version, unit, module, owner path, and name; the structured entity key also carries its kind. Do not add UUID or rename-stable identity. Nominal identities never merge by body equality.
2. **Objects own complete declarations and checked parts.** There is one object per top-level declaration, test, and explicit implementation block. Nested members map to their owner's address plus part path. An implementation block key includes both canonical receiver and applied interface; each member retains its contract/receiver/interface triple.
3. **Canonical records address the checked graph.** Use the closed <code>@checked-definition.v1</code> tree and <code>@checked-component.v1</code> frame. Normalize local/type binders by introduction order; replace confirmed resolved uses with canonical identities. SCC members sort by complete key; internal edges use member indices and external edges pin object addresses or trusted registry fingerprints.
4. **Domains and source are fixed.** SHA-256 addresses use the exact NUL-terminated domains and canonical VIBON bytes in the programs-and-packages chapter. Source bytes use an <code>@source.v1</code> wrapper; raw byte hash and wrapper address are distinct. Spans and origins are separate from checked identity.
5. **Revisions contain complete inputs.** The sorted revision manifest includes package/project attachment, entity mappings, source IDs/origins, target/trusted provenance, and every admitted object. Parent lineage and timestamps are excluded. Identical complete snapshots deduplicate.
6. **Publication is explicit compare-and-publish.** Stage and check before writing; compare expected absence or exact revision inside one transaction; write objects, attachments, membership, indexes, and head together. Busy and stale are different outcomes; no silent retry or rebase.
7. **Indexes are derived.** Persist revision-scoped declaration, origin, reference, and call projections from immutable workspace facts. Verify does not repair authoritative records; reindex only derives indexes from verified authority.
8. **Import and export cross the mode boundary explicitly.** File projects stay file-authoritative. Codebase reads pin one authorized head or revision and never refresh from files. Export is new-destination-only, previews a canonical plan, and publishes the complete tree with an exclusive same-filesystem no-replace operation.
9. **SQLite stays private.** The default store is <code>.vibra/codebase.sqlite</code>; read-only open never creates the directory, schema, journal, or store. Use SQLite's consistent backup facility. Restore and backup destinations must be absent.
10. **M1 public surface and exclusions are closed.** The CLI has init, head, import, lookup, read, export, verify, reindex, backup, and restore. There is no general query, MCP, LSP, REPL, ordinary dependency delivery, arbitrary SQL, merge, or pruning in M1.
11. **Local path ownership is a precondition.** M1 rejects link traversal and uses exclusive publication in a trusted local workspace. It does not defend against an untrusted local principal concurrently rebinding path ancestors; shared writable or privileged operations over untrusted directories are excluded. A descriptor-relative SQLite VFS would be a separate requirement before claiming that stronger boundary.
## Code boundaries to qualify

Existing entry points below were inspected. The implementation boundary is workspace-owned and adds no new crate.

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

Use workspace modules <code>codebase::graph</code>, <code>codebase::store</code>, and <code>codebase::export</code>. The pure graph encoder takes the immutable checked projection and performs no file, database, clock, random, or network acquisition. Storage cannot resolve or check language constructs. Export owns confinement, preview-plan verification, and exclusive absent-destination publication.

## Step guides

Step 1 approved the canonical contracts. Its evidence work continues with the JSON schema, diagnostic registry, independently authored codebase scenario corpus, reviewed vectors, and detailed guides. Each behavior slice owns focused host and conformance evidence; implementation cannot claim its gate from an outline or schema alone.

### Step 1 — Contract prerequisite

Status: approved design; implementation and independent evidence remain in progress. This step fixes observable behavior and record schemas. It does not claim shipped support or complete the host/conformance evidence. The normative contracts are in [01-contracts.md](01-contracts.md), the charter, source, type, effects, programs-and-packages, tooling, and diagnostics-and-conformance chapters. Runtime semantics do not change in M1.
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

Prerequisite: Step 4 and the absent-destination export policy in the tooling chapter.

Materialize the selected revision as an explicit file plan for an absent destination. Preview the canonical plan address and ordered file list; apply requires that exact address, rechecks confinement and continued absence, stages the complete tree beside the destination, flushes it, then publishes with an exclusive same-filesystem no-replace directory operation. Preserve exact project/source/test attachments, comments, and spans. Existing destinations, untracked files, creates/deletes, or an external destination created after preview are refused; M1 never modifies an existing tree. Interruption before publication leaves the destination absent; after publication it is complete. Never use a check-then-overwrite rename.

Evidence: import/export/import retains the approved identities; trivia-only changes preserve checked addresses and change source attachment addresses; stale plans, a destination created after preview, escaping paths, symlinks/junctions, malformed inputs, and unsupported no-replace publication make no unauthorized destination changes. Exercise interruption before and after the exclusive directory publish. Export does not format source implicitly; explicit formatting remains idempotent.
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

Prerequisite: Step 7 and the approved absent-destination backup/restore policy.

Back up a consistent store using SQLite's backup facility. Restore only a verified backup into a confined absent destination. A backup is a physical store copy, not portable VIBON record interchange. Backup and restore stage beside their destinations, flush data, and publish with exclusive no-replace file creation. Neither command overwrites an existing file.

Evidence: overlap a backup with publication and restore a complete committed snapshot; compare heads, revisions, object addresses, sources, and indexes. Reject corrupt input, unsupported versions, unsafe destinations, and existing destinations. Interrupt backup and restore and verify the specified outcome without damaging the source store.
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

Prerequisite: Steps 1–9 have complete reviewed local evidence for the cumulative
draft PR. The milestone gate remains in progress until that PR merges with
green CI. This step is an evidence exemption.

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

No distributed sync, automatic merge, pruning, arbitrary SQL tools, ordinary dependency delivery, export into an existing directory, public general query command, MCP server, LSP, semantic editing, REPL, runtime redefinition, Wasm build products, native code generation, or optimization enters this milestone. M1 provides the revision selectors, publication boundary, addresses, source origins, and error contracts needed by later milestones; it does not implement their adapters or behavior early.
