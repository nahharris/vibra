# Milestone execution model

Status: process guidance
Applies to: every milestone in [`v1.md`](v1.md)

This document records how roadmap milestones are broken into work and how
progress is tracked. It is process guidance, not specification. It cannot
weaken `docs/spec/`, and it cannot change a milestone's deliverables or exit
gate. When this document and the roadmap disagree about what a milestone
contains, the roadmap wins and this document is corrected.

Future sessions picking up an active milestone should read this document
first, then that milestone's step plan.

## Start every implementation session here

1. Read `AGENTS.md`, the charter, the active milestone row in `v1.md`, and the
   milestone README. Read the topic specification sections named by the chosen
   step before editing code. Archived code is not an implementation template.
2. Inspect the working tree; preserve unrelated work. Fetch the integration
   branch and create a short-lived branch from its refreshed remote head.
   Milestone 1 starts from `origin/m1`, not `main` or an old local `m1`.
3. Select the earliest unfinished step whose prerequisites are satisfied.
   Verify the prerequisite code and merge history; a status row alone is not
   evidence. Run the existing focused tests and the independent corpus runner.
4. Write a small behavior checklist: normative rule, input, expected result,
   diagnostic code/level/span, formatter result, host test, and corpus case.
   List semantic checks deferred to later milestones separately.
5. Resolve any missing observable contract in the specification before coding
   that behavior. Do not invent diagnostic codes, ordering, recovery policy,
   or schema fields in a test and then treat the test as authority.

## Implementation loop

Implement one row of the behavior checklist at a time. First add a focused
host test and an independently authored corpus expectation; confirm that the
failure is the missing behavior, not a broken fixture. Implement the smallest
complete path through syntax, formatting, diagnostics, and the real profile
handler. Re-run that row, then the existing neighboring regressions.

Prefer explicit grammar-context dispatch and small checked helpers over one
large parser with boolean mode flags. Every malformed-input path must either
consume input or return to its caller; retain source spans and error markers.
Do not turn malformed reserved syntax into a successful generic application.
Keep lossless source storage separate from normalized values and typed views.

An internal subtask may be narrow, but the merge unit remains the complete
vertical step. If a step is too large, revise its plan into smaller conforming
slices with explicit coverage before splitting delivery. Do not silently move
its formatter, negative cases, or machine contract into a follow-up.

## Required contents of an implementation guide

Each unfinished step has a linked guide containing:

- prerequisites and exact specification sections to read;
- existing entry points and proposed module responsibilities (new names are
  proposals, not claims that those APIs already exist);
- ordered implementation tasks and the invariants preserved by each;
- positive, negative, recovery, boundary, and formatter test matrices;
- diagnostic and schema changes or an explicit reason none are needed;
- executable validation commands and the required interpretation of results;
- excluded work, unresolved contract decisions, and completion evidence.

A guide is instructions for producing evidence, not the evidence itself.
Generated expected output must be reviewed against the specification before
being committed. Never fix a regression by accepting new snapshots wholesale,
removing failing cases, downgrading their profile, or treating unavailable as
passed. Host tests should inspect structure and failure paths; corpus cases
should assert externally observable behavior through the real handler.

## Decomposition rule

A roadmap milestone is too large to implement as one change. Each milestone is
decomposed into an ordered list of **steps** before implementation starts.

A step is cut so that it is a vertical, conforming slice of the language, as
roadmap rule 2 requires. Widening the accepted language by one grammar area,
with its tree nodes, formatter rules, diagnostics, schemas, and conformance
cases in the same change, is a step. "The whole lexer" and "the whole
formatter" are not steps, because they defer the conformance evidence that
proves them.

Two kinds of step are exempt from the vertical-slice rule because they carry
no language behavior:

- infrastructure steps, which create the workspace, CI, or a harness; and
- evidence steps, which run a fuzz campaign or sweep the exit gate.

Both must say so explicitly in the step plan.

## Definition of done for a step

