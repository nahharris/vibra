# Step 16 — M3 demo and exit gate

Prerequisite: Step 15 merged. Evidence step: it claims no new behavior.

## Demo

Extend or replace the Stage 3A demo so it also uses Stage 3B:
- an interface with a default member implemented by two nominal types;
- a generic function bounded by that interface;
- a map keyed by a user type through its own `ordered`;
- a conversion selected by a written destination;
- an `iter` pipeline with `map`, `filter`, and `collect`.

Run `vibra check`, `vibra test`, `vibra run`, and the `@index.v1` query on
it from a clean checkout with no network, and record the commands, exits, and
outputs.

## Exit gate

- Every inventory row is `M2`, `M4`, or landed with a positive and a negative
  case, or a stated reason a negative is impossible. No Stage 3B form reports
  `@tool.unavailable`.
- The corpus reports zero failed and zero unavailable, and counts per profile
  are recorded.
- Every conformance-chapter coverage clause owned by M3 maps to case IDs, in an
  evidence table beside the Stage 3A one.
- **M2 deferral sweep:** every row of the M2 ledger deferred to M3 is
  implemented, with evidence, or re-deferred by name to a later milestone.
- Every gap in the README is closed or named as a later milestone's decision.
  This includes G21, the command result for a failing entry.
- Stable diagnostics exist for ambiguous inference, shadowing, non-exhaustive
  matches, invalid `impl` placement, and ignored fallible values, as the
  roadmap exit requires.

## Done

The README records the demo, commands, counts, and evidence tables, every step
row is `landed`, and the `m3` → `main` pull request is ready to merge.
