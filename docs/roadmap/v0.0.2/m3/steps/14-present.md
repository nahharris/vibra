# Step 14: present

Kind: vertical. `vibra approve TARGET`: the presentation of a target's
authority in plain terms from the project file alone, the two design
limits, and the state and identity of its approval; the approval document
computed and returned, not written. Review finding F8 of M2 closes here;
`--write` is step 16.
Depends on: 10.
Crates: `vibra-workspace`, `vibra-cli`, `vibra-schema`, `conformance/cases`.

## Prerequisites

- Read: [03 Target admission](../../../../spec/03-effects.md#target-admission);
  [03 Filesystem policies](../../../../spec/03-effects.md#filesystem-policies)
  ("Two limits of this design"); [04 Targets](../../../../spec/04-programs-and-packages.md#targets);
  [07 Required coverage](../../../../spec/07-diagnostics-and-conformance.md#required-coverage) 11;
  [architecture](../architecture.md) rows I14, I20; [questions](../spec-questions.md) Q7, Q8.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-workspace/src/binding.rs:115` | `pub fn plan_of` | the `BindingPlan` |
| `crates/vibra-host/src/recipe.rs:528` | `pub fn order` | the startup order the presentation follows |
| `crates/vibra-host/src/recipe.rs:715` | `pub fn recipe_identity` | each binding's identity |
| `crates/vibra-host/src/policy.rs:989` | `pub fn normalize` | the normalized policy |
| `crates/vibra-host/src/policy.rs:1060` | `pub fn hash` | its hash |
| `crates/vibra-host/src/policy.rs:571` | `pub fn rules` | the rules, for plain terms |
| `crates/vibra-workspace/src/entry.rs:42` | `pub struct EntryFacts` | the entry and its leaves |
| `crates/vibra-cli/src/run.rs:51` | `pub fn run` | the model of a target command |

From step 10: `approval::{Approval, ApprovalState}`. Proposed:
`present::{present, Presentation, PresentedBinding, approval_document}`;
`vibra-cli::approve::run`; `vibra-schema::approve_payload::ApprovePayload`.

## Files owned

- `crates/vibra-workspace/src/present.rs`
- `crates/vibra-workspace/tests/it/present.rs`
- `crates/vibra-cli/src/approve/mod.rs`
- `crates/vibra-cli/tests/it/approve.rs`
- `crates/vibra-schema/src/approve_payload.rs`
- `crates/vibra-schema/schemas/v2/payloads/approve.json`
- `crates/vibra-schema/tests/it/approve_payload.rs`
- `conformance/rules/m3-14.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `present(check, identities, target)`: for a checked binary target, the
   entry's identity and claim; in startup order, each leaf with its
   interface, provider, resource as written, `source` read, normalized
   policy rendered as rules in plain terms (`read files under in/ with
   names ending ".txt", any depth`) beside its hash, limits, `after`; the
   two design limits as atoms with their sentences; the approval state
   with the identity of the approval in force or stale. Invariant: no
   `Context`, no `Driver`, no `admit` is reached; the function takes no
   path and reads no environment.
2. `approval_document(presentation, snapshot)`: the `@approval.v2` of Q7
   with the digests of every file of the `@tests` unit from the snapshot's
   bytes and the selection every test of the unit; its identity.
3. `vibra-cli::approve::run`: a target that is not an atom, names no
   binary target, or does not check is invalid input or
   `@command.diagnostics` as for `run`; the payload with `written: false`,
   `path: null`, and the candidate document's identity under `approval`;
   `--write` routes to `approve::write`, which answers `@tool.unavailable`
   with the preview returned until step 16. The human format prints the
   plain terms.
4. `ApprovePayload {target, entry, claim, bindings, designLimits, approval,
   written, path}` and its schema; its exhausted form keeps `written` and
   `path`; `SUPPORTED = true`.

## Test matrix

The preview is a host test over the gate program's project and a
`workspaceCheck` tree where the row names a case.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `ATK-approve-presents-every-rule` | positive | `approve @gate` on `examples/gate` | one binding per leaf in startup order; the `@notes.reader` policy's one rule in plain terms with the hash of the normalized document; limits and `after` as written | 03: "for each grant, the actual resource, the operation, the policy, and the limits, and not only an interface name" |
| `ATK-approve-design-limits-shown` | positive | the same | `designLimits` holds `@authorityFollowsOpenedDirectory` and `@hardLinkReplaceDenied` with their sentences | 03: "Two limits of this design are part of the contract and MUST appear in what a user is shown when approving a target" |
| `ATK-approve-from-project-alone` | negative | the same with the environment variable the `@metrics` leaf would read set and unset | identical payloads | 03: "presentable in plain terms from the project file alone" |
| `ATK-approve-no-acquisition` | negative | the same, with a hook counting `Driver::acquire` and `Context::new` | zero calls | architecture, I14 |
| `ATK-approve-startup-read-shown` | positive | the same | the `@notes.reader` binding shows its startup read through `@args` | 03: "A recipe that reads configuration to choose a resource shows that read" |
| `ATK-approve-state-and-identity` | positive | the same with no approval, with a current one, with a stale one | `approval.state` `@none`, `@current` with the file's identity, `@stale` with the identity and what changed | Q8 |
| `ATK-approve-candidate-identity` | positive | the same | the candidate document's identity equals the SHA-256 of the canonical bytes `approval_document` returns; with a current approval the two identities are equal | Q7 |
| `ATK-approve-write-unavailable-yet` | positive | `approve @gate --write` before step 16 | `@tool.unavailable`, the preview returned, nothing written | 05 Command surface: "A known command the tool does not yet support is unavailable" |
| `V2-TOOL-approve-not-a-target` | negative, `workspaceCheck` tree used by the CLI test `approve::not_a_target` | `approve @lib` on a library target | `@command.invalidInput` | 05 Command surface: "a library given to `run` or `build`, is invalid input"; the same shape |
| `ATK-approve-refused-on-error` | negative | `approve @gate` on a project with a type error | `@command.diagnostics`, no presentation | 05: `run`'s shape |
| JSON validates | positive | every envelope | validates | 05 |

## Diagnostics and schema changes

Codes: `@project.staleApproval` (through the state), `@tool.unavailable`.
Schema: the `approve` payload (new). The command `approve` is an addition
to 05's surface (Q8).

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; `help` lists `approve` as supported.

## Stays unavailable

`approve --write` (16).

## Excluded

Writing (16); the edit engine (15); MCP.

## Completion evidence

- [ ] `conformance/rules/m3-14.tsv` has one `active` row per case id.
- [ ] `git grep -n "Context::new\|admit(" crates/vibra-workspace/src/present.rs`
      prints nothing.
- [ ] The validation commands exit 0.
