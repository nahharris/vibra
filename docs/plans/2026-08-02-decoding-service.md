---
title: Design — Type-constrained decoding service
category: plans
status: proposed
updated: 2026-08-07
issue: 254
summary: >-
  Canonical first-deliverable design for a type-constrained decoding service;
  implementation remains gated on the typed frontend and CLI cutover.
---

# Design: type-constrained decoding (#254)

This is the canonical design record for the first deliverable of issue #254.
It is a working plan under [`docs/plans/`](README.md), not an accepted language
contract. This issue intentionally delivers no decoder, service, protocol
implementation, or prototype.

The standing language-design criterion is:

> Prefer type-system features whose decoding automaton is small.

That criterion applies to every future type-system proposal, including #151.
The decoder is the measurement instrument that makes the criterion falsifiable;
it is not a reason to add a large semantic subsystem before the typed frontend
is usable.

## Decision summary

| Question | Decision for the prototype that follows this design |
| --- | --- |
| Initial automaton | A closed, annotation-led subset of canonical S-expressions, primitive and immutable structural types, lexical bindings, direct calls, field/index access, primitive operators, `do`, `let`, `if`, and `return`. |
| Outside-subset behavior | Strict typed mode returns a tagged `unsupported` result. Syntax-only fallback is opt-in, tagged on every response, and counted separately. There is no silent degradation. |
| Transport | A dedicated versioned streaming protocol over a long-lived stdio connection first; LSP and MCP remain adapters for their existing roles. |
| Type graph | Immutable, content-addressed signature/type snapshots with dependency-aware incremental invalidation and prefix/state memoization. |
| Measurement | `Vanilla`, idealized `Syntax`, and `Types` controls, with the same compiler-error instance counting and syntax/type taxonomy as Mündler et al.; measure synthesis, translation, and repair separately. |
| Implementation gate | `surface_adapter.rs` parity and a real typed-path CLI cutover. The current staged typed subset is evidence for scope, not a usable decoder oracle. |

## Why this design is needed

Mündler et al. report that syntax is only about 6% of compilation errors in
LLM-generated TypeScript and that grammar-level constraint has an idealized
ceiling of 9.0% on HumanEval synthesis and 4.8% on MBPP synthesis. Their
type-constrained decoder reduced compilation errors by 74.8% and 56.0% on those
two synthesis tasks. The mechanism is a prefix automaton whose states retain
typing context and whose expression completion uses type-reachability search.
The relevant comparison is therefore not “valid S-expressions versus invalid
S-expressions”; it is typed prefix admission versus syntax-only admission.

Vibra is unusually suited to testing whether a language can make that automaton
cheap: its surface is canonical, signatures are explicit, and the philosophy
already requires a small typed search space. Those choices are useful only if
the implementation measures their cost rather than assuming it.

## Gating dependency: the typed path is not ready

The prototype must not be built on the current bridge. The risk investigations
establish all of the following:

- Source-compiling CLI entry points still use the legacy
  `load::load_legacy_yaml_program` path and lower through
  `surface_adapter::module_to_value_with_alias`. The typed producer is not
  called by the CLI; [`src/typed_program.rs`](../../src/typed_program.rs)
  explicitly describes it as additive and unreachable from existing commands.
- [`src/surface_adapter.rs`](../../src/surface_adapter.rs) is a one-way
  compatibility bridge, not a parity implementation. It fails closed for
  constructs it cannot map, so a decoder built against it would encode the
  wrong surface contract and be rewritten at cutover.
- The typed executable subset rejects or stages generic functions/calls,
  enums and nominal types, `match`, casts, interfaces, and related dispatch
  cases. Those are not hypothetical future concerns: they are current
  materialization/body failures documented in
  [`risk-findings/247-typed-path.md`](risk-findings/247-typed-path.md) and
  [`risk-findings/248-typed-path.md`](risk-findings/248-typed-path.md).
