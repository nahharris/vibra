# Step 19 — entry outcomes

Prerequisite: Step 18 merged. Stage 4B behavior step: the command result and exit
code of an entry that returns `err`, the reassigned M3 form of gap G16, and the
parity sweep over success, typed host error, propagation, and trap.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **Entry outcome**, **Traps**,
  **Activations and memory** (the host event stays exit 3).
- [Tooling](../../spec/05-tooling.md): the `run` payload, the exit mapping, and
  human `run` output.
- [Projects](../../spec/04-programs-and-packages.md): the entry signature rule.
- [Diagnostics](../../spec/07-diagnostics-and-conformance.md): the `workspace-run`
  observation and **Differential execution**.
- [Ledger](decision-ledger.md) rows D21.1, D30.3.

## Scope

1. `run` ends an entry that returns `err` with `@command.program-failed` and exit
   code 6: `programResult` is the canonical encoding of the entry's `result`, `trap`
   is `null`, there is no diagnostic, and human mode prints the encoding to standard
   error. An entry that returns `void` or `ok` stays `@command.ok`.
2. A host failure reaches the entry only as the `err` of its own operation, so an
   entry that propagates a host error with `try` ends the same way.
3. Both backends report the same outcome. The Wasm side reads the entry result
   through `vibra_v1_result` and classifies the variant.
4. The parity sweep: success, a typed host error handled, a typed host error
   propagated to the entry, an entry returning its own error, a trap, and the
   memory host event, each with the same result and exit code in both backends.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-cli/src/lib.rs` (`CommandResult::ProgramFailed`, the `run` result selection near line 580) | The variant exists since Step 13; this step produces it |
| `crates/vibra-workspace/src/semantic.rs` (`RunOutcome`, `canonical_result`) | Classifying the entry's `result` after a completed run |
| `crates/vibra-wasm-run`, `crates/vibra-conformance/src/wasm.rs` | The module's result slot and its classification |
| `crates/vibra-cli/tests/process_*.rs` | Process tests for the exit code and stderr |

## Ordered tasks

1. Classify a completed entry's result by its type: a `(result void e)` whose
   variant is `err` is a program failure. Do not inspect the error's content.
2. Produce the result atom, exit code, payload, and human stderr line.
3. Update the M3 demo or corpus expectation that recorded `@command.ok` for an
   entry returning `err`, as a reviewed change, and remove the sentence from the
   evidence documents that names the old behavior.
4. Add the `workspace-run` cases for each outcome and the process tests.
5. Run the parity sweep and record the counts.

Invariants preserved: an `err` is a value, so no diagnostic and no trap exists for
it; a trap stays `@command.trap`; a host event stays `@command.operational-failure`;
`test` has no program-failure result.

## Test matrix

- Positive: `ok`, `err` of an own error, `err` propagated through `try` from a host
  operation, an `err` handled and the entry then returning `ok`.
- Negative: an entry returning `void` (still `@command.ok`); an entry that traps
  while holding an `err` (trap wins); memory exhaustion while holding one.
- Recovery: none; the command ends.
- Boundary: an error value nested five thousand deep, an error with a payload that
  contains a union, a `programResult` of one million bytes.
- Formatter: none.

## Diagnostic and schema changes

The command-result schema and the CLI variant exist since Step 13; this step adds the
producer test that validates a real `run` envelope against the schema's program
failure rule.

## Validation

The [pre-merge list](validation.md#before-merging-each-step) and `cargo test
--locked --offline -p vibra-cli -p vibra-workspace -p vibra-conformance`, with the
process tests for exit codes 0, 3, 5, and 6.

## Excluded

Exit codes for individual error types, a way for a program to choose its exit code,
`vibra build`.

## Completion evidence

The exit clause "success, typed host error, propagation, and trap have
interpreter/Wasm parity" passes through a `workspace-run` sweep, and the M3 inherited
row I7 is closed.
