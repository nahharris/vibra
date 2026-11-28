# Step 15: edit-apply

Kind: vertical. `--write`: the publisher. A plan over one document is
applied at a commit point that validates the whole read set: the write
lock, a second capture, the revision comparison, the destination's bytes
through the handle, then the publish primitive of step 02; a plan over
several documents is refused before writing; every write confined to the
held workspace root; `--expect-revision`; `fmt --write` goes through the
same path; a read-only command writes nothing.
Depends on: 02, 13.
Crates: `vibra-workspace`, `vibra-cli`, `vibra-conformance`.

## Prerequisites

- Read: [05 Transactional edit plans](../../../../spec/05-tooling.md#transactional-edit-plans)
  ("Applying a plan", the six steps; "A backend that cannot write several
  files atomically MUST refuse before writing"); [03 Filesystem policies](../../../../spec/03-effects.md#filesystem-policies)
  rules 1 to 6; [07 Runtime and tool failures](../../../../spec/07-diagnostics-and-conformance.md#runtime-and-tool-failures)
  (`@tool.stalePlan`, `@tool.unavailable`); [architecture](../architecture.md)
  section 3, rows I6 to I10, I13, I17, "The theories"; [M2 architecture](../../m2/architecture.md#the-host)
  "The order of a mutating call"; [questions](../spec-questions.md) Q12 to
  Q17, Q28.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-fs-scope/src/lib.rs:51` | `pub use windows::{ScopedDir, ScopedListing, ScopedParent, close, is_regular_file}` | the held root and its operations |
| `crates/vibra-fs-scope/src/windows.rs:228` | `pub fn open_for_replace` | the destination's real names, link count, and bytes before the publish |
| `crates/vibra-fs-scope/src/windows.rs:366` | `pub fn create_new` | the reserved file and the lock, created exclusively |
| `crates/vibra-fs-scope/src/lib.rs:225` | `pub fn read_at_most` | the bytes read through the destination's handle |
| `crates/vibra-fs-scope/src/lib.rs:201` | `pub fn real_names` | the real-name check |
| `crates/vibra-workspace/src/format_plan.rs:239` | `pub fn write` | the current writer of `fmt --write`, outside the scoped adapter, replaced by the publisher |
| `crates/vibra-workspace/src/format_plan.rs:67` | `pub fn capture` | where a command takes its snapshot; the second capture of the commit point is the same function |
| `crates/vibra-host/src/system/fs.rs:1` | `//!` | the directory driver, the model of a confined writer over the adapter |

From step 02: `flush`, `rename_into`, `remove_file`; from 07: `EditPlan`;
from 13: `plan_body`; from 01: the stubs `edit::apply::{apply,
publish_source, publish_approval}` and `vibra-cli::edit::write`. Proposed:
their bodies, `Applied`, `Expectations`, `WRITE_LOCK`, `reserved_name`.

## Files owned

- `crates/vibra-workspace/src/edit/apply.rs`
- `crates/vibra-workspace/src/format_plan.rs`
- `crates/vibra-workspace/tests/it/edit_apply.rs`
- `crates/vibra-cli/src/edit/write.rs`
- `crates/vibra-cli/src/fmt.rs`
- `crates/vibra-cli/tests/it/edit_apply.rs`
- `crates/vibra-cli/tests/it/read_only.rs`
- `crates/vibra-conformance/tests/it/engine_writes.rs`
- `conformance/rules/m3-15.tsv`

## Ordered tasks

1. `apply(workspace, plan, expectations)`, the commit point, in this
   order and stopping at the first refusal with nothing written:
   `expectations.revision` against the plan's `workspaceRevision`, else
   `Stale` with `@tool.stalePlan`; a plan with more than one document is
   `Unavailable` with `@tool.unavailable` (Q15); the root is opened once as
   a held directory; the lock `.vibra-write-lock` is created exclusively
   under it with the process id and the command as its bytes, and a lock
   that exists is `Locked` with `@project.ioError` at the lock's path,
   never removed; the workspace is captured again and its revision must
   equal the plan's, else `Stale`; the destination must be a module of a
   target or `@tests` of the second capture's snapshot (`publish_source`),
   not protected, reached from the root one real name at a time with
   `real_names` equal to the plan's path components; its bytes read
   through the handle must equal the plan's document revision and each
   edit's fingerprint, else `Stale`; its link count must be one, else
   `Failed`; then the primitive: reserved file, write, flush, rename over;
   then the lock is removed; `Applied` with the new revision. On a
   platform with no adapter, `Unavailable`. Invariant: nothing is created
   but the lock and the reserved file, nothing removed but those two,
   nothing renamed but the reserved file over its destination.
2. A leftover reserved file for the destination: `create_new` answers
   `alreadyExists`; the publisher, holding the lock, removes it through the
   held directory and creates it once more. A failure of the write or the
   flush removes the reserved file and is `Failed` with `@module.ioError`
   at offset 0 of the module; a failure of the rename is the same, with the
   reserved file removed when it still exists.
3. `publish_approval(root, target, bytes)`: the same primitive at
   `approvals/<target>.vibon`, the path derived from the target's name
   alone, into an `approvals/` that must exist (`@project.ioError` at the
   path otherwise); step 16 calls it.
4. `fmt --write` goes through `apply` with a one-document format plan;
   `format_plan::write` is removed, and `vibra-cli::fmt` reads the
   `Applied` outcome. `vibra-cli::edit::write`: `--write` applies after a
   preview with no resulting error, no denial, and no invalid plan;
   `applied`, `resultingRevision`; the exhausted form of the payload keeps
   both (Q28).
5. `engine_writes.rs`: every call in `vibra-workspace/src` that creates,
   writes, renames, or removes a file is in `edit/apply.rs`; the test
   reads the sources.
6. `read_only.rs`: a recursive listing and every file's bytes of a tree
   before and after `check`, `query`, `eval`, `test`, `fmt` (preview),
   `edit body` (preview), and `approve` (preview), with a leftover reserved
   file and a stale lock present in the tree; equal.

## Positions

| Rule | Positions, each with a positive and a negative row |
| --- | --- |
| the whole read set is validated at the commit point | the edited document; an unedited imported module; the project file; an approval; against: none changed |
| exactly one writer commits | two writers from one revision, in sequence; against: a second writer while the lock is held |
| one document, or none | a one-document plan; against: a two-document plan; a failure before the rename |

## Test matrix

Host tests on Windows; elsewhere each `ATK-` row prints `NOT RUN: no scoped
directory adapter`. `P` is the plan of `V2-TOOL-edit-body-fn` applied to a
temporary copy of its tree.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| apply one document | positive | `P --write` | the file holds the new bytes; `applied: true`; `resultingRevision` is the new tree's revision; no reserved file and no lock remain; the directory listing is as before | 05: "publish all files, or none" |
| `ATK-edit-apply-expect-revision` | negative | `--expect-revision` of another revision | `@tool.stalePlan`, `applied: false`, bytes unchanged | 05: "A plan that is stale … fails with nothing written" |
| `ATK-edit-apply-stale-document` | negative | the module changed between the first capture and the lock (a hook between them) | `@tool.stalePlan`, unchanged | 05 step 1: "verify the workspace revision and every document revision" |
| `ATK-edit-apply-unedited-input-changed` | negative | a module the edited one imports, which the plan does not touch, changed between the first capture and the lock | `@tool.stalePlan`, unchanged | 05: "A plan that is stale … fails with nothing written"; Q17 |
| `ATK-edit-apply-two-writers-one-revision` | negative | two processes preview from one revision; the first applies; the second applies | the first `applied: true`; the second `@tool.stalePlan`, its document unchanged by it; the tree holds the first's bytes | Q17: "of two writers planned from one revision exactly one applies" |
| `ATK-edit-apply-lock-present` | negative | a `.vibra-write-lock` exists when the writer arrives | `@command.operationalFailure` with `@project.ioError` at the lock's path; the lock and every file unchanged; the human format names the lock | Q17: "A lock that is present is never broken" |
| `ATK-edit-apply-fingerprint-mismatch` | negative | a plan whose edit fingerprint is altered | refused before any write | Q13 |
| `ATK-edit-apply-several-documents-unavailable` | negative | the two-document fixture plan of step 07 with `--write` | `@tool.unavailable`, `@command.unavailable`, exit 4; the plan returned with `applied: false`; both documents unchanged; no lock taken | 05: "A backend that cannot write several files atomically MUST refuse before writing"; Q15 |
| `ATK-publish-failure-injected` | negative | a hook fails the write, then the flush, then the rename | each time the destination holds the old bytes, `applied: false`, `@module.ioError`; no reserved file remains after the command | 05: "An interrupted application is never reported as applied" |
| `ATK-edit-apply-leftover-reserved-file` | positive | a leftover `.math.vib.vibra-publish` with partial bytes before `P --write` | the publish completes; the leftover is gone; the destination holds the new bytes | architecture, section 3, "A leftover" |
| `ATK-edit-apply-symlink-refused` | negative | the target module replaced by a symbolic link to a file outside | refused; the outside file unchanged | 03 rule 1: "without following symbolic links, junctions, or other reparse points" |
| `ATK-edit-apply-alias-refused` | negative | the module reached by a short-name alias in the plan's path | refused | 03 rule 2: "A different-case or short-name alias MUST NOT satisfy a name test the real name fails" |
| `ATK-edit-apply-hard-link-refused` | negative | the destination has a second hard link | refused before the reserved file is created; both names unchanged | 03: "Replacing it MUST be refused with `denied` when the platform reports a link count above one" |
| `ATK-edit-apply-dotdot-refused` | negative | a plan document `../x.vib` | refused before opening | 05: "Escapes through a parent … are rejected" |
| `ATK-edit-apply-dependency-refused` | negative | a plan document under `dep/` | refused | 05: "never vendored dependencies" |
| `ATK-edit-apply-destination-not-a-module` | negative | a plan whose document is `project.vibon`, then `approvals/gate.vibon`, then `src/app/notes.txt`, built by a hook | each refused at the commit point: not a module of the recaptured snapshot's targets or `@tests` | Q17: "decided from the module list of the snapshot captured at the commit point, never from the plan's path" |
| `ATK-edit-apply-protected-unchanged` | negative | a plan whose document is a protected test module, built with the plan-time check bypassed by a hook | the publish refuses; bytes unchanged | 05: "No fix or format may alter a protected … test" |
| `ATK-fmt-write-protected-denied` | negative | `fmt tests/main.vib --write` under an approval in force | `@tool.actionDenied`, unchanged | the same |
| `ATK-edit-writers-enumerated` | negative | `engine_writes.rs` given a `query/index.rs` that calls `std::fs::write` | the test reports it | architecture, I6, I9 |
| `ATK-edit-apply-no-create-no-remove` | negative | the apply path's sources | the only creations are the lock's and the reserved file's, the only removals theirs, the only rename the reserved file's | 03 rule 4; architecture section 3 |
| `ATK-read-only-tree-unchanged` | positive | `read_only.rs` over a tree with a leftover reserved file and a stale lock | the recursive listing and every byte equal after `check`, `query`, `eval`, `test`, `fmt`, `edit body`, and `approve` without `--write` | 05: "A mutating command previews unless `--write` is given"; Q17: "A read-only command takes no lock and writes nothing" |
| `ATK-output-bound-applied-edit` | positive | `P --write` with a hook that makes the preview's plan render to 17 MiB | the exhausted envelope with `applied: true` and the new `resultingRevision`; the file holds the new bytes | Q28: "the fields that state what was committed kept" |
| `edit_apply::not_run_off_windows` | boundary | every row above on a platform with no adapter | each prints `NOT RUN`; `--write` is `@tool.unavailable` with the preview returned | Q16 |
| `ATK-fmt-write-through-apply` | positive | `fmt src/app/main.vib --write` | written through `apply`; `format_plan::write` no longer exists; `git grep "fs::rename" crates/vibra-workspace/src` prints nothing | architecture, I9 |
| JSON validates | positive | every envelope | validates | 05 |

## Diagnostics and schema changes

Codes: `@tool.stalePlan`, `@tool.actionDenied`, `@tool.unavailable`,
`@module.ioError`, `@project.ioError`. Schema: none new; the `edit`
payload's `applied` and `resultingRevision` are produced for the first
time.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-fs-scope -p vibra-workspace -p vibra-cli -p vibra-conformance --all-features -- --nocapture
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: exit 0. Count the `NOT RUN` lines; on Windows there are none.

## Stays unavailable

`--write` off Windows; `--write` of a plan over several documents.

## Excluded

Approval writing (16); rename, imports, fix, lint (26 to 30); MCP.

## Completion evidence

- [ ] Each row is a test, named in the report; each `ATK-` row was seen to
      fail with its protection removed, and the report says how.
- [ ] The report quotes `apply` showing the order: lock, second capture,
      revision comparison, destination check, primitive, unlock.
- [ ] `git grep -n "std::fs::" crates/vibra-workspace/src` shows the
      allowed read sites only, and no `fs::rename` or `fs::write`.
- [ ] The validation commands exit 0.
