# Step 12 — Stage 4A demo and corpus sub-gate

Prerequisite: Step 11 merged. Evidence step: it runs the stage demo and the
sub-gate and claims no language behavior. It changes no specification, and it
fixes a regression found here in the step that owns the behavior, not here.

## Read before editing

- [README](README.md#steps): the Stage 4A demo sentence and the sub-gate rule.
- [Runtime](../../spec/06-runtime.md): **Activations and memory**,
  **Reclamation**; [Diagnostics](../../spec/07-diagnostics-and-conformance.md):
  **Differential execution**.
- [M4 ledger](decision-ledger.md) rows D3.2, D6.4, D12.1 and the
  [inventory](supported-surface.md).

## Scope

Verify, and record in the README, each Stage 4A clause:

1. the M3 demo library's tests produce identical results in both backends;
2. a deep tail-recursive walk grows neither the engine stack nor the arena;
3. the parity inventory has no `not lowered` row and every executable case is
   `matched`;
4. every `Lowered` row of the inventory has a matched case;
5. interpreter-v1 conformance passes for the full pure language; and
6. the module bytes are deterministic and valid under the baseline features.

## Entry points

| File | Use |
| --- | --- |
| `examples/` and the M3 demo library | The programs of the stage demo |
| `conformance/parity.tsv` | The inventory, which must hold only `matched` rows |
| `docs/roadmap/milestone-4/README.md` | Where the evidence is recorded |

## Ordered tasks

1. Establish the baseline from [validation](validation.md) and confirm every
   Stage 4A step is in the integration head by its merge, not its row.
2. Run the demo library's `vibra test` through the interpreter and its modules
   through the harness; compare results and traces.
3. Run the deep tail-recursive walk at two sizes a factor of ten apart; record
   `vibra_v1_live_size` and the engine's peak stack for each.
4. Sweep the inventory and the parity table; list any form still unavailable.
5. Run the full pre-merge list and the fuzz smoke; record counts per profile and
   per backend.
6. Write the evidence section and set the Stage 4A rows to `landed` in the PR
   that completes the step.

## Test matrix

This step adds no tests. It re-runs the matrices of Steps 2–11 and records the
results. A failing item reopens its owning step.

- Positive: the demo and every parity case match.
- Negative: any `not lowered` row, any unmatched inventory row, or any case that
  differs between backends fails the sub-gate.
- Recovery and boundary: the exhaustion case and the 5,000-deep release still
  pass at the final head.
- Formatter: the fmt cases still pass.

## Diagnostic and schema changes

None.

## Validation

The full [pre-merge list](validation.md#before-merging-each-step) at the final
head, with counts per profile and per backend, and `m4_contract_inventory`.

## Excluded

Effects, host operations, the baseline (Stage 4B); any optimization.

## Completion evidence

An evidence section in the README with the base and tested commits, the commands
and exit results, the demo output, the two live sizes, the parity counts, and
the sub-gate verdict.
