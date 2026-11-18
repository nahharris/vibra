# Step 10: resolve-declarations

Kind: vertical. The new `vibra-resolve` crate: declarations with anchor
identities, the flat module scope, the three namespaces, derived import
names, import cycles, private access, prelude reservations, member
namespaces, and paths in type positions. Local bindings and expression
paths are step 11.

## Prerequisites

- Steps 07 and 09 landed.
- Read: [programs chapter](../../../../spec/04-programs-and-packages.md)
  "Modules and imports", "The standard library" (the `@std` unit and module
  list; the loader is step 14); [type
  chapter](../../../../spec/02-type-system.md) "The prelude", "Declared types"
  (identity paragraph, member namespace), "Namespaces and resolution",
  "Generics" (`@name.genericRedeclaration`); [source
  chapter](../../../../spec/01-source-language.md) "Declarations", "`use`",
  "Names and paths"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md) "Availability,
  discovery and resolution", the `@resolved.v2` row, the `@style.casing` rule.
- Open questions: Q13 (the identity encoding) must be answered before the
  `@resolved.v2` snapshots are committed; Q14 (`I.Item` on an unbounded
  parameter) is step 19's.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-resolve-v1/src/lib.rs:123` (`SourceModule`), `:178` (`SourceUnit`), `:425` (`EntityKind`), `:771` (`ResolvedSnapshot`), `:1038` (`Resolver`) | the neutral input and snapshot shapes are kept; the resolution rules are rewritten |
| `crates/vibra-workspace/src/source_graph.rs` (step 09) | the adapter from graph to `ResolveInput` |
| `crates/vibra-conformance-v1/src/resolve.rs` | the handler shape |

Proposed: `vibra-resolve::{input, graph, declare, imports, namespaces, paths,
snapshot}`; `vibra-workspace::resolve` adapter; `vibra-conformance::resolve`
handler. Until step 14 the standard library overlay is absent and the prelude
names are a fixed table in `vibra-resolve::prelude` (the closed set the type
chapter lists); step 14 binds them to real declarations.

## Ordered tasks

1. Collect declarations per module: the seven kinds, nested members, cases,
   fields, associated types, `impl` blocks (identified by target and
   receiver). Compute each anchor (package, unit, module path, owner path,
   kind, name, scheme version) and its identity per Q13. Invariant: the
   identity does not change when a body changes.
2. Module scope: one flat scope; a repeated top-level name or import name
   is `@name.redeclaration` at the later with the earlier related; the
   later still binds. One owner's fields, cases, nested members, and
   associated types share one namespace (`@name.memberCollision`). A
   nested member or `impl` redeclaring an inherited type parameter is
   `@name.genericRedeclaration`; a `given` entry for a name the header does
   not declare is `@name.unknownSymbol`.
3. Prelude: a declaration or binder spelled as a prelude type or interface
   name is `@name.reservedDeclaration`; a module value, function, or import
   spelled as a prelude name is `@name.reservedValueSpelling`; both still
   bind.
4. Imports: locator resolution by walking parts from the unit's root
   (`@module.unknownPath`), module import versus declaration import, the
   local name from the last part or `as`; a private declaration imported is
   `@name.privateAccess`; `@module.importCycle` at the later edge with the
   cycle related; an import name equal to a declaration or another import is
   `@name.redeclaration`. A target module importing `@tests` is
   `@module.unknownPath`. Invariant: resolution is total; access is checked
   by the referring position.
5. Paths in type positions and in section operands: `given` bounds, `impl`
   targets, type arguments, `use` locators; `@name.unknownSymbol`;
   `@name.wrongEntityKind` names what was found; a type position resolves
   across the type and interface namespaces.
6. Casing: `@style.casing` (warning) for a declaration name that departs
   from the convention; meaning unchanged.
7. `@resolved.v2` snapshot: `format`, `package`, `modules`, `declarations`
   (anchor, identity, kind, visibility, source, span), `imports`,
   `references` (type-position references in this step). The `resolve`
   handler.
8. Cases, as confined trees with `project.vibon`.

## Test matrix

