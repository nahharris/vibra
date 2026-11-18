# Step 12: checked-ir

Kind: infrastructure. The new `vibra-ir` crate is the boundary every other
crate of the pipeline builds against. It carries no language behavior of
its own: a program reaches it only through the checker. Its node set is
closed here for the whole of M1 so that the interpreter (13), the checker
(15 to 22), the runtime port (27), and the native track (X2-01) build
against one shape.

This step is the checked-IR part of the packages N1, N2, N3, N5, and N6 of
the [native contract](../../m0/native-contract.md#7-suggested-package-cut),
and the type forms of N4, as the
[architecture](../architecture.md#native-contract-packages) maps them. It
edits no `-v1` crate.

## Prerequisites

- Step 02 landed.
- Read: [type chapter](../../../../spec/02-type-system.md) whole;
  [runtime chapter](../../../../spec/06-runtime.md) "Semantic reference",
  "Evaluation", "Callable contract execution", "Generic instantiation",
  "Tail calls", "Compiler intrinsic registry" with its three operation
  tables, "Canonical value encoding" with "Types", "Open signatures",
  "Values", and "What has no encoding", "Traps";
  [native contract](../../m0/native-contract.md) section 1;
  [architecture](../architecture.md#boundary-types) "Boundary types";
  [Nexus design](../../../nexus/design.md) "From checked IR".

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-ir-v1/src/lib.rs:40` (`Type`), `:563` (`FunctionSignature`), `:1006` (`Expr`), `:2705` (`CheckedGlobal`), `:2779` (`CheckedFunction`), `:3220` (`CheckedModuleSet`), `:3396` (`CheckedProgram`), `:3518` (`IrError`) | the validated-construction style (`try_new`, shape validation, slot counting) is kept; the node set is replaced |
| `crates/vibra-ir-v1/src/lib.rs:3152` (`validate_global_initializer_cycles`) | the call-flow analysis is carried over |
| `crates/vibra-ir-v1/src/nominal.rs:81` (`TypeDefinition`) | gains variant cases with named fields, invariants, representation |
| `crates/vibra-ir-v1/src/external.rs` and `external/vectors.rs` | the compiler intrinsic rows and their sample vectors |
| `crates/vibra-ir-v1/src/lib.rs:6033` (`canonical_string`), `:6067` to `:6111` (canonical NaN bits and float text), `observed.rs` | the string and float writers are carried over; `@value.v2` replaces `@value.v1` |

## Files

Created:

| Path | Contents |
| --- | --- |
| `crates/vibra-ir/Cargo.toml` | package `vibra-ir`; dependency `vibra-diagnostics` |
| `crates/vibra-ir/src/lib.rs` | module list and re-exports |
| `crates/vibra-ir/src/types.rs` | `Type`, `TypeId`, `EntityRef`, `Row`, callable signatures |
| `crates/vibra-ir/src/nominal.rs` | `TypeDefinition`, `TypeBody`, `Case`, `Invariant`, `InterfaceDefinition`, `Implementation` |
| `crates/vibra-ir/src/signature.rs` | `SignatureRecord`, `Binder`, `Bound`, `InterfaceKey`, `ImplementationRecord` |
| `crates/vibra-ir/src/expr.rs` | `Expr`, `CallTarget`, `Guard` |
| `crates/vibra-ir/src/pattern.rs` | checked patterns |
| `crates/vibra-ir/src/constant.rs`, `value.rs` | `Constant`, `Value`, `ObservedValue` |
| `crates/vibra-ir/src/origin.rs` | `SourceOrigin` |
| `crates/vibra-ir/src/trap.rs` | `Trap`, with exactly `InvalidCheckedProgram` and `InvalidHostValue` |
| `crates/vibra-ir/src/module_set.rs`, `program.rs` | `CheckedGlobal`, `CheckedFunction`, `CheckedModuleSet`, `CheckedProgram`, `IrError` |
| `crates/vibra-ir/src/encode.rs` | canonical bytes of `@type.v2`, `@value.v2`, signature records, and implementation records |
| `crates/vibra-ir/src/external.rs`, `external/compiler.rs`, `external/vectors.rs` | the `@compiler` registry, major 2 minor 0 |
| `crates/vibra-ir/registry/compiler-2.0.txt` | the registry snapshot: one line per row, `symbol`, generics, parameters, result, sorted by symbol |
| `crates/vibra-ir/migration.md` | the table from every v1 node and type case to its successor or `removed` |
| `crates/vibra-ir/tests/it/main.rs`, `encode.rs`, `shape.rs`, `registry.rs`, `migration.rs` | host tests |

Edited: the root `Cargo.toml` and `Cargo.lock` (one package);
`crates/vibra-conformance/tests/it/architecture_boundary.rs` (row
`vibra-ir: [vibra-diagnostics]`).

## Ordered tasks

1. `Type`: the scalars `I8` to `U64`, `F32`, `F64`, `Char`, `Atom`, `Void`,
   `Never`; `Any`; `AtomSingleton`; `List`; `Tuple`; `Record` (canonical
   field order); `Fn`, `Proc`, `Intent`, where a row is a list of
   `EntityRef { path, seed }` sorted by the UTF-8 bytes of `path`;
   `Declared(TypeId, arguments)`; `Interface(TypeId, arguments, choices)`
   with one choice per associated type. Three open forms are valid only
   inside a signature or implementation record: `Binder(index)`, `SelfType`,
   `Associated(name)`. There is no union, no anonymous variant, no `Bool`,
   `Str`, `Bytes`, `Dict`, `Option`, or `Result` case: those are `Declared`
   with identities the checker binds by role. `TypeId` is the pair of the
   canonical path and the identity string of the type chapter; this crate
   stores it and never computes it. Invariant: structural equality is type
   equality; record fields compare as sets.
2. `TypeDefinition { id, parameters, body, invariant, representation }` with
   `TypeBody::{Record, Tuple, Wrapper, Variant(cases with named fields)}`; a
   builtin declared `of Intrinsic(@tag)` has no definition here, because its
   type is the scalar or `List` case of `Type`;
   `InterfaceDefinition { id, parameters, associated, members }`;
   `Implementation { binders, contract: InterfaceKey, receiver, choices, members
   }`, the key being the interface, the receiver, and the positional arguments.
3. `Expr`: `Literal`, `Intrinsic`, `Sequence`, `Variable`, `Global`,
   `Function`, `Closure`, `Captured`, `Let`, `LetElse`, `Match`, `If`,
   `Construct` (whose result is `Result(T E)` when the type's invariant has
   a consequence), `Field`, `Lookup`, `Call { target: Direct | Value |
   Contract { interface, member, receiver } }`, `TailCall`, `Return`,
   `Try`, `WidenToInterface`, `WidenAtom`, `Default`. There is no union
   widening, no member pattern, no tuple projection, no variadic tail, no
   `ArrayOf` or `DictOf`, no integer `Div` or `Rem`, no row tail, and no
   plan or `await` node. Guards with a consequence live on
   `CheckedFunction { entry_guard, exit_guard }` and on `Invariant`; a
   bare guard is discharged by the checker and leaves no node. Invariant:
   `validate_shape` rejects an `Expr` whose slot types disagree and a
   `Call` whose arity disagrees, so a checker defect cannot execute.
4. `Constant`, `Value` (scalar, atom, string, bytes, tuple, list, dict,
   record, variant, wrapper, interface value, function, closure).
   `encode`: the canonical bytes of the chapter, on one line with no
   trailing newline, written by this crate's own writer. A type record has
   its fields in the chapter's order with `kind` first; a scalar is its
   bare wire atom; `typeInfo` is the label of a type everywhere. A
   non-finite float is `{kind: @nonFinite value: …}` with one canonical
   NaN. An interface value is `{kind: @interface held: <observation>}`.
   A function, a closure, a plan, and a grant have no encoding: `encode`
   returns an error for them and never panics. An open form outside a
   signature or implementation record is rejected.
5. `signature`: the signature record `{binders receiver typeInfo}`, the
   binder record `{name bound}` with `bound` absent for `Any`, the bound
   record `{contract choices}`, the interface key `{path seed arguments}`,
   and the implementation record `{binders contract receiver choices}`.
6. `CheckedModuleSet::try_new` with static dependency edges;
   `CheckedProgram::for_entry` with call-flow analysis for initializer
   cycles (`IrError::InitializerCycle`).
7. `external::compiler`: the closed registry at major 2, minor 0. The rows
   are derived by one mechanical procedure and frozen in
   `registry/compiler-2.0.txt`:
   - start from the rows of `crates/vibra-ir-v1/src/external.rs`;
   - delete plain integer `div` and `rem`, `array.of`, and `dict.of`,
     which the runtime chapter does not have;
   - rename `array` to `list` in every owner and member;
   - respell every wire symbol by the one rule of the chapter,
     `<owner>.<member>` with a lower-case owner and a camelCase member, so
     `i32.add-checked` becomes `i32.addChecked` and `bytes.from-array`
     becomes `bytes.fromList`;
   - add `list.next` and `dict.next` with the signatures of the chapter.
   Each row has its wire symbol, generics, parameter and result types over
   role-bound types, and its sample vectors, ported from `vectors.rs`. The
   three `grant.*` rows belong to M2. Invariant: the snapshot test fails
   when a row of the file changes or disappears.
8. `migration.md`: a table from every public variant of `Type`, `Expr`,
   `Value`, `CallTarget`, and `IrError` of `vibra-ir-v1` to its successor
   or `removed`, for step 27 and X2-01.

## Test matrix

Host tests only (infrastructure).

| Test | Input | Expected |
| --- | --- | --- |
| type vectors | each row of the table under "Types" | the exact canonical bytes, for `List(I32)`, `{I32 Str}`, `{b: Str a: I32}` (written `a` first), an atom singleton, `Fn({I32 verbose: Bool} Bool)` with `labels: ["verbose": …]`, `Proc` and `Intent` with `effects: {roots: []}`, a declared type with arguments |
| chapter type examples | the `@declared` example and the `@type.v2` example with the `Iter(Item: U8)` parameter | byte-equal after removing layout |
| value vectors | each row of the table under "Values" | bytes, tuple, list, dict in key order, record, variant with and without payload, wrapper |
| chapter value examples | `{format: @value.v2 typeInfo: @u8 value: 200u8}`; the `@nonFinite` example; the record of `-0.0f32` and `1e20f64`; `Any` holding `200u8` | byte-equal after removing layout |
| one NaN | an `F32` NaN with a sign and a payload | `{kind: @nonFinite value: @nan}` |
| no encoding | a closure value; an interface value holding a closure | an encoding error, no panic |
| signature records | `fn first(T)` and `Iter.next` of the chapter | the chapter's records: `binders: [{name: "T"}]` and `{kind: @binder index: 0u64}`; `receiver: 0u64`, `{kind: @self}`, `{kind: @associated name: "Item"}` |
| open form misplaced | `Binder(0)` inside a `@type.v2` document | rejected |
| shape validation | a `Call` with the wrong arity; a `Let` whose body reads an unset slot | `IrError` |
| initializer cycle | globals `a` reading `b` through a function, `b` reading `a` | `IrError::InitializerCycle` naming one global |
| no cycle through functions alone | mutually recursive functions | accepted |
| registry snapshot | `registry/compiler-2.0.txt` | equals the table; no symbol contains `-`; no `div`, `rem`, `of`, or `array` row |
| registry vectors | every row with vectors | the vector's stated result has the row's result type |
| traps | `Trap` | two variants; their atoms are `@runtime.invalidCheckedProgram` and `@runtime.invalidHostValue` |
| migration table | every public variant named in task 8 | appears exactly once |

## Diagnostics and schema changes

No diagnostic. Schemas: `@value.v2`, `@type.v2`, the signature record, and
the implementation record, implemented here and asserted against the
chapter's text.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-ir
cargo doc --locked --offline -p vibra-ir --no-deps
cargo test --locked --offline -p vibra-conformance
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: every vector test passes; `cargo doc` prints no warning; the
architecture test of `vibra-conformance` lists `vibra-ir`.

## Excluded

Any execution (13); any checking (15 onward); the proof that the reader of
`vibra-syntax` decodes every encoding this crate writes (step 15, the
first step that may depend on both); the modules `arena`, `boundary`,
`external::host`, and `external::native`, which step 27 adds with the
runtime crates that use them; plan and `await` nodes, grant values, and
the registry-major-2 host rows (M2, packages N4, N7, and N8).

## Completion evidence

- [ ] `crates/vibra-ir/migration.md` lists every v1 `Type` and `Expr`
      variant with its successor or `removed`.
- [ ] The canonical encodings match every example of "Types", "Open
      signatures", and "Values" (host tests named per example).
- [ ] `crates/vibra-ir/registry/compiler-2.0.txt` is committed and its
      snapshot test passes.
- [ ] The architecture test lists `vibra-ir` with `vibra-diagnostics` only.
- [ ] Validation commands exit 0.
