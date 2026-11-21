# Step 02: host-registry-2

Kind: infrastructure. Slice 2: the host registry of major version 2, the
test-world registry, and the three grant rows, as data with a checked-in
snapshot. Package N7 of the native contract.
Depends on: 01.
Crates: `vibra-ir`, `vibra-host`, `vibra-interp`, `vibra-types`.

## Prerequisites

- Read: [runtime chapter](../../../../spec/06-runtime.md#host-registry)
  "Host registry", "Error types", "Registry versioning", "Compiler
  intrinsic registry" (the paragraph on the three narrowing rows);
  [effects chapter](../../../../spec/03-effects.md#standard-operation-inventory)
  "Standard operation inventory", "Delivery and typed outcomes";
  [questions](../spec-questions.md) Q10, Q25.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-ir/src/external/host.rs:161` | `pub struct HostRow` | replaced: gains `owner` as an interface path, `delivery`, typed parameters after the grant |
| `crates/vibra-ir/src/external/host.rs:236` | `HOST_ROWS` | replaced by the 17 rows of major 2 |
| `crates/vibra-ir/src/external/host.rs:377` | `pub fn host_row` | kept, keyed by the new symbols |
| `crates/vibra-ir/src/external/host.rs:397` | `pub fn registry_snapshot` | writes the major-2 snapshot |
| `crates/vibra-ir/src/external/compiler.rs:19` | `REGISTRY_MAJOR` | the registry the three grant rows join |
| `crates/vibra-types/src/stdlib/external.rs:52` | `pub fn check_externals` | the one caller of `host_row` outside `vibra-ir` and `vibra-host` |
| `crates/vibra-host/src/provider.rs:116` | `pub trait HostProvider` | deleted with the rest of the major-1 surface |
| `crates/vibra-host/src/random.rs:13` | `pub struct SplitMix64` | kept unchanged |

Proposed: `vibra-ir::external::host::{HostOp, TestWorldOp, HostInterface,
Delivery, TEST_WORLD_ROWS}`; `CompilerIntrinsic::{GrantRestrict, GrantSub,
GrantLimit}`; `crates/vibra-ir/registry/host-2.0.txt`.

## Files owned

- `crates/vibra-ir/src/external/host.rs`
- `crates/vibra-ir/src/external/compiler.rs`
- `crates/vibra-ir/registry/compiler-2.0.txt`
- `crates/vibra-ir/registry/host-2.0.txt`
- `crates/vibra-ir/tests/it/registry.rs`
- `crates/vibra-host/src/`
- `crates/vibra-host/tests/`
- `crates/vibra-interp/src/intrinsics.rs`
- `crates/vibra-types/src/stdlib/external.rs`

## Ordered tasks

1. Replace the row table with the 17 rows of the chapter: wire symbol,
   `HostOp`, owner (`HostInterface`, with its module and name), the typed
   parameters after the grant, the `Ok` type, the error type, `since: 0`,
   `Delivery::Once`. The rows `io.stdinText` and `io.stdinBytes` have the
   further parameter `max: U64` (Q10). Invariant: one owner per operation
   and one operation per owner.
2. The five error types carry their own cases and the four shared cases
   `denied`, `limitExceeded`, `unsupported`, `outcomeUnknown`. Invariant:
   every case name is camelCase and is in the chapter's tables.
3. Add `TEST_WORLD_ROWS`: the six `testWorld.*` rows owned by `TestWorld`
   in `@std.test`, as a second closed table. Invariant: `host_row` never
   returns a test-world row, and the reverse.
4. Add the rows `grant.restrict`, `grant.sub`, and `grant.limit` to the
   compiler registry, generic over `G`, with the signatures the chapter
   gives, and correct `compiler-2.0.txt` (Q25). In the interpreter's
   intrinsic table they are `Halt::Unavailable` until step 20.
5. `registry_snapshot` writes symbol, owner, exact signature, `since`, and
   delivery class for both host tables; `host-2.0.txt` is written by hand
   from the chapter and compared by the test.
6. Delete the major-1 modules of `vibra-host` (`provider.rs`,
   `operation.rs`, `event.rs`, `world.rs`, `system.rs`) and their tests.
   `lib.rs` keeps `random` and declares the empty modules the later steps
   fill: `path`, `policy`, `recipe`, `broker`, `driver`, `world`, `trace`,
   `test_world`, `startup`, `system`. `tests/it/main.rs` declares, as
   empty files, the test modules those steps own: `policy`, `recipe`,
   `broker`, `scripted`, `world`, `trace`, `test_world`, `startup`,
   `system`, `fs`, `dual`. Invariant: nothing in the workspace names a
   deleted item, and no later step edits `lib.rs` or `main.rs`.
7. `check_externals` reads the new rows: a `@host` symbol resolves in
   either host table; the owner it compares is the enclosing interface's
   module and name.

## Test matrix

Tests of `vibra-ir` (`tests/it/registry.rs`).

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| each of the 17 host rows | positive | `host_row("fs.readText")` and the other sixteen symbols | the owner, further parameters, and result of the chapter's table | 06 Host registry, the row of that symbol |
| each of the 6 test-world rows | positive | `test_world_row("testWorld.directory")` and the other five | owner `TestWorld`, module `@std.test` | 06 Host registry: "one row per member" |
| the stdin rows take a count | positive | `host_row("io.stdinText")` and `host_row("io.stdinBytes")` | each has the one further parameter `max: U64` (Q10) | 06 Host registry, the rows of the two symbols |
| each error type | positive | `HostErrorType::Fs.cases()` and the other four | its own cases followed by the four shared ones | 06 Error types, both tables |
| the three grant rows | positive | `grant.restrict`, `grant.sub`, `grant.limit` | `Fn({G FsPolicy} Result(G ScopeError))`, `Fn({G RelativePath} Result(G ScopeError))`, `Fn({G GrantLimits} G)` | 06 Compiler intrinsic registry: "are the rows `grant.restrict`, `grant.sub`, and `grant.limit`" |
| every row is `@once`, `since` 0 | positive | all 17 | `Delivery::Once`, 0 | 06 Host registry: "Every row of the host table below and of the test-world registry has `since` 0 and delivery `@once`" |
| a major-1 spelling | negative | `host_row("io.stdin-text")` | `None` | 06 Host registry: "wire symbols are dotted camelCase strings" |
| a test-world symbol in the host table | negative | `host_row("testWorld.output")` | `None` | 06 Host registry: "a separate closed test-world registry" |
| registry snapshot | boundary | `registry_snapshot()` against `host-2.0.txt` and `compiler-2.0.txt` | equal, byte for byte | 06 Registry versioning: "A checked-in registry snapshot … MUST reject an unintended change" |
| a changed row | negative | the snapshot with one result type edited, given to `snapshot_violations` | one violation naming the symbol | the same |
| derived minor | boundary | `derived_registry_minor` of no symbol, and of all 17 | 0 and 0 | 06 Registry versioning: "the greatest `since` among them, or zero for none" |

## Diagnostics and schema changes

No diagnostic. The registry snapshots are data files of `vibra-ir`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-ir -p vibra-host -p vibra-interp -p vibra-types -p vibra-workspace -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0. The corpus count is unchanged: no library module binds a
host symbol yet, so no case sees the registry.

## Stays unavailable

A `proc`, a host call, and the three narrowing members: the rows exist as
data and nothing runs them. `vibra-host` has no world, broker, or trace
until steps 14 to 17.

## Excluded

The library modules that bind the rows (19, 21); any dispatch (15, 20);
the native track's registry, which stays on `vibra-ir-v1`.

## Completion evidence

- [ ] `host-2.0.txt` has 17 host rows and 6 test-world rows.
- [ ] `compiler-2.0.txt` gained exactly three rows.
- [ ] `git grep -n "stdin-text" crates/vibra-ir crates/vibra-host` finds
      nothing.
- [ ] No file of `crates/vibra-host/src/` contains `@hostWorld.v1` or
      `@auditTrace.v1`.
- [ ] The validation commands exit 0.
