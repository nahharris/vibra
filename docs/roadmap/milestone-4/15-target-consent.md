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
   `@name.wrong-entity-kind`), `@effect.duplicate-root`, and the existing
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

No new registry code (Step 13 added it as `@project.duplicate-effect-root`; Step 14 renamed it `@effect.duplicate-root` and made it the code of a repeated entry in every written row). No schema
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

## As built

Landed as `landed`, conditional on its pull request merging; the base is Step 14
(PR #372, branch `claude/m4-step-14-effect-rows` at `f942853`, open when this step
was written), so the branch is rebased onto `m4` once that pull request merges.

- **Resolution** (`vibra-resolve`). `SourceUnit::with_effects` carries the atoms of a
  target array with the source identity and span of each, and
  `ResolvedSnapshot::target_effects` returns one `ResolvedTargetEffect` for every
  atom in written order, holding the canonical `DeclarationId` of the root, or none
  for an atom that was reported. The pass runs beside `resolve_entries`, so the
  project document is a privileged referrer and creates no import edge. An atom is
  walked like an entry: the first component is a unit, the longest module prefix is
  taken, and the rest is a declaration. No module is `@module.unknown-path`, no
  declaration is `@name.unknown-symbol`, a module, type, function, or operation is
  `@name.wrong-entity-kind` naming the kind found, a second atom for the same root
  is `@effect.duplicate-root` relating the first, and the `@tests` unit and
  `@std.builtin` are `@module.unknown-path`, as an import of them from a module that
  is not a test is. A bad atom or a duplicate stays in the list without a root, so
  its diagnostic is in the span set that a selected target admits.
- **Admission** (`vibra-types`, `effects.rs`, `resolved.rs`). `check_resolved` builds an
  `EntryConsent` for each resolved entry whose function it checked, from the entry's
  function index and the roots of its array, and `Graph::admit` compares the
  performed row of that node with the array. The entry's own ceiling is the Step 14
  check of the same node, so there is one computation and no second row. The
  single-source checker passes no entry. Admission runs in every caller of
  `check_resolved` (the workspace check, `run`, the index, and queries), and the
  test runner, which filters by the sources of its modules, never reports it.
- **The witness at `entry:`.** One `@effect.outside-ceiling` for each root the array
  omits, in canonical root order, whose primary span is the `entry:` value in
  `project.vibon` with no note and whose related spans are every hop of the Step 14
  chain, the first included, each noted `performs <root> via <callee>`
  (`witness` is now a function of its own that both diagnostics call). The
  `entry:` value is not a call site, so the first hop cannot be the primary span.
- **Unused roots.** `@contract.unused-effect` (a warning) at each atom whose root the
  entry's performed row does not contain, skipped when the entry reaches a function
  that did not check.
- **The workspace layer.** `check_resolved_scope` admits the atoms of the selected
  units into the project-document spans it keeps, as it does for the entry, so the
  atoms of a target that is not selected are not reported under `check TARGET`.
- **Formatter.** `canonical_target` writes the array in the byte order of its atoms,
  which is the canonical order of the roots, and keeps the field order of the
  schema.
- **Tests.** No code changed for effectful tests: Step 14 already checks a test
  against its written row and the test runner never consults a target. The text of
  the projects chapter that said consent was not yet granted is replaced, and
  `target_consent_m4_step15` pins that an effectful test and a pure test run under
  an empty array and that a test over its row is a source error.
- **`project init`.** The template already wrote `effects: (array)`; the tooling
  chapter's init paragraph now says so, and `init_consent_m4_step15` pins the
  generated text and that the generated target is admitted with no diagnostic.

### Rulings on text the specification was silent about

Each is canonical prose in [the effects chapter](../../spec/03-effects.md) now:

- A target atom is resolved like an `entry:`: written canonically (`project.vibon`
  has no imports), by a unit name, privileged (a non-public root may be named,
  `V1-EFFECT-target-array-private-root`), and unable to name the reserved `@tests`
  unit (`V1-PROJECT-workspace-check-effects-tests-unit`).
- The array is a set. Its order has no meaning and the formatter writes it in
  canonical order (`V1-EFFECT-target-array-any-order`).
- The `entry:` value is not a hop, so the witness of an omitted root has no primary
  note and every hop is a related span
  (`V1-EFFECT-demo-gate-target-array-omits`, `V1-EFFECT-target-array-omits-both`).
- A root that only an unresolved atom meant to name is omitted from the array and
  is reported by admission as well as by the atom's own diagnostic.
- An entry over its own ceiling and over the array is reported twice, once at its
  call sites and once at the `entry:` value, since the two consents are separate
  (`V1-EFFECT-target-array-entry-over-own-ceiling`).
- The wrong-kind and unknown-root diagnostics of an atom relate no span, as the
  diagnostics chapter's table says.

### Cases

- **Added, static (16):** `V1-EFFECT-target-array-additive-accepted`,
  `-demo-gate-function-ceiling-omits`, `-demo-gate-target-array-omits`,
  `-target-array-omits-both`, `-target-array-empty-pure-entry`,
  `-target-array-unused-root`, `-target-array-any-order`,
  `-target-array-entry-over-own-ceiling`, `-target-array-own-module-root`,
  `-target-array-private-root`, and `V1-PROJECT-workspace-check-effects-unknown-module`,
  `-unknown-root`, `-wrong-kind`, `-duplicate-root`, `-tests-unit`, and
  `V1-PROJECT-static-library-effects`. No case was re-expected, deleted, or
  downgraded; the existing `V1-RUNTIME-workspace-run-effect-operations` already lists
  the complete array, and no existing static case had an entry that performs a
  root outside an empty array.
- **Executable:** none added; the demo gate's accepting side is the existing
  `V1-RUNTIME-workspace-run-effect-operations`, which still matches in Wasm.

### Evidence

- Corpus at the head: 83 reader, 283 static, 143 interpreter, and 16 tooling cases,
  525 passed, 0 failed, 0 unavailable; the interpreter backend passes 135 cases and
  the Wasm backend matches 135, 0 not lowered (509 and 135 at the base).
- Host tests: `target_consent_m4_step15` in `vibra-workspace` (6: a bad atom or an
  unadmitted target does not stop its sibling, the witness at the `entry:` value, an
  unused root is a warning and the target still runs, a test is consent to its own
  row under an empty array in both directions, and the formatter's canonical array
  order) and `init_consent_m4_step15` in `vibra-cli` (1).
- Re-expected: the inventory of specification examples
  (`docs/roadmap/milestone-1/syntax-examples.tsv`) follows the line shifts of the
  edited chapters: 21 rows changed, 17 of them only in their line numbers and 4 in
  their inline counts as well.

### Left for Step 16a

The host provider infrastructure: the standard effect modules do not exist yet, so
no array can name an `@std` root and no effectful test can call a host operation.
The `@host-world.v1` decoder, the default world, and the registry table are Step
16a, and the first array that names `@std.io.stdout` is Step 16b.
