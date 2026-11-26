# M2 exit record

The evidence that M2 meets the [exit evidence](README.md#exit-evidence) of
its README, except the model-writing pilot, which steps 38 to 41 run and
record. Each line names the case, the host test, or the evidence file that
shows it. The commands ran on Windows 11 in the worktree of
`m2/37-m2-exit`, on `v0.0.2/m2-ops` at `0bcc6514` with the commits of step
37, with `--locked --offline`, `CARGO_BUILD_JOBS=4`, and
`RUSTFLAGS=-D warnings` for clippy and the tests.

## Status

The gate of M2 is evidenced: every clause of the table below except the
pilot has its cases and host tests, and they pass. The pilot's dated
observation does not exist yet, so the milestone has not exited.

## Exit clauses

| Exit clause | Status | Evidence |
| --- | --- | --- |
| the release program runs under `vibra run` | met | `examples/gate/`; [release program](evidence/release-program.md); `V2-GRANT-release-optional-absent`; `gate::run`, `gate::the_world_case_runs_the_release_program`, and the other tests of `crates/vibra-cli/tests/it/gate.rs` |
| rejected: a callback whose row exceeds its parameter's row | met | `V2-EFFECT-callback-row-wider`; `gate::showing_the_callback_row` |
| rejected: an operation outside a ceiling | met | `V2-EFFECT-outside-ceiling-direct`; `gate::showing_the_callback_cannot_read`, `gate::rejection_operation_outside_a_ceiling` |
| rejected: an entry row outside the admission ceiling | met | `V2-EFFECT-entry-outside-ceiling`; `ATK-admit-entry-row-outside-ceiling`; `gate::rejection_entry_row_outside_the_admission_ceiling` |
| rejected: a missing or cyclic binding | met | `V2-GRANT-binding-missing`, `V2-GRANT-binding-cycle`; `gate::rejection_missing_binding`, `gate::rejection_cyclic_binding` |
| a value: a request outside a grant's policy | met | `V2-GRANT-denied-outside-policy`, `V2-GRANT-denied-is-value`; the `denied` read of `c.md` in `V2-GRANT-release-optional-absent`; `gate::showing_typed_denial` |
| a value: a refused optional grant | met | `V2-RUNTIME-run-optional-absent`, `V2-GRANT-release-optional-absent`; `gate::optional_absent` |
| a value: an exhausted shared limit | met | `V2-GRANT-limit-siblings-share`, `V2-GRANT-limit-exceeded-is-value`; `gate::showing_exhausted_shared_limit` |
| file-scope attack cases through the real adapter | met | `ATK-fs-*` of `vibra-fs-scope`, `ATK-fs-*` and `ATK-gate-*` of `vibra-host`, `ATK-run-*` of `vibra-workspace`; [dual run](evidence/dual-run.md); [review of the drivers](evidence/review-drivers.md) |
| file-scope attack cases in the fake world | met | `V2-GRANT-prefix-by-names`, `V2-GRANT-suffix-exact-scalars`, `V2-GRANT-rule-correlation`, `V2-GRANT-file-rule-no-listing`, `V2-GRANT-sub-escape-denied`, `V2-RUNTIME-run-denied-by-recipe-policy` |
| no grant from data | met | `ATK-ir-grant-constant`, `ATK-trace-decode-grants-nothing`, `V2-GRANT-no-constructor`, `V2-GRANT-brand-forged`; `gate::no_grant_from_data` |
| no grant from a row alone | met | `V2-GRANT-test-without-world`; `gate::no_grant_from_a_row` |
| no grant outside a test world by test code | met | `ATK-test-runner-no-system-driver`, `ATK-test-world-op-on-leaf` |
| the pilot's dated observation | not met | steps 38 to 41 |

## Both suites

`cargo run -p vibra-conformance --bin vibra-conformance -- --root
conformance/cases` exits 0:

| Profile | Passed | Failed | Unknown | Unavailable |
| --- | --- | --- | --- | --- |
| `readerV2` | 178 | 0 | 0 | 0 |
| `staticV2` | 690 | 0 | 0 | 0 |
| `interpreterV2` | 336 | 0 | 0 | 0 |
| `toolingV2` | 9 | 0 | 0 | 0 |
| total | 1213 | 0 | 0 | 0 |

No required case under `staticV2` or `interpreterV2` is failed, unknown, or
unavailable. The two cases whose expected verdict is `unsupported` hold no
M2 form ([What stays unavailable](#what-stays-unavailable)).

The host suite is the test command of step 37 over the nine crates M2
touched:

```powershell
cargo test --locked --offline -p vibra-ir -p vibra-interp -p vibra-types -p vibra-fs-scope -p vibra-host -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
```

It exits 0. It prints 30 `test result:` lines, all `ok`, with 1743 tests
passed, 0 failed, and 1 ignored: `vibra-fs-scope`
`controls::no_handle_kept_child`, which `controls::no_handle_kept` runs
in a child process. The output holds one `NOT RUN` line, listed under
[NOT RUN](#not-run).

The other commands:

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | exit 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit 0, no warning |
| `cargo doc --workspace --no-deps --all-features` with `RUSTDOCFLAGS=-D warnings` | exit 0 |
| `pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check` | exit 0 |

## Coverage audit

`crates/vibra-conformance/tests/it/m2_coverage.rs` reads the [owner
table](../m0/owners.md), the [authority
argument](architecture.md#2-the-authority-argument), required coverage 10,
11, and 12 of the [diagnostics
chapter](../../../spec/07-diagnostics-and-conformance.md#required-coverage),
the rule index, and the corpus, and reports every gap at once. Its eight
tests pass:

| Test | What it checks |
| --- | --- |
| `m2_owner_forms_have_cases` | each of the 26 owner rows whose step names M2: every case identity and `*` pattern matches a case, every `ATK-` and `module::function` test exists, and the row has an accepted case and, where it names a diagnostic, a rejected one |
| `m2_owner_gap_detected` | the same audit, given the owner table with one invented case identity, reports exactly that identity |
| `m2_diagnostics_have_cases` | each of the 15 codes the owner table gives to M2 is expected by a case, or is one of two codes no case can produce, each with the host test that asserts it |
| `m2_authority_rows_have_cases` | the 31 invariants; each `V2-` identity is an active case or one of two host-only identities, each `ATK-` identity a host test |
| `m2_coverage_10`, `m2_coverage_11`, `m2_coverage_12` | each item of the chapter's text, in order, has at least one case or host test |
| `m2_no_unavailable_m2_form` | every case that expects `@tool.unavailable` is listed with its form and owner, and none is an M2 form |

The audit found three gaps. Each was closed by a case whose expectations are
written by hand from the chapters, with its row in
`conformance/rules/m2-37.tsv`:

| Case | Gap | Expected |
| --- | --- | --- |
| `V2-RUNTIME-await-twice-two-events` | authority row A29 and coverage 10 named a corpus case the corpus did not have (step 20 wrote the row as a host test) | one stored write plan awaited twice: two `@host` events, invocations 2 and 3, parent 1, each origin its own `await` (03 "Delivery and typed outcomes") |
| `V2-GRANT-startup-order-permuted` | authority row A19 and coverage 12 (step 28 wrote the row as a host test) | `grants` written `@z`, `@a`, `@b` with `@a` after `@b`: acquired `@b`, `@a`, `@z` (04 "Startup order") |
| `V2-GRANT-test-entries-sorted` | coverage 12, "sorted directory entries": every listing case listed names in the order they were created | `z.txt` written before `a.txt` lists as `a.txt`, `z.txt` (06 "Closed operation semantics") |

The owner rows whose side is held by host tests, as the row says: the
rejected side of "Host registry of major 2" and of "Host world"; every side
of "Real drivers and the file scope on Windows", "The toolchain record of
the workspace revision", "A case names the profile of its operation", "The
JSON spelling of a code", and "Case operations of each conformance
profile".

### What no case can observe

The specification gives these no observation a case can hold. Each is a
host test, named in the audit with the sentence that makes it so:

| Identity or code | Host test | Why no case |
| --- | --- | --- |
| `V2-GRANT-startup-failed-optional` | `vibra-host` `startup::v2_grant_startup_failed_optional` | 06 "Recipes under a world": every recipe resolves against the world, whose resources always exist, so no failure of 04 "Startup order" has a world form |
| `V2-GRANT-release-order` | `vibra-host` `startup::v2_grant_release_order` | 06 "Entry outcome": release runs no program code and appends no event; only the driver sees its order |
| `@grant.acquisitionFailed` | `vibra-host` `startup::v2_grant_startup_failed_optional` | the same as `V2-GRANT-startup-failed-optional` |
| `@runtime.invalidHostValue` | `vibra-host` `broker::atk_forged_token` | 06 "Host calls": checked source cannot make an invalid, stale, or foreign boundary value |

## Authority argument

Each row of [section 2](architecture.md#2-the-authority-argument), with its
identities and their result in the suites above. A `V2-` identity is a
corpus case; an `ATK-` identity is a host test, named by its crate and
function.

| # | Invariant | Identities and results |
| --- | --- | --- |
| A1 | A body's computed row is contained in its written ceiling | `ATK-ir-await-outside-row`: `vibra-ir` `effects::atk_ir_await_outside_row`, passed<br>`V2-EFFECT-outside-ceiling-direct`: case, passed<br>`V2-EFFECT-outside-ceiling-callback`: case, passed |
| A2 | `await` runs only in a `proc` or a test | `ATK-ir-await-in-fn`: `vibra-ir` `effects::atk_ir_await_in_fn`, passed<br>`ATK-interp-await-in-pure-frame`: `vibra-interp` `plans::atk_interp_await_in_pure_frame`, passed<br>`V2-EFFECT-await-in-anon-fn`: case, passed |
| A3 | Callable types compare rows exactly | `ATK-ir-row-widened-at-call`: `vibra-ir` `effects::atk_ir_row_widened_at_call`, passed<br>`V2-EFFECT-callback-row-wider`: case, passed<br>`V2-EFFECT-callback-row-narrower`: case, passed |
| A4 | Building a plan runs nothing and consumes nothing | `V2-RUNTIME-plan-built-not-run`: case, passed<br>`ATK-broker-no-charge-without-await`: `vibra-interp` `host::atk_broker_no_charge_without_await`, passed |
| A5 | A host action needs a grant, and the whole policy is enforced | `ATK-policy-ancestor-dropped`: `vibra-host` `broker::atk_policy_ancestor_dropped`, passed<br>`ATK-driver-ignores-gate`: `vibra-host` `broker::atk_driver_ignores_gate`, passed<br>`V2-GRANT-denied-outside-policy`: case, passed<br>`ATK-fs-alias-name`: `vibra-host` `fs::atk_fs_alias_name`, passed |
| A6 | Missing authority never falls back to ambient access | `ATK-ambient-read-outside-system`: `vibra-conformance` `ambient_reads::atk_ambient_read_outside_system`, passed<br>`ATK-ambient-read-in-interp`: `vibra-interp` `host::atk_ambient_read_in_interp`, passed |
| A7 | A denial, an unsupported action, and an exhausted limit are values | `V2-GRANT-denied-is-value`: case, passed<br>`V2-GRANT-limit-exceeded-is-value`: case, passed<br>`ATK-driver-error-not-fault`: `vibra-host` `world::atk_driver_error_not_fault`, passed |
| A8 | No source expression makes a grant | `V2-GRANT-no-constructor`: case, passed<br>`V2-GRANT-model-is-not-grant`: case, passed<br>`ATK-forged-token`: `vibra-host` `broker::atk_forged_token`, passed<br>`ATK-ir-grant-constant`: `vibra-ir` `effects::atk_ir_grant_constant`, passed |
| A9 | A grant has no encoding | `V2-GRANT-assert-equal-grant`: case, passed<br>`V2-GRANT-dict-key-grant`: case, passed<br>`ATK-encode-grant`: `vibra-interp` `host::atk_encode_grant`, passed |
| A10 | A foreign or released grant is unusable, and so is every grant narrowed from a released one | `ATK-foreign-grant`: `vibra-host` `broker::atk_foreign_grant`, passed<br>`ATK-stale-grant`: `vibra-host` `broker::atk_stale_grant`, passed<br>`ATK-stale-derived-grant`: `vibra-host` `broker::atk_stale_derived_grant`, passed<br>`ATK-released-grant`: `vibra-host` `startup::atk_released_grant`, passed<br>`ATK-released-derived-grant`: `vibra-host` `startup::atk_released_derived_grant`, passed |
| A11 | Implementing an operation interface makes no grant | `V2-GRANT-model-is-not-grant`: case, passed<br>`V2-EFFECT-impl-forwards-wider`: case, passed |
| A12 | Only the embedded library binds a host symbol, with the registry's owner and signature | `V2-SRC-DECL-fn-external-outside-stdlib`: case, passed<br>`ATK-ir-host-body-wrong-owner`: `vibra-ir` `effects::atk_ir_host_body_wrong_owner`, passed<br>`ATK-stdlib-host-signature`: `vibra-types` `externals::atk_stdlib_host_signature_a_host_member_with_another_parameter_type_is_a_defect`, passed |
| A13 | Narrowing a sealed grant never adds a right, a resource, a driver, or a budget; the parent stays a conjunction; narrowing another value runs that value's own member | `ATK-narrow-broader-policy`: `vibra-host` `broker::atk_narrow_broader_policy`, passed<br>`ATK-narrow-other-resource`: `vibra-host` `broker::atk_narrow_other_resource`, passed<br>`V2-GRANT-restrict-keeps-parent`: case, passed<br>`V2-GRANT-narrow-forward-limit`: case, passed<br>`V2-GRANT-impl-missing-limit`: case, passed |
| A14 | Copies and descendants share ancestor counters; known progress is charged and a known remainder refunded; uncertain progress and an unknown outcome are charged in full; a counter that would overflow denies | `ATK-counter-overflow`: `vibra-host` `broker::atk_counter_overflow`, passed<br>`ATK-counter-refund-unknown`: `vibra-host` `broker::atk_counter_refund_unknown`, passed<br>`ATK-counter-partial-known`: `vibra-host` `broker::atk_counter_partial_known`, passed<br>`ATK-counter-partial-unknown`: `vibra-host` `broker::atk_counter_partial_unknown`, passed<br>`ATK-counter-reservation-all-or-none`: `vibra-host` `broker::atk_counter_reservation_all_or_none`, passed<br>`V2-GRANT-limit-siblings-share`: case, passed |
| A15 | The fields of one rule stay together | `ATK-policy-rule-fields-crossed`: `vibra-host` `policy::atk_policy_rule_fields_crossed`, passed<br>`V2-GRANT-rule-correlation`: case, passed |
| A16 | A policy over the size or depth limit is refused | `V2-GRANT-policy-4097-nodes`: case, passed<br>`V2-GRANT-restrict-too-complex`: case, passed |
| A17 | An entry row and every startup row are inside the target's `effects`; a grant the entry takes and never performs needs no entry of the list | `V2-EFFECT-entry-outside-ceiling`: case, passed<br>`V2-EFFECT-startup-row-outside-ceiling`: case, passed<br>`V2-GRANT-binding-unused-grant`: case, passed<br>`ATK-admit-entry-row-outside-ceiling`: `vibra-host` `startup::atk_admit_entry_row_outside_ceiling`, passed<br>`ATK-admit-startup-read-outside-ceiling`: `vibra-host` `startup::atk_admit_startup_read_outside_ceiling`, passed |
| A18 | Every leaf has a valid recipe from a provider that issues its type; `after` is acyclic; a process read names its source leaf; a recipe's leaf has the interface of the parameter it binds | `V2-GRANT-binding-missing`: case, passed<br>`V2-GRANT-binding-cycle`: case, passed<br>`ATK-plan-provider-mismatch`: `vibra-host` `startup::atk_plan_provider_mismatch`, passed<br>`ATK-admit-leaf-interface-mismatch`: `vibra-host` `startup::atk_admit_leaf_interface_mismatch`, passed |
| A19 | Startup follows the one order; a refused required leaf and every failure stop it; a failure is never an absence | `V2-GRANT-startup-order-permuted`: case, passed<br>`V2-GRANT-startup-failed-optional`: host test `vibra-host` `startup::v2_grant_startup_failed_optional`, passed<br>`ATK-startup-failure-not-none`: `vibra-host` `startup::atk_startup_failure_not_none`, passed |
| A20 | On every ending the acquired grants are released in reverse order, and an acquired resource has a teardown owner from the moment it exists | `ATK-release-on-trap`: `vibra-workspace` `run::atk_release_on_trap`, passed<br>`ATK-release-on-exhaustion`: `vibra-workspace` `run::atk_release_on_exhaustion`, passed<br>`V2-GRANT-release-order`: host test `vibra-host` `startup::v2_grant_release_order`, passed<br>`ATK-acquire-then-fail-leaks-nothing`: `vibra-host` `startup::atk_acquire_then_fail_leaks_nothing`, passed |
| A21 | A file request is resolved from the held directory, one name at a time, following no link | `ATK-fs-file-symlink`: `vibra-fs-scope` `attacks::atk_fs_file_symlink`, passed<br>`ATK-fs-junction`: `vibra-fs-scope` `attacks::atk_fs_junction`, passed<br>`ATK-fs-swapped-junction`: `vibra-fs-scope` `attacks::atk_fs_swapped_junction`, passed<br>`ATK-run-symlink-denied`: `vibra-workspace` `attacks_run::atk_run_symlink_denied`, passed |
| A22 | The name the filesystem reports decides a name test | `ATK-fs-case-alias`: `vibra-fs-scope` `attacks::atk_fs_case_alias`, passed<br>`ATK-fs-short-name`: `vibra-fs-scope` `attacks::atk_fs_short_name`, passed<br>`ATK-run-alias-denied`: `vibra-workspace` `attacks_run::atk_run_alias_denied`, passed |
| A23 | Nothing is truncated or created before the last authority check; create is exclusive | `ATK-fs-replace-no-truncate`: `vibra-fs-scope` `attacks::atk_fs_replace_no_truncate`, passed<br>`ATK-fs-create-over-alias`: `vibra-fs-scope` `attacks::atk_fs_create_over_alias`, passed<br>`ATK-fs-create-parent-swapped`: `vibra-fs-scope` `attacks::atk_fs_create_parent_swapped`, passed<br>`ATK-fs-create-denied-on-real-parent`: `vibra-host` `fs::atk_fs_create_denied_on_real_parent`, passed<br>`ATK-run-denied-leaves-file`: `vibra-workspace` `attacks_run::atk_run_denied_leaves_file`, passed<br>`ATK-run-create-denied-leaves-directory`: `vibra-workspace` `attacks_run::atk_run_create_denied_leaves_directory`, passed |
| A24 | No intermediate handle outlives an operation; authority follows the opened directory | `ATK-fs-root-renamed`: `vibra-fs-scope` `attacks::atk_fs_root_renamed`, passed<br>`ATK-run-root-replaced`: `vibra-workspace` `attacks_run::atk_run_root_replaced`, passed |
| A25 | Replacing a file with more than one link is `denied` | `ATK-fs-hard-link-replace`: `vibra-host` `fs::atk_fs_hard_link_replace`, passed<br>`ATK-gate-replace-without-link-count`: `vibra-host` `broker::atk_gate_replace_without_link_count`, passed<br>`ATK-run-hard-link-read-allowed`: `vibra-workspace` `attacks_run::atk_run_hard_link_read_allowed`, passed |
| A26 | Where the platform cannot give the guarantees the result is `unsupported`, before acting | `ATK-fs-no-normalized-name`: `vibra-host` `fs::atk_fs_no_normalized_name`, passed<br>`ATK-driver-absent-at-admission`: `vibra-host` `system::atk_driver_absent_at_admission`, NOT RUN on Windows: compiled only where the platform has no directory driver |
| A27 | A test obtains grants only from its `TestWorld`, and reaches no real provider | `V2-GRANT-test-without-world`: case, passed<br>`ATK-test-runner-no-system-driver`: `vibra-workspace` `test_runner::atk_test_runner_no_system_driver`, passed |
| A28 | A target cannot obtain a `TestWorld` | `V2-GRANT-entry-test-world-leaf`: case, passed<br>`ATK-test-world-op-on-leaf`: `vibra-host` `test_world::atk_test_world_op_on_leaf`, passed |
| A29 | An `@once` operation is never dispatched twice for one invocation | `V2-RUNTIME-await-twice-two-events`: case, passed<br>`ATK-no-redispatch-on-error`: `vibra-host` `world::atk_no_redispatch_on_error`, passed |
| A30 | A completed call is recorded once, in order, after its continuation advanced; a refusal before the adapter and a reply the caller rejects record nothing; a projection restores nothing | `V2-RUNTIME-trace-order`: case, passed<br>`V2-GRANT-denied-no-event`: case, passed<br>`ATK-trace-uncommitted-no-event`: `vibra-host` `trace::atk_trace_uncommitted_no_event`, passed<br>`ATK-trace-commit-once`: `vibra-host` `trace::atk_trace_commit_once`, passed<br>`ATK-startup-read-malformed-reply`: `vibra-host` `startup::atk_startup_read_malformed_reply`, passed<br>`ATK-trace-decode-grants-nothing`: `vibra-host` `trace::atk_trace_decode_grants_nothing`, passed |
| A31 | No operation terminates the process or returns `Never` | `ATK-no-process-exit`: `vibra-host` `system::atk_no_process_exit`, passed |

## Required coverage

### Coverage 10, effects and authority

| Item | Cases and host tests |
| --- | --- |
| Complete rows through callbacks, records, closures, and plans | `V2-EFFECT-outside-ceiling-callback`: case, passed<br>`V2-EFFECT-callback-row-wider`: case, passed<br>`V2-EFFECT-proc-type-field`: case, passed<br>`V2-EFFECT-field-row-mismatch`: case, passed<br>`V2-EFFECT-anon-proc-outside-ceiling`: case, passed<br>`V2-RUNTIME-anon-proc-captures`: case, passed<br>`V2-EFFECT-outside-ceiling-stored-plan`: case, passed<br>`V2-EFFECT-proc-plan-bound`: case, passed |
| owner plus additive row on interface members | `V2-EFFECT-member-additive-row`: case, passed<br>`V2-EFFECT-member-owner-in-row`: case, passed |
| no `await` in a `fn` | `V2-EFFECT-await-in-fn`: case, passed |
| building against running a plan | `V2-RUNTIME-plan-built-not-run`: case, passed<br>`V2-EFFECT-plan-discarded`: case, passed |
| guards with `else` run on each `await` | `V2-RUNTIME-requires-else-on-await`: case, passed<br>`V2-RUNTIME-ensures-else-on-await`: case, passed |
| each `await` is a new invocation, with no implicit retry | `V2-RUNTIME-plan-awaited-twice`: case, passed<br>`V2-RUNTIME-await-twice-two-events`: case, passed<br>`ATK-no-redispatch-on-error`: `vibra-host` `world::atk_no_redispatch_on_error`, passed |
| wrong, omitted, and duplicate rows | `V2-EFFECT-row-names-type`: case, passed<br>`V2-EFFECT-std-row-missing`: case, passed<br>`V2-EFFECT-row-duplicate`: case, passed |
| shortest witnesses | `V2-EFFECT-outside-ceiling-transitive`: case, passed<br>`V2-EFFECT-outside-ceiling-cycle`: case, passed |
| required and optional grants | `V2-GRANT-required-optional-bundle`: case, passed<br>`V2-RUNTIME-run-optional-absent`: case, passed<br>`V2-RUNTIME-run-required-refused`: case, passed |
| brands | `V2-GRANT-brand`: case, passed<br>`V2-GRANT-brand-forged`: case, passed |
| forged, stale, and foreign grants | `V2-GRANT-no-constructor`: case, passed<br>`ATK-forged-token`: `vibra-host` `broker::atk_forged_token`, passed<br>`ATK-stale-grant`: `vibra-host` `broker::atk_stale_grant`, passed<br>`ATK-foreign-grant`: `vibra-host` `broker::atk_foreign_grant`, passed |
| narrowing against interface views | `V2-GRANT-view-does-not-narrow`: case, passed |
| limits never reset | `V2-GRANT-limit-child-after-use`: case, passed |

### Coverage 11, policies and drivers

The reserved HTTP rules have no case: no network operation exists in this
release.

| Item | Cases and host tests |
| --- | --- |
| Whole rules stay correlated | `V2-GRANT-rule-correlation`: case, passed<br>`ATK-policy-rule-fields-crossed`: `vibra-host` `policy::atk_policy_rule_fields_crossed`, passed |
| restriction keeps the parent | `V2-GRANT-restrict-keeps-parent`: case, passed<br>`V2-GRANT-parent-stays-valid`: case, passed |
| normalization and size limits | `V2-GRANT-policy-4096-nodes`: case, passed<br>`V2-GRANT-policy-4097-nodes`: case, passed<br>`V2-GRANT-policy-depth-64`: case, passed<br>`V2-GRANT-policy-depth-65`: case, passed<br>`V2-GRANT-restrict-too-complex`: case, passed |
| a read-only directory chosen at run time with a `.txt` rule | `V2-GRANT-release-optional-absent`: case, passed |
| the real name of an entry | `ATK-fs-alias-name`: `vibra-host` `fs::atk_fs_alias_name`, passed<br>`ATK-fs-case-alias`: `vibra-fs-scope` `attacks::atk_fs_case_alias`, passed<br>`ATK-run-alias-denied`: `vibra-workspace` `attacks_run::atk_run_alias_denied`, passed |
| a rejected `RelativePath` is `ScopeError.invalidPath()` | `V2-RUNTIME-relative-path-invalid-path`: case, passed |
| prefix by names, not by string | `V2-GRANT-prefix-by-names`: case, passed<br>`ATK-run-prefix-by-names`: `vibra-workspace` `attacks_run::atk_run_prefix_by_names`, passed |
| reparse points, links, mounts, renames, and a replaced root | `ATK-fs-junction`: `vibra-fs-scope` `attacks::atk_fs_junction`, passed<br>`ATK-fs-file-symlink`: `vibra-fs-scope` `attacks::atk_fs_file_symlink`, passed<br>`ATK-run-symlink-denied`: `vibra-workspace` `attacks_run::atk_run_symlink_denied`, passed<br>`ATK-run-junction-denied`: `vibra-workspace` `attacks_run::atk_run_junction_denied`, passed<br>`ATK-run-unc-root`: `vibra-workspace` `attacks_run::atk_run_unc_root`, passed<br>`ATK-fs-root-renamed`: `vibra-fs-scope` `attacks::atk_fs_root_renamed`, passed<br>`ATK-run-root-replaced`: `vibra-workspace` `attacks_run::atk_run_root_replaced`, passed |
| no cached intermediate handle | `ATK-fs-swapped-junction`: `vibra-fs-scope` `attacks::atk_fs_swapped_junction`, passed<br>`ATK-run-swapped-component`: `vibra-workspace` `attacks_run::atk_run_swapped_component`, passed |
| hard-link and opened-object limits shown at approval | `ATK-fs-hard-link-replace`: `vibra-host` `fs::atk_fs_hard_link_replace`, passed<br>`ATK-run-hard-link-replace-denied`: `vibra-workspace` `attacks_run::atk_run_hard_link_replace_denied`, passed<br>`ATK-run-hard-link-read-allowed`: `vibra-workspace` `attacks_run::atk_run_hard_link_read_allowed`, passed<br>`ATK-gate-replace-without-link-count`: `vibra-host` `broker::atk_gate_replace_without_link_count`, passed |
| shared counters under siblings | `V2-GRANT-limit-siblings-share`: case, passed<br>`V2-GRANT-copy-shares`: case, passed |
| Passing in the fake world does not qualify a real adapter | `ATK-run-denied-leaves-file`: `vibra-workspace` `attacks_run::atk_run_denied_leaves_file`, passed<br>`ATK-run-create-denied-leaves-directory`: `vibra-workspace` `attacks_run::atk_run_create_denied_leaves_directory`, passed |

### Coverage 12, host registry and entry

The first item is checked per operation interface, not per error variant.
The world format (06 "Host world") has no form for a closed stream, a
refusal by the operating system, a variable that is not Unicode, an
unavailable clock or random source, or an unknown outcome. So no case can
produce `IoError.closed`, `FsError.permissionDenied`, an `other`,
`EnvError.notUnicode`, `TimeError.unavailable`,
`RandomError.unavailable`, or an `outcomeUnknown`; only a host test of a
driver can show them. No audit maps each variant, of either kind, to a case
or a host test.

| Item | Cases and host tests |
| --- | --- |
| Every operation, with success and each typed error, in fresh worlds | `V2-EFFECT-std-each-interface`: case, passed<br>`V2-GRANT-test-each-fs-operation`: case, passed<br>`V2-GRANT-test-fs-errors`: case, passed<br>`V2-GRANT-test-stream-bytes`: case, passed<br>`V2-GRANT-test-clock-overflow`: case, passed<br>`V2-GRANT-denied-is-value`: case, passed<br>`V2-GRANT-limit-exceeded-is-value`: case, passed<br>`V2-GRANT-test-worlds-isolated`: case, passed<br>`V2-RUNTIME-run-fresh-world`: case, passed |
| standard input read twice | `V2-GRANT-test-stdin-twice`: case, passed |
| output captured under test | `V2-GRANT-test-output-captured`: case, passed<br>`V2-GRANT-test-output-two-stream-sets`: case, passed |
| sorted directory entries | `V2-GRANT-test-entries-sorted`: case, passed<br>`V2-GRANT-test-directory-two-files`: case, passed |
| create and replace | `V2-GRANT-test-create-and-replace`: case, passed |
| invalid paths and text | `V2-RUNTIME-relative-path-invalid-path`: case, passed<br>`V2-GRANT-test-fs-errors`: case, passed |
| present and absent variables | `V2-GRANT-test-variables`: case, passed<br>`V2-GRANT-env-test-any-name`: case, passed |
| exact arguments | `V2-GRANT-test-arguments`: case, passed |
| fake clock steps and random vectors | `V2-GRANT-test-clock-steps`: case, passed<br>`V2-GRANT-test-random-vector`: case, passed |
| Events are recorded once, in order | `V2-RUNTIME-trace-order`: case, passed |
| a trap or exhaustion invents no event | `V2-RUNTIME-trace-trap-no-event`: case, passed<br>`ATK-trap-on-each-side-of-the-commit`: `vibra-interp` `host::atk_trap_on_each_side_of_the_commit`, passed |
| projections restore no authority | `ATK-trace-decode-grants-nothing`: `vibra-host` `trace::atk_trace_decode_grants_nothing`, passed |
| The startup order on a permuted `grants` dict in which acquiring the first ready leaf makes a lexically earlier leaf ready | `V2-GRANT-startup-order-permuted`: case, passed |
| startup failure | `V2-RUNTIME-run-required-refused`: case, passed<br>`V2-GRANT-startup-failed-optional`: host test `vibra-host` `startup::v2_grant_startup_failed_optional`, passed |
| optional absence, including an absent `source` leaf | `V2-RUNTIME-run-optional-absent`: case, passed<br>`V2-GRANT-release-optional-absent`: case, passed |
| release order | `V2-GRANT-release-order`: host test `vibra-host` `startup::v2_grant_release_order`, passed |
| non-zero exit codes stay distinct from tool exits and traps | `V2-RUNTIME-run-nonzero`: case, passed<br>`V2-RUNTIME-run-code-255`: case, passed<br>`V2-RUNTIME-run-exhausted`: case, passed |

## NOT RUN

Every row of the milestone that did not run, once:

| Row | Platform | Reason |
| --- | --- | --- |
| `vibra-cli` `run_command::directory_target_other_platforms` | Windows | prints `NOT RUN: directory target, other platforms (this is Windows)`: the row is the `@tool.unavailable` of a target that binds a directory where the platform has no directory driver, and Windows has one |
| `ATK-driver-absent-at-admission`, `vibra-host` `system::atk_driver_absent_at_admission`; `vibra-host` `fs::not_windows` | Windows | compiled only off Windows, for the same reason; `system::the_directory_driver_passes_admission_on_windows` holds the Windows side |
| every host test and the corpus | Linux, macOS | not run on this machine; the CI matrix row "the corpus on three platforms" runs them. There the twelve `ATK-run-*` rows print `NOT RUN … no directory driver` (`attacks_run::attack_rows_not_run`), the run rows of `gate.rs` and `run_command.rs` print `NOT RUN`, the system half of each dual row prints `NOT RUN`, and the `ATK-fs-*` tests of `vibra-fs-scope` and of `vibra-host` `fs` are compiled out ([architecture, section 3](architecture.md#3-fake-and-real-drivers)) |
| a dual row with `stdin` or `env` | every platform | the system half takes the process's own standard input and environment, so the harness refuses such a world; no row sets either ([dual run](evidence/dual-run.md), "Domain") |
| probe 4 of the [review of the drivers](evidence/review-drivers.md), a root on a mapped network drive | Windows | mapping a drive changes the machine's state outside the worktree; the path read instead is the UNC root of probe 2, held by `ATK-run-unc-root` |
| probe 6, a mount point registered with the mount manager | Windows | `mountvol` needs elevation; such a mount is a reparse point, refused like probe 5 (`ATK-fs-junction`) |
| probe 12, a write interrupted by a full volume | Windows | a small volume or a quota needs elevation; the uncertain write is charged in full (`ATK-counter-*` of `vibra-host` `broker`) |
| probe 32, console standard input | Windows | the session has no interactive console; a console takes the unknown-size path of a pipe (step 35, F1) |

Probe 14 of the review, a failing release, is no longer `NOT RUN`: step 35
gave the driver its log (F4), and `startup::a_failed_release_is_one_log_line`
runs it.

## What stays unavailable

| Form | Owner | Evidence |
| --- | --- | --- |
| `eval` | M3, "Bounded evaluation" | owner table, Findings |
| `query` | M3 | owner table, "Runtime and tooling forms" |
| `lint` | M3 | owner table, "Runtime and tooling forms" |
| `build` | not in this release; v0.0.3 ships `vibra build` | owner table, "Build products" |
| `project init`, `inspect`, `add`, `remove`, `sync` | no milestone names them | owner table, Findings |
| `@git` dependencies and the lock | not in this release | `V2-PROJECT-git-dependency-unavailable` |
| native execution | M4 | owner table, "Native execution slice" |
| a module named as a value | the checker of M1 | `V2-PROJECT-test-unavailable-item`; see [Reported to the specification's owner](#reported-to-the-specifications-owner) |

## Specification questions

Every question of the [question list](spec-questions.md) is answered. The
rulings are in the chapters each question's anchor names; the rows are the
ones the question lists under "Depends".

| Question | State | Ruling | Rows |
| --- | --- | --- | --- |
| Q1. Narrowing members on a value that is not a sealed grant | answered | The three members are ordinary abstract members. They are bound with `external @compiler`, and such a binding is the implementation of a sealed grant only, exactly as an `external @host` binding is under Q2: the broker's narrowing is the sealed grant's implementation. | Steps 19, 20, 22. Rows `V2-GRANT-impl-supplies-limit`, `V2-GRANT-impl-missing-limit`, `V2-GRANT-narrow-model-limit`, `V2-GRANT-narrow-model-restrict`, `V2-GRANT-narrow-model-sub`, `V2-GRANT-forward-model-reads`, `V2-GRANT-narrow-forward-limit`, `V2-GRANT-narrow-forward-restrict`, and "narrowing on a model" of step 20. |
| Q2. Which members of a standard operation interface an `impl` supplies | answered | Both are abstract for every `impl`. An `external @host` binding and an `external @compiler` binding of an interface member are the implementation of sealed grants only, so an `impl` supplies the operation member and the narrowing members (Q1), and one that lacks any of them is `@type.missingAbstractMember`. | Steps 08, 19. Rows `V2-EFFECT-impl-host-member-supplied`, `V2-EFFECT-impl-host-member-missing`, `V2-GRANT-impl-supplies-limit`, `V2-GRANT-impl-missing-limit`. |
| Q3. An unawaited plan as a statement | answered | `@type.mismatch` at the statement: the checker already requires an expression statement to have type `Void` or `Never`, and an `Intent` is neither. | Step 05. Row `V2-EFFECT-plan-statement-unawaited`. |
| Q4. One fault for an `await` that is wrong twice | answered | One `@effect.invalidAwait` at the `await`. In a wrong position the operand is not checked; in a right position an operand that is not an `Intent` gives the same code, and no `@type.mismatch`. | Step 05. Rows `V2-EFFECT-await-not-plan`, `V2-EFFECT-await-in-fn-not-plan`. |
| Q5. Where `@contract.unusedEffect` applies | answered | The warning needs a body and a row written on the same declaration: a `proc`, an anonymous `proc`, a `test`, and the additive `performs` of a default member. | Step 09. Rows `V2-CONTRACT-unused-effect-impl-member`, `V2-CONTRACT-unused-effect-default-owner`, `V2-CONTRACT-unused-effect-plan-only`. |
| Q6. The two wordings of `@type.suspiciousRow` | answered | Only the result operand of a written `Proc` or `Intent` type whose row is omitted, with at least one component. | Step 10. Rows `V2-TYPE-NAMES-suspicious-row`, `V2-TYPE-NAMES-suspicious-row-returns`. |
| Q7. Whether `sub` rebases paths, and what a narrowed grant projects | answered | `sub` rebases: a request through the child is the child's prefix followed by the request, and a policy given to `restrict` is relative to the grant it restricts. | Steps 14, 15, 17, 22, 31. Rows `V2-GRANT-sub-rebases`, `V2-GRANT-sub-then-restrict`, `V2-GRANT-sub-projection`, `V2-GRANT-sub-escape-denied`. |
| Q8. What `restrict` can return for a source policy | answered | A source policy is rooted at the grant it restricts. `foreignRoot` and `unsupportedVersion` are returned by no operation of this release; the chapter says they are reserved for a decoded policy. | Steps 14, 15, 19, 22. Rows `V2-GRANT-restrict-empty-suffix`, `V2-GRANT-restrict-empty-actions`, `V2-GRANT-restrict-too-complex`. |
| Q9. What counts toward the size and depth of a policy | answered | Counted after normalization, over the effective conjunction of Q7. | Steps 14, 15, 26. Rows `V2-GRANT-policy-4096-nodes`, `V2-GRANT-policy-4097-nodes`, `V2-GRANT-policy-depth-64`, `V2-GRANT-policy-depth-65`, `V2-GRANT-restrict-too-complex`. |
| Q10. Reserving a read whose size is not known | answered | A stream read asks for a count. `StdinText.read` and `StdinBytes.read` take `max: U64` and return at most that many bytes; the empty value is the end of input. | Steps 02, 15, 16, 19, 20, 21, 22, 29, 30, 32. Rows `V2-GRANT-limit-stdin-exact`, `V2-GRANT-limit-stdin-over`, `V2-GRANT-limit-stdin-zero`, `V2-GRANT-limit-stdin-count`, `V2-EFFECT-std-read-all`, `V2-GRANT-test-stdin-twice`, `V2-RUNTIME-run-stdin`, and the stdin rows of steps 15, 16, and 30. |
| Q11. Own counters and ancestor counters under `limit` | answered | Every `limit` creates a counter that starts at zero with the written bound. | Steps 15, 17, 22. Rows `V2-GRANT-limit-siblings-share`, `V2-GRANT-limit-child-after-use`, `V2-GRANT-limit-projection`. |
| Q12. A request for a variable outside an `EnvReadVar` grant | answered | `denied`, decided in step 3 of the host call, so no event is appended and the process environment is not consulted. | Steps 15, 16, 21, 30. Rows `V2-GRANT-env-name-denied`, `V2-GRANT-env-test-any-name`. |
| Q13. Invocation ordinals and root ordinals | answered | (a) Every started `await` takes the next ordinal, whether or not an event follows, so a trace's ordinals may have gaps. | Steps 06, 15, 17, 21, 28. Every case with an `auditTrace` expectation; the focused rows are `V2-RUNTIME-invocation-gaps`, `V2-RUNTIME-test-body-parent`, `V2-RUNTIME-root-ordinals`. |
| Q14. A grant that fails validation at the dispatch boundary | answered | `@runtime.invalidHostValue`. Checked source cannot reach the state, so it is a defect, and `denied` would hide one. | Steps 15, 20. Rows `ATK-foreign-grant`, `ATK-stale-grant`, `ATK-forged-token`. |
| Q15. The `takes` of a test | answered | Keep the code. The reader reports `@syntax.invalidForm` at the `takes` section for anything but one bare-name parameter; the checker reports the same code at the same span when the one type does not resolve to `@std.test.TestWorld`. | Step 21. Rows `V2-PROJECT-test-takes-two`, `V2-PROJECT-test-takes-other-type`, `V2-PROJECT-test-takes-renamed`. |
| Q16. `RelativePath` as a dict key | answered | `RelativePath` implements `Equatable` and `Ordered` in `@std.fs`, comparing its names in order with the shared lexicographic helper, each name by Unicode scalar. | Steps 19, 21. Rows `V2-GRANT-test-directory-two-files`, `V2-TYPE-INTERFACE-relative-path-ordered`. |
| Q17. The observation of a `workspaceRun` case | answered | A closed document beside `@testRun.v2`, with `format`; `result` (the `@command.*` atom); the optional `applicationCode` (`U8`); `stdout` and `stderr` (bytes records); `startup` (a list, one record per processed leaf in processing order, of `binding`, `status`, and the optional `failure`); the optional `trap`; and `auditTrace` (one `@auditTrace.v2` document). | Steps 29, 33, 36, and every row whose kind is `workspaceRun`. |
| Q18. Which events a `run` payload holds, and where its policies go | answered | `startup.auditTrace` holds the `@startup` events and the `@host` events of phase `@startup`; the payload's `auditTrace` holds the events of phase `@entry`; no event is in both. | Step 32. Rows of the `run` payload schema test. |
| Q19. Startup reads against the `source` leaf's limits and errors | answered | The read is an invocation of the source grant: it reserves on its counters, so the entry's own read above returns `limitExceeded`. | Step 28. Rows `V2-GRANT-startup-read-counts`, `V2-GRANT-startup-read-limit-fails`. |
| Q20. The `TestWorld` root in a target's `effects` | answered | No dedicated check. A `TestWorld` parameter is a leaf no provider issues, `@grant.invalidBinding`; without such a parameter nothing can perform the root, so a row or list that names it is the warning `@contract.unusedEffect`. | Steps 25, 26. Rows `V2-GRANT-entry-test-world-leaf`, `V2-CONTRACT-unused-effect-test-world`. |
| Q21. Endings the outcome table does not list | answered | (a) `@command.trap`, exit 5, with the trap record; `startup.status` is `failed` with a `null` `failure`, because no leaf stopped startup; every leaf acquired so far is released. | Steps 28, 29, 32. Rows `ATK-trap-during-startup`, `V2-GRANT-release-status`. |
| Q22. "An unbound optional grant" in the release gate | answered | The gate means a refused optional leaf: a `@variablePath` whose variable is not present. | Step 36. Row `V2-GRANT-release-optional-absent`. |
| Q23. Local dependencies without a lock | answered | No. A lock file is neither read nor required while every dependency is `@path`; a `@git` dependency stays `@tool.unavailable` at its entry. | Step 27. Rows `V2-PROJECT-path-dependency-no-lock`, `V2-PROJECT-git-dependency-unavailable`. |
| Q24. The version of a driver | answered | The toolchain release, `"0.0.2"`, for every driver the toolchain ships, the fake world's included. | Steps 17, 28, 30, 31. Every `auditTrace` expectation. |
| Q25. The row of the grant intrinsics in the registry snapshot | answered | Rows of 2.0 with `since` 0: the chapter lists them in the registry that "starts at 2.0", and nothing has been released against the snapshot. | Step 02. Row "registry snapshot" of its matrix. |
| Q26. The import cycle between `@std.fs` and `@std.policy` | answered | `FsPolicy`, `NameTest`, `FileDepth`, `DirDepth`, `FileAction`, and `DirAction` are declared in `@std.fs`. | Steps 18, 19, 21, 22. Every case that imports a policy type; rows `V2-TYPE-INTERFACE-std-policy-types`, `V2-TYPE-INTERFACE-std-fs-policy-types`. |
| Q27. A malformed `effects` list | answered | The list is a row in data form: `@effect.invalidReference` at an entry that is not an operation interface, including one that resolves to nothing, and `@effect.duplicateRoot` at a later repeat, related to the first. | Step 25. Rows `V2-EFFECT-target-effects-not-interface`, `V2-EFFECT-target-effects-unknown`, `V2-EFFECT-target-effects-repeat`. |
| Q28. What a read that fails after reading charges | answered | Progress is what the adapter moved across the boundary: the bytes it took from the resource for this call, whatever the reply. | Steps 15, 30. Rows `ATK-counter-invalid-text-charged`, and "stdin read error" of step 30. |
| Q29. A refusal decided after the adapter opened an entry | answered | Neither is traced: the error table says of both cases that nothing was dispatched or attempted, and the format has no refusal kind. | Steps 15, 17, 22, 31. Rows `ATK-counter-size-refusal-holds-no-invocation`, `V2-GRANT-limit-input-bytes`, `ATK-fs-no-normalized-name`, and "size refusal at the gate" of step 17. |
| Q30. A text read whose count splits a scalar | answered | A text read returns the longest prefix of whole scalars that fits in `max` bytes, and consumes exactly those bytes: the first read returns U+20AC and `a`, the second the empty value. | Steps 16, 30. Rows "stdin text below a scalar" of both. Every other text row uses a count of at least 4. |
| Q31. A text read of unknown size reserved below one scalar | answered | Such a read is `limitExceeded` before the stream is read, so a completed text read is empty only at the end of input. | Steps 15, 30. Rows "stdin text, unsized, below one scalar" of both. |
| Q32. The `root` of a rule in an effective policy | answered | A rule a `restrict` or a `sub` contributes names the leaf its grant descends from; every rule of a grant that descends from a `TestWorld` directory grant names `@testWorld`. | Step 17. Rows "rule roots after `restrict`" and "rule roots of a test-world grant". |
| Q33. Expected revisions after a library change | answered | Step 21 checks in the recomputation tool, and every step that changes the library runs it as a validation line. | Steps 21, 24: rows "the script agrees with the engine" and "the script finds a stale revision" of step 21. Steps 18 and 19 change the library before step 21 adds the toolchain record and the tool, so no expected revision depends on the library while they run. |
| Q34. The profile a case names | answered | No. A case names the profile its operation belongs to, and the manifest rejects any other. | Steps 01, 21. Rows "manifest: the profile of the operation" and "manifest: another profile" of step 01. |
| Q35. The JSON spelling of a code | answered | In JSON a code is spelled as its full atom text, so the payload carries `"@tool.unsupportedValue"`. | Step 32. Row "observation error code in JSON". |

## Contradictions of the tree, checked again

Each row of the questions' table "Where the tree contradicts the M2
chapters" was read again against the tree at this commit. None remains.

| Where | Now |
| --- | --- |
| `crates/vibra-ir/src/external/host.rs` | registry major 2: `crates/vibra-ir/registry/host-2.0.txt` has dotted camelCase symbols (`io.stdinText`), one owner interface, a grant first, `@once`; no kebab symbol remains |
| `crates/vibra-host/src/provider.rs`, `operation.rs` | gone; every call passes through `Context::host` in `broker.rs`, and the five error types have the four broker cases |
| `crates/vibra-host/src/world.rs` | `@hostWorld.v2` only |
| `crates/vibra-host/src/event.rs` | gone; `trace.rs` writes `@auditTrace.v2` with `@startup` and `@host` events, and no crate outside the `-v1` ones names `@auditTrace.v1` |
| `crates/vibra-host/src/system.rs` | the directory driver holds an opened directory (`system/fs.rs` over `vibra-fs-scope`), follows no link, and checks real names |
| `crates/vibra-types/src/check/expr.rs` | `await` is valid in a `proc` and a test (`V2-EFFECT-proc-await-direct`, `V2-EFFECT-await-in-test`) |
| `crates/vibra-types/src/check/testing.rs` | a test takes its `TestWorld` and is checked against its row (`V2-GRANT-test-reads-note`) |
| `crates/vibra-workspace/src/semantic.rs` | a binary target's entry, ceiling, and recipes are validated (`V2-PROJECT-entry-minimal`, `V2-GRANT-binding-*`) |
| `crates/vibra-workspace/src/project.rs` | a `@path` dependency is a unit (`V2-PROJECT-dependency-*`); only `@git` stays unavailable |
| `crates/vibra-ir/registry/compiler-2.0.txt` | rows `grant.limit`, `grant.restrict`, `grant.sub` |

## Open with the owner

Not decided here; each waits for Hannah.

| Question | Where it stands | Rows that wait |
| --- | --- | --- |
| Review finding F8: no command presents a target's authority before a run, while 03 asks that it be "presentable in plain terms" | the gate of step 34 left it open; coverage 11, "hard-link and opened-object limits shown at approval", has the limits enforced (`ATK-fs-hard-link-replace`, `ATK-gate-replace-without-link-count`) but no command that shows them | none in M2; the command that presents a target's authority |
| May a brand over a grant implement `Equatable` or `Ordered` itself? | today it may; a direct `Equatable.equal` call on it is accepted, while `assert.equal` and a dict key refuse it (`V2-GRANT-assert-equal-brand`, `V2-GRANT-dict-key-ordered-brand`) | those two cases and the `Equatable` call row a ruling adds |
| Is a callable-typed labelled default a constant? | a default of type `Option(Fn(...))` is accepted and a `Proc` default is `@tool.unavailable` (`V2-EFFECT-plan-default-not-constant` covers an `Intent` default) | the default rows of 02 "Constant expressions" a ruling adds |

## Reported to the specification's owner

- `V2-PROJECT-test-unavailable-item` expects `@tool.unavailable` for a
  module named as a value. Chapter 02, "Namespaces and resolution", makes a
  path that resolves to another kind of entity `@name.wrongEntityKind`. The
  form belongs to the names of M1, not to M2; the checker still reports it
  unavailable.
- The doc comment of `TestFaults::unavailable` in
  `crates/vibra-types/src/check/testing.rs` says "each test that takes a
  world"; the field holds the tests whose closure holds an unavailable
  form.
