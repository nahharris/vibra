# Step 27: local-dependencies

Kind: vertical. Slice 4, the package half: a `@path` dependency binds one
library target of another package into the unit graph, without a lock.
Depends on: 26.
Crates: `vibra-workspace`, `conformance/cases`.

## Prerequisites

- Read: [programs
  chapter](../../../../spec/04-programs-and-packages.md#dependencies-and-lock)
  "Dependencies and lock" (the first two paragraphs and the rejection
  list), "Packages, units, and targets", "Discovery and the source
  snapshot"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md#availability-discovery-and-resolution)
  the rows of `@project.dependencyCycle`, `@project.duplicateName`,
  `@project.reservedUnitName`, the dependency key, and the dependency's
  `@project.notFound`; [the owner table](../../m0/owners.md);
  [questions](../spec-questions.md) Q23.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-workspace/src/project.rs:255` | `/// Dependencies are unavailable in this release.` | a `@path` dependency becomes available |
| `crates/vibra-workspace/src/project.rs:444` | `fn dependency` | the dependency schema |
| `crates/vibra-workspace/src/snapshot.rs:126` | `pub struct SourceSnapshot` | gains the dependency's files and fingerprint |
| `crates/vibra-workspace/src/source_graph.rs:79` | `pub struct SourceGraph` | gains the dependency's unit |

Proposed: `vibra-workspace::dependencies::{load, DependencyUnit}`.

## Files owned

- `crates/vibra-workspace/src/dependencies.rs`
- `crates/vibra-workspace/src/project.rs`
- `crates/vibra-workspace/src/snapshot.rs`
- `crates/vibra-workspace/src/source_graph.rs`
- `crates/vibra-workspace/src/resolve.rs`
- `crates/vibra-workspace/tests/it/dependencies.rs`
- `conformance/rules/m2-27.tsv`
- the case directories of the matrix below

## Ordered tasks

1. A `@path` dependency: its `path` is relative to the directory of the
   project file that declares it and stays beneath the root project's
   directory, first as written, then by its canonical directory
   (`@module.pathEscape` at `path` otherwise); that directory holds a
   `project.vibon` (`@project.notFound` at `path` otherwise).
2. The key selects the library target of that name: a key that names no
   target of the package is `@name.unknownSymbol` at the key, and one that
   names a `@bin` target is `@name.wrongEntityKind` at the key. There is
   no `target` field: one written is an unknown field of the closed
   record, `@data.invalidShape`.
3. The dependency's unit is named by its key, which is the target's own
   name, in every package of the snapshot: unit names are global, and two
   different units with one name are `@project.duplicateName` at the later
   key or name in discovery order. Its modules are those of the selected
   target's root. Its own dependencies load the same way, with an explicit
   work list; a cycle is `@project.dependencyCycle` at the entry that
   closes it in discovery order, related along the shortest chain.
4. The snapshot holds the dependency's files and fingerprints them, so
   the workspace revision changes when a dependency's source does.
5. An import of an undeclared unit stays `@module.unknownPath`. A
   dependency's private declarations stay private.
6. A `@git` dependency is `@tool.unavailable` at its entry. No lock file
   is read (Q23).

## Positions

| Rule | Positions |
| --- | --- |
| a dependency binds a library | the key names one of several libraries; the key names no target; the key names a binary; a written `target` field |
| the unit is importable | from a target module; from a test module; a public and a private declaration |
| graph shape | chain of two; diamond; cycle of two; self |
| names are global | a key equal to a root target; a transitive key equal to a root target |

## Test matrix

`workspaceCheck`, profile `staticV2`; a row that names another operation has
that operation's profile (Q34). Trees with a second package under
`lib/money/` holding `project.vibon` and `src/money/amount.vib`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-PROJECT-path-dependency-no-lock` | positive | `dependencies: ["money": {kind: @path path: "lib/money"}]`; `use @money.amount enduse` and a call of a public function | accepted, with no `project-lock.vibon` (Q23) | 04: "Local dependencies are not copied and are fingerprinted in every snapshot" |
| `V2-PROJECT-path-dependency-from-test` | positive, `workspaceTest` | a test module importing `@money.amount` | `@test.passed` | 04 Packages, units, and targets |
| `V2-PROJECT-path-dependency-private` | negative | a call of a private function of `@money.amount` | `@name.privateAccess` | 04: no private-access exception |
| `V2-PROJECT-path-dependency-undeclared` | negative | `use @money.amount enduse` with no `dependencies` entry | `@module.unknownPath` at the import | 04: "an import of an undeclared unit, `@module.unknownPath` at the import" |
| `V2-PROJECT-path-dependency-escape` | negative | `path: "../money"` | `@module.pathEscape` at `path` | 04: "one that resolves outside it is `@module.pathEscape` at the `path` value" |
| `V2-PROJECT-path-dependency-missing` | negative | `path: "lib/nope"` | `@project.notFound` | 07: "no `project.vibon` at or above the workspace" |
| `V2-PROJECT-dependency-target-named` | positive | the package has two `@lib` targets; the key `"amounts"` | accepted, the key binds `@amounts` | 04: "A dependency's key is the name of the library target it binds in the dependency's package" |
| `V2-PROJECT-dependency-target-unknown` | negative | the key `"money"`; the package's only target is `@cash` | `@name.unknownSymbol` at the key | 04: "A key that names no target of the package is `@name.unknownSymbol` at the key" |
| `V2-PROJECT-dependency-target-binary` | negative | the key names a `@bin` target | `@name.wrongEntityKind` at the key | 04: "a dependency never binds a binary target" |
| `V2-PROJECT-dependency-target-field` | negative | a dependency with `target: @money` | `@data.invalidShape` at the field | 04: "there is no renaming" |
| `V2-PROJECT-dependency-transitive` | positive | `app` depends on `money`, `money` on `units`; `app` imports only `@money` | accepted | 04: "package dependencies form a directed acyclic graph" |
| `V2-PROJECT-dependency-transitive-not-visible` | negative | `app` imports `@units` without declaring it | `@module.unknownPath` | 04: "an import of an undeclared unit" |
| `V2-PROJECT-dependency-diamond` | positive | `app` depends on `a` and `b`, both on `units` | accepted; one unit `units` | the same |
| `V2-PROJECT-dependency-cycle` | negative | `money` depends on `app` by path | `@project.dependencyCycle` at `money`'s entry, related to the root's entry | 07: "a dependency cycle" |
| `V2-PROJECT-dependency-name-collision` | negative | a dependency named as a target of the project | `@project.duplicateName` at the dependency's key | 07: "two different units with one name anywhere in the snapshot" |
| `V2-PROJECT-dependency-name-global` | negative | `money` depends on a package `other` whose library is `@app`, the root's own target | `@project.duplicateName` at `money`'s key, related to the root target's name | 04: "a unit's name is global" |
| `V2-PROJECT-dependency-named-std` | negative | a dependency keyed `"std"` | `@project.reservedUnitName` at the key | 07: "a target or dependency named `tests` or `std`" |
| `V2-PROJECT-git-dependency-unavailable` | boundary | `{kind: @git git: "https://example.com/money.git" rev: "0123456789abcdef0123456789abcdef01234567"}` | `@tool.unavailable` at the entry; verdict `unsupported` (Q23) | 07: "a valid … construct outside the tool's profile" |
| `V2-PROJECT-dependency-revision` | positive, host test | two snapshots that differ in one byte of `lib/money/src/money/amount.vib` | two workspace revisions | 04: "fingerprinted in every snapshot" |
| `V2-PROJECT-dependency-operation-interface` | positive | `money` declares an operation interface; `app` has a `proc` with it in `performs` | accepted; the root's identity is the dependency's declaration | 03: "Its identity is its declaration's identity" |

## Diagnostics and schema changes

Codes: `@module.pathEscape`, `@module.unknownPath`, `@project.notFound`,
`@project.dependencyCycle`, `@project.duplicateName`,
`@project.reservedUnitName`, `@name.unknownSymbol`, `@name.wrongEntityKind`,
`@name.privateAccess`, `@data.invalidShape`, `@tool.unavailable`. The
dependency record of the project file has no `target` field.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-workspace -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0.

## Stays unavailable

`@tool.unavailable`: a `@git` dependency. `project sync` and the lock
have no owner in this release.

## Excluded

`project-lock.vibon`, `@project.staleLock`, vendoring, `project add`.

## Completion evidence

- [ ] `conformance/rules/m2-27.tsv` has one `active` row per case id.
- [ ] A dependency chain of 1,000 packages, generated by a host test,
      loads without overflow on a 256 KiB thread.
- [ ] The validation commands exit 0.
