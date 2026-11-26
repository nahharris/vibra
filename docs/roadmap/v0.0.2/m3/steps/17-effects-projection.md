# Step 17: effects-projection

Kind: vertical. The `effects` member of a query: the enclosing body's
ceiling and performed row, the application's immediate and latent rows and
its await site, the target's reviewed recipes, the authority known at the
position, and the `effectWitnesses` expansion with the same chain
`@effect.outsideCeiling` reports; every effect fact finalized after the
body's verdict.
Depends on: 12.
Crates: `vibra-workspace`, `vibra-schema`, `conformance/cases`.

## Prerequisites

- Read: [05 Effect and authority projections](../../../../spec/05-tooling.md#effect-and-authority-projections);
  [07 Contract, effect and authority diagnostics](../../../../spec/07-diagnostics-and-conformance.md#contract-effect-and-authority-diagnostics)
  (the witness chain); [03 Effect ceilings](../../../../spec/03-effects.md#effect-ceilings);
  [architecture](../architecture.md) rows I14, I18; [questions](../spec-questions.md) Q35.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-types/src/check/effects/analysis.rs:109` | `pub(crate) struct Performed` | the performed row of a body |
| `crates/vibra-types/src/check/effects/analysis.rs:454` | `pub(crate) fn chain` | the witness chain; made reachable to the facts of step 08 by a `pub(crate)` export recorded as a fact |
| `crates/vibra-types/src/check/effects/entry.rs:190` | `pub struct EntrySignature` | the entry's row and leaves |
| `crates/vibra-workspace/src/binding.rs:57` | `pub struct StartupStep` | `startupOperations` |
| `crates/vibra-host/src/recipe.rs:482` | `pub struct BindingPlan` | `bindings` and `possibleOperations` |

Proposed: `query::effects::{effects, witnesses}`, filled into the
dispatcher's `effects` and `effectWitnesses` entries.

## Files owned

- `crates/vibra-workspace/src/query/effects.rs`
- `crates/vibra-workspace/tests/it/query_effects.rs`
- `crates/vibra-cli/tests/it/query_effects.rs`
- `crates/vibra-schema/schemas/v2/query/effects.json`
- `crates/vibra-schema/schemas/v2/query/effect-witnesses.json`
- `conformance/rules/m3-17.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `enclosing`: `{identity, kind, ceiling, performed}` for the declaration
   that holds the position, `kind` one of `function`, `proc`,
   `operation`, `anonymousFunction`, `test`; an operation's ceiling
   includes its owner; a rejected body's `performed` is `unavailable`,
   since the facts of step 08 finalize it after the verdict (I18). This
   step adds no computation.
2. `application`: `{immediate, latent, awaitSite}` as Q35 fills them.
3. `target`: `{entry, bindings, possibleOperations, startupOperations}`
   when the enclosing is a target's entry, from the `BindingPlan`; else
   `null`. `bindings` are projections (leaf, interface, provider, policy
   hash, limits), never grants.
4. `authority`: one entry per grant-typed parameter or local visible,
   `exact` for an entry parameter with its leaf, else `unavailable` (Q35).
5. `witnesses` under `--expand effectWitnesses`: `{root, hops}` per root
   in identity order, each hop `{span, callee: {kind, identity}}`, the same
   chain and tie-breaks as the diagnostic's related spans; for a body with
   no `@effect.outsideCeiling`, the chain to each performed root.

## Test matrix

Cases are `query` with `--include effects`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-query-effects-enclosing-proc` | positive | inside `copyNote` of the effects chapter | `enclosing {kind: "proc", ceiling: [DirReadText, DirWriteText], performed: the same}` | 05: "`null` or `{identity, kind, ceiling, performed}`" |
| `V2-TOOL-query-effects-enclosing-operation` | positive | inside a `proc` member of an operation interface with an additive row | `ceiling` includes the owner | 05: "An operation's ceiling includes its owner" |
| `V2-TOOL-query-effects-enclosing-test` | positive | inside a test with `performs {TestWorld DirReadText}` | `kind: "test"` and the row | 05 |
| `V2-TOOL-query-effects-application-plan` | positive | at `DirWriteText.write(writer at text)` not awaited | `application {immediate: [], latent: [DirWriteText], awaitSite: null}` | 05: "Building a plan performs none of its latent row" |
| `V2-TOOL-query-effects-application-awaited` | positive | at the same inside `await` | `awaitSite` the `await`'s span | Q35 |
| `V2-TOOL-query-effects-target` | positive | inside the entry of the gate program | `target {entry, bindings: [...], possibleOperations, startupOperations}` with the `@notes.reader` read as a startup operation | 05: "the reviewed recipes and drivers and the operations they allow, never a blanket permission" |
| `V2-TOOL-query-effects-target-null` | positive | inside a library `proc` | `target: null` | 05 |
| `V2-TOOL-query-effects-authority-entry` | positive | at a use of the entry's `notes` parameter | `authority` entry with `leaf: "@notes.reader"`, the policy hash, `status: exact` | Q35 |
| `V2-TOOL-query-effects-authority-unknown` | positive | at a grant parameter of a library `proc` | `status: unavailable`, `leaf: null` | 05: "which resource it holds may be unknown" |
| `V2-TOOL-query-effects-witnesses` | positive | `--expand effectWitnesses` in a `proc` whose body performs `DirWriteText` through `emit` through `sink` | `witnesses [{root, hops: [{span, callee: {kind: "proc", identity}}, {span, callee: {kind: "functionValue"}}]}]` equal to the diagnostic's chain on the negative twin | 05: "A witness is the same chain, with the same tie-breaks, that `@effect.outsideCeiling` reports" |
| `V2-TOOL-query-effects-witness-indirect` | positive | a chain through a callback | the hop's callee kind `functionValue`; the diagnostic's note ends ` (indirect)` | 07: "A hop whose callee is not statically known is marked indirect" |
| `V2-TOOL-query-rejected-body-no-rows` | positive | a position in a body rejected for a type error | `enclosing.performed` unavailable; `witnesses` empty; `enclosing.kind` and `identity` exact | 05: "A rejected body never shows exact rows or proofs"; 05: "Each semantic field is `{status, value}`" |
| `ATK-query-authority-is-data` | negative | the host test: every field of `authority` and `target` is a string, number, list, or record; no token type is reachable from `query::effects` | asserted by type and by a source scan for `GrantToken` | 05: "It grants nothing" |
| `V2-TOOL-query-effects-await-report` | positive | at an `await` of a host member in the entry | the operation, its delivery class `@once`, the driver bound for the target, the policy in plain terms | 05: "For each `await` a report shows the operation, its delivery class, the driver bound for the target, and the policy in plain terms" |
| JSON validates | positive | every envelope | validates | 05 |

## Diagnostics and schema changes

Codes: none. Schema: the `effects` member and the `effectWitnesses`
expansion of the query schema, one file each.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0. If `chain` needs a `pub(crate)` export in
`analysis.rs`, that one edit is listed in the report (the file is owned by
no M3 step).

## Stays unavailable

The expansions of step 25; the index.

## Excluded

References, members, calls, applications, declarations (25).

## Completion evidence

- [ ] `conformance/rules/m3-17.tsv` has one `active` row per case id.
- [ ] `ATK-query-authority-is-data` was seen to fail with a `GrantToken`
      field added; the report says how.
- [ ] The validation commands exit 0.
