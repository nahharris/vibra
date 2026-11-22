# M2 step 23 — grants, broker, policies, world and trace

Reviewed revision `7729a17c3eece4965fcdb094e196ba7d3945db43` on
`m2/23-review-grants`. This is a defensive review of the broker contracts,
the existing negative tests, and the fake-world conformance fixtures. Findings
about security-sensitive failure paths give the invariant and repair without
a new bypass reproduction.

## Probes

`V2-GRANT-` and `V2-RUNTIME-` are corpus IDs. Rust names identify tests
in the indicated crate. “Partial” identifies an unverified part of the
requested probe. The prescribed suites exercise the checked-in probes.

| Probe and source | Expected | Observed | Verdict |
| --- | --- | --- | --- |
| Cross-world and unissued token, host `atk_foreign_grant`, `atk_forged_token`, `a_foreign_world_and_its_grants_are_invalid_host_values` | Reject at grant validation | Pass. Validation checks context, slot, lineage, liveness and generation. No cross-test source storage probe. | Partial |
| Source views, models, and a module value, `V2-GRANT-view-does-not-narrow`, `V2-GRANT-model-is-not-grant`, `V2-GRANT-module-value-holds-model` | Views and initializers confer no authority | Corpus passes. Model executes with an empty trace. The full generic identity/Any observation matrix is unverified. | Partial |
| Observation and keys, `V2-GRANT-assert-equal-grant`, `V2-GRANT-dict-key-grant`, interpreter `atk_encode_grant` | Reject comparison, key and encoding at the proper boundary | Corpus and tests pass. Custom Equatable/Ordered and every hidden observation form are unverified. The brief's default-argument ruling is excluded from new findings. | Partial |
| Failure report with grants in scope, scratch source below | Assertion failure and no printed token | CLI JSON reports `@test.assertionFailed`, with null expected/actual values and empty trace | Pass |
| Policy equivalence shortcuts, policy normalization and broker ancestry | Apply each constraint by conjunction; no general subset reasoning | Read of `policy::normalize`, `permits`, and `ancestry_permits` finds normalization and local rule checks. Correlated-rules corpus passes. | Pass |
| Reservation and rollback, host `atk_counter_reservation_all_or_none`, `atk_counter_size_refusal_holds_no_invocation`, `atk_counter_overflow` | Atomic reservations; no change after failed size reservation | Tests pass, including ancestor accounting and overflow. | Pass |
| Adapter denial versus policy refusal, host `gate_real_name_fails`, `denied_before_the_adapter`, `V2-GRANT-denied-no-event` | Adapter denial keeps invocation and refunds bytes; early denial produces no event | Tests pass. Scratch denial commits one event and charges `[1, 0, 0]`. Early denial has no receipt. | Pass |
| Unknown outcome, host `atk_counter_refund_unknown`, `an_unknown_outcome_is_one_event`, `atk_no_redispatch_on_error` | Charge uncertain progress, record typed outcome, never redispatch | Tests pass for completed unknown outcomes. Driver-completion validation has a gap (F1). | F1 |
| World data, host `unknown_field`, `atk_trace_decode_grants_nothing` | Closed world schema; trace decoding restores no grant | Tests pass on unknown fields and inert trace data. | Pass |
| Host testing feature, `host_testing_feature` conformance integration tests and CLI `cargo tree` | Only host's dev graph enables hooks in the checked-in workspace | Three tests pass. CLI normal/build graph has only `default` for `vibra-host`. The scanner's hook list omits one gated method (F2). | F2 |
| Delivery hook and malformed replies, interpreter `commit_follows_the_continuation`, `a_reply_the_budget_cannot_hold_is_the_host_event`, `a_fault_of_the_commit_is_the_trap` | Commit follows taking a well-formed value; errors do not create success events | Tests pass. The callback field is crate-visible and has one setter, in a unit test. F3 describes the test seam. | Pass |
| Commit in plan contexts, `parent_across_a_tail_await`, `grant_captured_in_a_closure`, `V2-RUNTIME-trace-trap-no-event` | Tail and closure contexts preserve delivery and trace rules | Tests and corpus pass. A guard-consequence await is barred by chapter 03. The scratch `try` fixture propagates a completed `notFound` and commits one event (invocation 4, parent 3). Dedicated continuation-halt and immediate-before-commit exhaustion variants are unverified. | Partial |
| Entry arguments, interpreter `entry_arguments`, host `atk_forged_token`, `atk_test_world_op_on_leaf`, `a_context_over_another_driver_has_no_test_root` | Boundary wraps typed supplied tokens and broker rejects unknown/foreign tokens at use | Tests pass. `entry_arguments` validates the shape. `Context::validate` validates live authority. A separate end-to-end invalid-entry fixture is unverified. | Partial |
| Narrowing models, `V2-GRANT-narrow-model-limit`, `V2-GRANT-narrow-model-sub`, `V2-GRANT-narrow-forward-restrict`, `V2-GRANT-narrow-forward-parent-stays` | Own model member runs; forwards to sealed authority only if the body does so; real grant keeps ancestry | Fixtures pass. An adversarial model that returns a different held grant or the unchanged parent is unverified (F5). | Partial |
| Prefixes, `sub_rebases`, `sub_of_a_missing_directory`, `sub_too_complex`, `V2-GRANT-sub-then-restrict`, `V2-GRANT-sub-projection` | No widening; policy counts rebased names | Tests pass. Path tests cover reserved stems and trailing punctuation. Broker validates path operands and rebased prefix using `RelativeNames::new`. Scratch empty `sub` preserves coordinates and performs no action. The scratch `sub` → `restrict` → `sub` probe agrees with the independently computed policy hash. File-valued prefix and Unicode case through the broker are unverified. | Partial |
| Policy depth and ancestry, host `a_policy_of_depth_64_is_accepted_and_65_is_too_complex`, `deep_ancestry`, `V2-GRANT-policy-4096-nodes`, `V2-GRANT-restrict-too-complex` | Typed bounds and iterative walk | Tests pass; `deep_ancestry` covers 10,000 limits on a 256 KiB stack. A scratch run with 100,000 limits passes on a 256 KiB stack in 49.52 seconds with the optimized broker. The scratch combined depth and size case rejects before the adapter. The 4,090-name restriction is refused in `V2-GRANT-policy-4096-nodes`. | Partial |
| Stream counts, host `stdin_unsized_a_count_above_the_bound`, `stdin_unsized_nothing_remains`, `stdin_text_unsized_below_one_scalar`, `stdin_known_size_empty_at_zero` | Unknown size uses min; known-size overrun refuses; small text headroom is typed | Tests pass. Scratch max-zero on an unknown byte stream keeps the byte budget. Scratch `readAllText` returns `limitExceeded` at two bytes and the parent can still read the full fixture. | Pass |
| Stream progress, host `stdin_invalid_text`, `atk_counter_invalid_text_charged`, `V2-GRANT-limit-siblings-share` | Invalid window consumes bytes; siblings draw from common counter | Tests and corpus pass. | Pass |
| Test isolation, `V2-GRANT-test-worlds-isolated`, `V2-GRANT-test-clock-steps`, `V2-GRANT-test-random-vector`, workspace `an_exhausted_test_ends_only_itself` | Per-test world and ordinals; exhaustion stops one test | Fixtures pass and both isolated worlds start at invocation 1. The scratch exhaustion case restarts all three and reproduces the invocation, parent and root sequence in the next test. A distinct defect-trap variant is unverified. | Partial |
| Test worlds in targets, `atk_test_world_op_on_leaf` and `Context::test_root` review | Real contexts cannot issue a fake world token | Host checks pass. Startup and system modules are placeholders at this gate. Host API can intentionally build its own fake world (F5). | Partial |
| Registry agreements, `externals::every_embedded_external_is_a_registry_row_of_its_owner`, `host-2.0.txt` and `compiler-2.0.txt` | Exact registry symbol, owner and shape | Host snapshot has 17 host and six world rows; embedded externals test passes, including owner and signature. | Pass |
| Independent snapshot check below | Derived events and grant projections agree | Ten fixtures, 14 events, four policy hashes, and 15 declaration identities agree. Hash formula comes from a ruling rather than the active chapter (F4). | F4 |

## Specification coverage

This list maps the guide's architecture rows to the checks that exercise
them. Future integration rows and untested combinations are evidence gaps
in F5, F6 and F7.

