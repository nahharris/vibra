# Step 07: callables-and-tests-syntax

Kind: vertical. The last grammar areas: `fn` and `proc` declarations with
their signatures, guard and provider sections, and bodies; anonymous `fn`
and `proc`; nested members and `impl` members; `test … endtest`. After this
step the reader accepts the whole grammar of the source chapter.

## Prerequisites

- Step 06 landed.
- Read: [source chapter](../../../../spec/01-source-language.md) "Grammar"
  (`fn-decl`, `proc-decl`, `signature`, `params`, `labelled`, `test`,
  `requires`, `ensures`, `performs`, `external`, `symbol`, `native`, `anon-fn`,
  `anon-proc`), "Declarations" (`fn` and `proc`, `test`, "External
  definitions"), "Canonical format"; [type
  chapter](../../../../spec/02-type-system.md) "Guards" (the shapes only).
- Open questions: Q10 (a top-level `fn` with neither body nor `external`).

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-syntax/src/source/{expr,pattern,statement,types}.rs` (steps 05, 06) | bodies, parameter patterns, types |
| `crates/vibra-syntax/src/source/{type_decl,interface_decl,impl_block}.rs` (step 05) | members become `fn`/`proc` views; the `@tool.unavailable` placeholders go away |
| `crates/vibra-fmt/src/source.rs` | declaration body layout: statements two steps in, `do` on its own line in a multi-line declaration |

Proposed: `vibra-syntax::source::{callable, test_decl}` with
`FnDecl { name, params, signature, sections, body: Option<Body> }`, `ProcDecl`
(same plus `performs`), `Signature { takes: Vec<(Pattern, TypeExpr)>, labelled:
Vec<(Ident, TypeExpr, Expr)>, returns: (Ident, TypeExpr) }`,
`Guard { predicate: Expr, consequence: Option<Expr> }`, `External(Atom)`,
`Symbol(Str)`, `Native(Str)`, `AnonFn`, `AnonProc`, `Test { name: Str, takes:
Option<(Pattern, TypeExpr)>, performs: Option<Row>, body }`.

## Ordered tasks

1. Signature: `[takes params] [labelled labelled] returns ident : type-expr`.
   `params = { {pattern ":" type-expr} }`;
   `labelled = { {ident ":" type-expr "be" expr} }`. A missing `returns` is
   `@syntax.invalidForm`; an empty `takes {}` or `labelled {}` is accepted and
   removed by the formatter. Invariant: the result name is kept.
2. `fn … endfn` and `proc … endproc`: name, params, signature, sections in
   any order (`given`, `requires`, `ensures`, `visibility`, `external`,
   `symbol`, `native`, `doc`; `performs` for `proc` only), `do body` last.
   `performs` on a `fn`, `native` on a `proc`, and any section not admitted
   are `@syntax.invalidSection`. `external`, `symbol`, `native` outside the
   standard library are `@syntax.invalidSection`. A declaration with
   `external` must have `symbol` and no body, and `symbol` needs
   `external`; otherwise `@syntax.invalidForm`. Per Q10 a `fn` with no body
   and no `external` outside an interface is `@syntax.invalidForm`.
   Invariant: the admissibility table of step 05 gains the `fn`, `proc`,
   `test`, and `let` rows.
3. Nested members: `fn`/`proc` as items of `type`, `interface`, and `impl`;
   an abstract interface member has no `do`; a default member's body has at
   least one statement (`@syntax.invalidForm` for `do endfn`). A named
   `fn`, `proc`, `type`, `interface`, or `test` inside a body is
   `@syntax.invalidForm`.
4. Anonymous `fn`/`proc` expressions: no name, visibility, or doc; `given`,
   `requires`, `ensures` (and `performs` for `proc`) admitted; a name after
   the keyword in expression position is `@syntax.invalidForm`.
5. `test "name" [takes params] [performs row] do body endtest`: `takes`
   with other than exactly one entry is `@syntax.invalidForm` (the type
   `TestWorld` is checked by the checker, M2 for its meaning).
6. Guards: `requires expr [else expr]`, `ensures expr [else expr]`; one
   each (`@syntax.duplicateSection`).
7. Formatter: statements two steps in from the declaration; a guard's
   `else` on the next line one step in when both do not fit; anonymous
   callables use the same layout; the one-line rule holds for a short `fn`.
8. Remove every `@tool.unavailable` the reader still emitted; from this
   step the reader emits none.
9. Cases.

## Test matrix

| Case id | Kind | Input (`.vib`) | Expected |
| --- | --- | --- | --- |
| `V2-SRC-DECL-fn-greet` | positive, formatter | the `greet` example of the chapter (`fn greet takes {name: Str} returns greeting: Str visibility @public do return Str.concat("hello, " name) endfn`, multi-line) | accepted; formatted equals the chapter's layout |
| `V2-SRC-DECL-fn-one-line` | formatter | `fn next takes {value: I32} returns r: I32 do return I32.add(value 1i32) endfn` | formatted on one line (fits 88) |
| `V2-SRC-DECL-fn-section-order` | formatter | `fn f returns r: I32 takes {x: I32} do return x endfn` | accepted; formatted moves `takes` before `returns` |
| `V2-SRC-DECL-fn-labelled` | positive | the `splitCents` example with `labelled {roundUp: Bool be false}` and `requires … else …` | accepted; formatted equals the chapter's layout with `else` one step in |
| `V2-SRC-DECL-fn-generic` | positive | `fn first(T) takes {items: List(T)} returns head: Option(T) visibility @public do return items(0u64) endfn` | accepted; one type parameter |
| `V2-SRC-DECL-fn-pattern-param` | positive | `fn f takes {{a b}: {I32 I32}} returns r: I32 do return a endfn` | accepted; a tuple pattern parameter |
| `V2-SRC-DECL-fn-empty-takes` | formatter | `fn f takes {} returns r: I32 do return 1i32 endfn` | accepted; formatted drops `takes {}` |
| `V2-SRC-DECL-fn-missing-returns` | negative | `fn f takes {x: I32} do return x endfn` | `@syntax.invalidForm` over the form |
| `V2-SRC-DECL-fn-performs` | negative | `fn f returns r: I32 performs {X} do return 1i32 endfn` | `@syntax.invalidSection` at `performs {X}` |
| `V2-SRC-DECL-fn-duplicate-requires` | negative | `fn f takes {x: I32} returns r: I32 requires a requires b do return x endfn` | `@syntax.duplicateSection` at the second `requires` |
| `V2-SRC-DECL-fn-no-body` | negative | `fn f returns r: I32 endfn` | `@syntax.invalidForm` (Q10) |
| `V2-SRC-DECL-fn-external-outside-stdlib` | negative | `fn f returns r: I32 external @compiler symbol "x" endfn` | `@syntax.invalidSection` at `external @compiler` |
| `V2-SRC-DECL-fn-external-with-body` | negative, stdlib mode | the same with `do return 1i32` | `@syntax.invalidForm` |
| `V2-SRC-DECL-fn-local-declaration` | negative | `fn f returns r: I32 do fn g returns s: I32 do return 1i32 endfn return 1i32 endfn` | `@syntax.invalidForm` over the inner `fn` |
| `V2-SRC-DECL-proc-basic` | positive | `proc copyNote takes {reader: DirReadText writer: DirWriteText path: RelativePath} returns outcome: Result(Void FsError) performs {DirReadText DirWriteText} do let text be try await DirReadText.read(reader path) endlet return await DirWriteText.write(writer path text) endproc` | accepted by the reader (the checker reports `proc` unavailable until M2) |
| `V2-SRC-DECL-proc-native` | negative | `proc p returns r: Void native "x" do return void endproc` | `@syntax.invalidSection` |
| `V2-SRC-DECL-anon-fn` | positive, formatter | `let inc: Fn({I32} I32) be fn takes {value: I32} returns next: I32 do return I32.add(value 1i32) endfn endlet` | accepted |
| `V2-SRC-DECL-anon-fn-named` | negative | `let g: Fn({} I32) be fn h returns r: I32 do return 1i32 endfn endlet` | `@syntax.invalidForm` |
| `V2-SRC-DECL-anon-proc` | positive | `let p: Proc({Str} Void {Out}) be proc takes {s: Str} returns r: Void performs {Out} do return void endproc endlet` | accepted by the reader |
| `V2-SRC-DECL-member-and-impl` | positive, formatter | the `User` example with the nested `nameLength` and `impl Printable … endimpl` | accepted; members before `impl`, formatted equals the chapter |
| `V2-SRC-DECL-interface-default-empty` | negative | `interface I fn f takes {v: Self} returns r: Str do endfn endinterface` | `@syntax.invalidForm` over the member |
| `V2-SRC-DECL-test-basic` | positive, formatter | `test "adds one"\n  do\n    assert.equal(2u64 add(1u64 1u64))\nendtest` | accepted |
| `V2-SRC-DECL-test-world` | positive | `test "reads" takes {world: TestWorld} performs {TestWorld} do assert.true(true) endtest` | accepted by the reader |
| `V2-SRC-DECL-test-two-params` | negative | `test "x" takes {a: A b: B} do assert.true(true) endtest` | `@syntax.invalidForm` |
| `V2-SRC-READER-fn-recovery` | recovery | `fn f takes {x: I32 returns r: I32 do return x endfn\nfn g returns r: I32 do return 1i32 endfn` | `@syntax.unmatchedDelimiter` at `returns` or the closer per Q8; `g` exact |

## Diagnostics and schema changes

Codes: `@syntax.invalidForm`, `@syntax.invalidSection`,
`@syntax.duplicateSection`, `@syntax.unmatchedDelimiter`. The reader emits
`@tool.unavailable` nowhere after this step. No schema change.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-syntax -p vibra-fmt
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: every case passes, and `Select-String -Path conformance/cases -Pattern
'tool.unavailable' -Recurse` over `SRC-*` cases returns nothing.

## Excluded

Every semantic rule; the standard-library modules themselves (14).

## Completion evidence

- [ ] One reader case per production `fn-decl`, `proc-decl`, `signature`,
      `params`, `labelled`, `test`, `requires`, `ensures`, `performs`,
      `external`, `symbol`, `native`, `anon-fn`, `anon-proc`, named in the owner
      table.
- [ ] Every example fenced `vibra` in the source chapter parses with no
      diagnostic (host test, superseded by step 08's inventory).
- [ ] All cases pass; validation commands exit 0.
