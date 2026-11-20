# Step 18: stdlib-policy-types

Kind: vertical. Slice 2, the two leaf modules: `@std.policy` (`ScopeError`,
`GrantLimits`) and `@std.process` (`ExitCode`). They are plain data and
need nothing of the effect system.
Depends on: 01.
Crates: `stdlib`, `conformance/cases`.

## Prerequisites

- Read: [programs
  chapter](../../../../spec/04-programs-and-packages.md#the-standard-library)
  "The standard library"; [effects
  chapter](../../../../spec/03-effects.md#narrowing) the paragraph on
  `ScopeError` and `GrantLimits`; [runtime
  chapter](../../../../spec/06-runtime.md#entry-outcome) the first
  paragraph; [questions](../spec-questions.md) Q26.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-types/src/stdlib/manifest.rs:19` | `pub fn manifest_schema` | the manifest the two modules join |
| `crates/vibra-types/src/stdlib.rs:15` | `mod external;` | the loader that checks every module against its digest |

Existing library files: `stdlib/manifest.vibon`, `stdlib/src/std/`.

Proposed: `stdlib/src/std/policy.vib`, `stdlib/src/std/process.vib`.

## Files owned

- `stdlib/manifest.vibon`
- `stdlib/src/std/policy.vib`
- `stdlib/src/std/process.vib`
- `conformance/rules/m2-18.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `@std.policy`: `ScopeError` with the fieldless cases `invalidPolicy`,
   `unsupportedVersion`, `policyTooComplex`, `foreignRoot`; `GrantLimits`,
   a record of `invocations`, `inputBytes`, `outputBytes`, each an
   `Option(U64)`. The module imports only `@std.option` (Q26). Invariant:
   every declaration is `visibility @public`.
2. `@std.process`: `ExitCode`, a wrapper over `U8` with a public
   representation.
3. Add both modules to the manifest with their digests, sorted by path.
4. The implementations the other library types have by convention
   (`Equatable` where the representation allows it) follow the existing
   modules; none is invented beyond them.

## Test matrix

`workspaceCheck`, profile `staticV2`; one module `src/app/main.vib`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TYPE-INTERFACE-std-policy-types` | positive | `use @std.policy enduse` and `fn f returns e: policy.ScopeError do return policy.ScopeError.foreignRoot() endfn` | accepted | 03 Narrowing: "`ScopeError` (`@std.policy`) has the cases…" |
| `V2-TYPE-INTERFACE-scope-error-cases` | positive | a `match` over the four cases with no default arm | accepted: exhaustive | the same |
| `V2-TYPE-INTERFACE-scope-error-unknown-case` | negative | `policy.ScopeError.denied()` | `@name.unknownSymbol` at `denied` | the same: the case list is closed |
| `V2-TYPE-INTERFACE-grant-limits` | positive | `policy.GrantLimits(invocations: Option.some(value: 3u64) inputBytes: Option.none() outputBytes: Option.none())` | accepted | 03: "each an `Option(U64)`" |
| `V2-TYPE-INTERFACE-grant-limits-wrong-width` | negative | `invocations: Option.some(value: 3i32)` | `@type.argumentMismatch` at the argument | the same |
| `V2-TYPE-INTERFACE-grant-limits-missing-field` | negative | `policy.GrantLimits(invocations: Option.none())` | `@type.argumentMismatch` | the same: a record of three fields |
| `V2-TYPE-INTERFACE-exit-code` | positive | `use @std.process.ExitCode enduse` and `fn ok returns c: ExitCode do return ExitCode(0u8) endfn` | accepted | 06 Entry outcome: "`ExitCode`, a wrapper over `U8`" |
| `V2-TYPE-INTERFACE-exit-code-width` | negative | `ExitCode(256i32)` | `@type.argumentMismatch` at the argument | the same |

## Diagnostics and schema changes

Codes: `@name.unknownSymbol`, `@type.argumentMismatch`. The manifest gains
two module entries.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-workspace -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0. A digest that does not match its module fails the
library loader's own test with `@runtime.invalidCheckedProgram`.

## Stays unavailable

The operation modules (19) and `@std.test` (21).

## Excluded

`FsPolicy` and its family, which Q26 places in `@std.fs` (19).

## Completion evidence

- [ ] `conformance/rules/m2-18.tsv` has one `active` row per case id.
- [ ] `stdlib/manifest.vibon` lists the two modules with digests the
      report recomputes with `Get-FileHash`.
- [ ] The validation commands exit 0.