- The investigation's measured funnel was `surface-valid: 71/71`,
  `signature-valid: 71/71`, `body-valid: 22/71`, and
  `materialized-valid: 6/22`. The older corpus-migrator README reports
  `58/58`, `57/58`, and `19/57`; those figures describe an older or
  diagnostic-only scan and must not be presented as typed cutover readiness.

This gate does not block design work. While the gate is open, it is safe to
settle the protocol contract, subset boundaries, graph/cache keys, fallback
semantics, feature classification, and benchmark taxonomy. It is not safe to
claim that a typed prefix check is sound for the live CLI, to use
`surface_adapter` as the service's semantic source of truth, or to report
prototype results.

The prototype gate is closed only when:

1. the typed frontend either reaches parity for the measured subset or the
   compatibility bridge is removed from that path;
2. `run`, `test`, `build`, and tooling paths used by the benchmark compile from
   the same typed semantic representation rather than silently selecting the
   legacy bridge;
3. the readiness funnel is regenerated from the current branch and publishes
   separate surface, signature, body, and materialization counts; and
4. diagnostics and source locations have parity on the subset, including
   explicit unsupported-feature outcomes rather than accidental adapter errors.

## Automaton contract

### Initial type-constrained subset

The first automaton is deliberately closed and annotation-led. “Supported”
means that the decoder can prove prefix extendability using only the current
snapshot and bounded local search; it does not mean that the legacy compiler
can happen to lower the construct.

The initial subset contains:

- canonical S-expression reader/formatter forms, strings, atoms, booleans,
  numeric literals, and the primitive numeric/string/boolean operations whose
  operand and result types are fixed;
- `void`, primitive scalar types, `Range`, and immutable structural types:
  tuples, records, arrays, and maps. Named aliases are allowed only when they
  are non-generic, acyclic, and reduce to those closed structural types;
- explicit, non-generic function parameter and return signatures, resolved
  local/imported symbols, fixed-arity direct calls, and opaque host/Wasm leaves
  whose complete signatures are already present in the snapshot;
- lexical variable references, explicit `let` bindings, literals and direct
  typed-call initializers, field access, array/map indexing, and the canonical
  `do` sequence, `if`, and `return` forms;
- a bounded expression/type-search depth and a finite local environment. An
  expression whose only possible completion exceeds the budget is
  `unsupported` rather than guessed.

The subset intentionally does not include any feature that requires open-ended
polymorphic or nominal search:

- generic functions, generic calls, generic aliases, and unresolved type
  parameters;
- enums, `newtype`, nominal identity, interfaces, `impl` dispatch, and
  interface-pattern values;
- `match`, exhaustiveness/pattern search, and enum constructors;
- casts and any conversion whose target cannot be established from the
  expected type;
- function values, closures, higher-order calls, and `FnType` values;
- recursive/open aliases, `any`-like open expected types, unbounded union or
  intersection search, and inferred declarations without a closed type;
- loops, tasks, mutable references, and control-flow forms that require
  unbounded state are staged after the first measurement, even though their
  individual typing rules may be local.

This is a product decision, not a claim that the excluded features are bad
language features. They are expensive decoding features until measured. The
current typed-path rejection of several of them is recorded as a readiness risk
above, not hidden as successful support.

### Prefix state and decisions

A prefix state carries the following logical information:

1. the immutable source snapshot and byte/token position;
2. parser state for the canonical surface;
3. the lexical/type environment and visible function signatures;
4. the expression under construction, its known type when available, and the
   expected type imposed by the surrounding construct;
5. control state for whether the current body still requires a value or a
   return; and
6. the bounded type-search budget and feature profile.

For a candidate token (or a batch of candidate tokens), the service answers
whether at least one suffix can complete the prefix into a subset-valid,
well-typed program. Direct calls, member access, indexing, and primitive
operators add edges to a type-reachability search; the search is not a
completion list and must not accept a token merely because its name is in
scope.

The wire-level decision is a tagged union:

