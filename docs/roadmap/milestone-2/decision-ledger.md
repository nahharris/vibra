# M2 Step 1 decision ledger

This is the checked-in record for Step 1. Each atomic row has one disposition,
links to its canonical normative specification headings, and one downstream
proof obligation. A later step may implement a row only after its disposition
is `settled`; a `deferred` row is intentionally outside M2. A row marked `new`
is an M2 contract closed by Step 1 and must be copied into the owning normative
chapter in the same PR. The roadmap supported-surface inventory is an
exhaustive AST proof artifact; it does not define language behavior.

| ID | Disposition | Canonical anchors | Machine contract / diagnostic | Proof owner |
| --- | --- | --- | --- | --- |
| C1.1 | settled | [Model](../../spec/02-type-system.md#model); [Diagnostics are a language surface](../../spec/07-diagnostics-and-conformance.md#diagnostics-are-a-language-surface) | Primitive names and fixed-width ranges; `@type.numeric-out-of-range` | Step 5 corpus |
| C1.2 | new | [Model](../../spec/02-type-system.md#model) | M2 supports literal/name/application/lambda/do/let/if plus direct binders; other AST forms are availability failures | Step 1 inventory test; Steps 5–7 corpus |
| C1.3 | deferred | [Model](../../spec/02-type-system.md#model); [Generics](../../spec/02-type-system.md#generics) | Generic `where:`/`types:` checking, nominal collections, interfaces, and conversion remain M3 | M3 plan |
| C1.4 | deferred | [M2 executable effects](../../spec/03-effects.md#m2-executable-effects) | Only empty effect ceilings are admitted in M2; nonempty rows and `@host` execution remain M4 | Step 8/12 negatives |
| C1.5 | deferred | [Model](../../spec/02-type-system.md#model); [Control flow and failure](../../spec/02-type-system.md#control-flow-and-failure) | `match`, `try`, `option`, `result`, and exhaustive/refutable patterns remain M3 | Step 12 availability corpus |
| C1.6 | deferred | [Model](../../spec/02-type-system.md#model); [Labels and applications](../../spec/01-source-language.md#labels-and-applications) | Variadic array/map operands remain M3; M2 labels use reviewed literal defaults | Step 7 negatives / M3 |
| C1.7 | deferred | [Model](../../spec/02-type-system.md#model); [Type ascription and widening](../../spec/02-type-system.md#type-ascription-and-widening) | `as`, singleton widening, and narrowing remain M3 unless a later spec amendment says otherwise | Step 1 inventory; M3 |
| C2.1 | new | [M2 `@project.v1` schema](../../spec/04-programs-and-packages.md#m2-projectv1-schema) | Closed `@project.v1` field, type, order, role, and requiredness tables; unknown fields fail | Step 2 decoder |
| C2.2 | new | [Packages and targets](../../spec/04-programs-and-packages.md#packages-and-targets) | Binary requires `entry`/`effects`; library omits them; roots are canonical and disjoint | Steps 2–3 |
| C2.3 | settled | [VIBON data documents](../../spec/04-programs-and-packages.md#vibon-data-documents) | Atom roles are selected by schema slot, never spelling | Step 2 schema tests |
| C3.1 | new | [M2 project discovery and source snapshot](../../spec/04-programs-and-packages.md#m2-project-discovery-and-source-snapshot) | Discovery order is canonical path order; source graph is an explicit immutable input | Step 3 host tests |
| C3.2 | settled | [Modules and imports](../../spec/04-programs-and-packages.md#modules-and-imports) | File/dir collision and unknown path retain their distinct module diagnostics | Step 3/4 corpus |
| C3.3 | new | [Declarations](../../spec/01-source-language.md#declarations); [M2 command contract](../../spec/05-tooling.md#m2-command-contract) | M2 checks every declaration in each selected target and its transitive local-import units, including unreachable declarations, in deterministic file/declaration order; an omitted check selector covers every target | Step 4/12 corpus |
| C3.4 | new | [M2 unavailable-form spans](../../spec/07-diagnostics-and-conformance.md#m2-unavailable-form-spans) | Valid but deferred semantic forms emit `@tool.unavailable` at their form span | Step 1 registry; Step 12 corpus |
| C3.5 | new | [M2 project discovery and source snapshot](../../spec/04-programs-and-packages.md#m2-project-discovery-and-source-snapshot) | Existing file/dir starts search nearest ancestors for exact `project.vibon`; malformed nearest marker stops search; missing discovery is `@project.not-found`; legacy marker is never sniffed | Step 3 host + V1-PROJECT corpus |
| C3.6 | new | [M2 project discovery and source snapshot](../../spec/04-programs-and-packages.md#m2-project-discovery-and-source-snapshot) | Target roots are relative, existing, canonical, contained, and pairwise disjoint before walking; root failures use `@project.invalid-target-root` or `@project.io-error` with root spans | Step 3 host + V1-PROJECT corpus |
| C3.7 | new | [M2 project discovery and source snapshot](../../spec/04-programs-and-packages.md#m2-project-discovery-and-source-snapshot) | Symlink/junction roots are rejected; in-root links are canonicalized, cycles rejected, aliases deduplicated; escaping/dangling/read failures use `@module.path-escape`/`@module.io-error` | Step 3 host + platform fixture evidence |
| C3.8 | new | [M2 project discovery and source snapshot](../../spec/04-programs-and-packages.md#m2-project-discovery-and-source-snapshot) | Source IDs are project-relative slash paths; only `.vib` files are modules; segment/layout checks and collision diagnostics precede parsing; snapshot bytes/order are immutable and deterministic | Step 3 host + V1-PROJECT corpus |
| C3.9 | new | [M2 project discovery and source snapshot](../../spec/04-programs-and-packages.md#m2-project-discovery-and-source-snapshot) | Dependencies remain explicit unresolved graph edges and report `@tool.unavailable`; no network/cache/lock/dependency filesystem inspection | Step 3 host + V1-PROJECT corpus |
| C4.4 | new | [Diagnostics are a language surface](../../spec/07-diagnostics-and-conformance.md#diagnostics-are-a-language-surface) | Step 4 uses `@name.private-access` for resolved private imports, `@name.redeclaration` for duplicate top-level/alias/lexical names, `@name.member-collision` for one owner's flat members, and `@module.import-cycle` for import back edges; later referring/edge span is primary and earlier declaration/edge is related | Step 4 host + V1-TYPE-NAMES/V1-PROJECT corpus |
| C4.5 | new | [Namespaces and resolution](../../spec/02-type-system.md#namespaces-and-resolution); [M2 project discovery and source snapshot](../../spec/04-programs-and-packages.md#m2-project-discovery-and-source-snapshot); [Diagnostics are a language surface](../../spec/07-diagnostics-and-conformance.md#diagnostics-are-a-language-surface) | Declaration IDs carry package name and exact version (from the project record, or from the verified M2 bootstrap manifest), unit, module, owner path, and kind; resolver input is explicit and immutable and performs no filesystem, dependency, lock, cache, network, type, effect, or runtime work | Step 4/12 host + static-v1 resolved artifact |
| C4.1 | settled | [Diagnostics are a language surface](../../spec/07-diagnostics-and-conformance.md#diagnostics-are-a-language-surface) | Existing type/name/project codes retain their fixed levels and spans | Every static step |
| C4.2 | new | [Diagnostics are a language surface](../../spec/07-diagnostics-and-conformance.md#diagnostics-are-a-language-surface) | `@tool.unavailable` is an error with no fix; it is distinct from malformed syntax and runner `unavailable` | Step 1 registry/schema test |
| C4.3 | new | [Semantic reference](../../spec/06-runtime.md#semantic-reference); [M2 command contract](../../spec/05-tooling.md#m2-command-contract) | Type/check errors prevent lowering and execution; no partial program runs | Steps 5–12 |
| C5.1 | new | [Inference and checking](../../spec/02-type-system.md#inference-and-checking); [M2 module-value initialization](../../spec/06-runtime.md#m2-module-value-initialization) | `def` initialization is checked against written types and cycles are rejected before evaluation | Step 6 |
| C5.2 | deferred | [Model](../../spec/02-type-system.md#model) | Constructor/destructuring patterns are not M2 bindings; direct local names and discards are | Step 6 / M3 |
| C6.1 | new | [Labels and applications](../../spec/01-source-language.md#labels-and-applications) | Label binding uses a resolved signature; canonical order is fixed, labelled defaults are literals | Step 7 |
| C6.2 | deferred | [Model](../../spec/02-type-system.md#model); [Generics](../../spec/02-type-system.md#generics) | `types:` has no accepted M2 generic arguments; valid generic syntax is unavailable | Step 7/12 |
| C7.1 | new | [M2 compiler intrinsic profile](../../spec/06-runtime.md#m2-compiler-intrinsic-profile) | Only the two listed pure text operations may execute; exact signatures and total Unicode semantics are checked | Step 8 |
| C7.2 | settled | [Semantic reference](../../spec/06-runtime.md#semantic-reference); [Determinism and observability](../../spec/06-runtime.md#determinism-and-observability) | Pure execution has no host audit events and cannot read ambient state | Steps 5–14 |
| C8.1 | new | [M2 bootstrap trust input](../../spec/04-programs-and-packages.md#m2-bootstrap-trust-input) | The exact repository artifact, fixed `vibra-stdlib@0.1.0` identity from the pinned manifest, fixed toolchain-key identity, SHA-256 and Ed25519 verification order, resolver provenance, and explicit `@std.text`/`@std.assert` import map are closed; no network/vendor sync | Step 8/12 |
| C8.2 | deferred | [M2 bootstrap trust input](../../spec/04-programs-and-packages.md#m2-bootstrap-trust-input); [Dependencies and lock](../../spec/04-programs-and-packages.md#dependencies-and-lock) | Ordinary pinned dependency delivery and lock generation remain M5 | Step 2/3 negatives |
| C9.1 | new | [Tests](../../spec/04-programs-and-packages.md#tests); [M2 project discovery and source snapshot](../../spec/04-programs-and-packages.md#m2-project-discovery-and-source-snapshot) | Project-wide reserved `@tests` unit; optional confined top-level root; test declarations confined to that unit; test-relative module identity; target imports of `@tests` are unknown, while test modules may import targets or other test modules under ordinary visibility; deterministic test discovery | Step 13 |
| C9.2 | new | [Tests](../../spec/04-programs-and-packages.md#tests); [M2 command contract](../../spec/05-tooling.md#m2-command-contract) | Test identity is canonical module plus exact decoded string name; duplicate names are module-local errors; exact selector grammar and canonical string escaping are closed | Step 13 |
| C9.3 | new | [M2 assertion contract](../../spec/04-programs-and-packages.md#m2-assertion-contract) | Exact test-only assertion members/signatures, required explicit trusted import (`@module.missing-required-import` when absent), empty effects, canonical failure record, per-test isolation, and distinct trap/unavailable outcomes are closed; target references are unavailable, and assertion failures never become traps or host events | Step 13 |
| C10.1 | new | [M2 command contract](../../spec/05-tooling.md#m2-command-contract) | Exact grammar, options, canonical-root target selection, checking scope, canonical test selector and payload, stream routing, and exit mapping for `project init`, `fmt`, `check`, `run`, and `test` | Steps 11–13 |
| C10.2 | deferred | [V1 CLI](../../spec/05-tooling.md#v1-cli) | `lint`, `build`, `query`, `edit`, `mcp`, and `project inspect/add/remove/sync` are valid v1 names but unavailable in M2 | Step 12 command routing; Step 14 process regression |
| C11.1 | new | [Workspace queries](../../spec/05-tooling.md#workspace-queries) | Semantic results are neutral envelopes until their owning type/workspace step supplies payloads | Steps 4–10 |
| C11.2 | new | [Conformance corpus](../../spec/07-diagnostics-and-conformance.md#conformance-corpus) | Multi-file observations carry source ID; same byte offsets never merge | Step 1 synthetic tests |
| C11.3 | new | [Traps](../../spec/06-runtime.md#traps); [Conformance corpus](../../spec/07-diagnostics-and-conformance.md#conformance-corpus) | M2 checked-program boundary failures use `@runtime.invalid-checked-program`, with CLI/VIBON code spelling fixed and no source origin | Steps 12–13 |
| C12.1 | new | [Model](../../spec/02-type-system.md#model) | Every M1 AST enum variant has one M2 disposition and an inventory coverage test | Step 1 test |
| C12.2 | new | [Conformance profiles](../../spec/07-diagnostics-and-conformance.md#conformance-profiles) | Profile capability and semantic `@tool.unavailable` are separate; neither may be silently skipped | All steps |
| C12.3 | new | [M2 bootstrap trust input](../../spec/04-programs-and-packages.md#m2-bootstrap-trust-input); [Diagnostics are a language surface](../../spec/07-diagnostics-and-conformance.md#diagnostics-are-a-language-surface) | A resolver graph has unique source IDs across local and verified packages; duplicates emit `@module.source-id-collision` and block checking before lowering or execution | Step 12 host + corpus |

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
