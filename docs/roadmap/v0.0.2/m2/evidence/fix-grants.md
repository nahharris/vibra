# M2 step 24 — fixes of the review of grants

The fixes of the findings of the [review of grants](review-grants.md), on
the step branch `m2/24-fix-grants`, following the rulings on that review.
The tests of the three defects were committed before their fixes, in
`36a7e1cb` ("Failing tests for the review of grants"), and seen to fail
there; the fixes are in `b85e8477`, where they pass. The probes that the
review marks partial were closed with new tests and cases. One of them
found a defect: its case failed at `efa3087d`, and the fix is in
`5c96ea4f`. A finding that needed no code has a test or case that passes
when it is added.

## Findings

| Finding | Class | Case or test | Fails at | Passes at |
| --- | --- | --- | --- | --- |
| F1, a driver completion whose class and error case disagree is accepted | defect, fixed in `vibra-host/src/broker.rs` | `broker::atk_driver_completion_class`; `broker::consistent_completions_keep_their_class` | `36a7e1cb`: not submitted `outcomeUnknown` settles as a refusal | `b85e8477` |
| F2, the testing-feature hooks are a hand list that misses `startup_event_for_tests` | defect, fixed in the architecture test | `host_testing_feature::atk_seventh_hook_outside_the_gate`; `host_testing_feature::the_test_hooks_are_compiled_only_under_the_feature` | `36a7e1cb`: an ungated seventh hook is not reported | `b85e8477` |
| F3, the ambient-read scanners stop at the first `#[cfg(test)]` line, so `HostSlot::delivered` was ungated | defect, fixed in both scanners and in `vibra-interp/src/host.rs` | `ambient_reads::atk_read_after_a_test_only_item`; `host::atk_ambient_read_after_a_test_only_field` (`vibra-interp`) | `36a7e1cb`: a read after a `cfg(test)` field is not found | `b85e8477` |
| F4, the policy digest formula is not in chapter 03 | spec debt, no code | none | — | — |
| F5a, `Context::test_root` is public | ruling: stays public; its callers are pinned | `test_root_callers::only_the_test_runner_and_the_conformance_runner_issue_a_test_root`; `test_root_callers::atk_test_root_called_by_a_target_path` | new; seen to fail with a temporary caller in `vibra-workspace/src/run.rs` | `87199aa7` |
| F5b, a model's `restrict` returns its held grant unchanged | ruling: an author contract | `V2-GRANT-model-restrict-keeps-itself` | passes when added | `43ddd257` |
| F5b, a model's `restrict` returns a different held grant | ruling: an author contract | `V2-GRANT-model-restrict-swaps-grant` | passes when added | `43ddd257` |
| F6 | accepted | none | — | — |
| F7, a grant kept across tests | closed: it cannot be written | `V2-GRANT-module-value-cannot-await` | passes when added | `43ddd257` |
| F7, a grant through a generic identity, and every observation of it | closed | `V2-GRANT-generic-identity`; `V2-GRANT-generic-bound` | pass when added | `43ddd257` |
| F7, a brand over a grant with its own `Equatable` and `Ordered` | closed | `V2-GRANT-assert-equal-brand`; as a dict key, `V2-GRANT-dict-key-ordered-brand` | passes when added | `43ddd257` |
| F7, a `sub` whose prefix is a file | closed | `V2-GRANT-sub-file-prefix` | passes when added | `43ddd257` |
| F7, reserved device names in other cases through the broker | closed | `broker::atk_reserved_names_in_any_case_through_the_broker` | passes when added; seen to fail with the simple uppercase, and with the device check, removed | `efa3087d` |
| F7, a trap in the window between the adapter's return and the commit | closed | `host::atk_trap_on_each_side_of_the_commit` (`vibra-interp`) | passes when added; seen to fail with a commit on a trapped delivery | `efa3087d` |
| F7, an exhaustion in that window | defect, fixed in `vibra-interp/src/host.rs` | `host::atk_exhaustion_in_the_window_before_the_commit`; `host::an_exhaustion_after_the_commit_keeps_the_event` (`vibra-interp`) | `efa3087d`: a 100,000-byte reply under 100 bytes of room completes | `5c96ea4f` |
| F7, `run_entry` with a token the context never issued, and with one of another context | closed | `test_runner::isolation::run_entry_with_a_token_the_context_never_issued` (`vibra-workspace`) | passes when added; seen to fail with the context check, and with slot issuance, removed | `d98355f3` |
| F7, a defect-trap test followed by another test | closed | `test_runner::isolation::a_defect_trap_after_host_calls_ends_only_its_own_test` (`vibra-workspace`) | passes when added | `d98355f3` |

## The rules the fixes follow

- **F1.** 06 "Host calls" keeps "known not submitted, completed, and may
  have run" apart. A driver that reports `outcomeUnknown` as not
  submitted contradicts "It MUST NOT be reported as rejected before
  submission" (06 "Host error cases"). A driver that reports
  `limitExceeded` or `unsupported` as completed contradicts "wherever they
  are decided, these two refusals are not completed calls" (06 "Audit
  trace"). Each is `@runtime.invalidHostValue`. It appends no event, and
  the whole reservation stays charged, since what the driver did may have
  happened. A `denied` that the driver reports as not submitted stays
  accepted: the chapter does not settle that pairing (see the question
  below). Each of the three checks was seen to fail its pairing when it
  was removed.
