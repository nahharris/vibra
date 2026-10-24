# Step 16b — the host boundary and console

Prerequisite: Step 16a merged. Stage 4B behavior step: the first host operations, in
both backends in the same change. It makes the `@host` external a supported form,
lands the scalar-only ABI, the audit events, and the `io` roots, and shows an
effectful walk over `iter.next`.

## The split into 16b-1 and 16b-2

The step is too large for one reviewable change, and the delivery rule of
Milestone 4 says host operations land "each in both backends in the same change".
A first part that made `@std.io` callable in the interpreter alone would break
that rule, so the split is by layer: **no operation becomes reachable from source
until both backends have it.** 16b-1 lands the plumbing in both backends and proves
it with hand-built IR; 16b-2 turns the checker on and ships the `io` operations in
both backends together. The scope, tasks, and test matrix below are the whole
step; each item belongs to the part named beside it.

## 16b-1: the host-call plumbing

Scope items 3, 4, and 5 (and the wiring of 5 into `run`, `test`, and the
conformance handlers), ordered tasks 2, 3, 4, 5, 6, and 7, and the "Proof" and
"Single-source audit" below. `@host` stays `@tool.unavailable` in source and the
corpus is unchanged.

- **IR.** `Expr::Host { row, arguments, result, origin }` and
  `CheckedFunction::new_host`, a function whose body is the call of its row. No ID,
  offset, or instance is in it.
- **Interpreter.** `HostValues` over `RuntimeValue`, with the call's concrete
  `(result t e)` type and `program.types()` naming the enums. `Execution` and
  `TestExecution` carry `Vec<AuditEvent>`. `Interpreter::run_with_host` and
  `run_test_with_host` take a provider; the existing functions run over the default
  world.
- **Emitter.** One import of `vibra_v1` for each row the program applies, after the
  native imports in the same two-pass scheme; the seven builders and the routine
  that adopts a result, present exactly when a host import is; the `unsealed` header
  word and a written map, one bit for each component of an unsealed value, which the
  seal checks.
- **Runner.** The store carries a provider, the event vector, and the memory-event
  flag. `HostValues` is implemented through `vibra_v1_length`, `vibra_v1_read_i32`,
  `vibra_v1_read_id`, and the builders only. A provider's memory exhaustion is the
  host event; a builder trap reaches the entry as the recorded
  `@runtime.invalid-host-value`.
- **Wiring.** `run` gives the system provider, `test` a fresh `WorldProvider` for each
  test, and the corpus handlers a fresh copy of the case's world for each test and
  each backend. The CLI `auditTrace` is an array of event objects.
- **The output sink.** The system provider delivers standard output and error where
  the command chooses: human `run` lets the program write the process's streams, and
  JSON `run` captures the bytes for the envelope. The runtime chapter says so in
  one sentence.
- **Proof.** `host_boundary_m4_step16b` runs hand-built IR over the `io` rows in both
  backends and holds one result and one trace to expectations written from the
  specification; `host_builders_m4_step16b` forges a host through
  `Runner::start_forged` and exercises every condition of
  `@runtime.invalid-host-value`, the borrowed parameter, the adopted result, and a
  value nested 5000 deep; `host_single_source_m4_step16b` audits that no backend
  implements an operation, calls a provider method, or builds an event except with
  `AuditEvent::completed`, and that the runner's host module never reads
  `vibra_v1_memory`.

## 16b-2: the `io` operations

Scope items 1, 2, 6, and 7, ordered tasks 1 and 8, and the corpus cases of the
brief. The checker's `@host` branch becomes a registry check of symbol, owner, and
signature; `stdlib/src/std/io.vib` and the six manifest symbols; the parser's
`@syntax.invalid-attribute` spans move to the offending attribute, as the
diagnostics table states; every case and test that asserted `@host` unavailable is
re-expected; the cases are `matched` in both backends, with parity rows.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **WebAssembly boundary** (**Host calls and
  the builders**), **The host registry**, **The audit trace**, **Traps**,
  **The host world**.
- [Effects](../../spec/03-effects.md): **Nominal effects** (the `@host` diagnostics)
  and **V1 standard effect inventory**.
- [Source language](../../spec/01-source-language.md): **External definitions**.
- [Diagnostics](../../spec/07-diagnostics-and-conformance.md): **Effect and host
  diagnostics**, the host-operation coverage paragraph.
- [Tooling](../../spec/05-tooling.md): the `run` payload, `AuditEvent`, and
  `test` output.
- [Ledger](decision-ledger.md) rows D17.1–D17.5, D18, D20.1, D26.1, D30.

## Scope

1. `external: @host` and `symbol:` on a member of a `deffect`, with the registry
   check of symbol, signature, and owner, and the diagnostics of the effects
   chapter (`@syntax.invalid-attribute` outside a `deffect` and with an additive
   row, `@external.owner-mismatch`, `@external.unknown-symbol`,
   `@type.type-argument-mismatch`).
