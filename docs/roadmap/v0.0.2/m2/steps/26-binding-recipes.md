# Step 26: binding-recipes

Kind: vertical. Slice 4, static half: every grant leaf of an entry has a
valid recipe, the recipes form a `BindingPlan`, and the startup rows count
against the admission ceiling. A binary target with grants becomes
checkable.
Depends on: 14, 25.
Crates: `vibra-workspace`, `conformance/cases`.

## Prerequisites

- Read: [programs
  chapter](../../../../spec/04-programs-and-packages.md#targets) "Targets"
  from "`grants` is a dict of binding recipes" to its end; [effects
  chapter](../../../../spec/03-effects.md#target-admission) "Target
  admission", "The wire form"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md#contract-effect-and-authority-diagnostics)
  the six `@grant.*` rows and the two project rows of the ceiling;
  [questions](../spec-questions.md) Q9, Q20.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-workspace/src/project.rs:403` | `fn grant_binding` | the recipe schema; gains the closed resource kinds |
| `crates/vibra-workspace/src/project.rs:544` | `pub fn decode_project` | keeps field spans for the diagnostics |
| `crates/vibra-workspace/src/semantic.rs:79` | `pub fn check_snapshot` | calls `binding::check` after `entry::check_target` |

From step 14: `recipe::{BindingPlan, Leaf, BindingFault, PROVIDERS}`,
`policy::{decode, normalize, count}`. From step 25: `EntryFacts`.

Proposed: `vibra-workspace::binding::{check, plan_of}`.

## Files owned

- `crates/vibra-workspace/src/binding.rs`
- `crates/vibra-workspace/src/project.rs`
- `crates/vibra-workspace/src/semantic.rs`
- `crates/vibra-workspace/src/entry.rs`
- `crates/vibra-workspace/tests/it/binding.rs`
- `conformance/rules/m2-26.tsv`
- the case directories of the matrix below

## Ordered tasks

1. The keys of `grants` MUST be exactly the entry's leaves: a missing or
   extra key is `@grant.invalidBinding` at the `grants` field, related to
   the parameter.
2. Decode each recipe into a `Leaf`, with the span of each field. The
   resource kinds are closed records: `@fixedPath {path}`,
   `@argumentPath {index source}`, `@variablePath {name source}`,
   `@processStream`, `@variableSet {names}`, `@processArguments`,
   `@systemClock`, `@systemRandom`. `limits` with no field is
   `@data.invalidShape`.
3. `BindingPlan::validate`; map each `BindingFault` to
   `@grant.invalidBinding` at its field, a cycle to `@grant.bindingCycle`
   at the later edge related along the cycle.
4. Policies: `policy::decode` and the root rule; a failure is
   `@grant.invalidPolicy` at the policy value; over the limits,
   `@grant.policyTooComplex` at the policy.
5. Startup rows: an `@argumentPath` or `@variablePath` recipe is a
   startup step whose row is its `source` leaf's interface. Each MUST be
   in `effects`: `@effect.outsideCeiling` at the `effects` field, related
   to the recipe.
6. `@contract.unusedEffect` at an `effects` entry that neither the entry
   nor a startup step performs.
7. Remove the fail-closed rejection of step 25. `plan_of(target)` returns
   the validated `BindingPlan` for step 29.

## Positions

| Rule | Positions |
| --- | --- |
| keys equal leaves | plain parameter; `Option`; record field; nested record field |
| each provider | each of its grant types; each resource kind; a type and a kind it does not accept |
| `source` | present and right; absent; wrong kind; variable outside `names`; not in `after` |
| policy | present and rooted here; absent; another root; malformed; on another provider |

## Test matrix

`workspaceCheck`, profile `staticV2`. `M` is the chapter's `main` taking
`{args: EnvReadArgs notes: DirReadText out: Option(StdoutText)}` with
`effects` of the three interfaces and these recipes: `@args` from
`@host.env.args`; `@notes` from `@host.fs.directory` with `@argumentPath
{index: 0u64 source: @args}`, `after: [@args]`, and the policy `P` (a
`@fileRule` rooted at `@notes`, `@tree`, `[@read]`, suffix `.txt`); `@out`
from `@host.io.stdout`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-GRANT-binding-complete` | positive | `M` | accepted | 04 Targets, the recipe rules |
| `V2-GRANT-binding-missing` | negative | `M` without the `@out` recipe | `@grant.invalidBinding` at the `grants` field, related to the parameter | 04: "A missing or extra key is `@grant.invalidBinding`" |
| `V2-GRANT-binding-extra` | negative | `M` with a recipe `@err` | `@grant.invalidBinding` at the `grants` field | the same |
| `V2-GRANT-binding-optional-missing` | negative | an entry with only `out: Option(StdoutText)` and `grants: [:]` | `@grant.invalidBinding` | 04: "for a required and an optional leaf alike" |
| `V2-GRANT-binding-bundle-leaves` | positive | `logs: {reader: DirReadText writer: DirWriteText}` with keys `@logs.reader`, `@logs.writer` | accepted | 04: "named by the parameter and the field" |
| `V2-GRANT-binding-bundle-by-parameter` | negative | the same with one key `@logs` | `@grant.invalidBinding` at the `grants` field | 04: "The dict's keys MUST be exactly the entry's leaves" |
| `V2-GRANT-binding-each-provider` | positive | an entry with one leaf per grant type of the provider table, 17 leaves | accepted | 04, the provider table |
| `V2-GRANT-binding-provider-wrong-type` | negative | `@out` with `provider: @host.io.stderr` | `@grant.invalidBinding` at `provider` | 04: "MUST be one of the issuer's grant types" |
| `V2-GRANT-binding-provider-unknown` | negative | `provider: @host.net.socket` | `@grant.invalidBinding` at `provider` | 04: "The table is closed" |
| `V2-GRANT-binding-resource-kind` | negative | `@out` with `resource: {kind: @fixedPath path: "x"}` | `@grant.invalidBinding` at `resource` | 04: "of a kind the issuer accepts" |
| `V2-GRANT-binding-source-missing` | negative | `@notes` with `@argumentPath {index: 0u64}` | `@grant.invalidBinding` at `resource` | 04: "names in `source` the leaf whose grant performs that read" |
| `V2-GRANT-binding-source-wrong-kind` | negative | `source: @out` | `@grant.invalidBinding` at `source` | 07: "a `source` leaf of the wrong kind" |
| `V2-GRANT-binding-variable-not-in-names` | negative | `@variablePath {name: "HOME" source: @vars}` where `@vars` has `names: ["TMP"]` | `@grant.invalidBinding` at `name` | 04: "whose `names` include the variable" |
| `V2-GRANT-binding-after-lacks-source` | negative | `@notes` with `after: []` | `@grant.invalidBinding` at `after` | 04: "It MUST contain the recipe's `source` leaf" |
| `V2-GRANT-binding-after-unknown` | negative | `after: [@args @nope]` | `@grant.invalidBinding` at `after` | 07: "an unknown, repeated, or missing `after` name" |
| `V2-GRANT-binding-after-repeated` | negative | `after: [@args @args]` | `@grant.invalidBinding` at `after` | the same |
| `V2-GRANT-binding-cycle` | negative | two directory leaves, each `after` the other | `@grant.bindingCycle` at the later edge, related along the cycle | 04: "The `after` relation MUST be acyclic" |
| `V2-GRANT-binding-policy-missing` | negative | `@notes` without `policy` | `@grant.invalidBinding` at the binding | 04: "and required there" |
| `V2-GRANT-binding-policy-on-stream` | negative | `@out` with a `policy` | `@grant.invalidBinding` at `policy` | 04: "for `@host.fs.directory` only" |
| `V2-GRANT-policy-foreign-root` | negative | `P` with `root: @args` | `@grant.invalidPolicy` at the policy value | 07: "an invalid policy, or a policy naming another root" |
| `V2-GRANT-policy-unknown-kind` | negative | `expression: {kind: @glob pattern: "*.txt"}` | `@grant.invalidPolicy` | 03: "Unknown fields, kinds, and versions are rejected" |
| `V2-GRANT-policy-empty-suffix` | negative | `names: {kind: @suffix value: ""}` | `@grant.invalidPolicy` | 03: "An empty suffix is invalid" |
| `V2-GRANT-policy-http-family` | negative | `family: @http` | `@grant.invalidPolicy` | 04: "of family `@filesystem`" |
| `V2-GRANT-policy-4096-nodes-recipe` | boundary | a recipe policy counted at 4,096 | accepted (Q9) | 03: "at most 4,096 nodes and entries in total" |
| `V2-GRANT-policy-4097-nodes` | negative | one more | `@grant.policyTooComplex` at the policy (Q9) | 07: "a policy over the size or depth limit" |
| `V2-GRANT-policy-depth-64` | boundary | depth 64 | accepted (Q9) | 03: "a depth of at most 64" |
| `V2-GRANT-policy-depth-65` | negative | depth 65 | `@grant.policyTooComplex` (Q9) | the same |
| `V2-GRANT-limits-empty` | negative | `limits: {}` | `@data.invalidShape` | 04: "A `limits` with no field is the empty tuple and `@data.invalidShape`" |
| `V2-GRANT-limits-one-bound` | positive | `limits: {inputBytes: 1048576u64}` | accepted | 04: "A present record holds at least one bound" |
| `V2-EFFECT-startup-row-outside-ceiling` | negative | an entry that takes `args` and `notes` but whose row and `effects` omit `EnvReadArgs` | `@effect.outsideCeiling` at the `effects` field, related to the `@notes` recipe | 03: "that read is a startup step whose row counts against the admission ceiling" |
| `V2-CONTRACT-unused-effect-startup-only` | positive | `effects` holds `EnvReadArgs`; only the startup read performs it | accepted, no warning | 07: "that neither the entry nor a startup step performs" |
| `V2-GRANT-binding-unused-grant` | positive | an entry `main` taking `{out: StdoutText}` with the empty row and a body that performs nothing; `effects: []`; the recipe `@out` from `@host.io.stdout` | accepted, no diagnostic: a leaf's interface is not a row | 04: "The entry's written `performs` row, and the row of every startup step, MUST be contained in it" |
| `V2-GRANT-entry-test-world-leaf` | negative | an entry taking `world: TestWorld` with any recipe | `@grant.invalidBinding` at `provider` (Q20) | 04: "No binding recipe can name `TestWorld`" |
| `V2-CONTRACT-unused-effect-test-world` | positive with warning | `effects: [@std.test.TestWorld]` on an entry with no such parameter | `@contract.unusedEffect` at that entry (Q20) | 06: "No target can perform one of them" |
| `V2-GRANT-binding-permuted-dict` | positive | `M` with its `grants` entries written in another order | the same `BindingPlan` order (a host test compares `order()`) | 04: "the project file's written order plays no part" |

## Diagnostics and schema changes

Codes: `@grant.invalidBinding`, `@grant.bindingCycle`,
`@grant.invalidPolicy`, `@grant.policyTooComplex`, `@data.invalidShape`,
`@effect.outsideCeiling`, `@contract.unusedEffect`. Schema: the recipe's
`resource` becomes a closed union in `@project.v2`'s decoder, as the
chapter's table lists it.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-host -p vibra-workspace -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0.

## Stays unavailable

`run` (29, 32); dependencies (27). A checked target is not yet executed
by any command.

## Excluded

Acquisition (28); showing a target's authority in plain terms, which the
query and approval surfaces of M3 own.

## Completion evidence

- [ ] `conformance/rules/m2-26.tsv` has one `active` row per case id.
- [ ] Every position has a positive and a negative case.
- [ ] `binding.rs` calls `BindingPlan::validate` and holds no copy of the
      provider table.
- [ ] The validation commands exit 0.