| Contract | Evidence |
| --- | --- |
| A5, 03 grant/context/policy and 06 host steps 1–4 | Forged/foreign/stale grant tests, ancestor-policy test, and the gate-denial tests above. Real-object enforcement is the later adapter gate. |
| A6, 03 ambient access and 06 fake world | Interpreter ambient-read test and the workspace's `run_one` creates a new `WorldDriver` at the test boundary. Real driver modules are empty (F6). |
| A7, 03 typed delivery and 06 error mapping | `V2-GRANT-denied-is-value`, `V2-GRANT-limit-exceeded-is-value`, `atk_driver_error_not_fault`; F1 for adapter validation. |
| A8–A9, 03 grants are sealed and 06 encoding | Grant constructor and observation negatives, module model, IR grant constant, trace decode and hold-with-failure probes. |
| A10–A12, foreign/released lineages, model dispatch, registry ownership and versioning | Host stale/derived-stale tests, held-model fixtures, externals owner/signature tests. Release startup coverage remains for the later gate. |
| A13, 03 narrowing and 06 accounting | `V2-GRANT-restrict-keeps-parent`, `V2-GRANT-narrow-forward-limit`, `V2-GRANT-sub-rebases`, `V2-GRANT-parent-stays-valid`. |
| A14, reservations and refunds | Counter atomicity, overflow, progress, and random-tree reference model in `broker.rs`. |
| A15–A16, correlated rules, wire limits and normalized policies | `atk_policy_rule_fields_crossed`, `V2-GRANT-rule-correlation`, host depth tests and corpus policy boundary tests. |
| A27–A28, world-copy isolation and target/world separation | World corpus, workspace isolated initializers and fake-only broker branch. Running real targets remains for a later gate. |
| A29–A30, 03 once-only delivery and 06 trace step 5 | Await-twice, no redispatch, malformed reply and trace receipt tests, independent snapshot check. |
| 03 Target admission, approved bindings and published effects | Guide excludes real targets and drivers. Recipe structures are present; approval, startup and enforcement require later integration evidence (F6). |
| 03 filesystem and HTTP MUSTs | World tests exercise boundary logic and shared policy semantics. Real canonical-name, link, race, and network enforcement require adapter evidence (F6). |
| 06 world paths, schema, and fault injection | World decoder cases cover field rejection, path checks, suffix widths, implied ancestors and file/directory conflicts. The current world schema has no fault-injection field. |
| 06 trace origins, invocation numbering and canonical documents | Trace round-trip, origin/ordinal corpus and independent derivation below. Provider capability separation remains future integration evidence. |

## Independent trace derivation

