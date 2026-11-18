# Step 02: diagnostics-and-harness

Kind: infrastructure and specification prerequisite. The registry is
compiler data copied from the specification table; the harness is the
corpus runner. Neither is language behavior.

## Prerequisites

- Step 01 landed.
- Read: [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md): "Diagnostics are
  a language surface", "Diagnostic documents and spans", "Conformance corpus",
  "Case operations and inputs", "Rule identities and example coverage",
  "Conformance profiles"; [tooling chapter](../../../../spec/05-tooling.md)
  "Versioned envelope and results" for the status words; architecture
  section 3.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-diagnostics-v1/src/span.rs:23` (`ByteSpan`), `line_index.rs`, `diagnostic.rs:147` (`Diagnostic`) | copied unchanged into the new crate |
| `crates/vibra-diagnostics-v1/src/registry.rs:173` (the `ALL` table macro) | the shape to keep; the rows are replaced |
| `crates/vibra-conformance-v1/src/{corpus.rs,manifest.rs,runner.rs,profile.rs}` and `src/bin/conformance.rs:38` | copied into the new crate and edited as below |
| `crates/vibra-conformance-v1/tests/it/architecture_boundary.rs:22` and `tests/it/support.rs` (`rust_sources`, `workspace_root`) | the test moves to the new crate; the two helpers are copied |
| `crates/vibra-conformance-v1/tests/it/diagnostic_registry.rs:24` (`specification_table`), `:65` (the ignored membership test) | the table parser and the test move to the new crate |

## Files

Created:

| Path | Contents |
| --- | --- |
| `crates/vibra-diagnostics/Cargo.toml` | package `vibra-diagnostics`, no dependency, workspace lints |
| `crates/vibra-diagnostics/src/lib.rs` | re-exports of the four modules |
| `crates/vibra-diagnostics/src/span.rs`, `line_index.rs`, `diagnostic.rs` | copies of the v1 files |
| `crates/vibra-diagnostics/src/registry.rs` | the 115 codes of the specification table |
| `crates/vibra-conformance/Cargo.toml` | package `vibra-conformance`, binary `vibra-conformance` at `src/bin/conformance.rs`; dependencies `vibra-diagnostics`, `toml`, `serde` |
| `crates/vibra-conformance/src/lib.rs` | module list |
| `crates/vibra-conformance/src/corpus.rs` | discovery under `conformance/cases` |
| `crates/vibra-conformance/src/manifest.rs` | `case.toml` decoding |
| `crates/vibra-conformance/src/rules.rs` | `conformance/rules.tsv` decoding |
| `crates/vibra-conformance/src/profile.rs` | the profile and operation enums, `ProfileHandler`, `CaseObservation` |
| `crates/vibra-conformance/src/standard.rs` | the dispatcher, with no handler registered |
| `crates/vibra-conformance/src/runner.rs` | comparison and the per-profile report |
| `crates/vibra-conformance/src/bin/conformance.rs` | the binary: `--root <dir>` only |
| `crates/vibra-conformance/tests/it/main.rs`, `support.rs` | the test crate root and the two helpers |
| `crates/vibra-conformance/tests/it/architecture_boundary.rs` | the workspace architecture table |
| `crates/vibra-conformance/tests/it/diagnostic_registry.rs` | membership, level, domain, and capability tests |
| `crates/vibra-conformance/tests/it/manifest.rs`, `runner.rs` | the manifest and runner tests of the matrix |
| `conformance/rules.tsv` | the header line `identity<TAB>status<TAB>anchor` and no row |

Edited: the root `Cargo.toml` gains `vibra-diagnostics` and
`vibra-conformance` under `[workspace.dependencies]`; `Cargo.lock` gains
the two packages.

Deleted: `crates/vibra-conformance-v1/tests/it/architecture_boundary.rs`
and its `mod` line in `tests/it/main.rs`; the function
`the_registry_and_the_specification_table_have_the_same_codes` and its
helper `specification_table` in
`crates/vibra-conformance-v1/tests/it/diagnostic_registry.rs`, which
compared the v1 registry with a chapter that no longer describes it.

## Ordered tasks

1. Create `vibra-diagnostics` with the copied span, line index, and
   diagnostic types. Invariant: `Diagnostic` derives its level from its
   code and stores no level.
2. Write the registry: one variant per row of the table under "Diagnostics
   are a language surface", atom spelled exactly as the table, level from
   the table, and a domain enum with the fourteen first components
   (`syntax`, `name`, `module`, `type`, `pattern`, `effect`, `external`,
   `data`, `project`, `runtime`, `style`, `contract`, `grant`, `tool`).
   Every code has fix capability `@none`. Invariant: no code is spelled
   with a hyphen.
3. Restore the membership test in the new crate, not ignored: it parses
   the chapter table with the copied parser and compares the set of
   `(code, level)` pairs with the registry. Add the level, domain, and
   count tests of the matrix. Invariant: the test fails on an extra code,
   a missing code, or a different level.
4. Create `vibra-conformance`. `manifest` accepts:
   - `profile`: `readerV2`, `staticV2`, `interpreterV2`, `toolingV2`,
     `nativeV1`, `wasmTransitionV2`, `fullV2`;
   - `operation`: the ten of the chapter's table, `reader`,
     `projectDecode`, `sourceGraph`, `resolve`, `workspaceCheck`,
     `format`, `query`, `interpret`, `workspaceRun`, `workspaceTest`;
   - `id` and `rule`: a rule identity is `V1-` or `V2-`, one of the twenty
     prefixes of the chapter, `-`, and a suffix of lower-case letters,
     digits, and hyphens; `rule` must be a row of `conformance/rules.tsv`
     with status `active`; `id` is `rule` or `rule` followed by `-` and a
     further suffix;
   - `[inputs]`, `[expect]` with `verdict` (`accepted`, `refuted`,
     `unknown`, `unsupported`), `[[expect.diagnostics]]` rows (`code`,
     `level`, `source`, `span`, `related`, `notes`), and the snapshot keys
     `tree`, `formatted`, `graph`, `resolved`, `types`, `result`,
     `testRun`.
   The new crate has no index, native, or Wasm code and no
   option that writes an expectation. Invariant: an unknown key anywhere
   in `case.toml` is a corpus error.
5. `corpus_root` is `conformance/cases`, discovered two levels deep
   (`<PREFIX>/<id>/case.toml`); a case directory whose prefix directory
   does not match its identity is a corpus error.
6. The runner reports, per profile, the counts of passed, failed,
   unknown, and unavailable cases. A case whose operation has no
   registered handler is unavailable. The binary exits 0 only when every
   case passed. Invariant: there is no "skipped" count.
7. Move the architecture test into the new crate. Its table has the 24
   rows of step 01 unchanged and two new rows: `vibra-diagnostics: []`
   and `vibra-conformance: [vibra-diagnostics]`. Invariant: every
   directory under `crates/` has exactly one row, and a later step that
   adds a crate or a dependency edits this one table.

## Ignored tests

`diagnostic_registry::the_registry_and_the_specification_table_have_the_same_codes`
is restored by this step, in `crates/vibra-conformance/tests/it/`, and its
ignored v1 copy is deleted. The other two ignored tests stay ignored in
`vibra-conformance-v1`: [step 08](08-reader-exit.md) deletes the example
inventory test, and [step 28](28-cutover.md) deletes the native parity inventory
test.

## Test matrix

Host tests only; there is no case yet.

| Test | Input | Expected |
| --- | --- | --- |
| registry membership | the chapter table | the implemented set equals the specified set; the test is not ignored |
| registry count | the registry | 115 codes |
| registry levels | each code | `@type.suspiciousRow`, `@style.casing`, `@contract.unusedEffect` are `@warning`; every other code `@error` |
| registry domain | each code | the domain's spelling is the code's first component; the counts are syntax 11, name 10, module 11, type 29, pattern 5, effect 4, external 3, data 5, project 11, runtime 3, style 1, contract 7, grant 6, tool 9 |
| manifest: profile spellings | `profile = "readerV2"` … `"fullV2"` | accepted; `"reader-v1"` rejected as an unknown profile |
| manifest: operation spellings | each of the ten | accepted; `"workspace-check"` rejected |
| manifest: rule status | a case whose rule is `retired` or absent from the index | the corpus load fails naming the case |
| manifest: identity shape | `V2-SRC-READER-x`, `V1-PROJECT-x` | accepted; `V3-SRC-READER-x`, `V2-SRC-x` (unknown prefix), `V2-SRC-READER-X` (upper case suffix) rejected |
| manifest: unknown key | `[expect] wasm = true` | a corpus error naming the key |
| corpus layout | a case under the wrong prefix directory | a corpus error naming both |
| runner: no write mode | the binary's options | `--root` only; `--write-native-parity` is an unknown option, exit 1 |
| runner: unavailable | one valid `reader` case, no handler | exit 1; `readerV2 conformance: 0 passed, 0 failed, 0 unknown, 1 unavailable` |
| empty corpus | `--root` of an empty directory | exit 1, `corpus is empty` |
| architecture | the workspace manifests | 26 rows; every crate has one row; each row equals the crate's `Cargo.toml` |

## Diagnostics and schema changes

The new registry is exactly the specification table (115 rows). No schema:
`DiagnosticDocument` version 2 is rendered by `vibra-schema` in step 24.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-diagnostics
cargo test --locked --offline -p vibra-conformance
cargo test --locked --offline --workspace --all-targets --all-features
(Select-String -Path crates/vibra-diagnostics/src/registry.rs -Pattern '"@[a-z]+\.[A-Za-z0-9]+"' -AllMatches).Matches.Value | Sort-Object -Unique | Measure-Object
```

Read: `cargo test -p vibra-conformance` lists
`the_registry_and_the_specification_table_have_the_same_codes` as `ok`, not
`ignored`. The workspace run shows two ignored tests in
`vibra-conformance-v1` (example inventory, native parity) besides the
child-process helpers. The last command counts 115.

The new packages need no crate that is not already in `Cargo.lock`, so
`--offline` holds; run `cargo metadata --offline` once without `--locked`
to add the two package entries, then commit the lock file.

## Excluded

Any handler; any case; `DiagnosticDocument` rendering; a CI line for the
new runner (step 03 adds it with the first cases); the Wasm executor
column (step 27).

## Completion evidence

- [ ] The registry count test and the membership test pass and are not
      ignored.
- [ ] `crates/vibra-conformance-v1/tests/it/architecture_boundary.rs` does
      not exist; the new test lists 26 crates.
- [ ] `conformance/rules.tsv` exists with its header.
- [ ] The validation commands exit 0.
