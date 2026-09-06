# Step 4 — declarations, imports, and identity

Requires Step 3 and C3/C4/C11/C12. Read projects **Modules and imports** and
**Packages and targets**, types **Namespaces and resolution** and **Functions
as values**, source **Declarations**, and diagnostics **Recovery**:
[projects](../../spec/04-programs-and-packages.md),
[types](../../spec/02-type-system.md), [source](../../spec/01-source-language.md),
[diagnostics](../../spec/07-diagnostics-and-conformance.md).

## Implementation sequence

1. Add `vibra-resolve` with graph input, declaration IDs, scope records, and
   resolved AST origins. Collect headers before resolving bodies so same-module
   forward and mutual function references do not depend on source order.
2. Use one unit-rooted atom walker for import and project entry. Walk directories
   until the module leaf, then declaration components. Classify the found entity
   before applying slot kind/visibility rules.
3. Imports bind only explicit module aliases. Build/import-cycle-check the graph;
   expose only public declarations through aliases. An entry can reference its
   own target's private module-level `defn` without creating an import edge.
4. Enforce flat top-level spelling collisions across namespaces and reserved
   value spellings. Keep namespace selection separate from canonical identity.
   Record unavailable later-v1 declarations without treating them as calls.
5. Resolve supported body references into IDs. Establish reusable lexical-scope
   operations for Step 6; no discard ID, declaration, or reference is created.
6. Produce canonical resolved snapshots through the static handler. Resolution
   alone does not validate entry signature or claim runnable code.

| Positive | Negative / boundary |
| --- | --- |
| Local and imported public functions; forward references | Unknown unit/module/member; import of a declaration instead of a module |
| Entry naming a private function with a non-`main` name | Entry outside own target; entry naming module/value/type; private import access |
| Alias spelling differs from module name | Duplicate aliases/declarations, cross-namespace collision, import cycles |
| Identical basenames under distinct target roots | Relative/string/glob imports; root guessing; directory index fallback |
| Expression atoms remain literal values | Dotted string coincidence must not produce a resolved identity |
| Valid sibling facts beside a damaged declaration | Stable ordering and origins; no cascaded invented identities |

The independent corpus names each boundary explicitly: `V1-TYPE-NAMES-resolve-unknown-path`,
`V1-TYPE-NAMES-resolve-declaration-as-module`, `V1-PROJECT-resolve-entry-outside-target`,
`V1-PROJECT-resolve-entry-module`, `V1-PROJECT-resolve-entry-missing-member`,
`V1-TYPE-NAMES-resolve-cross-namespace`, `V1-TYPE-NAMES-resolve-duplicate-alias`,
`V1-TYPE-NAMES-resolve-missing-member`, `V1-TYPE-NAMES-resolve-imported-module-value`,
and `V1-TYPE-NAMES-resolve-reserved-value`
cover path, entity-kind, entry, and collision diagnostics. `V1-TYPE-NAMES-resolve-alias-differs`,
`V1-PROJECT-resolve-identical-basenames`, `V1-TYPE-NAMES-resolve-literals`, and
`V1-TYPE-NAMES-resolve-index-fallback` cover alias/root identity and literal/index boundaries.
`V1-TYPE-NAMES-resolve-relative-import`, `V1-TYPE-NAMES-resolve-string-import`,
`V1-TYPE-NAMES-resolve-glob-import`, and `V1-TYPE-NAMES-resolve-root-guessing` cover
forbidden import forms. `V1-TYPE-NAMES-resolve-cycle-three`,
`V1-TYPE-NAMES-resolve-visible-shadow`, and `V1-TYPE-NAMES-resolve-damaged-sibling`
cover longer-cycle provenance, visible-binding shadowing, and sibling recovery.

Keep `@module.unknown-path`, `@module.import-cycle`,
`@name.unknown-symbol`, `@name.wrong-entity-kind`, `@name.private-access`,
`@name.redeclaration`, `@name.member-collision`, and
`@project.entry-outside-target` distinct. Top-level declarations, import
aliases, and lexical bindings use `@name.redeclaration`; members in one flat
owner namespace use `@name.member-collision`; import back edges use
`@module.import-cycle`. Each diagnostic is anchored at the referring or
introducing span and carries a related declaration or edge span when one
exists. No types, runtime, effect resolution, nested implementation
semantics, or complete M3 index is included.

The resolver consumes an explicit immutable graph value owned by
`vibra-resolve`. Workspace and conformance adapters construct that value from
the Step 3 snapshot and project record; the resolver performs no filesystem,
dependency, lock, cache, network, or ambient project discovery. A declaration
identity contains package name and version, unit, module segments, owner path,
and entity kind. It never uses a source-order vector index as identity.

The static resolved artifact is canonical VIBON data with format atom
`@resolved.v1`. It records the package identity, sorted modules, declaration
IDs with source IDs/spans/visibility, imports, and body reference edges. Exact
module bytes remain in the Step 3 source graph artifact; the resolved artifact
records each module's source ID and does not re-read or normalize those bytes.
Expected artifacts are compared as canonical text snapshots.

Run [common validation](validation.md) and
`cargo test --locked --offline -p vibra-resolve -p vibra-workspace`.
Done requires `V1-TYPE-NAMES-*` / `V1-PROJECT-*` corpus identities and host
assertions that import/entry share the same walker while applying different
visibility rules.
