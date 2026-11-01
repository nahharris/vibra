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

## As built

Landed on its branch, conditional on its pull request merging; the base is the head
of Step 18 (PR #378, not yet merged).

- **Classification.** `Execution::program_failed` is the `err` variant of the entry's
  observed value (`ObservedValue::is_err_variant`); an entry signature is `void` or
  `(result void e)`, so the only enum an entry returns is a `result`, and the error's
  content is never inspected. `execute_run` maps it to `CommandResult::ProgramFailed`
  and, in human mode, writes the `programResult` encoding (a line of its own) to
  standard error; standard output keeps the program's bytes. A trap and the memory
  host event are decided before the value is read, so neither is recast.
- **Conformance observation.** The result snapshot already distinguishes `err` from
  `ok`, so no field was added for the command outcome (the corpus chapter already
  says so). The harness classifies the result in both backends (`wasm::observe_entry`
  classifies the module's `vibra_v1_result` value) and fails the Wasm backend on a
  disagreement. The corpus had no way to record a `workspace-run` trap, which the
  sweep needs: an accepted `workspace-run` case may expect the trap's runtime error
  diagnostic (unlocated, `0..0`) in place of the result and trace snapshots, and the
  Wasm backend must end with the same trap code (`WasmObservation::Trapped`).
  Written into the conformance chapter.
- **Cases.** `V1-RUNTIME-entry-outcome-own-err`, `-trap-holding-err`, and
  `-memory-holding-err`, all `matched`. Success with host output
  (`io-write-text-and-bytes`), a handled typed error (`fs-errors-are-values`,
  `io-invalid-text`), and propagation with `try` (`io`, `fs`, `env`
  `-error-propagates-with-try`) already existed and already recorded an `err`
  result; none needed re-expecting.
- **Re-expected.** `process_review_fixes::a_deeply_nested_entry_result_does_not_abort_the_process`
  asserted exit 0 and `@command.ok` for an entry returning `err`; it now expects exit 6
  and `@command.program-failed`.
- **Process tests.** `process_m4_entry_outcomes` (real binary, JSON validated against
  the command-result schema, and human mode): exit 0, 3, 5, and 6; own error,
  propagated host error, handled error, `void`, trap and memory exhaustion holding an
  `err`, an error nested 5000 deep, and a union payload with a result over a million
  bytes.
