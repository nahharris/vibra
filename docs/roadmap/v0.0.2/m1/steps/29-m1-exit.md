# Step 29: m1-exit

Kind: evidence. The exit record of M1: a coverage audit of the corpus
against the owner table, the contract evidence, the recursion and ingress
evidence, the list of gate-program constructions the contract rules
rejected with the repair used, and the pure tasks of the model-writing
pilot. The step adds cases only where the audit finds a gap, and changes no
implementation except to close a defect the audit exposes.

## Prerequisites

- Step 28 landed.
- Read: this milestone's [README](../README.md) "Exit evidence";
  [owner table](../../m0/owners.md); [release README](../../README.md)
  "Scope and priorities" item 3 and "Release gate";
  [diagnostics chapter](../../../../spec/07-diagnostics-and-conformance.md)
  "Required coverage", "Rule identities and example coverage", "Full
  conformance gate"; [type chapter](../../../../spec/02-type-system.md)
  "Guards"; [runtime chapter](../../../../spec/06-runtime.md) "Tail calls",
  "Callable contract execution"; the registered pilot draft that M0 keeps
  current.

## Entry points

| Existing | Use |
| --- | --- |
| `conformance/rules.tsv` (step 02) | the active rule identities |
| `docs/roadmap/v0.0.2/m0/owners.md` | the form-to-case table the audit checks |
| `crates/vibra-conformance/tests/it/spec_examples.rs` (step 08) | every fenced block of the specification read |
| `crates/vibra-conformance/tests/it/` | where the audit test lives |
| `examples/hello/` (step 28) | the directory the gate program's pure part joins |

Proposed: `crates/vibra-conformance/tests/it/owner_coverage.rs`;
`examples/gate-pure/` (the pure modules of the release gate program, as a
library target with tests); `examples/pilot-pure/` (the reference solutions
and controls of the pilot's pure tasks); `docs/roadmap/v0.0.2/m1/exit.md`;
`docs/roadmap/v0.0.2/m1/gate-rejections.md`.

## Ordered tasks

1. Owner audit: a host test reads the owner table and `conformance/cases`
   and fails when a case id named for an M1 form does not exist, when an
   M1 form has no positive case, no negative case where it has a
   diagnostic, or no formatter case where it is a grammar production, and
   when a diagnostic code owned by M1 appears in no case's expectation.
   Gaps are closed by adding cases written from the specification.
2. Both suites: the host suite and the corpus pass on the interpreter; the
   run's per-profile counts are copied into `exit.md`.
3. Contract evidence: `exit.md` lists, by case id, one discharged
   obligation per rule `GUARD-1` to `GUARD-7` and per fact `INT-1` to
   `INT-8`, the refuted and unproved cases with their diagnostics, the
   guard consequence value cases, the handled constant construction error,
   and the host test that shows no run-time check is emitted for a
   discharged bare guard (the lowered callable has no `entry_guard` or
   `exit_guard` node).
4. Recursion and ingress: the deep tail and non-tail recursion cases, and
   one case per ingress path of an invariant-carrying type: constructor,
   `TryFrom` result, a constant, a value passed through an interface
   value, and a rebuild after a pattern. Each shows the invariant checked
   or statically established.
5. Gate program, pure part: write the gate program's pure modules (path
   and line handling, the metric computation, the error types) in
   `examples/gate-pure/` with tests. Every construction the contract rules
   reject while writing it goes into `gate-rejections.md` with the
   predicate, the diagnostic, the rule that would have had to apply, and
   the repair used. A rejection the rules ought to accept is filed as a
   specification question and never repaired by adding a run-time check.
6. Pilot: the reference solutions and controls of the pilot's pure tasks
   in `examples/pilot-pure/`; `vibra check` and `vibra test` pass on them.
7. Status: this milestone's README says **Exited** with a link to
   `exit.md`; every step row says `landed`; the roadmap index lines are
   current.

## Test matrix

| Case or test | Kind | Input | Expected |
| --- | --- | --- | --- |
| `owner_coverage` | boundary, host | the owner table and the corpus | no missing case id, no M1 form without its case kinds, no M1 code without a case |
| `V2-RUNTIME-tail-loop` | boundary, interpret | the case of step 15 (a tail-recursive count of 100,000), with a second input of 10,000,000 iterations added | the count; the host test asserts `max_activation_depth` 1 for both |
| `V2-RUNTIME-non-tail-depth` | boundary, interpret | the case of step 15 (non-tail recursion 100,000 deep) | `100000i32` under the published memory budget |
| `V2-RUNTIME-memory-exhausted` | negative, interpret | the case of step 15 (unbounded non-tail recursion) | the host event `@runtime.memoryExhausted`; not a trap |
| `V2-CONTRACT-ingress-constructor` | positive, interpret | `Positive(0i32)` with `where … else` | `Result.err(error: PositiveError.notPositive())` |
| `V2-CONTRACT-ingress-conversion` | positive, interpret | a `TryFrom(I32)` implementation for `Positive` that returns the constructor's result | the error for 0, the value for 1 |
| `V2-CONTRACT-ingress-constant` | negative | a module value `let p: Positive be Positive(0i32) endlet` with a bare `where` | `@contract.refuted` at the construction |
| `V2-CONTRACT-ingress-interface-value` | positive, interpret | a `Positive` passed and returned through an interface value | the same value; no second check (host test) |
| `V2-CONTRACT-ingress-pattern-rebuild` | negative | `let Positive(raw) be p endlet` then `Positive(I32.sub(raw 1i32))` with a bare `where` | `@contract.unproved` at the second construction |
| `V2-CONTRACT-no-runtime-check` | positive, host | a `fn` with a discharged bare `requires` | the lowered callable has no guard node |
| `gate_pure_tests` | positive, host | `vibra test` in `examples/gate-pure/` | exit 0 |
| `gate_pure_format` | formatter, host | `vibra fmt` on every file of `examples/gate-pure/` and `examples/pilot-pure/` | `changed: false` |
| `pilot_pure_tests` | positive, host | `vibra check` and `vibra test` in `examples/pilot-pure/` | exit 0 |
| `pilot_controls_rejected` | recovery, host | each negative control of the pilot | the diagnostic the pilot registers for it |

A case above that an earlier step landed under the same id is confirmed,
not duplicated.

## Diagnostics and schema changes

Codes: `@runtime.memoryExhausted`, `@contract.refuted`,
`@contract.unproved`. No schema change.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline --workspace --all-targets --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases --executor wasm
cargo run --locked --offline -p vibra-cli --bin vibra -- --workspace examples/gate-pure test
cargo run --locked --offline -p vibra-cli --bin vibra -- --workspace examples/pilot-pure test
```

Read: no failed, unknown, or unavailable required case under `readerV2`,
`staticV2`, or `interpreterV2`; the Wasm row is recorded, not gated; both
`test` runs exit 0.

## Excluded

`proc`, grants, host operations, and the entry of the gate program (M2);
running the pilot (M2); native execution (M4); the `fullV2` claim.

## Completion evidence

- [ ] `docs/roadmap/v0.0.2/m1/exit.md` exists with the per-profile counts, the
      contract list, the recursion and ingress list, and links to the cases.
- [ ] `docs/roadmap/v0.0.2/m1/gate-rejections.md` exists; an empty list is
      stated as empty with the gate modules it covers.
- [ ] `owner_coverage` passes with no exemption list.
- [ ] No step of M1 has an open specification question.
- [ ] Validation commands exit 0.
