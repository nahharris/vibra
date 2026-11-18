# Step 17: check-bodies

Kind: vertical. Slice 4 completed: `let` with every pattern and with
`else`, parameter patterns, coverage over products, lists, literals, and
atoms, `try` with its expected type, fallible values, `Never` through
`let … else`, and `await` outside a `proc` rejected.

## Prerequisites

- Step 16 landed.
- Read: [type chapter](../../../../spec/02-type-system.md) "Coverage", "Constant
  expressions and constant patterns" (the coverage half), "`try`", "Fallible
  values", "`return` and `Never`"; [source
  chapter](../../../../spec/01-source-language.md) "Bodies and statements",
  "Patterns"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md) "Binding, return
  and pattern diagnostics"; [runtime chapter](../../../../spec/06-runtime.md)
  "Evaluation" (`let`, `try`).
- Open questions: none.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-types/src/{expr,lower}.rs` (steps 15, 16) | extend |
| `crates/vibra-types-v1/src/pattern.rs` | the coverage algorithm (first uncovered shape in declaration order, reachability by subsumption), re-expressed for the new pattern set |

Proposed: `vibra-types::pattern` (coverage, irrefutability, binding types).

## Ordered tasks

1. Pattern typing against a scrutinee type: `_`, binder, literal (integer,
   string, char, atom; never float, `void`, or `Never`), constant pattern
   (expanded value), case with named fields (omitted fields ignored),
   declared record by field, wrapper and declared tuple positionally with
   exact arity, tuple, record (omitted fields ignored), list of exact
   length. A pattern that does not fit its type is `@type.mismatch`.
2. Coverage: variants by inhabited cases; `Bool` by `true` and `false`;
   products by components; a fixed-length list never covers; literals
   never cover `Str`, `Bytes`, `Char`, numbers; atoms never cover `Atom`;
   an atom singleton is covered by its atom; an uninhabited position has
   nothing to cover; a `where` never proves coverage;
   `@pattern.nonExhaustive` with the first uncovered shape as a note in
   pattern spelling; `@pattern.unreachableArm` related to the earliest
   covering arm; `@pattern.unsupportedCoverage` when the checker cannot
   decide within its limits (a host test forces it with a deep product).
3. `let`: without `else` the pattern MUST be irrefutable
   (`@pattern.refutableBinding` at the pattern); with `else` it MUST be
   refutable (`@pattern.irrefutableLetElse`); the fallback has type `Never`
   (`@type.mismatch` at the `else` body when it can complete). Parameter
   patterns MUST be irrefutable.
4. `try`: operand `Option(T)` or `Result(T E)` inside a callable whose
   result is the same carrier with identical `E`; `@type.invalidTry`
   otherwise; the operand is checked at the expected carrier of `T` when
   the `try` has a written expected type; directly in a `test` body it is
   `@type.invalidTry` (tests are 23; the rule is implemented now).
5. Fallible values: an expression statement of type `Result(T E)` with `E`
   inhabited is `@type.unhandledFallible`; `let _ be … endlet` discards.
6. `await` in a `fn` body is `@effect.invalidAwait` (the `proc` and test
   contexts are M2 and 23).
7. Interpreter: `let … else`, `try` propagation, pattern matching over every
   kind.
8. Cases.

## Test matrix

| Case id | Kind | Input (inside `fn … do … endfn`, module context given) | Expected |
| --- | --- | --- | --- |
| `V2-TYPE-CONTROL-let-record-pattern` | positive, interpret | `let {name: n id: i} be user endlet return n` | `"Ada"` |
| `V2-TYPE-CONTROL-let-tuple-pattern` | positive, interpret | `let {a b} be {1i32 "x"} endlet return b` | `"x"` |
| `V2-TYPE-CONTROL-let-wrapper-pattern` | positive, interpret | `let Celsius(c) be Celsius(1.5f64) endlet return c` | `1.5f64` |
| `V2-TYPE-CONTROL-let-else` | positive, interpret | `let Option.some(value: head) be items(0u64) else return 0i32 endlet return head` with `items` empty and non-empty | `0i32` and the head |
| `V2-TYPE-CONTROL-let-else-not-never` | negative | `let Option.some(value: h) be items(0u64) else let x be 1i32 endlet endlet` | `@type.mismatch` at the `else` body |
| `V2-TYPE-CONTROL-let-refutable-no-else` | negative | `let Option.some(value: h) be items(0u64) endlet` | `@pattern.refutableBinding` at the pattern |
| `V2-TYPE-CONTROL-let-irrefutable-else` | negative | `let {a b} be t else return 0i32 endlet` | `@pattern.irrefutableLetElse` at the pattern |
| `V2-TYPE-CONTROL-param-refutable` | negative | `fn f takes {Option.some(value: x): Option(I32)} …` | `@pattern.refutableBinding` at the parameter pattern |
| `V2-TYPE-CONTROL-param-tuple` | positive, interpret | `fn f takes {{a b}: {I32 I32}} returns r: I32 do return I32.add(a b) endfn` | sum |
| `V2-TYPE-CONTROL-match-record-omitted` | positive, interpret | `match user case User(name: n) return n endmatch` | the name; one arm covers |
| `V2-TYPE-CONTROL-match-list-length` | negative | `match items case [a] return a case [a b] return b endmatch` | `@pattern.nonExhaustive`, note `_` |
| `V2-TYPE-CONTROL-match-list-exact` | positive, interpret | `match items case [a b] return b case _ return 0i32 endmatch` on `[1i32 2i32]` | `2i32` |
| `V2-TYPE-CONTROL-match-literal-open` | negative | `match n case 0i32 … case 1i32 … endmatch` | `@pattern.nonExhaustive` |
| `V2-TYPE-CONTROL-match-str-literal` | positive, interpret | `match s case "a" return 1i32 case _ return 0i32 endmatch` | by input |
| `V2-TYPE-CONTROL-match-atom-singleton` | positive, interpret | `fn f takes {a: @ok} returns r: I32 do match a case @ok return 1i32 endmatch endfn` | `1i32`; covered by its one atom |
| `V2-TYPE-CONTROL-match-atom-open` | negative | `takes {a: Atom}` and only `case @ok` | `@pattern.nonExhaustive` |
| `V2-TYPE-CONTROL-match-nested` | positive | `match o case Option.some(value: Shape.circle(radius: r)) … case Option.some(value: Shape.empty()) … case Option.none() … endmatch` | accepted |
| `V2-TYPE-CONTROL-match-nested-missing` | negative | the same without the `empty` arm | `@pattern.nonExhaustive`, note `Option.some(value: Shape.empty())` |
| `V2-TYPE-CONTROL-match-unreachable-nested` | negative | `case Option.some(value: _) …` then `case Option.some(value: Shape.empty()) …` | `@pattern.unreachableArm` related to the first arm |
| `V2-TYPE-CONTROL-match-never-field` | positive | `match r case Result.ok(value: v) return v endmatch` with `r: Result(I32 Never)` | accepted; no `err` arm needed |
| `V2-TYPE-CONTROL-match-constant-pattern` | positive, interpret | module `let zero: I32 be 0i32 endlet`; `case zero return "z" case _ return "n"` | `"z"` for `0i32` |
| `V2-TYPE-CONTROL-match-where-no-coverage` | negative | a wrapper with a bare `where` and a `match` on its representation covering only the invariant's range | `@pattern.nonExhaustive` (recorded now with the `where` parsed; the invariant rule lands in 21 and the expectation does not change) |
| `V2-TYPE-CONTROL-pattern-type-mismatch` | negative | `match n case "x" …` with `n: I32` | `@type.mismatch` at the pattern |
| `V2-TYPE-CONTROL-try-option` | positive, interpret | `fn f takes {o: Option(I32)} returns r: Option(I32) do let v be try o endlet return Option.some(value: I32.add(v 1i32)) endfn` | `some 2` for `some 1`; `none` for `none` |
| `V2-TYPE-CONTROL-try-result` | positive, interpret | the same with `Result(I32 Str)` | `ok`/`err` propagated |
| `V2-TYPE-CONTROL-try-wrong-carrier` | negative | `try o` with `o: Option(I32)` in a `fn` returning `Result(I32 Str)` | `@type.invalidTry` at the `try` |
| `V2-TYPE-CONTROL-try-error-type` | negative | `try r` with `r: Result(I32 Str)` in a `fn` returning `Result(I32 Bool)` | `@type.invalidTry` |
| `V2-TYPE-CONTROL-try-in-fn-returning-plain` | negative | `try o` in a `fn` returning `I32` | `@type.invalidTry` |
| `V2-TYPE-CONTROL-try-expected-type` | positive | `let port: U16 be try parse(text) endlet` where `parse` returns `Result(U16 Str)` and the `fn` returns `Result(U16 Str)` | accepted; the operand is checked at `Result(U16 Str)` |
| `V2-TYPE-CONTROL-unhandled-fallible` | negative | `attempt()` as a statement with `attempt: Fn({} Result(Void Str))` | `@type.unhandledFallible` at the statement |
| `V2-TYPE-CONTROL-discard-fallible` | positive | `let _ be attempt() endlet` | accepted |
| `V2-TYPE-CONTROL-option-not-fallible` | positive | `lookup()` as a statement returning `Option(I32)` | accepted |
| `V2-TYPE-CONTROL-fallible-never-error` | positive | a statement of type `Result(Void Never)` | accepted |
| `V2-EFFECT-await-in-fn` | negative | `let x be await p endlet` in a `fn` | `@effect.invalidAwait` at the `await` |
| `V2-TYPE-CONTROL-unsupported-coverage` | boundary | a `match` over a 12-component tuple of `Bool` with 4,095 arms (generated fixture) | `@pattern.unsupportedCoverage` at the `match`, never a false acceptance; the host test fixes the limit |

## Diagnostics and schema changes

Codes: `@pattern.refutableBinding`, `@pattern.irrefutableLetElse`,
`@pattern.nonExhaustive`, `@pattern.unreachableArm`,
`@pattern.unsupportedCoverage`, `@type.mismatch`, `@type.invalidTry`,
`@type.unhandledFallible`, `@effect.invalidAwait`. No schema change.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-interp -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: all cases pass; the `unsupportedCoverage` fixture produces exactly
that code and the generated arm count is recorded in the case description.

## Excluded

Anonymous callables and labelled defaults (18); constant-construction
irrefutability through `where … else` (21); tests (23).

## Completion evidence

- [ ] Every row of "Binding, return and pattern diagnostics" in the diagnostics
      chapter that M1 owns has a case (the owner table lists them).
- [ ] A host test asserts the first uncovered shape is reported in declaration
      order for a three-case variant.
- [ ] Validation commands exit 0.
