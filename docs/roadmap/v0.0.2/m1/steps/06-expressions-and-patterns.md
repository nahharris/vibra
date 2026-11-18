# Step 06: expressions-and-patterns

Kind: vertical. The expression, statement, and pattern grammar, hosted by
the module-level `let name: Type be expr endlet`, whose header needs only
the type grammar of step 05. `if`, `match`, `do`, application, field paths,
the anonymous products and collections, `return`, `try`, `await`, and the
pattern grammar in `match` arms and local `let`. Anonymous `fn`/`proc`
parse in step 07.

## Prerequisites

- Step 05 landed.
- Read: [source chapter](../../../../spec/01-source-language.md) "Grammar"
  (`module-let`, `body` through `pattern`), "Module values", "Names and paths",
  "Application", "Anonymous products and collections", "Bodies and statements"
  (`let`, `return`/`try`/`await`, `if`/`match`/`do`), "Patterns", "Canonical
  format"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md) "Binding, return
  and pattern diagnostics" (the `@syntax.misplacedBinding` row).
- The leaf kind of the discard `_` in `@tree.v2` is item S1 of the
  [specification questions](../spec-questions.md). It blocks no task
  here: no case of this step with a `_` in its input carries a `tree`
  snapshot until it is answered.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-syntax/src/source/{types,section}.rs` (step 05) | type expressions for `let x: T`, section boundaries for `be`/`else` |
| `crates/vibra-syntax-v1/src/ast.rs:98` (`Expression`, `ExpressionKind`) | the owned-view shape; none of the S-expression dispatch |
| `crates/vibra-fmt/src/source.rs` | add expression, block, and pattern layout |

Proposed: `vibra-syntax::source::{expr, pattern, statement}` with `Expr`,
`Argument { label: Option<Ident>, value: Expr }`,
`Statement = Let(Let) | Expr(Expr)`, `Let { pattern, written_type:
Option<TypeExpr>, value, fallback: Option<Body> }`, `Pattern`,
`MatchArm { pattern, body }`, `ModuleLet { name, ty, visibility, doc, value }`.

## Ordered tasks

1. Expression precedence as the EBNF: `return expr`, then `try`/`await`
   prefix, then postfix (`(args)` and `.ident`), then primary. `(` always
   applies the form before it; `f()` is a nullary call. `{…}` is a tuple
   value unless its first entry is `ident :` (then a record value, every
   entry labelled, mixing is `@syntax.invalidForm`); `[…]` is a list unless
   its first entry is `expr :` (then a dict); `[:]` is the empty dict. An
   argument is `expr` or `ident : expr`. A reserved word written as the
   label of an argument or of a record field is recovered as the label
   with `@name.reservedLabel` at the word. `as` in an expression is
   `@syntax.invalidForm` at the `as`, and a `(` where an expression or a
   statement is expected is `@syntax.invalidForm` once over the balanced
   parenthesized form. Invariant: no statement begins with `(`, `.`, or
   `:`, so statements need no terminator.
2. `if expr then body [else body] endif`, `match expr arm {arm} endmatch`
   with `arm = case pattern body`, `do body enddo`. A `match` with no arm is
   `@syntax.invalidForm`. Invariant: the arm's body ends at the next `case`
   or `endmatch`.
3. Statements and `let`: `let pattern [: type] be expr [else body] endlet`.
   A `let` in an operand position (argument, condition, initializer) is
   `@syntax.misplacedBinding` at the `let`. A missing `be`, expression, or
   `endlet` is `@syntax.invalidForm` or `@syntax.unmatchedDelimiter` per
   the chapter's table. Invariant: a body is a sequence of statements and
   nothing else.
4. Patterns: `_`, a `pattern-literal` (string, character, integer, or
   atom), `path`, `path(…)` with positional sub-patterns or with labelled
   ones, `{…}` tuple or record, `[…]` list. A float and `void` are not
   patterns (`@syntax.invalidForm` at the literal). A constructor pattern
   is positional or labelled, never both: mixing the two in one
   parenthesized or brace form is `@syntax.invalidForm` over the pattern.
   Whether the shape fits the constructor it names is the checker's
   `@type.argumentMismatch` (step 17). A reserved word as a binder is
   recovered with `@name.reservedDeclaration`, and as a field label with
   `@name.reservedLabel`. Invariant: one pattern grammar shared by `let`,
   `takes` (step 07), and arms.
5. Module `let`: `let ident : type-expr {visibility | doc} be expr endlet`.
   A pattern instead of an identifier, a missing type, or an `else` is
   `@syntax.invalidForm`. Invariant: the module `let` and the local `let`
   are two views over one construct kind.
6. `where` operands of step 05 are typed as expressions now.
7. Doc strings outside a `doc` section are `@syntax.invalidForm`.
8. Formatter: one-line rule for `if`/`match`/`do`/`let`/brace and bracket
   forms; the multi-line layouts of the chapter (`then` on the condition's
   line, `else` alone at the `if`'s indentation; each `case` one step in,
   its body one further; `let` with `be` on the pattern's line, `else` at
   the end of the initializer's last line, `endlet` at the `let`'s
   indentation; one entry per line for an overflowing brace or bracket
   form); arguments in written order always. Invariant: no evaluation
   order changes.
9. Cases. Every case is a module `let` so that it is a complete module.

## Test matrix

| Case id | Kind | Input (`.vib`) | Expected |
| --- | --- | --- | --- |
| `V2-SRC-EXPR-application` | positive | `let a: I32 be choose(c)(x) endlet` | accepted; the `tree` snapshot has one `@postfix` node holding the callee and both argument lists |
| `V2-SRC-EXPR-nullary-and-value` | positive | `let a: I32 be f() endlet\nlet g: Fn({} I32) be f endlet` | accepted; `f()` is a call, `f` a name |
| `V2-SRC-EXPR-labelled-arguments` | positive, formatter | `let u: User be User(id: UserId(1u64) name: "Ada") endlet` | accepted; formatted keeps the written order |
| `V2-SRC-EXPR-field-path` | positive | `let n: U64 be Str.length(user.name) endlet` | accepted; `Str.length` and `user.name` are both dotted names (resolution decides which is a path) |
| `V2-SRC-EXPR-products` | positive, formatter | `let t: {Str U64} be {"Ada" 42u64} endlet\nlet r: {name: Str id: U64} be {name: "Ada" id: 42u64} endlet` | accepted; formatted equals input |
| `V2-SRC-EXPR-collections` | positive | `let l: List(I32) be [1i32 2i32] endlet\nlet d: Dict(Str Str) be ["name": "Ada" "role": "dev"] endlet\nlet e: Dict(Str Str) be [:] endlet\nlet z: List(I32) be [] endlet` | accepted |
| `V2-SRC-EXPR-mixed-record` | negative | `let r: {name: Str} be {name: "Ada" 1u8} endlet` | `@syntax.invalidForm` over the brace form |
| `V2-SRC-EXPR-if-expression` | positive, formatter | `let label: Str be if ready then "go" else "wait" endif endlet` | accepted; one line |
| `V2-SRC-EXPR-if-multiline` | formatter | the same `if` with bodies of two statements each | `then` on the condition's line, `else` alone, `endif` at the `if`'s indentation |
| `V2-SRC-EXPR-match` | positive, formatter | `let n: U64 be match value\n  case Option.some(value: text)\n    Str.length(text)\n  case Option.none()\n    0u64\nendmatch endlet` | accepted; two arms; formatted equals input |
| `V2-SRC-EXPR-match-no-arm` | negative | `let n: U64 be match value endmatch endlet` | `@syntax.invalidForm` over the `match` |
| `V2-SRC-EXPR-do-block` | positive | `let n: I32 be do\n  let a be 1i32 endlet\n  I32.add(a a)\nenddo endlet` | accepted; the block holds a `let` and an expression |
| `V2-SRC-EXPR-let-else` | positive, formatter | `let n: I32 be do\n  let Option.some(value: head) be items(0u64) else\n    return 0i32\n  endlet\n  head\nenddo endlet` | accepted; `else` at the end of the initializer's line |
| `V2-SRC-EXPR-let-misplaced` | negative | `let n: I32 be f(let a be 1i32 endlet) endlet` | `@syntax.misplacedBinding` at the inner `let` |
| `V2-SRC-EXPR-let-missing-be` | negative | `let n: I32 be do let a 1i32 endlet a enddo endlet` | `@syntax.invalidForm` over the inner `let` |
| `V2-SRC-EXPR-return-try-await` | positive | `let n: I32 be do return try await f() enddo endlet` | accepted; the prefix chain parses as `return(try(await(f())))` (meaning is the checker's) |
| `V2-SRC-EXPR-paren-group` | negative | `let n: I32 be (a) endlet` | one `@syntax.invalidForm` at `14..17` |
| `V2-SRC-EXPR-as-ascription` | negative | `let n: I32 be x as I32 endlet` | `@syntax.invalidForm` at `16..18` |
| `V2-SRC-EXPR-reserved-label` | negative | `let u: User be User(role: 1u8) endlet` | `@name.reservedLabel` at `20..24`; the argument keeps the label `role` |
| `V2-SRC-EXPR-patterns` | positive | a `match` with arms `_`, `0i32`, `"a"`, `'x'`, `@ok`, `Lookup.found(value: v)`, `User(name: n)`, `Celsius(c)`, `Pair(a b)`, `{p q}`, `{name: p}`, `[p q]` | accepted; twelve arms with the listed pattern kinds |
| `V2-SRC-EXPR-pattern-mixed` | negative | `… case User(n id: i) …` | `@syntax.invalidForm` over `User(n id: i)` |
| `V2-SRC-EXPR-pattern-reserved-binder` | negative | `… case Lookup.found(value: where) …` | `@name.reservedDeclaration` at `where`; the arm still binds it |
| `V2-SRC-EXPR-pattern-float` | negative | `… case 1.5f64 …` | `@syntax.invalidForm` at the literal |
| `V2-SRC-EXPR-pattern-void` | negative | `… case void …` | `@syntax.invalidForm` at `void` |
| `V2-SRC-DECL-module-let` | positive, formatter | `let defaultRetries: U8 be 3u8 endlet` | accepted; one line |
| `V2-SRC-DECL-module-let-visibility` | positive, formatter | `let limit: U8\n  visibility @public\n  be 3u8\nendlet` | accepted; `visibility` between the type and `be` |
| `V2-SRC-DECL-module-let-pattern` | negative | `let {a b}: {U8 U8} be {1u8 2u8} endlet` | `@syntax.invalidForm` |
| `V2-SRC-DECL-module-let-no-type` | negative | `let a be 1u8 endlet` | `@syntax.invalidForm` |
| `V2-SRC-DECL-module-let-else` | negative | `let a: U8 be 1u8 else return 0u8 endlet` | `@syntax.invalidForm` |
| `V2-SRC-READER-doc-string-misplaced` | negative | `let a: Str be """x""" endlet` | `@syntax.invalidForm` at the doc string |
| `V2-SRC-FMT-wrap-arguments` | formatter | a call whose one-line form is 100 columns | one argument per line, one step in, `)` on its own line |
| `V2-SRC-READER-expr-recovery` | recovery | `let a: I32 be f( endlet\nlet b: I32 be 1i32 endlet` | `@syntax.unmatchedDelimiter`; `b` exact |

## Diagnostics and schema changes

Codes: `@syntax.invalidForm`, `@syntax.misplacedBinding`,
`@syntax.unmatchedDelimiter`, `@name.reservedLabel`,
`@name.reservedDeclaration`, `@tool.unavailable` (anonymous `fn`/`proc`
until 07). Schema: `@tree.v2` gains the node kinds `@moduleLet`, `@let`,
`@postfix`, `@if`, `@match`, `@arm`, `@doBlock`, and `@patternLiteral`;
the choice productions `statement`, `expr`, `unary`, `primary`, `literal`,
`argument`, and `pattern` are never nodes.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-syntax -p vibra-fmt
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: every case passes; the formatter idempotence property test in
`vibra-fmt` now runs over every `.vib` input of the corpus.

## Excluded

Anonymous callables (07); every typing, resolution, and coverage rule.

## Completion evidence

- [ ] One reader case per production `module-let`, `body`, `statement`, `let`,
      `expr`, `unary`, `postfix`, `argument`, `primary`, `literal`, `if`,
      `match`, `arm`, `do-block`, `pattern`, `pattern-literal`, named in
      the owner table.
- [ ] `conformance/rules.tsv` has one `active` row per case id above.
- [ ] A host test asserts the formatter never changes the order of arguments,
      fields, entries, or statements (compare token sequences).
- [ ] All cases pass; validation commands exit 0.
