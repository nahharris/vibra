# Step 05: type-declarations

Kind: vertical. The type-expression grammar and the three type-level
declarations: `type … endtype` (wrapper, declared tuple, declared record,
variant), `interface … endinterface` with associated types, and `impl …
endimpl` with associated choices, plus their sections and canonical
format. Nested `fn` and `proc` members inside them parse in step 07; until
then a member is `@tool.unavailable` over the member.

## Prerequisites

- Step 04 landed.
- Read: [source chapter](../../../../spec/01-source-language.md) "Grammar"
  (`type-decl` through `assoc-choice`, `type-params`, `given`, `where`,
  `visibility`, `representation`, `role`, `doc`, `type-expr`, `record-type`,
  `inputs`, `row`, `path`), "Declarations" (`type`, `interface`, `impl`),
  "Canonical format"; [type chapter](../../../../spec/02-type-system.md)
  "Declared types" and "Associated types" for the shapes the views must expose
  (checking is steps 15 to 19).
- Open questions: Q9 (which code a wrong tag in `visibility`,
  `representation`, `role`, `external` gets). Must be answered before this
  step starts.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-syntax/src/source/{declaration,section}.rs` (step 04) | the generic construct nodes to type |
| `crates/vibra-syntax-v1/src/ast.rs:16` (`RESERVED_TYPE_HEADS`) | the idea of special type heads; the new heads are `Fn`, `Proc`, `Intent`, `Intrinsic` |
| `crates/vibra-fmt/src/source.rs` (step 04) | section order; add type-expression layout |

Proposed: `vibra-syntax::source::{types, type_decl, interface_decl, impl_block}`
with `TypeExpr`, `TypeDecl { name, params, body: Of(TypeExpr) | Intrinsic(Atom) |
Cases(Vec<Case>), items }`,
`Case { name, fields: Option<Vec<(Ident, TypeExpr)>> }`,
`InterfaceDecl { name, params, items }`, `AssocType { name }`,
`Impl { target: TypeExpr, items }`, `AssocChoice { name, chosen: TypeExpr }`,
`Given(Vec<(Ident, Path)>)`, `Where { predicate_tokens, else_tokens }`
(expressions typed in step 06), `Visibility(Atom)`, `Representation(Atom)`,
`Role(Atom)`, `Doc(DocString)`.

## Ordered tasks

1. `type-expr`: `path [ "(" {type-expr} {ident ":" type-expr} ")" ]`,
   atom, `{…}` tuple type, `{name: T …}` record type (labelled entries
   decide record versus tuple; mixing is `@syntax.invalidForm`), `Fn(inputs
   T)`, `Proc(inputs T [row])`, `Intent(T [row])`, where `inputs` is
   `{ {type-expr} {ident ":" type-expr} }` and `row` is `{ {path} }`. A
   labelled type argument after a positional one only; a positional after a
   labelled one is `@syntax.invalidForm`. Invariant: `(` is never the start
   of a type.
2. `type … endtype`: name, optional `type-params` (`(` identifiers `)`),
   then `of type-body` or one or more `case`, then items. `type-body` is a
   type expression or `Intrinsic(atom)`. A `case` has a name and an
   optional `record-type`. Missing name, missing body and cases, both `of`
   and `case`, a case list that is empty, are `@syntax.invalidForm`.
   Invariant: cases keep written order.
3. Sections admitted by `type`: `given`, `where`, `role`, `visibility`,
   `representation`, `doc`; by `interface`: `given`, `role`, `visibility`,
   `doc`; by `impl`: none. Any other section is `@syntax.invalidSection`
   at the section. `role` and `Intrinsic` outside the embedded standard
   library are `@syntax.invalidSection` (the reader is told whether it
   parses a standard-library module through `parse_source`'s options; the
   default is "not standard library"). A tag outside the closed set of a
   tag section gets the Q9 code. Invariant: admissibility is one table per
   declaration kind.
4. `interface … endinterface`: name, params, items: `given`, `role`,
   `visibility`, `doc`, `type Name endtype` (associated type: a `type`
   with no body and no items; a body here is `@syntax.invalidForm`),
   `impl`, members (unavailable until 07).
5. `impl type-expr … endimpl`: valid only as a direct child of a `type` or
   `interface`; at top level or inside anything else it is
   `@syntax.invalidForm`. Items: `type Name of T endtype` (associated
   choice), members (unavailable until 07).
6. `where`: `where expr [else expr]`; the operand tokens are captured and
   typed as expressions in step 06; here the section boundary is fixed.
7. Formatter: type expressions on one line when they fit, otherwise one
   entry per line; `case` lines one step in; associated types and choices
   as one-line declarations; `impl` blocks after members; the canonical
   section order. Invariant: no reordering of cases, fields, or type
   arguments.
8. Cases.

## Test matrix

| Case id | Kind | Input (`.vib`) | Expected |
| --- | --- | --- | --- |
| `V2-SRC-DECL-type-wrapper` | positive, formatter | `type Celsius of F64 endtype` | accepted; one line |
| `V2-SRC-DECL-type-record` | positive, formatter | `type User of {name: Str id: UserId}\n  visibility @public\nendtype` | accepted; formatted equals input |
| `V2-SRC-DECL-type-tuple` | positive | `type Pair of {I32 Str} endtype` | accepted; body is a tuple type with two components |
| `V2-SRC-DECL-type-variant` | positive, formatter | `type Lookup(T)\n  case found {value: T}\n  case missing\n  visibility @public\nendtype` | accepted; cases in order; formatted equals input |
| `V2-SRC-DECL-type-generic-given` | positive | `type Box(T)\n  of {item: T}\n  given {T: Printable}\nendtype` | accepted; `given` entry `T: Printable` |
| `V2-SRC-DECL-type-section-order` | formatter | `type A given {T: Any} of {x: T} visibility @public endtype` written with `given` before `of` | formatted puts `of` first, then `given`, then `visibility`, each on its line |
| `V2-SRC-DECL-type-callable-types` | positive | `type Handler of Fn({I32 verbose: Bool} Bool) endtype` and `type Job of Proc({Str} Void {DirWrite}) endtype` and `type Plan of Intent(Void {DirWrite}) endtype` | accepted; the row `{DirWrite}` is the last operand |
| `V2-SRC-DECL-type-assoc-label` | positive | `type Items of Iter(Item: U8) endtype` | accepted; one labelled type argument |
| `V2-SRC-DECL-type-missing-body` | negative | `type A endtype` | `@syntax.invalidForm` over the form |
| `V2-SRC-DECL-type-both-of-and-case` | negative | `type A of U8 case x endtype` | `@syntax.invalidForm` |
| `V2-SRC-DECL-type-paren-type` | negative | `type A of (U8) endtype` | `@syntax.invalidForm` over `(U8)` |
| `V2-SRC-DECL-type-mixed-product` | negative | `type A of {U8 name: Str} endtype` | `@syntax.invalidForm` over the brace form |
| `V2-SRC-DECL-type-invalid-section` | negative | `type A of U8 performs {X} endtype` | `@syntax.invalidSection` at `performs {X}` |
| `V2-SRC-DECL-type-role-outside-stdlib` | negative | `type A of U8 role @bool endtype` | `@syntax.invalidSection` at `role @bool` |
| `V2-SRC-DECL-type-intrinsic-outside-stdlib` | negative | `type A of Intrinsic(@i32) endtype` | `@syntax.invalidSection` at the body |
| `V2-SRC-DECL-type-bad-tag` | negative | `type A of U8 visibility @friends endtype` | the Q9 code at `@friends` |
| `V2-SRC-DECL-interface-basic` | positive, formatter | `interface Printable\n  visibility @public\n  fn render\n    takes {value: Self}\n    returns text: Str\n  endfn\nendinterface` | accepted with `@tool.unavailable` over the `fn` member until 07; re-recorded as accepted in 07 |
| `V2-SRC-DECL-interface-assoc` | positive | `interface Iter\n  type Item endtype\nendinterface` | accepted; one associated type `Item` |
| `V2-SRC-DECL-interface-assoc-with-body` | negative | `interface I type Item of U8 endtype endinterface` | `@syntax.invalidForm` over the nested `type` |
| `V2-SRC-DECL-impl-in-type` | positive | `type Once of Option(U8)\n  impl Iter\n    type Item of U8 endtype\n  endimpl\nendtype` | accepted; the choice `Item of U8` |
| `V2-SRC-DECL-impl-top-level` | negative | `impl Printable endimpl` | `@syntax.invalidForm` over the form |
| `V2-SRC-DECL-impl-with-section` | negative | `type A of U8 impl P visibility @public endimpl endtype` | `@syntax.invalidSection` |
| `V2-SRC-READER-type-recovery` | recovery | `type A of {x: endtype\ntype B of U8 endtype` | `@syntax.invalidForm` on the first; `B` exact; formatter returns bytes unchanged |

## Diagnostics and schema changes

Codes: `@syntax.invalidForm`, `@syntax.invalidSection`,
`@syntax.duplicateSection`, `@syntax.unmatchedDelimiter`,
`@tool.unavailable`, and the Q9 code. No schema change.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-syntax -p vibra-fmt
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: every case above passes; the step 04 case `V2-SRC-DECL-use-first` is
re-recorded as fully accepted.

## Excluded

Nested `fn`/`proc` members (07); typing of `where` operands (06); every
semantic rule of the type chapter.

## Completion evidence

- [ ] One reader case exists per production `type-decl`, `type-body`, `case`,
      `type-item`, `interface-decl`, `assoc-type`, `impl`, `assoc-choice`,
      `type-params`, `given`, `where`, `visibility`, `representation`, `role`,
      `doc`, `type-expr`, `record-type`, `inputs`, `row`, `path`, and the owner
      table names it.
- [ ] The section admissibility table in code has one row per declaration kind
      and a host test checks it against the chapter's grammar.
- [ ] All cases pass; validation commands exit 0.
