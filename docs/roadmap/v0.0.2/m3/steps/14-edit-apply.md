# Step 14: edit-apply

Kind: vertical. `--write`: a plan is applied against the revision it was
planned for, every document verified through the handle opened for its
replacement, all files published or none through a write-ahead journal,
every write confined to the held workspace root; `--expect-revision`;
`fmt --write` and `approve --write` go through the same path.
Depends on: 11.
Crates: `vibra-workspace`, `vibra-cli`, `vibra-conformance`.

## Prerequisites

- Read: [05 Transactional edit plans](../../../../spec/05-tooling.md#transactional-edit-plans)
  ("Applying a plan", the six steps); [03 Filesystem policies](../../../../spec/03-effects.md#filesystem-policies)
  rules 1 to 6; [07 Runtime and tool failures](../../../../spec/07-diagnostics-and-conformance.md#runtime-and-tool-failures)
  (`@tool.stalePlan`); [architecture](../architecture.md) section 3, rows
  I6 to I9, I13, "The theories"; [M2 architecture](../../m2/architecture.md#the-host)
  "The order of a mutating call"; [questions](../spec-questions.md) Q12 to Q17.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-fs-scope/src/lib.rs:51` | `pub use windows::{ScopedDir, ScopedListing, ScopedParent, close, is_regular_file}` | the held root and its operations |
| `crates/vibra-fs-scope/src/lib.rs:248` | `pub fn replace_contents` | truncate-and-write on the checked handle |
| `crates/vibra-fs-scope/src/lib.rs:225` | `pub fn read_at_most` | the bytes read through the same handle |
| `crates/vibra-fs-scope/src/lib.rs:201` | `pub fn real_names` | the real-name check |
| `crates/vibra-workspace/src/format_plan.rs:239` | `pub fn write` | the current writer of `fmt --write`, replaced by the publish path |
| `crates/vibra-workspace/src/format_plan.rs:67` | `pub fn capture` | where a command takes its snapshot; the journal replay runs before it |
| `crates/vibra-host/src/system/fs.rs:1` | `//!` | the directory driver, the model of a confined writer over the adapter |

From step 07: `EditPlan`; from 11: `plan_replace`; from 01: the stubs
`edit::apply::{apply, publish_document}`, which step 13's `approve --write`
already calls. Proposed: their bodies, `Applied`, `Expectation`,
`edit::journal::{write, replay, JOURNAL_FILE}`.

## Files owned

- `crates/vibra-workspace/src/edit/apply.rs`
- `crates/vibra-workspace/src/edit/journal.rs`
- `crates/vibra-workspace/src/format_plan.rs`
- `crates/vibra-workspace/tests/it/edit_apply.rs`
- `crates/vibra-cli/tests/it/edit_apply.rs`
- `crates/vibra-conformance/tests/it/engine_writes.rs`
- `conformance/rules/m3-14.tsv`

## Ordered tasks

1. `replay(workspace)`: before any capture, look for `JOURNAL_FILE` at the
   root through the held root; a complete record is replayed (each
   document at its old digest is replaced through `open_for_replace` and
   `replace_contents`; one at its new digest is left; one at neither
   refuses the replay and leaves the journal, `@command.operationalFailure`);
   then the journal is removed; a record that does not decode is removed
   and one line is written to standard error. Invariant: replay is
   idempotent.
2. `apply(workspace, plan, expectation)`: `--expect-revision` against the
   capture's revision, else `Applied::Stale` with `@tool.stalePlan`; every
   document's path is a module of a target or `@tests`, or the approval or
   journal path; no document is protected; the root is opened once; for
   each document in path order, `open_for_replace` from the root one name
   at a time, `real_names` equal the plan's path components, bytes read
   through the handle equal the plan's revision and each edit's
   fingerprint, else `Stale` with nothing written; the journal is created
   exclusively and flushed with every new content; then each handle's
   `replace_contents`; then the journal is removed; `Applied` with the new
   revision. On a platform with no adapter, `Unavailable`. Invariant: no
   document is replaced before the journal is durable; nothing is created,
   removed, or renamed but the journal.
3. `publish_document(workspace, path, bytes)`: a one-document publish
   through the same path, which `fmt --write` and the call site in
   `approve.rs` (step 13) use; `format_plan::write` is removed. Until this
   step both answered `@tool.unavailable` through the stub of step 01.
4. `vibra-cli::edit`: `--write` applies after a preview with no resulting
   error and no denial; `applied`, `resultingRevision`.
5. `engine_writes.rs`: every call in `vibra-workspace/src` that creates,
   writes, replaces, or removes a file is in `edit/apply.rs` or
   `edit/journal.rs`; the test reads the sources.

## Test matrix

Host tests on Windows; elsewhere each `ATK-` row prints `NOT RUN: no scoped
directory adapter`. `P` is the plan of `V2-TOOL-edit-replace-body` applied
to a temporary copy of its tree.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| apply one document | positive | `P --write` | the file holds the new bytes; `applied: true`; `resultingRevision` is the new tree's revision; no journal remains | 05: "publish all files, or none" |
| `ATK-edit-apply-expect-revision` | negative | `--expect-revision` of another revision | `@tool.stalePlan`, `applied: false`, bytes unchanged | 05: "A plan that is stale … fails with nothing written" |
| `ATK-edit-apply-stale-document` | negative | the module changed between the capture and the publish (a hook between them) | `@tool.stalePlan`, unchanged | 05 step 1: "verify the workspace revision and every document revision" |
| `ATK-edit-apply-document-changed` | negative | a second document of a two-document plan changed | nothing written, the first included | 05: "or none" |
| `ATK-edit-apply-fingerprint-mismatch` | negative | a plan whose edit fingerprint is altered | refused before any write | Q13 |
| `ATK-edit-journal-crash-between-files` | negative | a hook aborts after the first of three replacements | the journal is present; the next command replays, the three files hold the new bytes, the journal is gone; the aborted command reported nothing applied | 05: "An interrupted application is never reported as applied" |
| `ATK-edit-journal-replay-idempotent` | positive | the replay run twice | the second changes nothing | architecture, "The theories" |
| `ATK-edit-journal-malformed` | negative | a truncated journal | removed; one line on standard error; no document touched; the command proceeds | Q15 |
| `ATK-edit-journal-foreign-document` | negative | a journal naming a document at neither digest | the replay refuses, the journal stays, `@command.operationalFailure` | Q15 |
| `ATK-edit-apply-symlink-refused` | negative | the target module replaced by a symbolic link to a file outside | refused; the outside file unchanged | 03 rule 1: "without following symbolic links, junctions, or other reparse points" |
| `ATK-edit-apply-alias-refused` | negative | the module reached by a short-name alias in the plan's path | refused | 03 rule 2: "A different-case or short-name alias MUST NOT satisfy a name test the real name fails" |
| `ATK-edit-apply-dotdot-refused` | negative | a plan document `../x.vib` | refused before opening | 05: "Escapes through a parent … are rejected" |
| `ATK-edit-apply-dependency-refused` | negative | a plan document under `dep/` | refused | 05: "never vendored dependencies" |
| `ATK-edit-apply-protected-unchanged` | negative | a plan whose document is a protected test module, built with the plan-time check bypassed by a hook | the publish refuses; bytes unchanged | 05: "No fix or format may alter a protected … test" |
| `ATK-fmt-write-protected-denied` | negative | `fmt tests/main.vib --write` under a current approval | `@tool.actionDenied`, unchanged | the same |
| `ATK-edit-writers-enumerated` | negative | `engine_writes.rs` given a `query/index.rs` that calls `std::fs::write` | the test reports it | architecture, I6, I9 |
| `ATK-edit-apply-no-create-no-remove` | negative | the apply path's sources | the only creation is the journal's, the only removal the journal's, no rename | 03 rule 4; architecture section 3 |
| `ATK-edit-apply-two-publishes` | negative | two applies started with a journal left by the first | the second replays first, then proceeds | Q15 |
| `edit_apply::not_run_off_windows` | boundary | every row above on a platform with no adapter | each prints `NOT RUN`; `--write` is `@tool.unavailable` with the preview returned | Q16 |
| `ATK-fmt-write-through-apply` | positive | `fmt src/app/main.vib --write` | written through `apply`; `format_plan::write` no longer exists | architecture, I9 |
| JSON validates | positive | every envelope | validates | 05 |

## Diagnostics and schema changes

Codes: `@tool.stalePlan`, `@tool.actionDenied`, `@tool.unavailable`,
`@module.ioError`. Schema: `@editJournal.v2` (new, VIBON, on disk only
during a publish; its schema is in the module's documentation and a decoder
test).

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-workspace -p vibra-cli -p vibra-conformance --all-features -- --nocapture
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: exit 0. Count the `NOT RUN` lines; on Windows there are none but
`ATK-approve-off-windows-preview`'s.

## Stays unavailable

`--write` off Windows.

## Excluded

Rename, imports, fix, lint (19 to 24); MCP.

## Completion evidence

- [ ] Each row is a test, named in the report; each `ATK-` row was seen to
      fail with its protection removed, and the report says how.
- [ ] The report quotes `apply` showing the journal's flush before the
      first `replace_contents`.
- [ ] `git grep -n "std::fs::" crates/vibra-workspace/src` shows the
      allowed sites only.
- [ ] The validation commands exit 0.
