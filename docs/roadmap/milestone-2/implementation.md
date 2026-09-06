# Milestone 2 implementation map

Read this with [the step plan](README.md) and [validation](validation.md).
Paths in code spans are repository-relative. Existing names below were checked
at M1's main merge `9c77b86`; re-read the actual API before calling it. Proposed
crate/module names describe responsibilities, not APIs already implemented.

## Existing entry points

| Existing file / API | Use in M2 |
| --- | --- |
| `crates/vibra-syntax/src/lib.rs`, `reader.rs` | Extension-selected loaders, shared lexer, lossless CST |
| `crates/vibra-syntax/src/ast.rs`: `decode_source_root`, `SourceAst`, `Declaration`, `ExpressionKind`, `TypeExpr`, `PatternKind` | Consume parsed structures; carry recovery and source origins forward |
| Same file: `BindingFacts`, `ApplicationBinding`, `BindingError` | Feed actual resolved signatures into argument binding; inspect limits before reuse |
| `crates/vibra-syntax/src/data.rs`: `decode_data_root`, `DataNode`, `DataValue` | Typed project decoding over generic data; no string re-parser |
| Same file: `TypedDataSchema`, `AtomRole` | Field order and atom-role starting point; currently only value/reference, not required entity kind or nested schema paths |
| `crates/vibra-fmt/src/lib.rs` | Source/data canonical output, comments and recovery preservation |
| `crates/vibra-syntax/src/query.rs`: `query_position`, `FactStatus` | Smallest-node selection and exact/recovered/unavailable syntax facts |
| `crates/vibra-diagnostics/src/registry.rs`, `diagnostic.rs`, `span.rs` | Closed codes/levels and origin spans; extend contracts before emitting new codes |
| `crates/vibra-schema/src/query.rs`, `diagnostic.rs`, `schemas/v1/` | Wire adapters and schemas; language phases must not import these |
| `crates/vibra-conformance/src/manifest.rs`, `corpus.rs` | Neutral TOML oracle and confined input/snapshot loading |
| `crates/vibra-conformance/src/runner.rs`: `ProfileHandler`, `CaseObservation`, `ExecutionObservation` | Real static/interpreter adapters and negative-oracle tests |
| `crates/vibra-conformance/src/profile.rs`, `bin/conformance.rs`, `reader.rs` | Capability dispatch; baseline binary registers only `reader-v1` |
| `crates/vibra-conformance/tests/architecture_boundary.rs` | Add actual new nodes and narrowly justified arrows |
| `.github/workflows/ci.yml` | Three-platform host checks and independent corpus job; broaden the job as handlers land |

Do not assume existing schema snapshots can express a workspace query or that
existing diagnostic spans identify a file. Step 1 inventories and closes both
contracts. Do not fork the neutral manifest into a second M2 test framework.

## Proposed ownership and dependency direction

| Node | Responsibility | May consume |
| --- | --- | --- |
| `vibra-resolve` | Explicit immutable source-graph input, declarations, scopes, canonical identities | syntax, diagnostics |
| `vibra-ir` | Semantic type/value IDs, immutable checked program representation, source origins | diagnostics |
| `vibra-types` | Expected-type checking, call contracts, lowering to checked IR, registry signature admission | resolve, syntax, ir, diagnostics |
| `vibra-interp` | Values, closures, activations, evaluation and intrinsic semantics | ir, diagnostics |
| `vibra-workspace` | Project decoder, filesystem snapshot, orchestration, semantic queries, format plans | syntax, resolve, types, ir, interp, fmt, diagnostics |
| `vibra-schema` | Versioned serialization of service facts/results | existing inputs plus workspace/semantic result types as needed |
| `vibra-cli` | Arguments, output routing, exit mapping, explicit write/run entrypoints | workspace, schema, diagnostics |
| `vibra-conformance` | Independent observations and cross-workspace invariants | required library nodes; CLI through process tests |

The IR crate owns the semantic type representation so the checker can produce
IR without an `ir -> types -> ir` cycle. Keep compiler registry signature data
in a backend-neutral module (proposed `vibra-ir::external`); the interpreter
owns execution of admitted operations. No checker dependency on interpreter.
Add only needed arrows, including dev-dependencies, to the architecture test.
Workspace may use filesystem APIs; resolver input is already an explicit graph.
The Step 4 resolver owns the neutral graph-input types so the semantic crate
does not depend on workspace acquisition or conformance adapters.

## Phase pipeline and invariants

1. Acquire immutable document bytes with explicit source IDs. Discovery and
   filesystem errors stay distinct from source diagnostics.
2. Decode project data without resolving any atom. Preserve per-field origins
   and schema-selected required kinds, including nested target entries.
3. Validate roots and module layout before parsing modules. Construct a sorted
   unit/module graph; resolve imports and reject cycles before body checking.
4. Collect declaration headers before resolving bodies. Use IDs, not repeated
   string lookup; distinguish unknown paths, wrong kinds, and private access.
5. Check supported signatures, then bodies against written expected types.
   Poison/recovered facts are not successful types. Suppress cascades according
   to the agreed diagnostic policy; keep useful neighboring facts.
6. Lower only accepted programs to immutable IR. Every runtime operand carries
   its type and source origin; bind labels once into semantic evaluation order.
7. Execute through an explicit activation/value machine. The interpreter must
   never reparse CST, resolve names, infer types, or trust unchecked IR from CLI.
8. Render results and queries from that same snapshot through schema adapters.
   Program execution is a separate service request, never a side effect of a
   query, formatter, or checker.

Keep modules organized by responsibility (project decoding, graph, scopes,
literal checking, call checking, lowering, evaluation, queries). Avoid one
recursive function with flags for phase, syntax role, and runtime mode.

## Slice discipline

For each matrix row: write the normative expectation, a failing focused host
test, and an independently authored corpus case; implement through the real
handler; run both suites. A rejected program must never execute partially.
Check the whole declared slice, including unused declarations, as specified by
the Step 1 checking-scope contract.

Use `static-v1` for checking observations and `interpreter-v1` when execution is
required. These remain the closed existing profile names. Preserve the reader
handler for reader-only cases. A negative availability case may assert the
agreed diagnostic without claiming execution support for its input; an
unavailable requested execution cannot be converted into a passed value test.

Extend handler coverage with each step. Handlers claim a closed manifest
operation, and the dispatcher rejects overlapping claims at the selected
profile. Do not register a stub that returns
`accepted: true`, compare a snapshot to itself, serialize Rust `Debug` as a
public contract, or copy expected values into observations. Prove each new
observation channel detects an intentionally wrong expected result.
