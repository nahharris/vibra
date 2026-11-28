# v0.0.2 M3: identity and repair loop

**Planned.** The services an agent needs to inspect, change, and retest a
project, over ordinary source files.

The design is in the [architecture](architecture.md). What the plan assumes
where the specification is silent is in the
[specification questions](spec-questions.md). Each step has a guide under
[steps/](steps/) that is the whole task of the agent that implements it.

## Scope

- **Snapshots and identity.** Source files and pinned snapshots are the
  authority. Each declaration has its anchor identity; its claim (signature,
  contracts, effect row) and its witness (body) are hashed separately. A
  content-addressed store is at most a derived cache that can be deleted.
- **Protected evidence.** Test bodies, fixtures, and the runner and policy
  versions are pinned for each run and distinguished from the code under
  repair. An approval names what it approved; it is void when the test, the
  policy, or the claim changes. A command presents a target's authority in
  plain terms before a run, so an approval names the authority it grants.
- **Queries.** Read-only, bounded, versioned: declaration, references, type,
  contracts, effect row, grant requirements, and diagnostics at a position
  or entity. Results name the snapshot they describe.
- **Bounded evaluation.** Evaluate a pure expression against a snapshot
  under stated limits. Exhaustion is an incomplete outcome, not a failure of
  the program.
- **Edits.** Preview a file edit against a named snapshot revision and apply
  it atomically. A preview shows identity-affecting changes, such as a
  rename. A stale revision is rejected.
- **Structured test results.** Failures carry spans, expected and actual
  encodings, and the audit trace.
- **Generated references.** The syntax card, checked examples, and grammar
  and schema data are generated from the implementation and checked against
  it.

One workspace engine serves the CLI. The command names start from the
existing hierarchy; `query` and `edit` become available.

## Exit evidence

