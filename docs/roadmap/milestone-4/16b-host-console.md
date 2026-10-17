# Step 16b — the host boundary and console

Prerequisite: Step 16a merged. Stage 4B behavior step: the first host operations, in
both backends in the same change. It makes the `@host` external a supported form,
lands the scalar-only ABI, the audit events, and the `io` roots, and shows an
effectful walk over `iter.next`.

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
