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

## Implemented evidence harness

The tracked inventory is [`syntax-examples.tsv`](syntax-examples.tsv). The
`evidence_step11` host test scans all active `docs/spec/*.md` chapters, checks
42 fenced fragments and 1,408 inline code spans against stable section
digests, rejects missing or stale rows, and losslessly exercises source and
VIBON fragments. Source examples retain their exact text; the inventory marks
type, effect, runtime, and resolution claims as deferred rather than silently
repairing them. Grammar, schema, CLI, and diagnostic-code illustrations carry
an explicit non-source review-only classification.

The configured harness is [`fuzz/m1.toml`](../../../fuzz/m1.toml), implemented
by the in-tree `m1-fuzz` binary. Its six targets cover raw bytes, source,
VIBON, formatting round trips, structural queries, and mutations of checked-in
inputs. The deterministic campaign uses seed `0x004d315f76315f11`, one worker,
128 iterations per target, and a generated depth limit of 20,000. The CI smoke
profile is 16 iterations per target and is not substituted for the gate
campaign.

The first repaired-head campaign exposed a stack overflow while cloning and
destroying deeply nested VIBON values. The owning slice was repaired by moving
the decoded value into `Document` instead of recursively cloning it and by
draining `DataNode` children iteratively during drop. The focused deep
regression and the complete six-target campaign were rerun after that repair.

## Gate evidence table

| Normative gate | Required evidence |
| --- | --- |
| Reader positive/negative/recovery corpus | Repaired local head: `cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases`; 73 passed, 0 failed, 0 unavailable. Final PR CI pending. |
| Every syntax example classified and exercised | `cargo test --locked --offline -p vibra-conformance --test evidence_step11`; 2 tests passed, including 42 fences and 1,408 inline spans. Final PR CI pending. |
| Formatter round-trip/idempotence including labelled/variadic normalization | Existing formatter host/conformance suites plus `m1-fuzz` roundtrip target: 128/128 passed. Final PR CI pending. |
| Unicode byte and display spans | Existing scalar/astral/combining/CRLF/interior-offset/EOF tests plus query target: 128/128 passed. Final PR CI pending. |
| Configured fuzz campaign | `target/step11/m1-fuzz-campaign.log`; `m1.toml` campaign command; six targets × 128 = 768 passed after the deep-data repair. Final PR CI pending. |

Run the full [validation sequence](validation.md) on the final head and obtain
CI results for that head. Record environment and logs sufficient to reproduce
each claim. Update the README evidence column and the milestone status only
when all rows pass. Review other active implementation-status prose for stale
Step 4 claims, and update it to precisely the supported M1 boundary. The
integration PR into `main` becomes ready only after this evidence exists.
