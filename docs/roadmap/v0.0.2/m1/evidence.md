# M1 implementation evidence

Status: local implementation, review, and evidence collection finished; cumulative draft delivery remains in progress until merge. This document records the
cumulative draft's evidence; it does not mark M1 landed or authorize a release.

The integration branch is `v0.0.2/m1`, based on
`c81a2836d84115d91af4a4baefca0a0ee1b5ff19`. Implementation uses a separate Git
worktree and preserves the original checkout. Delivery is one cumulative draft
PR, as recorded in the [execution plan](execution.md).

## Reproducible entry points

The [validation policy](validation.md) lists the final host, independent corpus,
lint, documentation, and reader-fuzz commands. The
[recursive demo](../../../../examples/codebase-demo/README.md) exercises import,
two revisions, separate-process reopen, addressed reads, exact export, backup,
and independent restore.

The [address fixtures](address-vectors.md) and
[independent reference script](address-vectors.py) fix canonical frame bytes,
component digests, member ordinals, and object addresses. They distinguish a
complete simple declaration projection from isolated SCC wire fixtures.

## Gate-to-evidence map

| Obligation | Focused evidence |
| --- | --- |
| Deterministic SCC addresses, recursion, dependency pinning | `codebase::graph::vectors`, graph encoder host tests, recursive corpus import/lifecycle |
| Generic/lexical alpha-renaming and trivia separation | `codebase_import::trivia_and_lexical_generic_alpha_renaming_preserve_checked_addresses` |
| Non-running complete checking, including helper-only test modules | `codebase_import` source-rejection and failing-assertion preparation tests |
| Native source meaning retained beside acceleration | IR `accelerated_functions_retain_source_meaning_without_changing_runtime_lowering`; types `native_source_bodies_remain_available_beside_accelerated_wrappers` |
| Nominal types, generics, explicit implementation, effect ownership | `V1-TOOL-codebase-declaration-kinds`, implementation/effect invalidation host tests |
| Old/new revision reads, exact relations, unrelated deduplication | `V1-TOOL-codebase-lifecycle`, CLI import/read process tests |
| Exact absent-destination export and stale conflict | `V1-TOOL-codebase-roundtrip`, CLI stale-export test, exclusive-rename host tests |
| Unknown selectors and revision-pinned edges | CLI lifecycle assertions cover well-formed unknown revision/entity/object selectors and old/new call targets; lifecycle corpus checks both edge sets after publication |
| Competing publishers and busy/stale distinction | CLI `separate_publisher_processes_compare_the_same_expected_head_atomically`, store transaction tests |
| Interrupted publication and uncertain success | `codebase::store::publication_tests::process_interruption_at_each_publication_boundary_is_atomic` checks five transaction boundaries, including committed state before response and an explicit deduplicating retry |
| Confined paths and exclusive publication | Confined-directory dot/parent traversal regression, export escape/junction test, exclusive rename tests, and before/after child-process export/backup/restore tests |
| Integrity, schema/record version rejection, derived index recovery | `V1-TOOL-codebase-integrity`, store corruption/reindex tests |
| Consistent independent backup and restore | `V1-TOOL-codebase-backup`, `codebase_concurrency::backup_overlapping_publication_restores_one_complete_committed_snapshot` |
| Malformed and deeply nested records | Closed scalar leaf cases and 512 nested definition lists validated on a 64 KiB worker stack; WAL-header opens create no sidecars |
| Closed machine contracts | Full command-result schema suite; CLI result-envelope validation; diagnostic registry tests |
| Local measurements | Ignored manual `codebase_measurements::measure_local_codebase_workload`; recorded artifact and configuration rationale follow integration |

## Platform boundary

Local evidence is collected on Windows 11 build 26300, x86-64, with Rust
1.96.1 (`31fca3adb`, LLVM 22.1.2), a Core Ultra 7 155H (16 cores, 22 logical
processors), 32 GiB RAM, and a local NTFS volume. Linux and macOS evidence belongs to the PR's
CI matrix; local Windows results alone do not establish those platforms.