- **F2.** The architecture test finds every function under
  `#[cfg(feature = "testing")]` in `vibra-host/src` by scanning, with each
  gated item matched brace by brace. Every function whose name ends in
  `_for_tests` must be in a gated item, and every gated function must be
  named as a hook. The scan finds the six hooks, `startup_event_for_tests`
  among them. A temporary seventh hook outside the gate in `broker.rs` is
  reported.
- **F3.** The scanners skip only a `#[cfg(test)]` module: the attribute,
  the `mod` it gates, and that module's block matched brace by brace, or
  the one line of `mod name;`. A `#[cfg(test)]` field, function or other
  item is scanned, and so is every line after the module. With the
  scanners fixed, `HostSlot::delivered` is `#[cfg(test)]`, and
  `deliver()` calls it only in a test build.
- **F5a.** Outside a `#[cfg(test)]` module, `test_root` is called only in
  `vibra-workspace/src/test_runner.rs` and below
  `vibra-conformance/src/`. The audit fails if it finds no allowed call.
- **F5b.** Neither model claims a narrowing that did not happen. With
  the model that keeps itself, the read of `b.bin` outside `textOnly`
  succeeds, and its one event projects `dir.readText`: root 3, the root
  policy, no limits. With the model that swaps, the first event projects
  the `sub` it read through, with the policy `allOf(root, sub ["sub"])`.
  The second projects `dir.readText`, root 3.

## The partial probes

- **Kept across tests.** A grant is had only through an `await`, and a
  module-value initializer is pure, so its `await` is
  `@effect.invalidAwait`. The language has no assignment to a module
  value. A module value of a grant's type holds a model
  (`V2-GRANT-module-value-holds-model`).
- **Generic identity.** Passed through an unbounded identity, alone and
  inside a record, the grant reads with the projection of `dir.readText`.
  A generic observes a value only through a bound. An interface value
  type satisfies only `Any`, so `Equatable` and `Ordered` are each
  `@type.unsatisfiedBound` at the argument that fixes the parameter,
  related to the bound.
- **Brand.** Its own `Equatable` does not make it an `assert.equal`
  operand. Its type contains a grant, which is `@type.functionNotEquatable`
  once at each operand.
- **A file prefix.** A `sub` is lexical, so naming a file is no refusal.
  The read of `["x"]` below it reaches the world, which finds a file above
  `x`: a completed `notADirectory`. The read of the empty path, the rebased
  `["a.txt"]`, is not selected by the sub's file rule, which needs a
  positive number of further names, so it is `denied` and appends nothing.
  The list of the empty path is a completed `notADirectory`.
- **Reserved names.** A path operand, a `sub` prefix and a policy prefix
  holding `nul`, `Nul.txt`, `CON .log`, `conın$` (with U+0131), `Com¹`,
  `lPt3.x`, `CONOUT$` or `aUx.tar.gz` are each `@runtime.invalidHostValue`
  before the driver is asked, with no event. `nul2` and `conİn$` (with
  U+0130) are read.
- **The window before the commit.** Between the adapter's return and the
  commit, the interpreter checks the reply against the row, accounts it,
  and hands it to the `await` that waits for it; the commit follows. A
  reply of another shape traps there and its receipt is dropped. A trap in
  the next node keeps the one event. Run time has no reduction limit,
  since only a compile-time evaluation has one and it has no host, so no
  reduction halt can fall in the window.
- **Exhaustion in the window.** The interpreter counted only the top of a
  reply that the host built whole. A nested text went uncounted until the
  next measurement, so a 100,000-byte reply under 100 bytes of room
  completed the run. Following 06 "Memory", every part of the reply is now
  compared with the room measured for the request, before it is accounted.
  An exhaustion after the commit keeps the event, at every budget between
  the request and the whole run.
- **Tokens never issued.** `run_entry` on the context's own `stdoutText`
  writes `x` and appends one event. A slot that the context never issued,
  or a token of another context, is `@runtime.invalidHostValue` at the
  broker's validation, with no event and nothing written.
- **Defect trap, then another test.** A test-only seam in the runner
  answers a chosen host call with a driver fault. The first test writes
  once and then faults: it is `@runtime.invalidHostValue`, and its trace
  keeps the write at invocation 3, parent 1, root 6. The second test
  passes, and its trace is the same as in a run without the fault.

## Questions

- **F1, a not-submitted `denied`.** Chapter 06 makes the adapter's
  `denied` a completed call. It also has a `denied` that is not completed,
  for a counter that would overflow, and the case table allows a `denied`
  "by the driver after it opened an entry". Whether a driver may report a
  `denied` as known not submitted is not settled, so the broker accepts it
  as a refusal.

## Probes of step 23, rerun

The four-crate suites rerun every named test of the review, and the
corpus runner every named case. The review's five scratch broker tests,
appended to a copy of the broker tests, pass. The scratch observation
fixture still reports `@test.assertionFailed` with no grant. The stream
fixture passes. The propagation fixture has one event, at invocation 4,
parent 3 and root 3. In the isolation fixture both traces hold
`stdinBytes 3/1/3`, `nowMonotonic 6/1/7`, `nowMonotonic 7/1/7` and
`generateBytes 9/1/9`. `deep_ancestry` with 100,000 limits passes on the
optimized broker, on a 256 KiB stack, in 55.50 seconds.

## The counter model

`broker::counters_against_a_model` runs the random-tree test of step 15
over 10,000 trees.
