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
   and explicit fact statuses. Preserve/version M1's published structural
   schema according to that decision; no uncoordinated field addition.

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
