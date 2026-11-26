# Step 39: repair-record

Kind: evidence. The repair scenario, last part: the registered rule applied
to the runner's log and the grader's results, the dated record, and what
the scenario does not show.
Depends on: 38.
Crates: none.

## Prerequisites

- Read: `docs/roadmap/v0.0.2/m3/repair/registration.md` "Pass rule",
  "Exclusions", "What the record reports", and `repair/run.md`, written by
  steps 35 and 38; [M2 pilot observation](../../m2/pilot/observation.md) as
  the model.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `docs/roadmap/v0.0.2/m2/pilot/observation.md:1` | `# Model-writing pilot: observation` | the shape of a dated record |

Proposed: `docs/roadmap/v0.0.2/m3/repair/observation.md`.

## Files owned

- `docs/roadmap/v0.0.2/m3/repair/observation.md`

## Ordered tasks

1. Apply the registered pass rule to `run.md`, each clause answered with
   the command that shows it: from the runner's ordered log, the failure
   read, the declaration queried with its type, contracts, and effects, a
   preview, an apply carrying the preview's revision and the same operation
   and subject, and the selected test rerun and passed; from the grader,
   the whole suite passed, `tests/` and `approvals/` equal to the frozen
   baseline byte for byte, the approval named in every envelope equal to
   the pin; every envelope valid; the controls passed before the run; the
   budget kept.
2. Record the deviations from the registration, if any, and what the
   scenario does not show: one program, one seed, one model, one run.
3. The record is dated. If the rule's answer is a fail, the record says
   so and names the clause; the exit step then records the gate as not met
   and the milestone does not exit. A second run needs a dated amendment
   and a new seed.
4. The seed is unsealed into the record after the rule is applied.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| every clause answered | positive | the pass rule | each clause has its command and result, read from the log or the grader | registration; Q39 |
| the controls precede the run | positive | the commits of steps 36 and 38 | 36's commit is an ancestor of 38's | execution model: evidence registered before gathered |
| the seed unsealed last | positive | the record | the seed appears after the rule's answer | registration |
| the end state alone decides nothing | negative | an invented log in which the selected test passes and the approval is current but a protected byte differs from the baseline | the rule answers fail | Q39 |

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

Changing the rule after the run.

## Completion evidence

- [ ] `repair/observation.md` is dated, answers the rule, and names the
      freeze, seeding, and run commits.
