# Step 16a — host provider infrastructure

Prerequisite: Step 15 merged. This is an infrastructure step: it creates the harness
that every host operation of Steps 16b–18 is tested through, and it claims no
language behavior. No `@host` external is accepted yet and no operation runs. It is
split from the original Step 16 because the decisions of 2026-10-17 made the host
world a schema with a decoder, a corpus input, and two providers behind one trait,
with a 17-row registry, a snapshot, and an event codec that survives unknown kinds;
see the [README](README.md#steps).

## Read before editing

- [Runtime](../../spec/06-runtime.md): **External providers**, **The host
  registry**, **Registry versioning**, **The host world**, **The audit trace**,
  **Determinism and observability**.
- [Projects](../../spec/04-programs-and-packages.md): **Toolchain standard-library
  input** (the `host` list, `0.3.0`).
- [Diagnostics](../../spec/07-diagnostics-and-conformance.md): **Conformance
  corpus** (the `host-world` input, the event snapshots), **Differential
  execution**, **Required implementation suites**.
- [Ledger](decision-ledger.md) rows D17, D18, D19, D20, D27.1, D30.2, D31.1.

## Scope

1. A new crate, proposed `vibra-host`: the provider trait, the value-access trait
   that each backend implements (read arguments, build results), the 17-row
   registry table as data, the system provider, and the in-memory world.
2. The `@host-world.v1` decoder, with the defaults of the runtime chapter and
   `@data.*` diagnostics for a malformed document, and the SplitMix64 vectors.
3. The audit event: the Rust type, its VIBON codec (`(record kind: @host op: …)`),
   its JSON form (`AuditEvent`), and a decoder that preserves an event of an
   unknown kind as opaque data.
4. The registry table with `since: 0` for every row, and a checked-in snapshot file
   that a host test compares row by row.
5. The manifest: the `host` list between `compiler` and `native`, the load-time
   check of symbol, signature, and owner, and the package version `0.3.0`
   (`stdlib/manifest.vibon` and `vibra-types/src/stdlib.rs`). The list may be empty
   until 16b adds `@std.io`; the shape and the check land here.
6. The corpus loader: an optional `host-world` input on `workspace-run` and
   `workspace-test`, copied fresh per test and per backend.
7. The ambient-read test.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-ir/src/external.rs` and `external/` | The `@compiler` registry the host table sits beside; `RoleTypes` for registry signatures |
| `crates/vibra-types/src/stdlib.rs` | The manifest decoder (`package-version`, `compiler`, `native` at lines 552–555) and its load-time checks |
| `stdlib/manifest.vibon` | The closed record; new field order is `format`, `package-name`, `package-version`, `modules`, `compiler`, `host`, `native`, `assertions` |
| `crates/vibra-conformance/src/corpus.rs`, `manifest.rs` | The case manifest (`inputs`, `expect`) and the loader |
| `crates/vibra-conformance/tests/architecture_boundary.rs` | The architecture table gains `vibra-host`; `vibra-interp` and `vibra-wasm-run` may depend on it, the emitter and the language phases may not |

## Ordered tasks

1. Add the crate and widen the architecture table deliberately. The value-access
   trait and the operation signatures live in `vibra-host`; `vibra-wasm` (the
   emitter) depends only on the registry's symbols and signatures, which sit in
   `vibra-ir`.
2. Write the registry table and the snapshot test: an added row passes only with a
   snapshot update and a `since` greater than the previous minor's; a changed
   symbol, signature, owner, or event shape fails.
3. Write the audit-event type and codec, with round-trip and unknown-kind tests
   shared with the JSON schema's cases of Step 13.
4. Write the host world: decoder, defaults, filesystem resolution, clock, SplitMix64
   (`seed 0` and a second seed pinned as vectors), and a fresh-copy constructor.
5. Write the system provider against the traits, behind no operation yet; its
   ambient reads are the only ones in the workspace's execution crates.
6. Extend the manifest and its load-time check, and bump the package to `0.3.0`
   (every consumer of the string, including the M3 strings in tests, is updated in
   place; no earlier manifest shape is kept).
7. Add the `host-world` input to the corpus manifest, with a host test that a
   malformed document is rejected by the case loader with the data diagnostics.
8. Add the ambient-read test with its allowlist of tooling crates.

Invariants preserved: no language behavior changes; the corpus counts do not move;
the emitter has no engine and no provider dependency; every world is a value that
is copied, never shared between tests or backends.

## Test matrix

- Positive: every row of the table decodes from the snapshot; the default world;
  a world with files, directories, an environment, arguments, a stepped clock, and
  a seed; the SplitMix64 vectors for `count` 0, 1, 8, 9, and across calls; an
  event round trip through VIBON and JSON; a manifest with a `host` list.
- Negative: an unknown field, a duplicate key, a wrong value kind, and an odd dict
  in a world; a snapshot with a changed owner, signature, and symbol; a manifest
  whose `host` list names an unknown symbol, a wrong signature, or a wrong owner;
  an event with an unknown field in a known kind.
- Recovery: a malformed world fails its case and not the corpus; the loader
  reports every world diagnostic once.
- Boundary: a path of many components, a file of 1 MiB in a world, a clock at the
  end of the `u64` range (`unavailable`), `count` 0, an event of a kind the decoder
  does not know.
- Formatter: the world document and the manifest are canonical VIBON; `fmt`
  idempotence over them.

## Diagnostic and schema changes

None to the registry. The `@host-world.v1` document is a VIBON data contract tested
through its typed decoder, like the project and the manifest, so it has no JSON
schema file.

## Validation

The [pre-merge list](validation.md#before-merging-each-step) and `cargo test
--locked --offline -p vibra-host -p vibra-ir -p vibra-types -p vibra-conformance
--test architecture_boundary`. The corpus is unchanged (475 passed).

## Excluded

Any operation, any `@host` external, the Wasm imports and builders, `vibra run`
arguments, user-supplied worlds for `vibra test`, replay.

## Completion evidence

The snapshot, world, codec, manifest, and ambient-read tests pass; the architecture
table names the new crate; the world's defaults match the runtime chapter field for
field.

## As built

Landed as `landed`, conditional on its pull request merging; the base is `m4` at
`f14aa66`, where Step 15 (PR #373) merged. Nothing here changes what a source
program does: `@host` is still `@tool.unavailable`, no operation is reachable from
source, and the corpus has the same 525 cases.

- **Crate layout.** The 17 rows are data in `vibra-ir`
  (`external::host`: `HOST_ROWS`, `HostRow`, `HostType`, `HostErrorType`,
  `host_row`, `derived_registry_minor`, `registry_snapshot`, `snapshot_violations`),
  beside the `@compiler` registry, so the emitter can read symbols and signatures
  without the provider. The new crate `vibra-host` depends on `vibra-diagnostics`,
  `vibra-ir`, and `vibra-syntax` (the reader's VIBON data, for the two documents it
  owns), plus `serde_json` for the JSON form and `getrandom` for the operating
  system's random source. Its modules are `provider`, `operation`, `world`,
  `random`, `system`, and `event`.
- **The traits.** `HostProvider` has one method for each row, and each answers
  `Done<T, E>`, an `Err(Failure::Error(e))` of the row's closed enum or
  `Err(Failure::MemoryExhausted)`, which is the host event of "Activations and
  memory" and never a registry error. `HostValues` is the value-access trait:
  `read_text`, `read_bytes`, `read_path`, and `read_u64` borrow a backend value, and
  `build_void`, `build_text`, `build_bytes`, `build_i64`, `build_u64`,
  `build_array`, `build_enum`, and `build_record` take sealed children, so a
  backend builds bottom-up as the seven builders of the Wasm boundary require.
  `HostCall::read` turns a symbol and its arguments into one typed call (rejecting
  an unknown symbol, a wrong arity, and an unreadable value), `perform` runs it on a
  provider, `build_reply` builds the `(result t e)` value, and
  `AuditEvent::completed` builds the event from the call and the reply, so both
  backends build the event with this one function. A reference implementation of
  `HostValues` over a plain tree is in the host tests.
- **The providers.** `WorldProvider::new(&HostWorld)` makes a fresh copy of a
  decoded world: a path-resolving filesystem (`/` separators, empty and `.`
  skipped, `..` to the parent, the root its own parent, no links), standard input
  with a position that ends after any call, the stepped monotonic clock (`None`
  once the `u64` range is spent), a constant wall clock, and SplitMix64.
  `SystemProvider` is the one module of the execution crates that reads ambient
  state; it takes the program arguments at construction and counts its monotonic
  clock from the instant it was made.
- **The decoder.** `HostWorld::decode(source_id, text)` reports every violation once
  as `@data.invalid-shape` or `@data.invalid-value` at the document: a non-record,
  an unknown or repeated field, a wrong value kind, an odd or repeated dict key (the
  generic data reader), a wrong format atom, a mis-suffixed or out-of-range integer,
  a NUL in a path, and a path that is both a file and a directory. The vectors for
  seeds `0` and `1234567` are pinned.
- **The event codec.** `AuditEvent` is `Host(HostEvent)` or `Opaque(OpaqueEvent)`.
  `to_vibon` writes the canonical one-line record, `decode` and `from_data` read
  one (an unknown kind keeps its fields as text; an unknown or repeated field of
  `@host` is `@data.invalid-shape`), `to_json` and `from_json` are the CLI object
  `{kind, op, args, result}` with `kind` as `@host`, and `trace_to_vibon` and
  `decode_trace` are the `@audit-trace.v1` record.
- **The manifest.** `stdlib/manifest.vibon` and `vibra-types` gain the `host` list
  between `compiler` and `native` and the package `0.3.0` (every consumer of the
  string was updated in place). At load, each listed symbol is a registry row, listed
  once, and bound by a module; each `external: @host` member of a `deffect` names a
  listed row, whose owner is `@<module>.<deffect>` and whose signature, in the
  registry's spelling, is the one written. The reader already rejects `@host` outside
  a `deffect` and a missing `symbol:`, so the loader keeps no second rule for them.
  The list is empty until Step 16b adds `@std.io`.
- **The corpus input.** `host-world` is an input of `workspace-run` and
  `workspace-test` cases only. `Case::host_world` decodes it each time it is called
  (a fresh copy), and both handlers decode it before anything else runs: a malformed
  document is its data diagnostics at the document and fails its case. The decoded
  world is not yet given to a backend; Step 16b threads a copy to each.
- **The ambient-read test.** `ambient_reads_m4_step16` scans the source of every
  crate except `vibra-host`, `vibra-cli`, `vibra-workspace`, and `vibra-conformance`
  for the filesystem, environment, clock, random, and process-stream APIs, outside
  comments and unit-test modules.
- **The architecture table** names `vibra-host`. Its own row is `vibra-diagnostics`,
  `vibra-ir`, and `vibra-syntax`. `vibra-interp` and `vibra-wasm-run` may depend on
  it, and so does the harness `vibra-conformance`, which decodes a case's world and
  will hand each backend a provider; a test holds the dependents to those three.
  The emitter and the language phases do not.

### Rulings on text the specification was silent about

Written into the runtime and conformance chapters and recorded as ledger D32.2.

- A path a world lists as a file and also needs as a directory (under
  `directories`, or as an ancestor of another listed path), the root as a file, and
  two spellings of one resolved path are `@data.invalid-value`, because every
  ancestor of a listed path exists and a path is one entry.
- An integer of a world carries exactly its suffix and fits its type; a missing
  field of `clock` or `random` takes its own default.
- A `host-world` input is a relative `.vibon` file of the case directory, outside
  the confined tree.

### Cases

None added, re-expected, deleted, or downgraded. The host tests are synthetic.

### Evidence

- Corpus at the head: 83 reader, 283 static, 143 interpreter, and 16 tooling cases,
  525 passed, 0 failed, 0 unavailable; the interpreter backend passes 135 and the
  Wasm backend matches 135, 0 not lowered (the same as the base).
- Host tests: `host_registry_snapshot_m4_step16` in `vibra-ir` (10);
  `host_world_m4_step16` (31), `audit_events_m4_step16` (14), and
  `system_provider_m4_step16` (8) in `vibra-host`; eight unit tests of the `host`
  list in `vibra-types`; `host_world_corpus_m4_step16` (6),
  `host_documents_fmt_m4_step16` (3), `ambient_reads_m4_step16` (4), and the new
  dependent test of `architecture_boundary` in `vibra-conformance`.
- Re-expected: the inventory of specification examples
  (`docs/roadmap/milestone-1/syntax-examples.tsv`) follows the line shifts of the
  edited chapters, 14 rows, only in their line numbers and the span of the two
  sections that grew.

### Left for Step 16b

Everything that makes an operation reachable: `external: @host` accepted in the
checker, the typed IR call target, `@std.io` and the manifest's first symbols, the
`vibra_v1` imports and the seven builders in the emitter, the interpreter's and the
runner's `HostValues`, the world and the system provider given to `run`, `test`, and
the workspace handlers (a fresh copy for each test and each backend), the event
appended after a completed call, and the `auditTrace` objects of the CLI.
