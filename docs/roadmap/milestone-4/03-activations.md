# Step 3 — deep non-tail recursion in the reference interpreter

Prerequisite: Step 2 merged. Stage 4A behavior step, reference interpreter
only. The WebAssembly backend meets the same outcome in Step 6. This step
claims the interpreter half of the exit clause "deep non-tail recursion has the
same specified outcome in both backends".

## Read before editing

- [Runtime](../../spec/06-runtime.md): **Activations and memory**, **Tail
  calls**, **The value arena**, **Reclamation**, **Traps**.
- [Tooling](../../spec/05-tooling.md): **M2 command contract** (result atoms,
  exit mapping, `run` and `test` payloads) and **Execution backend of `run` and
  `test`**.
- [Diagnostics](../../spec/07-diagnostics-and-conformance.md): **Diagnostics are
  a language surface** (the `@runtime.memory-exhausted` paragraph) and
  **Conformance corpus** (`expect.host_event`).
- [M4 ledger](decision-ledger.md) rows D1.1–D1.4, D3.3, D13.1.

## Scope

Replace the interpreter's interim host-stack rule with the specified outcome:
no host stack per language activation, depth bounded only by memory, and
exhaustion as the host event `@runtime.memory-exhausted`.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-interp/src/lib.rs`: `MAX_ACTIVATION_DEPTH`, `enter_activation`, `stack_exhausted`, `INTERPRETER_STACK_BYTES`, `STACK_GUARD_BYTES`, `RuntimeError::HostStackExhausted`, `host_diagnostic`, `run_activation`, `evaluate` | The bound and the recursion that needs it. Step 1 already made `host_diagnostic` report `@runtime.memory-exhausted`; this step removes the bound |
| `crates/vibra-interp/src/lib.rs`: `RuntimeValue` | Compound values are nested boxes and vectors, so dropping a deeply nested one recurses on the host stack |
| `crates/vibra-workspace/src/test_runner.rs`, `crates/vibra-cli` | Where the host event reaches `run` and `test` |
| `crates/vibra-conformance/src/manifest.rs`: `ExpectedExecution`, the `[expect]` table, and the interpreter handlers | Gains `host_event` |
| `crates/vibra-cli/tests/process_review.rs`, `process_review_fixes.rs`; `crates/vibra-conformance/tests/tail_calls_step9.rs`; `crates/vibra-workspace/tests/test_runner_step13.rs` | Existing tests that name the old bound |

## Ordered tasks

1. Characterize: a host test that a non-tail recursion a million deep, with a
   base case, currently ends in the host event, so the change is visible.
2. Make an activation a value of the machine. Evaluate with an explicit
   stack of frames and continuations held on the heap, so a non-tail call
   pushes a frame and a return pops it, and no Rust call is made per language
   activation. The tail-transfer path keeps replacing the current frame. A
   single function body's expression nesting is bounded by its source, so it
   needs no frame of its own.
3. Account for memory. Add a runner-supplied budget in bytes of live frames
   and values, applied to the whole run, with a default that the CLI chooses.
   Exceeding it, or failing to allocate, is `RuntimeError::MemoryExhausted`,
   renamed from `HostStackExhausted`, with no stack-address probe and no
   dedicated large-stack thread.
4. Release deeply nested values with bounded stack: give `RuntimeValue` a
   worklist drop, so releasing a value nested five thousand levels deep, and
   far deeper, cannot overflow the host stack
   ([ledger D3.3](decision-ledger.md)).
5. Add `expect.host_event` to the manifest (valid only on `interpret` and
   `workspace-run`, exclusive with the result and trace snapshots) and to the
   two interpreter handlers.
6. Update the old tests mechanically: the activation-bound tests become
   depth tests that complete, plus one exhaustion test under a small budget.

Invariants preserved: tail calls reuse the frame; evaluation order; traps are
never recast as host events; a host event never produces a program result.

## Test matrix

- Positive: non-tail recursion a million deep completes with its result;
  mutual non-tail recursion; non-tail recursion through a closure, a function
  value, and a contract member; a deeply nested value built and dropped.
- Negative: recursion with no base case ends with the host event; `run` is
  `@command.operational-failure` (exit 3) with null `programResult` and `trap`;
  `test` reports zero selected, passed, and failed and an empty `tests` array;
  the diagnostic is unlocated (`0..0`, no source ID).
- Recovery: after a host event in one `run`, a following valid run in the same
  process behaves normally.
- Boundary: the budget exactly at a program's need and one frame under it; a
  value nested 5,000 and 1,000,000 deep; a tail loop that never approaches the
  budget.
- Formatter: no change.

## Diagnostic and schema changes

`@runtime.memory-exhausted` was registered in Step 1. The `RuntimeError`
variant is renamed. No JSON schema changes: the host event is not a trap, so
the trap-code enumerations are untouched. A corpus case with `host_event`
needs the manifest to accept it; that is the only contract change.

## Validation

The [Step 3 row](validation.md#focused-checks), then the full
[pre-merge list](validation.md#before-merging-each-step). The million-deep case
must finish in release and debug builds within the CI time budget; record the
times.

## Excluded

The WebAssembly lowering of the same outcome (Step 6); a memory limit in
`vibra build` or a user-facing option for it; any change to which program
exhausts memory on a given host.

## Completion evidence

`MAX_ACTIVATION_DEPTH` and the stack-address probe are gone; the two
`V1-RUNTIME` cases exist (completion and exhaustion) with expectations authored
from the specification; the host tests pass; the PR lists every test whose
assertion changed.