| Result | Meaning | Caller action |
| --- | --- | --- |
| `accept` | Candidate preserves a reachable typed prefix. | Keep the candidate. |
| `reject` | Candidate is definitely invalid in the selected syntax/type subset. | Zero the candidate and resample. Include `phase: syntax` or `phase: type` and a stable reason. |
| `unsupported` | The prefix requires a feature or search outside the subset/budget. | Stop strict typed decoding or make an explicit fallback request. Never treat it as a typed rejection. |
| `stale-snapshot` / `not-ready` | The requested graph snapshot is unavailable or invalidated. | Refresh or wait; do not use a stale graph. |

The service must return the snapshot ID, feature profile, and mode on every
response. This makes logs and benchmark denominators auditable.

### Observable fallback behavior

Strict typed mode is the default. On `unsupported`, `stale-snapshot`, or
`not-ready`, it returns no typed mask and does not silently switch modes. A
caller may opt into syntax-only fallback at request creation or after an
`unsupported` response. In that case every decision is tagged
`mode: syntax-only`, `degraded: true`, and includes a stable fallback reason
such as `unsupported-generic-type` or `graph-not-ready`.

Fallback output is never included in the `Types` result. It is reported in a
separate `Fallback` column and as a coverage rate. Transport failure follows
the same rule: fail closed unless the caller explicitly selected syntax-only
operation. This prevents a grammar-only generation from being described as
type-constrained and preserves the paper's syntax-only control.

## Transport decision

Use a dedicated protocol over one long-lived stdio connection for the first
service integration. The protocol is versioned independently of LSP and MCP,
uses length-delimited messages, supports cancellation and batched candidate
checks, and keeps a snapshot open across many prefix decisions. The decoder
client supplies candidate token IDs/text; the service does not own model
logits or a sampling loop.

The minimum lifecycle to specify before implementation is:

1. `initialize` negotiates protocol version and the supported feature profile;
2. `open` installs a workspace/source snapshot and returns its content hash;
3. `edit` creates a new snapshot or reports a stale edit;
4. `admit` checks one or more candidate tokens against a snapshot and expected
   type, returning the tagged decisions above; and
5. `cancel`/`close` releases request and snapshot state.

The transport choice is deliberate:

| Transport | Keep for | Why it is not the decoder transport |
| --- | --- | --- |
| LSP | Editor diagnostics, hover, definitions, references, and ordinary completion. | LSP is document/request oriented and its completion model answers “what symbol could go here?”, not repeated prefix admission with typed search, cancellation, and candidate batches. |
| MCP | Agent-facing project inspection and coarse compiler/test tools. | MCP tool calls are intentionally coarse and serial in the current server; a token-by-token tool call would add the wrong latency and authority surface. |
| Dedicated protocol | Prefix admission and graph-snapshot lifecycle. | It can make latency, cancellation, versioning, and fallback status explicit without changing either public integration contract. |

LSP and MCP adapters may call the dedicated service later, but they must retain
their existing semantics and must not conceal an `unsupported` result. A local
stdio process is the initial deployment boundary; a local socket or embedded
library can be evaluated only after measurements show that framing overhead is
material.

## Incremental type graph and caching

The graph is a view of a source snapshot, not mutable global compiler state.
Each node is a supported closed type or function signature. Edges represent
field/member access, indexing, direct calls, and primitive operators. The graph
contains only declarations visible in the selected module/import closure; it
does not speculate about future modules or unbounded generic instantiations.

### Construction

1. Parse and normalize the changed module, then extract its public signatures,
   non-generic aliases, and source locations.
2. Resolve imports and compute a deterministic module signature hash from the
   canonical source, imported signature hashes, language/stdlib contract
   version, and feature profile.
3. Reuse unchanged module graph fragments. Rebuild the changed fragment and
   its reverse dependents only when a public signature, alias expansion, or
   import edge changes; a body-only edit invalidates local prefix states but
   does not invalidate dependent signature graphs.
4. Publish the new graph as an immutable snapshot. Requests name the snapshot
   explicitly, so a request cannot mix declarations from two edits.

### Cache keys and invalidation

The graph cache key is:

```text
(language-contract, compiler-build, feature-profile,
 root-module-signature-hash, ordered-import-closure-hash)
```

