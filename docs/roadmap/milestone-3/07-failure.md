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
