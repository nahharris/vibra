# Step 29: review-queries-and-eval

Kind: evidence. The adversarial review of queries, the index, bounded
evaluation, the test limits, and the generated references: an independent
model reads the code of steps 05, 06, 08, 10, 12, 15, 16, 22, 23, 25, 26
against the chapters and rows I5, I15 to I19, I24, and tries to make a
query compute, invent, or grant, an evaluation reach a host, or a limit go
unreported. It changes no code.
Depends on: 10, 15, 22, 23, 26.
Crates: none.

## Prerequisites

- Read: [05 Workspace queries](../../../../spec/05-tooling.md#workspace-queries)
  and its subsections; [05 Program arguments, execution and evaluation](../../../../spec/05-tooling.md#program-arguments-execution-and-evaluation);
  [07 Recovery](../../../../spec/07-diagnostics-and-conformance.md#recovery);
  [architecture](../architecture.md) section 2; [questions](../spec-questions.md)
  Q23 to Q38.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-interp/src/evaluate.rs:313` | `pub fn evaluate` | the one entry evaluation may use |

The files of the eleven steps: `crates/vibra-workspace/src/query/`,
`eval.rs`, `test_runner.rs`, `crates/vibra-types/src/check/facts.rs`,
`crates/vibra-syntax/src/position.rs`, and
`crates/vibra-conformance/src/bin/reference/`.

## Files owned

- `docs/roadmap/v0.0.2/m3/evidence/review-queries-and-eval.md`

## Ordered tasks

1. The reviewer, with the chapters, this plan, and the merged tree.
2. Probes it must try, at least: a position whose fact the checker
   recorded from syntax alone (a role from the token's look); a fact that
   stays `exact` in a body with an error; a type expanded instead of
   referenced; a `nodeId` that two positions share; a query that reads a
   file after the capture; an expansion that disagrees with the index; an
   index whose `facts` has an identity that differs from `Identities`; an
   `eval` of a `fn` that calls a `proc` through a value; an `eval` whose
   argument decodes without its `where`; an `eval` of a generic through a
   non-generic wrapper; a tail loop that is not counted; a test item that
   reaches both limits; an envelope just over the bound with one list; a
   bounded document that is invalid JSON; a reference example whose
   registered verdict the engine does not give; a card token the reader
   does not know.
3. Each finding with a probe, the observed behaviour, the sentence, a
   severity, and the file.
4. Write `evidence/review-queries-and-eval.md`.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| every probe answered | positive | the probe list | each holds, fails, or is not run with a reason | execution model |
| every invariant read | positive | rows I5, I15 to I19, I24 | each has a statement | architecture, section 2 |

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

Fixing anything (30).

## Completion evidence

- [ ] `evidence/review-queries-and-eval.md` lists every probe and finding.
- [ ] The report names the reviewing model and the commit reviewed.
