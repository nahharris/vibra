# Step 5 — patterns and exhaustive `match`

Prerequisite: Step 4 merged. Stage 3A behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Control flow and failure**,
  **Namespaces and resolution** (binder rules).
- [Source](../../spec/01-source-language.md): **Functions and expressions**.
- [Runtime](../../spec/06-runtime.md): **Evaluation** (`match`).
- [Decision ledger](decision-ledger.md) rows D6.3, D8.1.

## Scope

Literal, declared constructor (record, tuple, enum, union, wrapper), anonymous
`tupleof`, `recordof`, and `enumof`, and array patterns in
`let`, fixed positional parameters, lambda parameters, and `match`; one
exhaustiveness engine answering both "does this arm set cover the type" and
"is this single pattern irrefutable"; unreachable-arm detection; the canonical
uncovered-shape note. `as` patterns stay unavailable until Step 6.

## Design constraints

Use a usefulness algorithm over constructor spaces (Maranget-style) rather
than per-form special cases, so union members (Step 6) and later nominal forms
join as new constructor spaces. Types with unbounded value spaces (`str`,
numbers, `bytes`, `atom`) are infinite spaces that only a binder or discard
covers. Record patterns omit fields as wildcards. Binder no-shadowing and
duplicate-binder rules reuse the resolver's lexical scopes; resolution already
owns `@name.redeclaration`.

## Test matrix

- Positive: every pattern form in `match`; destructuring `let`, parameter, and
  lambda; single-variant enum constructor pattern accepted in `let`; nested
  patterns; all three discards repeated.
- Negative: non-exhaustive enum, `bool`, tuple, `atom`, and `str`
  (`@pattern.non-exhaustive` with the specified first shape); duplicate arm and
  arm after a binder (`@pattern.unreachable-arm`); refutable `let`, parameter,
  and lambda patterns (`@pattern.refutable-binding`); pattern type disagreeing
  with the scrutinee (`@type.mismatch`); binder shadowing and duplicate binder
  (`@name.redeclaration`); `(bind ...)` rejected.
- Interpreter: first matching arm selected; subject evaluated once.

## Done

Inventory rows `ExpressionKind::Match` and the four admitted pattern kinds
reference cases; M2 rows C1.5 (patterns) and C5.2 are implemented; validation
passes.
