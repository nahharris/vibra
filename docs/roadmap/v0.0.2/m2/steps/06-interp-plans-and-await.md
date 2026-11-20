# Step 06: interp-plans-and-await

Kind: vertical. Slice 1, in the interpreter: plan values, `await` as a
continuation, guards with `else` on each invocation, invocation ordinals.
Package N4 of the native contract, interpreter half.
Depends on: 03.
Crates: `vibra-interp`.

## Prerequisites

- Read: [runtime
  chapter](../../../../spec/06-runtime.md#cold-plans-and-invocation-identity)
  "Cold plans and invocation identity", "Callable contract execution",
  "Tail calls", "Activations and memory", "Audit trace" (the paragraph
  "Invocations"), "What has no encoding";
  [questions](../spec-questions.md) Q13.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-interp/src/machine.rs:160` | `pub(crate) enum Kont` | gains `Await` |
| `crates/vibra-interp/src/machine.rs:1159` | `pub(crate) fn invoke` | entered by an `await` exactly as by a call |
| `crates/vibra-interp/src/machine.rs:2034` | `fn unavailable_plan` | deleted |
| `crates/vibra-interp/src/value.rs:26` | `pub(crate) enum Repr` | gains `Plan` |
| `crates/vibra-interp/src/evaluate.rs:99` | `pub fn run` | unchanged; `run_test` gains the procedure context |
| `crates/vibra-interp/src/lib.rs:186` | `pub enum RuntimeError` | `Unavailable` stays for a host body |

Proposed: `vibra-interp::plan` (`PlanVal`, building, awaiting);
`Instrumentation::invocation_count`.

## Files owned

- `crates/vibra-interp/src/plan.rs`
- `crates/vibra-interp/src/machine.rs`
- `crates/vibra-interp/src/value.rs`
- `crates/vibra-interp/src/evaluate.rs`
- `crates/vibra-interp/src/encode.rs`
- `crates/vibra-interp/src/ingress.rs`
- `crates/vibra-interp/src/lib.rs`
- `crates/vibra-interp/tests/it/plans.rs`
- `crates/vibra-interp/tests/it/main.rs`

## Ordered tasks

1. Applying a callee of a `Proc` type evaluates the callee, then the
   operands in written order, and allocates a `PlanVal` holding the
   resolved target, the operands, and the type environment. It pushes no
   frame. Invariant: no guard, body, or consequence of the callee is
   evaluated by an application.
2. `Expr::Await`: evaluate the operand to a `Plan`; take the next
   invocation ordinal (from 1, per execution); push
   `Kont::Await { caller_invocation }`; enter the plan's target through
   `invoke`. On return the continuation restores the caller's ordinal.
   Invariant: each `await` enters the callee once, with its entry and exit
   guards.
3. An `Await` that is the operand of a `return` in tail position reuses
   the activation unless a pending `ensures … else`, widening, or cleanup
   forbids it, as for a call.
4. An `Await` reached in a frame whose function is `Pure` is
   `Halt::Invalid`: `@runtime.invalidCheckedProgram`.
5. A plan is accounted against the memory budget like a closure. A plan
   reached by the encoder is `RuntimeError::UnsupportedValue`.
6. `run_test` runs a function whose `context` is `Procedure`; the body is
   invocation 1 (Q13). `Instrumentation::invocation_count` counts started
   invocations.
7. A `FunctionBody::Host` stays `Halt::Unavailable` (step 20).

## Test matrix

Tests of `vibra-interp` over hand-built checked IR. `inc` is a `Proc({I32}
I32 {})` returning its argument plus one; `spin` is a `Proc({} Never {})`
that recurses without end.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| build does not run | positive | a procedure-context function that evaluates `Call(spin, [])` and returns `void` | completes; `invocation_count` is 1 | 06 Cold plans: "Building or copying a plan MUST NOT run a procedure's condition, body, or consequence" |
| await runs | positive | `Await(Call(inc, [1i32]))` | `2i32`; `invocation_count` is 2 | 06 Cold plans: "Each `await` is a new invocation" |
| await twice | positive | one plan bound to a slot and awaited twice, results added | `4i32`; `invocation_count` is 3 | 06 Cold plans: "A plan has no \"consumed\" bit and no memoized result" |
| `requires … else` per await | positive | a `Proc({I32} I32 {})` with entry guard `n > 0 else -1`; one plan with `0i32` awaited twice | `-1i32` both times; the body's reduction count is zero | 06 Cold plans: "A `requires … else` runs on each one" |
| `requires … else` not at build | positive | the same plan built and never awaited | the guard's predicate is not evaluated (reduction count) | 06 Cold plans: "Building … MUST NOT run a procedure's condition" |
| `ensures … else` on await | positive | a `Proc` whose exit guard replaces a negative candidate by `0i32`, awaited | `0i32` | 06 Callable contract execution: "`ensures C else V`" |
| plan through a record | positive | a plan stored in a record field, read, and awaited | the callee's result | 06 Cold plans: "The row survives … through capture, records, returns" |
| plan captured by a closure | positive | a `Proc` closure that awaits a captured plan | the callee's result | the same |
| operands in written order | positive | `Call(inc2, [a, b])` where both operands are observable intrinsic calls | evaluated a then b, at application | 06 Evaluation: written order |
| `return await` in tail position | boundary | a `Proc` that tail-awaits itself 100,000 times | completes; `max_activation_depth` does not grow with the count; `tail_transfer_count` is 100,000 | 07 Required coverage 13: "`return await` reuse their activation for at least 100,000 transfers" |
| non-tail await depth | boundary | 100,000 nested non-tail awaits, run on a 256 KiB host thread | completes within the default budget | 07 Required coverage 13: "no native stack frame per activation" |
| unbounded await recursion | boundary | `Await(Call(spin, []))` | `RuntimeError::MemoryExhausted` | 06 Activations and memory: "Memory exhaustion is the host event" |
| `ATK-interp-await-in-pure-frame` | negative | an `Await` in a `Pure` function, in a set made by the test-only unchecked constructor | the trap `@runtime.invalidCheckedProgram` | 06 Cold plans: "It never runs inside a `fn`" |
| a plan as a result | negative | `run` of an entry that returns a plan | `RuntimeError::UnsupportedValue` | 06 What has no encoding: "a plan … have no value encoding" |
| a host body without a context | boundary | a `Host` body reached under `run_test` | `RuntimeError::Unavailable` | 07: `@tool.unavailable` for a construct outside the profile |
| plan memory is accounted | boundary | 1,000 live plans against 1,010, with a budget between the two footprints | the smaller completes, the larger exhausts | 07 Required coverage 13: "accounted at counts that differ by ten" |

## Diagnostics and schema changes

None. `RuntimeError` is unchanged.

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

Read: all exit 0. The two depth rows run on a spawned thread with a
256 KiB stack; a stack overflow there aborts the test binary, which reads
as a failure.

## Stays unavailable

A host body (`RuntimeError::Unavailable`); grant values; the three
narrowing rows.

## Excluded

Host dispatch and grants (20); the test runner (07).

## Completion evidence

- [ ] Each of the 16 rows is a test, named in the report.
- [ ] `git grep -n "unavailable_plan" crates/vibra-interp` finds nothing.
- [ ] `plan.rs` contains no function that calls itself: the report shows
      the review of each `fn`.
- [ ] The validation commands exit 0.
