# Step 27: runtime-crate-port

Kind: infrastructure. The four kept runtime crates (`vibra-host`,
`vibra-native`, `vibra-wasm`, `vibra-wasm-run`) move from the `-v1` crates
to the new `vibra-ir` and `vibra-syntax`, and the conformance runner gains
the WebAssembly column. The port is fail-closed: a checked-IR node the
backend does not lower is reported unsupported and is never approximated.
After this step no kept crate depends on a `-v1` crate except the native
track, which package X2-01 moves.

## Prerequisites

- Steps 03 and 13 landed (the data reader and the interpreter, whose
  results are the reference). Executable cases arrive from step 15; the
  column runs whatever exists.
- Read: [runtime chapter](../../../../spec/06-runtime.md) "Wasm transition" (the
  module, imports, exports, the fail-closed rules), "Compiler intrinsic
  registry", "Native implementations", "Canonical value encoding", "Host world",
  "Audit trace"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md) "Conformance
  profiles" (`wasmTransitionV2`), "Differential execution"; [programs
  chapter](../../../../spec/04-programs-and-packages.md) "VIBON data documents";
  [architecture](../architecture.md) decisions 5 and 7; the migration table step
  12 publishes in `crates/vibra-ir/MIGRATION.md`.
- Open questions: Q1 (the release README's sentence about the v0.0.1
  corpus on the Wasm backend) must be answered before this step starts,
  because from here the backend does not run that corpus.

## Entry points

| Existing | Change |
| --- | --- |
| `crates/vibra-wasm/src/lib.rs:154` (`emit`), `:181` (`emit_tests`), `:364` (`lowers_primitive`) | signatures keep their shape over the new `CheckedProgram`; `lowers_primitive` reads the new intrinsic registry |
| `crates/vibra-wasm/src/lower.rs` and `src/lower/{dispatch,iterate,keys,typetab,assertion}.rs` | every `match` over IR nodes is rewritten against the new node set; each arm either lowers or returns `EmitError::Unsupported` |
| `crates/vibra-wasm/src/{layout,desc,types,pattern}.rs` | variant types with named fields replace unions; `List` replaces `Array` |
| `crates/vibra-wasm/src/error.rs` (`EmitError`) | carries the unsupported node and its origin |
| `crates/vibra-wasm-run/src/lib.rs:254` (`Runner`), `src/{observe,test_run}.rs` | observation renders canonical values through the new `vibra-ir` encoder |
| `crates/vibra-host/src/{world,event}.rs` | the host world and audit documents are read with `vibra_syntax::data` and named `@hostWorld.v1` and `@auditTrace.v1` |
| `crates/vibra-native/src/lib.rs` | compiles against the new `vibra-ir` registry rows |
| `crates/vibra-conformance/src/runner.rs` (step 02) | the Wasm column and the per-executor counts |
| `crates/vibra-conformance-v1/src/` (the v1 Wasm adapter) | the v1 runner's Wasm column is removed; its interpreter column stays |
| `Cargo.toml` of the four crates | `vibra-ir-v1`, `vibra-syntax-v1`, `vibra-diagnostics-v1` become `vibra-ir`, `vibra-syntax`, `vibra-diagnostics` |

Proposed: `vibra-conformance::wasm` (the executor adapter);
`conformance/wasm-support.tsv` (one row per executable case: `supported` or
`unsupported` with the node that stopped it), written by hand from the
runner's report and compared by a host test.

## Ordered tasks

1. `vibra-native`: switch the dependency, fix the registry row types, keep
   every test. Invariant: no native routine changes behaviour; the sample
   vectors of `vibra-ir` still pass.
2. `vibra-host`: switch dependencies; decode the world and trace documents
   with the data reader of step 03; respell the format atoms as
   `@hostWorld.v1` and `@auditTrace.v1` in the crate and its test
   fixtures. Field shapes do not change; M2 replaces both documents.
3. `vibra-wasm`: port the lowering in this order: scalars and intrinsics;
   tuples and records; variant construction and `match`; `let`, `if`,
   `do`, `return`; calls, closures, tail calls; `List`, `Dict`, `Str`,
   `Bytes` through the native imports; interface dispatch. Guard
   consequences (`entry_guard`, `exit_guard`, fallible constructors),
   module-value initialization, and anything the migration table marks as
   having no v1 equivalent return `EmitError::Unsupported` unless the
   mapping is one to one. Invariant: `emit` is total over well-formed
   programs and never panics; an unsupported node fails the whole module,
   so a program is never partly executed.
