# Step 21: guards-with-else

Kind: vertical. Slices 7 and 9, first half: the certified fragment, guards
with consequences (`requires … else`, `ensures … else`, `where … else`),
fallible construction through `Result`, and compile-time evaluation of
constant constructions through the interpreter under tool limits. Bare
guards are rejected as unproved in this step and discharged in step 22.

## Prerequisites

- Step 20 landed.
- Read: [type chapter](../../../../spec/02-type-system.md) "Type invariants",
  "Guards" ("Predicates", "Guards with `else`"), "Constant expressions and
  constant patterns"; [runtime chapter](../../../../spec/06-runtime.md)
  "Callable contract execution"; [tooling
  chapter](../../../../spec/05-tooling.md) "Versioned envelope and results" (the
  `limits` field); [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md) "Contract, effect
  and authority diagnostics", "Runtime and tool failures" (tool limits);
  [programs chapter](../../../../spec/04-programs-and-packages.md) "Tests" (the
  `RelativePath` example and its explanation).

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-types/src/{declare,expr,lower}.rs` | extend |
| `crates/vibra-interp/src/evaluate.rs` (step 13) | compile-time evaluation |
| `crates/vibra-ir/src/{nominal.rs,program.rs}` (`Invariant`, `Guard`) | lowering targets |

Proposed: `vibra-types::{contract, ctfe}`; `contract::fragment` (the
certified check), `contract::obligations` (collection and statuses),
`ctfe` (evaluation requests with limits).

## Ordered tasks

1. Certified fragment: a predicate or consequence is built from parameters,
   constants, constructions, field reads, `if` and `match` with full
   coverage, the primitive comparison and `Bool` operations, and calls of
   certified functions. A certified function is a `fn` whose body is
   certified and that is not recursive, directly or through other
   functions; a predicate that calls a recursive `fn` is
   `@contract.unsupportedPredicate` at the call. Any other expression
   outside the fragment is `@contract.unsupportedPredicate` at the
   expression. A `where` operand is an expression like any other function
   value: an anonymous `fn`, or the name of a `fn`, which resolves in the
   flat module scope and may be declared after the type. A predicate not
   of type `Bool`, or a `where` operand not a
   `Fn({R} Bool)`, is `@contract.invalidPredicate`; a consequence of the
   wrong type, or a `where … else` consequence that is not a constant
   expression, is `@contract.invalidConsequence`; a `where` on a variant
   is `@type.invalidInvariant`.
2. `requires C else V`: `V` at the callable's result type; no caller
   obligation; lowered to `entry_guard`. `ensures C else V`: the result
   name bound to the candidate in both `C` and `V`; lowered to
   `exit_guard`. A declaration has at most one of each (reader). An
   implementation member declares no guard (the reader rejects the
   section, step 07); a contract member's guard with `else` is copied into
   each implementation's checked function, so static and dynamic dispatch
   run it alike.
3. `where P else E`: the constructor's result type becomes
   `Result(T ErrorType)`; patterns on the type are infallible; every
   construction of the type yields a `Result` even when folded.
4. Compile-time evaluation: a construction with constant operands of a type
   whose invariant has a consequence is evaluated through
   `vibra-interp::evaluate` under the published limits; `Result.ok(value: x)`
   over it is irrefutable only when the evaluation returns `ok`; a
   `LimitReached` result makes the pattern refutable and is reported as
   `@tool.limitExceeded` alone, at the predicate;
   an evaluation that returns `err` makes a `let Result.ok(…) be …` without
   `else` `@pattern.refutableBinding` and the construction itself is a legal
   `Result.err` value. A guard with `else` whose predicate evaluates to a
   constant is folded: the check and the consequence are removed when
   `true`, the consequence selected when `false`; operands are still
   evaluated at run time.
5. Bare guards: collected as obligations with the status `@unknown` and
   reported `@contract.unproved` at the predicate (discharge rules are
   22); the `@types.v2` `obligations` list gains one obligation record
   per bare guard obligation, sorted by `declaration`, then `span`, with
   `declaration`, `kind` (`@requires`, `@ensures`, or `@where`), `span`
   (the predicate's span record), `status`, `rules` (empty in this step),
   `dependencies`, and no `counterexample`. A guard with `else` has no
   obligation record.
6. Interpreter: already executes consequences (13); add the `evaluate`
   plumbing and host tests for folded programs.
7. Cases.

## Test matrix

| Case id | Kind | Input | Expected |
| --- | --- | --- | --- |
| `V2-CONTRACT-requires-else-true` | positive, interpret | the `splitCents` example (`requires Ordered.greaterOrEqual(parts 1i32) else Result.err(error: SplitError.noParts())`) called with `parts` 2 | `Result.ok(value: …)` |
| `V2-CONTRACT-requires-else-false` | positive, interpret | the same with `parts` 0 | `Result.err(error: SplitError.noParts())`; the body is skipped (host test: no trace) |
| `V2-CONTRACT-requires-else-skips-ensures` | positive, interpret | a `fn` with both `requires … else` and `ensures … else`, entered with a false `requires` | the `requires` consequence; the `ensures` consequence is not evaluated (host test) |
| `V2-CONTRACT-requires-else-wrong-type` | negative | `requires c else "no"` on a `fn` returning `I32` | `@contract.invalidConsequence` at `"no"` |
| `V2-CONTRACT-requires-not-bool` | negative | `requires 1i32` | `@contract.invalidPredicate` at `1i32` |
| `V2-CONTRACT-requires-unsupported` | negative | `requires Equatable.equal(Iter.collect(xs) [])` | `@contract.unsupportedPredicate` at the `collect` call (not certified: it is not a certified function per the fragment) |
| `V2-CONTRACT-requires-uncertified-fn` | negative | `requires check(x)` where `check` uses a lookup (`items(0u64)`) | `@contract.unsupportedPredicate` at the call |
| `V2-CONTRACT-requires-certified-fn` | positive | `requires positive(x)` where `fn positive takes {x: I32} returns r: Bool do return Ordered.greater(x 0i32) endfn` | accepted as a predicate (discharged in 22; here with `else`) |
| `V2-CONTRACT-requires-recursive-fn` | negative | `requires even(x) else 0i32` where `even` and `odd` call each other | `@contract.unsupportedPredicate` at the call `even(x)` |
| `V2-CONTRACT-ensures-else` | positive, interpret | `fn clamp takes {x: I32} returns r: I32 ensures Ordered.lessOrEqual(r 10i32) else 10i32 do return x endfn` called with 3 and 12 | `3i32`, `10i32` |
| `V2-CONTRACT-ensures-else-candidate-bound` | positive, interpret | `ensures Ordered.less(r 10i32) else I32.add(r 1i32)` called with 10 | `11i32`: `V` sees the candidate and is not re-checked |
| `V2-CONTRACT-ensures-else-once` | positive, interpret | a predicate whose evaluation is observable only through a host trace | the host test asserts one evaluation |
| `V2-CONTRACT-ensures-result-name` | positive | `ensures Ordered.greater(r 0i32) else 1i32` | accepted; `r` resolves |
| `V2-CONTRACT-ensures-through-try` | positive, interpret | an `ensures … else` on a `fn` returning `Result` whose body leaves through `try` with an `err` | the candidate is the propagated `err`; the predicate runs on it |
| `V2-CONTRACT-where-else` | positive, interpret | `type Positive of I32 where fn takes {raw: I32} returns ok: Bool do return Ordered.greater(raw 0i32) endfn else PositiveError.notPositive() endtype`; `Positive(1i32)` and `Positive(0i32)` | `Result.ok(value: Positive(1i32))`, `Result.err(error: PositiveError.notPositive())` |
| `V2-CONTRACT-where-else-type` | positive | `let p: Result(Positive PositiveError) be Positive(x) endlet` | accepted; the constructor's type is the `Result` |
| `V2-CONTRACT-where-else-plain-type` | negative | `let p: Positive be Positive(x) endlet` | `@type.mismatch` |
| `V2-CONTRACT-where-else-nonconstant` | negative | `… else makeError(raw)` | `@contract.invalidConsequence` |
| `V2-CONTRACT-where-named-fn` | positive | `where isPositive else …` with a certified module `fn isPositive` declared after the type | accepted |
| `V2-CONTRACT-where-not-predicate` | negative | `where Ordered.greater(1i32 0i32)` (a `Bool`, not a `Fn({R} Bool)`) | `@contract.invalidPredicate` |
| `V2-CONTRACT-where-on-variant` | negative | `type S case a case b where … endtype` | `@type.invalidInvariant` at the `where` |
| `V2-CONTRACT-where-pattern-infallible` | positive, interpret | `let Positive(raw) be p endlet` with `p: Positive` | accepted without `else`; the predicate does not run again (host test) |
| `V2-CONTRACT-constant-construction-ok` | positive, interpret | `let Result.ok(value: p) be Positive(5i32) endlet return p` | accepted without `else` (rule 1 evaluation); `Positive(5i32)` |
| `V2-CONTRACT-constant-construction-err` | negative | `let Result.ok(value: p) be Positive(0i32) endlet` | `@pattern.refutableBinding` at the pattern; the construction is legal |
| `V2-CONTRACT-constant-construction-err-matched` | positive, interpret | `match Positive(0i32) case Result.ok(value: p) … case Result.err(error: e) return e endmatch` | the error value |
| `V2-CONTRACT-nonconstant-construction` | negative | `let Result.ok(value: p) be Positive(x) endlet` with `x` a parameter | `@pattern.refutableBinding` |
| `V2-CONTRACT-constant-limit` | boundary | a bare `where` whose predicate calls the last of thirty certified functions, each calling the one before it twice, so the evaluation needs more reductions than the limit profile `m1Default` allows; one constant construction | `@tool.limitExceeded` at the predicate and no `@contract.unproved`; the obligation's status is `@unknown`; never accepted |
| `V2-CONTRACT-folded-guard-evaluates-operands` | positive, interpret | `requires true else 0i32` on a `fn` whose argument expression builds a traced value | the body runs; the host test shows the operand was evaluated once |
| `V2-CONTRACT-bare-requires-unproved` | negative | `fn f takes {x: I32} returns r: I32 requires Ordered.greater(x 0i32) do return x endfn` called `f(y)` with `y` a parameter | `@contract.unproved` at the application, the declaration related (status `unknown`; discharged cases arrive in 22) |
| `V2-CONTRACT-obligations-snapshot` | negative, workspaceCheck | a library with one bare `requires` called once with a parameter, one `ensures … else`, one `where … else` | `@contract.unproved`; the `@types.v2` `obligations` list has one record, of kind `@requires` and status `@unknown` with empty `rules`; the guards with `else` have none |

## Diagnostics and schema changes

Codes: `@contract.unsupportedPredicate`, `@contract.invalidPredicate`,
`@contract.invalidConsequence`, `@type.invalidInvariant`,
`@contract.unproved`, `@pattern.refutableBinding`, `@type.mismatch`,
`@tool.limitExceeded`. Schema: `@types.v2` `obligations` populated.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-interp -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: all cases pass; the limit case reports `@unknown`; a host test asserts
`evaluate` is never called with a non-certified expression.

## Excluded

The seven discharge rules and the integer facts (22);
`@contract.obligationLost` (22); guards on `proc` (M2).

## Completion evidence

- [ ] `conformance/rules.tsv` has one `active` row per case id above.
- [ ] A host test enumerates the certified fragment's admitted node kinds and
      rejects each other kind.
- [ ] Validation commands exit 0.
