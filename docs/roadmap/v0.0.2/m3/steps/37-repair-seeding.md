# Step 37: repair-seeding

Kind: evidence. The repair scenario, third part: the seeder plants the
defect in a body as the registration says, records it in the sealed file,
and commits the seeded program; the seed is verified to fail exactly one
registered test without anyone else reading it; the seeded commit is the
frozen baseline.
Depends on: 36.
Crates: `examples`.

## Prerequisites

- Read: `docs/roadmap/v0.0.2/m3/repair/registration.md` "Seeding" and
  `repair/references.md`, written by steps 35 and 36.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `examples/gate/src/gate/tally.vib:1` | `//` | the code under repair the seeder may change |
| `examples/gate/tests/main.vib:1` | `//` | the protected tests the seeder may not change |

Proposed: `examples/repair/seeded/` (a copy of the gate program with the
seed), `docs/roadmap/v0.0.2/m3/repair/seeding.md`.

## Files owned

- `examples/repair/seeded/`
- `docs/roadmap/v0.0.2/m3/repair/seeding.md`

## Ordered tasks

1. The seeder, as registered, copies `examples/gate/` to
   `examples/repair/seeded/`, changes the body of exactly one declaration
   of the code under repair, runs `check` (accepted, the approval in force)
   and `test` (exactly one item `@test.assertionFailed`, the others
   passed) through the wrapper, and writes the sealed file outside the
   repository with the declaration, the change, and the failing selector.
2. The seeder commits the seeded copy with the message
   `M3 step 37: repair seeding` and records the commit hash and the
   failing selector in `repair/seeding.md`, and nothing about the change.
   That commit is the frozen baseline the grader compares against.
3. The integrator verifies, without reading the seed: the approval of the
   seeded copy is in force with the pinned identity (`check` has no
   diagnostic and names the pin), `test` fails exactly one item, the diff
   against `examples/gate/` touches one file under `src/` and only a body
   in it (the claims of the seeded copy equal the original's).

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `ATK-repair-seed-sealed` | positive | the seeded copy and the sealed file | the sealed file is outside the repository; `repair/seeding.md` names the selector and the commit and no declaration; the runner's prompt names only the selector | release gate: "an independently seeded defect"; registration |
| one failing test | positive | `test` through the wrapper | exactly one `@test.assertionFailed` | registration: "exactly one registered test fails" |
| check passes under the pin | positive | `check` through the wrapper | accepted; `approvals` names the pin | release gate: "Start from an independently seeded defect and a failing test" |
| one body under `src/` | positive | `git diff --stat examples/gate examples/repair/seeded`, then `query` of every declaration of the changed module | one source file differs; no file under `tests/` and not `project.vibon`; every claim of the module equal to the original's | registration: the seeding rule, the body of one declaration of the code under repair |

## Diagnostics and schema changes

None.

## Validation

```powershell
pwsh -NoProfile -File examples/repair/run.ps1 check
pwsh -NoProfile -File examples/repair/run.ps1 test
git diff --check
```

Read: `@command.ok`; `@command.testFailed` with one failed item; nothing.

## Stays unavailable

Unchanged.

## Excluded

The run (38).

## Completion evidence

- [ ] `repair/seeding.md` names the seeder's role, the commit, and the
      selector.
- [ ] The four verifications are in the report, by the integrator, who
      states that the seed was not read.
