# Step 05: proc-and-plan

Kind: vertical. Slice 1, first part: a named `proc` with the empty row,
application that builds a plan, `await`, and guards on a procedure, checked
and lowered. Running them is step 06; test bodies are step 07.
Depends on: 04.
Crates: `vibra-types`, `conformance/cases`.

## Prerequisites

- Read: [source chapter](../../../../spec/01-source-language.md#fn-and-proc)
  "`fn` and `proc`", "`return`, `try`, `await`", "Bodies and statements";
  [type chapter](../../../../spec/02-type-system.md#application)
  "Application", "Functions as values", "Guards and callable types";
  [effects chapter](../../../../spec/03-effects.md#cold-plans-and-performed-rows)
  "Cold plans and performed rows"; [questions](../spec-questions.md) Q3,
  Q4.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-types/src/check/declare.rs:1657` | `fn function_headers` | calls `effects::procs::header` (step 04) |
| `crates/vibra-types/src/check/expr.rs:1368` | `ExprForm::Await` | calls `effects::procs::await_expr` |
| `crates/vibra-types/src/check/lower.rs:1168` | `fn assemble` | calls `effects::procs::lower` |
| `crates/vibra-ir/src/types.rs:319` | `pub struct ProcType` | the type a `proc` header gets |
| `crates/vibra-ir/src/expr.rs:364` | `pub enum Expr` | `Await`, from step 03 |

Proposed: the bodies of `effects::procs::{header, apply, await_expr,
lower}`.

## Files owned

- `crates/vibra-types/src/check/effects/procs.rs`
- `crates/vibra-types/src/check/tests/procs.rs`
- `conformance/rules/m2-05.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `header`: a module-level `proc`, and a `proc` member of a `type`, with
   no `performs` or with `performs {}`, gets the type
   `Proc({…} R {})`. Type parameters, `given`, labelled parameters with
   defaults, and parameter patterns are those of a `fn`. A non-empty
   `performs` stays `@tool.unavailable` at the section (step 08).
   Invariant: a `proc` is never given a `Fn` type.
2. `apply`: an application whose callee has a `Proc` type has the type
   `Intent(R {Row})`, kind `@procPlan`; arguments are checked as for a
   function. A bare `requires` of the callee states its obligation here.
   Invariant: no `Await` is lowered for an application.
3. `await_expr`: valid in the body of a `proc`, at any depth of `if`,
   `match`, `do`, and `let`, and not inside an anonymous `fn` written
   there. Its operand MUST have an `Intent` type; the expression has that
   type's result. Otherwise one `@effect.invalidAwait` at the `await`
   (Q4). Invariant: a wrong position reports nothing about the operand.
4. A statement whose type is an `Intent` is `@type.mismatch` (Q3).
5. `lower`: a `proc` lowers to a `CheckedFunction` of its `Proc` type with
   its guards exactly as a `fn`'s; `await e` lowers to `Expr::Await`; a
   `return await f(x)` is a tail position for the awaited call's entry.
   Invariant: the lowered set passes `vibra-ir::effects`.
6. `@types.v2` lists each `proc` with its callable type.

## Positions

| Rule | Positions, each with a positive and a negative row |
| --- | --- |
| `await` is valid | `proc` body; inside `if`, `match`, `do`, and a `let` initializer of a `proc`; against: `fn` body, module-value initializer, an anonymous `fn` in a `proc`, a `requires` predicate of a `proc`, a labelled default of a `proc` |
| the operand is a plan | a direct application; a `let`-bound plan; against: an integer, a function value, a `Result` holding a plan |
| application builds a plan | callee a module-level `proc`; a `proc` member of a type; a generic `proc`; against: wrong arity, a wrong argument type |

## Test matrix

Every case is `workspaceCheck`, profile `staticV2`, one module
`src/app/main.vib`. `P` abbreviates:

```vibra
proc double takes {n: I32} returns twice: I32 do
  return I32.add(n n)
endproc
```

| Case id | Kind | Input, after `P` | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-EFFECT-proc-await-direct` | positive | `proc run takes {n: I32} returns r: I32 do return await double(n) endproc` | accepted; `@types.v2` lists `double` as `Proc({I32} I32 {})` | 03 Cold plans: "`await plan` runs the plan and yields its result" |
| `V2-EFFECT-proc-plan-bound` | positive | `proc run takes {n: I32} returns r: I32 do let plan be double(n) endlet return await plan endproc` | accepted | 03 Cold plans: "A plan can be stored, passed, returned, and awaited any number of times" |
| `V2-EFFECT-proc-await-twice` | positive | the same with `let a be await plan endlet return I32.add(a await plan)` | accepted | the same |
| `V2-EFFECT-await-in-if` | positive | `proc run takes {n: I32 go: Bool} returns r: I32 do if go then return await double(n) endif return n endproc` | accepted | 03 Cold plans: "valid only in a `proc` body" |
| `V2-EFFECT-await-in-match-and-do` | positive | an `await` in a `match` arm and in a `do … enddo` initializer | accepted | the same |
| `V2-EFFECT-proc-member-of-type` | positive | `type Box of {n: I32} proc twice takes {self: Self} returns r: I32 do return await double(self.n) endproc endtype` | accepted | 01 Grammar: `type-item` admits `proc-decl` |
| `V2-EFFECT-proc-generic` | positive | `proc same(T) takes {value: T} returns r: T do return value endproc` awaited at `I32` | accepted | 01 Grammar: `proc-decl` has `type-params` |
| `V2-EFFECT-proc-labelled` | positive | `proc scale takes {n: I32} labelled {by: I32 be 2i32} returns r: I32 do …` awaited with and without `by:` | accepted | 01 `fn` and `proc`: "`labelled` lists optional parameters" |
| `V2-EFFECT-proc-recursive` | positive | `proc down takes {n: I32} returns r: I32 do if Ordered.greater(n 0i32) then return await down(I32.sub(n 1i32)) endif return 0i32 endproc` | accepted | 03 Cold plans: "Named procedures may recurse" |
| `V2-EFFECT-await-in-module-value` | negative | `let x: I32 be await double(1i32) endlet` at module level | `@effect.invalidAwait` at the `await` | 03 Effect ceilings: "A module-value initializer is pure" |
| `V2-EFFECT-await-in-anon-fn` | negative | in a `proc`: `let f be fn takes {n: I32} returns r: I32 do return await double(n) endfn endlet` | `@effect.invalidAwait` at the `await` | 01 `return`, `try`, `await`: "valid only inside a `proc`, named or anonymous, or a `test`" |
| `V2-EFFECT-await-in-requires` | negative | `proc run takes {n: I32} returns r: I32 requires Ordered.greater(await double(n) 0i32) else 0i32 do return n endproc` | `@effect.invalidAwait` at the `await` | 03 Cold plans: "valid only in a `proc` body" |
| `V2-EFFECT-await-in-default` | negative | `proc run takes {n: I32} labelled {by: I32 be await double(1i32)} …` | `@effect.invalidAwait` at the `await` | the same |
| `V2-EFFECT-await-not-plan` | negative | in a `proc`: `return await 1i32` | `@effect.invalidAwait` at the `await` | 07: "`await` … of a value that is not a plan" |
| `V2-EFFECT-await-function-value` | negative | in a `proc`: `return await double` | `@effect.invalidAwait` at the `await` | the same |
| `V2-EFFECT-await-in-fn-not-plan` | negative | `fn f returns n: I32 do return await 1i32 endfn` | exactly one `@effect.invalidAwait` (Q4) | 07, the same row |
| `V2-EFFECT-plan-statement-unawaited` | negative | in a `proc`: the statement `double(1i32)` then `return 0i32` | `@type.mismatch` at the statement (Q3) | 03 Cold plans: "Applying a `proc` does not run it" |
| `V2-EFFECT-plan-discarded` | positive | `let _ be double(1i32) endlet` | accepted | 01 Bodies and statements: "`let _ be risky() endlet` discards a value explicitly" |
| `V2-EFFECT-proc-result-is-plan` | negative | `fn f takes {n: I32} returns r: I32 do return double(n) endfn` | `@type.mismatch` at the returned expression | 02 Application: a `Proc` value applied gives "an `Intent`" |
| `V2-EFFECT-proc-argument-mismatch` | negative | in a `proc`: `return await double("x")` | `@type.argumentMismatch` at `"x"` | 02 Application: "An argument that does not fit its slot" |
| `V2-EFFECT-proc-arity` | negative | `return await double(1i32 2i32)` | `@type.argumentMismatch` | the same: "a wrong arity" |
| `V2-EFFECT-proc-missing-return` | negative | `proc run returns r: I32 do let _ be double(1i32) endlet endproc` | `@type.missingReturn` | 01 `return`: "Every path through a callable body … MUST end in `return`" |
| `V2-CONTRACT-proc-requires-at-build` | positive | `proc half takes {n: I32} returns r: I32 requires Ordered.greater(n 0i32) do return n endproc` and a `fn` that builds `half(2i32)` | accepted: the obligation is discharged in the `fn` | 03 Cold plans: "A bare `requires` on a `proc` is discharged where the plan is built" |
| `V2-CONTRACT-proc-requires-refuted-at-build` | negative | the same, building `half(0i32)` | `@contract.refuted` at the predicate, related to the argument | the same |
| `V2-CONTRACT-proc-requires-else` | positive | `proc half … requires Ordered.greater(n 0i32) else 0i32 do …`, built with an unknown `n` | accepted | 03 Cold plans: "A `requires … else` and an `ensures … else` run on each `await`" |
| `V2-TOOL-unavailable-proc-row` | boundary | `proc p performs {Missing} do return void endproc` | `@tool.unavailable` at the `performs` section; verdict `unsupported` | 07 Runtime and tool failures: "a valid … construct outside the tool's profile" |

`V2-TOOL-unavailable-proc` of M1 is deleted: its form is supported.

## Diagnostics and schema changes

Codes: `@effect.invalidAwait`, `@type.mismatch`, `@type.argumentMismatch`,
`@type.missingReturn`, `@contract.refuted`, `@tool.unavailable`. Schema:
`@types.v2` callables now include procedures.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-workspace -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; the corpus gains the 26 cases above and loses one.

## Stays unavailable

`@tool.unavailable`: a non-empty `performs`; a `proc` member of an
interface or an `impl`; a written `Proc` or `Intent` type; an anonymous
`proc`; a `proc` named as a value; a test that awaits. Applying a `proc`
at run time is still `RuntimeError::Unavailable` until step 06.

## Excluded

Rows (08), ceilings (09), callable types with rows and proc values (10),
execution (06, 07).

## Completion evidence

- [ ] `conformance/rules/m2-05.tsv` has one `active` row per case id.
- [ ] Every row of "Positions" has its positive and negative case; the
      report maps each.
- [ ] `git diff --stat` names no file of `check/` outside `effects/` and
      `tests/`.
- [ ] The validation commands exit 0.
