# Step 23: review-grants

Kind: evidence. The review gate of slices 2, 3, and 6: an adversarial
reading of the broker, policies, the fake world, the trace, and grant
values, before any real driver or target runs. It writes findings and
changes no code.
Depends on: 22.
Crates: none.

## Prerequisites

- Steps 14 to 22 are merged.
- Read: [effects chapter](../../../../spec/03-effects.md) from "Capability
  values" to "Delivery and typed outcomes"; [runtime
  chapter](../../../../spec/06-runtime.md#host-calls) "Host calls", "Host
  world", "Audit trace"; [architecture](../architecture.md) rows A5 to
  A16, A27 to A30.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-ir/src/external/host.rs:842` | `pub fn host_row` | the rows the broker dispatches |
| `crates/vibra-workspace/src/test_runner.rs:621` | `fn run_one` | where a world and a context are made |

It reads `crates/vibra-host/src/`, `crates/vibra-interp/src/host.rs`,
`crates/vibra-types/src/check/effects/grants.rs`, and `stdlib/src/std/`.

## Files owned

- `docs/roadmap/v0.0.2/m2/evidence/review-grants.md`

## Ordered tasks

1. For every MUST of the sections above, name the case or host test that
   shows it, or record the gap as a finding.
2. Run each probe below and record the observed result.
3. Read every `pub` item of `vibra-host`: can a caller outside the crate
   make a root grant, read a counter, or reach a driver without the five
   steps? Each such path is a finding.
4. Read the hand-written trace snapshots of steps 21 and 22 against the
   chapter, independently of the implementation: a snapshot that encodes
   an implementation choice is a finding.
5. Classify each finding: defect, question, or accepted limit.

## Test matrix

Probes; each is run from a scratch test or case and recorded.

| Probe | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| a token across worlds | negative | a grant stored in a module value by one test, used by the next | impossible to write, or `@runtime.invalidHostValue` | 03: "A grant from another world … is unusable" |
| a grant through `Any` | negative | a generic `fn id(T)` returning its argument, used to move a grant; then every way to observe it | only use as the same interface | 02 Interface values: "A value of type `Any` can only be passed along" |
| a grant in a failure report | negative | `assert.true(false)` in a test holding grants; an `assert.equal` on a record beside a grant | no grant text in `@testRun.v2` | 05: "A function, a plan, or a grant is never encoded by inventing a string for it" |
| policy equivalence shortcuts | negative | search `policy.rs` and `broker.rs` for any subset or implication test | none | 03: "no implementation needs to decide whether one policy is a subset of another" |
| counter atomicity | negative | a reservation that succeeds on the child and fails on the root | no counter changed | 06 step 3: "Reserve atomically the shared count limits and the byte limits the operands decide" |
| limits after a gate refusal | boundary | a completed `denied` on a bounded grant | the invocation is charged; the bytes are not | 03 Limits: "An invocation is every call that passed reservation" |
| `outcomeUnknown` | negative | every place a driver error is mapped | none maps an unknown completion to a pre-dispatch refusal | 06: "It MUST NOT be reported as rejected before submission" |
| world decoding as authority | negative | a world document with fields named like grants | rejected as unknown fields | 06: "decoding one MUST NOT restore a live grant" |
| trace completeness | negative | a test that traps between the adapter's return and the event | no event; the item is `@test.trap` | 06: "A trap … appends no completed success" |
| deep structures | boundary | policy depth 64 through `restrict` plus a recipe-sized policy; ancestry of 100,000 `limit`s | typed errors, no overflow | architecture, risk 5 |
| test-world rows on a target | negative | every path by which `Context::issue` could be reached with a test root outside the runner | none | 06: "No target can perform one of them" |
| library and registry | positive | the 23 bound symbols of the library against the two snapshots | equal | 06 Registry versioning |

## Diagnostics and schema changes

None.

## Validation

```powershell
cargo test --locked --offline -p vibra-host -p vibra-interp -p vibra-types -p vibra-workspace --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
```

Read: both exit 0 on the unchanged tree.

## Stays unavailable

Unchanged.

## Excluded

Fixing anything; real drivers, which gate 34 reviews.

## Completion evidence

- [ ] `evidence/review-grants.md` lists every probe with its source,
      expected and observed result, and verdict.
- [ ] The public surface of `vibra-host` is listed with one line per item
      saying why it cannot mint or widen authority.
- [ ] Every finding has a class; the report states the commit reviewed.
