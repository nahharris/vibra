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

## As built: 16b-1

Landed (PR #375, merge 0fa74b0); the base is `m4` at
`746e3d3`, where Step 16a (PR #374) merged. Nothing a source program does changed:
`@host` is still `@tool.unavailable`, the corpus has the same 525 cases, and the
parity inventory is unchanged (135 of 135 matched).

- **IR.** `Expr::Host` carries a `&'static HostRow`, its operands, the checked
  `(result t e)` type, and an origin. `CheckedFunction::new_host` builds the
  function whose body is the call, marked an external wrapper, so the effect graph
  and the interpreter treat it as they treat a `@compiler` row. Validation checks
  the arity and the primitive parameter types against the row.
- **Interpreter.** `vibra-interp::host::Values` is the `HostValues` over
  `RuntimeValue`. The machine reads the call with `HostCall::read`, performs it,
  appends `AuditEvent::completed`, and builds the reply with `build_reply`; a
  provider's memory exhaustion is `Halt::Memory`, the host event, with no event
  recorded. `Execution::audit_trace` and `TestExecution::audit_trace` are
  `&[AuditEvent]`.
- **Emitter.** A host call evaluates its operands, issues an `i64` ID for each arena
  operand (`new_id`), calls the import `vibra_v1`.`<symbol>` with the IDs, adopts the
  returned ID into the destination slot, and then releases the IDs it issued. The
  imports follow the native imports in the function index space; the eight host
  routines (the seven builders and the adoption) follow the object constructor, and
  the optional routines after them are numbered from `extras_base()`. A module with no
  host import is byte-identical to before.
- **Runner.** `vibra-wasm-run::host` defines every row's import. Its `Access` is the
  runner's `HostValues`; it reaches a value through the exported accessors and the
  builders only (`host_single_source_m4_step16b` holds it to that). A value is an ID,
  a scalar, or `void`. The first stop of an accessor or builder is kept and returned
  from the import, after the module recorded it. `Runner::start_with_host`,
  `run_observed_with_host`, and `run_test_with_host` take a provider;
  `Instance::audit_trace` reads the events.
- **Wiring.** `run_target_with_host` and `run_tests_in_world` in `vibra-workspace`;
  the CLI builds a `SystemProvider`, capturing for `--format json`; the conformance
  handlers pass a fresh `WorldProvider` per test and per backend;
  `ExecutionObservation::audit_trace` and `TestRow::audit_trace` are `AuditEvent`s
  and the `@test-run.v1` and audit snapshots are written with `trace_to_vibon`.

### Rulings

- The runtime chapter says the command chooses the system provider's output
  destination (ledger D33.2).
- Header word 20 is `unsealed`, and an unsealed value carries a written map (D33.3).
- The `dict` and `function` kinds, an enum of more than one component, a wrapper or
  union of other than one, a nonzero variant for a kind without one, a write to a
  sealed value, and a second seal are `@runtime.invalid-host-value`.

### Cases

None added, re-expected, deleted, or downgraded: no source reaches an operation. The
specification-example inventory follows the lines of the runtime chapter, 9 rows.

### Evidence

- Host tests: `host_boundary_m4_step16b` (7: the `io` rows over a world in both
  backends, the same results and traces, non-ASCII text, arbitrary bytes, a second
  read, invalid UTF-8, a 1 MiB input and a million-byte output, and a provider's
  memory exhaustion as the host event in both), `host_builders_m4_step16b` (19: every
  trap condition, the borrowed parameter, the adopted result, a slot written twice,
  a value nested 5000 deep, and the arena returning to where it started), and
  `host_single_source_m4_step16b` (4).
- The output sink: two tests in `system_provider_m4_step16`.
- Validation of the head: format, clippy, `cargo test` (1366 passed), docs, the
  corpus (525 passed, 135 matched), the six named tests, the fuzz smoke, and
  `git diff --check`, all exit code 0.

### Left for 16b-2

The checker's `@host` branch (symbol, owner, signature), `std/io.vib` and the six
manifest symbols, the parser's attribute spans, the re-expected cases and tests
that assert `@host` unavailable (`V1-EFFECT-availability-host-member`, the
`@host` strings in `process_review`, `process_step13`, and `process_step14`, and
two others found by running them), the `io` and walk cases with parity rows, and
the negatives.

## As built: 16b-2

Landed on its branch, conditional on its pull request merging; the base is `m4` at
`0fa74b0`, where 16b-1 (PR #375) merged. `@host` is a supported form, and the `io`
operations are callable from source in both backends at once.

- **The checker rule.** `host_operation` checks a `deffect` member that writes
  `external: @host` against the registry, three facts on their own: the symbol is a
  row (`@external.unknown-symbol` at the symbol string); the row's owner,
  `@<unit>.<module>.<root>`, is the enclosing root (`@external.owner-mismatch` at the
  symbol string, relating the `deffect` name); and the written signature is the row's
  in every type, built from the `HostRow` with `result` and the module's error enum
  (`@type.type-argument-mismatch` at the member, noting the registry signature). A
  member that passes becomes `CheckedFunction::new_host` with its owner root; one that
  fails is not checked further. The same rule runs in the single-source and the
  workspace checkers.
- **The standard library.** `stdlib/src/std/io.vib` declares `io-error` and the roots
  `stdin`, `stdout`, and `stderr` with their six `visibility: @public` operations.
  The manifest lists the module and the six symbols; the package stays `0.3.0`.
- **The parser.** `FunctionAttributes::attribute_span` and `value_span` carry the span
  of each attribute, and `DeffectDeclaration::name_span` the root's name. `@host` on
  anything but a `deffect` member, a non-empty `effects:` row on a `@host` or
  `@compiler` member, and a non-host `external:` on an effect member are reported at
  the `external:` or `effects:` attribute, as the diagnostics table says.
- **The example.** `examples/m4-console` has a library walk over `iter.next`, a binary
  that echoes standard input and walks three lines, and a test; `process_m4_console`
  runs it with the binary over the real streams, in human and JSON modes.

### Rulings

- The three registry checks are independent and a member wrong in two ways is reported
  for both (the effects chapter, **Nominal effects**).
- A `@host` member in a module that is not an embedded standard-library module is
  `@external.owner-mismatch`, because every row's owner is a standard-library root.
  That is how "ordinary packages cannot declare them" holds, with no further rule.
  Ledger D33.4.

### Cases

Added, all `matched` in both backends: `V1-RUNTIME-io-write-text-and-bytes`,
`V1-RUNTIME-io-stdin-text-twice`, `V1-RUNTIME-io-stdin-bytes-twice`,
`V1-RUNTIME-io-invalid-text`, `V1-RUNTIME-io-error-propagates-with-try`,
`V1-RUNTIME-io-effectful-iter-walk` (`workspace-run`), and
`V1-RUNTIME-io-test-world-stdin` (`workspace-test`, whose world supplies stdin and is
copied fresh for each test and backend). Negatives:
`V1-EFFECT-host-owner-mismatch`, `V1-EFFECT-host-signature-mismatch` (which is also an
owner mismatch, so two diagnostics), `V1-EFFECT-host-additive-row`, and
`V1-EFFECT-host-target-array-omits-stdout`. An entry must return `void` or
`(result void e)`, so the cases observe what they read through the events.

Re-expected, because they asserted `@host` unavailable:

| Case or test | Now |
| --- | --- |
| `V1-EFFECT-availability-host-member` | `@external.unknown-symbol` at the symbol string |
| `V1-PROJECT-workspace-check-nominal-availability` | `@external.unknown-symbol` at the symbol string |
| `V1-TOOL-index-unavailable` | `@external.unknown-symbol` at the symbol string; the index is unchanged |
| `process_review`, `process_step13` (2), `test_runner_step13` (2), `process_step14` | the unavailable fixture is a `native:` declaration outside the standard library, which is still `@tool.unavailable` |
| `semantic_step12::host_provider_outside_a_deffect_and_unknown_provider_are_rejected` | `@host` on a module-level `defn` is `@syntax.invalid-attribute` |
| `trusted_checker_rejects_unknown_external_provider_symbol_and_signature` | the same, for the `@host` source |
| the `stdlib` unit tests of the `host` list | the embedded list is the six `io` symbols; the synthetic module replaces `@std.io` |
| `host_documents_fmt_m4_step16` | the manifest's `host` list is the six symbols |

Parity: 7 rows added, 142 of 142 `matched`.

### Evidence

Corpus 536 passed (83 reader, 287 static, 150 interpreter, 16 tooling); interpreter
backend 142 passed, Wasm backend 142 matched, 0 not lowered. The specification-example
inventory follows the lines of the effects, source-language, and type chapters (13
rows).