M1 requires trusted local workspace directories and filesystem support for
SQLite locking, file synchronization, and exclusive no-replace publication.
Network mounts, concurrent file synchronization, and hostile ancestor rebinding
are excluded. Windows directory-metadata power-loss durability is not claimed.
See the [normative filesystem contract](../../../spec/04-programs-and-packages.md#codebase-container-and-portable-records).

## Recovery and performance handoff

Private reindex race tests synchronize at the rebuilding checkpoint and report
worker failure/disconnection directly. They impose no wall-clock completion
threshold: the original 120-second test timeout was exceeded while the worker
was still checking records under concurrent load.

Temporary phase instrumentation was removed after a successful isolated
head-reuse test. On its tiny package (163–164 objects, 145–146 components,
1,408–1,410 references), canonical parsing and exact-source replay dominated
the unoptimized debug workload; SQL writes and commits took less than 70 ms.
These observations are not latency guarantees. The repeatable measurements
below use the final configuration and distinguish process/connection startup
from a cold operating-system page cache.

M2 can share parsed records within a consistent read snapshot and avoid
repeating validation of the same component. It must preserve closure,
provenance, corruption detection, and exact-source index reconstruction.
No semantic query, MCP, or cross-process cache is introduced in M1.

Unsupported no-replace filesystem errors are propagated without an overwrite
fallback. Local runtime evidence exercises NTFS's supported operation and
existing-destination refusal; it does not simulate an unsupported filesystem.

## Measured configuration

The canonical [measurement record](measurements.vibon) comes from the ignored
`codebase_measurements::measure_local_codebase_workload` test, explicitly run
with the full workspace feature set and one test thread. Its 128 local
functions produce 291 objects, 1,410 relations, and three source attachments.
The final two-revision store occupies 6,930,432 bytes (6.61 MiB).

SQLite is bundled 3.54.1 through rusqlite 0.42.0. The selected configuration is
DELETE journaling, FULL synchronization on writable connections, and a fixed
500 ms busy timeout. DELETE preserves read-only opens without WAL/shared-memory
sidecar creation; the failure, race, interruption, backup, and restore tests use
these same settings. SQL commit time is not the dominant cost in this debug
workload, so this evidence does not justify weakening synchronization.

| Operation | Observed seconds |
| --- | ---: |
| First checked import: preparation plus publication | 15.436 |
| Identical checked import: fresh preparation plus deduplicating publication | 45.572 |
| First publication alone | 6.466 |
| Deduplicating publication alone | 35.945 |
| New process reopen/head | 10.031 |
| New connection reopen/head | 9.389 |
| Entity lookup median / maximum, 10 samples | 3.591 / 3.767 |
| Addressed definition and direct relations median / maximum, 10 samples | 3.563 / 3.643 |
| Changed publication overlapping 20 pinned reads | 39.711 |

Each publication/startup measurement has one sample. The concurrent publication
succeeded without a busy result in this run; the dedicated lock test separately
forces `@store.busy` and proves an explicit retry preserves the original expected
head. Both startup measurements use a warm filesystem page cache; neither is an
OS-cache-cold disk measurement. Project crates use the default unoptimized test
profile, with dependency optimization configured by the repository. These are
local observations, not release latency promises.

## Final results

Implementation snapshot: `a3332ff6ff581c831dd691d254aaab4221c1c4d3`.
The following documentation-only commit records these results. Commands used
`CARGO_BUILD_JOBS=2`, the pinned toolchain, `RUSTFLAGS=-D warnings`, and a shared
Cargo target directory. The host run used two test threads; the measurement run
used one. No required M1 case was left unavailable.

| Check | Actual result |
| --- | --- |
| `cargo fmt --all --check` | exit 0 |
| `cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings` | exit 0 |
| Full host workspace, all targets/features, `--no-fail-fast` | 1,276 passed, five fixture/inventory failures, eight ignored; exit 101 before the fixture repairs below |
| Exact reruns of all five repaired cases, through the full workspace feature set | 5 passed, 0 failed; exit 0; total regular host coverage is 1,281 passing cases |
| `cargo doc --locked --offline --workspace --no-deps --all-features`, `RUSTDOCFLAGS=-D warnings` | exit 0 |
| Independent `vibra-conformance --root conformance/cases` | 602 passed, 0 failed, 0 unavailable; exit 0 |
| `cargo run --locked --offline -p vibra-conformance --bin m1-fuzz -- --profile ci-smoke` | six targets, 96 cases, exit 0 |
| Explicit ignored measurement test | 1 passed; exit 0 |
| Separate-process recursive demo | two revisions, old/new addressed reads, stale rejection, exact export, independent restore; exit 0 |
| Independent Python address vectors | all six matched; exit 0 |
| Local Markdown targets/anchors and `git diff --check` | passed |

The five repaired host cases were the diagnostic table count (107 registered
codes, independently compared with the specification), the number of examples
with backend tests (four), a nominal-object fixture that renamed its key without
rebuilding its entity mapping, the assertion fixture's obsolete flattened field
spelling, and Windows junction command quoting. They changed test setup or
assertions, not the implementation's validation guards. Their exact test filters
are:

- `registry::tests::the_registry_has_every_code_in_the_specification_table`
- `examples_backends::the_tests_of_every_example_give_identical_rows_in_both_backends`
- `codebase::graph::encoding::tests::nominal_keys_do_not_collapse_equal_definitions_and_missing_refs_fail`
- `codebase_import::preparing_an_import_checks_tests_without_running_a_failing_assertion`
- `codebase_store::export_refuses_a_destination_parent_replaced_by_a_directory_link`

Use the host command in [validation](validation.md) with `--` followed by
`--exact` and those five filters to reproduce the targeted reruns. This record
reports the full run plus focused repairs; it does not claim a subsequent clean
monolithic host invocation. The PR's CI must run that invocation on its final
head before merge.

The eight ordinary host ignores are two child-only helpers exercised by their
parent tests, five existing Windows source-loader symlink fixtures requiring
unavailable privileges, and the manual measurement test executed separately.
M1's junction-based export rejection test ran successfully on this host.

The independent corpus totals are reader 89, static 305, interpreter 181, and
tooling 27. All 173 executable cases passed in the interpreter and matched in
Wasm. The final runner was built by the full workspace test build and executed
from a private binary copy to avoid relinking an executable used by concurrent
Windows host tests. The Cargo reproduction command remains in
[validation](validation.md).

The canonical [demo record](demo-evidence.vibon) preserves its two revision and
object addresses. Both evidence records were checked through the existing
VIBON formatter and matched their canonical bytes. The example also passes its
two language tests in both backends through the repository example gates.

The standing draft is identified by head branch `v0.0.2/m1` into `main`.
Its attached CI checks supply Linux/macOS evidence and a clean final-head host
run; those remote checks are pending at local handoff. M1 remains in progress
until merge with green CI. No release tag or publication is part of this draft.
