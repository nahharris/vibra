# Step 01: rename-v1

Kind: infrastructure. It carries no language behavior; it makes room for the
new crates under their final names
([architecture](../architecture.md#the-decision), decision 1).

## Prerequisites

- M0 has exited; the integration branch `v0.0.2/m1` exists.
- Read: [AGENTS.md](../../../../../AGENTS.md); the
  [execution model](../../../execution.md); architecture sections 1 and 2.
- No specification section defines this step.

## Entry points

| Existing | Use |
| --- | --- |
| `Cargo.toml:1` (workspace `members`, `[workspace.dependencies]`) | every `vibra-*` path dependency is renamed here |
| `crates/vibra-cli/Cargo.toml` (`[[bin]] name = "vibra"`) | becomes `vibra-v1` |
| `crates/vibra-types/src/stdlib.rs:64` (`include_bytes!("../../../stdlib/manifest.vibon")`) and the module list at `:67` | paths move to `stdlib-v1/` |
| `crates/vibra-conformance/src/corpus.rs:21` (`corpus_root` = `conformance/cases`) | becomes `conformance/v1/cases` |
| `crates/vibra-conformance/tests/it/native_parity_inventory.rs:22` (`conformance/native-parity.tsv`) | becomes `conformance/v1/native-parity.tsv` |
| `crates/vibra-conformance/tests/it/architecture_boundary.rs:22` (`ARCHITECTURE` table) | every row renamed |
| `crates/vibra-conformance/tests/it/evidence.rs:11` (`docs/roadmap/v0.0.1/m1/syntax-examples.tsv`) | unchanged; the test stays ignored |
| `crates/vibra-conformance/src/bin/m1-fuzz.rs:183` (`--corpus-root`, default `fuzz/corpus`) | default becomes `fuzz-v1/corpus` |
| `crates/vibra-cli/tests/it/examples.rs:3` (every project under `examples/`) | reads `examples-v1/` |
| `.github/workflows/ci.yml:118` and `:122` | the two `cargo run` lines name the `-v1` package and roots |

Proposed: no new module. The renamed crates are
`vibra-diagnostics-v1`, `vibra-syntax-v1`, `vibra-fmt-v1`, `vibra-resolve-v1`,
`vibra-types-v1`, `vibra-ir-v1`, `vibra-interp-v1`, `vibra-workspace-v1`,
`vibra-schema-v1`, `vibra-cli-v1`, `vibra-conformance-v1`. The kept crates
(`vibra-host`, `vibra-native`, `vibra-wasm`, `vibra-wasm-run`, and the native
track) keep their names and now depend on `vibra-ir-v1`, `vibra-syntax-v1`,
`vibra-diagnostics-v1`.

## Ordered tasks

1. Move the eleven crate directories with
   `git mv crates/<name> crates/<name>-v1` and set each `[package] name`.
   Invariant: every file keeps its history.
2. Rewrite `[workspace.dependencies]` and every `<name>.workspace = true` to
   the `-v1` names; rewrite every `use vibra_<name>::` to `vibra_<name>_v1::`
   with one search-and-replace over `crates/`. Invariant: `cargo metadata`
   lists the same number of packages as before, with the new names.
3. Rename the binary `vibra` to `vibra-v1` and update
   `env!("CARGO_BIN_EXE_vibra")` in the CLI tests. Invariant: no two packages
   emit a binary with one name.
4. `git mv stdlib stdlib-v1`, `git mv examples examples-v1`,
   `git mv fuzz fuzz-v1`, `git mv conformance/cases conformance/v1/cases`,
   `git mv conformance/native-parity.tsv conformance/v1/native-parity.tsv`;
   fix every path constant named above. Invariant: the v1 stdlib digests
   still verify (`load_stdlib` passes), because bytes did not change.
5. Update the architecture table and CI. Invariant: the architecture test
   enumerates every crate directory and fails on an unlisted one.
6. Update `docs/roadmap/README.md` and the repository `README.md` with one
   sentence each: the v1 toolchain is the `vibra-v1` binary during M1.

## Test matrix

No language behavior. The matrix is the existing suites, run unchanged:

| Check | Expected |
| --- | --- |
| `cargo test --workspace` | the same pass count as before the step, with the ignored tests still ignored |
| v1 corpus runner on `conformance/v1/cases` | the same counts as before: 604 cases, none failed |
| `vibra-v1 help` | exit 0; the usage text is unchanged |
| `cargo metadata --no-deps` | 23 packages; every language crate name ends in `-v1` |

## Diagnostics and schema changes

None. No code is added or removed; the v1 registry keeps its kebab-case
codes until step 28.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline --workspace --all-targets --all-features
cargo run --locked --offline -p vibra-conformance-v1 --bin vibra-conformance -- --root conformance/v1/cases
cargo run --locked --offline -p vibra-conformance-v1 --bin m1-fuzz -- --profile ci-smoke --corpus-root fuzz-v1/corpus
cargo metadata --locked --offline --no-deps --format-version 1 | ConvertFrom-Json | Select-Object -ExpandProperty packages | ForEach-Object name
```

Read: every command exits 0; the runner prints `604` cases with `0 failed`
across its profiles and no `UNAVAILABLE` line that was not there before; the
package list shows eleven `-v1` names and no bare `vibra-syntax`.

## Excluded

Deleting anything; changing behavior; touching `archive/`.

## Completion evidence

- [ ] `git log --follow crates/vibra-syntax-v1/src/reader.rs` shows the
      pre-rename history.
- [ ] The validation commands above exit 0, output pasted in the PR.
- [ ] `Get-ChildItem crates -Directory` lists no directory without a row in the
      architecture table.
- [ ] CI is green on the PR.
