# M4 Stage 4A evidence

Captured 2026-10-17 for [Step 12](12-stage-4a-evidence.md), on `m4` at
`1f55ad6` (the merge of Step 11, PR #369). This step claims no new language
behavior. It records the stage demo, the corpus sub-gate, and the map from the
Stage 4A deliverables and exit-gate clauses to their evidence. Its own PR adds
two contract fixes that the stage surfaced; both state consequences of existing
rules and add no exception:

- `expect.host_event` is now valid on `workspace-test`. The tooling chapter
  already fixes the outcome of a memory host event during `test`
  (`@command.operational-failure`, zero selected, passed, and failed, an empty
  `tests` array), but the conformance chapter admitted the field only on
  `interpret` and `workspace-run`, so no case could exhaust memory in a test
  and the interpreter's `workspace-test` handler returned an error instead of
  an observation. The conformance chapter, the manifest decoder, and both
  backends' handlers now accept it. The new case is
  `V1-RUNTIME-workspace-test-activation-exhaustion`: an earlier test passes, a
  later test recurses without a base case, and both backends end with
  `@runtime.memory-exhausted` and no test record. A `workspace-test` case that
  does not end in a host event still supplies exactly one result snapshot. The
  workspace test result keeps `items` empty on a host event, as the CLI
  requires, and exposes the programs that ran so the second backend runs the
  same tests (`WorkspaceTestResult::stopped_programs`). The conformance runner
  also gave the interpreter's test handler the runner's memory budget, so both
  backends run a test under the same limit, as `workspace-run` already did.
- The runtime chapter's **Tail calls** section states that a call whose result
  is widened to a union or an interface value is not in tail position, because
  widening attaches a discriminant or a descriptor (further computation), and
  that a cycle of tail calls cannot widen, because each call in the cycle
  returns its callee's declared result type unchanged, so the rule never leaves
  an unbounded loop that grows the stack. Both backends already behaved so: the
  checker marks a widened call with `without_tail`, and
  `V1-RUNTIME-widened-tail-call` covers it. No behavior changed. The
  specification example inventory
  (`docs/roadmap/milestone-1/syntax-examples.tsv`) was refreshed
  mechanically for the moved lines; the changed digests are the Tail calls
  section, the Conformance corpus section, and the status paragraph of the
  conformance chapter, whose stale sentence about the empty void entry now says
  that the differential runs over every executable case.

## Integration

Every Stage 4A step is in the integration head by its merge, checked with
`gh pr view` and `gh pr list --state merged --base m4`, not by its README row.

| Step | PR and merge | CI run on the merged head |
| --- | --- | --- |
| 1 | [#354](https://github.com/nahharris/vibra/pull/354), `e7d2e08` | [37085988282](https://github.com/nahharris/vibra/actions/runs/37085988282) |
| 2 | [#355](https://github.com/nahharris/vibra/pull/355), `3289ee7` | [37115960861](https://github.com/nahharris/vibra/actions/runs/37115960861) |
| 2b | [#357](https://github.com/nahharris/vibra/pull/357), `388dfe1` | [37158655486](https://github.com/nahharris/vibra/actions/runs/37158655486) |
| 3 | [#356](https://github.com/nahharris/vibra/pull/356), `8a04c08` | [37155420689](https://github.com/nahharris/vibra/actions/runs/37155420689) |
| 4 | [#358](https://github.com/nahharris/vibra/pull/358), `704b7ad` | [37174529962](https://github.com/nahharris/vibra/actions/runs/37174529962) |
| 5a | [#359](https://github.com/nahharris/vibra/pull/359), `fc374df` | [37186067873](https://github.com/nahharris/vibra/actions/runs/37186067873) |
| 5b | [#360](https://github.com/nahharris/vibra/pull/360), `fb7e1e1` | [37202425526](https://github.com/nahharris/vibra/actions/runs/37202425526) |
| 6 | [#361](https://github.com/nahharris/vibra/pull/361), `e86d93e` | [37206772606](https://github.com/nahharris/vibra/actions/runs/37206772606) |
| 7 | [#362](https://github.com/nahharris/vibra/pull/362), `cc73408` | [37220864041](https://github.com/nahharris/vibra/actions/runs/37220864041) |
| 8a | [#363](https://github.com/nahharris/vibra/pull/363), `a450155` | [38297037394](https://github.com/nahharris/vibra/actions/runs/38297037394) |
| 8b | [#364](https://github.com/nahharris/vibra/pull/364), `705d297` | [38663888335](https://github.com/nahharris/vibra/actions/runs/38663888335) |
| 8c | [#365](https://github.com/nahharris/vibra/pull/365), `d51cc34` | [38825039628](https://github.com/nahharris/vibra/actions/runs/38825039628) |
| 8d | [#366](https://github.com/nahharris/vibra/pull/366), `17fe1ea` | [38851967289](https://github.com/nahharris/vibra/actions/runs/38851967289) |
| 9 | [#367](https://github.com/nahharris/vibra/pull/367), `c1217c1` | [38884251748](https://github.com/nahharris/vibra/actions/runs/38884251748) |
| 10 | [#368](https://github.com/nahharris/vibra/pull/368), `20db8c6` | [38983691557](https://github.com/nahharris/vibra/actions/runs/38983691557) |
| 11 | [#369](https://github.com/nahharris/vibra/pull/369), `1f55ad6` | [39022116218](https://github.com/nahharris/vibra/actions/runs/39022116218) |

Every run passed all its jobs (the archive boundary, the build and test jobs on Ubuntu, Windows, and macOS, and, from Step 4, the conformance corpus in both backends on the same three). The run for Step 11 was re-read after it completed.

## Stage demo

The demo is the M3 demo project
[`examples/m3-catalog`](../../../examples/m3-catalog). It had no deep
tail-recursive walk, so this step adds one: `catalog.stock.walk` builds a fresh
`part` record in each round and calls itself in tail position, and the test
`a deep walk over a line of stock` walks 100,000 rounds. The sources are in
canonical format (`vibra fmt` prints each unchanged), and
`vibra-cli/tests/process_m3_demo` now expects eight tests. The CLI has no
backend option in M4 (Wasm is executed only through the conformance harness), so
the Wasm run is a host test.

From a clean checkout, with no network, in `examples/m3-catalog`:

```bash
vibra check
```

`check accepted`, exit 0.

```bash
vibra test
```

`test suite passed: 8 test(s)`, exit 0, in the reference interpreter.

The host test
[`stage_4a_demo_m4_step12`](../../../crates/vibra-conformance/tests/stage_4a_demo_m4_step12.rs)
repeats the demo on every CI run, with `cargo test --locked --offline -p
vibra-conformance --test stage_4a_demo_m4_step12 -- --nocapture`:

- **Identical results.** The demo's eight tests run in the interpreter and,
  lowered to one test module, in the Wasm backend through the conformance
  harness. The rows (name, outcome, failure, trap) are identical and so is the
  `@test-run.v1` snapshot, `@command.ok`, eight of eight passed.
- **The walk at two sizes a factor of ten apart**, with the walk's literal
  rewritten in a copy of the demo:

| Rounds | Interpreter: max activation depth | Interpreter: tail transfers | Wasm: `vibra_v1_live_size` at start, at end | Wasm: arena high-water mark | Smallest engine stack that runs it |
| --- | --- | --- | --- | --- | --- |
| 10,000 | 4 | 10,000 | 0, 64 | 20,000 bytes | 8 KiB |
| 100,000 | 4 | 100,000 | 0, 64 | 20,000 bytes | 8 KiB |

The live size after the walk is 64 bytes at both sizes, which is the module
values the test reached (an instance keeps them for its life) and not the
rounds. The arena's high-water mark, which a frame stack or garbage heap that
grew with the rounds would show even after the run freed it, is identical. The
smallest engine stack was found by bisection over 8, 12, 16, 24, 32, 48, and 64
KiB; the engine's default is 512 KiB, so the walk runs on a fiftieth of it, and
the same engine stack serves 10,000 and 100,000 rounds. The test asserts the
equality of the activation depth, the live size, the arena (within 4 KiB), and
the stack across the two sizes.

The loop kinds of Step 6 (direct, mutual, function value, closure, method, field
and binding, hand-off) hold the same bounds at 10,000 and 100,000 rounds, and
run in 256 KiB of memory
(`calls_m4_step6::a_tail_loop_that_allocates_each_round_holds_a_bounded_live_arena`
and `a_hundred_thousand_tail_calls_run_in_a_few_pages_of_memory`).

## Corpus sub-gate

The full corpus reports 475 passed, 0 failed, and 0 unavailable, by profile:

| Profile | Passed | Failed | Unavailable |
| --- | --- | --- | --- |
| reader-v1 | 83 | 0 | 0 |
| static-v1 | 239 | 0 | 0 |
| interpreter-v1 | 138 | 0 | 0 |
| tooling-v1 | 15 | 0 | 0 |

By backend, over the 131 executable cases (the interpreter cases that expect
acceptance: 105 `interpret`, 6 `workspace-run`, and 20 `workspace-test`):

| Backend | Passed or matched | Failed | Unavailable or not lowered |
| --- | --- | --- | --- |
| interpreter | 131 | 0 | 0 |
| Wasm | 131 matched | 0 | 0 not lowered |

The requirement is met: 0 failed, 0 unavailable, and 0 not lowered over every
executable case. Step 12 adds one executable case, so the count rose from 130
to 131 (and the corpus from 474 to 475).

**The parity inventory's final state.** `conformance/parity.tsv` holds 131
rows, every one `matched` and none `not-lowered`; the host test
`parity_inventory_m4_step4` holds the table to the corpus and the runner (a case
with no row, an extra row, or a row that disagrees with the runner fails it).
`lowers_every_form_m4_step11` shows that every program the corpus checks lowers
and that no form of a checked program is named by `NotLowered`.

The workspace test suite reports 1,178 passed and 0 failed (`cargo test --workspace --all-targets --all-features`).

## Stage 4A exit clauses

Each deliverable of the Stage 4A rows in `docs/roadmap/v1.md`, and each
exit-gate clause that Stage 4A owns, maps to cases (interpreter plus Wasm
through the differential), host tests, and ledger rows. Case IDs are corpus
cases and run in both backends; host tests are in
`crates/vibra-conformance/tests` unless a crate is named.

| Clause | Evidence |
| --- | --- |
| Unoptimized Wasm lowering of the complete pure language: mandatory tail calls, union discriminants in written order, checked lookups, `@compiler` externals lowered before emission | Every executable case matched (131 of 131); `lowers_every_form_m4_step11`; `core_lowering_m4_step5b`, `calls_m4_step6`, `patterns_m4_step7`, `collections_m4_step8b`, `primitives_m4_step8a`, `numbers_m4_step8c`, `native_bodies_m4_step8d`, `interfaces_m4_step9`, `tests_traps_m4_step11`; union order `V1-RUNTIME-union-values`, `V1-RUNTIME-union-narrowing`, `V1-RUNTIME-union-atom-member`; tail calls `V1-RUNTIME-tail-*`, `V1-RUNTIME-return-tail-call`, `V1-RUNTIME-widened-tail-call`; lookups `V1-RUNTIME-lookups`; registry `V1-RUNTIME-registry-integers`, `V1-RUNTIME-registry-floats`, `V1-RUNTIME-library-roles` (ledger D2.12, D2.15, D12.1) |
| Deep non-tail recursion has the same specified outcome in both backends | `V1-RUNTIME-activation-depth` and `V1-RUNTIME-activation-depth-counter` (a hundred thousand activations deep, with a base case, complete with the result); `V1-RUNTIME-activation-exhaustion`, `V1-RUNTIME-activation-exhaustion-discarded-call`, `V1-RUNTIME-workspace-run-activation-exhaustion`, and the new `V1-RUNTIME-workspace-test-activation-exhaustion` (no base case: `expect.host_event = "@runtime.memory-exhausted"` in the entry of a run and in the body of a test); `activations_m4_step3` (interpreter), `calls_m4_step6` (`non_tail_recursion_a_hundred_thousand_deep_completes_in_both_backends`, `the_engine_stack_a_deep_recursion_needs_does_not_depend_on_its_depth`, `a_recursion_with_no_base_case_is_the_host_event_and_a_larger_limit_recovers`); ledger D1.1 to D1.4 |
| A tail-recursive loop allocating a fresh compound value per iteration holds a bounded live arena in the Wasm backend | `calls_m4_step6::a_tail_loop_that_allocates_each_round_holds_a_bounded_live_arena` (live size through `vibra_v1_live_size` at 10,000 and 100,000 rounds, with and without the result held, plus the arena high-water mark, for seven loop kinds); the stage demo's walk above; the control `the_same_loop_without_a_tail_call_does_not_run_in_that_memory`; ledger D3.2 |
| Releasing a value nested five thousand levels deep completes, on a bounded engine stack | `arena_runtime_m4_step5a::a_value_nested_deep_releases_with_bounded_engine_stack` (5,000 and 100,000 levels on a 64 KiB engine stack), `core_lowering_m4_step5b::non_tail_recursion_five_thousand_deep_runs_on_a_small_engine_stack`, `activations_m4_step3` (5,000 and 100,000 in the interpreter); ledger D3.3, D3.7 |
| Native implementations written once, lowered from the single source, with the body/native differential joined to the interpreter/Wasm harness | `natives_m4_step10::no_native_has_a_second_implementation_in_the_interpreter_the_emitter_or_the_runner` and its manifest and import-table checks, `natives_m4_step10::the_looping_rows_and_iteration_agree_across_both_backends_and_the_literal`, `natives_m3_step4b::every_listed_native_matches_its_body` (112 four-path samples of the 19 natives plus the looping rows), `native_faults_m4_step10` in `vibra-wasm-run`, `numbers_m4_step8c` (11,539 shared vectors); ledger D10.1 to D10.6 |
| Deterministic emission | `wasm_skeleton_m4_step4::emission_is_byte_identical_across_builds_in_one_process` and `emission_is_byte_identical_across_processes`; the module has no custom section and validates under the baseline feature set (`the_module_has_no_custom_section`, the runner's validation with exactly the baseline features); ledger D6.1 to D6.4 |
| Instance-owned arena and opaque-ID discipline: no value index or instance identity in typed IR, canonical encodings, audit events, or snapshots | `typed_ir_identity_m4_step4` (`no_field_of_the_canonical_form_denotes_a_place_in_an_instance`, the canonical form identical across independent checks and across processes); IDs are 64-bit, 0 invalid, never reused (`arena_runtime_m4_step5a`, `arena_m4_step5a`); canonical results and snapshots carry values only (every corpus snapshot); ledger D2.1 to D2.6 |
| Specified reclamation rule, unobservable apart from memory use | `arena_emitted_m4_step5a`, `arena_runtime_m4_step5a`, the live-size checks of `calls_m4_step6`, `core_lowering_m4_step5b`, `patterns_m4_step7`, `collections_m4_step8b`, `numbers_m4_step8c`, `interfaces_matrix_m4_step9`, and `tests_traps_m4_step11::a_passing_test_returns_the_live_size_to_its_baseline`; ledger D3.1 to D3.10 |
| Trap and origin parity: the same trap code and origin in both backends | `tests_traps_m4_step11` (the three assertions and their operands, `a_false_assertion_stops_only_its_test_and_is_not_a_trap`, `every_origin_is_the_assertion_call_the_interpreter_reports`, `a_function_hidden_behind_an_interface_is_the_trap_at_the_assertion_call`), `trap_origins_m4_step11` (every registered trap code with its origin or none), `V1-RUNTIME-workspace-test-failure`, `V1-RUNTIME-workspace-test-assert-equal`, `V1-RUNTIME-workspace-test-interface-encoding`, and the host event `memory_exhaustion_in_a_test_is_the_host_event_and_not_a_trap` |
| An interpreter/Wasm differential harness over the entire executable corpus, reached through the conformance harness | `differential_m4_step4`, `parity_inventory_m4_step4`, the runner's two backend lines (131 matched, 0 failed, 0 not lowered), and the CI `conformance` job on three platforms; `vibra build` stays unavailable (ledger D6.1, D6.4) |
| Interpreter-v1 conformance passes for the full pure language, and the Wasm backend matches it on every executable case | The 138 interpreter-v1 cases above pass and the 131 executable cases are matched; the M3 demo's tests agree (stage demo above) |
| Stage demo | The stage demo section above |
| The forms M3 reassigned that Stage 4A owns (the three contract-member forms) | `contract_members_m4_step2`, `interfaces_m4_step9`, `V1-RUNTIME-contract-member-dict-tail`, `V1-RUNTIME-tail-contract-member-forms`, `V1-RUNTIME-tail-contract-member-interface-value`; the result of an entry that returns `err` is Step 19 |

The exit-gate clauses owned by Stage 4B remain open and are not claimed here:
success, typed host error, propagation, and trap parity for host operations
(Steps 11 and 19; the Stage 4A part, trap parity, is above), the effect clauses,
the registry clauses, and the performance baseline.

## Inventory

Every row of the [M4 surface inventory](supported-surface.md) that Stage 4A
owns is implemented, and `m4_contract_inventory` fails when a variant has no
disposition. The inventory holds 51 `Lowered` rows, each lowered by its owning
Stage 4A step and exercised by the cases of the step's guide, 6 `Static` rows
that no step lowers, and the Stage 4B rows
`Declaration::Deffect` and `Attribute::Effects` (Step 14) and the `@host` half
of `Attribute::External` (Step 16), which report `@tool.unavailable` through
Stage 4A. No Stage 4A row is `not lowered`: the parity inventory has no such row
and the emitter names no form as unlowered. The inherited rows I3 to I6 (the
contract-member forms) are implemented in the interpreter by Step 2 and lowered
by Step 9. I1, I2, I7, I8, and I9 belong to Stage 4B.

## Commands

Run from the repository root with the pinned toolchain (Rust 1.96.1),
`CARGO_INCREMENTAL=0`, `--locked --offline`. Base `m4` at `1f55ad6`; tested
head TESTED_HEAD.

COMMANDS

## Verdict

Stage 4A is complete. The stage demo holds in both backends with a deep
tail-recursive walk that grows neither the engine stack nor the arena, the
corpus sub-gate holds with 0 failed, 0 unavailable, and 0 not lowered over 131
executable cases, and every Stage 4A deliverable and exit clause has evidence.
No gap blocks the declaration. The Step 12 pull request is conditional on its
merge, and Stage 4B (Step 13) starts only after it.
