# Step 10: approvals

Kind: vertical. The `@approval.v2` document: captured into the snapshot as
`approval` revision records, decoded, and admitted at every `check` and on
the path every test run takes, against the entry's claim, the bindings'
recipe identities, every file of the `@tests` unit, the selection, and the
toolchain; the document's identity; the pin `--expect-approval` checks
against; what is void and how it is reported; the approvals a result names
and the relation of a test run's selection to the approved one. Writing one
is step 16.
Depends on: 04, 09.
Crates: `vibra-workspace`, `conformance/cases`.

## Prerequisites

- Read: [05 Snapshot, claim and witness identity](../../../../spec/05-tooling.md#snapshot-claim-and-witness-identity)
  (the `approval` record kind; "An approval separately protects…"; the
  `@toolchain.v2` table); [04 Schemas](../../../../spec/04-programs-and-packages.md#schemas);
  [04 Dependencies and lock](../../../../spec/04-programs-and-packages.md#dependencies-and-lock)
  (the lock as the model of a project-level document);
  [04 Test worlds and grants](../../../../spec/04-programs-and-packages.md#test-worlds-and-grants)
  (fixture data is source);
  [07 Contract, effect and authority diagnostics](../../../../spec/07-diagnostics-and-conformance.md#contract-effect-and-authority-diagnostics)
  (`@grant.invalidBinding`, "unapproved"); [architecture](../architecture.md)
  rows I12, I13, I14, I21, I26, I27; [questions](../spec-questions.md) Q6,
  Q7, Q9, Q11, Q40, Q43.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-workspace/src/snapshot.rs:288` | `pub fn revision_records` | the approval records join these |
| `crates/vibra-workspace/src/revision.rs:26` | `pub enum RevisionKind` | `Approval` exists |
| `crates/vibra-workspace/src/project.rs:569` | `pub fn project_schema` | the model of a document schema |
| `crates/vibra-syntax/src/schema.rs:510` | `pub fn decode` | the typed decoder |
| `crates/vibra-workspace/src/binding.rs:115` | `pub fn plan_of` | the target's recipes and their identities |
| `crates/vibra-workspace/src/revision.rs:178` | `pub fn embedded_toolchain_record` | the toolchain digest |
| `crates/vibra-workspace/src/snapshot.rs:110` | `pub struct SourceUnit` | the `@tests` unit and its files |
| `crates/vibra-workspace/src/test_runner.rs:673` | `fn check_selection(` | the path every test run takes; the hook of step 01 follows it |
| `examples/gate/tests/main.vib:49` | `files: Dict(RelativePath Str)` | fixture data as a source constant of the `@tests` unit |

From steps 04 and 09: `identity::identities`. From step 01: the hooks
`approval::capture`, `admit`, `admit_test_run`, and the envelope's and the
test run's `approvals` lists. Proposed: `approval::{Approval,
approval_schema, capture, admit, admit_test_run, protects, ApprovalState,
ApprovalInForce, TestApproval, SelectionRelation}`.

## Files owned

- `crates/vibra-workspace/src/approval.rs`
- `crates/vibra-workspace/tests/it/approval.rs`
- `crates/vibra-cli/tests/it/approval.rs`
- `conformance/rules/m3-10.tsv`
- the case directories of the matrix below

## Ordered tasks

1. The schema of Q7 and its decoder; `Approval::identity()` is `sha256:`
   of the canonical bytes; `capture(root)` lists the entries of
   `approvals/` whose name ends `.vibon` and does not begin with `.`, in
   sorted order, and returns each as an `approval` record identified by
   its project-relative path; a file that is a link is `@project.ioError`
   at the document, a `.vibon` name that is not `<target>.vibon` for a
   target of the project is `@data.invalidValue` at the document, and
   every other entry is ignored. Invariant: the revision changes when an
   approval changes; capture writes nothing.
2. `admit(snapshot, check, identities, expectations)`, after the check.
   First the pin: with `expectations.approval` set, the approval with that
   identity must be in force (present, decoded, and current after the
   checks below), else one `@project.staleApproval` with the related span
   the chapter gives it and nothing else is reported. Then for each
   decoded approval, in this order, stopping at the first class that fires
   for one fault (Q9): the target exists and its entry is `entry`; each
   `bindings[].recipe` equals the recipe identity of the current plan,
   else `@grant.invalidBinding` at the recipe ("unapproved"), once per
   leaf, and a leaf of the plan absent from the approval likewise; each
   `files[].digest` equals the file's bytes and the file set is the
   `@tests` unit's, else `@project.staleApproval` related to the file;
   `selection` names tests that exist; `claim` equals the entry's current
   claim, else `@project.staleApproval` related to the entry; `toolchain`
   equals the embedded record's digest, else `@project.staleApproval`
   related to the field. The approvals that pass are the list in force,
   `{target, identity}` each, which the envelope carries. Invariant:
   digests are recomputed from the snapshot's bytes.
3. `admit_test_run(snapshot, selector, check, expectations)`: the same
   admission on the path that checks a selection, so a selected and a
   whole-suite run are both refused under a stale approval; for each
   approval in force, the relation of the run's selected set to the
   approval's `selection`: `@equal`, `@covers` (a superset), `@part` (a
   subset), `@other`. Invariant: the runner cannot run a test without this
   call.
4. `protects(snapshot, in_force, source_id)`: true for every file the
   snapshot captures for the `@tests` unit while any approval is in force;
   false for every other file. One rule, no helper list: everything a test
   relies on as evidence lives in the `@tests` unit; code in a target is
   code under repair.
5. The hooks of step 01 become real: capture, admit, and admit_test_run,
   with their diagnostics joining `check`'s and the run's.

## Positions

| Rule | Positions, each with a positive and a negative row |
| --- | --- |
| an approval is void when its protected content changes | a test module edited; a helper in another `@tests` module edited; a test module added; a test module removed; the entry's row; the entry's signature; a binding's policy; a binding's limits; a binding added; the selection; the toolchain; against: a body of the code under repair; a comment in a test module (the bytes change, so it is void: the row says so) |
| the pin decides | the expected approval present and current; against: missing, replaced by a freshly written current one, void; each also without the option |
| a test run is admitted | a selected run; the whole suite; against: either under a stale approval |

## Test matrix

`workspaceCheck`, `staticV2`, unless stated. `A` is a tree of the release
program's shape with `approvals/gate.vibon` written by hand to match it.
A row that needs a command-line option is a host test (`ATK-`) over the
engine and the CLI.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-GRANT-approval-decode-example` | positive | the approval document of Q7 decoded | round-trips to its canonical bytes; its identity is the SHA-256 of those bytes | 04 Schemas |
| `V2-GRANT-approval-decode-unknown-field` | negative | an approval with `signer: "x"` | `@data.invalidShape` at the field | 04 Schemas: "an unknown field" |
| `V2-GRANT-approval-decode-bad-format` | negative | `format: @approval.v1` | `@data.invalidValue` at `format` | 04 Schemas: "a `format` the reader does not implement" |
| `V2-GRANT-approval-round-trip` | positive | the same, laid out over several lines | identical canonical bytes | 07 Required coverage 15 |
| `V2-GRANT-approval-current` | positive | `A` | accepted, no diagnostic; the check's approvals in force name `@gate` and the document's identity | Q7, Q40 |
| `V2-GRANT-approval-absent-runs` | positive | `A` without `approvals/` | accepted; no approval in force | Q11 |
| `V2-GRANT-approval-stale-test` | negative | `A` with one byte changed in `tests/main.vib` | one `@project.staleApproval` at `format`, related to the file at offset 0 | 05: "An approval separately protects the test bodies it selected" |
| `V2-GRANT-approval-stale-helper` | negative | `A` with a helper `fn` of `tests/helpers.vib` changed, which no selected test names directly | one `@project.staleApproval` related to that file | 05: "their helper code and data" |
| `V2-GRANT-approval-stale-test-added` | negative | `A` with a new `tests/extra.vib` | one `@project.staleApproval` | the same |
| `V2-GRANT-approval-stale-fixture-constant` | negative | `A` with one byte of the `files` constant of `tests/main.vib` changed | one `@project.staleApproval` related to the file | 05: "their helper code and data"; 04: fixture data is a source constant |
| `V2-GRANT-approval-stale-claim` | negative | `A` with the entry's `performs` row changed | one `@project.staleApproval`, related to the entry | 05: "it is void when the test, the policy, or the claim changes" |
| `V2-GRANT-approval-stale-selection` | negative | `A` whose `selection` names another valid test of the module | one `@project.staleApproval` related to the `selection` entry; the document's identity differs from the registered one | 05: "the selection" |
| `V2-GRANT-approval-stale-toolchain` | negative | `A` whose `toolchain` digest is another | one `@project.staleApproval`, related to the `toolchain` field | 05: "the runner, tool, driver, and policy versions"; Q43 |
| `V2-GRANT-approval-unapproved-binding` | negative | `A` with the `@notes` policy's suffix changed | one `@grant.invalidBinding` at the recipe; no `@project.staleApproval` | 07: "a missing, extra, ill-typed, or unapproved grant binding" |
| `V2-GRANT-approval-binding-added` | negative | `A` with a new leaf and parameter | one `@grant.invalidBinding` at the new recipe | the same |
| `V2-GRANT-approval-one-fault` | negative | `A` with the policy changed, which also changes the claim | exactly one diagnostic | 07: "One fault is reported once" |
| `V2-GRANT-approval-body-edit-current` | positive | `A` with a body of `src/gate/tally.vib` changed | accepted: the code under repair is not protected | 05: "pinning every body would make repair impossible" |
| `V2-GRANT-approval-selection-missing` | negative | `A` whose `selection` names a test the module lacks | one `@project.staleApproval` related to the `selection` entry | 05: "the selection" |
| `V2-GRANT-approval-two-targets-shared-tests` | negative | `A` with a second binary target and `approvals/other.vibon` over the same `@tests`; one test byte changed | two `@project.staleApproval`, one per approval, each related to the file; with no change both are in force | 05: "An approval separately protects the test bodies it selected"; 07: "One fault is reported once" |
| `V2-GRANT-approval-case-collision` | negative | `A` with `tests/Main.vib` added beside `tests/main.vib` | the check fails at discovery with `@module.caseCollision`; no approval is admitted | 07: "two source paths that differ only by case" |
| `V2-GRANT-approval-wrong-target` | negative | `approvals/nope.vibon` | `@data.invalidValue` at the document's `target` | Q7 |
| `V2-GRANT-approval-ignored-entry` | positive | `A` with `approvals/.gate.vibon.vibra-publish` and `approvals/notes.txt` present | accepted; neither is a record | Q7: "every other entry is ignored" |
| `V2-RUNTIME-run-stale-approval-refused` | negative, `workspaceRun` | `A` with a stale test, run under a world | `@command.diagnostics`, empty `startup`, no events | 05 Command payloads: a run whose check fails |
| `ATK-approval-expected-current` | positive | `check --expect-approval <identity of A's approval>` on `A` | accepted; `approvals` names it | Q40 |
| `ATK-approval-expected-missing` | negative | the same option on `A` without `approvals/` | one `@project.staleApproval` with no location; `@command.diagnostics`; nothing else reported | Q40; Q9 |
| `ATK-approval-expected-replaced` | negative | the same option on `A` whose approval is a freshly written, current document of another selection | one `@project.staleApproval` at `format`; the fresh approval is not in force for this command | Q40: "a missing, replaced, or void approval refuses the command before anything else" |
| `ATK-approval-expected-void` | negative | the same option on `A` with a stale test | one `@project.staleApproval`, the pin's, related to the file | Q40 |
| `ATK-approval-expected-absent-without-option` | positive | `check`, `test`, `run` on `A` without `approvals/` and without the option; then with the replaced approval and without the option | each accepted; `approvals` is `[]`, then names the fresh document | Q11 |
| `ATK-approval-envelope-names-identity` | positive | `check`, `test`, `run` on `A` | each envelope's `approvals` is `[{target: "@gate", identity: "sha256:…"}]` equal to the file's SHA-256 | Q40 |
| `ATK-test-selected-run-admitted` | negative | `test @tests.main::"…"` on `A` with a stale test | `@command.diagnostics` with the one `@project.staleApproval`; no item runs | 05: "Every selected test is checked before any runs"; Q40 |
| `ATK-test-whole-suite-admitted` | negative | `test` with no selector on the same tree | the same refusal | the same |
| `ATK-test-selection-equal` | positive | `test` with the approval's one selector | `approvals[0].selection: "@equal"` | Q40 |
| `ATK-test-selection-covers` | positive | `test` with no selector on `A`, whose approval selects one of two tests | `"@covers"` | Q40 |
| `ATK-test-selection-part` | positive | an approval selecting two tests; `test` of one | `"@part"` | Q40 |
| `ATK-test-selection-other` | positive | `test` of a test the approval does not select | `"@other"` | Q40 |
| `ATK-approval-no-signature-is-data` | negative | the decoder given a document with a `signature` field | refused; the decoded type has no authority: the host test shows no function of `approval.rs` returns a grant or calls the broker | 06: "A world document describes inputs. It is not authority" |
| revision | positive | `A`, then with its approval changed | different revisions | 05: "Every admitted input contributes a record" |
| link | negative | `approvals/gate.vibon` a symbolic link | `@project.ioError` at the document | 04 Discovery: "A marker that is a symbolic link or junction is a project filesystem failure" |

## Diagnostics and schema changes

Codes: `@grant.invalidBinding`, `@project.staleApproval` (Q9: not in the
registry today; the rows are written against the recommended answer and
the step stops if the ruling has not landed), `@data.invalidShape`,
`@data.invalidValue`, `@project.ioError`, `@module.caseCollision`. Schema:
`@approval.v2` (new, VIBON, no JSON form); the envelope's `approvals` and
the test payload's `approvals` (Q40) are filled for the first time.

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

`approve`: no command presents or writes an approval until steps 14 and 16.

## Excluded

The presentation and the writer (14, 16); the edit engine's refusal (13,
15).

## Completion evidence

- [ ] `conformance/rules/m3-10.tsv` has one `active` row per case id.
- [ ] Every row of "Positions" has its case; the report maps each.
- [ ] The report quotes `admit`'s order of checks and `admit_test_run`'s
      call site after `check_selection`.
- [ ] The validation commands exit 0.
