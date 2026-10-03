# Step 6 — calls, closures, generics, tail calls, and depth

Prerequisite: Step 5b merged. Stage 4A behavior step, WebAssembly backend. It
claims the Wasm halves of two exit clauses: deep non-tail recursion has the
same specified outcome in both backends, and a tail-recursive loop that
allocates a fresh compound value per iteration holds a bounded live arena.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **Tail calls**, **Activations and
  memory**, **Reclamation**, **Generic instantiation**, **Evaluation**.
- [Types](../../spec/02-type-system.md): **Functions as values**, **Generics**.
- [M4 ledger](decision-ledger.md) rows D1.1, D1.2, D3.2, D9.2.

## Scope

Calls of every kind and the activation machinery that Step 5b only began:
generic instantiation by run-time type arguments, function values, lambdas and
closures with captures, indirect calls through a function table, a tail call to
every kind of callee, the deep non-tail recursion outcome, and a bounded live
arena across a long allocating tail loop.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-ir/src/lib.rs`: `CallTarget`, `Expr::Closure`, `Expr::Function`, `Expr::Call` and its `tail` flag | The shapes to lower; the checker already marks tail position, and `validate_tail_calls` is the invariant to preserve |
| `crates/vibra-wasm` | The dispatcher, the frame replacement for a tail call, the function table, and the type-argument passing convention |
| `crates/vibra-interp/src/lib.rs`: `Callable`, `TailTransferAction` | The behavior to match, kind by kind |
| The `V1-RUNTIME` tail-call, generic, lambda, and closure cases under `conformance/cases/` | The cases that move |

## Ordered tasks

1. Type arguments at run time: one function per source function, each taking the
   type arguments of its activation as a value ([ledger D9.2](decision-ledger.md)),
   so a value built in generic code carries its instantiated type.
2. Function values and the table: named functions, constructors as values where
   the checker admits them, and `call_indirect` guarded by the recorded
   signature; a mismatch is the toolchain defect, never a program result.
3. Closures and lambdas: captures by value with `dup`, type arguments of the
   creating activation, and a generic `lambda`'s own arguments at each call.
4. Tail calls: the callee replaces the current frame, with its own captures and
   type arguments, for a module function of any module, a method, a lambda, a
   function value, and (in Step 9) a contract member. No Wasm call is made per
   language activation, so no engine feature is needed.
5. Depth: a non-tail recursion a million deep completes; one with no base case
   exhausts the runner's memory limit and is reported as the host event, in both
   backends against one `expect.host_event`.
6. Bounded arena: the allocating tail loop measured through
   `vibra_v1_live_size` at two iteration counts a factor of ten apart; add the
   case that asserts the difference is within the constant the case states.
7. Move the matched cases.

Invariants preserved: an application evaluates its callee once before its
operands; a tail call reuses the activation, including from `return`; generics
stay observable; engine stack use is independent of language depth.

## Test matrix

- Positive: each callee kind in tail and non-tail position; a closure capturing
  across two lambdas; a generic function called at two instantiations; mutual
  recursion; a loop of a million tail calls through each kind of callee.
- Negative: no new source diagnostic; the exhaustion case and its host-event
  expectation; a forged indirect-call signature in a host test is a defect.
- Recovery: after an exhaustion stop, a new instance runs the same program with
  a larger limit.
- Boundary: zero and many captures; a closure capturing a closure; a tail call
  whose callee has more locals than its caller; recursion exactly at the memory
  limit.
- Formatter: no change.

## Diagnostic and schema changes

None.

## Validation

The [Step 4–12 row](validation.md#focused-checks) and the full
[pre-merge list](validation.md#before-merging-each-step). Record the two live
sizes, the depth reached, and the engine's peak stack.

## Excluded

Contract members and interface values (Step 9); patterns (Step 7); any monomorphization or
tail-call proposal use (M7); borrow inference.

## Completion evidence

The two exit clauses named above, in the
[coverage table](README.md#deliverable-and-gate-coverage), have a Wasm half that
passes next to Step 3's interpreter half; the bounded-arena numbers are in the
handoff.
