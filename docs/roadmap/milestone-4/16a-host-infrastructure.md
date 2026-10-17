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
