# Step 38: repair-run

Kind: evidence. The repair scenario, fourth part: the registered runner
repairs the seeded program through the wrapper, within the budget, and
every command and envelope is kept; the grader's verification follows
under its own budget.
Depends on: 37.
Crates: none.

## Prerequisites

- Read: `docs/roadmap/v0.0.2/m3/repair/registration.md` "Runner",
  "Budget", "Pass rule", and `repair/seeding.md`, written by steps 35 and
  37; [M2 pilot runs](../../m2/steps/40-pilot-runs.md) as the model of a
  run step.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `examples/pilot/grade.ps1:1` | `<#` | the model of a scripted harness that records every call |

From step 36: `examples/repair/run.ps1`, `compare.ps1`. Proposed:
`docs/roadmap/v0.0.2/m3/repair/runs/` (the log, each envelope, each
prompt and completion), `docs/roadmap/v0.0.2/m3/repair/run.md`.

## Files owned

- `docs/roadmap/v0.0.2/m3/repair/runs/`
- `docs/roadmap/v0.0.2/m3/repair/run.md`

## Ordered tasks

1. The runner session is started with the registered system prompt and
   the failing selector, on a fresh copy of the seeded commit, with the
   wrapper as its only tool; the integrator relays each command the model
   emits to the wrapper and the envelope back, verbatim, and keeps every
   prompt and completion.
2. The run ends when the model declares the repair done, when the budget
   is reached, or after one retried provider failure; the integrator then
   runs the registered grader commands through the wrapper under the
   grader's budget: `check`, `test` of the selector, `test` of the whole
   suite, and `compare.ps1` against the frozen baseline.
3. `repair/run.md`: the commit the run started from, every command in
   order with its result atom and the `approvals` it named, the applied
   edits' plans with the revision each carried, the grader's results, and
   the budget used. No grade is given here; step 39 applies the rule.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| every command through the wrapper | positive | the log | every line is an admitted shape and carries `--format json`, the workspace, and the pin where registered; the count is within the budget | registration |
| every envelope kept and valid | positive | `runs/envelopes/` | one file per command; each validates; each names the pinned approval or says none | 05: "A producer MUST validate every document it emits"; Q40 |
| the loop's steps present | positive | the log | at least one `test` of the selector that failed, one `query` of the declaration with its type, contracts, and effects, one `edit body` preview, one applied `edit body` carrying the preview's revision and the same subject, one `test` of the selector after it | release gate, step 3 |
| nothing but one body touched | positive | `compare.ps1` and `git status` after the run | `tests/` and `approvals/` equal the baseline byte for byte; changes under `examples/repair/seeded/src/` only | registration: the protected set and the baseline |

## Diagnostics and schema changes

None.

## Validation

```powershell
git diff --check
pwsh -NoProfile -File examples/repair/run.ps1 check
pwsh -NoProfile -File examples/repair/run.ps1 test
pwsh -NoProfile -File examples/repair/compare.ps1
```

Read: nothing; then the two grader envelopes and the comparison, recorded
in `run.md`.

## Stays unavailable

Unchanged.

## Excluded

Grading (39); a second attempt without an amendment.

## Completion evidence

- [ ] `repair/run.md` and `repair/runs/` hold every command, envelope,
      prompt, and completion.
- [ ] The grader's envelopes and the comparison are recorded.
