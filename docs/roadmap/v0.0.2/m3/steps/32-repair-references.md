# Step 32: repair-references

Kind: evidence. The repair scenario, second part: the wrapper the runner
goes through, a reference run of the loop by hand, and the four negative
controls, each passing before any seed is planted.
Depends on: 31.
Crates: `examples`.

## Prerequisites

- Read: `docs/roadmap/v0.0.2/m3/repair/registration.md` (step 31) whole;
  [M2 pilot references](../../m2/pilot/references.md) as the model.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `examples/pilot/grade.ps1:1` | `<#` | the model of a scripted grader over the binary |
| `crates/vibra-cli/tests/it/gate.rs:1` | `//!` | the gate program's tests, the reference for its failing test |

Proposed: `examples/repair/run.ps1` (the wrapper),
`examples/repair/controls.ps1`,
`docs/roadmap/v0.0.2/m3/repair/references.md`.

## Files owned

- `examples/repair/`
- `docs/roadmap/v0.0.2/m3/repair/references.md`

## Ordered tasks

1. `run.ps1 <command line>`: refuses any program but the registered
   `vibra` binary and any line that does not begin `--format json`; runs
   it in the scenario's working copy; appends the command, the exit code,
   and the envelope to `log.tsv` and `envelopes/<n>.json`; counts
   commands and applied edits against the budget; validates each envelope
   against the schema.
2. The reference run: a person performs the registered loop on a copy with
   a known defect (not the seed): `test` to read the failure, `query` the
   declaration with `--include signature,type,effects` and the obligations,
   `edit replace` preview, `edit replace --write --expect-revision`, `test`
   of the selector. Each envelope validates; the log is kept as the
   reference.
3. `controls.ps1`: the four rejections on a copy, each a command and the
   code it must give: a stale edit (`--expect-revision` of a previous
   revision, `@tool.stalePlan`); a replaced test (an `edit replace` of a
   test helper under the approval, `@tool.actionDenied`; and a test module
   changed by hand, `check` giving `@project.staleApproval`); a changed
   policy (the project's policy edited by hand, `@grant.invalidBinding`);
   a changed claim (the entry's row edited by hand, `@project.staleApproval`).
   Each must show its code before any run.
4. `repair/references.md`: the reference log and the controls' results.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `ATK-repair-wrapper-refuses-other-commands` | negative | `run.ps1 git status`, `run.ps1 vibra check` (no `--format json`), `run.ps1 vibra --format human test` | each refused and logged as refused; nothing runs | release gate: "Through machine-mode CLI only" |
| the reference loop | positive | the registered sequence on the known defect | every envelope validates; the selected test passes at the end; the approval stays current | release gate, step 3 |
| control: stale edit | negative | the registered command | `@tool.stalePlan`, nothing written | README exit evidence |
| control: replaced test | negative | the two registered commands | `@tool.actionDenied`; `@project.staleApproval` | README exit evidence |
| control: changed policy | negative | the registered edit of `project.vibon` | `@grant.invalidBinding` at the recipe | README exit evidence |
| control: changed claim | negative | the registered edit of the entry's row | `@project.staleApproval` related to the entry | README exit evidence |
| budget counted | positive | 41 commands through the wrapper | the 41st is refused | registration: the budget |

## Diagnostics and schema changes

None.

## Validation

```powershell
pwsh -NoProfile -File examples/repair/controls.ps1 -Vibra target/debug/vibra.exe
git diff --check
```

Read: the controls print `4 of 4 controls as registered`; `git diff
--check` prints nothing. The binary is built from the freeze commit.

## Stays unavailable

Unchanged.

## Excluded

Seeding (33); the run (34).

## Completion evidence

- [ ] `repair/references.md` holds the reference log and each control's
      command, code, and result.
- [ ] The wrapper's refusals were seen.
- [ ] No seed was planted.