2. `@std.io` with `io-error`, the roots `stdin`, `stdout`, `stderr`, and the six
   operations; the manifest `host` list gains the six symbols.
3. The `vibra_v1` imports in the emitter, the seven builders, the adoption of an
   owned result, borrowed parameters, and the trap `@runtime.invalid-host-value`
   for every builder misuse.
4. One implementation of each operation, behind the value-access trait of 16a; the
   interpreter and the Wasm runner supply the trait, and both build the audit event
   with the same code.
5. `vibra test` standard output and error going to the audit trace, and the CLI
   `auditTrace` producer emitting `AuditEvent` objects; `run` with the system
   provider.
6. An effectful walk over `iter.next` that reads standard input and writes standard
   output (inherited row I9).
7. The corpus cases: each operation's success and each of its errors, in both
   backends, with one expected result and one expected trace.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-types/src/lib.rs` (`compiler_intrinsic` and its `provider == "host"` branch near line 3457) | The `@host` branch reports unavailable today; it becomes the registry check |
| `crates/vibra-ir/src/external.rs` | The registry table of 16a; a typed IR call target for a host operation |
| `crates/vibra-interp/src/lib.rs`, `registry.rs`, `native.rs` | Calling an operation through `vibra-host`; the audit trace it already carries as `Vec<String>` becomes event records |
| `crates/vibra-wasm/src/runtime.rs`, `encode.rs`; `crates/vibra-ir/src/boundary.rs` | The import module `vibra_v1`, the builder exports, and their codes, written once in `boundary` |
| `crates/vibra-wasm-run/src/instance.rs`, `observe.rs`, `test_run.rs` | The host side of the imports and builders; the audit trace of a test |
| `crates/vibra-conformance/src/test_run.rs`, `runner.rs`, `wasm.rs` | The `@test-run.v1` writer and the trace comparison (event by event, naming the backend) |
| `crates/vibra-cli/src/lib.rs` (`RunPayload`, `TestPayload`) | `Vec<String>` becomes the event object; human output of `test` |

## Ordered tasks

1. The `@host` check in the checker and its diagnostics, with `@std.io` declared in
   source: `(deffect stdout visibility: @public (defn text (value str) (result void
   io-error) external: @host symbol: "io.stdout-text") …)`.
2. The typed IR call target for a host operation; no IR field names an ID, an
   offset, or an instance (the identity test of Step 4 still passes).
3. The operations in `vibra-host` against the value-access trait, and the
   interpreter's implementation of the trait.
4. The Wasm imports, the builders, the result adoption, and the runner's
   implementation of the trait through the accessors and builders only. Add a
   host test that the runner never touches `vibra_v1_memory` for a host call.
5. The audit event built by one function from the canonical encodings; both
   backends append it after a completed call and not for a trap or a host event.
6. The `test` world wiring and the `run` system provider wiring; `auditTrace`
   events in both payloads.
7. Parity: every new executable case `matched`; a builder-misuse host test for each
   trap row (sealed write, bad index, wrong class, unwritten slot, invalid `char`,
   unsealed child, unsealed result).
8. Add the effectful `iter.next` walk to the examples and the corpus.

Invariants preserved: parameters are borrowed and the host never releases them; a
result is adopted exactly once; no event for a trap or host event; a backend's
layout is unobservable; `never` is never returned by an operation.

## Test matrix

- Positive: `io.stdin-text` and `io.stdin-bytes` (including the second call
  returning empty); each stdout and stderr operation with text containing
  non-ASCII scalars and with arbitrary bytes; the walk; events in call order; a
  `void` result event.
- Negative: invalid UTF-8 standard input as text (`invalid-text`, position at the
  end); a closed stream (`closed`, in the system provider's host test); each
  `@host` declaration diagnostic; a program that reaches a builder misuse.
- Recovery: a failed call returns `err` and the program continues; an
  `invalid-host-value` ends one test and the next runs.
- Boundary: empty input, a 1 MiB input, a value nested five thousand deep passed as
  an argument, an output of one million bytes, an event for a `char`-heavy string.
- Formatter: the `deffect` forms with `external:` and `symbol:` in canonical
  attribute order.

## Diagnostic and schema changes

No new code. The command-result schema already lists the event object and
`@runtime.invalid-host-value` (Step 13); the producer now emits them. The
`@test-run.v1` observation gains event records in its `audit-trace` field.

## Validation

The [pre-merge list](validation.md#before-merging-each-step), the Step 4–12 row, and
`cargo test --locked --offline -p vibra-host -p vibra-ir -p vibra-interp -p vibra-wasm
-p vibra-wasm-run -p vibra-cli -p vibra-schema`. Record the corpus counts per profile
and per backend; the Wasm backend has no `not lowered` row.

## Excluded

The filesystem, environment, clock, and random operations (17, 18), program
arguments (18), entry outcomes (19), query metadata (20).

## Completion evidence

The `io` cases match in both backends with identical events; the registry snapshot
and the ambient-read test still pass; `@host` is no longer reported unavailable.
