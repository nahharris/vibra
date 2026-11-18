# Step 14: stdlib-bootstrap

Kind: infrastructure. The new `stdlib/` in the new surface, limited to the
modules the first checker steps need, with the `@stdlibManifest.v2`
manifest and its loader in the new `vibra-types` crate. The modules are
parsed and verified here; they are checked from step 15 onward, because the
checker checks every prelude module in every graph.

## Prerequisites

- Steps 07 and 12 landed.
- Read: [type chapter](../../../../spec/02-type-system.md) "Language core and
  standard library", "The prelude", "Scalars", "Dict keys, equality, and order",
  "Conversion", "Iteration"; [programs
  chapter](../../../../spec/04-programs-and-packages.md) "The standard library";
  [runtime chapter](../../../../spec/06-runtime.md) "Compiler intrinsic
  registry", "Native implementations", "External providers"; [source
  chapter](../../../../spec/01-source-language.md) "External definitions".
- Open questions: Q12 (`From`, `TryFrom` in the prelude) decides whether
  `@std.core` is imported or ambient in the examples of later steps; Q15
  (the `native` list may be empty). Q12 must be answered before step 19.

## Entry points

| Existing | Use |
| --- | --- |
| `stdlib-v1/manifest.vibon`, `stdlib-v1/src/std/{core,bool,option,result,iter,builtin}.vib` | the declarations to re-express; nothing is copied textually |
| `crates/vibra-types-v1/src/stdlib.rs:49` (`LANGUAGE_ROLES`), `:64` (embedding), `:336` (`load_stdlib`) | the loader design: embedded bytes, digests, roles once, externals against the registry |
| `crates/vibra-ir/src/external/compiler.rs` (step 12) | the registry the loader checks `external @compiler` declarations against |

Proposed: `stdlib/manifest.vibon`,
`stdlib/src/std/{core,bool,option,result,iter,builtin}.vib`;
`crates/vibra-types/src/stdlib.rs` (the crate's first module; the checker
arrives in 15).

## Ordered tasks

1. `@std.core`: `Ordering` (cases `less`, `equal`, `greater`),
   `ArithmeticError`, `ConversionError`, `Equatable` (role `@equatable`),
   `Ordered` (role `@ordered`) with the default members `less`,
   `lessOrEqual`, `greater`, `greaterOrEqual`, `min`, `max`, `inRange`
   written with one `compare` call each and left-biased ties, `From(Source)`,
   `TryFrom(Source)` with `type Error endtype`. Every declaration
   `visibility @public`.
2. `@std.bool`: `type Bool case false case true role @bool visibility @public …`
   with `not`, `and`, `or`, `xor` members (eager, by `match`), the module
   values `true` and `false`, and `impl Equatable`, `impl Ordered` (`false`
   before `true`).
3. `@std.option`: `Option(T)` with `case some {value: T}`, `case none`,
   role `@option`; `@std.result`: `Result(T E)` with `case ok {value: T}`,
   `case err {error: E}`, role `@result`.
4. `@std.iter`: `Iter` with role `@iter`, `type Item endtype`, abstract
   `next`, default `map(Out)`, `filter`, `skip`, `take`, `collect`,
   `fold(Acc)`, and the four adapter types `MappedIter(Item Out)`,
   `FilteredIter(Item)`, `SkippedIter(Item)`, `TakenIter(Item)` with nested
   `impl Iter`. (Checked in step 20; parsed and loaded now.)
5. `@std.builtin`: the `external @compiler` declarations of the intrinsic
   registry as static members of the compiler-owned types declared `of
   Intrinsic(@…)`: `I8` … `U64` (`add`, `sub`, `mul`, `neg` where signed,
   `shiftLeft`, `shiftRight`, the checked forms), `F32`, `F64` (`add`,
   `sub`, `mul`, `div`, `neg`, `equal`, `compareTotal`), `Char` (`toU32`,
   `fromU32`), `List(T)` (`length`, `append`, `concat`, `slice`). `Dict`
   arrives in 16, `Str` and `Bytes` in 20. Never imported.
6. `manifest.vibon`: `@stdlibManifest.v2` with package name and version,
   every module's path, digest, and role (`@source`, `@testRegistry` for
   `@std.assert` later), `compiler` symbols, `host: []`, `native: []`,
   `assertions: []`. Canonical VIBON.
7. Loader: decode the manifest through its closed record, hash each
   embedded module, parse each with the reader in standard-library mode,
   check every `external @compiler` symbol against the registry's signature
   (`@external.unknownSymbol`, `@external.signatureMismatch` as operational
   diagnostics that fail the load), check each role is claimed exactly once,
   build the resolver overlay (`@std` unit). Invariant: no I/O; a mismatch
   is a toolchain defect, never a fallback.
8. Architecture row:
   `vibra-types: [vibra-diagnostics, vibra-ir, vibra-resolve, vibra-syntax]`
   (step 15 adds `vibra-interp`).

## Test matrix

Host tests.

| Test | Input | Expected |
| --- | --- | --- |
| every module parses | the six modules in standard-library mode | no diagnostic from the reader |
| every module formats canonically | the six modules | `format(x) == x` |
| manifest round trip | `manifest.vibon` | decodes; re-encodes byte-identically |
| digests | each module | the manifest digest equals the embedded bytes' SHA-256 |
| roles once | the loaded modules | nine roles, each claimed by exactly one declaration (`@dict`, `@str`, `@bytes` claimed by placeholders until 16 and 20, see the note) |
| external symbols | every `external @compiler` declaration | its symbol is a registry row and the written signature equals the row's |
| unknown symbol | a fixture manifest naming `i32.add-rounded` | the load fails with `@external.unknownSymbol` |
| signature mismatch | a fixture declaring `addChecked` returning `I32` | `@external.signatureMismatch` |
| role missing | a fixture without `role @bool` | the load fails naming the role |
| prelude table | the loaded overlay | the prelude names of the type chapter all resolve to a declaration of `@std` |

Note on roles: `Str`, `Bytes`, and `Dict` are declared in this step as
their wrapper types with their roles and no members, so that the roles are
claimed once and the checker can type string literals and dict literals in
15 and 16; their members arrive in 16 and 20.

## Diagnostics and schema changes

Codes: `@external.unknownSymbol`, `@external.signatureMismatch` (operational,
on the toolchain's own library). Schema: `@stdlibManifest.v2` (closed
record, as the programs chapter lists).

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: the loader test prints the module count (six) and the role count
(nine); the runner is unchanged by this step.

## Excluded

Checking the stdlib (15 onward); `Str`, `Bytes`, `Dict` members (16, 20);
`@std.assert` (23); every M2 module.

## Completion evidence

- [ ] `stdlib/manifest.vibon` lists six modules, an empty `host`, an empty
      `native`, an empty `assertions`.
- [ ] Every `external @compiler` declaration in `stdlib/` has a registry row
      (the loader test).
- [ ] `stdlib-v1/` is untouched (`git diff --stat c6521c76 -- stdlib-v1` is
      empty apart from step 01's move).
- [ ] Validation commands exit 0.
