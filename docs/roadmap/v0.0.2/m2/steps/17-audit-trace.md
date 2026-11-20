# Step 17: audit-trace

Kind: vertical. Slice 6, the record: `@auditTrace.v2` with startup and host
events, grant projections, the policy table, and its closed decoder.
Depends on: 15.
Crates: `vibra-host`.

## Prerequisites

- Read: [runtime chapter](../../../../spec/06-runtime.md#audit-trace)
  "Audit trace" whole; "Canonical value encoding" for `args` and `result`;
  [questions](../spec-questions.md) Q7 (the projected policy), Q11 (the
  projected limits), Q13, Q24.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-ir/src/value.rs:18` | `pub enum Value` | the canonical data of arguments and results |
| `crates/vibra-ir/src/origin.rs:12` | `pub struct SourceOrigin` | origins; the ordinal comes in the request |

From step 15: `broker::Context`, its step 5.

Proposed: `vibra-host::trace::{AuditTrace, Event, StartupEvent, HostEvent,
GrantProjection, OriginRecord, TRACE_FORMAT, encode, decode}`.

## Files owned

- `crates/vibra-host/src/trace.rs`
- `crates/vibra-host/src/broker.rs`
- `crates/vibra-host/tests/it/trace.rs`

## Ordered tasks

1. The types and their canonical encoding, fields in the chapter's order;
   optional fields absent, never empty.
2. `Context` appends one `@host` event in step 5, after the reply is
   built: `invocation`, `parent`, `phase`, `op`, `driver`, `origin`, the
   projection, `args` (the canonical values after the grant), `result`
   (the complete `Result` as canonical data, with the library's seed).
   Invariant: a refusal of steps 1 to 3 appends nothing; a completed
   `denied` and an `outcomeUnknown` are appended.
3. The projection of a slot: `issuedBy` of its root; `policy` the hash of
   the normalized effective policy, or the empty string; `rights` the one
   interface; `limits` the bounds that are set, as Q11 recommends; `root`
   the root ordinal. Each projected policy is added to `policies`.
4. `drivers`: every driver identity bound in the run, sorted by provider.
5. `Context::startup_event(leaf, provider, status, grant, failure)` for
   step 28.
6. `decode`: the closed schema; an unknown event kind is kept as an
   opaque record. Decoding returns data only.
7. The JSON projection of an event, each field its own member, values as
   canonical text.

## Test matrix

Host tests over the scripted driver of step 15 and the `WorldDriver` of
step 16 where a row says so.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| one completed call | positive | `io.stdoutText "hello\n"` on a root with origin `{kind: @binding leaf: @out}`, invocation 2, parent 1, origin 1 | exactly the `@host` event of the chapter's example, field for field | 06 Audit trace, the example |
| fields in order | positive | the encoded event | `kind, invocation, parent, phase, op, driver, origin, grant, args, result` | 06, the host event table |
| absent `parent` | positive | a startup read: phase `@startup`, no parent | the field is absent | 06: "absent for a startup read" |
| denial before the adapter | negative | a policy refusal | `events` is empty | 06: "A refusal before the adapter is called … appends nothing" |
| `limitExceeded` | negative | a grant at its bound | `events` is empty | the same |
| completed denial | positive | a gate refusal on the real name | one event whose `result` is `Err(denied)` | 06: "A typed denial returned by the adapter is a completed result and is recorded" |
| `outcomeUnknown` | positive | the scripted driver answers `outcomeUnknown` | one event with that result | 06: "That typed result is recorded" |
| order | positive | three calls on two grants | three events in completion order | 06: "in completion order" |
| projection without a policy | positive | a stream grant | `policy: ""`, no `limits` field | 06: "or the empty string for a provider without a policy" |
| projection with a policy | positive | a directory grant with policy `P` | `policy` is `hash(normalize(P))`; `policies` has that key with the normal form | 06: "and a key of `policies`" |
| projection after `restrict` | positive | the same grant restricted by `Q` | the hash of the effective policy of Q7; both documents are in `policies` when both are projected | 06: "every policy a grant projection refers to" |
| projection after `limit` | positive | `limit(invocations: 5)` | `limits: {invocations: 5u64}` only | 06: "a record of the bounds that are set" |
| narrowing keeps the origin | positive | `restrict` and `sub` of a `@testWorld` root | `issuedBy: {kind: @testWorld}`, the same `root` | 06: "Narrowing keeps the origin record of the grant it narrows" |
| two roots | positive | two root grants | `root: 1u64` and `root: 2u64` (Q13) | 06: "a run-local ordinal of the root grant" |
| drivers sorted | positive | drivers `@host.io.stdout` and `@host.fs.directory` | `drivers` lists `fs` first | 06: "sorted by `provider`" |
| fake driver identity | positive | a `WorldDriver` | `{provider: @hostWorld.v2 version: "0.0.2"}` | 06: "`provider` is `@hostWorld.v2` for a fake world's driver" |
| startup event, acquired | positive | `startup_event(@out, @host.io.stdout, acquired, grant, none)` | the `@startup` event of the example | 06, the startup event table |
| startup event, refused optional | positive | status `@refused`, no failure | no `grant`, no `failure` | 06: "A refused optional leaf … has none" |
| startup event, failed | positive | status `@failed`, code `@grant.acquisitionFailed` | `failure: {code message}` | 06 |
| round trip | positive | a trace with both kinds | `decode(encode(t))` equals `t` | 06: "Its persistent format is `@auditTrace.v2`" |
| unknown field | negative | a `@host` event with `extra` | rejected | 06: "Both are closed records" |
| unknown kind | boundary | an event of kind `@attempt` | kept as an opaque record and re-encoded with fields in name order | 06: "A consumer MUST keep an unknown kind as an opaque record" |
| `ATK-trace-decode-grants-nothing` | negative | decode a trace, then build a token from every integer in it and call `Context::host` on a new context | `InvalidHostValue` each time | 06: "decoding one grants nothing" |
| JSON projection | positive | the example event | each field a JSON member; `args` items canonical text in strings | 06: "A whole event as one string is forbidden" |

## Diagnostics and schema changes

Schema: `@auditTrace.v2`, new. No diagnostic.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-host -p vibra-workspace --all-features
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0.

## Stays unavailable

Everything observable until step 21 puts the trace in `@testRun.v2`.

## Excluded

Redaction profiles other than `@none`; the trap record's origin (29).

## Completion evidence

- [ ] Each of the 24 rows is a test, named in the report.
- [ ] The example row compares against text copied from the chapter, not
      text produced by `encode`.
- [ ] The validation commands exit 0.
