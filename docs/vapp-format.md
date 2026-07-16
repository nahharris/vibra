# `.vapp` executable format

A `.vapp` is a deterministic, stored (uncompressed) ZIP archive. Version 1 has
these entries:

- `package.vibra`: canonical YAML metadata and the SHA-256 inventory.
- `program.wasm`: the application module for runtime ABI `vibra-v1`.
- `source/`: the complete project, lockfile, and package-local `dep/` graph.

Archive paths use `/`, are relative, and may contain only normal path
components. Entries are lexically ordered after `package.vibra`, use the ZIP
epoch timestamp, mode `0644`, and stored compression. Every entry except the
metadata itself must appear exactly once in the metadata inventory; extra and
duplicate entries are invalid.

The runtime verifies the complete inventory before extracting sources. It then
loads the declared entry, emits its Wasm again, checks that it matches
`program.wasm`, and executes that self-contained module through the `vibra-v1`
boundary with the policy approvals supplied to `vibra run`. Execution reconstructs host
descriptors from `vibra.plan.v1`; it does not retain the freshly lowered IR.

The metadata shape is normative in
[`package-manifest.schema.json`](../schemas/package-manifest.schema.json).
