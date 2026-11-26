# Step 33: repair-seeding

Kind: evidence. The repair scenario, third part: the seeder plants the
defect as the registration says, records it in the sealed file, and
commits the seeded program; the seed is verified to fail exactly one
registered test without anyone else reading it.
Depends on: 32.
Crates: `examples`.

## Prerequisites

- Read: `docs/roadmap/v0.0.2/m3/repair/registration.md` "Seeding" and
  `repair/references.md`, written by steps 31 and 32.

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
   `examples/repair/seeded/`, changes exactly one declaration of the code
   under repair, runs `check` (accepted) and `test` (exactly one item
   `@test.assertionFailed`, the others passed) through the wrapper, and
   writes the sealed file outside the repository with the declaration, the
   change, and the failing selector.
2. The seeder commits the seeded copy with the message
   `M3 step 33: repair seeding` and records the commit hash and the
   failing selector in `repair/seeding.md`, and nothing about the change.
3. The integrator verifies, without reading the seed: the approval of the
   seeded copy is current (`check` has no diagnostic), `test` fails exactly
   one item, and the diff against `examples/gate/` touches one file under
   `src/`.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `ATK-repair-seed-sealed` | positive | the seeded copy and the sealed file | the sealed file is outside the repository; `repair/seeding.md` names the selector and the commit and no declaration; the runner's prompt names only the selector | release gate: "an independently seeded defect"; registration |
| one failing test | positive | `test` through the wrapper | exactly one `@test.assertionFailed` | registration: "exactly one registered test fails" |
| check passes | positive | `check` | accepted; the approval current | release gate: "Start from an independently seeded defect and a failing test" |
| one file under `src/` | positive | `git diff --stat examples/gate examples/repair/seeded` | one source file differs; no file under `tests/` and not `project.vibon` | registration: "one declaration of the code under repair" |

## Diagnostics and schema changes

None.

## Validation

```powershell
pwsh -NoProfile -File examples/repair/run.ps1 --format json check
pwsh -NoProfile -File examples/repair/run.ps1 --format json test
git diff --check
```

Read: `@command.ok`; `@command.testFailed` with one failed item; nothing.

## Stays unavailable

Unchanged.

## Excluded

The run (34).

## Completion evidence

- [ ] `repair/seeding.md` names the seeder's role, the commit, and the
      selector.
- [ ] The three verifications are in the report, by the integrator, who
      states that the seed was not read.
