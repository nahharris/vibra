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
  discovery and resolution", `@resolved.v2` under "Persistent
  observations", the `@style.casing` rule.

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
   receiver). Compute each declaration's identity as the type chapter
   defines it: `sha256:` and the SHA-256 of the text
   `vibraDeclarationIdentityV2`, one zero byte, and six strings, each
   prefixed by its byte length as a big-endian `U64`: the package name,
   the package version, the module path atom with its `@`, the owner
   path, the kind atom, and the name. The kind is one of `@module`,
   `@type`, `@interface`, `@case`, `@fn`, `@proc`, `@value`, `@test`,
   `@associatedType`. Invariant: the identity does not change when a body
   changes.
2. Module scope: one flat scope; a repeated top-level name or import name
   is `@name.redeclaration` at the later with the earlier related; the
   later still binds. One owner's fields, cases, nested members, and
   associated types share one namespace (`@name.memberCollision`). A
   nested member or `impl` redeclaring an inherited type parameter is
   `@name.genericRedeclaration`, and so is an associated type whose name
   equals a visible type parameter; a `given` key that is not a parameter
   of the header is `@name.unknownSymbol`, and a repeated key is
   `@name.redeclaration`.
3. Prelude: a type, interface, type parameter, or pattern binder spelled
   as a prelude name is `@name.reservedDeclaration` at the name; a module
   value, `fn`, `proc`, or import spelled as one is
   `@name.reservedValueSpelling` at the name; either still binds.
4. Imports: locator resolution by walking parts from the unit's root
   (`@module.unknownPath`), module import versus declaration import, the
   local name from the last part or `as`; an import names a module or one
   top-level declaration, so a path that resolves to a member, a case, or
   a field is `@name.wrongEntityKind` at the import; a private declaration
   imported is
   `@name.privateAccess`; `@module.importCycle` at the later edge with the
   cycle related; an import name equal to a declaration or another import is
   `@name.redeclaration`. A target module importing `@tests` is
   `@module.unknownPath`. Invariant: resolution is total; access is checked
   by the referring position.
5. Paths in type positions and in section operands: `given` bounds, `impl`
   targets, type arguments, `use` locators; `@name.unknownSymbol`;
   `@name.wrongEntityKind` names what was found (a bound that is not an
   interface is one); a type position resolves across the type and
   interface namespaces. An associated type is named only by its bare name
   inside its interface or `impl`, or as a label in a type application.
   There is no projection path: in `I.Item` and `Once.Item` the part
   `Item` is `@name.unknownSymbol`, related to the parameter or type.
6. Casing: `@style.casing` (warning) for a declaration name that departs
   from the convention; meaning unchanged.
7. `@resolved.v2` snapshot, the closed schema of the diagnostics chapter:
   `format`, `package`, `modules`, `declarations` (each with `identity`,
   `anchor`, `access`, `span`), `imports` (each with `span`, `path`,
   `name`, `to`), and `references` (type-position references in this
   step). A span is the record `{sourceId start end}`. The `resolve`
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
| `V2-TYPE-NAMES-reserved-import` | negative | `use @app.util as true enduse` | `@name.reservedValueSpelling` at `true` |
| `V2-TYPE-NAMES-reserved-type-parameter` | negative | `type Box(Option) of {item: U8} endtype` | `@name.reservedDeclaration` at the parameter `Option` |
| `V2-TYPE-GENERIC-given-repeated` | negative | `type Box(T) of {item: T} given {T: Any T: Ordered} endtype` | `@name.redeclaration` at the second `T` |
| `V2-TYPE-NAMES-assoc-shadows-parameter` | negative | `interface Source(Item) type Item endtype endinterface` | `@name.genericRedeclaration` at the associated `Item` |
| `V2-TYPE-NAMES-no-projection-path` | negative | `fn first(I) takes {it: I} returns r: Option(I.Item) given {I: Iter} do return Option.none() endfn` | `@name.unknownSymbol` at `Item`, related to the parameter `I` |
| `V2-PROJECT-import-member` | negative | `use @app.model.User.nameLength enduse` | `@name.wrongEntityKind` at the import |
| `V2-PROJECT-import-std-unit` | positive | `use @std.core enduse` in a project that declares no dependency | resolves to the unit `@std` (with the placeholder table until step 14) |
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
| `V2-TYPE-NAMES-resolved-snapshot` | positive | the module-import case | the `@resolved.v2` snapshot, hand-written from the chapter, with identities computed from the framing by hand |
| host: identity vector | positive | package `hello`, version `0.1.0`, module `@hello.main`, empty owner path, kind `@proc`, name `main` | `sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad` |

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

- [ ] `conformance/rules.tsv` has one `active` row per case id above.
- [ ] The identity test vector of the type chapter passes as a host test.
- [ ] All cases pass; validation commands exit 0.
