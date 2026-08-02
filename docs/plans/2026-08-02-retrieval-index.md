---
title: Plan — Normalized retrieval index
category: plans
status: proposed
updated: 2026-08-02
issue: 250
---

# Plan: `vibra index` (#250)

**Wave 2, independent.** Can run fully parallel with #249; touches no shared
code beyond reading the effect report.

## Why

The Principles page asks for retrieval over normalized structure — types,
effects, API calls, control-flow summaries — rather than raw text. Vibra emits
none today. Most of the data already exists: `vibra effects` computes call
edges and declared/performed sets, and the typed AST carries signatures and
error types.

## Design decisions

**The compiler emits the artifact and stops there.** No embedding, no ranking,
no vector storage, no model dependency. This is not modesty about scope — it
follows from the evidence: a 600M embedding model beats a 30B decoder by 85
points on Java code retrieval at 1/50th the size, and the right retriever is
therefore a moving target that must not be frozen into a compiler.

**Normalization is `vibra fmt`, applied symmetrically.** Style normalization
lifts weak models 28–29 points, but only when applied to *both* query and
corpus, and only when it preserves comments and identifiers. Vibra's idempotent
formatter is already exactly right. The schema must document the symmetry
requirement, because a consumer that normalizes only the corpus gets no benefit
and will conclude the index is useless.

**Benchmark on rejection precision, not top-k.** ProjAgent's signal has 5.4%
precision but ≥97.9% precision on rejections. A first stage that eliminates
well beats one that ranks well when an exact verifier sits behind it — and
Vibra has the strongest possible verifier, the type checker. Any evaluation
harness added later must measure elimination power; stating this now prevents
the feature being judged by the wrong metric.

**Record shape.** Per function: module path, qualified name, signature
(parameter and return types), effect set (declared *and* inferred once #249
lands), outgoing call edges, error types, visibility, and fmt-normalized
source. Include private functions — they are prime retrieval targets for
repository-level generation — with visibility as a filterable field rather than
an exclusion.

**Determinism is a hard requirement.** Sorted keys, stable ordering, byte-
identical output for identical input. An index that churns produces spurious
diffs and defeats caching.

## Phases

1. **Schema first**: `schemas/index.schema.json` with a stable `$id`, added to
   the README schema guide. Designing the record before the extraction forces
   the consumer question — what does a retriever actually need — to be answered
   deliberately.
2. **Extraction** over the typed program, reusing the call-graph and effect
   machinery behind `vibra effects`.
3. **CLI**: `vibra index [--format json]`, defaulting to JSON since its stdout
   is primarily machine data.
4. **Determinism test**: run twice, assert byte equality.
5. **README** documents the command and the symmetric-normalization
   requirement.

## Testing

Rust tests over a fixture project covering: generic functions; interface
dispatch; `deffect` operations; private functions; macro-generated definitions
(which must carry their expansion origin, not a fabricated source span); and
determinism. One end-to-end CLI test asserting schema validity.

## Risks — the macro risk was investigated and is not real

The plan flagged macro-generated definitions as the trap, on the reasoning that
they have no honest source text and would poison a retrieval corpus. Verified
(`risk-findings/250-macro-definitions.md`): **`(macro` appears zero times in
`.vib` files repo-wide**, and definition-generating macros are not merely
absent but *unimplemented* — `typed_macro_expand.rs:197` explicitly refuses
`@definition-syntax` and `@module-syntax`.

**Simplify accordingly.** Drop the provenance subsystem the plan implied. Keep
a few-line `generated: true` guard on the `Expansion` origin variants, so the
field exists when definition macros eventually land and no retrieval corpus is
silently poisoned in the meantime. Origin tracking already exists and is
queryable (`Origin` enum, `surface.rs:55-102`, stamped by
`annotate_generated_expr` at `:2331`, consumed via `typed_body.rs:46`
`node_origins`).

**Note for whoever writes the code:** `docs/decisions/s-expression-language.md:590-603`
specifies an `OriginId`-keyed arena. **The code has neither** — it uses
`Arc<Origin>` chains. A plan written against the document's field list will not
compile. This is more #256 material.

The remaining risk is unchanged and unglamorous: determinism. An index that
churns produces spurious diffs and defeats caching.

## Definition of done

`vibra index` emits schema-valid, deterministic, fmt-normalized records
covering the corpus, the schema documents symmetry and the rejection-precision
metric, and both suites pass.
