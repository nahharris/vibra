# Step 6 — immutable values, bindings, and control flow

Requires Step 5 and C4/C5. Read source **Functions and expressions**, types
**Namespaces and resolution**, **Inference and checking**, **Control flow and
failure**, **Value semantics**, runtime **Evaluation**:
[source](../../spec/01-source-language.md), [types](../../spec/02-type-system.md),
[runtime](../../spec/06-runtime.md).

## Implementation sequence

1. Add typed `def` initialization using C5's dependency/order/cycle contract.
   Keep checking separate from initialization. A query or `check` never runs
   constant initializers to infer their types.
2. Resolve direct local binders and all three discards in fixed parameters and
   `let`. Validate visible scopes before adding named bindings. Resolve the
   initializer in the correct outer scope; do not accidentally enable self-use.
3. Check `let`, `do`, and body sequences against written expected types. Empty
   sequences follow C5; discarded values are still evaluated exactly once.
4. Require a boolean `if` condition; propagate a written expectation into both
   branches. Without one, require the specified identical type, not a computed
   union, numeric promotion, or truthiness.
5. Lower immutable slots, sequencing and branches to IR; execute only the selected
   branch. Add nonrecursive fixed positional calls to exercise bound values;
   share their binding representation with Step 7, not a second call checker.

| Positive | Negative / boundary |
| --- | --- |
| Constants with legal forward dependencies per C5 | Direct/indirect initializer cycles, mismatch, invalid initialization order |
| Nested binders with distinct names; sibling reuse | Shadowing parameter, visible local, module name or alias as C3 specifies |
| Repeated `-`, `@-`, `-:` in fixed parameters and nested `let` | Discard lookup/query must not resolve an identity |
| Empty/nonempty `do`, `let`, body sequences | Use before binding; escaped local; missing binding input |
| Boolean conditions, equal branch types, written numeric expectations | Non-bool condition, incompatible branch/result types, numeric ambiguity |
| Selected branch and initializer execute once | Host evaluator instrumentation proves untaken branch is not evaluated |

Literal/constructor/destructuring patterns remain deferred as C1 specifies;
never treat a constructor spelling as a new local binder. Retain M1 retired
form errors. No assignment, loops, `return`, `match`, or `try` implementation.
Calls requiring recursive execution remain unavailable until Step 9 delivers
the tail-call obligation; resolving their names earlier is not execution support.

Run [common validation](validation.md), then focused resolve/types/interpreter
tests. Done requires `V1-TYPE-NAMES-*`, `V1-TYPE-CONTROL-*`, and `V1-RUNTIME-*`
cases asserting scope identities, exact values, empty traces, and rejection
before execution. Step 1 owns any missing condition/mismatch/cycle code.
