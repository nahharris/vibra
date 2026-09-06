# Step 3 — project discovery and source graph

Requires Step 2 and C2/C3/C8. Read projects **Packages and targets**,
**Modules and imports**, **Dependencies and lock**, plus tooling
**One workspace engine** in [projects](../../spec/04-programs-and-packages.md)
and [tooling](../../spec/05-tooling.md).

## Implementation sequence

1. Add proposed workspace `discovery`, `snapshot`, and `source_graph` modules.
   Discover only `project.vibon` using C2's documented ancestor/root rules.
2. Canonicalize target roots and validate containment and pairwise disjointness
   before walking sources. Apply the project discovery/snapshot contract in
   `04-programs-and-packages.md`: exact nearest-marker search, component-aware
   confinement, explicit link/cycle/alias handling, and stable path/I/O
   diagnostics; string-prefix path checks are insufficient.
3. Enumerate modules deterministically. Validate kebab path segments and
   `.vib` extension; detect `text.vib` plus `text/` before parsing any module.
   Data files are never source modules and no directory index is implicit.
4. Acquire immutable bytes/source IDs and build a unit/module trie. The later
   resolver receives this explicit graph, never an ambient filesystem handle.
5. Represent dependency declarations without resolving unsupported packages.
   Only the explicit reviewed bootstrap in C8 is admitted before M5. Report
   unsupported dependency checking; never ignore declared dependencies or
   consult a network/cache fallback.
6. Extend the real static handler to acquire all declared case inputs through
   the confined corpus loader and exercise graph construction. Source-graph
   manifests bind `project` exactly to `<tree>/project.vibon`; the handler uses
   an exact-marker load rooted at that tree and never performs ancestor or
   sibling discovery. Cases also compare a canonical graph snapshot.

| Accept / preserve | Reject / prove |
| --- | --- |
| Nested modules, multiple disjoint targets, legal root names | Equal/nested roots in both directions; sibling-prefix paths are distinct |
| Same module basename in different units | File/directory collision, invalid path segment, missing root |
| Nested discovery under a valid project | Missing project, legacy `project.vib`, wrong loader extension |
| Sorted traversal despite shuffled filesystem enumeration | Escaping relative/absolute paths and symlink/junction escapes per C3 |
| Exact bytes and source IDs on a repeated snapshot | No content sniffing, index module, extension search, or ambient dependency |
| Nested starts, marker selection, same-basename units, and stable graph artifacts | Outside/sibling project inputs, malformed nearest markers, unavailable I/O, invalid segments, dot roots, and alias claims hidden by canonical-file deduplication |
| In-root directory aliases are visited once | Cycles and escaping file/directory links; symlink-privileged fixtures are explicit conditional evidence |

Assert `@project.overlapping-target-roots` and
`@module.file-directory-collision` at their prescribed phase. For platform
fixtures needing symlink privileges, report unavailable fixture setup honestly
and require equivalent CI evidence; do not silently pass an unexercised attack.

The Step 3 implementation MUST also prove the discovery and filesystem
diagnostic contract: `@project.not-found`, `@project.invalid-target-root`,
`@project.io-error`, `@module.invalid-segment`, `@module.path-escape`, and
`@module.io-error` carry the documented empty/path or project-root spans;
malformed nearest projects do not fall back to ancestors; in-root links are
canonicalized with cycle/alias suppression; and layout diagnostics precede
source parsing. The graph retains dependency declarations and reports
`@tool.unavailable` for unsupported ordinary dependency delivery without
performing resolution, network, cache, or lock inspection.

Run [common validation](validation.md) and workspace tests. Independent
`V1-PROJECT-*` cases plus temporary-tree host tests must show layout rejection
precedes parsing malformed module contents. No declaration resolution yet.
