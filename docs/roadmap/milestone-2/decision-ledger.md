# M2 Step 1 decision ledger

This is the checked-in record for Step 1. Each atomic row has one disposition,
one canonical source location, and one downstream proof obligation. A later
step may implement a row only after its disposition is `settled`; a `deferred`
row is intentionally outside M2. A row marked `new` is an M2 contract closed
by Step 1 and must be copied into the owning normative chapter in the same PR.

| ID | Disposition | Canonical anchor | Machine contract / diagnostic | Proof owner |
| --- | --- | --- | --- | --- |
| C1.1 | settled | `02-type-system.md` Model | Primitive names and fixed-width ranges; `@type.numeric-out-of-range` | Step 5 corpus |
| C1.2 | new | M2 supported-surface table | M2 supports literal/name/application/lambda/do/let/if plus direct binders; other AST forms are availability failures | Step 1 inventory test; Steps 5–7 corpus |
| C1.3 | deferred | `02-type-system.md` Generics | Generic `where:`/`types:` checking, nominal collections, interfaces, and conversion remain M3 | M3 plan |
| C1.4 | deferred | `03-effects.md` Static contract | Only empty effect ceilings are admitted in M2; nonempty rows and `@host` execution remain M4 | Step 8/12 negatives |
| C1.5 | deferred | `02-type-system.md` Control flow and failure | `match`, `try`, `option`, `result`, and exhaustive/refutable patterns remain M3 | Step 12 availability corpus |
| C1.6 | deferred | `01-source-language.md` Labels and applications | Variadic array/map operands remain M3; M2 labels use reviewed literal defaults | Step 7 negatives / M3 |
| C1.7 | deferred | `02-type-system.md` Type ascription and widening | `as`, singleton widening, and narrowing remain M3 unless a later spec amendment says otherwise | Step 1 inventory; M3 |
| C2.1 | new | `04-programs-and-packages.md` M2 `@project.v1` schema | Closed `@project.v1` field, type, order, role, and requiredness tables; unknown fields fail | Step 2 decoder |
| C2.2 | new | `04-programs-and-packages.md` Packages and targets | Binary requires `entry`/`effects`; library omits them; roots are canonical and disjoint | Steps 2–3 |
| C2.3 | settled | `04-programs-and-packages.md` VIBON data documents | Atom roles are selected by schema slot, never spelling | Step 2 schema tests |
| C3.1 | new | M2 supported-surface table | Discovery order is canonical path order; source graph is an explicit immutable input | Step 3 host tests |
| C3.2 | settled | `04-programs-and-packages.md` Modules and imports | File/dir collision and unknown path retain their distinct module diagnostics | Step 3/4 corpus |
| C3.3 | new | `03-source-language.md` Declarations | M2 checks all discovered source declarations in deterministic file/declaration order | Step 4/12 corpus |
| C3.4 | new | `07-diagnostics-and-conformance.md` Diagnostics are a language surface | Valid but deferred semantic forms emit `@tool.unavailable` at their form span | Step 1 registry; Step 12 corpus |
| C4.1 | settled | `07-diagnostics-and-conformance.md` canonical table | Existing type/name/project codes retain their fixed levels and spans | Every static step |
| C4.2 | new | `07-diagnostics-and-conformance.md` canonical table | `@tool.unavailable` is an error with no fix; it is distinct from malformed syntax and runner `unavailable` | Step 1 registry/schema test |
| C4.3 | new | M2 supported-surface table | Type/check errors prevent lowering and execution; no partial program runs | Steps 5–12 |
| C5.1 | new | `06-bindings.md` | `def` initialization is checked against written types and cycles are rejected before evaluation | Step 6 |
| C5.2 | deferred | `02-type-system.md` Control flow and failure | Constructor/destructuring patterns are not M2 bindings; direct local names and discards are | Step 6 / M3 |
| C6.1 | new | `01-source-language.md` Labels and applications | Label binding uses a resolved signature; canonical order is fixed, labelled defaults are literals | Step 7 |
| C6.2 | deferred | `02-type-system.md` Generics | `types:` has no accepted M2 generic arguments; valid generic syntax is unavailable | Step 7/12 |
| C7.1 | new | `06-runtime.md` M2 compiler intrinsic profile | Only the two listed pure text operations may execute; exact signatures and total Unicode semantics are checked | Step 8 |
| C7.2 | settled | `06-runtime.md` Evaluation | Pure execution has no host audit events and cannot read ambient state | Steps 5–14 |
| C8.1 | new | `04-programs-and-packages.md` Dependencies and lock | M2 bootstrap input is repository-owned and hash checked; no network/vendor sync | Step 8/11 |
| C8.2 | deferred | `04-programs-and-packages.md` Dependencies and lock | Ordinary pinned dependency delivery and lock generation remain M5 | Step 2/3 negatives |
| C9.1 | new | `04-programs-and-packages.md` Tests | Test identity is module plus literal name; duplicate names are module-local errors | Step 13 |
| C9.2 | new | M2 command table below | Pure assertion results are structured test failures, never traps or host events | Step 13 |
| C10.1 | new | `05-tooling.md` M2 command contract | Exact grammar, options, payload envelopes, stream routing, and exit mapping for `project init`, `fmt`, `check`, `run`, and `test` | Steps 11–13 |
| C10.2 | deferred | `05-tooling.md` V1 CLI | `lint`, `build`, `query`, `edit`, `mcp`, and dependency mutation are valid v1 names but unavailable in M2 | Step 12 availability cases |
| C11.1 | new | `05-tooling.md` Workspace queries | Semantic results are neutral envelopes until their owning type/workspace step supplies payloads | Steps 4–10 |
| C11.2 | new | `07-diagnostics-and-conformance.md` Conformance corpus | Multi-file observations carry source ID; same byte offsets never merge | Step 1 synthetic tests |
| C12.1 | new | M2 supported-surface table | Every M1 AST enum variant has one M2 disposition and an inventory coverage test | Step 1 test |
| C12.2 | new | `07-diagnostics-and-conformance.md` Conformance profiles | Profile capability and semantic `@tool.unavailable` are separate; neither may be silently skipped | All steps |

