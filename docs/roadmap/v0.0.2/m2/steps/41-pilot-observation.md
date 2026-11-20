# Step 41: pilot-observation

Kind: evidence. The model-writing pilot, last part: the registered analysis
and its dated observation about one surface choice on these tasks and
models.
Depends on: 40.
Crates: none.

## Prerequisites

- The registration, `pilot/registration.md`, and
  the run table, `pilot/runs.tsv`.
- Read: this milestone's [README](../README.md) "The model-writing
  pilot"; [roadmap index](../../../README.md) on what a pilot is evidence
  of.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-cli/src/lib.rs:82` | `fn execute` | unchanged; the analysis runs no tool |

Proposed: `docs/roadmap/v0.0.2/m2/pilot/observation.md`;
`examples/pilot/analyze.ps1`.

## Files owned

- `docs/roadmap/v0.0.2/m2/pilot/observation.md`
- `docs/roadmap/v0.0.2/m2/README.md`

## Ordered tasks

1. `analyze.ps1` computes the registered outcome measure from `runs.tsv`
   alone: per arm, per model, per task, and overall, with the registered
   exclusions applied and counted. Invariant: the script takes no
   parameter that changes the measure.
2. Apply the registered decision rule and state its answer in one
   sentence.
3. `observation.md`: the date; the models and tasks; the table; the
   rule's answer; every exclusion with its count; the failure kinds per
   arm by diagnostic code; and what the observation does not show (other
   models, other tasks, other surface choices, people).
4. If the rule favours the arm without `endlet`, open the change as a
   proposal against the specification and link it. The specification is
   not edited by this step.
5. The README's pilot paragraph gains one sentence with the date and a
   link to the observation.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| the measure from the table | positive | `analyze.ps1` on `runs.tsv` | the table of `observation.md`, number for number | README: "The result is a dated observation" |
| the rule on the result | positive | the registered rule applied to that table | one of its registered answers | README: "under its registered rule" |
| the rule on a tie | boundary | the rule applied to a table with equal arms, built by hand | the registered answer for no difference | the registration |
| exclusions counted | positive | every excluded attempt | listed with its registered reason; the totals add up to the schedule | the registration |
| an unregistered cut | negative | an analysis by a split the registration does not name | absent from the observation, or in a section titled as exploratory and outside the rule | execution model: evidence is registered before it is gathered |
| no wider claim | negative | the observation's text searched for a statement about models or tasks outside the pilot | none | roadmap index: "it is never a release gate" |

## Diagnostics and schema changes

None.

## Validation

```powershell
pwsh -NoProfile -File examples/pilot/analyze.ps1 docs/roadmap/v0.0.2/m2/pilot/runs.tsv
git diff --check
```

Read: the first prints the table that `observation.md` holds; the second
prints nothing.

## Stays unavailable

Unchanged.

## Excluded

Editing the specification; a second pilot.

## Completion evidence

- [ ] `pilot/observation.md` is committed with its date and the commit of
      the run table.
- [ ] The rule's answer is one sentence, and the proposal is linked when
      the rule asks for one.
- [ ] The README links the observation.
