# Step 20: interp-host-dispatch

Kind: vertical. Slice 3, in the interpreter: grant values, dispatch of a
host-bound member through `HostContext`, the three narrowing rows, the
test-world rows, and the entry call. Package N6 of the native contract,
interpreter half.
Depends on: 06, 12.
Crates: `vibra-interp`.

## Prerequisites

- Read: [runtime chapter](../../../../spec/06-runtime.md#host-calls) "Host
  calls", "Cold plans and invocation identity", "Audit trace"
  (Invocations, Origins), "What has no encoding", "Traps"; [effects
  chapter](../../../../spec/03-effects.md#operation-interfaces) the last
  paragraph of "Operation interfaces"; [architecture](../architecture.md)
  "The interpreter" and "The completion receipt";
  [questions](../spec-questions.md) Q1, Q13, Q14.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-interp/src/machine.rs:1264` | `fn dispatch` | selects the host-bound member for a `Grant` receiver |
| `crates/vibra-interp/src/machine.rs:1614` | `fn intrinsic` | routes the three grant rows |
| `crates/vibra-interp/src/intrinsics.rs:581` | `pub(crate) fn apply` | where step 02 left them unavailable |
| `crates/vibra-interp/src/machine.rs:31` | `pub(crate) enum Halt` | gains the host-value trap |
| `crates/vibra-interp/src/value.rs:26` | `pub(crate) enum Repr` | gains `Grant` |
| `crates/vibra-interp/src/lib.rs:206` | `pub enum RuntimeError` | unchanged |

From step 03: `vibra_ir::authority`. From step 06: `plan.rs`,
`Kont::Await`, `Invocation`. Step 12 repairs the files of step 06 that
this step edits, so this step starts after it.

Proposed: `vibra-interp::host` (request building, reply decoding, entry
arguments); `run_entry`, `EntryArgument`.

## Files owned

- `crates/vibra-interp/src/host.rs`
- `crates/vibra-interp/src/machine.rs`
- `crates/vibra-interp/src/value.rs`
- `crates/vibra-interp/src/intrinsics.rs`
- `crates/vibra-interp/src/evaluate.rs`
- `crates/vibra-interp/src/encode.rs`
- `crates/vibra-interp/src/lib.rs`
- `crates/vibra-interp/tests/it/host.rs`
- `crates/vibra-interp/tests/it/main.rs`

## Ordered tasks

1. `Repr::Grant(GrantToken)`: copied freely, never encoded
   (`UnsupportedValue`), never compared.
2. An awaited `FunctionBody::Host(Operation(op))`: the receiver MUST be a
   `Grant`; build `HostArgument`s from the operands after it; fill
   `invocation` and `parent` from the current `Invocation`, whether the
   `await` was in tail position or not, the origin ordinal of the
   `await`, and `room`, the bytes the memory budget can still account;
   call `HostContext::host` once. Then, in this order: build the member's
   `Result` value from the reply, with the types of the member's
   signature, and account it; deliver it to the continuation; call
   `HostContext::commit` with the receipt, when there is one. Invariant:
   one call per invocation; `commit` is the last of the three and is
   reached only when the first two succeeded.
3. A reply whose shape does not fit the row's result, and a `HostFault`
   from `host` or from `commit`, are the trap
   `@runtime.invalidHostValue`; `HostFault::MemoryExhausted`, and a reply
   the budget cannot account, are the host event. In each case the
   receipt is dropped without `commit`.
4. Dispatch: a `Grant` receiver selects the interface's host-bound
   member; a held model selects its `impl`. A `Host` body reached with a
   receiver that is not a `Grant` is `@runtime.invalidCheckedProgram`.
5. `grant.restrict`, `grant.sub`, `grant.limit`: on a `Grant`, encode the
   policy, prefix, or limits and call `HostContext::narrow`. They are
   intrinsics: they take no invocation ordinal. A receiver that is not a
   `Grant` never reaches them: dispatch selects that type's own member,
   as for every member (Q1). An intrinsic reached with another receiver
   is `@runtime.invalidCheckedProgram`.
6. `FunctionBody::Host(TestWorld(op))`: call `HostContext::test_world`
   and build the member's result record from `Issued`.
7. `run_entry`: build the entry's arguments from `EntryArgument`s
   (`Absent` is `Option.none()`), give the entry's body the ordinal
   `first_invocation` with no parent, as Q13 rules, build the entry's
   plan and await it.
8. With no `HostContext`, a host body is `RuntimeError::Unavailable`, as
   before.

## Test matrix

Tests of `vibra-interp` over hand-built checked IR and a recording
`HostContext` defined in the tests, which answers from a table and logs
every request. `R` is an interface with one host-bound member bound to
`fs.readText`. The recording context logs `commit` too, with its position
among the requests.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| a host call | positive | `Await(Call(R.read, [grant, path]))`; the context answers `Ok(Text("hi"))` with a receipt | `Result.ok("hi")`; one request with `op: FsReadText`, `Path(["a.txt"])`; then one `commit` of that receipt | 06 Host calls, step 4 |
| commit follows the continuation | positive | the same; a test hook records when the continuation receives the value | the log is: request, value delivered, `commit` | 06 step 5: "Advance the continuation consistently with the completion, and then append the completed event" |
| the room of a request | boundary | a memory budget with 100 accountable bytes left at the `await` | the request has `room: 100` | 06: "Memory exhaustion stays a host event and never becomes an error case" |
| an error reply | positive | the context answers `Err(notFound)` | `Result.err(FsError.notFound)` as a value, no trap | 06 Error types: "An ordinary domain failure is never a trap" |
| each argument form | positive | one call per row kind: `Str`, `Bytes`, `RelativePath`, `U64` (random's `count` and standard input's `max`), none | `Text`, `Bytes`, `Path`, `Count`, empty | 06 Host registry, "Further parameters" |
| each result form | positive | replies `Void`, `Text`, `Bytes`, `Names`, `Metadata`, `I64`, `U64` | the typed value of the row | 06 Host registry, "Result of an invocation" |
| `ATK-broker-no-charge-without-await` | negative | build `Call(R.read, …)` 1,000 times and drop each plan | the context logged no request | 03 Cold plans: "Building a plan MUST NOT … dispatch a host action, or consume any limit" |
| `V2-RUNTIME-await-twice-two-events` | positive | one plan awaited twice | two requests, invocations 2 and 3, both with parent 1 | 03 Delivery: "Two `await`s of one plan are two invocations" |
| invocation and parent | positive | the body (invocation 1) awaits a procedure (2) that awaits a pure procedure (3) and then a host member (4) | the request has `invocation: 4`, `parent: 2` (Q13) | 06 Invocations: "whose parent is the invocation whose body performed it" |
| parent across a tail await | positive | the body (1) awaits `p` (2); `p` returns `await` of a host member in tail position (3); then the body awaits a host member (4) | requests `invocation: 3, parent: 2` and `invocation: 4, parent: 1` | the same |
| the first invocation of an entry | positive | `run_entry` with `first_invocation: 3`; the entry's body awaits a host member | the request has `invocation: 4`, `parent: 3` | 06 Invocations: "Every `await` is an invocation" |
| origin ordinal | positive | two awaits at different origins | the ordinals of the program's origin table | 06 Origins |
| model receiver | positive | a type implementing `R`, widened to the interface, awaited | the `impl` body's result; no request | 03: "Dispatch on any other held value … selects that type's member" |
| grant receiver through an interface value | positive | a `Grant` passed where `R` is expected, awaited in a callee | one request | 03: "dispatch on a sealed receiver selects them" |
| host body with a model receiver | negative | an unchecked set that calls the `Host` body directly with a record | the trap `@runtime.invalidCheckedProgram` | 06 Host registry: "a record with similar fields is not a grant" |
| reply of the wrong shape | negative | the context answers `Ok(U64(1))` with a receipt to `fs.readText` | the trap `@runtime.invalidHostValue`; the context logged no `commit` | 06 Traps: "a malformed trusted result" |
| fault from the context | negative | the context returns `HostFault::InvalidHostValue` | the trap `@runtime.invalidHostValue` (Q14) | 06 Host calls, step 1 |
| memory fault from the context | boundary | `HostFault::MemoryExhausted` | `RuntimeError::MemoryExhausted` | 06: "Memory exhaustion stays a host event" |
| `restrict` on a grant | positive | `grant.restrict(g, policy)`; the context answers `Grant(t2)` | `Result.ok` holding `t2`; a `Restrict` request with the policy's canonical value | 03 Narrowing |
| `restrict` refused | positive | the context answers `Refused(policyTooComplex)` | `Result.err(ScopeError.policyTooComplex)` | 03: "A larger one is `ScopeError.policyTooComplex`" |
| `limit` on a grant | positive | `grant.limit(g, {invocations: some(3) …})` | the returned token; `GrantLimits { invocations: Some(3), .. }` | 03 Narrowing |
| narrowing on a model | positive | a type whose `impl` of `R` supplies `limit` returning the receiver; `R.limit` on it through an interface value | the `impl` member's result; the context logged no `narrow` (Q1) | 03: "Dispatch on any other held value … selects that type's member" |
| a forwarding model narrows its grant | positive | a type holding a `Grant` whose `limit` calls `R.limit` on it; `R.limit` on the model | one `Limit` request for the held token; the result is a model holding the returned token (Q1) | the same |
| a narrowing intrinsic with a model receiver | negative | an unchecked set that calls `grant.limit` directly with a record | the trap `@runtime.invalidCheckedProgram` | 06 Host registry: "a record with similar fields is not a grant" |
| narrowing is no invocation | positive | `limit` then one host call | the call's invocation is the next ordinal | 06 Invocations: "Every `await` is an invocation" |
| test-world row | positive | `Await(Call(TestWorld.directory, [world], files: …))`; the context answers a `Bundle` of six tokens | the record of six grants by field name | 04 Test worlds, the table |
| test-world output | positive | `TestWorld.output`; the context answers `Output` | `{stdout: Bytes stderr: Bytes}` | the same |
| entry arguments | positive | `run_entry` with `[Grant(a), Absent, Record([("reader", Grant(b))])]` | the entry sees a grant, `Option.none()`, and a record | 03 Capability values, the three shapes |
| entry result | positive | an entry returning `ExitCode(3u8)` | `Execution` with that value | 06 Entry outcome |
| `ATK-encode-grant` | negative | `run` of a function returning a grant | `RuntimeError::UnsupportedValue` | 03: "A grant has no encoding" |
| grant captured in a closure | positive | a `Proc` closure capturing a grant, awaited after the frame returned | one request with that token | 06 Accounting: "A grant outlives the variable that introduced it" |
| `ATK-ambient-read-in-interp` | negative | the ambient-reads test over `crates/vibra-interp/src` | no ambient reference | 06 Host world: "Pure evaluation has no provider at all" |
| no context | boundary | a host body under `run` with no `HostContext` | `RuntimeError::Unavailable` | 07: `@tool.unavailable` |
| deep await chain to a host call | boundary | 100,000 nested procedures ending in a host call, on a 256 KiB thread | one request; no overflow | 07 Required coverage 13 |

## Diagnostics and schema changes

The trap `@runtime.invalidHostValue` becomes reachable. No schema change.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-interp -p vibra-types -p vibra-workspace -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0.

## Stays unavailable

No tool passes a `HostContext` yet: `workspaceTest` gains one in step 21,
a run in step 29.

## Excluded

The broker's behaviour (15); what a real context answers.

## Completion evidence

- [ ] Each of the 34 rows is a test, named in the report.
- [ ] `HostContext::commit` is called from one place, after the
      continuation received the value: the report quotes the function.
- [ ] `crates/vibra-interp/Cargo.toml` has no `vibra-host` dependency,
      development dependencies included.
- [ ] `Repr::Grant` is constructed in exactly two functions of `host.rs`,
      quoted in the report.
- [ ] The validation commands exit 0.
