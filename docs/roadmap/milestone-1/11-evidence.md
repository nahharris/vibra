# Step 11 — executable exit-gate evidence

Prerequisites: Steps 1–10 landed; all contract decisions closed. This is an
evidence step, not a place to implement missing language surfaces. A failed
gate produces a regression case and a repair in its owning slice, followed by
a new audit of the repaired head.

## Syntax-example inventory

Create a tracked inventory covering every syntax-bearing example in active
`docs/spec/` chapters, including inline examples and fenced fragments. Record
chapter/heading, stable identifying excerpt, document mode, classification,
case ID or host test, required wrapper/context, and deferred semantic checks.
Classifications distinguish reader-positive, reader-negative, recovery, and
non-source grammar/schema illustrations. Invalid examples are tests too.

Fragments may use a documented module/declaration wrapper. Preserve the exact
fragment within it; do not silently repair it to make the example pass. Later
semantic examples still exercise their M1 syntax, with type/effect/runtime
claims explicitly deferred. Review-only illustrations carry a specific reason,
not a blanket exclusion of a chapter. Add a repeatable inventory check that
detects new/unclassified examples and stale case references.

## Fuzz campaign to add and run

Add reproducible targets for raw byte ingestion, valid UTF-8 parsing in both
modes, parse/format/reparse, and structural queries at generated offsets.
Include deep nesting and destruction/traversal, long tokens, truncated escapes,
Unicode, comments, mismatched delimiters, and mutation of existing corpus inputs.
Raw invalid UTF-8 must return an error rather than being lossily decoded.

Before the campaign starts, commit its configuration: tool/version, exact
commands, seed corpus, seed where supported, target list, platform, per-target
duration or iteration budget, worker count, and resource limits. These are
test-harness limits, not new language limits. Use a short deterministic smoke
run in CI and a separately recorded bounded campaign for the gate. Do not claim
that a smoke run alone is the configured campaign.

Properties: no panic or nontermination within the harness bounds; lossless CST;
valid span boundaries; accepted formatted output reparses successfully and is
structurally equivalent and idempotent; recovered formatting preserves bytes;
queries do not panic on arbitrary offsets. Minimize failures and retain them as
host regressions and corpus cases where representable. Re-run the affected
target and full validation after repair. No targets currently exist at the
baseline; this guide is not evidence of a completed campaign.

## Gate evidence table to fill on completion

| Normative gate | Required evidence |
| --- | --- |
| Reader positive/negative/recovery corpus | Tested commit, exact runner command and counts; zero failed/unavailable |
| Every syntax example classified and exercised | Inventory path, checker command/result, no unclassified or stale entries |
| Formatter round-trip/idempotence including labelled/variadic normalization | Host/property tests, corpus IDs, signature-input contract, successful results |
| Unicode byte and display spans | Scalar/astral/combining/CRLF/interior-offset/EOF tests and results |
| Configured fuzz campaign | Configuration and log locations, target budgets completed, failure disposition, repaired-head rerun |

Run the full [validation sequence](validation.md) on the final head and obtain
CI results for that head. Record environment and logs sufficient to reproduce
each claim. Update the README evidence column and the milestone status only
when all rows pass. Review other active implementation-status prose for stale
Step 4 claims, and update it to precisely the supported M1 boundary. The
integration PR into `main` becomes ready only after this evidence exists.
