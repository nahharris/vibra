# Step 16: approve-write

Kind: vertical. `vibra approve TARGET --write`: the approval document of
step 14 written through the publisher to the one destination the target's
name derives, after a renewal check in which this target's approval
diagnostics are set aside and every other error refuses; a separate
operation that no plan's `--write` implies.
Depends on: 14, 15.
Crates: `vibra-workspace`, `vibra-cli`.

## Prerequisites

- Read: [05 Snapshot, claim and witness identity](../../../../spec/05-tooling.md#snapshot-claim-and-witness-identity)
  (the last paragraphs); [05 MCP server](../../../../spec/05-tooling.md#mcp-server)
  ("A permission makes no grant, widens no resource scope, and replaces no
  target approval"); [architecture](../architecture.md) section 3, rows
  I9, I10, I14; [questions](../spec-questions.md) Q7, Q8, Q9, Q40.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-workspace/src/semantic.rs:80` | `pub fn check_snapshot` | the check the renewal runs, with this target's approval set aside |
| `crates/vibra-workspace/src/binding.rs:115` | `pub fn plan_of` | the recipes the written document names |

From step 14: `present::approval_document`; from 15:
`edit::apply::publish_approval`, the lock. Proposed:
`approve_write::write_approval`; `vibra-cli::approve::write::run`.

## Files owned

- `crates/vibra-workspace/src/approve_write.rs`
- `crates/vibra-workspace/tests/it/approve_write.rs`
- `crates/vibra-cli/src/approve/mod.rs`
- `crates/vibra-cli/src/approve/write.rs`
- `crates/vibra-cli/tests/it/approve_write.rs`
- `conformance/rules/m3-16.tsv`

## Ordered tasks

1. `write_approval(workspace, target, expectations)`: the renewal check
   runs `check_snapshot` with the approval diagnostics of this target set
   aside (`@project.staleApproval` of its document, `@grant.invalidBinding`
   reported against its bindings) and every other error refusing as
   `@command.diagnostics`; with `expectations.approval` set, the existing
   approval's identity must equal it, else `@project.staleApproval` and
   nothing written; then the document of step 14 is computed from the
   checked snapshot and published through `publish_approval` under the
   write lock, with the commit point's second capture equal to the checked
   one, else `@tool.stalePlan`. Invariant: the destination is derived from
   the target's name; no plan is involved; the lock is taken.
2. `vibra-cli::approve::write`: `--write` answers the payload with
   `written: true`, `path: "approvals/<target>.vibon"`, and `approval`
   showing the previous state and the new identity; `SUPPORTED` of the
   write arm flips. The human format prints the path and the identity.
3. The sentence of the architecture: creating or replacing an approval is
   its own operation; `edit … --write`, `fmt --write`, and `lint --write`
   never write an approval, which `engine_writes.rs` of step 15 shows by
   the one call site of `publish_approval`.

## Test matrix

Host tests on Windows over a copy of `examples/gate`; elsewhere each row
prints `NOT RUN`.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `ATK-approve-write-creates-document` | positive | `approve @gate --write` on a copy with an empty `approvals/` | `approvals/gate.vibon` decodes to the presentation's bindings and the digests of every file of the `@tests` unit; a following `check` is accepted and names its identity; `written: true` | Q7, Q40 |
| `ATK-approve-write-then-stale` | positive | after the write, one test module byte changed, `check` | `@project.staleApproval` | Q9 |
| `ATK-approve-write-refused-on-error` | negative | `--write` on a project with a type error | `@command.diagnostics`, nothing written, no lock remains | Q8 |
| `ATK-approve-write-renewal` | positive | after a binding's policy changed, so that `check` reports `@grant.invalidBinding`, `approve @gate --write` | the renewal sets that diagnostic aside, writes the new document, and a following `check` is accepted; the payload's `approval` shows the previous state `@stale` and the new identity | Q8: "a check in which every error but the approval diagnostics of this target refuses" |
| `ATK-approve-write-renewal-keeps-other-errors` | negative | the same with a type error in a module as well | `@command.diagnostics` with the type error; nothing written | the same |
| `ATK-approve-write-expected-mismatch` | negative | `approve @gate --write --expect-approval <another digest>` with a current approval | `@project.staleApproval`; the file unchanged | Q40 |
| `ATK-approve-write-expected-matches` | positive | the same option with the current approval's identity, after a binding change | written | Q40 |
| `ATK-approve-write-no-directory` | negative | `--write` on a copy with no `approvals/` | `@command.operationalFailure` with `@project.ioError` at `approvals/gate.vibon`; no directory created | Q7: "the writer never creates a directory" |
| `ATK-approve-write-creates-no-grant` | negative | the write with the broker hook | zero `Context::new` calls; the written document holds digests and identities only | 06: decoding a document "MUST NOT restore a live grant" |
| `ATK-approve-write-through-publisher` | positive | the write with the write-site test of step 15 | `publish_approval` is the one call site; the lock was taken and released; a leftover lock refuses the write | architecture, I9; Q17 |
| `ATK-approve-write-not-by-plan` | negative | `edit body … --write`, `fmt --write`, and `lint --write` on the copy after a protected change made `check` fail | no approval is written or changed by any of them | Q8: "Creating or replacing an approval is its own operation" |
| `ATK-approve-write-two-targets` | positive | a copy with a second binary target; `approve @other --write` | `approvals/other.vibon` written; `approvals/gate.vibon` unchanged | Q7: "One `@approval.v2` record per approved target" |
| `ATK-approve-off-windows-preview` | positive | the preview on a platform with no directory driver | the preview is computed; `--write` is `@tool.unavailable` there (Q16); on Windows the row prints `NOT RUN` | Q16 |
| JSON validates | positive | every envelope | validates | 05 |

## Diagnostics and schema changes

Codes: `@project.staleApproval`, `@grant.invalidBinding`, `@tool.stalePlan`,
`@project.ioError`, `@tool.unavailable`. Schema: none new; `written` and
`path` of the `approve` payload are produced for the first time.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features -- --nocapture
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; `help` lists `approve` with `--write` supported.

## Stays unavailable

`approve --write` off Windows.

## Excluded

MCP and its permissions; a signature on the document.

## Completion evidence

- [ ] `conformance/rules/m3-16.tsv` has one `active` row per case id.
- [ ] `git grep -n "publish_approval" crates/vibra-workspace/src` shows the
      definition and one call site, in `approve_write.rs`.
- [ ] The validation commands exit 0.
