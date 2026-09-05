# Step 7 — VIBON grammar and canonical data

Prerequisites: Steps 5–6; the data diagnostic and canonical-order decisions in
[implementation.md](implementation.md). Read programs-and-packages **VIBON data
documents**, source **Reader/Canonical format**, and diagnostics **Recovery**.

## Implementation sequence

1. Keep extension dispatch ahead of content parsing. Add a proposed `data`
   syntax module that validates the existing CST as exactly one data value.
   A valid source list must not become valid data merely because it is balanced.
2. Represent literal, record, array, tuple, and map data explicitly, retaining
   source-node references/spans. Recognize only these closed data heads; recurse
   through an explicit work stack where nesting could exhaust the host stack.
3. Validate record label/value pairs and unique fields; validate map pair arity;
   reject bare symbols, discards, declarations, and executable applications at
   every depth. A label is valid only in its grammar slot, not as a free value.
4. Separate generic data decoding from typed schema decoding. Generic decoding
   cannot know a project record's fields. Provide a way for a typed schema to
   supply field order and atom-slot roles without loading source or resolving
   identities. The actual `@project.v1` decoder remains Milestone 2 work.
5. Implement canonical output from the specified value/order contract. Preserve
   comments with their associated entries when reordering; retain complete
   original bytes on recovery. Test generic data and a small synthetic typed
   schema separately; do not claim project checking from a synthetic schema.
6. Update `ReaderV1Handler` to exercise real data validation/formatting through
   its existing project/data roles. Add assertions of decoded structure in host
   tests; snapshots alone cannot prove correct decoding.

## Required matrix

| Family | Positive | Negative/boundary |
| --- | --- | --- |
| Root | Each literal/container, comments around one value | Zero values, two values, trailing executable form |
| Containers | Empty record/array/tuple/map, deeply nested mixed values | Odd map tail, missing record value, duplicate field, non-label field |
| Closed grammar | Nested literals and atoms | Bare symbol, arbitrary application, `import`, `let`, source constructor such as `array.of`, discard in a value slot |
| Dispatch | `.vibon` through data loader | `.vib` through data loader and `.vibon` through source loader, even with plausible contents |
| Ordering | Equivalent inputs produce specified canonical order | Mixed key kinds and duplicate keys according to the resolved contract; no hash-order dependence |
| Typed adapter | Explicit field order and atom-value/reference roles | Unknown/missing/duplicate fields and bad version according to that schema; no reference lookup during decoding |
| Recovery | Useful following nodes remain inspectable | Invalid nested child, truncated list/string; exact original-byte formatting |

Use `V1-PROJECT` for data-rule cases and `V1-SRC-FMT` for formatting cases;
the case description identifies the specific VIBON clause. Do not create an
unregistered rule prefix. Run [validation](validation.md), including existing
loader-mismatch and quoted-leaf regressions. Record the resolved canonical-order
contract and new diagnostic mappings before marking Step 7 landed.
