# Step 28: cutover

Kind: infrastructure. The single point where the v0.0.1 surface leaves the
repository: every `-v1` crate, `stdlib-v1/`, `examples-v1/`, `fuzz-v1/`,
and `conformance/v1/` are deleted, with their CI lines and architecture
rows. Nothing is added except what replaces a deleted reference. After this
step one reader, one checker, one corpus, and one `vibra` binary exist.

## Prerequisites

- Steps 08, 20, 22, 23, 26, and 27 landed.
- Package X2-01 of the [Nexus plan](../../../nexus/plan-v0.0.2.md) landed
  its first PR, which moves the native track to the new `vibra-ir`,
  `vibra-native`, and `vibra-host`. If it has not when every other
  prerequisite has, task 2 applies.
- Read: [architecture](../architecture.md) decisions 4 to 7 and the
  corpus section; [release README](../../README.md) "Scope and priorities";
  [diagnostics chapter](../../../../spec/07-diagnostics-and-conformance.md)
  "Required implementation suites", "Exclusions and retired surfaces";
  `AGENTS.md`.

## Entry points

| Existing | Change |
| --- | --- |
| `Cargo.toml` (`[workspace] members`, `[workspace.dependencies]`) | the thirteen `-v1` entries removed |
| `crates/vibra-{diagnostics,syntax,fmt,resolve,types,ir,interp,host,native,workspace,schema,cli,conformance}-v1/` | deleted |
| `stdlib-v1/`, `examples-v1/`, `fuzz-v1/`, `conformance/v1/` (with `conformance/v1/native-parity.tsv`) | deleted |
| `.github/workflows/ci.yml` (the v1 conformance and fuzz lines step 01 renamed) | removed |
| `crates/vibra-conformance/tests/it/architecture_boundary.rs` (step 02) | the `-v1` rows removed |
| `crates/vibra-conformance-v1/tests/it/native_parity_inventory.rs` | deleted with its crate; package X2-04 writes the inventory of the new corpus and its test |
| `README.md`, `AGENTS.md`, `docs/roadmap/README.md`, `docs/roadmap/v0.0.2/README.md` | every path, command, and status line that names a deleted thing |

No new module.

## Ordered tasks

1. Preflight: `cargo tree --workspace --invert` for each `-v1` package
   shows no dependent outside the `-v1` set. Any other dependent stops the
   step.
2. Native track fallback: if a native crate still depends on a `-v1`
   crate, re-point its imports with the migration table of step 12. A test
   that cannot be re-pointed mechanically is moved to a list in
   `docs/roadmap/nexus/` as X2-01's remainder, with its file and name, and
   leaves with the crate that hosted it. No native crate is deleted and no
   native test is weakened in place. The list is the record of every test
   this task removes.
3. Delete the thirteen crates and the four directories with `git rm -r`.
   Remove their workspace entries. `Cargo.lock` loses exactly the removed
   package blocks; `cargo metadata --locked --offline` must still succeed.
4. CI: remove the v1 lines; the conformance job runs the new corpus, the
   Wasm executor row, and the new fuzz smoke.
5. Architecture test: remove the `-v1` rows; the table lists every
   remaining crate once.
6. Repository documents: `README.md` describes the keyword-led language
   with one example that `vibra check` accepts, placed at
   `examples/hello/`; `AGENTS.md` commands name the new packages; the
   roadmap status lines say what is current. No sentence narrates the
   removal.
7. Search for leftovers: `-v1` in a crate name, `conformance/v1`,
   `stdlib-v1`, `examples-v1`, `fuzz-v1`, `vibra-v1`, and the profile
   spellings `reader-v1`, `static-v1`, `interpreter-v1`, `tooling-v1`. Each
   hit is removed, or is a wire name the runtime chapter keeps
   (`vibra_v1_`, `vibra_native_v1`, `@hostWorld.v1`, `@auditTrace.v1`,
   `nativeV1`).

## Test matrix

| Test or case | Kind | Input | Expected |
| --- | --- | --- | --- |
| `V2-SRC-READER-top-paren` | negative, reader | the case of step 04: `(defn main () void void)` | one `@syntax.invalidForm` over the parenthesized form; nothing else |
| `V2-SRC-READER-recovery-then-valid` | recovery, reader | the case of step 04: an invalid top-level form followed by a valid declaration | one diagnostic; the declaration is read |
| `V2-TOOL-format-excluded-form` | formatter, format | `(defn main () void void)` followed by a valid `fn` | `@syntax.invalidForm`; the bytes unchanged |
| `examples_check` | positive, host | `vibra check` in `examples/hello/` | exit 0, `status: "passed"` |
| `architecture_boundary` | boundary, host | the workspace | one row per crate; none ends in `-v1` |
| `no_v1_paths` | boundary, host | a test that walks the repository root | none of the four directories exists |

The two reader cases landed in step 04 and are confirmed here; the format
case is new.

## Diagnostics and schema changes

Codes: `@syntax.invalidForm`. Schemas: every `schemas/v1/` document leaves
with `vibra-schema-v1`; `schemas/v2/` is the only schema directory.

## Ignored tests

`native_parity_inventory::a_pending_cell_names_a_package_of_the_plan`, the
last ignored test of the v0.0.1 surface, is deleted by this step with
`vibra-conformance-v1` and `conformance/v1/native-parity.tsv`. It is not
restored in M1: the native parity inventory of the new corpus belongs to
package X2-04. After this step the workspace has no ignored test that
names a milestone as its reason.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
(cargo metadata --locked --offline --format-version 1 --no-deps | ConvertFrom-Json).packages.name | Select-String -- '-v1$'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline --workspace --all-targets --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases --executor wasm
cargo run --locked --offline -p vibra-conformance --bin fuzz -- --profile ci-smoke
cargo run --locked --offline -p vibra-cli --bin vibra -- --workspace examples/hello check
git grep -n -E "conformance/v1|stdlib-v1|examples-v1|fuzz-v1|vibra-v1" -- . ":!docs/roadmap/v0.0.2/m1"
```

Read: the second line prints nothing; the suites and the corpus pass; the
Wasm row has no failed case; the last command prints nothing (this
directory is excluded because the step guides name the transition).

## Excluded

Any language change; deleting or rewriting a native crate; removing the
Wasm backend (v0.0.3); rewriting `archive/`; the M1 exit evidence (29).

## Completion evidence

- [ ] `Get-ChildItem crates -Name` lists no `-v1` directory and the four
      top-level directories are gone.
- [ ] The workspace has one `vibra` binary.
- [ ] If task 2 ran, the remainder list exists and names every moved test.
- [ ] `conformance/rules.tsv` has an `active` row for
      `V2-TOOL-format-excluded-form`.
- [ ] `cargo test --workspace -- --ignored --list` names no test of the
      three that the specification merge ignored.
- [ ] Validation commands exit 0 and the two search commands print nothing.
