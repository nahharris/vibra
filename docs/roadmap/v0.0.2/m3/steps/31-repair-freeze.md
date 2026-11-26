# Step 31: repair-freeze

Kind: evidence. The repair scenario, first part: its registration is
frozen against the merged mechanisms before anyone seeds a defect or runs
a repair. Nothing in it can be chosen after a result is seen.
Depends on: 18, 28, 30.
Crates: none.

## Prerequisites

- Read: [v0.0.2 release gate](../../README.md#release-gate), step 3;
  [M3 README](../README.md) exit evidence; [M2 pilot registration](../../m2/pilot/registration.md)
  as the model of a frozen protocol; [execution model](../../../execution.md);
  [questions](../spec-questions.md) Q39.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-cli/src/lib.rs:68` | `pub fn run` | the one interface the runner may use, through `vibra --format json` |
| `examples/gate/project.vibon:1` | `{` | the release program the scenario repairs |

Proposed: `docs/roadmap/v0.0.2/m3/repair/registration.md`.

## Files owned

- `docs/roadmap/v0.0.2/m3/repair/registration.md`

## Ordered tasks

1. The registration states: the program (`examples/gate/` with its tests,
   under an approval of `@gate` written by `approve --write` at the freeze
   commit and committed with it); the seeder (a person or a model session
   that is not the runner's, named by role, with the exact model
   identifier where a model); the seeding rule (one declaration of the
   code under repair changed so that exactly one registered test fails
   and `check` passes; the seed recorded in a sealed file outside the
   repository the runner never reads; the seeded commit's hash recorded
   after seeding, before the run); the runner (one model session, exact
   identifier, the whole system prompt, which permits `vibra --format
   json` commands only, through the wrapper of step 32, and gives the
   failing test's selector and nothing about the defect); the budget (at
   most 40 commands, at most 3 applied edits, one retry of a provider
   failure); the pass rule and the exclusions of Q39; the four rejections
   each shown by a reference control before the run; the record's
   contents; the freeze commit and the date.
2. The grader is the toolchain at the freeze commit; a later commit may
   build it only if it changes no engine code.
3. From the freeze commit the registration changes only by a dated
   amendment written before the runs it affects.

## Test matrix

Checks of the registration itself, by reading, recorded in the report.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| separation | positive | the seeder and runner sections | different sessions or a person; the seed is sealed from the runner | release gate: "an independently seeded defect"; README: "seeded by someone other than the author of the repair run" |
| machine mode only | positive | the system prompt and the wrapper's rule | every permitted command begins `vibra --format json` | release gate: "Through machine-mode CLI only" |
| the loop is complete | positive | the pass rule | names reading the failure, querying the declaration, its type, contracts, and effects, previewing, applying, and rerunning the selected test | release gate, step 3 |
| the four rejections | positive | the controls section | stale edit, replaced test, changed policy, changed claim, each with the command and the expected code | README exit evidence: "Rejections: stale edit, replaced test, changed policy, changed claim under an old approval" |
| the rule is decidable | positive | three invented command logs | each gives one answer | M2 README: "under its registered rule" |
| an unregistered choice | negative | a draft that leaves the runner's model open | the freeze is refused until fixed | execution model: evidence is registered before it is gathered |

## Diagnostics and schema changes

None.

## Validation

```powershell
git diff --check
git log -1 --format=%H -- docs/roadmap/v0.0.2/m3/repair/registration.md
```

Read: the first prints nothing; the second prints the freeze commit, which
the registration names in the following commit.

## Stays unavailable

Unchanged.

## Excluded

Seeding, running, changing the specification.

## Completion evidence

- [ ] `repair/registration.md` is committed and names its freeze commit in
      the following commit's report.
- [ ] The six rows are answered in the report.
- [ ] No defect was seeded and no run was made.
