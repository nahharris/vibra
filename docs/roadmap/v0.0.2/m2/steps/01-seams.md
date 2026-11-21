# Step 01: seams

Kind: infrastructure. It serves every slice: it removes the two files that
every later step would share, before any feature lands.
Depends on: —.
Crates: `vibra-conformance`, `vibra-host`, `vibra-workspace`.

## Prerequisites

- M1 is merged into the M2 integration branch.
- Read: [execution model](../../../execution.md) "Required contents of an
  implementation guide"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md#rule-identities-and-example-coverage)
  "Rule identities and example coverage";
  [architecture](../architecture.md) sections 1 and 4; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md#conformance-profiles)
  "Conformance profiles"; [questions](../spec-questions.md) Q34.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-conformance/src/rules.rs:17` | `RULES_FILE_NAME` | the one rules file of M1 |
| `crates/vibra-conformance/src/rules.rs:227` | `pub fn parse` | parses one file; reused per file |
| `crates/vibra-conformance/src/rules.rs:280` | `pub fn from_path` | gains the directory |
| `crates/vibra-conformance/src/corpus.rs:213` | `rules_path` | where the corpus finds the index |
| `crates/vibra-conformance/tests/it/architecture_boundary.rs:23` | `ARCHITECTURE` | the dependency table |
| `crates/vibra-conformance/src/manifest.rs:68` | `pub enum ManifestError` | the error of a case naming another profile |
| `crates/vibra-conformance/src/profile.rs:23` | `pub enum ConformanceProfile` | gains the profile of each operation |

Proposed: `RuleIndex::from_root`; the directory `conformance/rules/`;
`ConformanceOperation::profile`.

## Files owned

- `crates/vibra-conformance/src/rules.rs`
- `crates/vibra-conformance/src/corpus.rs`
- `crates/vibra-conformance/src/manifest.rs`
- `crates/vibra-conformance/src/profile.rs`
- `crates/vibra-conformance/tests/it/manifest.rs`
- `crates/vibra-conformance/tests/it/architecture_boundary.rs`
- `crates/vibra-conformance/tests/it/rules_directory.rs`
- `crates/vibra-host/Cargo.toml`
- `crates/vibra-workspace/Cargo.toml`
- `crates/vibra-workspace/src/lib.rs`
- `crates/vibra-workspace/tests/it/main.rs`
- `Cargo.lock`
- `conformance/rules/m2-01.tsv`

## Ordered tasks

1. `RuleIndex::from_root(conformance_root)` reads `rules.tsv` and then
   every `rules/*.tsv`, in the byte order of their file names, each with the
   header of `RULES_HEADER`. Invariant: a rule identity is a row of exactly
   one file; a second row for one identity, in any file, is the error that
   a duplicate inside one file already is.
2. The corpus loads the index through `from_root`. Invariant: a corpus
   with no `rules/` directory loads exactly as before.
3. `conformance/rules/m2-01.tsv` holds the header and no row. Invariant:
   an empty rules file is valid.
4. Add the edges `vibra-host` → `vibra-fs-scope` and `vibra-workspace` →
   `vibra-host` to the two manifests and to `ARCHITECTURE`. Invariant: the
   table still forbids `vibra-interp` → `vibra-host` and `vibra-types` →
   `vibra-host`.
5. Declare in `vibra-workspace`, as empty public modules, `entry`,
   `binding`, `dependencies`, and `run`, and in its `tests/it/main.rs`,
   as empty files, the test modules `entry`, `binding`, `dependencies`,
   `run`, `dual_run`, and `attacks_run`. Invariant: a later step adds a
   file's contents and never a module declaration.
6. Update `Cargo.lock` with one `cargo metadata --offline` run without
   `--locked`; state in the report that only the two edges changed.
7. `ConformanceOperation::profile` returns the own profile of each
   operation, as the profile table of the diagnostics chapter gives it.
   Decoding a case manifest rejects a `profile` other than that of its
   `operation`, naming both (Q34). The manifest tests that use another
   profile, among them the `fullV2` manifests of every operation, name
   the operation's own profile. Invariant: every case of the corpus loads
   unchanged.

## Test matrix

Host tests of `vibra-conformance`; the inputs are files the test writes to
a temporary directory.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| rules: two files | positive | `rules.tsv` with `V2-EFFECT-a`, `rules/m2-01.tsv` with `V2-GRANT-b` | both statuses found | 07 Rule identities: "A case has a stable rule identity" |
| rules: no directory | positive | `rules.tsv` only | loads | the same |
| rules: empty file | boundary | `rules/m2-01.tsv` with the header only | loads, adds no row | the same |
| rules: duplicate across files | negative | `V2-EFFECT-a` in both files | the duplicate-identity error, naming the second file | the same: one identity, one rule |
| rules: bad header in a directory file | negative | `rules/x.tsv` starting `id\tstatus` | the header error, naming `rules/x.tsv` | the same |
| rules: not a `.tsv` | boundary | `rules/README.md` | ignored | — (harness rule) |
| architecture: new edges | positive | the workspace as it is after task 4 | the test passes | architecture, section 1 |
| manifest: the profile of the operation | positive | the whole corpus; and one manifest per operation naming its own profile | every case loads (Q34) | 07 Conformance profiles: "A case names the profile its operation belongs to" |
| manifest: another profile | negative | `operation = "workspaceCheck"`, `profile = "interpreterV2"` | the manifest error naming `workspaceCheck` and `staticV2` (Q34) | 07: "a case manifest that names any other is malformed, and loading the corpus rejects it" |
| manifest: a profile with no operation of its own | negative | `operation = "interpret"`, `profile = "fullV2"`; and the same with `nativeV1` | the manifest error naming `interpreterV2` (Q34) | the same |
| architecture: forbidden edge | negative | a manifest table with `vibra-interp` → `vibra-host` given to the checking function | the test reports that edge | architecture, section 1 |

## Diagnostics and schema changes

None.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-conformance -p vibra-host -p vibra-workspace --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: every command exits 0; the corpus runner reports the same case count
as before the step, all passing.

## Stays unavailable

Everything M1 left unavailable. This step changes no observable behaviour
of the checker, the interpreter, or the tool.

## Excluded

Any case, any registry row, any module of `vibra-host`.

## Completion evidence

- [ ] `git diff --stat` names only the files owned.
- [ ] `Cargo.lock` gained exactly two dependency lines.
- [ ] The eleven rows above are tests, named in the report.
- [ ] The declared modules are empty and documented with the step that
      fills each.
- [ ] The validation commands exit 0.
