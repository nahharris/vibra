# `.vapp` executable format

A `.vapp` is a deterministic, stored (uncompressed) ZIP archive. Version 2 has
these entries:

- `package.json`: canonical compact JSON metadata and the SHA-256 inventory.
- `program.wasm`: the application module for runtime ABI `vibra-v1`.
- `source/`: the complete project, lockfile, and package-local `dep/` graph.

Metadata also records the sorted `compilation-flags` used for the build and
the sorted `selected-sources` physical-file inventory. These fields make the
compilation context inspectable and ensure different flag sets produce
different deterministic archive identities. Verification recompiles sources
with the recorded flags; runtime callers cannot override them.

Archive paths use `/`, are relative, and may contain only normal path
components. Entries are lexically ordered after `package.json`, use the ZIP
epoch timestamp, mode `0644`, and stored compression. Every entry except the
metadata itself must appear exactly once in the metadata inventory; extra and
duplicate entries are invalid.

Canonical metadata is UTF-8 compact JSON with a single trailing LF. Object
members use the schema/serializer order, and the `files` inventory is sorted
lexically. Verifiers reject non-canonical JSON even when it decodes to the same
values. Format 1 archives containing YAML `package.vibra` metadata are rejected
and must be rebuilt.

The runtime verifies the complete inventory before extracting sources. It then
loads the declared entry, emits its Wasm again, checks that it matches
`program.wasm`, and executes that self-contained module through the `vibra-v1`
boundary with the policy approvals supplied to `vibra run`. Execution reconstructs host
descriptors from `vibra.plan.v1`; it does not retain the freshly lowered IR.
Validated static wasm dependency bytes are embedded in that plan and their
original files remain covered by the `source/` inventory. Their digests affect
both the program fingerprint and deterministic archive bytes.

The metadata shape is normative in
[`package-manifest.schema.json`](../schemas/package-manifest.schema.json).
