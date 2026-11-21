# Step 25: entry-and-admission

Kind: vertical. Slice 5, first part: a binary target is checked. Its entry
is resolved and held to the entry signature, and its `effects` list is the
admission ceiling of the entry's row. Recipes are step 26; until then a
target with a grant leaf stays unavailable.
Depends on: 24.
Crates: `vibra-types`, `vibra-workspace`, `conformance/cases`.

## Prerequisites

- Read: [programs
  chapter](../../../../spec/04-programs-and-packages.md#targets) "Targets"
  up to "`grants` is a dict of binding recipes"; [effects
  chapter](../../../../spec/03-effects.md#target-admission) "Target
  admission"; [runtime
  chapter](../../../../spec/06-runtime.md#entry-outcome) "Entry outcome",
  Startup, the last item; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md#availability-discovery-and-resolution)
  the rows of `@project.entryOutsideTarget`, `@project.entryOnLibrary`,
  `@project.invalidEntrySignature`; [questions](../spec-questions.md) Q20,
  Q27.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-workspace/src/project.rs:490` | `TargetKind::Bin => Availability::Unavailable` | a binary target becomes available |
| `crates/vibra-workspace/src/project.rs:374` | `fn target` | the target schema with `entry`, `effects`, `grants` |
| `crates/vibra-workspace/src/semantic.rs:69` | `pub fn check_snapshot` | calls `entry::check_target` per binary target |
| `crates/vibra-types/src/check/lower.rs:868` | `pub(crate) fn check_program` | exposes the entry's declaration to `effects::entry` |

Proposed: `vibra-types::check::effects::entry::{entry_signature,
EntrySignature, GrantShape, LeafType}`; `vibra-workspace::entry::
{check_target, EntryFacts}`.

## Files owned

- `crates/vibra-types/src/check/effects/entry.rs`
- `crates/vibra-types/src/check/tests/entry.rs`
- `crates/vibra-types/src/lib.rs`
- `crates/vibra-workspace/src/entry.rs`
- `crates/vibra-workspace/src/project.rs`
- `crates/vibra-workspace/src/semantic.rs`
- `crates/vibra-workspace/tests/it/entry.rs`
- `conformance/rules/m2-25.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `entry`'s first part MUST be the target's name
   (`@project.entryOutsideTarget` at the field); on a library it is
   `@project.entryOnLibrary`. Invariant: naming an entry makes no import
   edge and changes no visibility.
2. Resolve the path: another kind of entity is `@name.wrongEntityKind`; a
   path that resolves to nothing is `@name.unknownSymbol`, both at the
   field.
3. `entry_signature`: a module-level `proc`; every parameter a bare name,
   none labelled; every parameter type a grant shape (an operation
   interface value type, `Option` of one, a record whose fields are grant
   shapes); the result `@std.process.ExitCode`; no bare `requires`. Any
   failure is one `@project.invalidEntrySignature` at the field, related
   to the declaration. It returns the leaves, each with its atom
   spelling, interface, and whether it is optional.
4. The ceiling: resolve each `effects` entry as a row entry in data form
   (Q27). The entry's written row MUST be contained in it:
   `@effect.outsideCeiling` at the `effects` field, related to the
   entry's `performs` entry, once per interface.
5. With no leaf, an `effects` entry that the entry does not perform is
   `@contract.unusedEffect` at that entry. With leaves the warning waits
   for step 26, which knows the startup rows.
6. A target with at least one leaf reports `@tool.unavailable` at its
   `grants` field until step 26. Invariant: no target is accepted with
   unvalidated recipes.

## Positions

| Rule | Positions |
| --- | --- |
| the entry resolves | module-level `proc`; private; not named `main`; in a nested module of the target |
| entry signature | each parameter form; each grant shape; the result; guards |
| ceiling | equal; entry row smaller (warning); entry row larger |

## Test matrix

`workspaceCheck`, profile `staticV2`; a tree with `project.vibon` and
`src/app/main.vib`. `T(entry effects)` abbreviates the target
`{name: @app kind: @bin root: "src/app" entry: … effects: […] grants:
[:]}`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-PROJECT-entry-minimal` | positive | `proc main returns code: ExitCode do return ExitCode(0u8) endproc`; `T(@app.main.main [])` | accepted | 04 Targets: "It MUST resolve to a module-level `proc` with an entry signature" |
| `V2-PROJECT-entry-private-other-name` | positive | the same `proc` named `start`, not public | accepted | 04: "The entry need not be public or named `main`" |
| `V2-PROJECT-entry-outside-target` | negative | `entry: @other.main.main` | `@project.entryOutsideTarget` at the `entry` field | 04: "whose first part MUST be the target's own name" |
| `V2-PROJECT-entry-on-library` | negative | a `@lib` target with `entry` | `@project.entryOnLibrary` at the field | 04: "An `entry` on a library is `@project.entryOnLibrary`" |
| `V2-PROJECT-entry-unknown` | negative | `entry: @app.main.nope` | `@name.unknownSymbol` at the field | 07: "a name that resolves to nothing … an entity reference slot" |
| `V2-PROJECT-entry-is-fn` | negative | `main` declared as a `fn` | `@project.invalidEntrySignature` at the field, related to the declaration | 07: "an `entry` that is not a module-level `proc` with an entry signature" |
| `V2-PROJECT-entry-is-type` | negative | `entry` names a type | `@name.wrongEntityKind` at the field | 04: "a path that resolves to another kind of entity is `@name.wrongEntityKind`" |
| `V2-PROJECT-entry-type-member` | negative | `entry` names a `proc` member of a type | `@project.invalidEntrySignature` | 04: "a module-level `proc`" |
| `V2-PROJECT-entry-result-not-exit-code` | negative | `returns code: U8` | `@project.invalidEntrySignature` | 04: "the result type is `ExitCode`" |
| `V2-PROJECT-entry-labelled-parameter` | negative | `labelled {verbose: Bool be false}` | `@project.invalidEntrySignature` | 04: "with no labelled parameters" |
| `V2-PROJECT-entry-pattern-parameter` | negative | `takes {{reader: r writer: w}: {reader: DirReadText writer: DirWriteText}}` | `@project.invalidEntrySignature` | 04: "every parameter is a bare name" |
| `V2-PROJECT-entry-parameter-not-grant` | negative | `takes {name: Str}` | `@project.invalidEntrySignature` | 04: "every parameter type is a grant shape" |
| `V2-PROJECT-entry-parameter-brand` | negative | `takes {logs: LogDir}` | `@project.invalidEntrySignature` | 04: "A declared type over a grant … is not an entry parameter type" |
| `V2-PROJECT-entry-parameter-tuple` | negative | `takes {pair: {DirReadText DirList}}` | `@project.invalidEntrySignature` | 04: "A tuple of grants is not one either" |
| `V2-PROJECT-entry-parameter-list` | negative | `takes {dirs: List(DirReadText)}` | `@project.invalidEntrySignature` | 04, the grant shapes |
| `V2-PROJECT-entry-option-of-record` | negative | `takes {logs: Option({reader: DirReadText})}` | `@project.invalidEntrySignature` | 04: "`Option` of one" |
| `V2-PROJECT-entry-bare-requires` | negative | `requires true` on the entry | `@project.invalidEntrySignature` | 04: "and a bare `requires` on the entry" |
| `V2-PROJECT-entry-requires-else` | positive | `requires true else ExitCode(2u8)` | accepted | 06: "A `requires … else` and either form of `ensures` behave as on any `proc`" |
| `V2-PROJECT-entry-generic` | negative | `proc main(T)` | `@project.invalidEntrySignature` | 04: every parameter type is a grant shape; a type parameter is none |
| `V2-EFFECT-entry-ceiling-equal` | positive | an entry with `performs {Log}` over a module-level model; `effects: [@app.main.Log]` | accepted | 04: "MUST be contained in it" |
| `V2-EFFECT-entry-outside-ceiling` | negative | the same with `effects: []` | `@effect.outsideCeiling` at the `effects` field, related to `Log` in the entry's `performs` | 07: "an entry row or startup step row outside the target's `effects`" |
| `V2-CONTRACT-unused-effect-target` | positive with warning | `effects: [@app.main.Log]`, an entry with the empty row | accepted; `@contract.unusedEffect` at that entry of the list | 07: "an `effects` entry that neither the entry nor a startup step performs" |
| `V2-EFFECT-target-effects-not-interface` | negative | `effects: [@std.fs.FsError]` | `@effect.invalidReference` at the entry (Q27) | 04: "a list of entity references to operation interfaces" |
| `V2-EFFECT-target-effects-unknown` | negative | `effects: [@std.fs.Nope]` | `@effect.invalidReference` at the entry (Q27) | the same |
| `V2-EFFECT-target-effects-repeat` | negative | `effects: [@app.main.Log @app.main.Log]` | `@effect.duplicateRoot` at the second (Q27) | 04: "without repeats" |
| `V2-TOOL-unavailable-entry-with-grants` | boundary | the chapter's `main` with three leaves and well-formed recipes | `@tool.unavailable` at the `grants` field; verdict `unsupported` | 07: "a valid … construct outside the tool's profile" |

`V2-PROJECT-workspace-check-bin-unavailable` of M1 is deleted.

## Diagnostics and schema changes

Codes: `@project.entryOutsideTarget`, `@project.entryOnLibrary`,
`@project.invalidEntrySignature`, `@name.wrongEntityKind`,
`@name.unknownSymbol`, `@effect.outsideCeiling`, `@effect.invalidReference`,
`@effect.duplicateRoot`, `@contract.unusedEffect`, `@tool.unavailable`. No
schema change: `@project.v2` already holds the three fields.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-workspace -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0.

## Stays unavailable

`@tool.unavailable`: a target with any grant leaf; dependencies; `run`.

## Excluded

Recipes (26); running the entry (29).

## Completion evidence

- [ ] `conformance/rules/m2-25.tsv` has one `active` row per case id.
- [ ] Every position has a positive and a negative case.
- [ ] `V2-TOOL-unavailable-entry-with-grants` shows the fail-closed
      boundary.
- [ ] The validation commands exit 0.
