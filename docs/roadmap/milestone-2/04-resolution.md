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

Keep `@module.unknown-path`, `@name.unknown-symbol`,
`@name.wrong-entity-kind`, and `@project.entry-outside-target` distinct.
Use Step 1's codes for access/collision/cycle conditions. No types, runtime,
effect resolution, nested implementation semantics, or complete M3 index.

Run [common validation](validation.md) and
`cargo test --locked --offline -p vibra-resolve -p vibra-workspace`.
Done requires `V1-TYPE-NAMES-*` / `V1-PROJECT-*` corpus identities and host
assertions that import/entry share the same walker while applying different
visibility rules.
