# Step 2 — typed project data

Requires Step 1, especially C2/C11. Read projects **VIBON data documents**,
**Project file**, **Packages and targets**, and **Dependencies and lock** in
[the canonical chapter](../../spec/04-programs-and-packages.md).

## Implementation sequence

1. Add proposed `vibra-workspace::project` for a pure decoder accepting parsed
   `DataNode` plus source origin. Reuse M1's extension dispatch and data grammar.
   Add the crate to Cargo and the architecture test in the same PR.
2. Implement explicit closed record decoders for project, package, targets and
   dependency variants from C2's tables. Keep validation separate from graph
   lookup; a well-shaped unresolved entry atom can decode successfully.
3. Represent atom values, aliases, module/declaration/effect references with
   schema-selected roles and required kinds. Extend the generic adapter only
   as needed for nested fields; do not classify atoms by their text.
4. Preserve field/value spans, comments, and source identity. Feed schema field
   order to formatting; verify decode/format/decode equivalence.
5. Register a real static project-observation handler in the existing runner.
   This slice claims schema decoding only; dependency/source resolution remains
   explicitly unavailable. Update CLI-free library status and corpus CI scope.

## Required matrix

| Positive | Negative / boundary |
| --- | --- |
| Minimal bin with entry and empty effects; lib omitting both | Unknown, missing, duplicate, wrong-type fields at every record depth |
| Field permutations and comments | Unsupported version; malformed package name/version; wrong target kind |
| Every legal path/Git dependency shape, explicit/omitted library target | Invalid revision/URL/variant fields per C2; alias/target collision |
| Entry/effect reference syntax with nonexistent source | Decoder must not read disk or reject merely because source does not yet exist |
| Format/version/alias atoms remain values | Library entry/effects forbidden; binary required fields absent |
| Canonical round trip | `.vib` sent to data loader fails before parsing; Unicode diagnostic origins |

Use `@data.invalid-extension`, generic data codes, and project-specific codes
only as assigned in Step 1. Preserve M1 generic decoding behavior. No lock
generation, filesystem discovery, network, resolver, or project-edit command.

Run the common [validation](validation.md) sequence plus
`cargo test --locked --offline -p vibra-workspace`. Done includes independent
`V1-PROJECT-*` static cases, schema/order observations, and proof the decoder
works with no source graph supplied.
