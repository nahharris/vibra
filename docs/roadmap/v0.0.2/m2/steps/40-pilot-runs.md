# Step 40: pilot-runs

Kind: evidence. The model-writing pilot, third part: the registered runs,
each graded mechanically, with every prompt and output kept.
Depends on: 39.
Crates: none.

## Prerequisites

- The registration, `pilot/registration.md`, and
  the reference results, `pilot/references.md`.
- The owner's approval of the registered budget, recorded in the
  registration before the first call.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-cli/src/lib.rs:82` | `fn execute` | the grader, through `examples/pilot/grade.ps1` |

Proposed: `docs/roadmap/v0.0.2/m2/pilot/runs/` (one record per attempt);
`docs/roadmap/v0.0.2/m2/pilot/runs.tsv`.

## Files owned

- `docs/roadmap/v0.0.2/m2/pilot/runs/`
- `docs/roadmap/v0.0.2/m2/pilot/runs.tsv`

## Ordered tasks

1. Generate the schedule from the registration: every task, arm, model,
   and attempt index, in the registered order. Invariant: the schedule is
   written before the first call and never reordered.
2. For each entry: send the registered prompt, with no other context;
   store the prompt digest, the model identifier the provider reports,
   the parameters, the complete output, and the time; extract the
   solution by the registered rule; grade it with `grade.ps1`; append one
   line to `runs.tsv`.
3. A provider error is recorded as such and retried only as the
   registration's stop rule allows; an output is never regenerated
   because it failed.
4. Nothing is excluded here. An attempt the registration excludes is
   marked with the registered reason and kept.

## Test matrix

Checks of the run records, by a script the step writes and runs.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| schedule complete | positive | `runs.tsv` against the schedule | one line per scheduled attempt, in order | the registration: attempts per task, arm, and model |
| every record kept | positive | `runs/` | one record per line of `runs.tsv`, with prompt digest and whole output | README: "a dated observation" |
| prompt identity | positive | each record's digest | one of the two registered digests | README: "two surface arms that differ only in…" |
| regrade is stable | positive | `grade.ps1` rerun on 20 stored outputs chosen by the registered seed | the same results | README: "graded by this milestone's checker and test world" |
| an extra attempt | negative | a record with no schedule entry | the script fails | the registration |
| a missing attempt | negative | a schedule entry with no record | the script fails, or the line says `providerError` under the stop rule | the registration |

## Diagnostics and schema changes

None.

## Validation

```powershell
pwsh -NoProfile -File examples/pilot/grade.ps1 -VerifyRuns docs/roadmap/v0.0.2/m2/pilot/runs.tsv
```

Read: exit 0 and the line `schedule complete: N of N`.

## Stays unavailable

Unchanged.

## Excluded

Analysis and interpretation (41); any run outside the schedule.

## Completion evidence

- [ ] `runs.tsv` and `runs/` are committed with the toolchain commit and
      the dates of the first and last call.
- [ ] The verification script's output is pasted in the report.
- [ ] No output was regenerated; every provider error is listed.
