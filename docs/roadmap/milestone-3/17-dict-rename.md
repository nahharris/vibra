# Step 17 — the associative type is `dict`

Prerequisite: Step 16 merged. A mechanical rename; it claims no new behavior.

## Read before editing

- [Types](../../spec/02-type-system.md): **Language core and standard
  library**, **Iteration**.
- [Decision ledger](decision-ledger.md) row D24.1.

## Scope

The associative collection type is spelled `dict` everywhere, so that it
shares no spelling with the `iter` default member `map`, which transforms
items (D24.1). `iter.map` keeps its name.

| Before | After |
| --- | --- |
| the type `(map k v)` | `(dict k v)` |
| `map.of`, `map.entries` | `dict.of`, `dict.entries` |
| the role `@map` | `@dict` |
| the VIBON data form `(map …)` | `(dict …)` |
| the value and type encodings `kind: @map`, `type: @map` | `kind: @dict`, `type: @dict` |
| `@type.invalid-map-key` | `@type.invalid-dict-key` |
| the query type kind `map` | `dict` |

`dict` takes `map`'s place among the reserved type heads and builtin names, so
`map` is an ordinary name again. Nothing accepts the old spellings: Vibra is
pre-alpha, and the old contracts are replaced, not kept.

The renamed corpus cases are `V1-PROJECT-odd-dict`, `V1-RUNTIME-dict-order`,
`V1-RUNTIME-library-dict`, `V1-RUNTIME-user-dict-keys`,
`V1-SRC-CALLS-variadic-dict-application`, `V1-SRC-CALLS-variadic-dict-type`,
and `V1-TYPE-NOMINAL-dict-keys`.

## Test matrix

- The whole corpus and workspace suite pass with the same counts as before the
  rename. Expected spans move by one byte per renamed token before them, and
  the index and query snapshots change only by the renamed spellings and the
  workspace revision.
- `V1-TYPE-NAMES-resolve-reserved-value` rejects a value named `dict`, and
  `V1-SRC-DECL-native-forms` accepts a method named `dict`.

## Done

No source, specification chapter, schema, or case spells the associative type
`map`, and validation passes.
