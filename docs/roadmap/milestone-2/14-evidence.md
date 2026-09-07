# Step 14 — reproducible M2 demo and exit audit

Requires Step 13. This is an evidence step. Read the complete
[M2 gate](../v1.md#milestone-2--executable-pure-core), the
[coverage table](README.md#deliverable-and-gate-coverage), diagnostics
**Required implementation suites**, and runtime **Tail calls** /
**Determinism and observability**:
[diagnostics](../../spec/07-diagnostics-and-conformance.md),
[runtime](../../spec/06-runtime.md).

## Ordered audit

1. Fetch `origin/m2`; verify each step PR actually merged, its scope is complete,
   and every C1–C12 decision has a canonical specification link. Audit code and
   corpus against the coverage table rather than trusting green status rows.
2. Add an explicit clause/case inventory of the implemented pure subset and all
   deferred-form availability families. Preserve M1's reader example/fuzz
   inventory. Unsupported execution is not a passed conformance observation.
3. Run the [validation sequence](validation.md) on the exact candidate head and
   review all profile counts. Require nonempty reader/static/interpreter groups,
   zero failed/unavailable cases in the declared M2 gate corpus, and no silent
   downgrade, omission, or snapshot auto-acceptance.
4. From a clean exact checkout, prepare the pinned Rust dependencies once, then
   work offline. Use the actual binary to initialize a fresh project, add a
   public helper module and explicit import, preview/apply formatting, check,
   run a pure entry, and run passing tests. Repeat from another empty directory.
5. Add the negative demo: a mismatched primitive argument, private reference,
   unsupported host/dependency surface, and failing assertion each produce the
   specified diagnostic/outcome and nonzero exit without partially executing.
6. Re-run Step 9's source stress at two sizes and record maximum activation depth,
   result, trace and command. Verify pure registry/runtime code cannot observe
   ambient state; confirm repeated program results and empty program event lists.
7. Run architecture/archive checks, schema producer/consumer tests and all three
   platform CI jobs. Review actual output routing, preview/write behavior and
   availability reporting at the CLI boundary.
8. Record evidence in the README and a checked-in evidence report/script under
   this directory. Use relative portable fixture paths, exact input revisions,
   executable commands, exits, counts, depth and output hashes/bytes as relevant.

## Required handoff

Every deliverable/gate row names its cases, host tests and final-head CI link.
Failures stay visible with a concrete owner/fix; Step 14 is not landed with
unresolved gates. Fix small audit defects in this PR with regression evidence;
if a promised slice is materially absent, repair its plan and implementation
explicitly before continuing the gate.

Update `README.md`, `docs/index.md`, `docs/roadmap/v1.md` and affected topic
implementation-status prose to the exact implemented subset. Planning is not
execution evidence; green reader tests alone are not M2 completion. Only after
the evidence passes may the standing `m2 -> main` draft become ready for review.
Review the final head and verify actual merge/ancestry before saying M2 landed
on `main`. No automatic full-v1 or interpreter/Wasm parity claim.
