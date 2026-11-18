# Step 12: checked-ir

Kind: infrastructure. The new `vibra-ir` crate is the boundary every other
crate of the pipeline builds against. It carries no language behavior of
its own: a program reaches it only through the checker. It is closed here
for the whole of M1 so that the interpreter (13), the checker (15 to 22),
the runtime port (27), and the native track (X2-01) build against one
shape.

## Prerequisites

- Step 02 landed.
- Read: [type chapter](../../../../spec/02-type-system.md) whole;
  [runtime chapter](../../../../spec/06-runtime.md) "Semantic reference",
  "Evaluation", "Callable contract execution", "Generic instantiation",
  "Tail calls", "Compiler intrinsic registry", "Value arena" (value kinds),
  "Canonical value encoding", "External providers", "Host registry" (the
  rows, as data); [architecture](../architecture.md#boundary-types)
  "Boundary types"; [Nexus design](../../../nexus/design.md) "From checked
  IR".
- Open questions: Q13 (identity encoding) for `TypeId`.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-ir-v1/src/lib.rs:40` (`Type`), `:563` (`FunctionSignature`), `:1006` (`Expr`), `:2705` (`CheckedGlobal`), `:2779` (`CheckedFunction`), `:3250` (`CheckedModuleSet`), `:3426` (`CheckedProgram`) | the validated-construction style (`try_new`, shape validation, slot counting) is kept; the node set is replaced |
| `crates/vibra-ir-v1/src/nominal.rs:81` (`TypeDefinition`) | gains variant cases with named fields, invariants, representation |
| `crates/vibra-ir-v1/src/external/{mod.rs,host.rs,vectors.rs}` | the compiler intrinsic table with v1 wire symbols; source names become camelCase; the host rows become data only |
| `crates/vibra-ir-v1/src/observed.rs` and `canonical_*` | `@value.v2` replaces `@value.v1` |

Proposed: `vibra-ir::{types, nominal, signature, expr, pattern, constant, value,
module_set, program, encode, external::{compiler, host}, migration}`.

## Ordered tasks

1. `Type` as the architecture lists: scalars, `List`, `Tuple`, `Record`
   (canonical field order), `Fn`, `Proc`, `Intent` (rows as sorted identity
   lists), `Declared(TypeId, args)`, `Interface(TypeId, args, assoc)`,
   `Param`, `Any`, `AtomSingleton`, `Never`. No `Bool`, `Str`, `Bytes`,
   `Dict`, `Option`, `Result`: these are `Declared` with identities the
   checker binds by role. Invariant: structural equality is type equality;
   record fields compare as sets.
2. `TypeDefinition { id, parameters, body, invariant, representation }`
   with `TypeBody::{Record, Tuple, Wrapper, Variant(cases with named
   fields)}`; `InterfaceDefinition { id, parameters, assoc_types, members }`;
   `Implementation { interface, args, receiver, assoc_choices, members }`.
3. `Expr`: the node set of the architecture, including `LetElse`,
   `Construct` (whose result is `Result(T E)` when the type's invariant has
   a consequence), `Field`, `Lookup`, `Call { target: Direct | Value |
   Contract { interface, member, receiver } }`, `TailCall`, `Return`,
   `Try`, `WidenToInterface`, `WidenAtom`, `Default`. No union, no
   variadic, no `As`, no bare-guard node. Guards with consequences live on
   `CheckedFunction { entry_guard, exit_guard }` and `Invariant`.
   Invariant: `validate_shape` rejects an `Expr` whose slot types disagree
   and a `Call` whose arity disagrees, so a checker defect cannot execute.
4. `Constant`, `Value` (scalar, atom, string, bytes, tuple, list, dict,
   record, variant, wrapper, function, closure), `canonical_value` and
   `canonical_type` for `@value.v2` and `@type.v2` with the chapter's
   vectors as host tests; functions have no encoding and `encode` returns
   an error for them.
5. `CheckedModuleSet::try_new` with static dependency edges;
   `CheckedProgram::for_entry` with call-flow analysis for initializer
   cycles (`IrError::InitializerCycle`), as v1 does.
6. `external::compiler`: one row per intrinsic of the runtime chapter's
   registry with its wire symbol (v1 spelling, `i32.add-checked`) and
   camelCase source name (`addChecked`), exact signature over role-bound
   types, and sample vectors (ported from `vectors.rs`). `external::host`:
   the 17 rows as data (symbol, owner, parameters, result) for M2 and step
   27; no behavior.
7. `migration.md` in the crate: a table from every v1 node and type case to
   its successor or "removed", for X2-01 and step 27.
8. Architecture table row: `vibra-ir: [vibra-diagnostics]`.

## Test matrix

Host tests only (infrastructure).

| Test | Input | Expected |
| --- | --- | --- |
| canonical type vectors | each row of "Types" in the runtime chapter | the exact VIBON text, for `List(I32)`, `{I32 Str}`, `{a: I32 b: Str}` (canonical field order), an atom singleton, `Fn({I32 verbose: Bool} Bool)`, a declared type with arguments |
| canonical value vectors | each row of "Values" | bytes, tuple, list, dict in key order, record, variant with and without payload, wrapper |
| no encoding for functions | a closure value | an encoding error, no panic |
| shape validation | a `Call` with the wrong arity; a `Let` whose body reads an unset slot | `IrError` |
| initializer cycle | globals `a` reading `b` through a function, `b` reading `a` | `IrError::InitializerCycle` naming one global |
| no cycle through functions alone | mutually recursive functions | accepted |
| intrinsic table | every row | source name camelCase, wire symbol unchanged from v1, signature arity matches the chapter table; `div` and `rem` plain do not exist |
| host table | the 17 rows | symbols equal the chapter's; owners and modules match |
| migration table | every `vibra-ir-v1` public variant | appears exactly once |

## Diagnostics and schema changes

No diagnostic. Schemas: `@value.v2`, `@type.v2` (VIBON encodings),
implemented here and asserted against the chapter's text.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-ir
cargo doc --locked --offline -p vibra-ir --no-deps
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: every vector test passes; `cargo doc` has no warning (the crate's
public items carry docs, which the migration table links).

## Excluded

Any execution (13); any checking (15 onward); host behavior (M2).

## Completion evidence

- [ ] `crates/vibra-ir/migration.md` lists every v1 `Type` and `Expr` variant
      with its successor or `removed`.
- [ ] The canonical encodings match every example of the runtime chapter byte
      for byte (host tests named per row).
- [ ] The architecture test lists `vibra-ir` with `vibra-diagnostics` only.
- [ ] Validation commands exit 0.
