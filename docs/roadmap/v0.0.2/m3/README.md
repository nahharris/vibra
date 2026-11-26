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
- `vibra-ir`, `vibra-interp`, `vibra-host`, the native-track crates, and
  `stdlib/` are not changed by M3, so M4 and fix package E may run beside
  it (section 1).
- Every shared file gets its seam in step 01; after it no M3 step owns a
  file another M3 step owns while both may run (section 1).
- Identities hash canonical content through the formatter's token stream;
  the revision alone hashes exact bytes (section 2, I1 to I4).
- An approval is one `@approval.v2` document per target under `approvals/`,
  written by `approve --write` and validated at every `check`
  ([Q7](spec-questions.md#q7-the-approval-document-and-who-writes-it),
  [Q8](spec-questions.md#q8-the-command-that-presents-authority)).
- The repair edit is `edit replace`, a typed replacement of one declaration
  ([Q1](spec-questions.md#q1-no-supported-edit-changes-a-body)).
- Several files publish through a write-ahead journal, replayed by the next
  command ([Q15](spec-questions.md#q15-atomic-application-of-several-files));
  `--write` is unavailable where no scoped adapter exists
  ([Q16](spec-questions.md#q16-applying-an-edit-off-windows)).
- Every service has a work and memory limit and every command an output
  bound; none has a time limit ([Q26](spec-questions.md#q26-the-limits-of-eval-and-their-names)
  to [Q29](spec-questions.md#q29-a-time-limit)).

## Step table

A step is `planned`, `in progress`, or `landed`. The status column is edited
on the integration branch when a step merges, never on a step's own branch.
"Can run in parallel with" lists the steps that are neither before nor after
this one in the dependency graph; no two of them own one file.

| Id | Name | Kind | Depends on | Can run in parallel with | Crates touched | Files owned | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 01 | [seams](steps/01-seams.md) | infrastructure | — | — | `vibra-workspace`, `vibra-types`, `vibra-syntax`, `vibra-fmt`, `vibra-cli`, `vibra-schema`, `vibra-conformance`, `conformance/rules` | [list](steps/01-seams.md#files-owned) | planned |
| 02 | [canonical-text](steps/02-canonical-text.md) | vertical | 01 | 05, 06, 07, 08, 10, 12, 15, 16, 23, 25, 26 | `vibra-fmt` | [list](steps/02-canonical-text.md#files-owned) | planned |
| 03 | [claim-identity](steps/03-claim-identity.md) | vertical | 02 | 04, 05, 06, 07, 08, 10, 12, 15, 16, 23, 25, 26 | `vibra-workspace`, `conformance/cases` | [list](steps/03-claim-identity.md#files-owned) | planned |
| 04 | [witness-identity](steps/04-witness-identity.md) | vertical | 02 | 03, 05, 06, 07, 08, 10, 12, 15, 16, 23, 25, 26 | `vibra-workspace`, `conformance/cases` | [list](steps/04-witness-identity.md#files-owned) | planned |
| 05 | [query-subjects](steps/05-query-subjects.md) | vertical | 01 | 02, 03, 04, 06, 07, 08, 09, 10, 11, 13, 14, 17, 18, 20, 21, 23, 24, 25, 26 | `vibra-workspace`, `vibra-cli`, `vibra-schema`, `vibra-conformance`, `conformance/cases`, `conformance/tools` | [list](steps/05-query-subjects.md#files-owned) | planned |
| 06 | [structural-position](steps/06-structural-position.md) | vertical | 01 | 02, 03, 04, 05, 07, 08, 09, 10, 11, 13, 14, 17, 18, 20, 21, 23, 24, 25, 26 | `vibra-syntax`, `vibra-workspace`, `conformance/cases` | [list](steps/06-structural-position.md#files-owned) | planned |
| 07 | [edit-plan-data](steps/07-edit-plan-data.md) | infrastructure | 01 | 02, 03, 04, 05, 06, 08, 09, 10, 12, 13, 15, 16, 17, 18, 22, 23, 25, 26, 29, 30 | `vibra-workspace`, `vibra-schema` | [list](steps/07-edit-plan-data.md#files-owned) | planned |
| 08 | [semantic-facts](steps/08-semantic-facts.md) | vertical | 01 | 02, 03, 04, 05, 06, 07, 09, 10, 11, 13, 14, 17, 18, 20, 21, 23, 24, 25, 26 | `vibra-types` | [list](steps/08-semantic-facts.md#files-owned) | planned |
| 09 | [approvals](steps/09-approvals.md) | vertical | 03, 04 | 05, 06, 07, 08, 10, 11, 12, 14, 15, 16, 19, 20, 21, 23, 24, 25, 26, 27, 28 | `vibra-workspace`, `conformance/cases` | [list](steps/09-approvals.md#files-owned) | planned |
| 10 | [eval](steps/10-eval.md) | vertical | 01 | 02, 03, 04, 05, 06, 07, 08, 09, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28 | `vibra-workspace`, `vibra-cli`, `vibra-schema`, `vibra-conformance`, `conformance/cases` | [list](steps/10-eval.md#files-owned) | planned |
| 11 | [edit-replace](steps/11-edit-replace.md) | vertical | 03, 04, 07 | 05, 06, 08, 09, 10, 12, 13, 15, 16, 17, 18, 22, 23, 25, 26, 29, 30 | `vibra-workspace`, `vibra-cli`, `vibra-conformance`, `conformance/cases` | [list](steps/11-edit-replace.md#files-owned) | planned |
| 12 | [semantic-envelope](steps/12-semantic-envelope.md) | vertical | 05, 06, 08 | 02, 03, 04, 07, 09, 10, 11, 13, 14, 17, 18, 20, 21, 23, 24, 25, 26 | `vibra-workspace`, `conformance/cases` | [list](steps/12-semantic-envelope.md#files-owned) | planned |
| 13 | [present-and-approve](steps/13-present-and-approve.md) | vertical | 09 | 05, 06, 07, 08, 10, 11, 12, 14, 15, 16, 19, 20, 21, 23, 24, 25, 26, 27, 28 | `vibra-workspace`, `vibra-cli`, `vibra-schema`, `conformance/cases` | [list](steps/13-present-and-approve.md#files-owned) | planned |
| 14 | [edit-apply](steps/14-edit-apply.md) | vertical | 11 | 05, 06, 08, 09, 10, 12, 13, 15, 16, 17, 18, 22, 23, 25, 26, 29, 30 | `vibra-workspace`, `vibra-cli`, `vibra-conformance` | [list](steps/14-edit-apply.md#files-owned) | planned |
| 15 | [effects-projection](steps/15-effects-projection.md) | vertical | 12 | 02, 03, 04, 07, 09, 10, 11, 13, 14, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28 | `vibra-workspace`, `conformance/cases` | [list](steps/15-effects-projection.md#files-owned) | planned |
| 16 | [expansions](steps/16-expansions.md) | vertical | 12 | 02, 03, 04, 07, 09, 10, 11, 13, 14, 15, 17, 18, 20, 21, 23, 24, 25, 26 | `vibra-workspace`, `conformance/cases` | [list](steps/16-expansions.md#files-owned) | planned |
| 17 | [review-identity](steps/17-review-identity.md) | evidence | 13 | 05, 06, 07, 08, 10, 11, 12, 14, 15, 16, 19, 20, 21, 23, 24, 25, 26, 27, 28 | none | [list](steps/17-review-identity.md#files-owned) | planned |
| 18 | [fix-identity](steps/18-fix-identity.md) | evidence | 17 | 05, 06, 07, 08, 10, 11, 12, 14, 15, 16, 19, 20, 21, 23, 24, 25, 26, 27, 28 | `vibra-fmt`, `vibra-workspace`, `vibra-cli`, `conformance/cases` | [list](steps/18-fix-identity.md#files-owned) | planned |
| 19 | [edit-rename](steps/19-edit-rename.md) | vertical | 14, 16 | 09, 10, 13, 15, 17, 18, 20, 21, 22, 23, 24, 25, 26, 29, 30 | `vibra-workspace`, `vibra-cli`, `conformance/cases` | [list](steps/19-edit-rename.md#files-owned) | planned |
| 20 | [edit-imports](steps/20-edit-imports.md) | vertical | 14 | 05, 06, 08, 09, 10, 12, 13, 15, 16, 17, 18, 19, 21, 22, 23, 24, 25, 26, 29, 30 | `vibra-workspace`, `vibra-cli`, `conformance/cases` | [list](steps/20-edit-imports.md#files-owned) | planned |
| 21 | [edit-fix](steps/21-edit-fix.md) | vertical | 14 | 05, 06, 08, 09, 10, 12, 13, 15, 16, 17, 18, 19, 20, 22, 23, 25, 26, 29, 30 | `vibra-diagnostics`, `vibra-workspace`, `vibra-cli`, `conformance/cases` | [list](steps/21-edit-fix.md#files-owned) | planned |
| 22 | [index](steps/22-index.md) | vertical | 16, 18 | 07, 10, 11, 14, 15, 19, 20, 21, 23, 24, 25, 26, 27, 28 | `vibra-workspace`, `vibra-schema`, `conformance/cases` | [list](steps/22-index.md#files-owned) | planned |
| 23 | [test-limits](steps/23-test-limits.md) | vertical | 01 | 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 28 | `vibra-workspace`, `vibra-cli`, `vibra-schema`, `conformance/cases` | [list](steps/23-test-limits.md#files-owned) | planned |
| 24 | [lint](steps/24-lint.md) | vertical | 21 | 05, 06, 08, 09, 10, 12, 13, 15, 16, 17, 18, 19, 20, 22, 23, 25, 26, 29, 30 | `vibra-workspace`, `vibra-cli`, `vibra-schema`, `conformance/cases` | [list](steps/24-lint.md#files-owned) | planned |
| 25 | [reference-data](steps/25-reference-data.md) | infrastructure | 01 | 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 27, 28 | `vibra-conformance`, `docs/reference` | [list](steps/25-reference-data.md#files-owned) | planned |
| 26 | [syntax-card](steps/26-syntax-card.md) | vertical | 25 | 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 27, 28 | `vibra-conformance`, `docs/reference` | [list](steps/26-syntax-card.md#files-owned) | planned |
| 27 | [review-edits](steps/27-review-edits.md) | evidence | 19, 20, 21, 24 | 09, 10, 13, 15, 17, 18, 22, 23, 25, 26, 29, 30 | none | [list](steps/27-review-edits.md#files-owned) | planned |
| 28 | [fix-edits](steps/28-fix-edits.md) | evidence | 27 | 09, 10, 13, 15, 17, 18, 22, 23, 25, 26, 29, 30 | `vibra-workspace`, `vibra-cli`, `vibra-schema`, `vibra-conformance`, `conformance/cases` | [list](steps/28-fix-edits.md#files-owned) | planned |
| 29 | [review-queries-and-eval](steps/29-review-queries-and-eval.md) | evidence | 10, 15, 22, 23, 26 | 07, 11, 14, 19, 20, 21, 24, 27, 28 | none | [list](steps/29-review-queries-and-eval.md#files-owned) | planned |
| 30 | [fix-queries-and-eval](steps/30-fix-queries-and-eval.md) | evidence | 29 | 07, 11, 14, 19, 20, 21, 24, 27, 28 | `vibra-syntax`, `vibra-types`, `vibra-workspace`, `vibra-cli`, `vibra-schema`, `vibra-conformance`, `conformance/cases` | [list](steps/30-fix-queries-and-eval.md#files-owned) | planned |
| 31 | [repair-freeze](steps/31-repair-freeze.md) | evidence | 18, 28, 30 | — | none | [list](steps/31-repair-freeze.md#files-owned) | planned |
| 32 | [repair-references](steps/32-repair-references.md) | evidence | 31 | — | `examples` | [list](steps/32-repair-references.md#files-owned) | planned |
| 33 | [repair-seeding](steps/33-repair-seeding.md) | evidence | 32 | — | `examples` | [list](steps/33-repair-seeding.md#files-owned) | planned |
| 34 | [repair-run](steps/34-repair-run.md) | evidence | 33 | — | none | [list](steps/34-repair-run.md#files-owned) | planned |
| 35 | [repair-record](steps/35-repair-record.md) | evidence | 34 | — | none | [list](steps/35-repair-record.md#files-owned) | planned |
| 36 | [m3-exit](steps/36-m3-exit.md) | evidence | 35 | — | `vibra-conformance`, `vibra-cli`, `conformance/cases` | [list](steps/36-m3-exit.md#files-owned) | planned |

### How the steps run

Three lanes, and at most three implementation steps at a time.

- **Identity lane:** canonical text, claims, witnesses, approvals, the
  presentation of authority, the identity gate and its fixes, the index.
  Steps 02, 03, 04, 09, 13, 17, 18, 22.
- **Query lane:** subjects, the structural record, the checker's facts, the
  semantic envelope, effects, expansions. Steps 05, 06, 08, 12, 15, 16.
- **Edit lane:** plan data, replace, apply, rename, imports, fix, lint, the
  edit gate and its fixes. Steps 07, 11, 14, 19, 20, 21, 24, 27, 28.
- **Beside any lane:** evaluation, the test limits, the reference data and
  the card, which depend on step 01 alone. Steps 10, 23, 25, 26.

Step 01 runs alone. From step 02 to step 28 the graph is at least three
wide: after step 01 eight steps are ready at once (02, 05, 06, 07, 08, 10,
23, 25), and the three lanes stay independent, with the fourth group
filling a lane whenever one waits. The graph narrows where it must: the
three review gates (17, 27, 29) read a merged tree, and the fix step after
each owns the files the findings can touch, so nothing that shares those
files runs beside it (the table shows what still may). Step 22 waits for
the identity fixes, since the index shows the identities. The queries gate
(29) also waits for evaluation, the test limits, and the card, so its fix
step closes every service before the repair scenario. Steps 31 to 36 are
serial: a registered protocol is frozen, then referenced, seeded, run, and
recorded, and the exit reads the record.

A suggested order that respects the table, one triple per line:

| Identity lane | Query lane | Edit lane |
| --- | --- | --- |
| 01 | — | — |
| 02 | 05 | 07 |
| 03 | 06 | 10 |
| 04 | 08 | 11 |
| 09 | 12 | 14 |
| 13 | 15 | 23 |
| 17 | 16 | 25 |
| 18 | 26 | 19 |
| 22 | 20 | 21 |
| 29 | 24 | 27 |
| 30 | 28 | — |
| 31 to 36 | — | — |

## Scope and steps

| Scope item | Steps |
| --- | --- |
| Snapshots and identity | 02, 03, 04 (identities); 22 (the index shows them); gate 17, 18 |
| Protected evidence | 09 (approvals and the protected set), 13 (the presentation of authority and the approval writer), 11 and 14 (the edit engine's refusals); gate 17, 18 |
| Queries | 05, 06, 08, 12, 15, 16, 22; gate 29, 30 |
| Bounded evaluation | 10; gate 29, 30 |
| Edits | 07, 11, 14, 19, 20, 21, 24; gate 27, 28 |
| Structured test results | 23 |
| Generated references | 25, 26; gate 29, 30 |
| One workspace engine for the CLI; `query` and `edit` available | 01 (the seams and the output bound), 05, 11, and every command step |
| The repair scenario and the exit | 31, 32, 33, 34, 35, 36 |

| Step | Scope item or purpose |
| --- | --- |
| 01 | the seams of every shared file, the output bound, the case operations |
| 02 | identity: canonical source text |
| 03 | identity: claims |
| 04 | identity: witnesses |
| 05 | queries: subjects and the `query` command |
| 06 | queries: the structural record |
| 07 | edits: the plan data and schema |
| 08 | queries: the checker's facts |
| 09 | protected evidence: approvals |
| 10 | bounded evaluation |
| 11 | edits: `edit replace`, the repair edit |
| 12 | queries: the semantic envelope |
| 13 | protected evidence: `approve`, the presentation of authority |
| 14 | edits: atomic apply, the journal, confinement |
| 15 | queries: effects and witnesses |
| 16 | queries: expansions |
| 17, 18 | the review gate of identity and protected evidence and its fixes |
| 19 | edits: `edit rename` |
| 20 | edits: `edit imports` |
| 21 | edits: `edit fix` |
| 22 | identity and queries: the index |
| 23 | structured test results and the test work limit |
| 24 | edits: `lint` |
| 25 | generated references: the data and the generator |
| 26 | generated references: the card and the checked examples |
| 27, 28 | the review gate of edits and its fixes |
| 29, 30 | the review gate of queries, evaluation, limits, and references, and its fixes |
| 31 to 35 | the repair scenario: freeze, references and controls, seeding, run, record |
| 36 | the exit record |

## Exit clauses

| Exit clause | Steps that produce it | Recorded by |
| --- | --- | --- |
| the repair scenario of the release gate, from a defect seeded by someone other than the author of the repair run | 31 (the registration), 32 (the references and controls), 33 (the seed), 34 (the run), 35 (the record); the mechanisms it uses: 05, 10, 11, 12, 13, 14, 15, 23 | 35, 36 |
| rejected: a stale edit | 14 (`ATK-edit-apply-expect-revision`, `ATK-edit-apply-stale-document`), 32 (the control) | 35, 36 |
| rejected: a replaced test | 09 (`V2-GRANT-approval-stale-test`), 11 (`V2-TOOL-edit-replace-test-denied`), 32 | 35, 36 |
| rejected: a changed policy | 09 (`V2-GRANT-approval-unapproved-binding`), 32 | 35, 36 |
| rejected: a changed claim under an old approval | 09 (`V2-GRANT-approval-stale-claim`), 11 (`V2-TOOL-edit-replace-entry-claim-denied`), 32 | 35, 36 |
| every machine output validates against its schema | 01 (the bound), 05, 07, 10, 13, 22, 23, 24 (each payload's schema), 36 (`ATK-schema-every-ending`) | 36 |

## Plan validation

The plan is checked by a script over the guides, the architecture, the
questions, the owner table, the README, and the tree. It reports:

- 36 guides, each with every required section;
- every dependency points to an earlier step, so the graph is acyclic;
- 255 pairs of steps can run in parallel, and no pair owns one file;
- every diagnostic and result code cited is in the registry of the
  diagnostics chapter or the result tables of the tooling chapter, except
  `@project.staleApproval`, which [Q9](spec-questions.md#q9-the-code-of-a-void-approval)
  proposes and the script reports as proposed;
- every `path:line` of an entry-point table exists and holds the symbol
  named beside it;
- every package named with `-p` is known to `cargo metadata --offline`,
  and no guide runs the test suite of the whole workspace;
- every case and attack id is a row of exactly one guide and names no
  case that the corpus already has;
- each of the 25 invariants of the identity and evidence argument has at
  least one attack case, and each such id is a row of the step the table
  names;
- every case id that a question binds is a row of a guide;
- every M3 row of the owner table names ids a guide defines;
- every step is named by a scope or exit row above;
- every relative link and its anchor resolve.
