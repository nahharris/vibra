# Step 9 — mandatory tail-call execution

Requires Step 8. Read runtime **Tail calls**, **Evaluation**, and types
**Functions as values** in [runtime](../../spec/06-runtime.md) and
[types](../../spec/02-type-system.md). Copy the normative recursive-group
definition accurately; do not substitute an optimization heuristic for it.

## Implementation sequence

1. Compute static function-call edges from resolved callable entities. Preserve
   the distinction between indirect function values and statically known calls.
   Use the specified same-module group relation, including mutual recursion.
2. Mark tail position relative to an activation. Only the final body/`do`/`let`
   expression and branches of a tail `if` inherit tail status. A nested final
   expression inside a non-tail operand is still non-tail.
3. Represent tail transfer in checked IR. Use an explicit continuation/activation
   stack or trampoline in the interpreter; replace the current activation for
   every required call instead of recursively calling a Rust evaluator.
4. Preserve captured environments and evaluation order across transfer. Do not
   retain obsolete activations through accidental parent-environment chains.
5. Add host-only counters for current/maximum language activation depth. Use
   C7's legal terminating source workload for at least 100,000 tail transfers;
   assert bounded depth as input increases, as well as the correct value.

| Required | Counterexample / boundary |
| --- | --- |
| Direct and same-module mutual tail recursion | Non-tail call whose result is consumed by another expression |
| Tail `if`, nested tail `let`/`do` | Condition, initializer, callee/argument position cannot become tail |
| Forward function references and retained closures | No capture use-after-pop or values overwritten by frame reuse |
| Source stress through real interpreter handler | Hand-built IR alone cannot prove frontend tail marking |
| Instrumented depth independent of transfer count | Successful completion alone cannot prove constant activation depth |

The 100,000-transfer test is an evidence workload, not a language execution
budget. External watchdog timeout is a failed/inconclusive test, never a Vibra
result. Non-tail recursion remains legal; no portable stack-limit claim is made.
Do not add tail syntax, source loops, fuel, Wasm, or optimizations unrelated to
the guarantee. `match`/`try` tail behavior remains with their later slices.

Run [common validation](validation.md) and focused IR/interpreter tests. Done
requires `V1-RUNTIME-*` source cases, depth measurements for two workload sizes,
negative tail-position tests, and unchanged values/audit traces.
