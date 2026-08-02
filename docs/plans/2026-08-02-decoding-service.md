---
title: Plan — Type-constrained decoding service
category: plans
status: proposed
updated: 2026-08-02
issue: 254
---

# Plan: type-constrained decoding (#254)

**Wave 6, and deliberately last.** Not because it matters least — it is the
highest-value item in the roadmap — but because it is the one that most needs a
stable typed frontend underneath it, and because its first deliverable is a
design document rather than code.

**Blocked by:** `src/surface_adapter.rs` reaching parity and being removed. A
decoding service built against a temporary bridge would be built twice.

## Why this is the most valuable item

Mündler et al. measure syntax at ~6% of compilation errors in LLM-generated
TypeScript against ~94% from type failures, with a ~9% ceiling on grammar-level
constraint. Their type-constrained decoding — a prefix automaton whose states
carry a typing context, driving type-inhabitation search over a type graph —
cut errors 74.8% on HumanEval and 56.0% on MBPP across six models from 2B to
34B. It is the strongest evidence in the entire review.

Two consequences:

1. It **replaces the justification** for Vibra's canonical syntax and mandatory
   signatures. They are not primarily about reducing malformed output; they are
   what makes prefix-time type search terminate. (#256 puts this on the
   record.)
2. **Nobody has combined a designed-for-LLM language with type-constrained
   decoding**, and nobody has asked whether a language can be designed so its
   decoding engine is cheap. Building one for a TypeScript subset cost ~11
   kLoC. Vibra controls the language and can therefore ask a question the
   literature has not.

## The design criterion this establishes

**Prefer type-system features whose decoding automaton is small.** This should
become a standing constraint on language evolution, evaluated for every future
type-system proposal — including #151 — not a property of this service alone.
That is the durable value here; the service is how the criterion gets teeth.

## Design decisions

**First deliverable is a design document, not an implementation.** An 11 kLoC
estimate against a language still retiring a lowering bridge should not begin
as a code PR. The document is the artifact this issue delivers.

**The LSP context query is a seed, not a starting point.** It reports valid
keys, types, imports, and symbols at a position — that is completion, which
answers "what could go here," not "what is well-typed here given everything
generated so far." The difference is the type-inhabitation search, and it is
the hard part.

**Measurement plan is mandatory and specified up front.** The entire
justification is empirical, so the plan must state a baseline and use the same
error taxonomy as the source paper, or the numbers will not be comparable to
the result motivating the work.

## What the design document must settle

- Which type-system subset the automaton covers initially, and the fallback
  behaviour outside it. A service that silently degrades to grammar-only
  constraint recovers the 9% ceiling and none of the 74.8%; degradation must be
  observable.
- Transport: LSP, MCP, or a dedicated protocol. Constrained decoding is
  latency-critical in a way LSP was not designed for; this needs deciding, not
  defaulting.
- Incremental construction and caching of the type graph.
- Which existing Vibra features are *expensive* to decode. This is the feedback
  into language design and the most useful output of the document — an honest
  list of features that should perhaps be reconsidered.
- The measurement plan: baseline, taxonomy, and what result would falsify the
  approach for Vibra specifically.

## Phases

1. Design document under `docs/plans/`, linked from `docs/index.md`.
2. Cheap/expensive feature classification, fed back to #151 and future type
   work.
3. Prototype over a deliberately narrow subset, measured against the stated
   baseline.
4. Go/no-go on full implementation, decided on the prototype's numbers.

## Risks

The dominant risk is starting the implementation before the frontend is stable
and paying for it twice. The second is scope: the source paper's engine covers
a TypeScript subset and still cost 11 kLoC. A prototype that tries to cover all
of Vibra will not finish. Narrow deliberately, measure, then decide.

## Definition of done for this issue

A design document that settles the five questions above, an explicit
cheap-versus-expensive feature classification, and a measurement plan with a
stated baseline. Implementation is a separate issue gated on the prototype.
