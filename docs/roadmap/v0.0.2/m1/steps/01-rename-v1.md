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

The workspace has 24 crates and `members = ["crates/*"]`, so a moved
directory needs no edit of `members`.

| Existing | Use |
| --- | --- |
| `Cargo.toml` (`[workspace.dependencies]`) | thirteen keys and paths are renamed |
| `crates/vibra-cli/Cargo.toml` (`[[bin]] name = "vibra"`) | the binary becomes `vibra-v1` |
| `crates/vibra-conformance/Cargo.toml` (`[[bin]] name = "vibra-conformance"`) | the binary becomes `vibra-conformance-v1` |
| `crates/vibra-types/src/stdlib.rs:64` to `:94` (`include_bytes!("../../../stdlib/…")`) | paths read `stdlib-v1/` |
| `crates/vibra-resolve/tests/it/resolution.rs` (seven `include_bytes!`/`include_str!` of `stdlib/src/std/…`) | paths read `stdlib-v1/` |
| `crates/vibra-conformance/tests/it/primitives.rs:374`, `host_documents_fmt.rs:64` and `:81`, `native_differential.rs:103`, `:259`, `:314` | paths read `stdlib-v1/` |
| `crates/vibra-conformance/src/corpus.rs:22` (`conformance/cases`) | becomes `conformance/v1/cases` |
| `crates/vibra-conformance/src/baseline.rs:116` and `:155`, `tests/it/corpus.rs:43`, `lowering_slice.rs:867`, `m1_evidence.rs:153` and `:170`; `crates/vibra-schema/tests/it/index.rs:19`, `semantic_query.rs:325` and `:458` | the corpus path gains `v1/` |
| `crates/vibra-conformance/tests/it/native_parity_inventory.rs:24` (`conformance/native-parity.tsv`) | becomes `conformance/v1/native-parity.tsv` |
| `crates/vibra-conformance/src/baseline.rs:101` to `:147`, `tests/it/examples_backends.rs:43`, `m1_evidence.rs:35`, `performance_baseline.rs:216` and `:300`; `crates/vibra-cli/tests/it/examples.rs:30` | `examples` becomes `examples-v1` |
| `crates/vibra-conformance/src/bin/m1-fuzz.rs:628` (`fuzz/corpus`) | becomes `fuzz-v1/corpus` |
| `crates/vibra-cli/tests/it/support.rs:119`, `review_regressions.rs:73` and `:77` (`CARGO_BIN_EXE_vibra`) | `CARGO_BIN_EXE_vibra-v1` |
| `crates/vibra-conformance/tests/it/differential.rs:507`, `native_parity_inventory.rs:485`, `reader.rs:39` and `:51` (`CARGO_BIN_EXE_vibra-conformance`) | `CARGO_BIN_EXE_vibra-conformance-v1` |
| `crates/vibra-conformance/tests/it/architecture_boundary.rs:22` (`ARCHITECTURE`) | every renamed crate's row and every mention of it in another row is renamed |
| `.github/workflows/ci.yml:118` and `:122` | the two `cargo run` lines name the `-v1` package, binary, and roots |

The string `"stdlib/src/std/…"` also appears as a logical source identity
(`crates/vibra-types/src/standard.rs:364`, `nominal.rs:2194`,
`crates/vibra-resolve/src/lib.rs:1334`, and tests). A source identity is
data that the v1 expectations contain. It does not change.

## The renames

Thirteen crates gain the suffix `-v1` (directory, `[package] name`,
workspace dependency key and path, and every `vibra_<name>::` path):

| Crate | Becomes |
| --- | --- |
| `vibra-diagnostics` | `vibra-diagnostics-v1` |
| `vibra-syntax` | `vibra-syntax-v1` |
| `vibra-fmt` | `vibra-fmt-v1` |
| `vibra-resolve` | `vibra-resolve-v1` |
| `vibra-types` | `vibra-types-v1` |
| `vibra-ir` | `vibra-ir-v1` |
| `vibra-interp` | `vibra-interp-v1` |
| `vibra-host` | `vibra-host-v1` |
| `vibra-native` | `vibra-native-v1` |
| `vibra-workspace` | `vibra-workspace-v1` |
| `vibra-schema` | `vibra-schema-v1` |
| `vibra-cli` | `vibra-cli-v1` |
| `vibra-conformance` | `vibra-conformance-v1` |

Nine crates keep their names and their directories: `vibra-actors`,
`vibra-fs-scope`, `vibra-exec`, `vibra-lower`, `vibra-nexus`,
`vibra-nexus-eval`, `vibra-nexus-bridge`, `vibra-codegen`,
`vibra-build`. Those of them that depend on a renamed
crate now depend on its `-v1` name. `vibra-fs-scope` and `vibra-actors`
depend on no workspace crate and are not edited.

Five trees move:

| From | To |
| --- | --- |
| `stdlib/` | `stdlib-v1/` |
| `examples/` | `examples-v1/` |
| `fuzz/` | `fuzz-v1/` |
| `conformance/cases/` | `conformance/v1/cases/` |
| `conformance/native-parity.tsv` | `conformance/v1/native-parity.tsv` |

No file is created and no file is deleted.

## Ordered tasks

