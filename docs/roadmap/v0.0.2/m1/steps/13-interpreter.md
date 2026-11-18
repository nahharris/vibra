# Step 13: interpreter

Kind: infrastructure. The reference interpreter over the checked IR of step
12. Its language-level evidence lands with the checker steps that admit each
form (15 to 23); here it is verified by host tests over hand-built IR, as
the v1 interpreter was before its checker existed. The design of v1 is kept
(heap activations, tail transfer, memory budget, instrumentation); the code
is new because every value kind changed.

## Prerequisites

- Step 12 landed.
- Read: [runtime chapter](../../../../spec/06-runtime.md) "Semantic
  reference", "Evaluation", "Module-value initialization", "Callable
  contract execution", "Generic instantiation", "Tail calls", "Activations
  and memory", "Test assertions", "Compiler intrinsic registry",
  "Representation latitude", "Canonical value encoding", "Traps";
  [tooling chapter](../../../../spec/05-tooling.md) "Limits".
- Open questions: none.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-interp-v1/src/lib.rs:52` (`MemoryBudget`), `:83` (`Execution`), `:260` (`RuntimeError`), `:388` (`run_with_host`) | API shape; `run` and `run_test` take a budget and no host in M1 |
| `crates/vibra-interp-v1/src/machine.rs` | the explicit-continuation machine: frames on the heap, tail transfer, lazy globals, memory accounting; redesign the value representation |
| `crates/vibra-interp-v1/src/registry.rs` | the intrinsic implementations: integer saturating and checked arithmetic, floats, char, strings; port the arithmetic with the chapter's exact meanings |
| `crates/vibra-interp-v1/src/machine/compare.rs` | canonical comparison for dict keys and structural equality |

Proposed: `vibra-interp::{machine, frame, value, intrinsics, compare, encode,
evaluate}`.

## Ordered tasks

1. Values with representation latitude: `Str` as a Rust string, `Bytes` as
   bytes, `Bool` unboxed, `Option` compact, fieldless cases as tags; the
   `@value.v2` encoder renders them as their declared types. Invariant: no
   observation depends on the representation.
2. The machine: strict evaluation in written order (callee, then every
   argument, labelled included), frames on the heap, `let` binding after
   evaluation, `let … else` fallback, `match` first-arm selection, `if`,
   `do`, `return` leaving the nearest callable, `try` propagation of `none`
   and `err`. Invariant: no Rust recursion proportional to Vibra depth.
3. Tail calls: every call in tail position replaces the frame, whatever the
   callee (direct, value, closure, contract member); a pending `ensures …
   else`, widening, or `try` is work after the call and prevents it.
4. Module values: lazy, once per execution, states unread, evaluating,
   ready; evaluating is an invariant violation (`@runtime.invalidCheckedProgram`
   trap), never a way to accept a cycle.
5. Guard consequences: `requires C else V` on entry (false skips body and
   `ensures`); `ensures C else V` on the candidate, once, with the result
   name bound, `V` not re-checked; `where P else E` at construction
   returning `Result`; a bare `where` leaves no check. Invariant: operands
   are still evaluated when a check is folded away by the checker (the
   checker passes the folded program, so nothing to do here but a test).
6. Dispatch: contract calls select the implementation from the receiver's
   runtime type; default members run with the receiver and interface
   arguments of their call; interface values carry the concrete
   implementation.
7. Intrinsics: every row of the compiler registry with the chapter's
   meaning: saturating `add`, `sub`, `mul`, `neg`, shifts; checked variants
   returning `Result`; float IEEE operations with NaN canonicalization;
   `Char`, `Str`, `Bytes`, `List`, `Dict` operations; comparison for the
   role types. No plain division. Invariant: no intrinsic traps on user
   data.
8. Memory budget: frames and values counted; `RuntimeError::MemoryExhausted`
   as a host event; peak and live measurements exposed.
9. Traps: `InvalidCheckedProgram` for an IR invariant violation; nothing
   else traps. `UnobservableValue` for a function behind an interface
   value at an observation is an observation error, not a trap.
10. `evaluate(&CheckedModuleSet, &Expr, &[Value], Limits) -> Evaluation`
    for the checker: `Value`, `LimitReached(limit)`, or `Defect(trap)`; the
    reduction count is the limit.
11. `run_test`: a `Void` entry; a false assertion stops the test and is
    returned as `TestAssertionFailure { assertion, expected, actual, origin }`.
12. Architecture row: `vibra-interp: [vibra-diagnostics, vibra-ir]`.

## Test matrix

Host tests over hand-built IR.

| Test | Input | Expected |
| --- | --- | --- |
| evaluation order | a call whose callee and three arguments each append to a trace through a closure | callee first, then arguments in written order, labelled included |
| let else | a refutable pattern that fails | only the fallback runs; the body after the `let` does not |
| match first arm | overlapping arms | the first matching arm's body runs; no other arm's body is evaluated |
| tail depth | a self tail-recursive function counting to 1,000,000 | completes; `max_activation_depth` stays 1; `tail_transfer_count` is 999,999 |
| indirect tail | tail call through a closure, a function value in a record, and a contract member | each reuses the frame |
| ensures blocks tail | a tail call under an `ensures … else` | the frame is kept; the consequence runs on a failing candidate |
| non-tail depth | recursion 100,000 deep with work after the call | completes under a 64 MiB budget; no Rust stack growth (run in a thread with a 256 KiB stack) |
| unbounded recursion | no base case | `MemoryExhausted` within the budget; `is_host_event()` |
| module value once | a global read twice with a counting initializer | the initializer runs once |
| requires else | a false entry condition and a body with a trace | the consequence is returned; the body and `ensures` leave no trace |
| ensures else | a candidate that fails | the consequence is evaluated once with the result name bound to the candidate; its own value is not re-checked |
| where else | a wrapper whose predicate fails | `Result.err(error: E)`; a passing one yields `Result.ok` |
| saturating arithmetic | `I8.add(127i8 1i8)`, `U8.sub(0u8 1u8)`, `I8.neg(-128i8)`, `I32.shiftLeft(1i32 40u32)` | `127i8`, `0u8`, `127i8`, `2147483647i32` |
| checked arithmetic | `divChecked(1i32 0i32)`, `divChecked(-2147483648i32 -1i32)`, `remChecked(-2147483648i32 -1i32)`, `shiftLeftChecked(1u8 8u32)` | `divisionByZero`, `overflow`, `ok 0i32`, `invalidShift` |
| float text | `1e20`, `0.0025`, `-0.0`, NaN, a value that rounds to infinity when parsed | `1e20`, `0.0025`, `-0.0`, `nan`; parse gives `outOfRange` |
| NaN canonical | two NaNs with different payloads | `equal` is false for each with itself; `compareTotal` and the encoding agree and show one NaN |
| lookups | `List`, `Dict`, `Str`, `Bytes` with an index out of range or a missing key | `Option.none()`; never a trap |
| dict order | a dict built from keys `"b"`, `"a"`, `"a"` | two entries, `"a"` first with the later value |
| canonical observation | a record with declared type, a variant with payload, a wrapper | the `@value.v2` text of the chapter |
| unobservable | an interface value holding a closure at the entry's result | an observation error, not a trap |
| evaluate limit | a loop of 10,000 reductions under a limit of 100 | `LimitReached`; under 100,000 the value |
| dispatch | two implementations of one interface; a call through a value of each | each selects its own |

## Diagnostics and schema changes

Codes: `@runtime.invalidCheckedProgram` (trap), `@runtime.memoryExhausted`
(host event), `@tool.unsupportedValue` (observation error). No schema
change.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-interp
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: every test above passes; the depth tests run inside a thread with a
small stack and still pass.

## Excluded

Host operations and grants (M2); native implementations (v0.0.3); any
front-end dependency.

## Completion evidence

- [ ] The architecture test lists `vibra-interp` with `vibra-diagnostics` and
      `vibra-ir` only.
- [ ] Every intrinsic row of the chapter has a host test named after its wire
      symbol.
- [ ] The tail-depth and non-tail-depth tests pass in a 256 KiB-stack thread.
- [ ] Validation commands exit 0.
