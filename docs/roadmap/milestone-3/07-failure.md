# Step 7 — `result`, `try`, and unhandled failure

Prerequisite: Step 6 merged. Stage 3A behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Control flow and failure**.
- [Source](../../spec/01-source-language.md): **Functions and expressions**
  (`try`).
- [Runtime](../../spec/06-runtime.md): **Evaluation**, **M3 compiler intrinsic
  registry** (`@std.core`, `@std.result`).
- [Decision ledger](decision-ledger.md) rows D4.1, D4.4, D9.1, D9.2.

## Scope

Add `@std.core` (`ordering`, `arithmetic-error`, `conversion-error`) and
`@std.result` to the standard-library input. `try` over `option` and `result`
with the enclosing-context rules; early exit from the innermost function,
`lambda`, or test body, including from a tail position without breaking the
Step 9 M2 tail-call guarantee; `@type.unhandled-fallible` at ignored `result`
positions and acceptance under each discard spelling.

Per ledger D17.1–D17.2 (Step 4a), `result` is an ordinary standard-library
`deftype` in `@std.result` that claims the `@result` role, exactly as Step 4b made
`option` claim `@option`; `try` and unhandled-failure checking bind the role and
never the definition.

## Test matrix

- Positive: `try` in a function and a `lambda` returning `option` and
  `result`; differing success types; nested `try`; a tail-recursive loop that
  exits through `try` without growing activation depth.
- Negative: mismatched error type, non-container operand, no enclosing
  function, and a test body (`@type.invalid-try`); ignored `result` in a
  non-final `do`, `let`, and function position (`@type.unhandled-fallible`).
- Accepted: the same ignored values under `-`, `@-`, and `-:`; an ignored
  `option`.

## Done

Inventory row `ExpressionKind::Try` references cases; M2 row C1.5 is fully
implemented; validation passes.

## Delivery notes

- `@std.result` declares `result` claiming `@result`, and `@std.core` declares
  `ordering`, `arithmetic-error`, and `conversion-error`. Both are in the
  manifest and embedded. Every run declares the types of `@std.option`,
  `@std.result`, and `@std.core` it has not declared itself, so the role types
  need no import and `(import core @std.core)` works in both check paths.
- `try` lowers to `Expr::Try`, which carries the enclosing written result
  type. The interpreter unwinds a failing `try` to the innermost function or
  `lambda` boundary, which returns `none` or the same `err` rebuilt at that
  type. A tail-recursive loop that exits through `try` keeps its constant
  activation depth (`try_m3_step7`).
- The checker tracks the enclosing written result type and its span (the
  reader now records `result_span` for `defn` and `lambda`). A test body's
  result is `void`, and a `def` initializer has none. `@type.invalid-try`
  relates the written result type when there is one.
- `@type.unhandled-fallible` is reported at every non-final element of a body
  sequence whose type plays `@result`. `(let - …)` and the other discard
  spellings bind the value, so they are not ignored positions. `option` is not
  fallible.
- A void payload is written without an operand, `(result.ok)`, as for every
  enum.
- An entry returning `(result void e)` now checks and runs. The command
  result set has no atom for a program that returned `err`, so `vibra run`
  still reports `@command.ok`. README gap G21 tracks the tooling decision.