1. Move the thirteen crate directories with
   `git mv crates/<name> crates/<name>-v1` and set each `[package] name`.
   Invariant: every file keeps its history.
2. In the root `Cargo.toml`, rename the thirteen keys of
   `[workspace.dependencies]` and their paths. In every crate manifest,
   rename each `<name>.workspace = true` and each `<name> = { … }` entry of
   a renamed crate. Invariant: `cargo metadata` lists 22 packages.
3. Rewrite every Rust path of a renamed crate with one search-and-replace
   over `crates/`: `vibra_<name>::` to `vibra_<name>_v1::`, and
   `extern crate`, `use vibra_<name>;`, and `vibra_<name> as` forms the
   same way. Match whole identifiers, so that `vibra_native` does not
   match inside `vibra_native_v1`.
4. Rename the binary `vibra` to `vibra-v1` and the binary
   `vibra-conformance` to `vibra-conformance-v1`, and update the seven
   `env!("CARGO_BIN_EXE_…")` sites of the table. Invariant: no two
   packages emit a binary with one name, now or after step 02 and step 24
   add `vibra-conformance` and `vibra`.
5. Move the five trees with `git mv` and fix every path constant of the
   table. Invariant: the v1 stdlib digests still verify (`load_stdlib`
   passes), because no byte changed.
6. Rename the rows of the `ARCHITECTURE` table. Invariant: the test
   enumerates every directory under `crates/` and fails on an unlisted
   one.
7. Edit the two CI lines to `-p vibra-conformance-v1 --bin vibra-conformance-v1
   -- --root conformance/v1/cases` and
   `-p vibra-conformance-v1 --bin m1-fuzz -- --profile ci-smoke`.
8. Add one sentence to `docs/roadmap/README.md` and one to the repository
   `README.md`: the v0.0.1 toolchain is the `vibra-v1` binary while M1 is
   in progress.

## Ignored tests

Three host tests are `#[ignore]`d when this step starts. This step leaves
all three ignored and only moves them with their crate:

| Test (in `crates/vibra-conformance-v1/tests/it/`) | Restored or deleted by |
| --- | --- |
| `diagnostic_registry::the_registry_and_the_specification_table_have_the_same_codes` | [step 02](02-diagnostics-and-harness.md) restores it in the new `vibra-conformance` and deletes this copy |
| `evidence::spec_examples_are_inventoried_and_losslessly_exercised` | [step 08](08-reader-exit.md) deletes it and adds its replacement |
| `native_parity_inventory::a_pending_cell_names_a_package_of_the_plan` | [step 28](28-cutover.md) deletes it with the crate; package X2-04 owns the new inventory |

## Test matrix

No language behavior. The matrix is the existing suites, run unchanged:

| Check | Expected |
| --- | --- |
| `cargo test --workspace` | the pass count of the commit before the step; the three tests above still ignored |
| v1 corpus runner on `conformance/v1/cases` | 597 cases, none failed, the same counts per profile as before |
| `vibra-v1 help` | exit 0; the usage text is unchanged |
| `cargo metadata --no-deps` | 22 packages; thirteen names end in `-v1` |
| leftover paths | the search of the validation block prints nothing |

## Diagnostics and schema changes

None. No code is added or removed; the v1 registry keeps its kebab-case
codes until step 28.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline --workspace --all-targets --all-features
cargo run --locked --offline -p vibra-conformance-v1 --bin vibra-conformance-v1 -- --root conformance/v1/cases
cargo run --locked --offline -p vibra-conformance-v1 --bin m1-fuzz -- --profile ci-smoke
cargo run --locked --offline -p vibra-cli-v1 --bin vibra-v1 -- help
(cargo metadata --locked --offline --no-deps --format-version 1 | ConvertFrom-Json).packages.name | Sort-Object
Get-ChildItem crates -Recurse -Include *.rs,*.toml | Select-String -Pattern '\.\./stdlib/|join\("stdlib|join\("examples|"\.\./\.\./examples"|join\("conformance/cases|"fuzz/corpus"'
```

Read: every command exits 0. `Cargo.lock` changes only in the thirteen
package names. The runner prints 597 cases with `0 failed` and no
`UNAVAILABLE` line that the commit before the step did not print. The
package list shows thirteen `-v1` names and no bare `vibra-syntax`,
`vibra-ir`, `vibra-host`, or `vibra-native`. The last command prints
nothing.

`--locked` holds because the step edits `Cargo.lock` by hand in the same
commit: each renamed `name = "vibra-…"` entry and each mention of it in a
`dependencies` list gains `-v1`, and the entries are re-sorted by name. Run
`cargo metadata --locked --offline` first; it fails until the lock file
matches.

## Excluded

Deleting anything; changing behavior; touching `archive/`; renaming a
logical source identity.

## Completion evidence

- [ ] `git log --follow crates/vibra-syntax-v1/src/reader.rs` shows the
      pre-rename history.
- [ ] The validation commands above exit 0, output pasted in the PR.
- [ ] `Get-ChildItem crates -Directory` lists no directory without a row in
      the architecture table.
- [ ] `git status --short` after the build shows no untracked file outside
      `target/`.
- [ ] CI is green on the PR.
