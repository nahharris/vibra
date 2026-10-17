# Step 15 — target consent and effectful tests

Prerequisite: Step 14 merged. Stage 4B behavior step: the required `effects` array
of a binary target, its diagnostics, static admission of the resolved entry, effectful
test ceilings, and the `project init` template. No host operation runs yet.

## Read before editing

- [Effects](../../spec/03-effects.md): **Target consent** and **The witness**.
- [Projects](../../spec/04-programs-and-packages.md): **Packages and targets** (the
  entry and `effects` rules), **M2 `@project.v1` schema**, **Tests** (effectful
  tests and the host world), **Modules and imports**.
- [Tooling](../../spec/05-tooling.md): the `check`, `run`, and `test` contract and
  **Execution backend of `run` and `test`**.
- [Diagnostics](../../spec/07-diagnostics-and-conformance.md): **Effect and host
  diagnostics**.
- [Ledger](decision-ledger.md) rows D23.1, D23.2, D20.1.

## Scope

1. Resolution of each target `effects` atom to a canonical effect root through the
   ordinary resolution diagnostics (`@module.unknown-path`, `@name.unknown-symbol`,
   `@name.wrong-entity-kind`), `@project.duplicate-effect-root`, and the existing
   `@data.invalid-shape` for a missing field.
2. Static admission: the entry's performed row against the array
   (`@effect.outside-ceiling` at the `entry:` value, witness as related spans) and
   against its own ceiling; `@contract.unused-effect` at an unused array atom.
3. Effectful tests: a test writes its complete `effects:` row, the runner admits
   it, and a pure test may omit it. An effectful test runs in the default host
   world, which has no host operation to call until Step 16b.
4. The `project init` template and its documentation: the generated target keeps
   `effects: (array)`.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-workspace/src/project.rs` (`TARGET_FIELDS`, the target `effects` decode near line 1151) | The decoded, unresolved atoms; resolution is the new step |
| `crates/vibra-workspace/src/semantic.rs` | Admission before `check`, `test`, `run`; where the entry's signature is already checked |
| `crates/vibra-workspace/src/test_runner.rs` | Selecting and admitting an effectful test; the declaration-dependency closure |
| `crates/vibra-cli/src/init.rs` | The `project init` template |
| `crates/vibra-types/src/effects.rs` (Step 14) | The performed row and witnesses the admission reuses |

## Ordered tasks

1. Resolve a target's effect atoms in the typed `project.vibon` schema slot that
   already declares the role, producing canonical identities, in the same pass that
   resolves `entry`. Emit the diagnostics at the atom spans with `project.vibon`
   as the source identity.
2. Compare the entry's performed row (from Step 14) with the array and with the
   entry's ceiling; build the witness from the entry declaration.
3. Warn on an unused array atom with `@contract.unused-effect`.
4. Admit effectful tests and remove the M2 unavailable path for nonempty test
   ceilings; the test's row is checked against its body like any ceiling.
5. Update the `project init` template text and its tests; the entry still imports
   no standard-library module.
6. Corpus cases for each diagnostic, in `V1-EFFECT` and `V1-PROJECT`.

Invariants preserved: the project document is a privileged referrer and creates no
import edge; an empty array permits only an effect-free entry graph; a library
target declares no array; library roots are not consent.

## Test matrix

- Positive: a target whose array is exactly the entry's row with an operation
  reaching `fs.write` only through an additive row; an empty array with a pure
  entry; an effectful test with its row; a pure test with none.
- Negative: an unknown module, an unknown root, a type or a function named as a
  root, a duplicate root (second atom primary, first related), a root omitted from
  the array (witness through two hops), an entry over its own ceiling, a missing
  `effects` field, a library target with an array.
- Recovery: a bad atom does not stop the other atoms or the other targets from
  being checked; a valid sibling target still runs under `check TARGET`.
- Boundary: the array names a root from a dependency alias; an entry in another
  module of its own target; roots listed in an order other than canonical.
- Formatter: `project.vibon` canonical field and array order after `fmt`.

## Diagnostic and schema changes

No new registry code (Step 13 added `@project.duplicate-effect-root`). No schema
change.

## Validation

The [pre-merge list](validation.md#before-merging-each-step) and `cargo test
--locked --offline -p vibra-workspace -p vibra-cli -p vibra-conformance`.

## Excluded

Host operations and the host world's decoder (16a), program arguments (18),
`@command.program-failed` (19), query metadata of the target (20).

## Completion evidence

The exit clause that an additive root reaches the target array of every transitive
caller passes through a `workspace-check` case; the unavailable cases for a
nonempty target array are replaced.
