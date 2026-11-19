# Step 27: runtime-crate-port

Kind: infrastructure. The runtime crates below the interpreter arrive on
the new `vibra-ir`: `vibra-native` and `vibra-host` are created under their
final names from their `-v1` sources, and `vibra-ir` gains the runtime
modules they read. After this step the only crates that depend on a `-v1`
crate are the `-v1` crates themselves and the native track, which package
X2-01 moves.

This step is the runtime part of the package N1 of the
[native contract](../../m0/native-contract.md#7-suggested-package-cut), as
the [architecture](../architecture.md#native-contract-packages) maps it. It
edits no `-v1` crate.

## Prerequisites

- Steps 03 and 13 landed (the data reader and the interpreter).
- Read: [runtime chapter](../../../../spec/06-runtime.md) "Traps",
  "Compiler intrinsic registry", "Native implementations", "Canonical
  value encoding";
  [native contract](../../m0/native-contract.md) sections 3 and 6;
  [architecture](../architecture.md) decisions 5 and 6;
  `crates/vibra-ir/migration.md` (step 12).

## Files

| Path | Action |
| --- | --- |
| `crates/vibra-ir/src/arena.rs`, `src/external/host.rs`, `src/external/native.rs`, `src/external/vectors.rs`, `src/external/vectors/numbers.rs` | created as copies of the files of the same names in `crates/vibra-ir-v1/src/`, edited only where a type they name changed in step 12 (`Array` to `List`, the trap type) |
| `crates/vibra-ir/src/lib.rs`, `src/external.rs` | the new modules declared |
| `crates/vibra-native/` | created: `Cargo.toml`, `src/{lib,arena,number,utf8}.rs`, and `tests/`, copied from `crates/vibra-native-v1/` and compiled against `vibra-ir` |
| `crates/vibra-host/` | created: `Cargo.toml`, `src/{lib,event,operation,provider,random,system,world}.rs`, and `tests/`, copied from `crates/vibra-host-v1/` and compiled against `vibra-ir`, `vibra-syntax`, and `vibra-diagnostics` |
| `crates/vibra-conformance/tests/it/architecture_boundary.rs` | rows for `vibra-native` and `vibra-host` added |
| `Cargo.toml` (`[workspace] members`, `[workspace.dependencies]`) | `vibra-native` and `vibra-host` added |

## Ordered tasks

1. `vibra-ir` runtime modules: copy `arena` and the two external
   registries from `vibra-ir-v1`. `external::host` keeps the major-1 host
   rows unchanged; package N7 replaces them in M2. Invariant: the sample
   vectors of `external::vectors` pass unchanged.
2. `vibra-native`: copy the crate and fix the registry row types. No
   native routine changes behaviour; every test of the `-v1` crate passes
   in the new one.
3. `vibra-host`: copy the crate. It decodes the world and trace documents
   with `vibra_syntax::data` (step 03) and names them `@hostWorld.v1` and
   `@auditTrace.v1` in the crate and its fixtures, which are respelled in
   the brace grammar. Operations, providers, and field shapes do not
   change. Nothing in M1 calls the crate; it is the base that the packages
   N7 to N12 change in M2 and that X2-01 re-points the native track to.

## Test matrix

Host tests in the three crates.

| Test or case | Kind | Input | Expected |
| --- | --- | --- | --- |
| `host_world_decodes_data_grammar` | positive | a `@hostWorld.v1` document in the data grammar | the same `HostWorld` the fixture describes |
| `host_world_rejects_other_format` | negative | the document with the format atom `@hostWorld.v2` | `@data.invalidValue` |
| `audit_trace_round_trip` | positive | a trace with three events | canonical text decodes to equal events |
| `native_vectors` | positive | the shared sample vectors | all pass |

Formatter: not applicable.

## Diagnostics and schema changes

Codes: `@data.invalidValue`. No machine schema changes.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all -- --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-ir -p vibra-native -p vibra-host -p vibra-conformance
cargo run --locked --offline -p vibra-conformance-v1 --bin vibra-conformance-v1 -- --root conformance/v1/cases
cargo tree --locked --offline -p vibra-native -e normal
cargo tree --locked --offline -p vibra-host -e normal
cargo test --locked --offline --workspace --all-targets --all-features
```

`Cargo.lock` gains the two new package blocks; they are written by hand,
as in step 01, because the build is offline and locked. Read: the v1 run
passes; neither `cargo tree` output contains a `-v1` package.

## Excluded

The registry-major-2 host rows, the broker, grants, policies, the host
world and audit trace of major 2, startup, and `TestWorld` (M2, packages
N7 to N12); the native track (`vibra-exec`, `vibra-lower`,
`vibra-codegen`, `vibra-build`, `vibra-nexus`, `vibra-nexus-eval`,
`vibra-nexus-bridge`, `vibra-actors`), which X2-01 moves after this step;
`vibra-fs-scope`, which no crate depends on until package N9.

## Completion evidence

- [ ] `cargo tree -p vibra-host` and `-p vibra-native` list no `-v1`
      package.
- [ ] Validation commands exit 0.
