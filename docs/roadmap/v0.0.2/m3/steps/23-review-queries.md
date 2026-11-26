# Step 23: review-queries

Kind: evidence. The adversarial review of the queries the repair loop
reads and of the test limits: an independent model reads the code of steps
05, 06, 08, 12, 17, and 18 against the chapters and rows I5, I16, I18,
I19, and tries to make a query compute, invent, or grant, a fact carry a
status its layer did not give it, or a limit go unreported. It changes no
code.
Depends on: 17, 18.
Crates: none.

## Prerequisites

- Read: [05 Workspace queries](../../../../spec/05-tooling.md#workspace-queries)
  and its subsections; [05 Tests and selection](../../../../spec/05-tooling.md#tests-and-selection);
  [07 Recovery](../../../../spec/07-diagnostics-and-conformance.md#recovery);
  [architecture](../architecture.md) section 2; [questions](../spec-questions.md)
  Q27, Q29, Q32 to Q37.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-interp/src/evaluate.rs:243` | `pub fn run_test` | the test entry with its limits |

The files of the six steps: `crates/vibra-workspace/src/query/subject.rs`,
`structural.rs`, `semantic.rs`, `effects.rs`, `test_runner.rs`,
`crates/vibra-types/src/check/facts.rs`, `crates/vibra-syntax/src/position.rs`,
`crates/vibra-interp/src/evaluate.rs`.

## Files owned

- `docs/roadmap/v0.0.2/m3/evidence/review-queries.md`

## Ordered tasks

1. The reviewer, with the chapters, this plan, and the merged tree.
2. Probes it must try, at least: a position whose fact the checker
   recorded from syntax alone (a role from the token's look); a fact that
   stays `exact` in a body with an error; a node whose fields share one
   status where the chapter gives them three; an effect fact readable
   before the body's verdict; a type expanded instead of referenced; a
   `nodeId` that two positions share; a query that reads a file after the
   capture; a captured snapshot over the nesting limit that answers
   `workspaceRevision: null`; a checked edge the facts do not export (a
   destination dispatch, a default through `Self`, a bound's conformance);
   a tail loop that is not counted; a test-world operation that is not
   charged; a test item that reaches both limits; a failed item followed
   by an exhausted one whose suite is not `@command.operationalFailure`;
   a test run that reaches the runner without the admission hook.
3. Each finding with a probe, the observed behaviour, the sentence, a
   severity, and the file.
4. Write `evidence/review-queries.md`.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| every probe answered | positive | the probe list | each holds, fails, or is not run with a reason | execution model |
| every invariant read | positive | rows I5, I16, I18, I19 | each has a statement | architecture, section 2 |

## Diagnostics and schema changes

None.

## Validation

```powershell
git diff --check
```

Read: prints nothing.

## Stays unavailable

Unchanged.

## Excluded

Fixing anything (24); evaluation, expansions, the index, and the references,
which gate 33 reads.

## Completion evidence

- [ ] `evidence/review-queries.md` lists every probe and finding.
- [ ] The report names the reviewing model and the commit reviewed.
