# Step 36: repair-references

Kind: evidence. The repair scenario, second part: the wrapper the runner
goes through, a reference run of the loop by hand, and the four negative
controls with their positive twins, each passing before any seed is
planted.
Depends on: 35.
Crates: `examples`.

## Prerequisites

- Read: `docs/roadmap/v0.0.2/m3/repair/registration.md` (step 35) whole;
  [M2 pilot references](../../m2/pilot/references.md) as the model.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `examples/pilot/grade.ps1:1` | `<#` | the model of a scripted grader over the binary |
| `crates/vibra-cli/tests/it/gate.rs:1` | `//!` | the gate program's tests, the reference for its failing test |

Proposed: `examples/repair/run.ps1` (the wrapper),
`examples/repair/controls.ps1`, `examples/repair/compare.ps1` (the byte
comparison), `docs/roadmap/v0.0.2/m3/repair/references.md`.

## Files owned

- `examples/repair/`
- `docs/roadmap/v0.0.2/m3/repair/references.md`

## Ordered tasks

1. `run.ps1 <command line>`: admits a line only when it matches the
   registration's list of command and operand shapes; refuses any other
   program, any `approve`, and any global option the runner writes;
   supplies `--format json`, `--workspace <copy>`, `--expect-approval
   <pin>` where the registration says, and `--expect-revision <last
   preview's revision>` on a `--write`; runs the registered `vibra` binary;
   appends the command, the exit code, and the envelope to `log.tsv` and
   `envelopes/<n>.json`; counts commands and applied edits against the
   budget; validates each envelope against the schema.
2. `compare.ps1`: the recursive listing and every byte of the copy's
   `tests/` and `approvals/` against the frozen baseline commit; equal or
   the first difference.
3. The reference run: a person performs the registered loop on a copy with
   a known defect (not the seed): `test` to read the failure, `query` the
   declaration with `--include signature,type,effects` and the obligations,
   `edit body` preview, `edit body --write`, `test` of the selector. Each
   envelope validates and names the pinned approval; the log is kept as the
   reference.
4. `controls.ps1`: the four rejections on a copy, each a negative control
   with its code and a zero-write check by `compare.ps1`, and a positive
   twin that writes: a stale edit (`--expect-revision` of a previous
   revision, `@tool.stalePlan`; the twin with the current revision
   applies); a replaced test (an `edit body` of a test helper under the
   approval, `@tool.actionDenied`; a test module changed by hand, `check`
   giving `@project.staleApproval`; the twin a body edit of the code under
   repair that applies); a changed policy (the project's policy edited by
   hand, `@grant.invalidBinding`; the twin the policy restored, `check`
   accepted); a changed claim (the entry's row edited by hand,
   `@project.staleApproval`; the twin restored). Each negative control's
   zero-write check passes and each twin's write is seen before any run.
5. `repair/references.md`: the reference log and the controls' results.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `ATK-repair-wrapper-refuses-other-commands` | negative | `run.ps1 git status`, `run.ps1 vibra check --format human`, `run.ps1 vibra approve @gate --write`, `run.ps1 vibra edit rename @gate.tally.count n`, `run.ps1 vibra --workspace .. test` | each refused and logged as refused; nothing runs | release gate: "Through machine-mode CLI only"; registration: the admission list |
| `ATK-repair-wrapper-pins-approval` | positive | `run.ps1 vibra test @tests.main::"…"` | the line run carries `--format json`, `--workspace <copy>`, `--expect-approval <pin>`; the envelope's `approvals` names the pin | Q40 |
| `ATK-repair-wrapper-supplies-revision` | positive | an `edit body … --write` after a preview | the line run carries `--expect-revision` of the preview relayed last | Q14 |
| the reference loop | positive | the registered sequence on the known defect | every envelope validates; the selected test passes at the end; the approval stays in force; `compare.ps1` reports equal | release gate, step 3 |
| control: stale edit | negative | the registered command | `@tool.stalePlan`, `compare.ps1` equal and the source byte-for-byte unchanged; the twin applies | README exit evidence |
| control: replaced test | negative | the two registered commands | `@tool.actionDenied`; `@project.staleApproval`; zero writes by the tool; the twin applies | README exit evidence |
| control: changed policy | negative | the registered edit of `project.vibon` | `@grant.invalidBinding` at the recipe; zero writes; the twin accepted | README exit evidence |
| control: changed claim | negative | the registered edit of the entry's row | `@project.staleApproval` related to the entry; zero writes; the twin accepted | README exit evidence |
| budget counted | positive | 41 commands through the wrapper | the 41st is refused; a grader command does not count | registration: the budget |

## Diagnostics and schema changes

None.

## Validation

```powershell
pwsh -NoProfile -File examples/repair/controls.ps1 -Vibra target/debug/vibra.exe
git diff --check
```

Read: the controls print `8 of 8 controls as registered`; `git diff
--check` prints nothing. The binary is built from the freeze commit.

## Stays unavailable

Unchanged.

## Excluded

Seeding (37); the run (38).

## Completion evidence

- [ ] `repair/references.md` holds the reference log and each control's
      command, code, zero-write check, twin, and result.
- [ ] The wrapper's refusals were seen.
- [ ] No seed was planted.
