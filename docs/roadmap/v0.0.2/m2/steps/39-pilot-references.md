# Step 39: pilot-references

Kind: evidence. The model-writing pilot, second part: every task's
reference solution passes and every negative control fails as registered,
in both arms, before any model call.
Depends on: 38.
Crates: `examples`.

## Prerequisites

- The registration, `pilot/registration.md`, frozen.
- Read: this milestone's [README](../README.md) "The model-writing
  pilot"; [programs
  chapter](../../../../spec/04-programs-and-packages.md#test-worlds-and-grants)
  "Test worlds and grants".

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-cli/src/test.rs:52` | `pub fn run` | the grader's test command |
| `crates/vibra-cli/src/check.rs:145` | `pub fn run` | the grader's check command |

Material of M1's exit step: `examples/pilot-pure/`.

Proposed: `examples/pilot/` (one project per task: statement, reference
solution, hidden tests, controls); `examples/pilot/grade.ps1`.

## Files owned

- `examples/pilot/`
- `docs/roadmap/v0.0.2/m2/pilot/references.md`

## Ordered tasks

1. For each of the ten tasks, a project under `examples/pilot/<task>/`:
   the statement as registered, the reference solution, the hidden tests,
   and the negative controls. The effectful tasks take every grant from
   `TestWorld`. Invariant: a task's tests decide pass or fail without a
   person.
2. `grade.ps1 <task> <arm> <file>`: applies the registered translation
   for the second arm, places the file, runs `vibra check` and `vibra
   test` in JSON format, and prints one line: task, arm, `passed`,
   `refuted`, `unknown`, or `unsupported`, and the diagnostic codes.
   Invariant: the script reads no network and writes only under a
   temporary directory.
3. Run the grader on every reference solution in both arms and on every
   control; write `references.md` with the table of results.
4. A reference that fails, or a control that passes, is fixed in the task
   by a dated amendment of the registration, never by changing the
   grader's rule for one task.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| each reference, first arm | positive | `grade.ps1 <task> endlet <reference>` for the ten tasks | `passed` | README: "Reference solutions and negative controls pass before any model call" |
| each reference, second arm | positive | the reference written in the second arm's `let` form | `passed` | the same |
| each negative control | negative | every control of every task | `refuted`, with the diagnostic or test failure the task registers | the same |
| a solution that is not source | negative | an empty file; prose; a file with a Markdown fence | `refuted`, never a crash of the grader | README: "graded by this milestone's checker and test world" |
| a solution that asks for authority | negative | a solution whose tests are replaced, or that adds a target with recipes | `refuted`: the grader restores the task's tests and project file before running | 04: "Selecting a test is consent to run it in its fake world only" |
| a solution that does not end | boundary | a solution that recurses without end | `unknown`, by the memory limit | 05 Limits: "Reaching a tool limit is an unknown outcome" |
| translation is local | positive | a first-arm reference passed through the second arm's translation | unchanged | the registration's translation rule |

## Diagnostics and schema changes

None.

## Validation

```powershell
cargo build --locked --offline -p vibra-cli --bin vibra
pwsh -NoProfile -File examples/pilot/grade.ps1 -All
```

Read: the first exits 0; the second prints one line per reference and
control and exits 0 only when every reference is `passed` and every
control `refuted`.

## Stays unavailable

Unchanged.

## Excluded

Calling a model; tuning a prompt.

## Completion evidence

- [ ] `pilot/references.md` holds the full table, with the toolchain
      commit.
- [ ] Every amendment of the registration made in this step is dated and
      precedes the runs of step 40.
- [ ] No model was called.
