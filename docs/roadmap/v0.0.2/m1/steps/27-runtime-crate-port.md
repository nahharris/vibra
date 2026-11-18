# Step 27: runtime-crate-port

Kind: infrastructure. The runtime crates below the interpreter arrive on
the new `vibra-ir`: `vibra-native` and `vibra-host` are created under their
final names from their `-v1` sources, and `vibra-wasm` and `vibra-wasm-run`
are ported in place to the transition profile of the runtime chapter. The
conformance runner gains the WebAssembly column. The port is fail-closed: a
checked-IR node the backend does not lower is reported unsupported and is
never approximated. After this step the only crates that depend on a `-v1`
crate are the `-v1` crates themselves and the native track, which package
X2-01 moves.

This step is the runtime part of the packages N1 and N2 and the whole of N13 of
the [native contract](../../m0/native-contract.md#7-suggested-package-cut), as
the [architecture](../architecture.md#native-contract-packages) maps them. It
edits no `-v1` crate except to remove the Wasm column of the v1 runner.

## Prerequisites

- Steps 03 and 13 landed (the data reader and the interpreter, whose
  results are the reference). Executable cases arrive from step 15; the
  column runs whatever exists.
- Read: [runtime chapter](../../../../spec/06-runtime.md) "Wasm transition"
  whole, "Traps", "Compiler intrinsic registry", "Native implementations",
  "Canonical value encoding"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md) "Conformance
  profiles" (`wasmTransitionV2`), "Differential execution";
  [native contract](../../m0/native-contract.md) sections 2 and 6 and
  package N13; [architecture](../architecture.md) decisions 5 and 7;
  `crates/vibra-ir/migration.md` (step 12).

## Files

| Path | Action |
| --- | --- |
| `crates/vibra-ir/src/arena.rs`, `src/boundary.rs`, `src/external/host.rs`, `src/external/native.rs`, `src/external/vectors.rs`, `src/external/vectors/numbers.rs` | created as copies of the files of the same names in `crates/vibra-ir-v1/src/`, edited only where a type they name changed in step 12 (`Array` to `List`, the trap type) |
| `crates/vibra-ir/src/lib.rs`, `src/external.rs` | the new modules declared |
| `crates/vibra-native/` | created: `Cargo.toml`, `src/{lib,arena,number,utf8}.rs`, and `tests/`, copied from `crates/vibra-native-v1/` and compiled against `vibra-ir` |
| `crates/vibra-host/` | created: `Cargo.toml`, `src/{lib,event,operation,provider,random,system,world}.rs`, and `tests/`, copied from `crates/vibra-host-v1/` and compiled against `vibra-ir`, `vibra-syntax`, and `vibra-diagnostics` |
| `crates/vibra-wasm/Cargo.toml`, `crates/vibra-wasm-run/Cargo.toml` | `vibra-ir-v1`, `vibra-diagnostics-v1`, and `vibra-native-v1` become `vibra-ir`, `vibra-diagnostics`, and `vibra-native`; `vibra-host-v1` is removed from `vibra-wasm-run` |
| `crates/vibra-wasm/src/host.rs`, `crates/vibra-wasm-run/src/host.rs` | deleted: the module imports no host symbol |
| `crates/vibra-wasm/src/lower.rs`, `src/lower/{assertion,dispatch,iterate,keys,typetab}.rs`, `src/{layout,desc,types,pattern,primitive,runtime,encode,canon,canon_typed,key,support,error,lib}.rs` | ported to the new node set |
| `crates/vibra-wasm-run/src/{lib,instance,natives,observe,test_run,validate}.rs` | ported; the host provider and the audit trace leave |
| `crates/vibra-conformance/src/wasm.rs` | created: the executor adapter |
| `crates/vibra-conformance/src/{lib,runner}.rs`, `Cargo.toml` | the Wasm column, the option `--executor wasm`, and the dependencies `vibra-wasm` and `vibra-wasm-run` |
| `crates/vibra-conformance/tests/it/wasm_support.rs`, `tests/it/main.rs` | created and declared: the inventory test |
| `conformance/wasm-support.tsv` | created |
| `crates/vibra-conformance/tests/it/architecture_boundary.rs` | rows for `vibra-native` and `vibra-host` added; the rows of `vibra-wasm` and `vibra-wasm-run` name the new dependencies |
| `crates/vibra-conformance-v1/` | the Wasm adapter, its column in the runner, the tests that need it, and the two dependencies removed |
| `Cargo.toml` (`[workspace] members`, `[workspace.dependencies]`) | `vibra-native` and `vibra-host` added |
| `docs/roadmap/v0.0.2/README.md` | the sentence of "Scope and priorities" on the WebAssembly backend says what the backend runs after this step: the pure part of this language under the transition profile |

## Ordered tasks

1. `vibra-ir` runtime modules: copy `arena`, `boundary`, and the two
   external registries from `vibra-ir-v1`. `external::host` keeps the
   major-1 host rows unchanged; package N7 replaces them in M2.
   `boundary` keeps the Wasm strings `vibra_native_v1` and `vibra_v1_`.
   Invariant: the sample vectors of `external::vectors` pass unchanged.
2. `vibra-native`: copy the crate and fix the registry row types. No
   native routine changes behaviour; every test of the `-v1` crate passes
   in the new one.
3. `vibra-host`: copy the crate. It decodes the world and trace documents
   with `vibra_syntax::data` (step 03) and names them `@hostWorld.v1` and
   `@auditTrace.v1` in the crate and its fixtures, which are respelled in
   the brace grammar. Operations, providers, and field shapes do not
   change. Nothing in M1 calls the crate; it is the base that the packages
   N7 to N12 change in M2 and that X2-01 re-points the native track to.
4. `vibra-wasm`: port the lowering in this order: scalars and intrinsics;
   tuples and records; variant construction and `match`; `let`, `if`,
   `do`, `return`; calls, closures, tail calls; `List`, `Dict`, `Str`,
   `Bytes` through the native imports; interface dispatch; assertions.
   The module is the transition module of the runtime chapter:
   - its only imports are the pure native routines of `vibra_native_v1`;
   - its exports are exactly the closed table of the chapter, with the
     prefix `vibra_v1_`: an evaluation module has `eval` and a test module
     has `test`, never both; `status`, `trap_code`, `origin`, `failure`,
     `failure_expected`, `failure_actual`, `result`, `live_size`,
     `release`, `variant`, `length`, the four scalar readers, and
     `read_id`. The exports `entry`, `new`, `seal`, and the `write_*`
     builders leave, because no host value enters the module;
   - `trap_code` returns 1 for `invalidCheckedProgram` and 3 for
     `invalidHostValue`; 2 and 4 are never returned. The kind codes and
     trap codes are constants of `vibra-wasm` alone.
   `emit` and `emit_tests` return `EmitError::Unsupported`, with the node
   and its origin, for a test with `takes`, a `proc`, a plan or `await`
   node, a grant, a host operation, guard consequences (`entry_guard`,
   `exit_guard`, a fallible constructor), module-value initialization, and
   any node the migration table marks as having no v1 equivalent unless
   the mapping is one to one. Invariant: `emit` is total over well-formed
   programs and never panics; an unsupported node fails the whole module,
   so a program is never partly executed.
5. `vibra-wasm-run`: port; observations are rendered through the
   canonical encoder of `vibra-ir`; the runner links only the native
   imports and reads `status`, `trap_code`, and `failure` after `eval` or
   `test` returns. An engine stop without a matching recorded status is
   reported as `@runtime.invalidCheckedProgram`.
6. v1 runner: remove its Wasm column, the adapter, and the tests that
   need them. Their subject, the v0.0.1 corpus on the Wasm backend, ends
   with this step; no other v1 test changes.
7. New runner: an `interpret` or `workspaceTest` case accepted by the
   interpreter is also emitted and run on the backend when the run is
   invoked with `--executor wasm`. The report prints, per profile and
   executor, passed, failed, unknown, and unavailable; an unsupported emit
   counts as unavailable; a missing executor is reported missing. A
   mismatch names the executor and the first difference.
8. `conformance/wasm-support.tsv`: one row per executable case,
   `id<TAB>support<TAB>node`, where `support` is `supported` or
   `unsupported` and `node` is the node kind that stopped the emit or is
   empty. It is written by hand from the runner's report. The host test
   compares it with the runner's classification; a case that moves from
   `supported` to `unsupported` fails the test.
9. Release README: update the sentence named in "Files".

## Test matrix

Host tests in the four crates; the corpus cases are those of the checker
steps, run a second time on the backend.

| Test or case | Kind | Input | Expected |
| --- | --- | --- | --- |
| `emit_scalar_program` | positive | checked IR for `fn main returns r: I32 do return I32.add(1i32 2i32) endfn` | a module that validates; `eval` returns; `status` 0; result `3i32`, equal to the interpreter |
| `emit_variant_match` | positive | IR for a `match` over `Option(I32)` | equal to the interpreter |
| `emit_tail_recursion` | boundary | IR for a tail-recursive count to 1,000,000 | completes; no stack growth |
| `emit_test_module` | positive | IR for one test with `assert.equal(3i32 4i32)` | `test(0)` returns; `status` 3; `failure` 3; `failure_expected` and `failure_actual` hold `3i32` and `4i32` |
| `module_imports_only_natives` | boundary | every module the tests above emit | each import's module name is `vibra_native_v1` |
| `module_exports_are_closed` | boundary | an evaluation module and a test module | the export names are exactly the chapter's table; no `entry`, `new`, `seal`, or `write_*`; never both `eval` and `test` |
| `emit_unsupported_guard` | negative | IR with an `entry_guard` consequence | `EmitError::Unsupported` naming the node and its origin; no module |
| `emit_unsupported_test_with_takes` | negative | IR for a test whose declaration has a grant parameter | `EmitError::Unsupported` |
| `emit_unsupported_is_total` | recovery | each IR node kind once, generated from the node enumeration | every kind either emits or returns `Unsupported`; none panics |
| `trap_codes` | boundary | a fault-injection hook that records each defect trap | `trap_code` 1 and 3; `status` 1 |
| `host_world_decodes_data_grammar` | positive | a `@hostWorld.v1` document in the data grammar | the same `HostWorld` the fixture describes |
| `host_world_rejects_other_format` | negative | the document with the format atom `@hostWorld.v2` | `@data.invalidValue` |
| `audit_trace_round_trip` | positive | a trace with three events | canonical text decodes to equal events |
| `native_vectors` | positive | the shared sample vectors | all pass |
| `wasm_column_reports_unavailable` | boundary | a corpus case using a guard with `else`, run with `--executor wasm` | counted unavailable for the Wasm executor, passed for the interpreter |
| `wasm_column_mismatch_fails` | negative | a fault-injection hook that flips one result | the report names `wasm` and the first difference; non-zero exit |
| `wasm_support_inventory` | boundary | `conformance/wasm-support.tsv` | equals the runner's classification |

Formatter: not applicable.

## Diagnostics and schema changes

Codes: `@tool.unavailable` (an unsupported emit, reported by the runner),
`@data.invalidValue`, `@runtime.invalidCheckedProgram` (a module that
fails validation after emit, or an engine stop with no recorded status).
No machine schema changes.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all -- --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-ir -p vibra-native -p vibra-host -p vibra-wasm -p vibra-wasm-run -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases --executor wasm
cargo run --locked --offline -p vibra-conformance-v1 --bin vibra-conformance-v1 -- --root conformance/v1/cases
cargo tree --locked --offline -p vibra-wasm-run -e normal
cargo tree --locked --offline -p vibra-host -e normal
cargo test --locked --offline --workspace --all-targets --all-features
```

`Cargo.lock` gains the two new package blocks; they are written by hand,
as in step 01, because the build is offline and locked. Read: the
`--executor wasm` run prints a Wasm row with no failed case; the v1 run
passes on its interpreter column; neither `cargo tree` output contains a
`-v1` package, and the first contains no `vibra-host`.

## Excluded

Lowering guard consequences, grants, plans, or the grant-receiving entry
to Wasm (the runtime chapter forbids that ABI); the registry-major-2 host
rows, the broker, grants, policies, the host world and audit trace of
major 2, startup, and `TestWorld` (M2, packages N7 to N12); the native
track (`vibra-exec`, `vibra-lower`, `vibra-codegen`, `vibra-build`,
`vibra-nexus`, `vibra-nexus-eval`, `vibra-nexus-bridge`, `vibra-actors`),
which X2-01 moves after this step; `vibra-fs-scope`, which no crate
depends on until package N9.

## Completion evidence

- [ ] `cargo tree -p vibra-host`, `-p vibra-native`, `-p vibra-wasm`,
      `-p vibra-wasm-run` list no `-v1` package.
- [ ] `conformance/wasm-support.tsv` exists and its test passes.
- [ ] The counts of supported and unsupported executable cases are in the
      PR description.
- [ ] The release README's sentence on the WebAssembly backend is current.
- [ ] Validation commands exit 0.
