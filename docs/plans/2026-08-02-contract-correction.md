---
title: Plan — Correct stale and self-contradicting contracts
category: plans
status: proposed
updated: 2026-08-02
issue: 256
---

# Plan: contract correction (#256)

**Wave 0. Nothing else should land first.** Every other plan in this roadmap
amends a contract. Amending a document that already disagrees with the corpus
compounds the drift instead of removing it.

## Why this is first, and why it is not cosmetic

`AGENTS.md` makes documentation part of the safety model. Today
`decisions/s-expression-language.md` normatively specifies `fn`,
`(case pattern body)` inside `match`, and `(def name Type expression)`
constants — none of which the corpus uses. An agent that reads the accepted
contract and writes to it produces code the parser rejects. For an LLM-first
language, that is a correctness bug in the primary interface, not a docs chore.

## Design decisions

**Corpus wins on all three grammar conflicts.** `defn`, bare pattern/body pairs
in `match`, and `def`-with-type-constructor are implemented, tested, and used
everywhere. Changing the implementation to match the prose would be a breaking
change purchased for nothing.

One caveat worth recording rather than silently accepting: bare pattern/body
pairs in `match` are *less* regular than `(case ...)`, because they make arm
boundaries positional rather than delimited. That is a real cost for a language
whose thesis is one-canonical-form regularity, and it will matter again when
`try` lands (#248). Record it as a known wart with a rationale, so a future
revisit is informed rather than rediscovered.

**Rewrite the syntax rationale in `philosophy.md`, do not delete it.** The
conclusion — canonical syntax, one spelling per idea — survives the research.
The stated reason does not. Replace "every extra choice is another chance for
hallucination" with the decoding-and-retrieval justification from
[`../research/01-design-directions.md`](../research/01-design-directions.md).
This matters beyond accuracy: the new justification implies a *design
criterion* (prefer type-system features whose decoding automaton is small) that
the old one does not, and #254 depends on it being on the record.

## Phases

1. **Grammar reconciliation.** Update the EBNF in
   `decisions/s-expression-language.md` for `defn`, `match` arms, and `def`
   constants. Add the `match`-arm regularity caveat.
2. **Archive the decommissioned material.** Move the `policy.narrow` and
   capability grammar — including the rationale paragraph — from inline notes
   into `archive/`, linked from `docs/index.md`. Leave a one-line pointer at
   the original site.
3. **Philosophy rewrite.** Replace the YAML-shapes and `$`-keys guidance;
   restate the syntax rationale; correct the host-access paragraph to say what
   is true now and reference #253 for what is planned.
4. **Record the runtime capability finding.** `CapabilityGrant` survives in
   `src/async_runtime.rs`. Neither contract mentions it, and it changes the
   cost estimate for #253 by an order of magnitude.
5. **The durable fix.** A test that parses every fenced `vibra` block in
   `docs/decisions/*.md` and asserts it is accepted by the reader. Without
   this, drift recurs and this issue is reopened in six months.

## Testing

- New Rust test extracting fenced `vibra` blocks from decision documents and
  running them through the reader. Blocks that are intentionally invalid get an
  explicit `vibra-invalid` fence tag so the test can assert rejection instead.
- `rg` assertions in the repository-policy test: no current-guidance reference
  to YAML shapes or `$` keys.

## Risks

The doc-block test is the load-bearing part and the part most likely to be cut
under time pressure. Some blocks are fragments, not whole modules, and will not
parse standalone. Mitigation: allow a fence tag marking a block as a fragment,
and wrap fragments in a synthetic module before parsing — do not exempt them
from checking, or the test decays to nothing.

## Definition of done

No accepted contract describes syntax the parser rejects, and that fact is
enforced by a test rather than by review. Both suites pass.
