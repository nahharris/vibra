# Step 18: check-callables

Kind: vertical. The callable half of slice 5: anonymous `fn` with
captures, `Fn` types, function values of nested members, labelled
parameters with constant defaults, generic functions and generic anonymous
callables, and the tail-call rule through every callee kind.

## Prerequisites

- Step 17 landed.
- Read: [type chapter](../../../../spec/02-type-system.md) "Callable types",
  "Functions as values", "Generics" (anonymous callables), "Constant expressions
  and constant patterns" (labelled defaults); [source
  chapter](../../../../spec/01-source-language.md) "`fn` and `proc`" (labelled,
  anonymous), "Application" (labelled arguments in any position); [runtime
  chapter](../../../../spec/06-runtime.md) "Generic instantiation", "Tail
  calls".
- Open questions: none.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-types/src/{expr,infer,lower}.rs` | extend with closures and labelled slots |
| `crates/vibra-types-v1/src/defaults.rs` | the constant-default check |
| `crates/vibra-ir/src/expr.rs` (`Closure`, `Captured`, `Default`) | lowering targets |

Proposed: `vibra-types::callables`.

## Ordered tasks

1. Labelled parameters: `labelled {name: T be default}`; the default MUST be
   a constant expression (`@type.notConstant`); labelled arguments bind by
   name in any position; missing labelled arguments take the default;
   duplicate or unknown labels `@type.argumentMismatch`. The type of a
   function with labelled parameters is `Fn({A B name: T} R)`; defaults are
   not part of the type.
2. Anonymous `fn`: an expression of callable type; it captures visible
   locals and module values by value; it cannot name itself; a generic
   anonymous callable bound by `let` stays generic, elsewhere it is
   instantiated from its written expected type (`@type.ambiguousInference`
   without one).
3. Function values: a module-level `fn` path or nested member path in
   expression position; constructors, field reads, and lookups are not
   values (`@type.mismatch` where a function is expected; an anonymous `fn`
   wraps them).
4. Generic functions: parameters inferred from operands and expected
   result; `given` bounds other than `Any` are checked in 19.
5. Tail calls through values, closures, and record fields in the
   interpreter; `Never`-typed calls.
6. Cases.

## Test matrix

| Case id | Kind | Input | Expected |
| --- | --- | --- | --- |
| `V2-SRC-CALLS-labelled-default` | positive, interpret | `fn pad takes {s: Str} labelled {width: U64 be 4u64} returns r: U64 do return width endfn` called `pad("x")` and `pad("x" width: 2u64)` | `4u64`, `2u64` |
| `V2-SRC-CALLS-labelled-any-position` | positive, interpret | `pad(width: 2u64 "x")` | `2u64` |
| `V2-SRC-CALLS-labelled-duplicate` | negative | `pad("x" width: 1u64 width: 2u64)` | `@type.argumentMismatch` |
| `V2-SRC-CALLS-labelled-unknown` | negative | `pad("x" depth: 1u64)` | `@type.argumentMismatch` |
| `V2-SRC-CALLS-default-not-constant` | negative | `labelled {width: U64 be compute()}` | `@type.notConstant` at `compute()` |
| `V2-SRC-CALLS-default-constant-construction` | positive | `labelled {mode: Shape be Shape.empty()}` | accepted |
| `V2-TYPE-NOMINAL-fn-type-with-labelled` | positive | `let f: Fn({Str width: U64} U64) be pad endlet` | accepted |
| `V2-TYPE-NOMINAL-fn-type-labelled-mismatch` | negative | `let f: Fn({Str} U64) be pad endlet` | `@type.mismatch` |
| `V2-SRC-EXPR-anon-fn-capture` | positive, interpret | `let base be 10i32 endlet let add be fn takes {x: I32} returns r: I32 do return I32.add(x base) endfn endlet return add(1i32)` | `11i32` |
| `V2-SRC-EXPR-anon-fn-no-self` | negative | an anonymous `fn` whose body names the `let` it is bound to | `@name.unknownSymbol` (resolution) |
| `V2-SRC-EXPR-anon-fn-generic-let` | positive, interpret | `let id be fn(T) takes {x: T} returns r: T do return x endfn endlet return {id(1i32) id("x")}` | `{1i32 "x"}` |
| `V2-SRC-EXPR-anon-fn-generic-argument` | positive | `apply(fn(T) takes {x: T} returns r: T do return x endfn 1i32)` where `apply` takes `Fn({I32} I32)` | accepted; instantiated from the parameter type |
| `V2-SRC-EXPR-anon-fn-generic-ambiguous` | negative | the generic anonymous `fn` as a statement's value with no expected type: `let _ be fn(T) … endfn endlet` | `@type.ambiguousInference` |
| `V2-TYPE-INFER-constructor-not-value` | negative | `let f: Fn({F64} Celsius) be Celsius endlet` | `@type.mismatch` at `Celsius` |
| `V2-TYPE-INFER-constructor-wrapped` | positive, interpret | `let f: Fn({F64} Celsius) be fn takes {v: F64} returns c: Celsius do return Celsius(v) endfn endlet return f(1.0f64)` | a `Celsius` |
| `V2-TYPE-INFER-member-value` | positive, interpret | `type U of {n: Str} fn len takes {u: Self} returns r: U64 do return Str.length(u.n) endfn endtype`; `let f be U.len endlet return f(U(n: "ab"))` | `2u64` |
| `V2-TYPE-GENERIC-fn-inferred-result` | positive, interpret | `fn none(T) returns r: Option(T) do return Option.none() endfn`; `let o: Option(I32) be none() endlet` | `Option.none()` typed `Option(I32)` in the snapshot |
| `V2-RUNTIME-tail-through-value` | positive, interpret | mutual recursion through a function value held in a record, 100,000 transfers | the result; depth 1 (host assertion) |
| `V2-RUNTIME-tail-through-closure` | positive, interpret | a closure calling itself through a parameter, 100,000 transfers | depth 1 |
| `V2-RUNTIME-closure-captures-value` | positive, interpret | a closure built in one scope and called after the scope ended | the captured value |

## Diagnostics and schema changes

Codes: `@type.argumentMismatch`, `@type.notConstant`, `@type.mismatch`,
`@type.ambiguousInference`, `@name.unknownSymbol`. No schema change.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-interp -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: all cases pass; the tail cases assert depth 1 in host tests.

## Excluded

Interfaces, dispatch, bounds, widening, conversion (19); guards on
callables and `@contract.obligationLost` (21, 22); `proc` (M2).

## Completion evidence

- [ ] Every row of "Applications and values" in the diagnostics chapter's
      required coverage that concerns functions has a case.
- [ ] Validation commands exit 0.
