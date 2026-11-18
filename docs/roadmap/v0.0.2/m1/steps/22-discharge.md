# Step 22: discharge

Kind: vertical. Slices 7 and 9, second half: bare `requires`, `ensures`,
and `where` discharged by exactly the seven rules and the eight integer
facts, or refuted, or left unproved; guards on contract members; conversion
of guarded callables to callable types; the obligation records with their
rule identifiers.

## Prerequisites

- Step 21 landed.
- Read: [type chapter](../../../../spec/02-type-system.md) "Bare guards" (the
  seven rules), "Guards and callable types"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md) "Integer facts",
  "Contract, effect and authority diagnostics", "Required coverage" item 9;
  [tooling chapter](../../../../spec/05-tooling.md) "Obligation record";
  [runtime chapter](../../../../spec/06-runtime.md) "Callable contract
  execution" ("Bare guards leave no code").
- Open questions: Q21 (how the integer facts apply to the `Ordered` default
  members) must be answered before this step starts.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-types/src/contract/{fragment,obligations}.rs` (step 21) | add the prover |
| `crates/vibra-types/src/ctfe.rs` (step 21) | rule 1 |

Proposed: `vibra-types::contract::{facts, prove}`: a fact set per program
point (branch facts, let aliases, argument substitutions, certified
results), the eight integer facts as rewrite rules, and the discharge
procedure with rule identifiers `GUARD-1` … `GUARD-7`, `INT-1` … `INT-8`.

## Ordered tasks

1. Obligations: `requires C` at every call (substituting actual arguments),
   `ensures C` at every `return` of the body (and the `try` exits), `where P`
   at every construction. Each gets a status `passed`, `refuted`,
   `unknown`, or `unsupported`, its rule identifiers, dependencies, and a
   counterexample when refuted.
2. Rule 1, constants: evaluate the predicate over constant operands through
   `ctfe`; `true` discharges, `false` refutes, a limit leaves `unknown`.
3. Rule 2, type invariant: a value of a type with an invariant supplies the
   type's predicate over its representation.
4. Rule 3, dominating branch: facts from the condition of the enclosing
   `if` (and its negation), and the case and field facts of a `match` arm,
   valid only inside the branch.
5. Rule 4, substitution: renaming, immutable `let` aliases, literal
   equality, actual arguments; two predicates match only when callee, type
   arguments, and operands are the same resolved entities.
6. Rule 5, Boolean structure: introduction and projection of `Bool.and`,
   and the branch rules of `Bool.not`, `or`, `xor`.
7. Rule 6, the listed integer facts, over the Bool-returning members per
   Q21; nothing about arithmetic results.
8. Rule 7, certified results: a callee's proven `ensures` holds for its
   result when its `requires` was discharged.
9. `@contract.refuted` at the predicate, related to the refuting argument
   or return; `@contract.unproved` at the predicate or the failing
   application with the declaration, and notes naming the three repairs.
10. Callable conversion: a `fn` with a bare `requires` converts to a `Fn`
    type only when its parameter types alone establish the `requires`
    (rule 2); otherwise `@contract.obligationLost` at the conversion,
    related to the guard. A callable whose `requires` has an `else`
    converts freely and keeps its checks.
11. Contract members: an implementation is checked under the contract's
    `requires` and must establish the contract's `ensures`; it cannot add
    or strengthen a `requires`.
12. A discharged guard leaves no code: the lowered program of a function
    with a discharged bare guard equals the program without it (host
    test).
13. Cases: each rule and each fact with a passing and a nearest failing
    case.

## Test matrix

`gt`, `ge`, `lt`, `le`, `eq` abbreviate `Ordered.greater`,
`Ordered.greaterOrEqual`, `Ordered.less`, `Ordered.lessOrEqual`,
`Equatable.equal` in this table only; the cases spell the members out. `pos` is
`fn pos takes {x: I32} returns r: I32 requires gt(x 0i32) do return x endfn`.

| Case id | Kind | Input | Expected |
| --- | --- | --- | --- |
| `V2-CONTRACT-rule1-constant-true` | positive, interpret | `pos(5i32)` | accepted; `5i32`; obligation `passed` with `GUARD-1` |
| `V2-CONTRACT-rule1-constant-refuted` | negative | `pos(0i32)` | `@contract.refuted` at the predicate, related to `0i32`; counterexample recorded |
| `V2-CONTRACT-rule2-invariant` | positive, interpret | `type Positive of I32 where isPositive endtype` (bare); `fn f takes {p: Positive} returns r: I32 do let Positive(raw) be p endlet return pos(raw) endfn` | accepted by `GUARD-2` |
| `V2-CONTRACT-rule2-wrong-invariant` | negative | the same with `type NonNegative of I32 where isNonNegative endtype` | `@contract.unproved` |
| `V2-CONTRACT-rule3-if-true-branch` | positive | `if gt(y 0i32) then return pos(y) else return 0i32 endif` | accepted by `GUARD-3` |
| `V2-CONTRACT-rule3-if-false-branch` | negative | `if gt(y 0i32) then return 0i32 else return pos(y) endif` | `@contract.unproved` (the negation holds, not the predicate) |
| `V2-CONTRACT-rule3-negation` | positive | `fn nonpos takes {x: I32} returns r: I32 requires le(x 0i32) …`; `if gt(y 0i32) then … else return nonpos(y) endif` | accepted (`INT-2` with the negation) |
| `V2-CONTRACT-rule3-outside-branch` | negative | the fact used after `endif` | `@contract.unproved` |
| `V2-CONTRACT-rule3-match-fact` | positive | `match o case Option.some(value: v) return posOpt(o) …` where `posOpt` requires `o` to be `some` by a certified `isSome` | accepted by `GUARD-3` |
| `V2-CONTRACT-rule4-alias` | positive | `let z be y endlet if gt(y 0i32) then return pos(z) …` | accepted by `GUARD-4` |
| `V2-CONTRACT-rule4-text-not-enough` | negative | two different functions named `gt` in two modules, one used in the `if` and one in the `requires` | `@contract.unproved` |
| `V2-CONTRACT-rule4-substitution` | positive | `fn g takes {a: I32} returns r: I32 requires gt(a 0i32) do return pos(a) endfn` | accepted (the caller's obligation becomes `g`'s) |
| `V2-CONTRACT-rule5-and-projection` | positive | `fn h takes {a: I32 b: I32} returns r: I32 requires Bool.and(gt(a 0i32) gt(b 0i32)) do return pos(a) endfn` | accepted by `GUARD-5` |
| `V2-CONTRACT-rule5-and-introduction` | positive | `if gt(a 0i32) then if gt(b 0i32) then return h(a b) …` | accepted |
| `V2-CONTRACT-rule5-or-insufficient` | negative | `if Bool.or(gt(a 0i32) gt(b 0i32)) then return pos(a) …` | `@contract.unproved` |
| `V2-CONTRACT-int1` | positive | `if lt(x y) then return notEqualReq(x y) …` where the callee requires `Bool.not(eq(x y))` | accepted by `INT-1` |
| `V2-CONTRACT-int1-nearest-failing` | negative | `if le(x y) then return notEqualReq(x y) …` | `@contract.unproved` |
| `V2-CONTRACT-int2` | positive | `if gt(x y) then return geReq(x y) …` | `INT-2` |
| `V2-CONTRACT-int3` | positive | `if eq(x y) then return leReq(y x) …` | `INT-3` |
| `V2-CONTRACT-int4` | positive | `if le(x y) then if le(y x) then return eqReq(x y) …` | `INT-4` |
| `V2-CONTRACT-int4-nearest-failing` | negative | `if le(x y) then return eqReq(x y) …` | `@contract.unproved` |
| `V2-CONTRACT-int5` | positive | `if le(x y) then if Bool.not(eq(x y)) then return ltReq(x y) …` | `INT-5` |
| `V2-CONTRACT-int6` | positive | `if lt(x y) then if le(y z) then return ltReq(x z) …` | `INT-6` |
| `V2-CONTRACT-int7` | positive | `if le(x y) then if le(y z) then return leReq(x z) …` | `INT-7` |
| `V2-CONTRACT-int7-nearest-failing` | negative | `if le(x y) then if le(z y) then return leReq(x z) …` | `@contract.unproved` |
| `V2-CONTRACT-int8` | positive | `return pos(I32.add(2i32 3i32))`? No: arithmetic results have no facts, so the case is `requires gt(5i32 3i32)` on a constant pair | `INT-8` (evaluation) |
| `V2-CONTRACT-no-arithmetic-fact` | negative | `if gt(x 0i32) then return pos(I32.add(x 1i32)) …` | `@contract.unproved` (saturation) |
| `V2-CONTRACT-no-float-fact` | negative | the same shapes over `F64` with `F64.compareTotal` | `@contract.unsupportedPredicate` or `unproved` as the fragment decides: `F64` has no `Ordered`, so the predicate is a static member call, certified, and the fact set is empty: `@contract.unproved` |
| `V2-CONTRACT-no-user-ordered-fact` | negative | a user type with `impl Ordered`; `if Ordered.less(a b) then return ltUserReq(a b)` where the callee requires `Bool.not(Equatable.equal(a b))` | `@contract.unproved` |
| `V2-CONTRACT-rule7-certified-result` | positive | `fn mk returns r: I32 ensures gt(r 0i32) do return 1i32 endfn`; `return pos(mk())` | accepted by `GUARD-7` |
| `V2-CONTRACT-rule7-requires-not-discharged` | negative | the callee's `ensures` depends on a `requires` the caller did not establish | `@contract.unproved` at the caller |
| `V2-CONTRACT-ensures-refuted` | negative | `fn bad returns r: I32 ensures gt(r 0i32) do return 0i32 endfn` | `@contract.refuted` at the predicate, related to the `return` |
| `V2-CONTRACT-ensures-every-return` | negative | two returns, one proven and one not | `@contract.unproved` related to the unproven `return` |
| `V2-CONTRACT-ensures-through-try` | negative | a bare `ensures` on a `Result`-returning body with a `try` whose `err` exit cannot establish it | `@contract.unproved` related to the `try` |
| `V2-CONTRACT-where-bare-construction` | positive, interpret | `type Positive of I32 where isPositive endtype`; `Positive(3i32)` | accepted (`GUARD-1`); the value, type unchanged (`Positive`, not `Result`) |
| `V2-CONTRACT-where-bare-unproved` | negative | `Positive(x)` with `x` a parameter and no fact | `@contract.unproved` at the construction |
| `V2-CONTRACT-where-bare-branch` | positive, interpret | `if isPositive(x) then return Positive(x) …` | accepted (`GUARD-3`, `GUARD-4`) |
| `V2-CONTRACT-obligation-lost` | negative | `let f: Fn({I32} I32) be pos endlet` | `@contract.obligationLost` at `pos`, related to the guard |
| `V2-CONTRACT-obligation-kept-by-type` | positive | `fn posP takes {p: Positive} returns r: I32 requires isPositiveP(p) …`; `let f: Fn({Positive} I32) be posP endlet` | accepted (parameter type establishes it) |
| `V2-CONTRACT-else-converts-freely` | positive, interpret | `let f: Fn({I32} I32) be posElse endlet return f(0i32)` where `posElse` has `requires … else -1i32` | `-1i32` |
| `V2-CONTRACT-member-guard-contract` | positive | an interface member with `requires`; an implementation checked under it | accepted |
| `V2-CONTRACT-member-adds-requires` | negative | an implementation member with its own `requires` | `@type.mismatch` at the member (the contract's guards are the member's) |
| `V2-CONTRACT-member-ensures-established` | negative | an implementation that does not establish the contract's `ensures` | `@contract.unproved` at the member's `return` |
| `V2-CONTRACT-no-runtime-check` | positive | the lowered program of `pos(5i32)` | host test: identical to the program without the `requires` |
| `V2-CONTRACT-unproved-notes` | negative | `pos(y)` | the `@contract.unproved` diagnostic carries three notes naming the repairs |

## Diagnostics and schema changes

Codes: `@contract.unproved`, `@contract.refuted`,
`@contract.obligationLost`, `@contract.unsupportedPredicate`,
`@type.mismatch`. Schema: `@types.v2` obligations carry `ruleIds` and
`counterexample`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: every rule and fact has a `PASS` pair; the no-runtime-check host test
compares two lowered programs structurally.

## Excluded

Bare `requires` on `proc` and `await` (M2); quantified predicates and
proofs by sampling (never).

## Completion evidence

- [ ] One passing and one nearest-failing case per rule `GUARD-1` to `GUARD-7`
      and per fact `INT-1` to `INT-8`.
- [ ] The `@types.v2` snapshot of a discharged obligation lists its rule
      identifiers.
- [ ] Validation commands exit 0.
