# Step 34: repair-run

Kind: evidence. The repair scenario, fourth part: the registered runner
repairs the seeded program through the wrapper, within the budget, and
every command and envelope is kept.
Depends on: 33.
Crates: none.

## Prerequisites

- Read: `docs/roadmap/v0.0.2/m3/repair/registration.md` "Runner",
  "Budget", "Pass rule", and `repair/seeding.md`, written by steps 31 and
  33; [M2 pilot runs](../../m2/steps/40-pilot-runs.md) as the model of a
  run step.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `examples/pilot/grade.ps1:1` | `<#` | the model of a scripted harness that records every call |

From step 32: `examples/repair/run.ps1`. Proposed:
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
   runs the registered verification commands through the wrapper: `check`,
   `test` of the selector, `test` of the whole suite, and reads the
   approval state.
3. `repair/run.md`: the commit the run started from, every command in
   order with its result atom, the applied edits' plans, the verification
   results, and the budget used. No grade is given here; step 35 applies
   the rule.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| every command through the wrapper | positive | the log | every line begins `vibra --format json`; the count is within the budget | registration |
| every envelope kept and valid | positive | `runs/envelopes/` | one file per command; each validates | 05: "A producer MUST validate every document it emits" |
| the loop's steps present | positive | the log | at least one `test`, one `query` of the declaration, one `edit replace` preview, one applied edit, one `test` of the selector | release gate, step 3 |
| nothing but the seeded copy touched | positive | `git status` after the run | changes under `examples/repair/seeded/src/` only | registration: the protected set |

## Diagnostics and schema changes

None.

## Validation

```powershell
git diff --check
pwsh -NoProfile -File examples/repair/run.ps1 --format json check
pwsh -NoProfile -File examples/repair/run.ps1 --format json test
```

Read: nothing; then the two verification envelopes, recorded in `run.md`.

## Stays unavailable

Unchanged.

## Excluded

Grading (35); a second attempt without an amendment.

## Completion evidence

- [ ] `repair/run.md` and `repair/runs/` hold every command, envelope,
      prompt, and completion.
- [ ] The verification envelopes are recorded.
