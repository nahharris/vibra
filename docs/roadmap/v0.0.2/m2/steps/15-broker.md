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
  [architecture](../architecture.md) "The host" whole (lineages, the
  gate, settlement, the order of a mutating call), "The completion
  receipt", rows A5 to A14, A23, A25, A30;
  [questions](../spec-questions.md) Q7, Q8, Q9, Q10, Q11, Q12, Q14, Q28, Q29,
  Q31.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-ir/src/external/host.rs:377` | `pub fn host_row` | the operation's signature and error type |
| `crates/vibra-conformance/tests/it/ambient_reads.rs:19` | `PROVIDER_CRATES` | the ambient-reads test, narrowed to `vibra-host/src/system/` |

From step 03: `vibra_ir::authority::{HostContext, HostRequest, HostReply,
HostCompletion, Receipt, NarrowRequest, Narrowed, GrantToken, HostFault}`.
From step 14: `policy::permits`, `recipe::Limits`.

Proposed: `vibra-host::broker::{Context, RootGrant, Lineage, Counters}`;
`vibra-host::driver::{Driver, Gate, Refused, Found, Completion, Progress,
Measured, Acquire, Action, ResourceId, DriverIdentity}`.

## Files owned

- `crates/vibra-host/src/broker.rs`
- `crates/vibra-host/src/driver.rs`
- `crates/vibra-host/tests/it/broker.rs`
- `crates/vibra-host/tests/it/scripted.rs`
- `crates/vibra-conformance/tests/it/ambient_reads.rs`

## Ordered tasks

1. The grant table: a slot holds the interface, its parent slot (none
   for a root), the one constraint it adds (a policy, a prefix, or limits
   with their counters), and its lineage. A lineage holds the resource,
   the origin record, the root ordinal, a generation, and whether it is
   live. A token is `{context, slot, generation}`, with the generation of
   its lineage at issue; a context's identity is unique per
   `Context::new`. `issue` returns a `HostFault` when the table cannot
   grow. Invariant: a token is an index; every use validates it; a slot
   has no liveness of its own.
2. Step 1 of a call, and of every `narrow`: the token's context and
   slot; the slot's lineage is live and has the token's generation; the
   slot's interface is the owner of `request.op`; no event is pending. A
   failure is `HostFault::InvalidHostValue` (Q14).
3. Step 2: derive the `Action` from the operands: the names and the
   action set the operation may turn out to perform (a write: create or
   replace). For `env.readVar`, the name against the grant's `names`:
   outside it is `denied` (Q12).
4. Step 3: for every constraint from the slot to its root, by a loop over
   parent slots, evaluate the policy on the request in root coordinates
   (Q7). Then reserve, on every counter of the ancestry, all or none: one
   invocation, and the bytes an operand fixes (the encoded size of a
   write; the `count` of `random.generateBytes`). A counter that would
   overflow denies. A `count` above `request.room` is
   `HostFault::MemoryExhausted`. A `denied` or `limitExceeded` here
   returns with no driver call, no reservation left, and no receipt.
5. Step 4: `Driver::call` with a `Gate`. The gate, in this order:
   evaluates the same policy on `Found::real_names` and `Found::action`;
   applies the hard-link rule, where a replace with `links: None` is
   `unsupported`; reserves `Found::size` input bytes on every counter of
   the ancestry, where a size over a bound is `limitExceeded` and a size
   above `request.room` is `HostFault::MemoryExhausted`. For a stream
   read the reserved bytes are the smaller of the request's `max` and
   `Found::size`; when the size is `None`, the smaller of `max` and what
   remains of the `inputBytes` bound on the slot and on every ancestor,
   which is `limitExceeded` only when `max` is positive and nothing
   remains, or for a text read reserved below 4 bytes (Q10, Q31). The
   gate returns the reserved count as `Reserved`, and the driver reads at
   most that many bytes. The broker keeps the gate's reason. Invariant:
   the gate has answered before the driver reads, creates, or changes
   anything, and a driver whose gate refused returns
   `Completion::Refused`; any other completion after a refusal is
   `InvalidHostValue`.
6. Map a gate refusal: a policy or hard-link refusal is the completed
   reply `denied`, with a receipt; `limitExceeded` and `unsupported` have
   no receipt, and a size refusal releases the invocation too (Q29).
7. Settle every byte reservation, on the slot's counters and on every
   ancestor's, from the completion's `Progress`, by the table of the
   architecture: `Known(n)` charges `n` and refunds the rest; `Uncertain`
   and a reply `outcomeUnknown` charge the whole; `Refused` and
   `NotSubmitted` refund the whole. A `Known(n)` above the reservation is
   `InvalidHostValue` and charges the whole. Invariant: the reply is
   never read to decide a charge, except `outcomeUnknown`.
8. For a `Completed`, keep the event pending under a new serial and
   return the reply with its `Receipt`. `commit` checks that the receipt
   is the pending one, else `InvalidHostValue`, and clears it; step 17
   makes it append. Invariant: nothing is appended inside `host`.
9. `narrow`: `Restrict` decodes the source policy value, counts it with
   the ancestry (`policyTooComplex`), rejects only an empty suffix as
   `invalidPolicy` (Q8), and adds a slot with that constraint; `Sub` adds
   a prefix (Q7), counted with the ancestry as one more level of depth,
   so its only refusal is `policyTooComplex` (Q8, Q9); `Limit` adds
   counters holding the smaller of each bound
   (Q11). Each new slot has the parent's lineage. Invariant: no operation
   edits or removes a constraint, and none makes a lineage.
10. `release(resource)`: marks every lineage on that resource dead and
    bumps its generation, and only then calls `Driver::release`.
11. The ambient-reads test allows `std::fs`, `std::env`,
    `std::io::stdin`, `stdout`, `stderr`, and `std::time` only under
    `crates/vibra-host/src/system/`.

## Test matrix

Host tests over a scripted driver that records each `call`, each gate
answer, and each of its own steps (`read`, `create`, `truncate`,
`write`), and answers from a table with a `Progress` per completion.
`root(I)` issues a root grant of interface `I` with policy `P0` = file
rule `prefix: [] depth: @tree actions: [@read @replace @create] names:
anyName`. "Completed" means the call returned a receipt; "no receipt"
means no event is due.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| a permitted read | positive | `fs.readText` of `["a.txt"]` on `root(DirReadText)`; the driver finds 5 bytes and completes with `input: Known(5)` | `Ok("hello")` with a receipt; one driver call; counters: 1 invocation, 5 input bytes | 06 Host calls, steps 1 to 5 |
| `ATK-forged-token` | negative | a token with a slot the context never issued | `InvalidHostValue`; no driver call | 06 step 1: "Validate the grant" |
| `ATK-foreign-grant` | negative | a token of another `Context` | `InvalidHostValue` | 03: "A grant from another world … is unusable" |
| `ATK-stale-grant` | negative | a token used after `release` of its resource | `InvalidHostValue` | 03: "or one released at shutdown, is unusable" |
| `ATK-stale-derived-grant` | negative | `restrict` of `sub` of `limit` of a root, the token kept; `release` of the root's resource; then a call and a `narrow` through the kept token | `InvalidHostValue` both times; no driver call; the slot of the kept token was never written by `release` | 03: "or one released at shutdown, is unusable" |
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
| `sub`: too complex | negative | an ancestry at depth 64 and one more `sub` | `Refused(policyTooComplex)` (Q9) | 03: "plus one for each `restrict` and each `sub` in the grant's ancestry" |
| `limit` keeps the smaller | positive | `limit(invocations: 5)` then `limit(invocations: 9)`; six calls | the sixth is `limitExceeded` | 03 Narrowing: "`limit` keeps the smaller of each bound" |
| `limit` with `none` | positive | `limit(invocations: none)` on a grant bounded at 2; three calls | the third is `limitExceeded` | 03: "`Option.none()` leaves that bound as it was" |
| siblings share the parent | negative | parent bounded at 3 invocations; two children; two calls each | the fourth call is `limitExceeded` | 03 Limits: "every copy and descendant of a grant draws on the same ancestor counters" |
| a child's own limit | negative | parent unbounded, child bounded at 1; two calls through the child, then one through the parent | second `limitExceeded`; the parent's call `Ok` | 06 Accounting: "its own tighter limits apply in addition" |
| `limitExceeded` dispatches nothing | negative | a grant at its invocation bound | `Err(limitExceeded)`; no driver call; no receipt | 03: "A limit would be exceeded; nothing was attempted" |
| read over the byte limit | negative | `inputBytes: 4`; the driver finds 5 bytes | `Err(limitExceeded)`; the driver returns `Refused` and its `read` step is not reached; no receipt (Q29) | 06: "a size over the remaining limit is `limitExceeded` before any content is read" |
| `ATK-counter-size-refusal-holds-no-invocation` | negative | `invocations: 1`, `inputBytes: 4`; a read where the driver finds 5 bytes, then one where it finds 4 | `Err(limitExceeded)`, then `Ok` (Q29) | 03 Limits: "An invocation is every call that passed reservation" |
| result above the room | boundary | `room: 4` in the request; the driver finds 5 bytes | `HostFault::MemoryExhausted`; the driver's `read` step is not reached | 06: "Memory exhaustion stays a host event and never becomes an error case" |
| read at the byte limit | boundary | `inputBytes: 5`, 5 bytes | `Ok`; a second read of 1 byte is `limitExceeded` | the same |
| write reserves its size | negative | `outputBytes: 3`, write of `"hello"` | `Err(limitExceeded)`; no driver call | 06: "A write reserves the encoded size of its argument before it sends" |
| metadata counts an invocation only | positive | `inputBytes: 0`; `fs.metadataStat` | `Ok` | 03 Limits: "Metadata results … count as an invocation only" |
| `ATK-counter-overflow` | negative | counters preset to `u64::MAX - 1`, a 5-byte read | `Err(denied)`; counters unchanged | 03 Limits: "A counter that would overflow denies" |
| `ATK-counter-refund-unknown` | negative | `outputBytes: 10`; the driver completes a write of 6 bytes with the reply `outcomeUnknown` and `output: Known(0)`; then a write of 6 | the second is `limitExceeded`: the reply forbids the refund | 06 Accounting: "A consumption that might have happened MUST NOT be refunded" |
| `ATK-counter-partial-known` | positive | `outputBytes: 10`; the driver completes a write of 6 with `Err(other)` and `output: Known(2)`; then a write of 8; then a write of 1 | `Err(other)`, `Ok`, `Err(limitExceeded)`: 2 were charged and 4 refunded | 03 Limits: "charges actual progress" |
| `ATK-counter-partial-unknown` | negative | the same bound, the same write, the same reply `Err(other)`, with `output: Uncertain`; then a write of 5; then a write of 4 | `Err(other)`, `Err(limitExceeded)`, `Ok`: all 6 stay charged | 06 Accounting: "A reservation is released or refunded only when the capacity is known to be unused" |
| settlement reaches every ancestor | positive | parent `outputBytes: 10`; child `limit(outputBytes: 8)`; through the child, the write of 6 completed with `Known(2)`; then a write of 8 through the parent | `Ok`: the parent's counter held 2, as the child's did | 03 Limits: "every copy and descendant of a grant draws on the same ancestor counters" |
| progress above the reservation | negative | a write of 6 completed with `output: Known(7)` | `InvalidHostValue`; 6 stay charged | 06 Traps: "a malformed trusted result" |
| `ATK-counter-invalid-text-charged` | negative | `inputBytes: 5`; a text read the driver completes with `Err(invalidText)` and `input: Known(5)`; then a read of 1 byte | `Err(invalidText)`, then `Err(limitExceeded)` (Q28) | 03 Limits: "charges actual progress" |
| `ATK-counter-reservation-all-or-none` | negative | `invocations: 2`, `outputBytes: 3`; a write of `hello`; then writes of `ab` and `c` | `Err(limitExceeded)`, then `Ok`, `Ok`: the refused call took no invocation | 06 step 3: "Reserve atomically the shared count limits and the byte limits the operands decide" |
| refund when known unused | positive | `inputBytes: 5`; the driver answers `notFound` with `input: Known(0)` to a read whose size was never learned; then a read of 5 | `Ok` | 06: "released or refunded only when the capacity is known to be unused" |
| gate: real name fails | negative | policy `suffix(".txt")`; request `a.txt`; the driver reports the real name `A.TXT` | `Err(denied)`, completed; the driver returns `Refused` and its `read` step is not reached | 06 step 4: "the name test against the name the filesystem reports" |
| `ATK-driver-ignores-gate` | negative | the same request; the scripted driver returns `Completed` with `Ok("x")` after its gate refused | `InvalidHostValue`; no receipt | 03 `denied`: "no file content was read, created, or changed" |
| `ATK-gate-create-on-real-parent` | negative | policy `prefix: ["OUT"]`, `actions: [@create]`; a write of `["OUT" "n.txt"]`; the driver's gate call reports `Create` with the real names `["out" "n.txt"]` | `Err(denied)`, completed; the driver's `create` step is not reached | 03: "Never truncate or create before authorization" |
| gate: found action | negative | policy `actions: [@create]`; a write; the driver finds an existing file | `Err(denied)`, completed | 06 step 4: "the right for the action it found (create or replace)" |
| gate: a write either way | positive | policy `[@create @replace]`; a write | `Ok` for a new and for an existing file | 06 step 3: "permits at least one of the actions the operation may turn out to perform" |
| write with neither action | negative | policy `[@read]`; a write | `Err(denied)`; no driver call | the same |
| gate: hard link | negative | a replace; the driver reports `links: Some(2)` | `Err(denied)`, completed; the driver's `truncate` step is not reached | 03 Hard links: "MUST be refused with `denied`" |
| `ATK-gate-replace-without-link-count` | negative | a replace; the driver reports `links: None` | `Err(unsupported)`; the driver's `truncate` step is not reached; no receipt (Q29) | 03: "There is no unchecked fallback" |
| read of a hard link | positive | a read; `links: Some(2)` | `Ok` | 03: "Reading such a file is allowed" |
| env name inside the set | positive | `names: ["HOME"]`; `env.readVar("HOME")` | the driver's value | 06 Environment: "reads one permitted name" |
| env name outside the set | negative | `env.readVar("PATH")` | `Err(denied)`; no driver call (Q12) | 06: "Neither grant exposes anything outside its reviewed scope" |
| stdin, known size, within | boundary | `inputBytes: 3`; `io.stdinBytes` with `max: 3`; the driver reports `size: Some(3)` and holds `abc` | `Ok` of the bytes `abc`; 3 charged | 06 Standard input: "the reservation is the smaller of `max` and that size" |
| stdin, known size, over | negative | `inputBytes: 2`; `max: 3`; `size: Some(3)` | `Err(limitExceeded)`; the driver's `read` step is not reached; no receipt; a later unbounded read returns `abc` | 06 Standard input: "A rejection before the stream is read" |
| stdin, known size below the count | boundary | `inputBytes: 3`; `max: 10`; `size: Some(3)` | `Ok` of `abc`: the smaller of the two was reserved (Q10) | 06 Standard input: "the reservation is the smaller of `max` and that size" |
| stdin, known size, empty at zero | boundary | `inputBytes: 0`; `max: 1`; `size: Some(0)` | `Ok` of no bytes (Q10) | the same |
| stdin, unsized, within the count | boundary | `inputBytes: 10`; `max: 10`; `size: None`; the driver holds `abc` and completes with `input: Known(3)` | `Ok` of `abc`; 10 reserved at the gate, 3 charged, 7 refunded: a later read with `max: 7` is `Ok` | 06: "A limit MUST be reserved before an operation could exceed its count or byte bound" |
| stdin, unsized, a short count | boundary | `inputBytes: 2`; `max: 2`; `size: None`; the driver holds `abc` | `Ok` of `ab`; 2 charged; the driver still holds `c` (Q10) | the same |
| stdin, unsized, a count above the bound | boundary | `inputBytes: 2`; `max: 3`; `size: None`; the driver holds `abc` | `Ok` of `ab`: the gate reserved 2, the smaller of `max` and the remaining bound; 2 charged; the driver still holds `c` (Q10) | 06 Standard input: "the reservation is the smaller of `max` and what remains of the `inputBytes` bound of the grant and of every ancestor" |
| stdin, unsized, nothing remains | negative | the grant of the row above after its read; `max: 3` | `Err(limitExceeded)`; the driver's `read` step is not reached; no receipt; a later read through an unbounded grant on the same resource returns `c` (Q10) | 06 Standard input: "The call is `limitExceeded` before the stream is read only when `max` is positive and nothing remains" |
| stdin, unsized, a remaining bound below the count | boundary | a parent with `inputBytes: 5` that has 3 charged; a child of it with no bound of its own; through the child, `max: 10`; `size: None`; the driver holds `abcd` | `Ok` of `ab`: the gate reserved 2, what remains of the parent's bound (Q10) | the same |
| stdin text, unsized, below one scalar | negative | `inputBytes: 2`; `io.stdinText` with `max: 16`; `size: None`; the driver holds `abc` | `Err(limitExceeded)`; the driver's `read` step is not reached; no receipt; nothing charged (Q31) | 06 Standard input: "A text read of unknown size whose `max` is at least 4 and whose reservation is below 4 is `limitExceeded` before the stream is read" |
| deep ancestry | boundary | 10,000 chained `limit` calls, then a read, on a 256 KiB thread | `Ok`, no overflow | architecture, risk 5 |
| a call while an event is pending | negative | a completed call whose receipt is not committed, then a second call | `InvalidHostValue` for the second | 06 step 5: "Advance the continuation consistently with the completion, and then append the completed event" |
| a receipt of another context | negative | `commit` with a receipt a second `Context` returned | `InvalidHostValue` | 06 Traps: "a boundary value that is invalid, stale, foreign, or of the wrong kind" |
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

- [ ] Each of the 62 rows is a test, named in the report.
- [ ] `Context::issue` is `pub(crate)`; `git grep -n "fn issue"
      crates/vibra-host/src` shows one definition.
- [ ] No function of `broker.rs` caches a decision between calls, and a
      slot has no liveness field: the report quotes the fields of the
      slot type and of the lineage type.
- [ ] `release` writes the lineage before it calls the driver, and
      `host` appends nothing: the report quotes both functions.
- [ ] The settlement function takes a `Progress` and no `HostReply`
      other than to test for `outcomeUnknown`: the report quotes its
      signature.
- [ ] The validation commands exit 0.
