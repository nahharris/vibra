# M2 Step 14 exit evidence

Captured 2026-09-23 for [Step 14](14-evidence.md). The implementation and
process tests were run from the clean candidate source revision
`0521364dd920fdd2f20aefef897aebca778e8d6d`. Its integration base was the
verified Step 13 merge `d12cb6cd66630436e4503a899873a688e3502ad3` on `m2`.
The M2 branch descends from the M1 merge `9c77b8642e1a7f3f4d8aab1eb0cd709bbbefebf6`
(PR #283). PR #302's final head `f0d7785996bff546dc43db26da176871ae2d2732`
merged to `m2` as `6132925ed4024d7a85ed82bf657ce3b313eb3054`. Final-head CI
[run 35927423703](https://github.com/nahharris/vibra/actions/runs/35927423703)
passed all five jobs: Ubuntu, Windows, macOS, reader corpus, and archive
boundary. The full PR history and checks remain visible at
[PR #302](https://github.com/nahharris/vibra/pull/302) and its
[check page](https://github.com/nahharris/vibra/pull/302/checks).

## Integration and decision audit

The step PRs that delivered M2 Steps 9–14 are:

| Step | PR and verified merge | Merged validation / CI |
| --- | --- | --- |
| 9 | [#296](https://github.com/nahharris/vibra/pull/296), `5392ad0bf7dcb9867d4dd123aa1b03fd706f22e9` | 162-case corpus; all five checks passed, including macOS in [run 34131459465](https://github.com/nahharris/vibra/actions/runs/34131459465). |
| 10 | [#297](https://github.com/nahharris/vibra/pull/297), `fdbf6d22fec20c9c300791ad98128583e44c5a8c` | 164-case corpus; all five checks passed in [run 35803840805](https://github.com/nahharris/vibra/actions/runs/35803840805). |
| 11 | [#299](https://github.com/nahharris/vibra/pull/299), `c624c3d216c2e1c0e7237dff1fc4590d5889c802` | 165-case corpus and 429 workspace tests; all five checks passed in [run 35858148272](https://github.com/nahharris/vibra/actions/runs/35858148272). |
| 12 | [#300](https://github.com/nahharris/vibra/pull/300), `ed1b7ae868c44cc0c58c96e184ffd6f7d0ab5cb5` | 173-case corpus; all five checks passed in [run 35878876006](https://github.com/nahharris/vibra/actions/runs/35878876006). The PR description called macOS broken, but its attached macOS check succeeded. |
| 13 | [#301](https://github.com/nahharris/vibra/pull/301), `d12cb6cd66630436e4503a899873a688e3502ad3` | 178-case corpus; all five checks passed in [run 35903397276](https://github.com/nahharris/vibra/actions/runs/35903397276). |
| 14 | [#302](https://github.com/nahharris/vibra/pull/302), merged as `6132925ed4024d7a85ed82bf657ce3b313eb3054` | Final-head run [35927423703](https://github.com/nahharris/vibra/actions/runs/35927423703) passed Ubuntu, Windows, macOS, reader corpus, and archive-boundary jobs. The implementation and process tests ran from source commit `0521364dd920fdd2f20aefef897aebca778e8d6d`. |

GitHub's M2 pull request history contains no Step PRs for Steps 1–8. Those
steps are present in the `m2` history at the integration revisions recorded in
the [step table](README.md#steps), but were pushed directly before the per-step
PR workflow was enforced. The workflow was followed from Step 9 onward. PR #298
is a separate M3–M7 roadmap document and is not an M2 step. This historical
process gap cannot be represented as eight already-merged PRs; it is recorded
here rather than inferred from commit history. The implementation and gate
tests for those steps are included in the final workspace and corpus checks.

The [decision ledger](decision-ledger.md) contains C1–C12. Each decision row
links to its owning normative section in `docs/spec/`; the M2 admission,
availability, bootstrap, assertion, command, trap, and test contracts are
closed there. A heading audit resolved all 60 normative spec links in the
ledger to existing files and anchors. Step 14 adds no diagnostic code or wire
field: it reuses `@tool.unavailable` and the existing command-result schema.
The supported and deferred AST inventory is in [supported-surface.md](supported-surface.md),
and the exact deferred-form cases and source spans are in
[availability-audit.md](availability-audit.md). The M1 syntax-example inventory
was mechanically refreshed for the implementation-status and tooling prose
updates. The reader-example counts and classifications are unchanged; the
updated line ranges and hashes pass the evidence test. Active graph snapshots
use `.vibon` `@source-graph.v1`, and pure runtime trace snapshots use `.vibon`
`@audit-trace.v1`; there are no text graph or trace snapshots in the active
conformance tree.

A post-merge review found that the manifest decoder did not enforce the
specified `.vibon` extension for interpreter and Wasm audit-trace paths.
Commit `0e89b56d275058f016e9684a7aaa36b56d59d86f` closes that gap. The
`corpus_step3::execution_audit_snapshots_require_vibon_extensions` regression
accepts `.vibon` and rejects `.txt` for both expectation fields. The updated
full workspace suite reports 527 passed, 0 failed, and 5 ignored; cumulative
M2 CI on head `0e89b56d275058f016e9684a7aaa36b56d59d86f` passed all five jobs in
[run 35928991639](https://github.com/nahharris/vibra/actions/runs/35928991639).

## Gate-to-test map

The final Step 14 PR run covers its candidate head; the snapshot-extension row
also links cumulative M2 CI after the post-merge guard. Both runs include the
three platform checks, independent reader corpus, and archive boundary check.

| Roadmap obligation | Conformance cases and host tests | Final-head PR CI |
| --- | --- | --- |
| Project schema, discovery, and schema-selected atom roles | `V1-PROJECT-static-schema-order`, `V1-PROJECT-static-variants`, `V1-PROJECT-static-unknown-field`; workspace project decoder tests | [PR #302 checks](https://github.com/nahharris/vibra/pull/302/checks) |
| Unit-rooted graph, imports, roots, and visibility | `V1-PROJECT-graph-static-*`, `V1-TYPE-NAMES-resolve-*`; workspace and resolver host suites | [PR #302 checks](https://github.com/nahharris/vibra/pull/302/checks) |
| Primitive types, ranges, bindings, and control flow | `V1-TYPE-INFER-*`, `V1-TYPE-CONTROL-*`, `V1-RUNTIME-bindings`, `V1-RUNTIME-if-selected`; type, IR, and interpreter workspace tests | [PR #302 checks](https://github.com/nahharris/vibra/pull/302/checks) |
| Function values, closures, and labelled calls | `V1-SRC-CALLS-functions-*`, `V1-RUNTIME-functions-closures`; `functions_step7` and type/IR/interpreter suites | [PR #302 checks](https://github.com/nahharris/vibra/pull/302/checks) |
| Compiler externals and signed pure bootstrap | `V1-RUNTIME-external-std-text-concat`, `V1-RUNTIME-external-std-text-length`, `V1-RUNTIME-external-untrusted`; IR, interpreter, and workspace tests | [PR #302 checks](https://github.com/nahharris/vibra/pull/302/checks) |
| Tail calls and bounded activation depth | `V1-RUNTIME-tail-mutual`, `V1-RUNTIME-tail-negative`; `tail_calls_step9::source_tail_counter_keeps_bounded_depth_at_two_workload_sizes` | [PR #302 checks](https://github.com/nahharris/vibra/pull/302/checks) |
| Semantic position metadata and schema consumers | `V1-TOOL-workspace-position-*`; `semantic_queries_step10`, `queries_step10`, and `cargo test --locked --offline -p vibra-schema` | [PR #302 checks](https://github.com/nahharris/vibra/pull/302/checks) |
| Project init and source formatting | `V1-TOOL-format-safe-label-order`, `V1-TOOL-format-imported-source`; `format_plan_step11`, `process_step11`, and `process_step14` | [PR #302 checks](https://github.com/nahharris/vibra/pull/302/checks) |
| Check, run, pure test, and structured assertion failure | `V1-PROJECT-workspace-check-*`, `V1-RUNTIME-workspace-run-*`, `V1-RUNTIME-workspace-test-*`; `workspace_semantic_step12`, `workspace_test_step13`, `process_step12`, `process_step13`, and `process_step14` | [PR #302 checks](https://github.com/nahharris/vibra/pull/302/checks) |
| Static/interpreter/tooling profile dispatch | Full corpus command below: reader 73, static 97, interpreter 24, tooling 4; `architecture_boundary` and conformance host suites | [PR #302 checks](https://github.com/nahharris/vibra/pull/302/checks) |
| Pure execution has no host events or ambient reads | `V1-RUNTIME-tail-*`, `V1-RUNTIME-workspace-run-*`, and workspace test cases; tail and process tests assert stable results and empty event arrays | [PR #302 checks](https://github.com/nahharris/vibra/pull/302/checks) |
| Deferred forms remain explicit availability results | All rows in [availability-audit.md](availability-audit.md); `evidence_step11` and full corpus | [PR #302 checks](https://github.com/nahharris/vibra/pull/302/checks) |
| Graph and audit snapshots use only canonical VIBON paths | `V1-PROJECT-graph-static-*`, `V1-RUNTIME-workspace-run-*`; `corpus_step3::execution_audit_snapshots_require_vibon_extensions` rejects `.txt` for interpreter and Wasm traces | [PR #302 checks](https://github.com/nahharris/vibra/pull/302/checks); cumulative [M2 run 35928991639](https://github.com/nahharris/vibra/actions/runs/35928991639) |
| Clean repeated multi-module CLI demo | `process_step14::actual_binary_positive_demo_repeats_in_two_fresh_hello_workspaces`; `process_step14::actual_binary_run_preflight_failures_never_produce_a_program_result`; `process_step14::actual_binary_failing_assertion_is_a_structured_test_failure` | [PR #302 checks](https://github.com/nahharris/vibra/pull/302/checks) |
| Architecture, archive, and interchange schema boundary | `architecture_boundary` checks the workspace dependency graph, archive exclusion, and archive references; all process tests validate one command envelope against `COMMAND_RESULT_SCHEMA` | [PR #302 checks](https://github.com/nahharris/vibra/pull/302/checks) |

## Local validation

All commands ran from the repository root with the pinned Rust toolchain and
locked dependencies, after dependency preparation. The tests and corpus were
offline. Every command exited 0 unless a contained CLI negative case explicitly
expects a nonzero process exit.

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | Pass |
| `cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo test --locked --offline --workspace --all-targets --all-features` | 527 passed, 0 failed, 5 ignored across 64 targets; the ignored tests require symlink privileges unavailable on this ordinary Windows host. Platform CI supplies the cross-platform checks. |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline --workspace --no-deps --all-features` | Pass |
| `cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases` | 198 passed: reader 73, static 97, interpreter 24, tooling 4; 0 failed and 0 unavailable. |
| `cargo test --locked --offline -p vibra-cli --test process_step14` | 3 passed; actual-binary demo and negative outcomes. |
| `cargo test --locked --offline -p vibra-conformance --test format_plan_step11` | 1 passed. |
| `cargo test --locked --offline -p vibra-conformance --test evidence_step11` | 2 passed; M1 examples remain losslessly inventoried. |
| `cargo test --locked --offline -p vibra-conformance --test tail_calls_step9 source_tail_counter_keeps_bounded_depth_at_two_workload_sizes -- --exact --nocapture` | 1 passed; 13 other tests filtered out. |
| `cargo test --locked --offline -p vibra-conformance --test architecture_boundary` | Pass; architecture, archive exclusion, and archive dependency checks. |
| `cargo test --locked --offline -p vibra-schema` | Pass; schema producers and consumers. |
| `cargo run --locked --offline -p vibra-conformance --bin m1-fuzz -- --profile ci-smoke` | Pass; 6 targets and 96 cases. |
| `git diff --check` | Pass. |

The type checker verifies the exact signed bootstrap files under an explicit
root as a C8 trust check. The runtime audit is separate: the interpreter
consumes checked in-memory IR, and a source scan found no filesystem,
environment, process, or clock APIs in `vibra-ir/src` or `vibra-interp/src`.
The pure program boundary cannot call the bootstrap verifier or a host provider.

### Tail workload evidence

The checked source workload in `tail_calls_step9.rs` runs a binary counter at
15 and 17 bits: 32,768 and 131,072 tail transfers. Both executions return
`0i32`, report maximum activation depth `1`, and have an empty
`@audit-trace.v1` event list. The focused test above completed in 3.89 seconds
on this Windows host. The real `interpreter-v1` handler separately runs the
17-bit workload against authored result and empty-trace VIBON snapshots.

### Actual-binary demo evidence

The process suite runs the real `CARGO_BIN_EXE_vibra` binary in two newly
created, empty temporary directories named `hello`; it uses JSON mode and
removes only each unique temporary parent. Each pass initializes the project,
adds a public helper plus explicit import, previews and writes formatting,
checks the target, runs its pure entry, and runs a passing assertion test.
Every command emits exactly one schema-valid JSON envelope. Init creates
`project.vibon`, `src/`, `src/hello/`, `src/hello/main.vib`, and `tests/`. The
formatter preview reports `changed: true`, `written: false`, and leaves input
bytes unchanged. `fmt --write` reports `changed: true`, `written: true`, and
installs exactly the bytes returned by preview. The checked target is accepted
with no diagnostics; run exits 0 with the canonical void result, empty stdout,
empty stderr, empty audit trace, and no trap. The selected assertion suite
exits 0 with one selected and one passed test, no failures, and an empty trace.

The negative process checks expect: a primitive mismatch (exit 1,
`@type.argument-mismatch`), private access (exit 1,
`@name.private-access`), host/dependency availability (exit 4,
`@tool.unavailable`), and a failing assertion (exit 1,
`@command.test-failed` / `@test.assertion-failed`). Each `run` preflight
failure returns no program result, program output, trace event, or trap; the
assertion failure retains its structured item result with an empty trace and
null trap. The type-mismatch source places a nonterminating call
before the error and uses a 10-second external watchdog, proving that preflight
rejects the program before execution. JSON diagnostics also appear on stderr
in envelope order. The imported-source formatted snapshot is 112 bytes with SHA-256
`80aa0239f6f765d9189a91b3d39281a5a9d632527d878d25d1f9068929849aea`.

## Exit result

The current M2 gate corpus has nonempty reader, static, interpreter, and tooling
profiles and zero failed or unavailable cases. The captured Step 14 CI run
`35924662446` on source revision `0521364` passed all three platform jobs, the
independent reader corpus job, and the archive-boundary job. The linked PR
checks page tracks the final report head. The standing
[M2-to-main PR #295](https://github.com/nahharris/vibra/pull/295) remains the
single release-review vehicle; its live base, head, and review status are shown
by GitHub.

## Post-review remediation

The owner review of [PR #295](https://github.com/nahharris/vibra/pull/295)
found six defects, three plan or specification gaps, and several performance
and structure issues. Each was addressed on `m2` after the Step 14 exit
evidence above:

- The toolchain verifies bootstrap bytes embedded at build time. The pure
  verifier decodes the manifest and signed artifact into typed records and
  requires the exact Ed25519 SPKI prefix. `vibra-types` performs no filesystem
  I/O, and an architecture test enforces this for every semantic crate.
- An initializer cycle through a closure-valued global is a static
  `@type.initializer-cycle`
  (`V1-TYPE-INFER-initializer-cycle-closure-global`,
  `V1-PROJECT-workspace-check-closure-global-initializer-cycle`). Checked IR is
  the single initializer-cycle and recursive-group authority, and call-flow
  analysis fails closed if it does not converge.
- Human `vibra test` reports each non-passing item and a summary. `vibra help`
  prints the closed grammar.
- Non-tail recursion stops at the interpreter's host budget with
  `@runtime.host-stack-exhausted` (exit 3) instead of aborting the process.
- The interpreter threads one frame by reference and shares closure bodies.
  One validated `CheckedModuleSet` serves every entry and test. Recursive
  groups use an SCC condensation. Call-flow analysis runs on a dependency
  worklist, and the test runner uses indexed worklist closures.

After remediation, the full workspace suite reports 557 passed, 0 failed, and
0 ignored across 66 targets on Linux. The independent corpus reports 200
passed (reader 73, static 99, interpreter 24, tooling 4) with 0 failed or
unavailable. `check_paths_agree` compares both check paths over the
single-source corpus.
