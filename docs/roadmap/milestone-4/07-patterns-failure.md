# Step 7 — patterns and typed failure

Prerequisite: Step 6 merged. Stage 4A behavior step, WebAssembly backend.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **Evaluation** (`match`, `let-else`,
  `try`, `as`), **Tail calls** (arm results and `let-else` fallbacks), **Canonical
  value encoding**.
- [Types](../../spec/02-type-system.md): **Control flow and failure**,
  **Body sequences and bindings**, **`return`**, **The `never` type**, **Type
  ascription and widening**, **Nominal declarations** (unions).
- [M4 ledger](decision-ledger.md) rows D8.1, D12.1.

## Scope

`match` with every pattern kind (binding, literal, atom, constructor, tuple,
`recordof`, `enumof`, `as`), destructuring in `let`, parameters, and lambdas,
`let-else`, `as` narrowing of a union by discriminant, `try` over `option` and
`result`, and the `never` type (a fallback that never completes). Array patterns
need arrays and are Step 8b's.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-ir/src/pattern.rs`, `Expr::Match`, `Expr::Try`, `Expr::LetElse` (or its lowered shape) | The shapes to lower; `option` and `result` are recognized by canonical identity |
| `crates/vibra-wasm` | A decision-tree lowering of the first matching arm, discriminant comparison for `as`, and early exit for `try` that drops the frame's live locals |
| `crates/vibra-interp/src/lib.rs` | The reference behavior, arm by arm |

## Ordered tasks

1. Refutable tests per pattern kind and the binder extraction, with exactly one
   evaluation of the subject and one `dup` per binder.
2. `match` as an ordered arm sequence with the first match winning, and arm
   results in tail position when the `match` is.
3. `let-else`: bind for the rest of the sequence on a match, otherwise run the
   fallback, whose type is `never`; the fallback is a non-tail position.
4. `as` narrowing: compare the written-order discriminant and yield the payload;
   the expression form is erased, as the runtime chapter requires.
5. `try`: inspect the value, continue with the payload, or leave the activation
   with the early exit; the operand is never a tail position.
6. `never`: no value, no slot, no representation; a body ending in `never`
   returns nothing.
7. Move the matched cases.

Invariants preserved: arms are tested in order; a binder is `dup`ed once and its
owner dropped once; `try` performs only its early exit; no representation of
`never`.

## Test matrix

- Positive: every pattern kind in `match`, `let`, parameters, and lambdas;
  nested patterns; an exhaustive union `match`; `let-else` as the final element;
  `try` over both containers in matching enclosing results; a `return` and a
  `let-else` fallback that is a call of a `never` function.
- Negative: no new source diagnostic; unlowered array patterns stay
  `not-lowered` until Step 8b.
- Recovery: a failed arm followed by a covering arm.
- Boundary: an empty-payload variant, a single-arm `match`, a union of many
  members, a `try` in tail-adjacent position, deeply nested destructuring.
- Formatter: no change.

## Diagnostic and schema changes

None.

## Validation

The [Step 4–12 row](validation.md#focused-checks) and the full
[pre-merge list](validation.md#before-merging-each-step).

## Excluded

Array and dict patterns and lookups (Step 8b); interfaces (Step 9); an effect
ceiling on a `let-else` fallback or `return` operand (Step 14).

## Completion evidence

Every `Lowered` row owned by Step 7 in the [inventory](supported-surface.md) has
a matched case; the parity inventory only grew.