The type-inhabitation cache key adds the expected type, lexical-environment
fingerprint, search-depth budget, and reachable-scope fingerprint. The prefix
automaton cache key adds the snapshot ID, canonical prefix hash, parser state,
expected type, and budget. Positive and negative results are both bounded and
evictable; negative results must never survive a snapshot/profile change.

Cache hits, misses, invalidations, graph-build time, graph size, and search
budget exhaustion are observable counters in the protocol and benchmark. A
stale graph is a protocol status, not a cache hit.

## Cheap versus expensive language features

The following is the initial classification of existing Vibra features. “Cheap”
means finite, local, and cacheable under the initial state model; it does not
mean free. “Expensive” means that the feature adds unbounded polymorphic search,
nominal/branching state, higher-order reachability, or whole-program invalidation
and therefore needs a separate automaton design and measurements.

| Classification | Features | Automaton consequence |
| --- | --- | --- |
| Cheap | Canonical S-expression syntax and idempotent formatting | Small parser state with one surface spelling per construct. |
| Cheap | Explicit non-generic function signatures and visible imports | Expected types and call arity are known before body completion; graph changes are signature-addressable. |
| Cheap | Primitive literals/operators, lexical names, `let`, direct calls, field access, and indexing | Finite local environments and fixed type edges. |
| Cheap | Immutable tuples, records, arrays, maps, and acyclic structural aliases | Product/container expansion is finite when element types and search depth are closed. |
| Cheap | `do`, `if`, and `return` with explicit expected types | Control state is finite and does not require pattern enumeration. |
| Expensive | Generics, generic aliases, and polymorphic calls | Each prefix can create new substitutions and type-graph nodes; cache identity becomes open-ended. |
| Expensive | Enums, nominal/newtype identity, interfaces, `impl` dispatch, and interface patterns | Nominal equality and dispatch add branch families that are not captured by structural shape alone. |
| Expensive | `match`, patterns, and exhaustiveness | The automaton must track scrutinee refinements, bindings, and remaining cases across branches. |
| Expensive | Casts and implicit conversions | They weaken the expected-type invariant and can introduce arbitrary target-type search. |
| Expensive | Function values, closures, higher-order calls, and recursive/open aliases | Type reachability becomes higher-order or non-terminating without a stronger abstraction. |
| Expensive | Unannotated inference, `any`-like openness, unbounded unions/intersections, loops, tasks, and mutable references | Prefix validity depends on larger environments or unbounded state; the current local cache key is insufficient. |

This table feeds language evolution. Every future type-system proposal must add
an automaton note covering: new state dimensions, type-graph nodes/edges,
worst-case search depth, cache invalidation scope, explicit unsupported/fallback
behavior, and measured prefix latency/memory. A proposal may be accepted as
cheap only after it stays within the prototype's measured latency and state
budgets; convenience or compiler implementability alone is not evidence.

## Measurement plan

### Controls and units

The primary baseline is the paper's `Vanilla` condition: the model generates
without a decoder. The two comparison conditions are named exactly as in the
paper:

- `Syntax`: idealized syntax-only constraint, where all baseline syntax-invalid
  instances are treated as repaired by grammar constraint. If a real grammar
  mask is later implemented, report it separately as a sensitivity check.
- `Types`: strict type-constrained decoding over the supported Vibra profile.
- `Fallback`: explicit syntax-only operation after an observable unsupported or
  readiness result; never pool this with `Types`.

The unit is one generated artifact/instance. An artifact with one or many
compiler diagnostics counts once as a compiler-error instance, matching the
paper's Table 2 rather than counting diagnostic messages. Report both the
number of attempted artifacts and the number covered by the typed subset.

The benchmark has two tracks:

1. a replication track using the paper's HumanEval and MBPP problem IDs and
   task modes (synthesis, translation, repair), preserving its six open-weight
   model families/sizes where the models and toolchain are available; and
2. a Vibra track with the same problem IDs translated to canonical Vibra
   signatures and tests, plus a fixed Vibra-native suite for features not
   represented by those tasks.