A step is done when all of the following are true for the behavior it claims:

1. the behavior matches the active specification;
2. host-language tests cover the implementation, including its failure paths;
3. Vibra conformance cases cover the observable behavior, addressed by the
   specification rule IDs in `spec/07-diagnostics-and-conformance.md`;
4. every diagnostic the step can emit is in the closed registry with its
   fixed level;
5. affected machine schemas are updated in the same change;
6. canonical prose is updated in the same change; and
7. CI is green.

A step that implements only part of a promised feature states which part in
its pull request, and the step plan keeps the remainder assigned to a later
step. It is never advertised as complete.

## Branch and pull request model

Each milestone has one long-lived **integration branch** named for the
milestone: `m1`, `m2`, and so on. It is created from `main` when the milestone
starts.

- Every step is developed on its own short-lived branch and merged into the
  integration branch through one pull request.
- The integration branch has one **draft pull request into `main`**, opened as
  soon as the first step lands and left in draft for the life of the
  milestone. It is the milestone's running view: the cumulative diff, the
  cumulative CI result, and the place the exit-gate evidence is recorded.
- That draft is marked ready for review only when the exit gate is evidenced,
  and it is the single merge of the milestone into `main`.
- `main` therefore never carries a partially implemented milestone, which is
  what roadmap rule 3 requires.

Checks run on pull requests. Every step pull request is checked against the
integration branch, and every push to the integration branch re-checks the
cumulative milestone through its standing draft. No branch-level trigger is
needed for the integration branch, and none is configured: the draft pull
request already covers it, and a second trigger would only duplicate runs.

A step pull request is not merged with red CI.

## Progress tracking

Each active or completed milestone has a directory:

```text
docs/roadmap/milestone-<n>/
  README.md
```

`README.md` contains:

- the fixed design decisions taken for that milestone, with their reasons;
- the ordered step table, with one row per step; and
- a mapping from steps to the milestone's roadmap deliverables and exit-gate
  clauses, so no deliverable is silently dropped.

Each step row carries a status:

| Status | Meaning |
| --- | --- |
| `not started` | No branch exists. |
| `in progress` | A branch exists; the step is not merged. |
| `landed` | Merged into the milestone integration branch. |

Record the PR and merge commit beside landed work. A step may have open
prerequisites while still `not started`; identify those in its guide rather
than suggesting implementation may choose the missing contract. In a pending
PR, a proposed `landed` row is conditional on that PR merging, not a claim that
the remote integration branch already contains it.

The pull request that completes a step sets that row to `landed`, so the table
is true on the integration branch the moment the merge happens. A session that
finishes a step and leaves the table stale has not finished the step.

## Design decisions

A decision that constrains later steps — a crate boundary, an on-disk format,
a dependency choice — is recorded in the milestone `README.md` with the reason
it was taken. Recording the reason is the point: a later session must be able
to tell a deliberate constraint from an accident.

A decision that changes observable language behavior is not a milestone
decision. It is a specification change and follows the change protocol in
[`../index.md`](../index.md).

## Milestone completion

When the last step lands, an evidence step verifies every exit-gate clause and
records the evidence in the milestone `README.md`. Only then does the
integration branch merge to `main`, together with the status updates to
`v1.md`, `README.md`, and any other active document whose claims the milestone
changed.

## Review and handoff evidence

Before publishing, inspect the complete diff against the refreshed integration
base. Check every promised behavior against its test and case, including a
nearby invalid input and recovery with a valid following form. Verify that no
later milestone behavior or archive dependency entered the change. Check local
documentation links and all commands against the actual workspace.

The handoff records base and tested commit, changed behavior, exact commands
and exit results, corpus passed/failed/unavailable counts, remaining scope,
and PR state. A command not run is recorded as not run; a dependency-download
failure is not a passing test. CI must validate the final PR head. If the head
changes, review the new diff and re-run affected checks. Verify the actual merge
before reporting delivery to the integration branch.
