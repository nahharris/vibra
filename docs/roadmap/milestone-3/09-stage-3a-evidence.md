# Step 9 — Stage 3A demo and corpus sub-gate

Prerequisite: Step 8 merged. Evidence step: it claims no new behavior.

## Stage demo

Build a small checked-in example project whose library parses a line-oriented
format (for example `key = value` records with typed fields) into a nominal
record/enum model, reports every failure through a nominal error union with
`try`, and is tested exhaustively with `assert.equal`. It uses no interface,
no compiler-private escape hatch, and only public standard-library modules.
Run `vibra check`, `vibra test`, and `vibra run` on it from a clean checkout
with no network, and record the commands, exits, and outputs.

## Sub-gate

- Every Stage 3A inventory row references at least one positive and one
  negative corpus case, or states why a negative case is impossible.
- No Stage 3A form reports `@tool.unavailable`; every Stage 3B form still does,
  with a case.
- The corpus reports zero failed and zero unavailable; counts per profile are
  recorded.
- Each new diagnostic code from Step 1 has a focused case with its exact span.
- The Stage 3A clauses of the conformance chapter's coverage paragraphs each
  map to a case ID in an evidence table in this directory.

## Done

The README records the demo, commands, counts, and the evidence table, and the
Stage 3A rows are `landed`. Step 10 may then begin.