All samples are checked against [chapter 06](../../../../spec/06-runtime.md#audit-trace).
Common fields are `@auditTrace.v2`, no target, driver
`{@hostWorld.v2, 0.0.2}`, redaction `@none`, and one
`{kind: @testWorld}` origin for each grant projection. Revision verification
uses the repository's independent `revisions.ps1 -Check` and succeeds.
The test run emits `@command.ok` with each listed test passing. Each event
is `@host` in phase `@entry`. The table gives source offsets in UTF-8
bytes, using the raw source up to the first byte of the `await` and then
the byte length of the full await expression. Invocation 1 is the test;
every start advances the ordinal, including source procedures and world
operations whose starts have no trace row. Every successful payload below
is the canonical `Result.ok` payload with its operation's result and error
type from the registry.

| Snapshot | Invocation / parent / root | Operation, await origin and result | Projection |
| --- | --- | --- | --- |
| [`sub-projection`](../../../../../conformance/cases/GRANT/V2-GRANT-sub-projection/test-run.vibon) | 3 / 1 / 3 | `fs.readText`, `tests/grants.vib:2741..2776`, `T` | P4; `DirReadText`; no limits; `t.txt` (request-relative) |
| [`limit-projection`](../../../../../conformance/cases/GRANT/V2-GRANT-limit-projection/test-run.vibon) | 3 / 1 / 3 | `fs.readText`, `tests/grants.vib:2465..2495`, `A` | P1; `DirReadText`; invocations 5, input bytes 9; `a.txt` |
| [`denied-outside-policy`](../../../../../conformance/cases/GRANT/V2-GRANT-denied-outside-policy/test-run.vibon) | — | Policy refuses the request before opening an object | Empty events and policies |
| [`forward-model-reads`](../../../../../conformance/cases/GRANT/V2-GRANT-forward-model-reads/test-run.vibon) | 5 / 4 / 3 | `fs.readText`, `src/app/notes.vib:3074..3112`, `A` | P1; `DirReadText`; no limits; `a.txt` |
| [`model-is-not-grant`](../../../../../conformance/cases/GRANT/V2-GRANT-model-is-not-grant/test-run.vibon) | — | Source model returns one line without a host call | Empty events and policies |
| [`denied-no-event`](../../../../../conformance/cases/GRANT/V2-GRANT-denied-no-event/test-run.vibon) | — | Policy refuses the request before opening an object | Empty events and policies |
| [`test-worlds-isolated`](../../../../../conformance/cases/GRANT/V2-GRANT-test-worlds-isolated/test-run.vibon) | 3 / 1 / 6 | `fs.writeText`, `tests/run.vib:408..449`, `void` | P2; `DirWriteText`; no limits; `n.txt`, `x` |
| [`test-worlds-isolated`](../../../../../conformance/cases/GRANT/V2-GRANT-test-worlds-isolated/test-run.vibon) | 3 / 1 / 1 | `fs.readEntries`, `tests/run.vib:698..732`, `[]` | P3; `DirList`; no limits; empty path |
| [`test-stdin-twice`](../../../../../conformance/cases/GRANT/V2-GRANT-test-stdin-twice/test-run.vibon) | 3 / 1 / 4 | `io.stdinText`, `tests/run.vib:288..324`, `x\ny` | No policy; `StdinText`; no limits; `65536u64` |
| [`test-stdin-twice`](../../../../../conformance/cases/GRANT/V2-GRANT-test-stdin-twice/test-run.vibon) | 4 / 1 / 4 | `io.stdinText`, `tests/run.vib:364..400`, empty string | No policy; `StdinText`; no limits; `65536u64` |
| [`test-clock-steps`](../../../../../conformance/cases/GRANT/V2-GRANT-test-clock-steps/test-run.vibon) | 4 / 1 / 2 | `time.nowWall`, `tests/run.vib:416..443`, `5i64` | No policy; `WallClock`; no limits; no operands |
| [`test-clock-steps`](../../../../../conformance/cases/GRANT/V2-GRANT-test-clock-steps/test-run.vibon) | 5 / 1 / 4 | `time.nowWall`, `tests/run.vib:485..512`, `5i64` | No policy; `WallClock`; no limits; no operands |
| [`test-clock-steps`](../../../../../conformance/cases/GRANT/V2-GRANT-test-clock-steps/test-run.vibon) | 6 / 1 / 1 | `time.nowMonotonic`, `tests/run.vib:555..587`, `10u64` | No policy; `MonotonicClock`; no limits; no operands |
| [`test-clock-steps`](../../../../../conformance/cases/GRANT/V2-GRANT-test-clock-steps/test-run.vibon) | 7 / 1 / 3 | `time.nowMonotonic`, `tests/run.vib:630..662`, `13u64` | No policy; `MonotonicClock`; no limits; no operands |
| [`test-clock-steps`](../../../../../conformance/cases/GRANT/V2-GRANT-test-clock-steps/test-run.vibon) | 8 / 1 / 1 | `time.nowMonotonic`, `tests/run.vib:705..737`, `16u64` | No policy; `MonotonicClock`; no limits; no operands |
| [`test-random-vector`](../../../../../conformance/cases/GRANT/V2-GRANT-test-random-vector/test-run.vibon) | 3 / 1 / 1 | `random.generateBytes`, `tests/run.vib:290..329`, `149 110 235` | No policy; `RandomBytes`; no limits; byte count 3 then 8 |
| [`test-random-vector`](../../../../../conformance/cases/GRANT/V2-GRANT-test-random-vector/test-run.vibon) | 4 / 1 / 1 | `random.generateBytes`, `tests/run.vib:419..458`, `3 241 102 178 51 227 239 40` | No policy; `RandomBytes`; no limits; byte count 3 then 8 |


Directory fixtures issue roots in the order `list`, `readBytes`,
`readText`, `stat`, `writeBytes`, `writeText`. The read uses root 3
even after two narrowings, and a write uses root 6. The second isolation
test issues its own roots from 1. Each clock pair issues `monotonic` then
`wall`. Streams use canonical order with `stdinText` fourth. The
forwarding fixture starts the outer source procedure at 3, the model
member at 4 and the host read at 5. The model itself projects no grant.

The two random calls from seed 42 use consecutive SplitMix64 outputs. The
first keeps three little-endian bytes and discards five. The second keeps
the entire next output. Clocks start at 5 and 10 in their respective
signed and unsigned types, with step 3 on the monotonic counter shared
within this world. Both stream reads consume a single finite fixture,
so the second returns the empty string.

For the policies map, the keys are the hashes below. P1 is shared by
the limit and forwarded-read snapshots. P2 and P3 each appear only in
the corresponding isolation test. P4 conjoins P1 and a file/directory
union for each rebased prefix. The event argument keeps the path as the
program supplies it. With no policy, the projection has an empty policy
string and the trace contains no map entry for it.

| Policy | Hash |
| --- | --- |
| P1 | `sha256:e1da6b9c5e485eaa540dd9740f547d1afc4c017c6728308cbbc2abd7a642efe0` |
| P2 | `sha256:98fda770238e06958cfd872bfe7c0b2165ca154251c5ddaf1b2ab06e2df1c030` |
| P3 | `sha256:5e4bcc48ead86cd79b7b6f37803e54fac818b79ad11aa661f06dd7219b4e7928` |
| P4 | `sha256:a9b3501a4aa35317e339a4f23429d78214b37dd8ae29bb2afa6ffc6b90ab891a` |

The following normalized expressions are compacted with a single space
between fields/items, preserving the order shown. Each line's exact
UTF-8 bytes, excluding the newline, are hashed independently.

```vibon
{format: @scopePolicy.v1 family: @filesystem expression: {kind: @fileRule root: @testWorld prefix: [] depth: @tree actions: [@read] names: {kind: @anyName}}}
{format: @scopePolicy.v1 family: @filesystem expression: {kind: @fileRule root: @testWorld prefix: [] depth: @tree actions: [@create @replace] names: {kind: @anyName}}}
{format: @scopePolicy.v1 family: @filesystem expression: {kind: @directoryRule root: @testWorld prefix: [] depth: @tree actions: [@list]}}
{format: @scopePolicy.v1 family: @filesystem expression: {kind: @allOf items: [{kind: @anyOf items: [{kind: @directoryRule root: @testWorld prefix: ["sub" "inner"] depth: @tree actions: [@list @stat]} {kind: @fileRule root: @testWorld prefix: ["sub" "inner"] depth: @tree actions: [@create @read @replace @stat] names: {kind: @anyName}}]} {kind: @anyOf items: [{kind: @directoryRule root: @testWorld prefix: ["sub"] depth: @tree actions: [@list @stat]} {kind: @fileRule root: @testWorld prefix: ["sub"] depth: @tree actions: [@create @read @replace @stat] names: {kind: @anyName}}]} {kind: @fileRule root: @testWorld prefix: [] depth: @tree actions: [@read] names: {kind: @anyName}}]}}
```

All 15 distinct expected seeds in the traces match the chapter 02
declaration identity computation: the domain tag and zero byte, followed
by six UTF-8 strings each framed with its big-endian U64 length. The
library anchor uses package `std`, release `0.0.2`, an empty owner
and the declaration's kind, module atom and name. The same derivation
covers both the grant right identity and the nested canonical value type
records. The checked-in revision script verifies the revision values.
Its output is “every expected revision is current.”

## Public surface of vibra-host

Each entry is a source declaration in the reviewed revision. Public
trait methods and tuple-struct fields are part of the trait/type shown.
The Rust embedding API assumes a trusted caller that owns its adapter.
Live authority from checked Vibra must still enter through broker dispatch.

| Item | Authority boundary |
| --- | --- |
| [`driver::ResourceId`](../../../../../crates/vibra-host/src/driver.rs#L37) | Driver-local identifier, with no broker authority. |
| [`driver::DriverIdentity`](../../../../../crates/vibra-host/src/driver.rs#L42) | Adapter protocol data. It confers no broker token. |
| [`driver::provider`](../../../../../crates/vibra-host/src/driver.rs#L45) | Adapter protocol data. It confers no broker token. |
| [`driver::version`](../../../../../crates/vibra-host/src/driver.rs#L47) | Adapter protocol data. It confers no broker token. |
| [`driver::Resolved`](../../../../../crates/vibra-host/src/driver.rs#L52) | Adapter protocol data. It confers no broker token. |
| [`driver::Acquire`](../../../../../crates/vibra-host/src/driver.rs#L75) | Adapter protocol data. It confers no broker token. |
| [`driver::Operand`](../../../../../crates/vibra-host/src/driver.rs#L87) | Adapter protocol data. It confers no broker token. |
| [`driver::Action`](../../../../../crates/vibra-host/src/driver.rs#L101) | Adapter protocol data. It confers no broker token. |
| [`driver::op`](../../../../../crates/vibra-host/src/driver.rs#L103) | Adapter protocol data. It confers no broker token. |
| [`driver::names`](../../../../../crates/vibra-host/src/driver.rs#L107) | Adapter protocol data. It confers no broker token. |
| [`driver::operand`](../../../../../crates/vibra-host/src/driver.rs#L109) | Adapter protocol data. It confers no broker token. |
| [`driver::FoundAction`](../../../../../crates/vibra-host/src/driver.rs#L114) | Adapter protocol data. It confers no broker token. |
| [`driver::Found`](../../../../../crates/vibra-host/src/driver.rs#L132) | Adapter protocol data. It confers no broker token. |
| [`driver::real_names`](../../../../../crates/vibra-host/src/driver.rs#L137) | Adapter protocol data. It confers no broker token. |
| [`driver::action`](../../../../../crates/vibra-host/src/driver.rs#L139) | Adapter protocol data. It confers no broker token. |
| [`driver::size`](../../../../../crates/vibra-host/src/driver.rs#L143) | Adapter protocol data. It confers no broker token. |
| [`driver::links`](../../../../../crates/vibra-host/src/driver.rs#L146) | Adapter protocol data. It confers no broker token. |
| [`driver::Reserved`](../../../../../crates/vibra-host/src/driver.rs#L151) | Adapter protocol data. It confers no broker token. |
| [`driver::input`](../../../../../crates/vibra-host/src/driver.rs#L153) | Adapter protocol data. It confers no broker token. |
| [`driver::Refused`](../../../../../crates/vibra-host/src/driver.rs#L159) | Adapter protocol data. It confers no broker token. |
| [`driver::Gate`](../../../../../crates/vibra-host/src/driver.rs#L170) | Gate interface for the adapter handoff. A public implementation has no path to the context's live counters. |
| [`driver::Progress`](../../../../../crates/vibra-host/src/driver.rs#L183) | Adapter protocol data. It confers no broker token. |
| [`driver::input`](../../../../../crates/vibra-host/src/driver.rs#L185) | Adapter protocol data. It confers no broker token. |
| [`driver::output`](../../../../../crates/vibra-host/src/driver.rs#L187) | Adapter protocol data. It confers no broker token. |
| [`driver::NONE`](../../../../../crates/vibra-host/src/driver.rs#L192) | Adapter protocol data. It confers no broker token. |
| [`driver::Measured`](../../../../../crates/vibra-host/src/driver.rs#L200) | Adapter protocol data. It confers no broker token. |
| [`driver::Completion`](../../../../../crates/vibra-host/src/driver.rs#L209) | Adapter protocol data. It confers no broker token. |
| [`driver::Driver`](../../../../../crates/vibra-host/src/driver.rs#L224) | Trusted adapter operations. A caller's own driver is callable; the context's borrowed driver stays private. |
| [`path::RESERVED_STEMS`](../../../../../crates/vibra-host/src/path.rs#L25) | Name data or validation. |
| [`path::NameError`](../../../../../crates/vibra-host/src/path.rs#L62) | Name data or validation. |
| [`path::check_name`](../../../../../crates/vibra-host/src/path.rs#L105) | Name data or validation. |
| [`path::Name`](../../../../../crates/vibra-host/src/path.rs#L157) | Name data or validation. |
| [`path::new`](../../../../../crates/vibra-host/src/path.rs#L165) | Name data or validation. |
| [`path::as_str`](../../../../../crates/vibra-host/src/path.rs#L173) | Name data or validation. |
| [`path::RelativeNames`](../../../../../crates/vibra-host/src/path.rs#L188) | Name data or validation. |
| [`path::InvalidName`](../../../../../crates/vibra-host/src/path.rs#L193) | Name data or validation. |
| [`path::index`](../../../../../crates/vibra-host/src/path.rs#L195) | Name data or validation. |
| [`path::error`](../../../../../crates/vibra-host/src/path.rs#L197) | Name data or validation. |
| [`path::empty`](../../../../../crates/vibra-host/src/path.rs#L203) | Name data or validation. |
| [`path::new`](../../../../../crates/vibra-host/src/path.rs#L212) | Name data or validation. |
| [`path::names`](../../../../../crates/vibra-host/src/path.rs#L229) | Name data or validation. |
| [`path::len`](../../../../../crates/vibra-host/src/path.rs#L235) | Name data or validation. |
| [`path::is_empty`](../../../../../crates/vibra-host/src/path.rs#L241) | Name data or validation. |
| [`path::is_prefix_of`](../../../../../crates/vibra-host/src/path.rs#L248) | Name data or validation. |
| [`broker::RootGrant`](../../../../../crates/vibra-host/src/broker.rs#L98) | Describes an issuer's proposed root. Ordinary issuance is crate-private. |
| [`broker::interface`](../../../../../crates/vibra-host/src/broker.rs#L100) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::resource`](../../../../../crates/vibra-host/src/broker.rs#L102) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::origin`](../../../../../crates/vibra-host/src/broker.rs#L104) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::policy`](../../../../../crates/vibra-host/src/broker.rs#L107) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::variables`](../../../../../crates/vibra-host/src/broker.rs#L110) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::limits`](../../../../../crates/vibra-host/src/broker.rs#L112) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::Lineage`](../../../../../crates/vibra-host/src/broker.rs#L118) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::driver`](../../../../../crates/vibra-host/src/broker.rs#L131) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::resource`](../../../../../crates/vibra-host/src/broker.rs#L137) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::origin`](../../../../../crates/vibra-host/src/broker.rs#L143) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::root`](../../../../../crates/vibra-host/src/broker.rs#L149) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::generation`](../../../../../crates/vibra-host/src/broker.rs#L155) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::is_live`](../../../../../crates/vibra-host/src/broker.rs#L161) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::Counter`](../../../../../crates/vibra-host/src/broker.rs#L169) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::bound`](../../../../../crates/vibra-host/src/broker.rs#L195) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::used`](../../../../../crates/vibra-host/src/broker.rs#L201) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::reserved`](../../../../../crates/vibra-host/src/broker.rs#L207) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::Counters`](../../../../../crates/vibra-host/src/broker.rs#L245) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::invocations`](../../../../../crates/vibra-host/src/broker.rs#L262) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::input_bytes`](../../../../../crates/vibra-host/src/broker.rs#L268) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::output_bytes`](../../../../../crates/vibra-host/src/broker.rs#L274) | Broker descriptor or read-only accessor. Live context state stays private. |
| [`broker::Context`](../../../../../crates/vibra-host/src/broker.rs#L793) | Owns private slots, lineages, driver binding and pending state. |
| [`broker::new`](../../../../../crates/vibra-host/src/broker.rs#L823) | An empty context or a new value, depending on the receiver type. |
| [`broker::for_world`](../../../../../crates/vibra-host/src/broker.rs#L844) | Creates a context over a caller-owned fake driver. |
| [`broker::test_root`](../../../../../crates/vibra-host/src/broker.rs#L856) | Issues one test root on a fake context. It cannot bind a system resource. |
| [`broker::trace`](../../../../../crates/vibra-host/src/broker.rs#L869) | Returns the immutable context trace. |
| [`broker::uncommitted`](../../../../../crates/vibra-host/src/broker.rs#L877) | Reads pending-count metadata only. |
| [`broker::root_for_tests`](../../../../../crates/vibra-host/src/broker.rs#L1923) | Feature-gated fixture issuance. |
| [`broker::release_for_tests`](../../../../../crates/vibra-host/src/broker.rs#L1928) | Feature-gated fixture release. |
| [`broker::startup_event_for_tests`](../../../../../crates/vibra-host/src/broker.rs#L1937) | Feature-gated fixture startup-event insertion (F2). |
| [`broker::counters_for_tests`](../../../../../crates/vibra-host/src/broker.rs#L1950) | Feature-gated fixture counter inspection. |
| [`broker::preset_for_tests`](../../../../../crates/vibra-host/src/broker.rs#L1957) | Feature-gated fixture counter mutation. |
| [`broker::slot_for_tests`](../../../../../crates/vibra-host/src/broker.rs#L1973) | Feature-gated fixture slot inspection. |
| [`recipe::LeafName`](../../../../../crates/vibra-host/src/recipe.rs#L36) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::new`](../../../../../crates/vibra-host/src/recipe.rs#L41) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::as_str`](../../../../../crates/vibra-host/src/recipe.rs#L47) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::Provider`](../../../../../crates/vibra-host/src/recipe.rs#L60) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::atom`](../../../../../crates/vibra-host/src/recipe.rs#L84) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::from_atom`](../../../../../crates/vibra-host/src/recipe.rs#L100) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::row`](../../../../../crates/vibra-host/src/recipe.rs#L109) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::ResourceKind`](../../../../../crates/vibra-host/src/recipe.rs#L126) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::atom`](../../../../../crates/vibra-host/src/recipe.rs#L148) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::ProviderRow`](../../../../../crates/vibra-host/src/recipe.rs#L165) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::provider`](../../../../../crates/vibra-host/src/recipe.rs#L167) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::grant_types`](../../../../../crates/vibra-host/src/recipe.rs#L169) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::resource_kinds`](../../../../../crates/vibra-host/src/recipe.rs#L171) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::PROVIDERS`](../../../../../crates/vibra-host/src/recipe.rs#L176) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::Resource`](../../../../../crates/vibra-host/src/recipe.rs#L237) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::kind`](../../../../../crates/vibra-host/src/recipe.rs#L278) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::source`](../../../../../crates/vibra-host/src/recipe.rs#L293) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::canonical_bytes`](../../../../../crates/vibra-host/src/recipe.rs#L310) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::Limits`](../../../../../crates/vibra-host/src/recipe.rs#L351) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::invocations`](../../../../../crates/vibra-host/src/recipe.rs#L353) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::input_bytes`](../../../../../crates/vibra-host/src/recipe.rs#L355) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::output_bytes`](../../../../../crates/vibra-host/src/recipe.rs#L357) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::is_unbounded`](../../../../../crates/vibra-host/src/recipe.rs#L363) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::canonical_bytes`](../../../../../crates/vibra-host/src/recipe.rs#L372) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::Leaf`](../../../../../crates/vibra-host/src/recipe.rs#L394) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::name`](../../../../../crates/vibra-host/src/recipe.rs#L396) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::interface`](../../../../../crates/vibra-host/src/recipe.rs#L399) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::optional`](../../../../../crates/vibra-host/src/recipe.rs#L401) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::provider`](../../../../../crates/vibra-host/src/recipe.rs#L403) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::resource`](../../../../../crates/vibra-host/src/recipe.rs#L405) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::policy`](../../../../../crates/vibra-host/src/recipe.rs#L407) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::limits`](../../../../../crates/vibra-host/src/recipe.rs#L409) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::after`](../../../../../crates/vibra-host/src/recipe.rs#L411) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::recipe_identity`](../../../../../crates/vibra-host/src/recipe.rs#L413) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::span`](../../../../../crates/vibra-host/src/recipe.rs#L415) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::Field`](../../../../../crates/vibra-host/src/recipe.rs#L420) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::Problem`](../../../../../crates/vibra-host/src/recipe.rs#L435) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::BindingFault`](../../../../../crates/vibra-host/src/recipe.rs#L471) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::leaf`](../../../../../crates/vibra-host/src/recipe.rs#L473) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::field`](../../../../../crates/vibra-host/src/recipe.rs#L475) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::problem`](../../../../../crates/vibra-host/src/recipe.rs#L477) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::BindingPlan`](../../../../../crates/vibra-host/src/recipe.rs#L482) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::target`](../../../../../crates/vibra-host/src/recipe.rs#L484) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::leaves`](../../../../../crates/vibra-host/src/recipe.rs#L486) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::validate`](../../../../../crates/vibra-host/src/recipe.rs#L504) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::order`](../../../../../crates/vibra-host/src/recipe.rs#L528) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::recipe_bytes`](../../../../../crates/vibra-host/src/recipe.rs#L677) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::RECIPE_IDENTITY_TAG`](../../../../../crates/vibra-host/src/recipe.rs#L709) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`recipe::recipe_identity`](../../../../../crates/vibra-host/src/recipe.rs#L715) | Recipe data, ordering or identity. This surface performs no acquisition. |
| [`random::SplitMix64`](../../../../../crates/vibra-host/src/random.rs#L21) | Deterministic numeric generator. |
| [`random::new`](../../../../../crates/vibra-host/src/random.rs#L28) | Deterministic numeric generator. |
| [`random::next_u64`](../../../../../crates/vibra-host/src/random.rs#L33) | Deterministic numeric generator. |
| [`random::bytes`](../../../../../crates/vibra-host/src/random.rs#L42) | Deterministic numeric generator. |
| [`policy::FORMAT`](../../../../../crates/vibra-host/src/policy.rs#L50) | Policy data or pure policy computation. |
| [`policy::FILESYSTEM`](../../../../../crates/vibra-host/src/policy.rs#L53) | Policy data or pure policy computation. |
| [`policy::HTTP`](../../../../../crates/vibra-host/src/policy.rs#L56) | Policy data or pure policy computation. |
| [`policy::MAX_SIZE`](../../../../../crates/vibra-host/src/policy.rs#L59) | Policy data or pure policy computation. |
| [`policy::MAX_DEPTH`](../../../../../crates/vibra-host/src/policy.rs#L62) | Policy data or pure policy computation. |
| [`policy::Join`](../../../../../crates/vibra-host/src/policy.rs#L66) | Policy data or pure policy computation. |
| [`policy::atom`](../../../../../crates/vibra-host/src/policy.rs#L76) | Policy data or pure policy computation. |
| [`policy::FileDepth`](../../../../../crates/vibra-host/src/policy.rs#L86) | Policy data or pure policy computation. |
| [`policy::atom`](../../../../../crates/vibra-host/src/policy.rs#L96) | Policy data or pure policy computation. |
| [`policy::DirDepth`](../../../../../crates/vibra-host/src/policy.rs#L114) | Policy data or pure policy computation. |
| [`policy::atom`](../../../../../crates/vibra-host/src/policy.rs#L126) | Policy data or pure policy computation. |
| [`policy::FileAction`](../../../../../crates/vibra-host/src/policy.rs#L146) | Policy data or pure policy computation. |
| [`policy::atom`](../../../../../crates/vibra-host/src/policy.rs#L160) | Policy data or pure policy computation. |
| [`policy::DirAction`](../../../../../crates/vibra-host/src/policy.rs#L182) | Policy data or pure policy computation. |
| [`policy::atom`](../../../../../crates/vibra-host/src/policy.rs#L192) | Policy data or pure policy computation. |
| [`policy::FileRule`](../../../../../crates/vibra-host/src/policy.rs#L210) | Policy data or pure policy computation. |
| [`policy::root`](../../../../../crates/vibra-host/src/policy.rs#L212) | Policy data or pure policy computation. |
| [`policy::prefix`](../../../../../crates/vibra-host/src/policy.rs#L214) | Policy data or pure policy computation. |
| [`policy::depth`](../../../../../crates/vibra-host/src/policy.rs#L216) | Policy data or pure policy computation. |
| [`policy::actions`](../../../../../crates/vibra-host/src/policy.rs#L218) | Policy data or pure policy computation. |
| [`policy::names`](../../../../../crates/vibra-host/src/policy.rs#L220) | Policy data or pure policy computation. |
| [`policy::DirectoryRule`](../../../../../crates/vibra-host/src/policy.rs#L225) | Policy data or pure policy computation. |
| [`policy::root`](../../../../../crates/vibra-host/src/policy.rs#L227) | Policy data or pure policy computation. |
| [`policy::prefix`](../../../../../crates/vibra-host/src/policy.rs#L229) | Policy data or pure policy computation. |
| [`policy::depth`](../../../../../crates/vibra-host/src/policy.rs#L231) | Policy data or pure policy computation. |
| [`policy::actions`](../../../../../crates/vibra-host/src/policy.rs#L233) | Policy data or pure policy computation. |
| [`policy::Expr`](../../../../../crates/vibra-host/src/policy.rs#L241) | Policy data or pure policy computation. |
| [`policy::NameTest`](../../../../../crates/vibra-host/src/policy.rs#L258) | Policy data or pure policy computation. |
| [`policy::canonical_bytes`](../../../../../crates/vibra-host/src/policy.rs#L563) | Policy data or pure policy computation. |
| [`policy::rules`](../../../../../crates/vibra-host/src/policy.rs#L571) | Policy data or pure policy computation. |
| [`policy::canonical_bytes`](../../../../../crates/vibra-host/src/policy.rs#L579) | Policy data or pure policy computation. |
| [`policy::Rule`](../../../../../crates/vibra-host/src/policy.rs#L646) | Policy data or pure policy computation. |
| [`policy::root`](../../../../../crates/vibra-host/src/policy.rs#L656) | Policy data or pure policy computation. |
| [`policy::Rules`](../../../../../crates/vibra-host/src/policy.rs#L667) | Policy data or pure policy computation. |
| [`policy::TooComplex`](../../../../../crates/vibra-host/src/policy.rs#L693) | Policy data or pure policy computation. |
| [`policy::Size`](../../../../../crates/vibra-host/src/policy.rs#L709) | Policy data or pure policy computation. |
| [`policy::total`](../../../../../crates/vibra-host/src/policy.rs#L712) | Policy data or pure policy computation. |
| [`policy::depth`](../../../../../crates/vibra-host/src/policy.rs#L715) | Policy data or pure policy computation. |
| [`policy::fits`](../../../../../crates/vibra-host/src/policy.rs#L722) | Policy data or pure policy computation. |
| [`policy::count`](../../../../../crates/vibra-host/src/policy.rs#L772) | Policy data or pure policy computation. |
| [`policy::normalize`](../../../../../crates/vibra-host/src/policy.rs#L989) | Policy data or pure policy computation. |
| [`policy::Policy`](../../../../../crates/vibra-host/src/policy.rs#L995) | Policy data or pure policy computation. |
| [`policy::new`](../../../../../crates/vibra-host/src/policy.rs#L1007) | Policy data or pure policy computation. |
| [`policy::expression`](../../../../../crates/vibra-host/src/policy.rs#L1019) | Policy data or pure policy computation. |
| [`policy::size`](../../../../../crates/vibra-host/src/policy.rs#L1025) | Policy data or pure policy computation. |
| [`policy::canonical_bytes`](../../../../../crates/vibra-host/src/policy.rs#L1032) | Policy data or pure policy computation. |
| [`policy::hash`](../../../../../crates/vibra-host/src/policy.rs#L1060) | Policy data or pure policy computation. |
| [`policy::Action`](../../../../../crates/vibra-host/src/policy.rs#L1066) | Policy data or pure policy computation. |
| [`policy::Request`](../../../../../crates/vibra-host/src/policy.rs#L1076) | Policy data or pure policy computation. |
| [`policy::names`](../../../../../crates/vibra-host/src/policy.rs#L1079) | Policy data or pure policy computation. |
| [`policy::action`](../../../../../crates/vibra-host/src/policy.rs#L1081) | Policy data or pure policy computation. |
| [`policy::permits`](../../../../../crates/vibra-host/src/policy.rs#L1142) | Policy data or pure policy computation. |
| [`policy::PolicyError`](../../../../../crates/vibra-host/src/policy.rs#L1153) | Policy data or pure policy computation. |
| [`policy::decode`](../../../../../crates/vibra-host/src/policy.rs#L1436) | Policy data or pure policy computation. |
| [`policy::decode_value`](../../../../../crates/vibra-host/src/policy.rs#L1458) | Policy data or pure policy computation. |
| [`lib::broker`](../../../../../crates/vibra-host/src/lib.rs#L21) | Module or re-export. Its public contents are listed here. |
| [`lib::driver`](../../../../../crates/vibra-host/src/lib.rs#L22) | Module or re-export. Its public contents are listed here. |
| [`lib::path`](../../../../../crates/vibra-host/src/lib.rs#L23) | Module or re-export. Its public contents are listed here. |
| [`lib::policy`](../../../../../crates/vibra-host/src/lib.rs#L24) | Module or re-export. Its public contents are listed here. |
| [`lib::recipe`](../../../../../crates/vibra-host/src/lib.rs#L25) | Module or re-export. Its public contents are listed here. |
| [`lib::startup`](../../../../../crates/vibra-host/src/lib.rs#L26) | Placeholder module. |
| [`lib::system`](../../../../../crates/vibra-host/src/lib.rs#L27) | Placeholder module. |
| [`lib::test_world`](../../../../../crates/vibra-host/src/lib.rs#L28) | Module or re-export. Its public contents are listed here. |
| [`lib::trace`](../../../../../crates/vibra-host/src/lib.rs#L29) | Module or re-export. Its public contents are listed here. |
| [`lib::world`](../../../../../crates/vibra-host/src/lib.rs#L30) | Module or re-export. Its public contents are listed here. |
| [`lib::random`](../../../../../crates/vibra-host/src/lib.rs#L34) | Module or re-export. Its public contents are listed here. |
| [`trace::TRACE_FORMAT`](../../../../../crates/vibra-host/src/trace.rs#L53) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::NO_REDACTION`](../../../../../crates/vibra-host/src/trace.rs#L56) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::declaration_identity`](../../../../../crates/vibra-host/src/trace.rs#L78) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::StdlibIdentity`](../../../../../crates/vibra-host/src/trace.rs#L99) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::package`](../../../../../crates/vibra-host/src/trace.rs#L101) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::version`](../../../../../crates/vibra-host/src/trace.rs#L103) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::seed`](../../../../../crates/vibra-host/src/trace.rs#L110) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::TraceHeader`](../../../../../crates/vibra-host/src/trace.rs#L118) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::revision`](../../../../../crates/vibra-host/src/trace.rs#L120) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::target`](../../../../../crates/vibra-host/src/trace.rs#L122) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::library`](../../../../../crates/vibra-host/src/trace.rs#L124) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::GrantOrigin`](../../../../../crates/vibra-host/src/trace.rs#L129) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::policy_root`](../../../../../crates/vibra-host/src/trace.rs#L143) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::canonical_bytes`](../../../../../crates/vibra-host/src/trace.rs#L152) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::GrantProjection`](../../../../../crates/vibra-host/src/trace.rs#L171) | Inert trace data, with no token encoding. |
| [`trace::issued_by`](../../../../../crates/vibra-host/src/trace.rs#L173) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::policy`](../../../../../crates/vibra-host/src/trace.rs#L176) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::rights`](../../../../../crates/vibra-host/src/trace.rs#L178) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::limits`](../../../../../crates/vibra-host/src/trace.rs#L181) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::root`](../../../../../crates/vibra-host/src/trace.rs#L183) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::canonical_bytes`](../../../../../crates/vibra-host/src/trace.rs#L190) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::json`](../../../../../crates/vibra-host/src/trace.rs#L208) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::StartupStatus`](../../../../../crates/vibra-host/src/trace.rs#L240) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::atom`](../../../../../crates/vibra-host/src/trace.rs#L252) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::FailureCode`](../../../../../crates/vibra-host/src/trace.rs#L269) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::atom`](../../../../../crates/vibra-host/src/trace.rs#L279) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::of`](../../../../../crates/vibra-host/src/trace.rs#L297) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::StartupFailure`](../../../../../crates/vibra-host/src/trace.rs#L308) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::code`](../../../../../crates/vibra-host/src/trace.rs#L310) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::message`](../../../../../crates/vibra-host/src/trace.rs#L312) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::StartupEvent`](../../../../../crates/vibra-host/src/trace.rs#L317) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::binding`](../../../../../crates/vibra-host/src/trace.rs#L319) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::provider`](../../../../../crates/vibra-host/src/trace.rs#L321) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::status`](../../../../../crates/vibra-host/src/trace.rs#L323) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::grant`](../../../../../crates/vibra-host/src/trace.rs#L325) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::failure`](../../../../../crates/vibra-host/src/trace.rs#L327) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::is_consistent`](../../../../../crates/vibra-host/src/trace.rs#L335) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::canonical_bytes`](../../../../../crates/vibra-host/src/trace.rs#L347) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::json`](../../../../../crates/vibra-host/src/trace.rs#L373) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::Phase`](../../../../../crates/vibra-host/src/trace.rs#L389) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::atom`](../../../../../crates/vibra-host/src/trace.rs#L399) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::HostEvent`](../../../../../crates/vibra-host/src/trace.rs#L415) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::invocation`](../../../../../crates/vibra-host/src/trace.rs#L417) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::parent`](../../../../../crates/vibra-host/src/trace.rs#L420) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::phase`](../../../../../crates/vibra-host/src/trace.rs#L422) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::op`](../../../../../crates/vibra-host/src/trace.rs#L424) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::driver`](../../../../../crates/vibra-host/src/trace.rs#L426) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::origin`](../../../../../crates/vibra-host/src/trace.rs#L428) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::grant`](../../../../../crates/vibra-host/src/trace.rs#L430) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::args`](../../../../../crates/vibra-host/src/trace.rs#L433) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::result`](../../../../../crates/vibra-host/src/trace.rs#L435) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::canonical_bytes`](../../../../../crates/vibra-host/src/trace.rs#L442) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::json`](../../../../../crates/vibra-host/src/trace.rs#L475) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::OpaqueEvent`](../../../../../crates/vibra-host/src/trace.rs#L510) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::kind`](../../../../../crates/vibra-host/src/trace.rs#L518) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::canonical_bytes`](../../../../../crates/vibra-host/src/trace.rs#L526) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::Event`](../../../../../crates/vibra-host/src/trace.rs#L533) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::canonical_bytes`](../../../../../crates/vibra-host/src/trace.rs#L545) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::json`](../../../../../crates/vibra-host/src/trace.rs#L556) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::AuditTrace`](../../../../../crates/vibra-host/src/trace.rs#L567) | Data, independent of the context's protected receipt path. |
| [`trace::revision`](../../../../../crates/vibra-host/src/trace.rs#L569) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::target`](../../../../../crates/vibra-host/src/trace.rs#L571) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::drivers`](../../../../../crates/vibra-host/src/trace.rs#L573) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::policies`](../../../../../crates/vibra-host/src/trace.rs#L575) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::redaction`](../../../../../crates/vibra-host/src/trace.rs#L577) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::events`](../../../../../crates/vibra-host/src/trace.rs#L579) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::new`](../../../../../crates/vibra-host/src/trace.rs#L585) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::bind_driver`](../../../../../crates/vibra-host/src/trace.rs#L598) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::encode`](../../../../../crates/vibra-host/src/trace.rs#L616) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::argument_bytes`](../../../../../crates/vibra-host/src/trace.rs#L773) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::result_bytes`](../../../../../crates/vibra-host/src/trace.rs#L797) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`trace::decode`](../../../../../crates/vibra-host/src/trace.rs#L1477) | Inert metadata or serialization. It cannot mint a token or mutate a context trace. |
| [`world::WORLD_FORMAT`](../../../../../crates/vibra-host/src/world.rs#L48) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::DRIVER_VERSION`](../../../../../crates/vibra-host/src/world.rs#L52) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::FixtureConflict`](../../../../../crates/vibra-host/src/world.rs#L77) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::names`](../../../../../crates/vibra-host/src/world.rs#L79) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::WorldError`](../../../../../crates/vibra-host/src/world.rs#L84) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::VirtualFs`](../../../../../crates/vibra-host/src/world.rs#L95) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::new`](../../../../../crates/vibra-host/src/world.rs#L121) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::with_files`](../../../../../crates/vibra-host/src/world.rs#L134) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::file`](../../../../../crates/vibra-host/src/world.rs#L202) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::is_directory`](../../../../../crates/vibra-host/src/world.rs#L211) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::list`](../../../../../crates/vibra-host/src/world.rs#L218) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::directories`](../../../../../crates/vibra-host/src/world.rs#L234) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::files`](../../../../../crates/vibra-host/src/world.rs#L242) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::Clock`](../../../../../crates/vibra-host/src/world.rs#L262) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::wall`](../../../../../crates/vibra-host/src/world.rs#L264) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::monotonic`](../../../../../crates/vibra-host/src/world.rs#L266) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::step`](../../../../../crates/vibra-host/src/world.rs#L268) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::HostWorld`](../../../../../crates/vibra-host/src/world.rs#L284) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::stdin`](../../../../../crates/vibra-host/src/world.rs#L296) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::filesystem`](../../../../../crates/vibra-host/src/world.rs#L303) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::env`](../../../../../crates/vibra-host/src/world.rs#L309) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::args`](../../../../../crates/vibra-host/src/world.rs#L315) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::clock`](../../../../../crates/vibra-host/src/world.rs#L321) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::seed`](../../../../../crates/vibra-host/src/world.rs#L327) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::decode_world`](../../../../../crates/vibra-host/src/world.rs#L816) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::Streams`](../../../../../crates/vibra-host/src/world.rs#L955) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::stdin`](../../../../../crates/vibra-host/src/world.rs#L957) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::stdout`](../../../../../crates/vibra-host/src/world.rs#L959) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::stderr`](../../../../../crates/vibra-host/src/world.rs#L961) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::Environment`](../../../../../crates/vibra-host/src/world.rs#L966) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::variables`](../../../../../crates/vibra-host/src/world.rs#L968) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::arguments`](../../../../../crates/vibra-host/src/world.rs#L970) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::Output`](../../../../../crates/vibra-host/src/world.rs#L975) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::stdout`](../../../../../crates/vibra-host/src/world.rs#L977) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::stderr`](../../../../../crates/vibra-host/src/world.rs#L979) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::WorldDriver`](../../../../../crates/vibra-host/src/world.rs#L1005) | Caller-owned, deterministic fixture driver with public driver methods. |
| [`world::new`](../../../../../crates/vibra-host/src/world.rs#L1026) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::make_directory`](../../../../../crates/vibra-host/src/world.rs#L1060) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::make_streams`](../../../../../crates/vibra-host/src/world.rs#L1082) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::set_environment`](../../../../../crates/vibra-host/src/world.rs#L1101) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |
| [`world::output`](../../../../../crates/vibra-host/src/world.rs#L1117) | Data or a caller-owned fake fixture operation. It cannot reach the real provider. |

The public trait implementations are the two cross-crate boundaries.
`Context` implements `HostContext::host`, `commit`, `narrow`, and
`test_world`. These validate the live token/world or the context-bound
receipt. A `GrantToken` supplied by an embedding carries no usable authority
until the context validates it. The only public `Driver` implementation
here is `WorldDriver`, which acquires, releases, and calls only caller-owned
fixture resources. Direct access to a fixture, including reading its
output, has no route to a hidden context slot.

## Findings

### F1. Defect — validate submission classification at the adapter boundary

[`broker.rs:1337`](../../../../../crates/vibra-host/src/broker.rs#L1337) validates
the row error type for the no-submission completion without establishing
compatibility between the completion class and the error case. The
chapter 06 [result mapping](../../../../spec/06-runtime.md#host-calls) requires
`outcomeUnknown` to preserve uncertain submission. The existing
`atk_counter_refund_unknown` and `an_unknown_outcome_is_one_event` tests
cover the well-formed completed case. This is a source-inspection finding
on the trusted adapter contract. Reject internally inconsistent
completion classifications before settlement, preserving uncertain charge.

### F2. Defect — one test-only method lacks an architecture assertion

[`host_testing_feature.rs:27`](../../../../../crates/vibra-conformance/tests/it/host_testing_feature.rs#L27)
enumerates five methods. The gated
[`startup_event_for_tests`](../../../../../crates/vibra-host/src/broker.rs#L1937)
is a sixth and accepts fixture startup-event input. The active gate is
present. Chapter 06 [Audit trace](../../../../spec/06-runtime.md#audit-trace) says
“One `@startup` event is appended for each processed leaf”. The cited
method and the `the_test_hooks_are_compiled_only_under_the_feature` test
are the smallest evidence of the guard's incomplete inventory. Include
all six methods in the architecture assertion.

### F3. Accepted limit — delivery observer remains in production layout

[`host.rs:67`](../../../../../crates/vibra-interp/src/host.rs#L67) declares the
crate-private callback, and the production return path calls it after
the continuation takes the value, before commit. A source search finds
one setter, in the unit test. The [host protocol](../../../../spec/06-runtime.md#host-calls)
says “Advance the continuation consistently with the completion” before
appending the completed event. The smallest order check is
[`commit_follows_the_continuation`](../../../../../crates/vibra-interp/src/host.rs#L933).
Keep this observer behind `cfg(test)` and retain the existing ordering
assertion with a recording host.

### F4. Question — the chapter leaves the policy digest input implicit

[Chapter 03](../../../../spec/03-effects.md#the-wire-form) specifies normal form and
says “The hash identifies this normal form.”
[`policy.rs:1060`](../../../../../crates/vibra-host/src/policy.rs#L1060) defines
the digest over compact VIBON bytes. The active text does not identify
the digest algorithm, the string prefix, or whether any domain bytes are
prepended. The step 21 ruling uses SHA-256 of canonical policy bytes with
no prefix on the input and a `sha256:` prefix on the output. P1, the
single rule above, is the smallest snapshot example. State the digest
formula in the active policy wire form.

### F5. Question — trusted embedding and source model guarantees

[`Context::test_root`](../../../../../crates/vibra-host/src/broker.rs#L856) is
public and intentionally supports an embedding that constructs its own
fake driver. Chapter 06 [Host world](../../../../spec/06-runtime.md#host-world)
assigns world roots to tests. The smallest positive supporting case is
the normal `TestWorld.streams` fixture. Clarify the expected boundary
between a trusted embedding and Vibra target execution.

Chapter 03 [Narrowing](../../../../spec/03-effects.md#narrowing) says “A model that
wraps a grant forwards the narrowing to it”. The forward cases implement
that pattern in source. Source model behavior is separate from narrowing
a sealed grant, whose ancestry remains broker-owned
([`broker.rs:1390`](../../../../../crates/vibra-host/src/broker.rs#L1390)).
Say whether model forwarding is an author contract or a statically
enforced rule.

### F6. Accepted limit — later adapter and launch gates

[`startup.rs`](../../../../../crates/vibra-host/src/startup.rs) and
[`system.rs`](../../../../../crates/vibra-host/src/system.rs) contain only module
documentation. The scope chapter's requirement that the driver enforce
the file rules at actual use needs real-driver and admission evidence.
The smallest exercised surrogate is the host `gate_real_name_fails`
test. No claim about real filesystem, network, or production target
confinement follows from this gate.

### F7. Accepted limit — probe coverage at this revision

The probe table lists unverified combinations. In particular,
[`deep_ancestry`](../../../../../crates/vibra-host/tests/it/broker.rs#L1447)
covers 10,000 limits. Its scratch variant covers 100,000 limits and passes without an overflow.
Repeated limit construction walks ancestors, which accounts for the
substantial runtime even with an optimized broker. The [runtime memory
rule](../../../../spec/06-runtime.md#activations-and-memory) prohibits host stack use
proportional to language depth. The standard library guard inventory has
one `where` guard, the ruled `RelativePath` case. No separate reproduction
extends that guard issue.

## Scratch observation fixture

Replace `tests/run.vib` in a scratch copy of the
[`test-stdin-twice`](../../../../../conformance/cases/GRANT/V2-GRANT-test-stdin-twice/case.toml) project and run the
built CLI with `--format json test`. The returned failure points to
`assert.true(false)` and exposes no grant value.

```vib
use @std.assert enduse
use @std.test.TestWorld enduse

test "report" takes {world: TestWorld} performs {TestWorld} do
  let held be await TestWorld.streams(world stdin: "fixture") endlet
  assert.true(false)
endtest
```

## Additional scratch fixtures

The following Rust tests append to a scratch copy of the existing
[`broker.rs`](../../../../../crates/vibra-host/tests/it/broker.rs)
test module, using its fake driver and helpers. Each passes.

```rust
#[test]
fn review_empty_sub_keeps_request_coordinates() {
    let script = Scripted::new();
    script.file(&["note.txt"], "note");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let original = root(&mut cx, HostInterface::DirReadText);
    let child = granted(sub(&mut cx, original, &[]));
    assert!(script.calls().is_empty());
    assert_eq!(read(&mut cx, child, &["note.txt"]), Ok((text("note"), true)));
    assert!(matches!(
        script.calls().as_slice(),
        [Event::Call { names: asked, .. }] if *asked == names(&["note.txt"])
    ));
}

#[test]
fn review_unsized_zero_read_keeps_remaining_input() {
    let script = Scripted::new();
    script.stdin("abc", false);
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let input = streams(&mut cx, HostInterface::StdinBytes, Some(3));
    assert_eq!(stdin(&mut cx, HostOp::IoStdinBytes, input, 0), Ok((bytes(""), true)));
    assert_eq!(used(&cx, input), [1, 0, 0]);
}

#[test]
fn review_sub_restrict_sub_preserves_root_coordinates() {
    let script = Scripted::new();
    script.file(&["sub", "inner", "note.txt"], "note");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let original = root(&mut cx, HostInterface::DirReadText);
    let first = granted(sub(&mut cx, original, &["sub"]));
    let policy = files(&[], &["read"], suffix(".txt"));
    let limited = granted(restrict(&mut cx, first, &policy));
    let child = granted(sub(&mut cx, limited, &["inner"]));
    assert_eq!(read(&mut cx, child, &["note.txt"]), Ok((text("note"), true)));
    assert_eq!(
        cx.trace().policies.keys().map(String::as_str).collect::<Vec<_>>(),
        ["sha256:f2ff65dd9a3fed2f5734514bbc6ceefbffce2cd14a981df41e4748d45c3ddde1"]
    );
    assert!(matches!(
        script.calls().as_slice(),
        [Event::Call { names: asked, .. }] if *asked == names(&["sub", "inner", "note.txt"])
    ));
}

#[test]
fn review_depth_and_size_refuse_before_adapter() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let original = root(&mut cx, HostInterface::DirReadText);
    let deep = at_depth_64(&mut cx, original);
    let path = vec!["part"; 4_090];
    let policy = files(&path, &["read"], suffix(".txt"));
    assert_eq!(
        restrict(&mut cx, deep, &policy),
        Narrowed::Refused(ScopeErrorCase::PolicyTooComplex)
    );
    assert_eq!(
        sub(&mut cx, original, &path),
        Narrowed::Refused(ScopeErrorCase::PolicyTooComplex)
    );
    assert!(script.calls().is_empty());
}

#[test]
fn review_adapter_denial_charges_only_the_invocation() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = text_only(&mut cx);
    script.tweak(Tweak {
        real_names: Some(names(&["A.TXT"])),
        ..Tweak::default()
    });
    assert_eq!(
        read(&mut cx, notes, &["a.txt"]),
        Ok((err(HostOp::FsReadText, "denied"), true))
    );
    assert_eq!(used(&cx, notes), [1, 0, 0]);
    assert_eq!(cx.trace().events.len(), 1);
}
```

The composed projection combines the two prefix unions in the order
`["sub" "inner"]`, `["sub"]`, followed by the suffix restriction re-rooted
at `["sub"]` and the base file rule. Its exact hash input is:

```vibon
{format: @scopePolicy.v1 family: @filesystem expression: {kind: @allOf items: [{kind: @anyOf items: [{kind: @directoryRule root: @notes prefix: ["sub" "inner"] depth: @tree actions: [@list @stat]} {kind: @fileRule root: @notes prefix: ["sub" "inner"] depth: @tree actions: [@create @read @replace @stat] names: {kind: @anyName}}]} {kind: @anyOf items: [{kind: @directoryRule root: @notes prefix: ["sub"] depth: @tree actions: [@list @stat]} {kind: @fileRule root: @notes prefix: ["sub"] depth: @tree actions: [@create @read @replace @stat] names: {kind: @anyName}}]} {kind: @fileRule root: @notes prefix: ["sub"] depth: @tree actions: [@read] names: {kind: @suffix value: ".txt"}} {kind: @fileRule root: @notes prefix: [] depth: @tree actions: [@create @read @replace] names: {kind: @anyName}}]}}
```

The stream program uses the same scratch project shape as the failure
report probe. It passes and checks that the refused read consumes nothing.

```vib
use @std.assert enduse
use @std.test.TestWorld enduse
use @std.io.StdinText enduse
use @std.io.IoError enduse
use @std.io.readAllText enduse
use @std.policy.GrantLimits enduse

test "whole text bound" takes {world: TestWorld}
  performs {TestWorld StdinText}
  do
    let {stdinText: input} be await TestWorld.streams(world stdin: "abc") endlet
    let bounds be GrantLimits(invocations: Option.none()
      inputBytes: Option.some(value: 2u64) outputBytes: Option.none()) endlet
    let bounded be StdinText.limit(input bounds) endlet
    assert.equal(Result.err(error: IoError.limitExceeded()) await readAllText(bounded))
    assert.equal(Result.ok(value: "abc") await readAllText(input))
endtest
```

The completed-error propagation fixture uses the same scratch project
shape. It passes with one event at invocation 4, parent 3 and root 3.
The event encodes `notFound` and `try` propagates that same error.

```vib
use @std.assert enduse
use @std.test.TestWorld enduse
use @std.fs.DirReadText enduse
use @std.fs.FsError enduse
use @std.fs.RelativePath enduse

proc readThrough takes {reader: DirReadText at: RelativePath}
  returns outcome: Result(Str FsError)
  performs {DirReadText}
  do
    let value be try await DirReadText.read(reader at) endlet
    return Result.ok(value: value)
endproc

test "propagates completed error" takes {world: TestWorld}
  performs {TestWorld DirReadText}
  do
    let {readText: reader} be await TestWorld.directory(world) endlet
    match RelativePath(["absent.txt"])
      case Result.ok(value: at)
        assert.equal(Result.err(error: FsError.notFound()) await readThrough(reader at))
      case Result.err(error: _)
        assert.true(false)
    endmatch
endtest
```

Replace the test file in a scratch copy of the
[`trace-trap-no-event`](../../../../../conformance/cases/RUNTIME/V2-RUNTIME-trace-trap-no-event/case.toml)
fixture for the isolation check. The first test reports
`@test.exhausted`, and the next passes. Both traces contain the same
four operation, invocation, parent and root tuples:
`stdinBytes 3/1/3`, `nowMonotonic 6/1/7`, `nowMonotonic 7/1/7`,
and `generateBytes 9/1/9`. The exhausted call adds no event.

```vib
use @std.assert enduse
use @std.test.TestWorld enduse
use @std.io.StdinBytes enduse
use @std.io.IoError enduse
use @std.policy.GrantLimits enduse
use @std.time.MonotonicClock enduse
use @std.random.RandomBytes enduse
use @app.main enduse

test "first exhausts" takes {world: TestWorld}
  performs {TestWorld StdinBytes MonotonicClock RandomBytes}
  do
    let {stdinBytes: input} be await TestWorld.streams(world stdin: "x") endlet
    let lim be GrantLimits(invocations: Option.some(value: 1u64)
      inputBytes: Option.none() outputBytes: Option.none()) endlet
    let bounded be StdinBytes.limit(input lim) endlet
    assert.equal(Result.ok(value: Bytes([120u8])) await StdinBytes.read(bounded 1u64))
    assert.equal(Result.err(error: IoError.limitExceeded()) await StdinBytes.read(bounded 1u64))
    let {monotonic: clock} be await TestWorld.clocks(world) endlet
    assert.equal(Result.ok(value: 0u64) await MonotonicClock.read(clock))
    assert.equal(Result.ok(value: 1u64) await MonotonicClock.read(clock))
    let random be await TestWorld.random(world) endlet
    assert.equal(Result.ok(value: Bytes([175u8])) await RandomBytes.generate(random 1u64))
    let _ be await main.spin() endlet
endtest

test "next passes" takes {world: TestWorld}
  performs {TestWorld StdinBytes MonotonicClock RandomBytes}
  do
    let {stdinBytes: input} be await TestWorld.streams(world stdin: "x") endlet
    let lim be GrantLimits(invocations: Option.some(value: 1u64)
      inputBytes: Option.none() outputBytes: Option.none()) endlet
    let bounded be StdinBytes.limit(input lim) endlet
    assert.equal(Result.ok(value: Bytes([120u8])) await StdinBytes.read(bounded 1u64))
    assert.equal(Result.err(error: IoError.limitExceeded()) await StdinBytes.read(bounded 1u64))
    let {monotonic: clock} be await TestWorld.clocks(world) endlet
    assert.equal(Result.ok(value: 0u64) await MonotonicClock.read(clock))
    assert.equal(Result.ok(value: 1u64) await MonotonicClock.read(clock))
    let random be await TestWorld.random(world) endlet
    assert.equal(Result.ok(value: Bytes([175u8])) await RandomBytes.generate(random 1u64))
endtest
```

The deep-ancestry scratch source is the linked `deep_ancestry` test with
the single range replacement below. It uses the same 256 KiB stack and
performs the same final read. The optimized scratch binary links
`cargo build --locked --offline --release -p vibra-host --features testing`.

```rust
for _ in 0..100_000 {
    grant = limit(&mut cx, grant, invocations(Some(3)));
}
```

## Validation

- Four-crate guide suite with `--all-features`: 1,012 tests pass.
- Full conformance runner: 1,092 pass, zero failed, unknown or unavailable.
- `host_testing_feature` integration filter: three pass.
- Independent corpus revision check: every expected revision is current.
- `cargo fmt --all --check`: passes.
- Workspace Clippy with all targets and features: passes.
- Workspace documentation build with all features: passes.