The replication track checks that the harness reproduces the control labels;
the Vibra track measures the language/design question. Prompt templates,
model/tokenizer versions, seeds, sampling budget, timeout, compiler revision,
stdlib revision, and test inputs are frozen in the benchmark record.

### Error taxonomy

Use the source paper's three reporting concepts without renaming them:

| Label | Definition for Vibra | Reporting rule |
| --- | --- | --- |
| `Syntax` | Reader/parser/formatter-shape failure before a typed program can be formed. | Count once per generated artifact. |
| `Types` | Static semantic failure after parsing, including unresolved names, invalid member/operator use, argument/return mismatch, and other type-check failures. | Count once per generated artifact. |
| `Compiler errors` | Any artifact that does not compile; the headline total is the union of `Syntax` and `Types`. | Report counts and rates exactly as `Vanilla`, `Syntax`, and `Types` columns, not message counts. |

Vibra-specific failures that are not syntax or static typing—manifest/workspace,
effect-policy, host/ABI, timeout, or service-protocol failures—are reported in
an explicit `Other/out-of-scope` side column and never silently folded into
`Types`. They are excluded from the paper-comparable compiler-error headline
only when the run record identifies them; an unexplained failure invalidates
that sample. Unsupported/fallback outcomes are also side-counted and do not
become successful `Types` samples.

### Metrics and decision rule

For every model, dataset, task mode, and decoder, record:

- compiler-error rate and syntax/type counts;
- error reduction relative to `Vanilla`, with `Syntax` as the idealized
  grammar ceiling and `Types` as the typed result;
- pass@1 and the same functional tests used by the task;
- median/p95/p99 prefix-decision latency, graph-build latency, cache hit rate,
  accepted-token rate, search-budget exhaustion, unsupported rate, and explicit
  fallback rate; and
- coverage: attempted artifacts, subset-covered artifacts, strict typed
  artifacts, and fallback artifacts.

Before seeing results, classify the approach as a Vibra-wide decoder failure if
either of these holds on subset-covered artifacts across two independent task
families and a majority of evaluated model families:

1. `Types` does not reduce `Types`-taxonomy compiler-error instances by at least
   25% relative to the `Syntax` condition; or
2. p95 prefix-decision latency exceeds 2× the syntax-only condition, or more
   than 20% of target artifacts require `unsupported`/fallback.

Meeting the error threshold while failing coverage or latency is evidence for a
smaller language profile, not permission to silently broaden fallback. Meeting
all thresholds permits a prototype expansion proposal; it does not establish
functional correctness, security, or semantic intent, because the paper also
found that well-typed programs can still be wrong.

## Delivery sequence and non-goals

The work after this design is staged:

1. finish the typed frontend/adapter parity and make the selected CLI paths use
   it;
2. implement the dedicated protocol and immutable graph snapshot contract for
   the closed subset;
3. measure `Vanilla`, `Syntax`, `Types`, and explicit `Fallback` on the frozen
   benchmark; and
4. classify additions by the feature table and make a go/no-go decision before
   expanding the subset or claiming a general service.

This issue does not implement any of those phases. It does not add a new LSP
method, MCP tool, decoder binary, token mask, type-graph cache, benchmark
harness, or language feature. It also does not change the accepted language
contract; any future surface change belongs in the relevant decision document
and tests.

## References

- Mündler, He, Wang, Sen, Song, and Vechev, [*Type-Constrained Code
  Generation with Language Models*](https://doi.org/10.1145/3729274), PLDI
  2025; [open preprint](https://arxiv.org/abs/2504.09246).
- Repository distillation: [`docs/research/notes/code-dsl-design.md`](../research/notes/code-dsl-design.md), section “Type-Constrained Code Generation”.
- Repository synthesis: [`docs/research/01-design-directions.md`](../research/01-design-directions.md), “The reframe”.
- Readiness evidence: [`risk-findings/247-typed-path.md`](risk-findings/247-typed-path.md) and [`risk-findings/248-typed-path.md`](risk-findings/248-typed-path.md).