4. `vibra-wasm-run`: switch dependencies; observations through the new
   canonical encoder; the export and import sets are unchanged.
5. v1 runner: remove its Wasm column and the tests that need it. Their
   subject, the v0.0.1 corpus on the Wasm backend, ends here by Q1's
   answer; no other v1 test changes.
6. New runner: an `interpret` or `workspaceTest` case accepted by the
   interpreter is also emitted and run on the backend when the run is
   invoked with `--executor wasm`. The report prints, per profile and
   executor, passed, failed, unknown, and unavailable; an unsupported emit
   counts as unavailable; a missing executor is reported missing. A
   mismatch names the executor and the first difference.
7. `conformance/wasm-support.tsv` and its host test: the file equals the
   runner's classification; a case that moves from `supported` to
   `unsupported` fails the test.
8. Architecture test: the four crates' rows name the new dependencies.

## Test matrix

Host tests in the four crates; the corpus cases are those of the checker
steps, run a second time on the backend.

| Test or case | Kind | Input | Expected |
| --- | --- | --- | --- |
| `emit_scalar_program` | positive | checked IR for `fn main returns r: I32 do return I32.add(1i32 2i32) endfn` | a module that validates; run result `3i32`, equal to the interpreter |
| `emit_variant_match` | positive | IR for a `match` over `Option(I32)` | equal to the interpreter |
| `emit_tail_recursion` | boundary | IR for a tail-recursive count to 1,000,000 | completes; no stack growth |
| `emit_unsupported_guard` | negative | IR with an `entry_guard` consequence | `EmitError::Unsupported` naming the node and its origin; no module |
| `emit_unsupported_is_total` | recovery | each IR node kind once, generated from the node enumeration | every kind either emits or returns `Unsupported`; none panics |
| `host_world_decodes_data_grammar` | positive | a `@hostWorld.v1` document in the data grammar | the same `HostWorld` the fixture describes |
| `host_world_rejects_other_format` | negative | the document with format atom `@hostWorld.v2` | `@data.invalidShape` |
| `audit_trace_round_trip` | positive | a trace with three events | canonical text decodes to equal events |
| `native_vectors` | positive | the shared sample vectors | all pass |
| `wasm_column_reports_unavailable` | boundary | a corpus case using a guard with `else`, run with `--executor wasm` | counted unavailable for the Wasm executor, passed for the interpreter |
| `wasm_column_mismatch_fails` | negative | a fault-injection hook that flips one result | the report names `wasm` and the first difference; non-zero exit |
| `wasm_support_inventory` | boundary | `conformance/wasm-support.tsv` | equals the runner's classification |

Formatter: not applicable.

## Diagnostics and schema changes

Codes: `@tool.unavailable` (an unsupported emit, reported by the runner),
`@data.invalidShape`, `@runtime.invalidCheckedProgram` (a module that fails
validation after emit, a toolchain defect). No machine schema changes. The
host document format atoms are `@hostWorld.v1` and `@auditTrace.v1`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-native -p vibra-host -p vibra-wasm -p vibra-wasm-run -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases --executor wasm
cargo run --locked --offline -p vibra-conformance-v1 --bin vibra-conformance-v1 -- --root conformance/v1/cases
cargo tree --locked --offline -p vibra-wasm-run -e normal
cargo test --locked --offline --workspace --all-targets --all-features
```

`vibra-conformance-v1` is the name step 01 gives the existing package.
Read: the `--executor wasm` run prints a Wasm row with no failed case; the
v1 run passes on its interpreter column; the `cargo tree` output contains
no `-v1` package.

## Excluded

Lowering guard consequences, grants, plans, or the new entry to Wasm (the
runtime chapter forbids inventing that ABI); the native track
(`vibra-exec`, `vibra-lower`, `vibra-codegen`, `vibra-build`,
`vibra-nexus`, `vibra-nexus-eval`, `vibra-nexus-bridge`, `vibra-actors`),
which X2-01 moves; any change to the major-1 import and export sets.

## Completion evidence

- [ ] `cargo tree -p vibra-host`, `-p vibra-native`, `-p vibra-wasm`,
      `-p vibra-wasm-run` list no `-v1` package.
- [ ] `conformance/wasm-support.tsv` exists and its test passes.
- [ ] The counts of supported and unsupported executable cases are in the header
      comment of `wasm-support.tsv`.
- [ ] Validation commands exit 0.
