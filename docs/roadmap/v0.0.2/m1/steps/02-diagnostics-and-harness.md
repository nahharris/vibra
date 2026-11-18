# Step 02: diagnostics-and-harness

Kind: infrastructure and specification prerequisite. The registry is
compiler data copied from the specification table; the harness is the
corpus runner. Neither is language behavior.

## Prerequisites

- Step 01 landed.
- Read: [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md): "Diagnostics are
  a language surface", "Diagnostic documents and spans", "Conformance corpus",
  "Rule identities and example coverage", "Conformance profiles"; [tooling
  chapter](../../../../spec/05-tooling.md) "Versioned envelope and results" for
  the status words; architecture section 3.
- Open questions that block parts of this step: none. Q4 (tree snapshot
  format) affects step 03, not this one.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-diagnostics-v1/src/span.rs:23` (`ByteSpan`), `line_index.rs`, `diagnostic.rs:147` (`Diagnostic`) | copied unchanged into the new crate |
| `crates/vibra-diagnostics-v1/src/registry.rs:173` (the `ALL` table macro) | the shape to keep; the rows are replaced |
| `crates/vibra-conformance-v1/src/{corpus.rs,manifest.rs,runner.rs,profile.rs}` and `src/bin/conformance.rs:38` | copied into the new crate and edited as below |
| `crates/vibra-conformance-v1/tests/it/architecture_boundary.rs:22` | moved to the new crate; the v1 copy is deleted so one table rules |
| `crates/vibra-conformance-v1/tests/it/diagnostic_registry.rs:24` (`specification_table`) | copied; the ignored membership test is restored here |

Proposed crates and modules:

- `crates/vibra-diagnostics`: `span`, `line_index`, `diagnostic`, `registry`.
- `crates/vibra-conformance`: `corpus`, `manifest`, `profile`, `runner`,
  `rules` (reads `conformance/rules.tsv`), `standard` (empty dispatcher),
  `bin/conformance.rs`; tests `architecture_boundary.rs`,
  `diagnostic_registry.rs`, `manifest.rs`.

## Ordered tasks

1. Create `vibra-diagnostics` with the copied span, line index, and
   diagnostic types. Invariant: `Diagnostic` derives its level from its
   code and stores no level.
2. Generate the registry: one variant per row of the specification table,
   atom spelled exactly as the table, domain enum from the first components
   (`syntax`, `name`, `module`, `type`, `pattern`, `effect`, `external`,
   `data`, `project`, `runtime`, `style`, `contract`, `grant`, `tool`), fix
   capability `@none` everywhere (no safe fix exists in M1). Invariant: the
   table parser of the v1 test is reused unchanged, so the test and the
   chapter agree on the row shape.
3. Restore the membership test, un-ignored, plus the level, domain, and
   capability tests. Invariant: the test fails on an extra or missing code.
4. Create `vibra-conformance` from the v1 runner core. Edit `manifest`:
   `profile` accepts `readerV2`, `staticV2`, `interpreterV2`,
   `toolingV2`, `wasmTransitionV2`, `fullV2`; `operation` accepts
   `reader`, `projectDecode`, `sourceGraph`, `resolve`, `workspaceCheck`,
   `format`, `interpret`, `workspaceTest` (query and run arrive in M2/M3);
   `rule` must be `active` in `conformance/rules.tsv`; identities use the
   twenty prefixes under `V1-` or `V2-`; snapshot keys `tree`, `formatted`,
   `graph`, `resolved`, `types`, `result`, `testRun`. Remove `codebase`,
   `index`, `query`, `write-native-parity`, and the Wasm and native columns
   (step 27 adds the Wasm column back). Invariant: the runner has no option
   that writes an expectation.
5. `corpus_root` is `conformance/cases`, discovered two levels deep
   (`<PREFIX>/<id>/case.toml`); a case directory whose prefix directory does
   not match its identity is a corpus error.
6. Move the architecture test into the new crate with both the `-v1` rows
   and the new rows (`vibra-diagnostics: []`, `vibra-conformance: every
   crate`). Delete the v1 copy. Invariant: every directory under `crates/`
   has exactly one row.
7. Write `conformance/rules.tsv` with the header and the rule identities the
   step guides name, all `active`, and the retired v1 identities as
   `retired` (step 28 fills the retired rows; this step writes the header
   and the active rows).

## Test matrix

Host tests only; there is no case yet.

| Test | Input | Expected |
| --- | --- | --- |
| registry membership | the chapter table | the implemented set equals the specified set; the test is not ignored |
| registry levels | each code | `@type.suspiciousRow`, `@style.casing`, `@contract.unusedEffect` are `@warning`; every other code `@error` |
| registry domain | each code | the domain's spelling is the code's first component |
| manifest: profile spellings | `profile = "readerV2"` … `"fullV2"` | accepted; `"reader-v1"` rejected as an unknown profile |
| manifest: operation spellings | each of the eight | accepted; `"workspace-check"` rejected |
| manifest: rule status | a case whose rule is `retired` or absent | the corpus load fails naming the case |
| manifest: identity shape | `V2-SRC-READER-x`, `V1-PROJECT-x` | accepted; `V3-SRC-x`, `V2-SRC-x` (unknown prefix) rejected |
| corpus layout | a case under the wrong prefix directory | a corpus error naming both |
| runner: no write mode | the binary's options | `--root` only; `--write-*` is an unknown option, exit 1 |
| empty corpus | `--root` of an empty directory | exit 1, `corpus is empty` |
| architecture | the workspace manifests | every crate has one row; the row matches `Cargo.toml` |

## Diagnostics and schema changes

The new registry is exactly the specification table (114 rows). No schema:
`DiagnosticDocument` version 2 is rendered by `vibra-schema` in step 24.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-diagnostics
cargo test --locked --offline -p vibra-conformance
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: `cargo test -p vibra-conformance` lists
`the_registry_and_the_specification_table_have_the_same_codes` as `ok`, not
`ignored`. The workspace run still shows the v1 copy's two ignored tests
(example inventory, native parity).

## Excluded

Any handler; any case; `DiagnosticDocument` rendering; the Wasm column.

## Completion evidence

- [ ] `cargo test -p vibra-diagnostics` prints `114` in the membership test's
      assertion message or the count check it adds.
- [ ] `Select-String -Path crates/vibra-diagnostics/src/registry.rs -Pattern
      '"@'` yields 114 atoms, all camelCase (no `-`).
- [ ] `crates/vibra-conformance-v1/tests/it/architecture_boundary.rs` does not
      exist; the new test lists 25 crates.
- [ ] The validation commands exit 0.
