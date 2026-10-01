# M3 exit evidence

Captured 2026-10-01 for [Step 16](16-exit.md). This step claims no new
behavior. It records the demo, the corpus gate, the map from the conformance
chapter's Stage 3B coverage clauses to case IDs, the M2 deferral sweep, and
the forms M3 reassigns. Stage 3A is recorded in
[its own report](stage-3a-evidence.md). Its own PR adds the demo with its
process test and corrects one diagnostic message that named a step which had
already landed.

## Integration

| Step | PR and merge | CI run on the merged head |
| --- | --- | --- |
| 9 | [#320](https://github.com/nahharris/vibra/pull/320), `7175606` | [36687852322](https://github.com/nahharris/vibra/actions/runs/36687852322) |
| 10 | [#321](https://github.com/nahharris/vibra/pull/321), `191e71d` | [36714268888](https://github.com/nahharris/vibra/actions/runs/36714268888) |
| 11 | [#322](https://github.com/nahharris/vibra/pull/322), `66369c4` | [36800924519](https://github.com/nahharris/vibra/actions/runs/36800924519) |
| 11b | [#323](https://github.com/nahharris/vibra/pull/323), `ec9ff93` | [36848725235](https://github.com/nahharris/vibra/actions/runs/36848725235) |
| 11c | [#324](https://github.com/nahharris/vibra/pull/324), `587f000` | [36854095399](https://github.com/nahharris/vibra/actions/runs/36854095399) |
| 11d | [#325](https://github.com/nahharris/vibra/pull/325), `2ec1749` | [36856283208](https://github.com/nahharris/vibra/actions/runs/36856283208) |
| 12 | [#326](https://github.com/nahharris/vibra/pull/326), `2df633d` | [36857304147](https://github.com/nahharris/vibra/actions/runs/36857304147) |
| 13 | [#327](https://github.com/nahharris/vibra/pull/327), `76b1b90` | [36858893457](https://github.com/nahharris/vibra/actions/runs/36858893457) |
| 14a | [#328](https://github.com/nahharris/vibra/pull/328), `aa6090a` | [36860325486](https://github.com/nahharris/vibra/actions/runs/36860325486) |
| 14b | [#329](https://github.com/nahharris/vibra/pull/329), `7b0252d` | [36861193572](https://github.com/nahharris/vibra/actions/runs/36861193572) |
| 14c | [#330](https://github.com/nahharris/vibra/pull/330), `ebfd777` | [36861882988](https://github.com/nahharris/vibra/actions/runs/36861882988) |
| 15a | [#331](https://github.com/nahharris/vibra/pull/331), `3025969` | [36863682560](https://github.com/nahharris/vibra/actions/runs/36863682560) |
| 15b | [#332](https://github.com/nahharris/vibra/pull/332), `442114f` | [36865607751](https://github.com/nahharris/vibra/actions/runs/36865607751) |

Each step merged only after all five jobs passed on its head commit: Ubuntu,
Windows, macOS, the reader corpus, and the archive boundary. The runs above
repeat them on the merged `m3` head.

## Demo

[`examples/m3-catalog`](../../../examples/m3-catalog) is a two-target project
that uses only the public standard library and no compiler-private form.

- The `catalog` library, `src/catalog/stock.vib`:
  - **An interface with a default member, implemented by two nominal types.**
    `priced` declares the abstract `cents` and the default `above`; the
    records `part` and `service` each implement `cents`.
  - **A generic function bounded by that interface.** `total` sums
    `priced.cents` over `(array t)` with `where: (t priced)`.
  - **A map keyed by a user type through its own `ordered`.** The wrapper
    `sku` orders from the highest number down, so `(map sku u64)` lists in
    that order rather than the order of the number it wraps.
  - **A conversion selected by a written destination.** `sku` implements
    `(from u32)`, and `shelf` calls `from.convert` with only its result type
    naming the destination.
  - **A generic collection with an `iter` implementation.** `(stack t)`
    implements `(iter t)` from its top, and `costly-names` runs
    `iter.filter`, `iter.map`, and `iter.collect` over a `(stack part)`.
  - **A nominal error type.** `reserve` returns
    `(result u64 stock-error)`, where `stock-error` is the union of the
    records `unknown-sku` and `short-stock`.
- The `app` binary's entry returns `(result void stock.stock-error)` and
  propagates with `try`.
- `tests/stock.vib` has seven tests, one per bullet above plus each error
  member.

From a clean checkout, with no network, in `examples/m3-catalog`:

```bash
vibra check
```

`check accepted`, exit 0.

```bash
vibra test
```

`test suite passed: 7 test(s)`, exit 0.

```bash
vibra --format json run src/app
```

Exit 0, `@command.ok`, with `programResult`
`(record type: (record type: @std.result.result arguments: (array @void
@catalog.stock.stock-error)) value: (record kind: @enum type:
@std.result.result variant: @ok))`.

The three demo sources are in canonical format: `vibra fmt PATH` prints each
unchanged.

The public `query` command belongs to M6, so the `@index.v1` projection is
read through the library function the corpus `index` operation uses
(ledger D23.1). For the demo it lists five implementation blocks: `sku` under
`@std.core.ordered` and under `(from u32)`, `part` and `service` under
`@catalog.stock.priced`, and `(stack t)` under `(iter t)`. Two runs over the
same snapshot give byte-identical documents. A position query at
`(from.convert number)` reports a `@contract` application dispatched
statically to `catalog.stock.sku` by its destination.

The process test `vibra-cli/tests/process_m3_demo` repeats all of this with
the actual binary on every CI run.

## Corpus gate

The full corpus reports 319 passed, 0 failed, and 0 unavailable, by profile:

| Profile | Passed | Failed | Unavailable |
| --- | --- | --- | --- |
| reader-v1 | 76 | 0 | 0 |
| static-v1 | 170 | 0 | 0 |
| interpreter-v1 | 65 | 0 | 0 |
| tooling-v1 | 8 | 0 | 0 |

The workspace suite reports 636 passed and 0 failed.

### Stage 3B inventory rows

Each Stage 3B row of the [inventory](supported-surface.md), and each M2 or
Stage 3A row that Stage 3B widened, has a positive and a negative case.

| AST variant | Positive | Negative |
| --- | --- | --- |
| `Declaration::Defint` | `V1-RUNTIME-interface-dispatch`, `V1-RUNTIME-workspace-test-interfaces` | `V1-TYPE-INTERFACE-undispatchable`, `V1-TYPE-INTERFACE-default-override` |
| `TypeMember::Implementation` | `V1-RUNTIME-interface-dispatch`, `V1-TYPE-INTERFACE-generic-targets` | `V1-TYPE-INTERFACE-missing-abstract`, `V1-TYPE-INTERFACE-extra-member`, `V1-TYPE-INTERFACE-member-signature`, `V1-TYPE-INTERFACE-redundant-implementation`, `V1-TYPE-INTERFACE-overlapping`, `V1-TYPE-INTERFACE-deftype-target-type`, `V1-TYPE-INTERFACE-defint-target-interface`, `V1-TYPE-INTERFACE-anonymous-target` |
| `TypeMember::Method` as a contract member | `V1-RUNTIME-interface-dispatch`, `V1-RUNTIME-conversion` | `V1-TYPE-INTERFACE-default-override`, `V1-TYPE-INTERFACE-undispatchable` |
| `TypeExpr::Name` naming `any` or an interface | `V1-RUNTIME-interface-values`, `V1-TYPE-GENERIC-stage-3b-types` | `V1-TYPE-INTERFACE-value-restrictions`, `V1-TYPE-INTERFACE-value-positions` |
| `TypeExpr::Applied` naming a generic interface | `V1-RUNTIME-iter-next`, `V1-RUNTIME-iter-defaults` | `V1-TYPE-INTERFACE-iter-rejections` |
| `TypeExpr::Map` with a user or generic key | `V1-RUNTIME-user-map-keys`, `V1-RUNTIME-library-map` | `V1-TYPE-INTERFACE-user-key-unordered`, `V1-TYPE-INTERFACE-key-generic-unbounded` |
| `Attribute::Where` with an interface bound | `V1-RUNTIME-workspace-test-interfaces`, `V1-RUNTIME-key-contracts`, `V1-RUNTIME-bounded-deftype-lambda` | `V1-TYPE-INTERFACE-unsatisfied-bound`, `V1-TYPE-INTERFACE-unimplemented-receiver`, `V1-TYPE-INTERFACE-bounded-deftype-unsatisfied`, `V1-TYPE-INTERFACE-key-closed-registry`, `V1-TYPE-GENERIC-interface-bound` |
| `ExpressionKind::As` to an interface | `V1-RUNTIME-interface-values` | `V1-TYPE-CONVERT-interface-widening-rejections` |
| `ExpressionKind::Application` of a destination-dispatched member | `V1-RUNTIME-conversion` | `V1-TYPE-CONVERT-destination-rejections`, `V1-TYPE-CONVERT-redundant-conversion` |

### What still reports `@tool.unavailable`

No Stage 3B form does, apart from the three reassigned in the next section.
The remaining reports are M4's, a single-source limit, or a trust boundary:

| Form | Owner | Case |
| --- | --- | --- |
| `deffect`, effect operations, and nonempty effect rows, including an effectful `iter` default callback | M4 | `V1-PROJECT-workspace-check-nominal-availability`, `V1-EFFECT-availability-function-type`, `V1-SRC-CALLS-functions-effects`, `V1-TYPE-INTERFACE-iter-default-rejections`, `V1-TOOL-index-unavailable` |
| `@host` externals | M4 | `V1-EFFECT-availability-host-member` |
| Ordinary dependency delivery | M5 | `V1-PROJECT-graph-static-dependency` |
| A source with no value or function; an import in the single-source path | not a language form: the workspace path checks both | `V1-TYPE-NAMES-binding-empty-module`, `V1-TYPE-INFER-bootstrap-extra-import` |
| `external:`, `native:`, and `role:` outside the embedded standard library | a trust boundary, by design | `V1-RUNTIME-external-untrusted`, `V1-TYPE-NAMES-library-attributes` |

## Reassigned to later milestones

The exit gate lets an M3 form be reassigned to a named later milestone
instead of implemented. These are reassigned. Each still reports
`@tool.unavailable` at its form, never a wrong answer.

| Form | Reassigned to | Why not M3 |
| --- | --- | --- |
| An abstract contract member with its own generic parameters | M4 | A default member with its own generics is checked as one function (Step 14b, `iter.map`). An abstract one needs each implementation instantiated per call, which the typed IR does not yet express; M4 lowers the same IR to Wasm and must settle that shape once for both backends. |
| Labelled operands and written `types:` arguments on a contract member call | M4 | Same call path as the row above. |
| A map variadic tail on a contract member | M4 | Same call path; an array tail is supported (`V1-RUNTIME-conversion`, `factory.of`). |
| `from` and `try-from` between float types and between floats and integers | M4 | The conversion registry covers the 56 integer pairs (D21.1). Float conversions need a rounding and range contract in the runtime chapter that both backends must share. |
| The public `query` command | M6 | It is M6's `query <subject>` deliverable. M3 delivers the records and the position envelope as library projections with corpus operations (D23.1, D23.2). |
| G21: the command result for an entry that returns `err` | M4 | It needs a tooling-chapter decision on a result atom and exit code. M4 introduces host failures at the entry and must make the same decision for them. |

`hashable` is not reassigned: v1 has no `hashable`, because map keys need only
`ordered` (D18.2), so the roadmap's law clause covers `equatable`, `ordered`,
and `iter` (Step 14c).

Two findings are recorded for a later tooling pass, neither an M3 form:

- The formatter lays out a `deftype`, `defint`, or `defn` that carries an
  attribute with each label and value on its own line, and can leave a closing
  parenthesis alone on a line. The demo is stored in that canonical format.
- A position query reports a `pattern` fact for the arm pattern only, an
  expected type only where the source writes a primitive or function type,
  and no separate fact for an interface bound (D23.2).

## Conformance-chapter coverage

The Stage 3B clauses of `docs/spec/07-diagnostics-and-conformance.md`:

| Clause | Cases |
| --- | --- |
| Interface coverage: abstract and default contract members; `@type.default-override` and `@type.missing-abstract-member` | `V1-RUNTIME-interface-dispatch`, `V1-RUNTIME-workspace-test-interfaces`, `V1-TYPE-INTERFACE-default-override`, `V1-TYPE-INTERFACE-missing-abstract` |
| Interface coverage: the `iter` contract and default-method semantics; closed registry conformance for `(array t)`, `(map k v)`, `str`, and `(option t)`; explicit `impl (iter item)` on the four adapter types | `V1-RUNTIME-iter-next`, `V1-RUNTIME-iter-defaults`, `V1-TYPE-INTERFACE-iter-rejections`, `V1-RUNTIME-workspace-test-contract-laws` |
| Interface coverage: pure `iter` default methods with `effects: ()` callbacks only | `V1-TYPE-INTERFACE-iter-default-rejections` |
| Interface coverage: effectful walks as tail-recursive functions over `iter.next` | The pure walk is `V1-RUNTIME-iter-next`; an effectful one needs M4's effect rows |
| Map coverage over user and generic keys | `V1-RUNTIME-user-map-keys`, `V1-RUNTIME-library-map`, `V1-RUNTIME-key-contracts`, `V1-TYPE-INTERFACE-user-key-unordered`, `V1-TYPE-INTERFACE-key-generic-unbounded`, `V1-TYPE-INTERFACE-key-closed-registry` |
| Unification: a bound does not make two members or two targets disjoint | `V1-TYPE-NOMINAL-union-member-overlap`, `V1-TYPE-INTERFACE-overlapping` |
| Widening: concrete-to-interface at every written boundary | `V1-RUNTIME-interface-values`: a parameter, a result, an `as` ascription, a record field, and a `def` annotation |
| No chaining: a union member where only its union implements the interface; an atom singleton where `any` is expected | `V1-TYPE-CONVERT-interface-widening-rejections` |
| Conversion: `from` on a destination `deftype`; `try-from` returning `conversion-error`; selection through a parameter type, a result type, and `as` | `V1-RUNTIME-conversion` |
| Conversion: a bare call rejected with `@type.ambiguous-destination`; an unsuffixed literal rejected with `@type.ambiguous-implementation` | `V1-TYPE-CONVERT-destination-rejections` |
| Conversion: one receiver implementing `(from i16)` and `(from i8)`; two block identities and two member identities in index output; completeness per block | `V1-RUNTIME-conversion`, `V1-TOOL-index-records`, `V1-TYPE-INTERFACE-generic-targets`, `V1-TYPE-INTERFACE-missing-abstract` |
| Conversion: a `from`/`try-from` pair on one source, and `(from t)` with `(try-from i32)`, rejected with `@type.redundant-conversion`; `(from t)` and `(from i32)` rejected with `@type.overlapping-implementation` | `V1-TYPE-CONVERT-redundant-conversion`, `V1-TYPE-INTERFACE-overlapping` |
| Contract members rejected with `@type.undispatchable-contract-member`; a member naming `self` in its result and a variadic tail accepted | `V1-TYPE-INTERFACE-undispatchable`, `V1-RUNTIME-conversion` (`factory.of`) |
| Destination dispatch beyond conversion: a `(defn empty () self)` factory | `V1-RUNTIME-conversion`, `V1-TYPE-CONVERT-destination-rejections` |
| Index records and position metadata | `V1-TOOL-index-records`, `V1-TOOL-index-unavailable`, `V1-TOOL-workspace-position-query`, `V1-TOOL-workspace-position-boundaries`, `V1-TOOL-workspace-position-types` |

## Roadmap exit clauses

| Clause | Evidence |
| --- | --- |
| The type-system chapter has focused positive and negative cases | The two inventory tables and the two coverage tables, here and in the [Stage 3A report](stage-3a-evidence.md) |
| Ambiguous inference has a stable atom diagnostic | `@type.ambiguous-inference`: `V1-TYPE-GENERIC-ambiguous`, `V1-TYPE-INFER-binding-ambiguity` |
| Illegal shadowing | `@name.redeclaration`: `V1-TYPE-NAMES-binding-shadow`, `V1-TYPE-NAMES-binding-local-shadow`, `V1-TYPE-CONTROL-pattern-redeclaration` |
| Non-exhaustive matches | `@pattern.non-exhaustive`: `V1-TYPE-CONTROL-match-non-exhaustive`, `V1-TYPE-CONVERT-narrowing-rejections` |
| Invalid implementation placement | `@name.wrong-entity-kind`: `V1-TYPE-INTERFACE-deftype-target-type`, `V1-TYPE-INTERFACE-defint-target-interface`, `V1-TYPE-INTERFACE-anonymous-target`; `@type.redundant-implementation`: `V1-TYPE-INTERFACE-redundant-implementation` |
| Silently ignored fallible values | `@type.unhandled-fallible`: `V1-TYPE-CONTROL-unhandled-fallible`, with the accepted discards in `V1-TYPE-CONTROL-fallible-discards` |
| Interpreter behavior is independent of map/hash iteration | The interpreter and the typed IR use no hash collection; a map is its sorted entry array (D19.4). `V1-RUNTIME-map-order` builds one map in two insertion orders, and `V1-RUNTIME-user-map-keys` and `V1-RUNTIME-library-map` order by a user `compare` |
| Index and context output use canonical resolved identities | `V1-TOOL-index-records` (declaration, implementation, and reference identities), `V1-TOOL-workspace-position-types` (declared type paths and dispatch receivers), and `index::tests` (two source orders, identical bytes) |
| Every M2 deferral is implemented or reassigned | The sweep below |
| No M3-owned form reports `@tool.unavailable` | The two sections above: what still reports it, and what is reassigned |

## M2 deferral sweep

| M2 row | Disposition | Evidence |
| --- | --- | --- |
| C1.3 — generics, nominal collections, interfaces, conversion | implemented, Steps 2–4 and 11–13 | `V1-RUNTIME-generic-functions`, `V1-RUNTIME-nominal-record`, `V1-RUNTIME-lookups`, `V1-RUNTIME-interface-dispatch`, `V1-RUNTIME-conversion` |
| C1.5 — `match`, `try`, `option`, `result`, refutable patterns | implemented, Steps 4, 5, and 7 | `V1-TYPE-CONTROL-match-non-exhaustive`, `V1-TYPE-CONTROL-pattern-refutable-binding`, `V1-RUNTIME-try-propagation`, `V1-TYPE-CONTROL-unhandled-fallible` |
| C1.6 — variadic array and map operands | implemented, Step 4 | `V1-RUNTIME-variadics`, `V1-SRC-CALLS-variadic-map-application` |
| C1.7 — `as`, singleton widening, narrowing | implemented, Steps 6 and 12 | `V1-TYPE-CONVERT-widening-boundaries`, `V1-TYPE-CONVERT-narrowing-rejections`, `V1-RUNTIME-interface-values` |
| C5.2 — constructor and destructuring patterns | implemented, Step 5 | `V1-RUNTIME-match-patterns`, `V1-RUNTIME-workspace-test-patterns` |
| C6.2 — `types:` generic arguments | implemented, Step 3 | `V1-TYPE-GENERIC-type-arguments`, `V1-TOOL-format-types-order` |
| Unavailable `integer.*` compiler symbols | implemented, Step 8 | `V1-RUNTIME-registry-integers`, `V1-RUNTIME-registry-floats` |
| Every AST variant the M2 inventory marked deferred to M3 | implemented | One row each in the [M3 inventory](supported-surface.md), enforced by `m3_contract_inventory` |
| `deffect`, nonempty effects, `@host` | still M4, as M2 assigned them | The unavailable table above |

## Planning gaps

G1–G20 were closed by the steps the [README](README.md) names. G21 is
reassigned to M4 above.
