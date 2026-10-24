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

## As built

The step landed as one change on the mechanism of Steps 5b and 6 ([D2.15 and
D3.10](decision-ledger.md)); the design is written once in the documentation of
`crates/vibra-wasm/src/lower.rs` ("Patterns and typed failure") and
`crates/vibra-wasm/src/pattern.rs`.

- **Only `match` is lowered.** Typed IR writes a destructuring `let`, a
  destructuring parameter, a `lambda` parameter, and `let-else` as a `match`: one
  arm, or an arm and a wildcard fallback of type `never`. No form of the IR was
  added, and the emitter has no case of its own for `let-else`.
- **One recursive scheme over a table.** `pattern::decompose` maps a checked
  pattern at the type of the value it meets to a node: its **test** (none, the
  bits of a cell, the discriminant of an object, or equal characters or bytes),
  its **binder**, and its **children**, the parts its sub-patterns meet, each with
  a position, the cell count of its object, and a presence. The lowering tests a
  node, descends into its children, and binds a node the same way for every kind,
  and `classify` reads the same table to report what cannot be lowered, so no
  routine names a kind of pattern. A literal expression and a literal pattern read
  a literal through one classification (`pattern::literal`).
- **The shape of a `match`.** The subject is evaluated once into a slot. Each arm
  is a run of blocks: its tests, each of which jumps to the next arm on failure;
  its binders, each one `dup`; a drop of the subject; the arm's result into the
  destination; a drop of its binders; and a jump to the join. Past the last arm is
  the trap `@runtime.invalid-checked-program`, which the checker's exhaustiveness
  rules out and which no program reaches.
- **Tests read in place.** A part that is itself an object is borrowed into a
  temporary whose class byte is `0`, so a failed test and a leaving frame drop
  nothing they do not own. A scalar literal compares the low 32 bits of the cell
  (or all 64 for `i64`, `u64`, and `f64`), whatever the upper bits are; a `str`,
  atom, or `bytes` literal is built, compared with the new routine `equal` (the
  length and the payload bytes, naming no kind), and dropped; `bool`, whose
  patterns the checker writes as boolean literals, is the discriminant.
- **Presence.** An enum's payload of `void` has no cell, and a payload of a
  generic type has one only when its type argument is not `void`. The table
  records which of the two applies, and the object's length says at run time. A
  binder on a part with no cell binds `void`, as the reference interpreter does.
- **`as`.** The pattern is the discriminant test of the member's position in the
  union's member list, then the payload at the member type. In expression
  position `as` is the widening typed IR already carries, which adds nothing.
- **`try`.** The operand's discriminant is compared with the success variant
  (`some` or `ok`, found by name in the operand's shape as the interpreter reads
  it). It continues with a `dup` of the payload and a drop of the operand, or
  builds `none` or `err` again at the enclosing result type, from the same
  payload, drops the operand, and returns as `return` does. Its operand is not in
  tail position.
- **`never`.** No value, slot, or representation: an expression of that type
  writes no destination, and the block after it is reachable from no block. A
  `let-else` fallback that calls a function of type `never`, and the body of that
  function, are lowered and validate.
- **Reclamation.** Every path leaves the frame balanced ([D3.10](decision-ledger.md)):
  the arm of a `match`, a failed arm, a `try` that leaves, a `let-else` fallback
  that returns, and a `return` inside an arm all return the live size to what the
  module values keep, with no frame left. Typed IR numbers every binding slot of an
  activation once and never reuses one, so the drop of an arm's binders when its
  result is done is memory returned early and not a leak avoided; the frame's scan
  would drop them at the end.
- **Tail position.** The checker marks the calls of an arm, of the rest of a
  body after a `let-else`, and of a `return` operand, and nothing here changes it.
  A hundred thousand rounds of a loop through each hold the same live size, 10,304
  bytes (what the module values keep), and the same arena high-water mark as ten
  thousand (26,816, 26,816, 26,880, and 26,848 bytes for the arm, the `let-else`,
  the `try`, and the `return` loop), and run under 256 KiB. The same loop through
  the operand of a `try` is not a tail loop and exhausts that memory.
- **Evidence.** 37 host tests in `patterns_m4_step7` (about 20 s in a debug build;
  the loop tests take most of it, almost all of it checking the counter's source),
  six unit tests of the table in `pattern.rs`, and five cases that add executable
  parity:
  `V1-RUNTIME-literal-patterns`, `-constructor-patterns`, `-union-narrowing`,
  `-union-atom-member`, and `-try-and-never`. `V1-RUNTIME-constant-patterns` and
  `V1-RUNTIME-let-else` moved from `not-lowered` to `matched`.
- **The risk Step 5b noted.** An atom singleton does not reach a union's `atom`
  member by a widening of its own: the checker admits no chain, so
  `(defn main () tag @ok)` over `(union atom i32)` is `@type.mismatch`, and a
  singleton reaches the member only as `(as atom @ok)` or through a parameter or
  a result written `atom`, where the first widening, which is erased, comes
  before the second. The interpreter then records the static type of the
  operand of the union widening, which is `atom`, and Wasm reads `atom` from the
  union's member list at the discriminant, and an `as` pattern over the union
  narrows to the atom and compares it. The two agree on
  `an_atom_singleton_widened_into_a_union_with_an_atom_member` and on the case
  `V1-RUNTIME-union-atom-member`. No union type with a singleton member is
  writable (`@syntax.invalid-form`), and a union member must be a concrete type
  (`@type.union-member-not-concrete`), so generic code cannot widen a value whose
  type is a parameter into a union either: the singleton type has no other way
  into one.
- **Not lowered.** The array pattern (`array:pattern`) and a wrapper pattern over
  `str` or `bytes` (`wrap`), both Step 8b's; a program that needs them still
  reports them, with every other form it needs.
- **Left for later steps.** Every case that needs arithmetic, an array or dict,
  a contract call, or `workspace-test` still reports that form and keeps its
  owner: 8a, 8b, 8c, 9, and 11.
