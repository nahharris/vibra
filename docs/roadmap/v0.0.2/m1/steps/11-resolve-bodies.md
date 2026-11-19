# Step 11: resolve-bodies

Kind: vertical. Resolution inside bodies: lexical scopes, binders, no
shadowing, the dotted-name rule (longest entity prefix, then field reads),
constant patterns versus binders, and the references the `@resolved.v2`
snapshot lists for expressions.

## Prerequisites

- Step 10 landed.
- Read: [source chapter](../../../../spec/01-source-language.md) "Names and
  paths", "Bodies and statements" (scopes, `let` visibility), "Patterns" (the
  bare-name rule); [type chapter](../../../../spec/02-type-system.md)
  "Namespaces and resolution" (shadowing), "Constant expressions and constant
  patterns" (the resolution half: which names are constant module values;
  evaluation is step 21); [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md) "Binding, return
  and pattern diagnostics" rows for `@name.redeclaration` and
  `@name.reservedDeclaration`.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-resolve/src/{declare,namespaces,paths}.rs` (step 10) | extend to bodies |
| `crates/vibra-resolve-v1/tests/it/binding_scopes.rs`, `reserved_binders.rs` | the scope matrix to port into the new grammar |

Proposed: `vibra-resolve::scope` (lexical scopes, binder classification),
`vibra-resolve::constants` (syntactic constant-expression test per the type
chapter: literal, atom, `void`, construction, tuple, record, list, dict of
constants whose type has no `where … else` (that last check is the
checker's; resolution records the candidates), a constant module value's
name; never a call).

## Ordered tasks

1. Scopes: a callable body is one scope with its parameters; each `if`
   branch, `match` arm, `do` block, and `let` fallback is its own scope; a
   `let`'s names are visible after `endlet` to the end of the directly
   enclosing body and not in its initializer or fallback. Invariant: a
   `let` opens no nested scope.
2. Binders: a bare name in a pattern that resolves to a visible module value
   whose initializer is a syntactic constant expression is a constant
   pattern and binds nothing; `true` and `false` are such names; any other
   bare name binds; a dotted name never binds; a name resolving to a
   function or a non-constant module value in pattern position is
   `@name.redeclaration` at the name, related to the declaration. A pattern
   binding one name twice, or a binder that shadows any visible name, is
   `@name.redeclaration`; a binder spelled as a prelude name is
   `@name.reservedDeclaration` (the reader already reports a reserved
   word, step 06). The later binder still binds.
3. Dotted names in expressions: if the first identifier is a local or a
   module value, each later part is a field read; otherwise the longest
   prefix naming an entity is the path and the rest are field reads; the
   reference records the entity and the field parts (`@type.unknownRecordField`
   is the checker's). A dotted name whose first part names nothing is
   `@name.unknownSymbol`. Invariant: because nothing shadows, a name is
   never both.
4. Result name: visible only in `ensures`; a use in `requires` or the body
   is `@name.unknownSymbol`.
5. `Self`: visible inside a `type`, `interface`, and `impl`; in a
   module-level callable it is `@name.unknownSymbol`.
6. `_` binds nothing and may repeat.
7. `@resolved.v2` `references` for expression and pattern positions.
8. Cases.

## Test matrix

| Case id | Kind | Input (body excerpts inside `fn f takes {…} returns r: … do … endfn`) | Expected |
| --- | --- | --- | --- |
| `V2-TYPE-NAMES-let-visible-after` | positive | `let a be 1i32 endlet return a` | accepted |
| `V2-TYPE-NAMES-let-not-in-initializer` | negative | `let a be a endlet return a` | `@name.unknownSymbol` at the inner `a` |
| `V2-TYPE-NAMES-let-not-in-else` | negative | `let Option.some(value: a) be x else return a endlet return a` | `@name.unknownSymbol` at the `a` in the fallback |
| `V2-TYPE-NAMES-sibling-scopes` | positive | `if c then let a be 1i32 endlet return a else let a be 2i32 endlet return a endif` | accepted; two bindings named `a` |
| `V2-TYPE-NAMES-shadow-parameter` | negative | `takes {a: I32}` and `let a be 2i32 endlet` | `@name.redeclaration` at the `let`'s `a`, related to the parameter |
| `V2-TYPE-NAMES-shadow-module` | negative | module value `limit` and a local `let limit be …` | `@name.redeclaration` |
| `V2-TYPE-NAMES-shadow-nested` | negative | `let a be 1i32 endlet if c then let a be 2i32 endlet … endif` | `@name.redeclaration` at the inner `a` |
| `V2-TYPE-NAMES-pattern-twice` | negative | `let {a a} be t endlet` | `@name.redeclaration` at the second `a` |
| `V2-TYPE-NAMES-discard-repeats` | positive | `let {_ _ c} be t endlet return c` | accepted |
| `V2-TYPE-NAMES-constant-pattern` | positive | module `let zero: I32 be 0i32 endlet`; `match x case zero … case _ …` | accepted; the first arm is a constant pattern (snapshot shows no binding) |
| `V2-TYPE-NAMES-bool-constant-pattern` | positive | `match b case true … case false …` | accepted; neither arm binds |
| `V2-TYPE-NAMES-function-as-pattern` | negative | module `fn helper …`; `match x case helper …` | `@name.redeclaration` at `helper`, related to the function |
| `V2-TYPE-NAMES-nonconstant-as-pattern` | negative | module `let limit: I32 be f() endlet`; `case limit` | `@name.redeclaration` at `limit` |
| `V2-TYPE-NAMES-reserved-binder` | negative | `let Option be x endlet` | `@name.reservedDeclaration` at `Option` |
| `V2-TYPE-NAMES-path-vs-field` | positive | `takes {user: User}`; `Str.length(user.name)` | accepted; `Str.length` resolves to the member, `user.name` to the local plus field `name` |
| `V2-TYPE-NAMES-longest-prefix` | positive | module import `use @app.model enduse`; `model.User.nameLength(u).x` | the path is `model.User.nameLength`, `.x` a field read of the result |
| `V2-TYPE-NAMES-unknown-first-part` | negative | `nothere.field` | `@name.unknownSymbol` at `nothere` |
| `V2-TYPE-NAMES-result-name-in-body` | negative | `returns r: I32 do return r` | `@name.unknownSymbol` at `r` |
| `V2-TYPE-NAMES-result-name-in-ensures` | positive | `returns r: I32 ensures Ordered.greater(r 0i32) do return 1i32` | accepted by resolution (discharge is step 22) |
| `V2-TYPE-NAMES-self-outside` | negative | module-level `fn f takes {s: Self} …` | `@name.unknownSymbol` at `Self` |
| `V2-TYPE-NAMES-anon-fn-scope` | positive | `fn takes {x: I32} returns r: I32 do return I32.add(x outer) endfn` inside a body with local `outer` | accepted; `outer` resolves to the enclosing local |

## Diagnostics and schema changes

Codes: `@name.unknownSymbol`, `@name.redeclaration`,
`@name.reservedDeclaration`. Schema: `@resolved.v2` gains expression
references; its shape does not change.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-resolve
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: every `resolve` case passes; the snapshot of
`V2-TYPE-NAMES-constant-pattern` lists the arm's reference as a value
reference, not a binder.

## Excluded

Type-directed facts (which case a constructor path names, field existence);
evaluation of constants (21).

## Completion evidence

- [ ] A host test enumerates the visible locals at every statement of a fixture
      and matches the chapter's rule.
- [ ] All cases pass; validation commands exit 0.
