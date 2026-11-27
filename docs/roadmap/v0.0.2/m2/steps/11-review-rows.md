# Step 11: review-rows

Kind: evidence. The review gate of slice 1: an adversarial reading of rows,
plans, and ceilings against the effects chapter, before any grant exists.
It writes findings and changes no code.
Depends on: 10.
Crates: none.

## Prerequisites

- Steps 03 to 10 are merged.
- Read: [effects chapter](../../../../spec/03-effects.md) from "Operation
  interfaces" to "Cold plans and performed rows";
  [architecture](../architecture.md) section 2, rows A1 to A4, and the
  trust table.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-ir/src/types.rs:152` | `pub struct Row` | row equality and containment |
| `crates/vibra-ir/src/module_set.rs:529` | `pub fn try_new` | where the IR is validated |
| `crates/vibra-interp/src/machine.rs:1221` | `pub(crate) fn invoke` | what an `await` enters |

It reads `crates/vibra-ir/src/effects.rs`,
`crates/vibra-types/src/check/effects/`, and
`crates/vibra-interp/src/plan.rs`.

## Files owned

- `docs/roadmap/v0.0.2/m2/evidence/review-rows.md`

## Ordered tasks

1. For each sentence of the chapter sections above that states a MUST or a
   diagnostic, name the case that shows it, or record the gap. Invariant:
   a sentence without a case is a finding, never a note.
2. Try to break A1 to A4. For each probe below write a program or a
   hand-built IR, run it, and record the observed result beside the
   expected one.
3. Compare the checker's performed row with the IR validator's on every
   accepted corpus case of prefix `EFFECT`: a difference in either
   direction is a finding.
4. Classify each finding: defect (a guide of step 12 fixes it), question
   (added to the [questions](../spec-questions.md)), or accepted limit.

## Test matrix

Probes; each is run and its result recorded, pass or fail.

| Probe | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| row through a generic | negative | `proc call(T) takes {f: T}` cannot await `f`; a `Proc` type parameter bound through an alias with a wider row | a diagnostic, never acceptance | 03: "There are no row variables" |
| row through a type alias | negative | an alias of `Proc({} Void {Log})` used where `Proc({} Void {})` is expected | `@type.argumentMismatch` | 03: "Callable types compare rows exactly" |
| row through `Option` and `List` | negative | `Option(Proc({} Void {Log}))` unwrapped and awaited in a body with the empty row | `@effect.outsideCeiling` | 03 Cold plans: "or a stored callable" |
| row through an interface member default | negative | a default `fn` member returning a plan that a `proc` with the empty row awaits | `@effect.outsideCeiling` | the same |
| row through an associated type | negative | an associated type bound to a `Proc` type with a row, awaited at the interface | a diagnostic, never acceptance | 03: "A plan's type always carries its complete row" |
| await in a guard consequence | negative | `ensures … else await p()` on a `proc` | `@effect.invalidAwait` | 03 Cold plans |
| await in a compile-time context | negative | a module value whose initializer calls a `fn` that returns a plan, then awaits | `@effect.invalidAwait` | 03: "A module-value initializer is pure" |
| a plan as a module value | positive | a module value of an `Intent` type | accepted: building a plan is pure, and a module value needs no encoding | 03: "A module-value initializer is pure"; 01 Module values |
| a plan where a constant is required | negative | a labelled default that builds a plan | `@type.notConstant` | 01 `fn` and `proc`: "a default that MUST be a constant expression" |
| the IR accepts what the checker rejects | negative | each negative case of steps 05 to 10, lowered by hand | `IrError` | architecture, trust table |
| deep rows | boundary | a row of 1,000 interfaces; a chain of 10,000 procedures | accepted or a documented limit, no overflow | 07 Required coverage 13 |
| plan identity | positive | one plan awaited from two closures | two invocations | 06 Cold plans |

## Diagnostics and schema changes

None.

## Validation

```powershell
cargo test --locked --offline -p vibra-ir -p vibra-interp -p vibra-types --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
```

Read: both exit 0 on the unchanged tree; the probes are run from scratch
files outside the repository and their sources are quoted in the report.

## Stays unavailable

Unchanged.

## Excluded

Fixing anything; grants and policy, which gate 23 reviews.

## Completion evidence

- [ ] `evidence/review-rows.md` lists every probe with its source, the
      expected and observed result, and a verdict.
- [ ] Every finding has a class and, for a defect, the file and the
      sentence it violates.
- [ ] The report states the commit reviewed.