| Case id | Kind | Input (modules under `src/app/`) | Expected |
| --- | --- | --- | --- |
| `V2-TYPE-NAMES-flat-scope-forward` | positive | `main.vib`: `type A of B endtype\ntype B of U8 endtype` | accepted; `A`'s body references `B` declared later |
| `V2-TYPE-NAMES-redeclaration` | negative | `type A of U8 endtype\ntype A of U16 endtype` | `@name.redeclaration` at the second `A`, related to the first |
| `V2-TYPE-NAMES-import-name-collision` | negative | `use @std.io enduse\ntype io of U8 endtype` | `@name.redeclaration` at `io` (the type), related to the import |
| `V2-TYPE-NAMES-member-collision` | negative | `type U of {name: Str}\n  fn name takes {u: Self} returns s: Str do return u.name endfn\nendtype` | `@name.memberCollision` at the member `name` |
| `V2-TYPE-NAMES-generic-redeclaration` | negative | `type Box(T) of {item: T}\n  fn f(T) takes {b: Self} returns r: T do return b.item endfn\nendtype` | `@name.genericRedeclaration` at the inner `T` |
| `V2-TYPE-GENERIC-given-unknown` | negative | `type Box(T) of {item: T} given {U: Any} endtype` | `@name.unknownSymbol` at `U` |
| `V2-TYPE-NAMES-reserved-declaration` | negative | `type Option of U8 endtype` | `@name.reservedDeclaration` at `Option` |
| `V2-TYPE-NAMES-reserved-value` | negative | `let true: U8 be 1u8 endlet` | `@name.reservedValueSpelling` at `true` |
| `V2-TYPE-NAMES-reserved-import` | negative | `use @app.util.Option enduse` with `util.vib` declaring nothing named so | `@module.unknownPath` first; with a real `Option` declared in `util` the case is `@name.reservedDeclaration` there and `@name.reservedValueSpelling` at the import |
| `V2-PROJECT-import-module` | positive | `use @app.model enduse` with `model.vib` declaring public `User`; `main.vib` uses `model.User` in a type | accepted; one module import |
| `V2-PROJECT-import-declaration` | positive | `use @app.model.User enduse`; `User` used bare | accepted |
| `V2-PROJECT-import-alias` | positive | `use @app.model as m enduse`; `m.User` | accepted |
| `V2-PROJECT-import-private` | negative | `use @app.model.Secret enduse` where `Secret` lacks `visibility @public` | `@name.privateAccess` at the import, related to `Secret` |
| `V2-PROJECT-import-unknown` | negative | `use @app.nothere enduse` | `@module.unknownPath` at the locator |
| `V2-PROJECT-import-cycle` | negative | `a.vib` imports `@app.b`, `b.vib` imports `@app.a` | `@module.importCycle` at the later edge, related along the cycle |
| `V2-PROJECT-import-case-exact` | negative | `use @app.Model enduse` for `model.vib` | `@module.unknownPath` |
| `V2-PROJECT-import-tests-from-target` | negative | `main.vib`: `use @tests.t enduse` | `@module.unknownPath` |
| `V2-TYPE-NAMES-wrong-kind-bound` | negative | `type Box(T) of {item: T} given {T: U8} endtype` | `@name.wrongEntityKind` at `U8`, naming a type where an interface is required |
| `V2-TYPE-NAMES-unknown-type` | negative | `type A of Nothere endtype` | `@name.unknownSymbol` at `Nothere` |
| `V2-TYPE-NAMES-impl-target-kind` | negative | `type A of U8 impl U16 endimpl endtype` | `@name.wrongEntityKind` at `U16` |
| `V2-DIAG-style-casing` | warning | `type user of U8 endtype` | `@style.casing` at `user`; accepted |
| `V2-TYPE-NAMES-resolved-snapshot` | positive | the module-import case | the `@resolved.v2` snapshot (hand-written from the chapter and Q13) |

## Diagnostics and schema changes

Codes: `@name.redeclaration`, `@name.memberCollision`,
`@name.genericRedeclaration`, `@name.unknownSymbol`,
`@name.wrongEntityKind`, `@name.privateAccess`,
`@name.reservedDeclaration`, `@name.reservedValueSpelling`,
`@module.unknownPath`, `@module.importCycle`, `@style.casing`. Schema:
`@resolved.v2`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-resolve -p vibra-workspace -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: the `resolve` cases pass; a host test shows the identity of `User`
is unchanged when its field list changes and changed when it is renamed.

## Excluded

Local bindings, expression paths, constant patterns (11); the standard
library overlay (14); every type rule.

## Completion evidence

- [ ] `conformance/rules.tsv` has `V2-TYPE-NAMES` and `V2-DIAG` active.
- [ ] The identity scheme is implemented as Q13 answers, with a host test
      vector.
- [ ] All cases pass; validation commands exit 0.