- The repair scenario of the [release gate](../README.md#release-gate), from
  a defect seeded by someone other than the author of the repair run.
- Rejections: stale edit, replaced test, changed policy, changed claim under
  an old approval.
- Every machine output validates against its schema.

## Not in this milestone

An authoritative code database, a publication workflow, persistent sessions
with live redefinition, MCP, and editor integrations.

## Design decisions

Each is recorded with its reason in the [architecture](architecture.md);
the list here is the index.

- No new crate; one new edge, `vibra-workspace` → `vibra-fs-scope`, so the
  edit engine writes through the held workspace directory with the
  guarantees of the M2 drivers (architecture, section 1).
- `vibra-ir`, `vibra-host`, the native-track crates, and `stdlib/` are not
  changed by M3, and `vibra-interp` changes in one file for the test
  entry's work limit, so M4 and fix package E may run beside it (section 1).
- Every shared file gets its seam in step 01; after it no M3 step owns a
  file another M3 step owns while both may run (section 1).
- Identities hash canonical content through the formatter's token stream
  and promise the equivalences of that normal form alone; the witness graph
  joins the resolver's references and the checker's checked edges; the
  revision alone hashes exact bytes (section 2, I1 to I4;
  [Q5](spec-questions.md#q5-which-references-make-a-witness-dependency),
  [Q6](spec-questions.md#q6-what-each-hash-survives)).
- An approval is one `@approval.v2` document per target under `approvals/`,
  protecting the `@tests` unit whole, written by `approve --write` and
  admitted at every `check` and every test run; its identity is pinned
  outside the tree by `--expect-approval` and named in every envelope
  ([Q7](spec-questions.md#q7-the-approval-document-and-who-writes-it),
  [Q8](spec-questions.md#q8-the-command-that-presents-authority),
  [Q40](spec-questions.md#q40-the-approval-pin-and---expect-approval)).
- The repair edit is `edit body`, a typed replacement of one body that
  changes no claim by construction
  ([Q1](spec-questions.md#q1-no-supported-edit-changes-a-body)).
- One document is published by exclusive creation under a reserved name,
  flush, and rename over the destination relative to the held directory; a
  plan over several documents is refused before writing; the commit point
  takes the workspace write lock and validates the whole read set
  ([Q15](spec-questions.md#q15-several-documents-cannot-be-published-atomically-on-windows),
  [Q17](spec-questions.md#q17-what-edit-may-write-and-what-confines-it));
  `--write` is unavailable where no scoped adapter exists
  ([Q16](spec-questions.md#q16-applying-an-edit-off-windows)).
- Every service has a work and memory limit and every command an output
  bound with a frozen exhausted envelope; none has a time limit
  ([Q26](spec-questions.md#q26-the-limits-of-eval-and-their-names)
  to [Q29](spec-questions.md#q29-a-time-limit)).
- The repair scenario's freeze depends on the focused lanes only (queries,
  the body edit and the publisher, approvals, test limits, and their
  gates); the breadth commands run beside it and are required by the exit
  (section 2, I25).

## Step table

A step is `planned`, `in progress`, or `landed`. The status column is edited
on the integration branch when a step merges, never on a step's own branch.
"Can run in parallel with" lists the steps that are neither before nor after
this one in the dependency graph; no two of them own one file. Steps 11 and
18 also depend on fix package E (`m2/fix-e`) having merged.

| Id | Name | Kind | Depends on | Can run in parallel with | Crates touched | Files owned | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 01 | [seams](steps/01-seams.md) | infrastructure | — | 02 | `vibra-workspace`, `vibra-types`, `vibra-syntax`, `vibra-fmt`, `vibra-cli`, `vibra-schema`, `vibra-conformance`, `conformance/rules` | [list](steps/01-seams.md#files-owned) | planned |
| 02 | [publish-primitive](steps/02-publish-primitive.md) | infrastructure | — | 01, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 17, 18, 23, 24, 25, 31, 32 | `vibra-fs-scope` | [list](steps/02-publish-primitive.md#files-owned) | planned |
| 03 | [canonical-text](steps/03-canonical-text.md) | vertical | 01 | 02, 05, 06, 07, 08, 11, 12, 17, 18, 23, 24, 31, 32 | `vibra-fmt` | [list](steps/03-canonical-text.md#files-owned) | planned |
| 04 | [claim-identity](steps/04-claim-identity.md) | vertical | 03 | 02, 05, 06, 07, 08, 09, 11, 12, 17, 18, 23, 24, 31, 32 | `vibra-workspace`, `conformance/cases` | [list](steps/04-claim-identity.md#files-owned) | planned |
| 05 | [query-subjects](steps/05-query-subjects.md) | vertical | 01 | 02, 03, 04, 07, 08, 09, 10, 11, 13, 14, 15, 16, 18, 19, 20, 21, 22, 27, 28, 30, 31, 32 | `vibra-workspace`, `vibra-cli`, `vibra-schema`, `vibra-conformance`, `conformance/cases`, `conformance/tools` | [list](steps/05-query-subjects.md#files-owned) | planned |
| 06 | [structural-position](steps/06-structural-position.md) | vertical | 05 | 02, 03, 04, 07, 08, 09, 10, 11, 13, 14, 15, 16, 18, 19, 20, 21, 22, 27, 28, 30, 31, 32 | `vibra-syntax`, `vibra-workspace`, `vibra-schema`, `conformance/cases` | [list](steps/06-structural-position.md#files-owned) | planned |
| 07 | [edit-plan-data](steps/07-edit-plan-data.md) | infrastructure | 01 | 02, 03, 04, 05, 06, 08, 09, 10, 11, 12, 14, 17, 18, 23, 24, 25, 31, 32 | `vibra-workspace`, `vibra-schema` | [list](steps/07-edit-plan-data.md#files-owned) | planned |
| 08 | [semantic-facts](steps/08-semantic-facts.md) | vertical | 01 | 02, 03, 04, 05, 06, 07, 11, 18, 31, 32 | `vibra-types` | [list](steps/08-semantic-facts.md#files-owned) | planned |
| 09 | [witness-identity](steps/09-witness-identity.md) | vertical | 03, 08 | 02, 04, 05, 06, 07, 11, 12, 17, 18, 23, 24, 31, 32 | `vibra-workspace`, `conformance/cases` | [list](steps/09-witness-identity.md#files-owned) | planned |
| 10 | [approvals](steps/10-approvals.md) | vertical | 04, 09 | 02, 05, 06, 07, 11, 12, 17, 18, 23, 24, 25, 31, 32 | `vibra-workspace`, `conformance/cases` | [list](steps/10-approvals.md#files-owned) | planned |
| 11 | [eval](steps/11-eval.md) | vertical | 01 | 02, 03, 04, 05, 06, 07, 08, 09, 10, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 35, 36, 37, 38, 39 | `vibra-workspace`, `vibra-cli`, `vibra-schema`, `vibra-conformance`, `conformance/cases` | [list](steps/11-eval.md#files-owned) | planned |
| 12 | [semantic-envelope](steps/12-semantic-envelope.md) | vertical | 05, 06, 08 | 02, 03, 04, 07, 09, 10, 11, 13, 14, 15, 16, 18, 19, 20, 21, 22, 27, 28, 30, 31, 32 | `vibra-workspace`, `vibra-schema`, `conformance/cases` | [list](steps/12-semantic-envelope.md#files-owned) | planned |
| 13 | [edit-body](steps/13-edit-body.md) | vertical | 04, 07, 09, 10 | 02, 05, 06, 11, 12, 14, 17, 18, 23, 24, 25, 31, 32 | `vibra-workspace`, `vibra-cli`, `vibra-conformance`, `conformance/cases` | [list](steps/13-edit-body.md#files-owned) | planned |
| 14 | [present](steps/14-present.md) | vertical | 10 | 02, 05, 06, 07, 11, 12, 13, 15, 17, 18, 21, 22, 23, 24, 25, 26, 27, 28, 30, 31, 32 | `vibra-workspace`, `vibra-cli`, `vibra-schema`, `conformance/cases` | [list](steps/14-present.md#files-owned) | planned |
| 15 | [edit-apply](steps/15-edit-apply.md) | vertical | 02, 13 | 05, 06, 11, 12, 14, 17, 18, 23, 24, 25, 31, 32 | `vibra-workspace`, `vibra-cli`, `vibra-conformance` | [list](steps/15-edit-apply.md#files-owned) | planned |
| 16 | [approve-write](steps/16-approve-write.md) | vertical | 14, 15 | 05, 06, 11, 12, 17, 18, 21, 22, 23, 24, 25, 26, 27, 28, 30, 31, 32 | `vibra-workspace`, `vibra-cli` | [list](steps/16-approve-write.md#files-owned) | planned |
| 17 | [effects-projection](steps/17-effects-projection.md) | vertical | 12 | 02, 03, 04, 07, 09, 10, 11, 13, 14, 15, 16, 18, 19, 20, 21, 22, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34 | `vibra-workspace`, `vibra-schema`, `conformance/cases` | [list](steps/17-effects-projection.md#files-owned) | planned |
| 18 | [test-limits](steps/18-test-limits.md) | vertical | 01 | 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16, 17, 19, 20, 21, 22, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34 | `vibra-interp`, `vibra-workspace`, `vibra-cli`, `vibra-schema`, `conformance/cases` | [list](steps/18-test-limits.md#files-owned) | planned |
| 19 | [review-identity](steps/19-review-identity.md) | evidence | 16 | 05, 06, 11, 12, 17, 18, 21, 22, 23, 24, 25, 26, 27, 28, 30, 31, 32 | none | [list](steps/19-review-identity.md#files-owned) | planned |
| 20 | [fix-identity](steps/20-fix-identity.md) | evidence | 19 | 05, 06, 11, 12, 17, 18, 21, 22, 23, 24, 25, 26, 27, 28, 30, 31, 32 | `vibra-fmt`, `vibra-workspace`, `vibra-cli`, `conformance/cases` | [list](steps/20-fix-identity.md#files-owned) | planned |
| 21 | [review-edits](steps/21-review-edits.md) | evidence | 15 | 05, 06, 11, 12, 14, 16, 17, 18, 19, 20, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34 | none | [list](steps/21-review-edits.md#files-owned) | planned |
| 22 | [fix-edits](steps/22-fix-edits.md) | evidence | 21 | 05, 06, 11, 12, 14, 16, 17, 18, 19, 20, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34 | `vibra-fs-scope`, `vibra-workspace`, `vibra-cli`, `vibra-schema`, `vibra-conformance`, `conformance/cases` | [list](steps/22-fix-edits.md#files-owned) | planned |
| 23 | [review-queries](steps/23-review-queries.md) | evidence | 17, 18 | 02, 03, 04, 07, 09, 10, 11, 13, 14, 15, 16, 19, 20, 21, 22, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34 | none | [list](steps/23-review-queries.md#files-owned) | planned |
| 24 | [fix-queries](steps/24-fix-queries.md) | evidence | 23 | 02, 03, 04, 07, 09, 10, 11, 13, 14, 15, 16, 19, 20, 21, 22, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34 | `vibra-syntax`, `vibra-types`, `vibra-interp`, `vibra-workspace`, `vibra-cli`, `vibra-schema`, `vibra-conformance`, `conformance/cases` | [list](steps/24-fix-queries.md#files-owned) | planned |
| 25 | [expansions](steps/25-expansions.md) | vertical | 04, 09, 12 | 02, 07, 10, 11, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 27, 28, 30, 31, 32, 35, 36, 37, 38, 39 | `vibra-workspace`, `vibra-schema`, `conformance/cases` | [list](steps/25-expansions.md#files-owned) | planned |
| 26 | [edit-rename](steps/26-edit-rename.md) | vertical | 15, 25 | 11, 14, 16, 17, 18, 19, 20, 21, 22, 23, 24, 27, 28, 29, 30, 31, 32, 35, 36, 37, 38, 39 | `vibra-workspace`, `vibra-cli`, `conformance/cases` | [list](steps/26-edit-rename.md#files-owned) | planned |
| 27 | [edit-imports](steps/27-edit-imports.md) | vertical | 15 | 05, 06, 11, 12, 14, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 28, 29, 30, 31, 32, 35, 36, 37, 38, 39 | `vibra-workspace`, `vibra-cli`, `conformance/cases` | [list](steps/27-edit-imports.md#files-owned) | planned |
| 28 | [edit-fix](steps/28-edit-fix.md) | vertical | 15 | 05, 06, 11, 12, 14, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 29, 31, 32, 35, 36, 37, 38, 39 | `vibra-diagnostics`, `vibra-workspace`, `vibra-cli`, `conformance/cases` | [list](steps/28-edit-fix.md#files-owned) | planned |
| 29 | [index](steps/29-index.md) | vertical | 20, 25 | 11, 17, 18, 21, 22, 23, 24, 26, 27, 28, 30, 31, 32, 35, 36, 37, 38, 39 | `vibra-workspace`, `vibra-schema`, `conformance/cases` | [list](steps/29-index.md#files-owned) | planned |
| 30 | [lint](steps/30-lint.md) | vertical | 28 | 05, 06, 11, 12, 14, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 29, 31, 32, 35, 36, 37, 38, 39 | `vibra-workspace`, `vibra-cli`, `vibra-schema`, `conformance/cases` | [list](steps/30-lint.md#files-owned) | planned |
| 31 | [reference-data](steps/31-reference-data.md) | infrastructure | 01 | 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 35, 36, 37, 38, 39 | `vibra-conformance`, `docs/reference` | [list](steps/31-reference-data.md#files-owned) | planned |
| 32 | [syntax-card](steps/32-syntax-card.md) | vertical | 31 | 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 35, 36, 37, 38, 39 | `vibra-conformance`, `docs/reference` | [list](steps/32-syntax-card.md#files-owned) | planned |
| 33 | [review-breadth](steps/33-review-breadth.md) | evidence | 11, 26, 27, 29, 30, 32 | 17, 18, 21, 22, 23, 24, 35, 36, 37, 38, 39 | none | [list](steps/33-review-breadth.md#files-owned) | planned |
| 34 | [fix-breadth](steps/34-fix-breadth.md) | evidence | 33 | 17, 18, 21, 22, 23, 24, 35, 36, 37, 38, 39 | `vibra-diagnostics`, `vibra-workspace`, `vibra-cli`, `vibra-schema`, `vibra-conformance`, `conformance/cases`, `docs/reference` | [list](steps/34-fix-breadth.md#files-owned) | planned |
| 35 | [repair-freeze](steps/35-repair-freeze.md) | evidence | 20, 22, 24 | 11, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34 | none | [list](steps/35-repair-freeze.md#files-owned) | planned |
| 36 | [repair-references](steps/36-repair-references.md) | evidence | 35 | 11, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34 | `examples` | [list](steps/36-repair-references.md#files-owned) | planned |
| 37 | [repair-seeding](steps/37-repair-seeding.md) | evidence | 36 | 11, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34 | `examples` | [list](steps/37-repair-seeding.md#files-owned) | planned |
| 38 | [repair-run](steps/38-repair-run.md) | evidence | 37 | 11, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34 | none | [list](steps/38-repair-run.md#files-owned) | planned |
| 39 | [repair-record](steps/39-repair-record.md) | evidence | 38 | 11, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34 | none | [list](steps/39-repair-record.md#files-owned) | planned |
| 40 | [m3-exit](steps/40-m3-exit.md) | evidence | 34, 39 | — | `vibra-conformance`, `vibra-cli`, `conformance/cases` | [list](steps/40-m3-exit.md#files-owned) | planned |

### How the steps run

Three lanes, and at most three implementation steps at a time. The three
focused lanes lead to the freeze of the repair scenario; the breadth steps
run beside them and are required by the exit.

- **Identity and approvals lane:** canonical text, claims, witnesses,
  approvals, the presentation of authority, the approval writer, the
  identity gate and its fixes. Steps 03, 04, 09, 10, 14, 16, 19, 20.
- **Query lane:** subjects, the structural record, the checker's facts,
  the semantic envelope, effects, the queries gate and its fixes, with the
  test limits beside it. Steps 05, 06, 08, 12, 17, 18, 23, 24.
- **Edit lane:** the publish primitive, plan data, the body edit, the
  publisher, the edit gate and its fixes. Steps 02, 07, 13, 15, 21, 22.
- **Breadth, beside the lanes:** evaluation, expansions, rename, imports,
  fix, the index, lint, the reference data and the card, and their gate
  and fixes. Steps 11, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34.
- **The scenario and the exit:** steps 35 to 40.

Steps 01 and 02 share no file and start together. From step 03 to step 34
the graph is at least three wide: after step 01 seven steps are ready at
once (03, 05, 07, 08, 11, 18, 31), and the three lanes stay independent,
with the breadth steps filling a lane whenever one waits. The graph narrows
where it must: each review gate (19, 21, 23, 33) reads a merged tree, and
the fix step after it owns the files the findings can touch, so nothing
that shares those files runs beside it (the table shows what still may).
The freeze (35) waits for the three focused fix steps (20, 22, 24) and for
nothing else; the index (29) waits for the identity fixes, since it shows
the identities. Steps 35 to 39 are serial: a registered protocol is frozen,
then referenced, seeded, run, and recorded. The exit (40) waits for the
record and for the breadth fixes (34).

A suggested order that respects the table, one triple per line:

| Identity and approvals | Queries | Edits and breadth |
| --- | --- | --- |
| 01 | — | 02 |
| 03 | 05 | 07 |
| 08 | 06 | 11 |
| 04 | 12 | 18 |
| 09 | 17 | 31 |
| 10 | 23 | 32 |
| 13 | 14 | 25 |
| 15 | 24 | 27 |
| 16 | 21 | 28 |
| 19 | 22 | 26 |
| 20 | 30 | — |
| 29 | 35 | — |
| 33 | 36 | — |
| 34 | 37 | — |
| 38, 39, 40 | — | — |

## Scope and steps

| Scope item | Steps |
| --- | --- |
| Snapshots and identity | 03, 04, 09 (identities); 29 (the index shows them); gate 19, 20 |
| Protected evidence | 10 (approvals, the protected set, the pin, test-run admission), 14 (the presentation of authority), 16 (the approval writer), 13 and 15 (the edit engine's refusals); gate 19, 20 |
| Queries | 05, 06, 08, 12, 17, 25, 29; gate 23, 24, 33, 34 |
| Bounded evaluation | 11; gate 33, 34 |
| Edits | 02, 07, 13, 15, 26, 27, 28, 30; gate 21, 22, 33, 34 |
| Structured test results | 18; gate 23, 24 |
| Generated references | 31, 32; gate 33, 34 |
| One workspace engine for the CLI; `query` and `edit` available | 01 (the seams and the output bound), 05, 13, and every command step |
| The repair scenario and the exit | 35, 36, 37, 38, 39, 40 |

| Step | Scope item or purpose |
| --- | --- |
| 01 | the seams of every shared file, the output bound and the exhausted envelope, the case operations |
| 02 | edits: the publish primitive of the scoped adapter |
| 03 | identity: canonical source text |
| 04 | identity: claims |
| 05 | queries: subjects and the `query` command |
| 06 | queries: the structural record |
| 07 | edits: the plan data and schema |
| 08 | queries: the checker's facts, checked edges, body verdicts |
| 09 | identity: witnesses |
| 10 | protected evidence: approvals, the pin, test-run admission |
| 11 | bounded evaluation |
| 12 | queries: the semantic envelope |
| 13 | edits: `edit body`, the repair edit |
| 14 | protected evidence: `approve`, the presentation of authority |
| 15 | edits: the publisher, the commit point, confinement |
| 16 | protected evidence: `approve --write` |
| 17 | queries: effects and witnesses |
| 18 | structured test results and the test work limit |
| 19, 20 | the review gate of identity and protected evidence and its fixes |
| 21, 22 | the review gate of the focused edit lane and its fixes |
| 23, 24 | the review gate of the focused queries and the test limits, and its fixes |
| 25 | queries: expansions |
| 26 | edits: `edit rename` |
| 27 | edits: `edit imports` |
| 28 | edits: `edit fix` |
| 29 | identity and queries: the index |
| 30 | edits: `lint` |
| 31 | generated references: the data and the generator |
| 32 | generated references: the card and the checked examples |
| 33, 34 | the review gate of evaluation, the breadth edits, the index, and the references, and its fixes |
| 35 to 39 | the repair scenario: freeze, references and controls, seeding, run, record |
| 40 | the exit record |

## Exit clauses

| Exit clause | Steps that produce it | Recorded by |
| --- | --- | --- |
| the repair scenario of the release gate, from a defect seeded by someone other than the author of the repair run | 35 (the registration), 36 (the references and controls), 37 (the seed), 38 (the run), 39 (the record); the mechanisms it uses: 02, 05, 06, 08, 10, 12, 13, 14, 15, 16, 17, 18 | 39, 40 |
| rejected: a stale edit | 15 (`ATK-edit-apply-expect-revision`, `ATK-edit-apply-stale-document`, `ATK-edit-apply-unedited-input-changed`), 36 (the control) | 39, 40 |
| rejected: a replaced test | 10 (`V2-GRANT-approval-stale-test`, `ATK-approval-expected-replaced`), 13 (`V2-TOOL-edit-body-test-denied`), 36 | 39, 40 |
| rejected: a changed policy | 10 (`V2-GRANT-approval-unapproved-binding`), 36 | 39, 40 |
| rejected: a changed claim under an old approval | 10 (`V2-GRANT-approval-stale-claim`), 13 (`V2-TOOL-edit-body-claims-unchanged`, since a body edit cannot change a claim), 36 | 39, 40 |
| every machine output validates against its schema | 01 (the bound and the exhausted envelope), 05, 07, 11, 14, 18, 29, 30 (each payload's schema), 40 (`ATK-schema-every-ending`) | 40 |

## Plan validation

The plan is checked by a script over the guides, the architecture, the
questions, the owner table, the README, and the tree. It reports:

- 40 guides, each with every required section;
- every dependency points to an earlier step, so the graph is acyclic;
- 368 pairs of steps can run in parallel, and no pair owns one file;
- every diagnostic and result code cited is in the registry of the
  diagnostics chapter or the result tables of the tooling chapter, except
  `@project.staleApproval` and `@tool.invalidPlan`, which
  [Q9](spec-questions.md#q9-the-code-of-a-void-approval) and
  [Q42](spec-questions.md#q42-a-code-for-an-invalid-plan) propose and the
  script reports as proposed;
- every `path:line` of an entry-point table exists and holds the symbol
  named beside it;
- every package named with `-p` is known to `cargo metadata --offline`,
  and no guide runs the test suite of the whole workspace;
- every case and attack id is a row of exactly one guide and names no
  case that the corpus already has;
- each of the 28 invariants of the identity and evidence argument has at
  least one attack case, and each such id is a row of the step the table
  names;
- every case id that a question binds is a row of a guide;
- every M3 row of the owner table names ids a guide defines;
- every step is named by a scope or exit row above;
- every relative link and its anchor resolve.
