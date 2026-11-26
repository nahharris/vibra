# Step 09: approvals

Kind: vertical. The `@approval.v2` document: captured into the snapshot as
`approval` revision records, decoded, and validated at every `check`
against the entry's claim, the bindings' recipe identities, the protected
test modules, the selection, and the toolchain; the protected set; what is
void and how it is reported. Writing one is step 13.
Depends on: 03, 04.
Crates: `vibra-workspace`, `conformance/cases`.

## Prerequisites

- Read: [05 Snapshot, claim and witness identity](../../../../spec/05-tooling.md#snapshot-claim-and-witness-identity)
  (the `approval` record kind; "An approval separately protects…");
  [04 Schemas](../../../../spec/04-programs-and-packages.md#schemas);
  [04 Dependencies and lock](../../../../spec/04-programs-and-packages.md#dependencies-and-lock)
  (the lock as the model of a project-level document);
  [07 Contract, effect and authority diagnostics](../../../../spec/07-diagnostics-and-conformance.md#contract-effect-and-authority-diagnostics)
  (`@grant.invalidBinding`, "unapproved"); [architecture](../architecture.md)
  rows I12, I14, I21; [questions](../spec-questions.md) Q6, Q7, Q9, Q11.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-workspace/src/snapshot.rs:288` | `pub fn revision_records` | the approval records join these |
| `crates/vibra-workspace/src/revision.rs:26` | `pub enum RevisionKind` | `Approval` exists |
| `crates/vibra-workspace/src/project.rs:569` | `pub fn project_schema` | the model of a document schema |
| `crates/vibra-syntax/src/schema.rs:510` | `pub fn decode` | the typed decoder |
| `crates/vibra-workspace/src/binding.rs:115` | `pub fn plan_of` | the target's recipes and their identities |
| `crates/vibra-workspace/src/revision.rs:178` | `pub fn embedded_toolchain_record` | the toolchain digest |
| `crates/vibra-workspace/src/snapshot.rs:110` | `pub struct SourceUnit` | the `@tests` unit and its modules |

From steps 03 and 04: `identity::identities`. Proposed: `approval::{Approval,
approval_schema, capture, validate, protects, ApprovalState}`.

## Files owned

- `crates/vibra-workspace/src/approval.rs`
- `crates/vibra-workspace/tests/it/approval.rs`
- `conformance/rules/m3-09.tsv`
- the case directories of the matrix below

## Ordered tasks

1. The schema of Q7 and its decoder; `capture(root)` lists
   `approvals/*.vibon` in sorted order and returns each as an `approval`
   record identified by its project-relative path; a file that is a link,
   or a name that is not `<target>.vibon` for a target of the project, is
   `@project.ioError` or `@data.invalidValue` at the document. Invariant:
   the revision changes when an approval changes.
2. `validate`, after the check, for each decoded approval, in this order,
   stopping at the first class that fires for one fault (Q9): the
   target exists and its entry is `entry`; each `bindings[].recipe`
   equals the recipe identity of the current plan, else
   `@grant.invalidBinding` at the recipe ("unapproved"), once per leaf,
   and a leaf of the plan absent from the approval likewise; each
   `tests[].digest` equals the module's bytes, and the module set is the
   same, else `@project.staleApproval` related to the module; `selection`
   names tests that exist; `claim` equals the entry's current claim, else
   `@project.staleApproval` related to the entry; `toolchain` equals the
   embedded record's digest, else `@project.staleApproval` related to the
   field. Invariant: digests are recomputed from the snapshot's bytes.
3. `protects(snapshot, path)`: true for every module of `@tests` and for
   `project.vibon` when a current approval exists for any target.
4. The three hooks of step 01 become real: capture, validate, and the
   diagnostics join `check`'s.

## Positions

| Rule | Positions, each with a positive and a negative row |
| --- | --- |
| an approval is void when its protected content changes | a test module edited; a test module added; a test module removed; the entry's row; the entry's signature; a binding's policy; a binding's limits; a binding added; the toolchain; against: a body of the code under repair; a comment in a test module (the bytes change, so it is void: the row says so) |

## Test matrix

`workspaceCheck`, `staticV2`, unless stated. `A` is a tree of the release
program's shape with `approvals/gate.vibon` written by hand to match it.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-GRANT-approval-decode-example` | positive | the approval document of Q7 decoded | round-trips to its canonical bytes | 04 Schemas |
| `V2-GRANT-approval-decode-unknown-field` | negative | an approval with `signer: "x"` | `@data.invalidShape` at the field | 04 Schemas: "an unknown field" |
| `V2-GRANT-approval-decode-bad-format` | negative | `format: @approval.v1` | `@data.invalidValue` at `format` | 04 Schemas: "a `format` the reader does not implement" |
| `V2-GRANT-approval-round-trip` | positive | the same, laid out over several lines | identical canonical bytes | 07 Required coverage 15 |
| `V2-GRANT-approval-current` | positive | `A` | accepted, no diagnostic | Q7 |
| `V2-GRANT-approval-absent-runs` | positive | `A` without `approvals/` | accepted | Q11 |
| `V2-GRANT-approval-stale-test` | negative | `A` with one byte changed in `tests/main.vib` | one `@project.staleApproval` at `format`, related to the module at offset 0 | 05: "An approval separately protects the test bodies it selected" |
| `V2-GRANT-approval-stale-test-added` | negative | `A` with a new `tests/extra.vib` | one `@project.staleApproval` | the same, "their helper code" |
| `V2-GRANT-approval-stale-claim` | negative | `A` with the entry's `performs` row changed | one `@project.staleApproval`, related to the entry | 05: "it is void when the test, the policy, or the claim changes" |
| `V2-GRANT-approval-stale-toolchain` | negative | `A` whose `toolchain` digest is another | one `@project.staleApproval`, related to the `toolchain` field | 05: "the runner, tool, driver, and policy versions" |
| `V2-GRANT-approval-unapproved-binding` | negative | `A` with the `@notes` policy's suffix changed | one `@grant.invalidBinding` at the recipe; no `@project.staleApproval` | 07: "a missing, extra, ill-typed, or unapproved grant binding" |
| `V2-GRANT-approval-binding-added` | negative | `A` with a new leaf and parameter | one `@grant.invalidBinding` at the new recipe | the same |
| `V2-GRANT-approval-one-fault` | negative | `A` with the policy changed, which also changes the claim | exactly one diagnostic | 07: "One fault is reported once" |
| `V2-GRANT-approval-body-edit-current` | positive | `A` with a body of `src/gate/tally.vib` changed | accepted: the code under repair is not protected | 05: "pinning every body would make repair impossible" |
| `V2-GRANT-approval-selection-missing` | negative | `A` whose `selection` names a test the module lacks | one `@project.staleApproval` related to the `selection` entry | 05: "the selection" |
| `V2-GRANT-approval-wrong-target` | negative | `approvals/nope.vibon` | `@data.invalidValue` at the document's `target` | Q7 |
| `V2-RUNTIME-run-stale-approval-refused` | negative, `workspaceRun` | `A` with a stale test, run under a world | `@command.diagnostics`, empty `startup`, no events | 05 Command payloads: a run whose check fails |
| `ATK-approval-no-signature-is-data` | negative | the decoder given a document with a `signature` field | refused; the decoded type has no authority: the host test shows no function of `approval.rs` returns a grant or calls the broker | 06: "A world document describes inputs. It is not authority" |
| revision | positive | `A`, then with its approval changed | different revisions | 05: "Every admitted input contributes a record" |
| link | negative | `approvals/gate.vibon` a symbolic link | `@project.ioError` at the document | 04 Discovery: "A marker that is a symbolic link or junction is a project filesystem failure" |

## Diagnostics and schema changes

Codes: `@grant.invalidBinding`, `@project.staleApproval` (Q9: not in the
registry today; the row is written against the recommended answer and the
step stops if the ruling has not landed), `@data.invalidShape`,
`@data.invalidValue`, `@project.ioError`. Schema: `@approval.v2` (new,
VIBON, no JSON form).

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-workspace -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0. `revisions.ps1` must list `approvals/*.vibon` as
`approval` records; the step extends it only if step 05 has not.

## Stays unavailable

`approve`: no command writes an approval until step 13.

## Excluded

The presentation and the writer (13); the edit engine's refusal (11, 14).

## Completion evidence

- [ ] `conformance/rules/m3-09.tsv` has one `active` row per case id.
- [ ] Every row of "Positions" has its case; the report maps each.
- [ ] The report quotes `validate`'s order of checks.
- [ ] The validation commands exit 0.
