# Step 38: pilot-freeze

Kind: evidence. The model-writing pilot, first part: the registration is
frozen against the exited milestone before any model is called.
Depends on: 37.
Crates: none.

## Prerequisites

- M2 is exited ([step 37](37-m2-exit.md)).
- Read: this milestone's [README](../README.md) "The model-writing
  pilot"; [execution model](../../../execution.md); the registered pilot
  draft that [M0](../../m0/README.md) keeps current; [roadmap
  index](../../../README.md) on what a pilot is evidence of.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-cli/src/lib.rs:82` | `fn execute` | the grader's only interface: `check` and `test` in machine mode |

Material of M1's exit step: `examples/pilot-pure/`, the reference
solutions and controls of the pure tasks.

Proposed: `docs/roadmap/v0.0.2/m2/pilot/registration.md`.

## Files owned

- `docs/roadmap/v0.0.2/m2/pilot/registration.md`

## Ordered tasks

1. Copy the registered draft into `registration.md` and bring it to the
   exited specification. It states: the one question (whether an ordinary
   `let` ends with `endlet`); the two arms, which differ in nothing else;
   the ten tasks by name; the two model families and exact model
   identifiers; the number of attempts per task, arm, and model; the
   prompt each arm receives, whole; the grader (this milestone's `check`
   and `test`, at a named commit); the outcome measure; the decision
   rule, with its threshold; and what is excluded from the analysis and
   why. Invariant: nothing in it can be chosen after a result is seen.
2. State how the second arm is graded: a mechanical translation of its
   `let` form to the specified one before `check`, defined in the
   registration, applied to nothing else.
3. State the budget: calls, tokens, and the stop rule when a provider
   fails.
4. Record the commit of the toolchain, the digest of each prompt, and
   the date. From this commit the registration changes only by a dated
   amendment that says what changed and why, written before the affected
   runs.

## Test matrix

Checks of the registration itself, done by reading and recorded in the
step's report.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| one difference | positive | the two prompts, compared line by line | they differ only in the `let` form and the examples that show it | README: "two surface arms that differ only in whether an ordinary `let` ends with `endlet`" |
| ten tasks | positive | the task list | ten, each with a statement, a reference solution, hidden tests, and negative controls | README: "ten tasks" |
| two families | positive | the model list | two families, exact identifiers | README: "two model families" |
| the rule is decidable | positive | the decision rule applied to three invented result tables | each gives one answer | README: "under its registered rule" |
| an unregistered choice | negative | a draft that leaves the attempt count open | the freeze is refused until it is fixed | execution model: evidence is registered before it is gathered |
| grader identity | positive | the named commit | it is the exit commit of step 37 or later | README: "graded by this milestone's checker and test world" |

## Diagnostics and schema changes

None.

## Validation

```powershell
git diff --check
git log -1 --format=%H -- docs/roadmap/v0.0.2/m2/pilot/registration.md
```

Read: the first prints nothing; the second prints the freeze commit, which
the registration names.

## Stays unavailable

Unchanged.

## Excluded

Calling a model; changing the specification.

## Completion evidence

- [ ] `pilot/registration.md` is committed and names its own freeze
      commit in the following commit's report.
- [ ] The six rows above are answered in the report.
- [ ] No model was called.
