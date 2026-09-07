# Step 10 — semantic source-position facts

Requires Step 9 and C11. Read tooling **One workspace engine**, **Workspace
queries**, **Schemas and errors**, types **Namespaces and resolution** and
**Inference and checking**: [tooling](../../spec/05-tooling.md),
[types](../../spec/02-type-system.md).

## Implementation sequence

1. Add proposed workspace `query` results composed from M1 `query_position`
   plus the snapshot's resolved IDs, scope records, expected/observed types,
   and supported function application contracts.
2. Keep structural selection unchanged: byte offsets, UTF-8 boundaries, EOF,
   recovery-node precedence and trivia semantics. Join by source/node IDs,
   never by textual spelling or an independent symbol index.
3. Return visible locals and import aliases at that position, canonical
   declaration candidates and primitive expectations. Discards return role
   and context only; no identity, references, or rename target.
4. Track semantic fact availability independently of intact syntax. A recovered
   neighbor cannot poison unrelated exact facts, and unknown types cannot
   masquerade as exact empty sets or successful inference.
5. Render through C11's schema adapter with snapshot revision, source identity,
   and explicit fact statuses. Keep M1's closed structural schema unchanged;
   publish the separate `workspace-position-query.v1` envelope, which embeds
   the structural result and owns the semantic fields.

## Frozen machine contract

The semantic envelope is `urn:vibra:schema:v1:workspace-position-query` and
uses one required `{status,value}` wrapper for each semantic field. Status is
independent for `role`, `context`, `identity`, `expectedType`, `observedType`,
`visibleLocals`, `visibleImports`, `declarationCandidates`, and `application`.
Exact empty collections are `exact` with `[]`; unavailable facts are
`unavailable` with `null`. The envelope retains M1's complete structural
result under `structural` and never adds semantic fields to the old schema.

`nodeId` is `<sourceId>#<start>-<end>`, identities use resolver canonical
spelling, and lexical binders use
`binder:<sourceId>:<binding-start>-<binding-end>` scoped to the enclosing
revision. Consumers join only by `(workspaceRevision, sourceId, nodeId)`.
Locals follow lexical introduction order, imports follow source order, and
declaration candidates sort by canonical identity. Types are structured
primitive/function objects; M2 applications expose only the `@function`
contract.

The immutable snapshot revision is
`sha256:<64 lowercase hexadecimal digits>`, hashing the binary domain bytes
`vibra-workspace-revision-v1` plus one `0x00` byte, length-prefixed exact
`project.vibon` bytes, then length-prefixed source IDs and exact bytes in
deterministic source-ID order. Absolute paths and ambient state are excluded;
queries never reread files or mix revisions. The fixed vector
`project.vibon = "project"`, `src/main.vib = "(defn f () str \\\"ok\\\")"`
has digest
`sha256:292c672b9ced8e6e02fbb3768f658fb52880ffd43b624c591b18e77fb083b996`.

| Positive | Negative / boundary |
| --- | --- |
| Expected primitive type in initializer, argument, branch, result | Ambiguous type, unresolved callee, damaged signature |
| Imported function identity and visible aliases | Private/invisible candidates; discard identity |
| Names before/inside/after lexical scopes | Same spelling in sibling scopes must not merge identities |
| Valid node next to recovered syntax | Trivia, EOF, zero-width recovery, out-of-range/mid-scalar offsets |
| Stable query from identical snapshot | Changed source gets new revision; no mixed old/new facts |

Run [common validation](validation.md), workspace query tests and schema
producer/consumer tests. Add `V1-TOOL-*` semantic query observations through the
real library handler using the Step 1 contract. Do not advertise full
`tooling-v1`, MCP, relations/index completeness, effect witnesses, semantic edit
commands, or public `query` CLI merely because this library slice works.
Done requires reviewed JSON snapshots plus a deliberately wrong semantic fact
that the corpus comparator rejects.
