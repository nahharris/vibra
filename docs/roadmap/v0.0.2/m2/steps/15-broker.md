# Step 15: broker

Kind: vertical. Slice 3, the authority core: the grant table, narrowing,
shared counters, and the five steps of `Context::host`, against the
`Driver` trait and a scripted driver of the tests. Package N6 of the native
contract, broker half.
Depends on: 03, 14.
Crates: `vibra-host`, `vibra-conformance`.

## Prerequisites

- Read: [runtime chapter](../../../../spec/06-runtime.md#host-calls) "Host
  calls" whole, "Error types"; [effects
  chapter](../../../../spec/03-effects.md#capability-values) "Capability
  values", "Narrowing", "Limits", "Delivery and typed outcomes";
  [architecture](../architecture.md) "The host", rows A5 to A14;
  [questions](../spec-questions.md) Q1, Q7, Q8, Q10, Q11, Q12, Q14.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-ir/src/external/host.rs:377` | `pub fn host_row` | the operation's signature and error type |
| `crates/vibra-conformance/tests/it/ambient_reads.rs:19` | `PROVIDER_CRATES` | the ambient-reads test, narrowed to `vibra-host/src/system/` |

From step 03: `vibra_ir::authority::{HostContext, HostRequest, HostReply,
NarrowRequest, Narrowed, GrantToken, HostFault}`. From step 14:
`policy::permits`, `recipe::Limits`.

Proposed: `vibra-host::broker::{Context, RootGrant, Ancestry, Counters}`;
`vibra-host::driver::{Driver, Gate, Found, Completion, Acquire, Action,
ResourceId, DriverIdentity}`.

## Files owned

- `crates/vibra-host/src/broker.rs`
- `crates/vibra-host/src/driver.rs`
- `crates/vibra-host/tests/it/broker.rs`
- `crates/vibra-host/tests/it/scripted.rs`
- `crates/vibra-conformance/tests/it/ambient_reads.rs`

## Ordered tasks

1. The grant table: a slot holds the interface, the resource, the
   ancestry (a flat list of constraints: policy, prefix, limits, each
   with its counters), the origin record, the root ordinal, and a
   generation. A token is `{context, slot, generation}`; a context's
   identity is unique per `Context::new`. Invariant: a token is an index;
   every use validates it.
2. Step 1 of a call: the token's context, slot, and generation, and that
   the slot's interface is the owner of `request.op`. A failure is
   `HostFault::InvalidHostValue` (Q14).
3. Step 2: derive the `Action` from the operands: the names and the
   action set the operation may turn out to perform (a write: create or
   replace). For `env.readVar`, the name against the grant's `names`:
   outside it is `denied` (Q12).
4. Step 3: for every constraint of the ancestry, root first, evaluate the
   policy on the request in root coordinates (Q7); then reserve one
   invocation and the known output bytes on every counter of the
   ancestry, all or none. A counter that would overflow denies.
   `denied`, `limitExceeded` here return `Completion`-free: no driver
   call and no event.
5. Step 4: `Driver::call` with a `Gate` that evaluates the same policy
   on `Found::real_names` and `Found::action`, applies the hard-link
   rule, and reserves `Found::size` input bytes (a failure there is
   `limitExceeded` before content is read). A gate refusal is a completed
   `denied`.
6. Step 5: charge actual progress; refund only a reservation known
   unused; `outcomeUnknown` refunds nothing. Return the reply; the event
   hook of step 17 is called after it.
7. `narrow`: `Restrict` decodes the source policy value, counts it with
   the ancestry (`policyTooComplex`), rejects only an empty suffix as
   `invalidPolicy` (Q8), and appends a constraint; `Sub` appends a prefix
   (Q7); `Limit` appends counters holding the smaller of each bound
   (Q11). Each returns a new slot with the parent's resource, origin,
   and root. Invariant: no operation edits or removes a constraint.
8. `release(resource)`: bumps the generation of every slot of that
   resource.
9. The ambient-reads test allows `std::fs`, `std::env`, `std::io::stdin`,
   `stdout`, `stderr`, and `std::time` only under
   `crates/vibra-host/src/system/`.

## Test matrix

Host tests over a scripted driver that records each `call` and answers
from a table. `root(I)` issues a root grant of interface `I` with policy
`P0` = file rule `prefix: [] depth: @tree actions: [@read @replace
@create] names: anyName`.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| a permitted read | positive | `fs.readText` of `["a.txt"]` on `root(DirReadText)`; the driver finds 5 bytes | `Ok("hello")`; one driver call; counters: 1 invocation, 5 input bytes | 06 Host calls, steps 1 to 5 |
| `ATK-forged-token` | negative | a token with a slot the context never issued | `InvalidHostValue`; no driver call | 06 step 1: "Validate the grant" |
| `ATK-foreign-grant` | negative | a token of another `Context` | `InvalidHostValue` | 03: "A grant from another world … is unusable" |
| `ATK-stale-grant` | negative | a token used after `release` of its resource | `InvalidHostValue` | 03: "or one released at shutdown, is unusable" |
| wrong operation for the grant | negative | `fs.writeText` with a `DirReadText` token | `InvalidHostValue` | 06 step 1: "the identity of the registered operation" |
| denied before the adapter | negative | policy `prefix: ["in"]`; read `["out" "a.txt"]` | `Err(denied)`; no driver call; no charge | 06 step 3; 03 `denied`: "The policy may refuse before the adapter runs" |
| `ATK-policy-ancestor-dropped` | negative | parent `suffix(".txt")`, child `restrict` to `anyName`; read `a.bin` through the child | `Err(denied)` | 03 Meaning: "none may drop an ancestor's constraint because it believes the child is narrower" |
| `ATK-narrow-broader-policy` | negative | parent `prefix: ["in"]`, `restrict` with `prefix: []`; read `["out" "a"]` | `Err(denied)` | 03 Narrowing: "A policy broader than the parent's changes nothing" |
| `ATK-narrow-other-resource` | negative | two roots A and B; a `restrict` of A; the driver records the `ResourceId` of each call | every call through the child names A's resource | 06 Accounting: "Narrowing MUST NOT add a root, an operation, a resource extent" |
| the parent stays valid | positive | `restrict` a root, then read through the root | `Ok` | 03 Narrowing: "The parent stays valid" |
| `sub` rebases | positive | `sub(["reports"])`, read `["a.txt"]` | the driver is asked for `["reports" "a.txt"]` (Q7) | 03 Narrowing: "`sub` adds a path prefix below the same held directory" |
| `sub` of a missing directory | positive | `sub(["nope"])` | `Narrowed::Grant` | 03: "It does not check that the directory exists" |
| `restrict`: empty suffix | negative | a source policy with `suffix("")` | `Refused(invalidPolicy)` | 03 Names: "An empty suffix is invalid" |
| `restrict`: too complex | negative | an ancestry at depth 64 and one more `restrict` | `Refused(policyTooComplex)` | 03: "counting ancestors acquired by restriction" |
| `limit` keeps the smaller | positive | `limit(invocations: 5)` then `limit(invocations: 9)`; six calls | the sixth is `limitExceeded` | 03 Narrowing: "`limit` keeps the smaller of each bound" |
| `limit` with `none` | positive | `limit(invocations: none)` on a grant bounded at 2; three calls | the third is `limitExceeded` | 03: "`Option.none()` leaves that bound as it was" |
| siblings share the parent | negative | parent bounded at 3 invocations; two children; two calls each | the fourth call is `limitExceeded` | 03 Limits: "every copy and descendant of a grant draws on the same ancestor counters" |
| a child's own limit | negative | parent unbounded, child bounded at 1; two calls through the child, then one through the parent | second `limitExceeded`; the parent's call `Ok` | 06 Accounting: "its own tighter limits apply in addition" |
| `limitExceeded` dispatches nothing | negative | a grant at its invocation bound | `Err(limitExceeded)`; no driver call | 03: "A limit would be exceeded; nothing was attempted" |
| read over the byte limit | negative | `inputBytes: 4`; the driver finds 5 bytes | `Err(limitExceeded)`; the scripted driver's `read` step is not reached | 06: "a size over the remaining limit is `limitExceeded` before any content is read" |
| read at the byte limit | boundary | `inputBytes: 5`, 5 bytes | `Ok`; a second read of 1 byte is `limitExceeded` | the same |
| write reserves its size | negative | `outputBytes: 3`, write of `"hello"` | `Err(limitExceeded)`; no driver call | 06: "A write reserves the encoded size of its argument before it sends" |
| metadata counts an invocation only | positive | `inputBytes: 0`; `fs.metadataStat` | `Ok` | 03 Limits: "Metadata results … count as an invocation only" |
| `ATK-counter-overflow` | negative | counters preset to `u64::MAX - 1`, a 5-byte read | `Err(denied)`; counters unchanged | 03 Limits: "A counter that would overflow denies" |
| `ATK-counter-refund-unknown` | negative | `outputBytes: 10`; the driver answers `outcomeUnknown` to a write of 6 bytes; then a write of 6 | the second is `limitExceeded` | 06 Accounting: "A consumption that might have happened MUST NOT be refunded" |
| refund when known unused | positive | the driver answers `notFound` to a read whose size was never learned; then a read of 5 under `inputBytes: 5` | `Ok` | 06: "released or refunded only when the capacity is known to be unused" |
| gate: real name fails | negative | policy `suffix(".txt")`; request `a.txt`; the driver reports the real name `A.TXT` | `Err(denied)`, completed | 06 step 4: "the name test against the name the filesystem reports" |
| gate: found action | negative | policy `actions: [@create]`; a write; the driver finds an existing file | `Err(denied)`, completed | 06 step 4: "the right for the action it found (create or replace)" |
| gate: a write either way | positive | policy `[@create @replace]`; a write | `Ok` for a new and for an existing file | 06 step 3: "permits at least one of the actions the operation may turn out to perform" |
| write with neither action | negative | policy `[@read]`; a write | `Err(denied)`; no driver call | the same |
| gate: hard link | negative | a replace; the driver reports `links: 2` | `Err(denied)`, completed | 03 Hard links: "MUST be refused with `denied`" |
| read of a hard link | positive | a read; `links: 2` | `Ok` | 03: "Reading such a file is allowed" |
| env name inside the set | positive | `names: ["HOME"]`; `env.readVar("HOME")` | the driver's value | 06 Environment: "reads one permitted name" |
| env name outside the set | negative | `env.readVar("PATH")` | `Err(denied)`; no driver call (Q12) | 06: "Neither grant exposes anything outside its reviewed scope" |
| stdin within `inputBytes` | boundary | `inputBytes: 3`; stdin holds `abc` | `Ok("abc")` (Q10) | 06: "A read first learns the size of the opened object, reserves it" |
| stdin over `inputBytes` | negative | `inputBytes: 2`; stdin holds `abc` | `Err(limitExceeded)`; a later unbounded read returns `abc` (Q10) | 06 Standard input: "A rejection before dispatch … consumes nothing" |
| deep ancestry | boundary | 10,000 chained `limit` calls, then a read, on a 256 KiB thread | `Ok`, no overflow | architecture, risk 5 |
| counters against a model | positive | 1,000 random trees of `limit`, `restrict`, `sub` with random calls, compared with a direct model | equal results and counters | 06 Accounting |
| `ATK-ambient-read-outside-system` | negative | the ambient-reads test given a file of `vibra-host/src/broker.rs` containing `std::fs::read` | the test reports it | 03 Exclusions: "ambient authority" |

## Diagnostics and schema changes

None. `HostFault::InvalidHostValue` reaches a user as
`@runtime.invalidHostValue` in step 20.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-host -p vibra-workspace -p vibra-conformance --all-features
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0. The model row prints its seed; a failure is reproduced
with that seed.

## Stays unavailable

Everything observable: no interpreter calls the broker until step 20.

## Excluded

The fake world (16); events (17); startup (28); real drivers (30, 31).

## Completion evidence

- [ ] Each of the 39 rows is a test, named in the report.
- [ ] `Context::issue` is `pub(crate)`; `git grep -n "fn issue"
      crates/vibra-host/src` shows one definition.
- [ ] No function of `broker.rs` caches a decision between calls: the
      report quotes the fields of the slot type.
- [ ] The validation commands exit 0.
