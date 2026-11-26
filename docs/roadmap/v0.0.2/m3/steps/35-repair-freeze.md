# Step 35: repair-freeze

Kind: evidence. The repair scenario, first part: its registration is
frozen against the merged focused lanes before anyone seeds a defect or
runs a repair. Nothing in it can be chosen after a result is seen. The
breadth commands run beside the scenario and are required by the exit, not
by this freeze.
Depends on: 20, 22, 24.
Crates: none.

## Prerequisites

- Read: [v0.0.2 release gate](../../README.md#release-gate), step 3;
  [M3 README](../README.md) exit evidence; [M2 pilot registration](../../m2/pilot/registration.md)
  as the model of a frozen protocol; [execution model](../../../execution.md);
  [architecture](../architecture.md) rows I25 to I27;
  [questions](../spec-questions.md) Q14, Q39, Q40.

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
   commit and committed with it); the **pin** (the approval's identity,
   the SHA-256 of `approvals/gate.vibon`, written into the registration,
   which the repair copy cannot change); the **frozen baseline** (the
   commit of the seeded copy before the run, against which every protected
   byte and the approval file are compared); the seeder (a person or a
   model session that is not the runner's, named by role, with the exact
   model identifier where a model); the seeding rule (the body of one
   declaration of the code under repair changed so that exactly one
   registered test fails and `check` passes; the seed recorded in a sealed
   file outside the repository the runner never reads; the seeded commit's
   hash recorded after seeding, before the run); the runner (one model
   session, exact identifier, the whole system prompt, which permits
   commands through the wrapper of step 36 only, and gives the failing
   test's selector and nothing about the defect); the wrapper's admission
   list, by command and operand shape: `test` with no operand or one
   selector; `query` with one subject and `--include` and `--expand`
   values from the chapter's tables; `edit body` with an entity atom and a
   record, with or without `--write`; `check`; nothing else, and never
   `approve`; the wrapper's additions to every command: `--format json`,
   `--workspace <the repair copy>`, `--expect-approval <the pin>` on
   `check`, `test`, and `edit body`, and `--expect-revision <the revision
   of the last preview relayed>` on every `--write`; the budget (at most
   40 runner commands, at most 3 applied edits, one retry of a provider
   failure; the grader's commands counted apart); the pass rule and the
   exclusions of Q39, with the grade read from the runner's ordered log;
   the four rejections each shown by a negative control that verifies zero
   writes and a positive control that writes, before the run; the record's
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
| machine mode only, by shape | positive | the system prompt and the wrapper's admission list | every admitted command is a listed command with listed operand shapes; `approve` is absent; every command carries `--format json` | release gate: "Through machine-mode CLI only" |
| the pin is outside the copy | positive | the pin section | the identity is written in the registration and supplied by the wrapper; the copy's approval file cannot change it | Q40 |
| the loop is complete | positive | the pass rule | names reading the failure, querying the declaration, its type, contracts, and effects, previewing, applying with the preview's revision and the same operation and subject, and rerunning the selected test, all read from the runner's log; then the grader's whole-suite pass and the byte comparison | release gate, step 3; Q39 |
| the four rejections | positive | the controls section | stale edit, replaced test, changed policy, changed claim, each with the command, the expected code, the zero-write check, and its positive twin | README exit evidence: "Rejections: stale edit, replaced test, changed policy, changed claim under an old approval" |
| the rule is decidable | positive | three invented command logs | each gives one answer | M2 README: "under its registered rule" |
| an unregistered choice | negative | a draft that leaves the runner's model open, or the pin blank | the freeze is refused until fixed | execution model: evidence is registered before it is gathered |

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

Seeding, running, changing the specification; the breadth commands.

## Completion evidence

- [ ] `repair/registration.md` is committed and names its freeze commit in
      the following commit's report.
- [ ] The seven rows are answered in the report.
- [ ] No defect was seeded and no run was made.
