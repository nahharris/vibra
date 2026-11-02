# Step 22 — M4 demo and exit gate

Prerequisite: Step 21 merged. Evidence step: it runs the demo gate and sweeps the
exit gate and the M3 deferrals, and claims no language behavior. It changes no
specification. A regression found here is fixed in the step that owns the behavior,
never here.

## Read before editing

- [Roadmap](../v1.md#milestone-4--webassembly-spine-static-effects-and-host-operations):
  the demo gate and the exit gate.
- [README](README.md#deliverable-and-gate-coverage) and the
  [M3 deferral inventory](README.md#m3-deferral-inventory).
- [Stage 4A evidence](stage-4a-evidence.md) for the form of the record.
- [Effects](../../spec/03-effects.md), [Runtime](../../spec/06-runtime.md), and
  [Tooling](../../spec/05-tooling.md) for the clauses the demo exercises.
- [Validation](validation.md).

## Scope

Verify, and record in the README, each clause:

1. **Demo gate.** A target declaring `@std.fs.read`, `@std.env.read`, and
   `@std.io.stdout` reads a path given as `vibra run TARGET -- ARG...`, transforms
   the content, and writes to standard output. The same source is rejected when its
   function ceiling omits a performed root, and when its target array does, each with
   the witness. No path grant, prompt, or runtime effect check participates. The
   accepted program produces byte-identical standard output and audit traces in both
   backends (a corpus case in a host world) and runs against the real filesystem
   through `vibra run` in a process test.
2. **Exit clauses.** An additive root reaches the performed row, the rejection, and
   the target array of every transitive caller, with a witness for each missing root;
   every host operation has one registry entry, owner, signature, and event shape
   (the snapshot test); no compiler-generated ambient host read exists outside the
   registry (the ambient-read test); success, typed host error, propagation, and trap
   have parity; deep non-tail recursion and the bounded live arena still hold; and
   interpreter-v1 conformance passes with the Wasm backend matching on every
   executable case.
3. **M3 deferrals.** Every row I1–I19 of the surface inventory has a landed owner and
   no M4-owned form is still `@tool.unavailable`.
4. The baseline of Step 21 is current.

## Entry points

| File | Use |
| --- | --- |
| `examples/` | The demo program and its world |
| `conformance/parity.tsv` | Only `matched` rows |
| `crates/vibra-conformance/tests/m4_contract_inventory.rs`, `architecture_boundary.rs` | The inventory and the dependency rules |
| `docs/roadmap/milestone-4/README.md` | Where the evidence is recorded |

## Ordered tasks

1. Establish the baseline from [validation](validation.md) and confirm that every
   Stage 4B step is in the integration head by its merge, not its row.
2. Run the demo in the interpreter, in a module through the harness, and through the
   process; compare standard output and traces.
3. Run the rejection variants and record the diagnostics and witnesses.
4. Run the exit-clause tests and sweep the inventory and the parity table.
5. Run the full pre-merge list and the fuzz smoke at the final head; record counts
   per profile and per backend.
6. Write the evidence section, set the Stage 4B rows to `landed`, and mark the draft
   PR from `m4` to `main` ready only when every clause is evidenced.

## Test matrix

This step adds no tests beyond the demo's cases. It re-runs the matrices of Steps
2–21.

- Positive: the demo and every parity case match.
- Negative: a rejected variant, any `not lowered` or `unavailable` count, or any case
  that differs between backends fails the gate.
- Recovery and boundary: the exhaustion and 5,000-deep release cases still pass.
- Formatter: the fmt cases still pass.

## Diagnostic and schema changes

None.

## Validation

The full [pre-merge list](validation.md#before-merging-each-step) at the final head,
with counts per profile and per backend, plus the parity inventory and the inventory
test.

## Excluded

Anything M5 or later: dependency delivery, the public `query` command, build
products, and optimization.

## Completion evidence

An evidence section in the README with the base and tested commits, the commands and
exit results, the demo output, the rejection diagnostics, the parity counts, and the
exit-gate verdict.
