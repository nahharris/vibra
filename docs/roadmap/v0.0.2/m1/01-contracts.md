# Step 1 — Codebase contract prerequisite

Status: not started. This guide begins the design review; its proposals are
not accepted source, data, CLI, or schema contracts.

This is the specification-prerequisite exemption in the
[execution model](../../execution.md#decomposition-rule). No database,
encoder, public command, or language feature is implemented by this step.

## Read first

- [Charter](../../../spec/00-charter.md), especially persistent data,
  nominal declarations, deterministic behavior, and compatibility.
- [Source language](../../../spec/01-source-language.md), Reader,
  Declarations, Types/interfaces/methods, and Canonical format.
- [Type system](../../../spec/02-type-system.md), Nominal declarations,
  Namespaces and resolution, Generics, and Interfaces and methods.
- [Effects](../../../spec/03-effects.md), Nominal effects, Performed rows,
  Target consent, and Query metadata.
- [Programs and packages](../../../spec/04-programs-and-packages.md), VIBON,
  project snapshots, modules, trusted stdlib input, and tests.
- [Tooling](../../../spec/05-tooling.md), the workspace engine, command
  results, queries, edit plans, and authorization boundaries.
- [Diagnostics and conformance](../../../spec/07-diagnostics-and-conformance.md),
  the closed registry, corpus format, profiles, and required suites.
- [M1 scope](README.md) and [proposed design](execution.md).

## Contract decisions to close

Record each accepted decision with its canonical section, rejected
alternative, reason, rule/case coverage, and downstream owner. New observable
rules use MUST, SHOULD, and MAY under the charter. Do not freeze public
spellings in implementation tests before this review.

| ID | Decision | Required resolution |
| --- | --- | --- |
| C1 | Portable records and SQLite container | State explicitly whether SQLite is a permitted private container for canonical VIBON payloads; define inspect/interchange obligations and the distinction from `.vibon` files. Update the charter and data chapter together if wording changes. |
| C2 | Entity and nominal identity | Preserve all resolver provenance fields and type-keyed implementation identities. Define rename, package-version change, same-name/different-body, and same-body/different-nominal behavior. Audit the different effect identity spellings noted in M4 before freezing hash inputs; coordinate its M2 owner if the fix belongs there. |
| C3 | Checked-object granularity | Inventory every declaration/member/test kind, resolved edge kind, and trusted external/stdlib reference. Decide which are objects or owned parts and how reference closure includes type, interface dispatch, effect, initializer, and body dependencies. |
| C4 | Canonical encoding and addresses | Specify domains, version tags, exact bytes, algorithm and text spelling; encode every supported checked node, generic/local binder and literal. Define SCC ordering, internal references, component/member addresses, provenance, and collision/integrity behavior. |
| C5 | Four identities and source attachment | Distinguish checked object, exact source attachment, codebase revision, and file workspace revision. Specify full-document ownership, span units, comments, project/test attachments, source moves, malformed drafts, and source-to-object mapping. |
| C6 | Revisions and heads | Close membership ordering, target/project/trust input capture, revision digest and lineage, named-head grammar, initial creation, expected absence, same-content republish, and missing revision/entity/object outcomes. |
| C7 | File/codebase authority | Define explicit import/export and the selected mode, snapshot freshness, external edits, destination policy, preview/apply, creates/deletes, unrelated destination files, and complete round-trip expectations. |
| C8 | Multi-file export failure boundary | Specify stale-check timing, path confinement, rollback, interruption/restart recovery, and what other processes may observe. Decide the necessary journal/staging protocol or a narrower reviewed export contract before promising atomicity. |
| C9 | Store open, publish and readers | Close database placement, read-only open behavior, local filesystem support, transaction visibility, expected-head compare-and-publish, bounded busy handling, cancellation/interruption, and uncertain commit responses. |
| C10 | Integrity, versions and lifecycle | Specify validation boundaries, corruption outcomes, version rejection/upgrades, index rebuild authority, backup consistency, restore/replacement, and supported recovery without silent data loss. |
| C11 | Tool and authorization surface | Close one minimal M1 command/operation grammar, selectors, capability/write boundaries, JSON envelopes, payload limits, ordering/pagination, and process exits. A hash never grants access. General query and MCP remain M2. |
| C12 | Independent evidence | Allocate canonical rule IDs, profile/operation changes, reviewed payload/hash vectors, diagnostic registry entries with fixed levels/spans, JSON schemas, failure precedence, and actual corpus handler coverage. |

## Files and responsibilities

Modify affected `docs/spec/00-charter.md` through
`docs/spec/05-tooling.md` and `docs/spec/07-diagnostics-and-conformance.md`
together; chapters without a behavior change receive an explicit reviewed
invariant in the decision record. Review `docs/spec/06-runtime.md` only for
checked-input/runtime-identity boundaries; M1 changes no execution semantics.

Record accepted decisions and rationale in this milestone's `README.md`,
with a linked ledger if the table becomes too large. Update `execution.md`
and the release roadmap if a contract decision changes a scheduled gate.
Keep the release described as planned until exit evidence exists.

Use `crates/vibra-schema/schemas/v1/command-result.json` and its existing
serializers/tests as the baseline for machine output changes. Inspect
`crates/vibra-diagnostics/src/registry.rs`,
`crates/vibra-conformance/src/`, and `conformance/cases/` before assigning
diagnostics and operations. New schema/corpus file names follow the approved
formats, not invented names from this planning draft.

## Ordered review tasks

- [ ] Confirm the v0.0.1 exit and current branch baseline without disturbing
  untracked or uncommitted work.
- [ ] Compare C1–C12 with existing canonical rules and record conflicts before
  proposing changes; old future milestone assignments are not current scope.
- [ ] Review the graph/identity proposal first: C2–C6. Show worked recursive,
  nominal, local-binder, and changed-dependency examples, including exact
  candidate payloads and manually reviewed expected addresses.
- [ ] Review C1 and C7–C10 against those identities. State file-export and
  store-publication atomicity separately and explain each failure boundary.
- [ ] Review C11 and C12: close command grammar, failure/result precedence,
  registry levels/spans, JSON shapes, and the independent corpus operation.
- [ ] Update all affected normative chapters in one specification change;
  retain explicit exclusions and one canonical spelling for each identity.
- [ ] Add schema/registry evidence and contract fixtures that the existing
  harness can validate without advertising an unimplemented store. Distinguish
  reviewed vectors from passing execution coverage. Add executable corpus
  behavior alongside its real handler in the owning implementation step.
- [ ] Expand Steps 2–9 into detailed implementation instructions with exact
  files, approved types/payloads, test cases, commands, and failure outcomes.
  Reconcile every C1–C12 decision with the gate mapping before approval.
- [ ] Run the applicable [validation](validation.md), record what ran and
  what remains review-only, and submit the contract prerequisite for approval.

## Required example and failure matrix

| Area | Positive example | Negative, boundary, or recovery example |
| --- | --- | --- |
| Binder normalization | Rename a parameter and all its local uses | Shadowed binders must retain different binding positions; labels retain their names |
| Recursion | Self-call and two mutually recursive functions imported in different traversal orders | Repoint one member's resolved dependency; addresses must reflect the change |
| Nominality | Same entity and same admitted graph across two imports | Equal bodies under two nominal identities cannot substitute for each other |
| Graph closure | Change a callee body and observe affected dependents | Keep an unrelated component unchanged; distinguish reference and call edges |
| Source | Add a comment between top-level declarations | Checked hashes stay fixed, source hash changes, spans remain tied to exact source |
| Versions and provenance | Reload exact trusted stdlib/project inputs | Reject changed trust evidence, unsupported payload version, or malformed record |
| Publication | Two complete revisions survive reopen | Same expected head races, interrupted transaction, busy writer, uncertain response |
| Export | Round trip source, project data, tests, and comments | External edit, added/deleted file, stale plan, escaping junction, interrupted apply |
| Read authority | Read an old object in an explicitly selected authorized store | Known hash from another store confers no access; no fallback to current head |
| Recovery | Rebuild indexes and restore an independent backup | Corrupt object, partial backup, interrupted restore, valid operation after a rejected input |

For every failure, specify result precedence, fixed diagnostic level, primary
and related spans or their explicit absence, and whether persistent bytes may
change. Formatter expectations cover canonical record ordering, LF/newline
rules, literal encoding, and exact-source export without implicit formatting.

## Completion evidence

Step 1 is complete only with approved C1–C12 decisions, affected canonical
prose, schema/registry evidence, reviewed vectors, a complete gate mapping,
and detailed downstream guides. It does not demonstrate a working codebase.
Do not mark any later step landed or enable a CLI capability from this step.