## Closed M2 pure compiler registry

Step 8 may admit only these operations after the provenance contract is
implemented. The table is intentionally small; adding a symbol is a
specification change, not an implementation detail.

| Symbol | Signature | Pure semantics |
| --- | --- | --- |
| `text.concat` | `str str -> str` | Concatenate Unicode scalar sequences |
| `text.length` | `str -> u64` | Count Unicode scalars, not UTF-8 bytes |

Both operations are total, preserve scalar order, emit no host event, and have
no ambient input. `integer.add-checked`, `integer.increment`, and
`integer.to-str` remain v1 source names but are unavailable M2 compiler
symbols: the first returns the nominal `result` type and the latter two need a
separate reviewed signature. M2 must report `@tool.unavailable` rather than
wrapping, trapping, or inventing a private arithmetic result. This table does
not authorize arbitrary intrinsic names or a host provider.

## M2 command admission

| Command | M2 status | Valid-but-deferred behavior |
| --- | --- | --- |
| `project init` | admitted Step 11 | — |
| `fmt` | admitted Step 11 | — |
| `check` | admitted Step 12 | — |
| `run` | admitted Step 12 | — |
| `test` | admitted Step 13 | — |
| `project inspect` | unavailable | `@tool.unavailable` |
| `project add/remove/sync` | unavailable | `@tool.unavailable`; no network or lock mutation |
| `lint`, `build`, `query`, `edit`, `mcp` | unavailable | `@tool.unavailable` |

The CLI must distinguish this diagnostic from an unknown option and from a
conformance handler's capability result. The exact flags, JSON envelope, stream
routing, and numeric exit mapping are frozen in the M2 command contract in
`05-tooling.md`.
