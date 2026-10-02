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

## Delivery notes

- The reader accepts atom patterns (`@name`): the spec grammar has always
  counted `atom-name` as a `literal`, and D8.1 needs atom arms. The old
  `V1-SRC-EXPR-pattern-atom-rejected` case is gone. `PatternKind::Atom` has its
  own inventory row.
- Checked patterns are `vibra_ir::Pattern` (wildcard, binder, literal,
  variant, record, tuple, wrapper, array). `Expr::Match` evaluates its subject
  once and runs the first matching arm; destructuring `let`, positional
  parameters, and `lambda` parameters lower to a single-arm `match`, so the
  interpreter and every IR analysis have one form to handle. Parameter
  binders take the slots after every parameter slot.
- `crates/vibra-types/src/pattern.rs` is the single usefulness engine
  (Maranget). `bool`, enums, and `void` are finite spaces; records, tuples,
  and wrappers have one constructor; arrays (by length) and the primitive
  scalars are infinite. The non-exhaustive and refutable-binding notes spell
  the first uncovered shape, with concrete constructors for finite spaces and
  `-` where only a binder or discard covers the rest (the spec now says so).
- An unreachable arm relates the earliest single arm that covers it; a
  duplicate of an arm covered only by several earlier arms relates none.
- Pattern types must match exactly: a constructor pattern of another declared
  type, an anonymous pattern over a mismatched shape, and a literal of another
  type are `@type.mismatch`. Arity and label errors reuse the constructor
  diagnostics (`@type.argument-mismatch`, `@type.unknown-record-field`).
- `as` patterns reported `@tool.unavailable` until Step 6; the case that
  pinned it, `V1-TYPE-CONTROL-availability-patterns`, was removed when Step 6
  landed them.
- Subject-once evaluation is structural in the interpreter (the subject is
  evaluated before any arm is tried) but not observable in the pure
  `interpreter-v1` profile; M4 effects make it observable.
