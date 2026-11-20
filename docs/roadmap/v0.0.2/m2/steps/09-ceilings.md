# Step 09: ceilings

Kind: vertical. Slice 1: the performed-row analysis. A body's row is
computed, compared with its ceiling, and explained by a witness chain; an
unused root is a warning. Awaiting a plan with a row becomes available, and
a user-implemented operation interface runs under a test.
Depends on: 08.
Crates: `vibra-types`, `conformance/cases`.

## Prerequisites

- Read: [effects chapter](../../../../spec/03-effects.md#effect-ceilings)
  "Effect ceilings", "Cold plans and performed rows"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md#contract-effect-and-authority-diagnostics)
  the rows of `@effect.outsideCeiling` and `@contract.unusedEffect`, and
  the paragraph after the table on related spans;
  [questions](../spec-questions.md) Q5.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-types/src/check/lower.rs:868` | `pub(crate) fn check_program` | calls `effects::analysis::check_bodies` (step 04) |
| `crates/vibra-types/src/check/expr.rs:1368` | `ExprForm::Await` | records each `await` with its operand's `Intent` type |
| `crates/vibra-ir/src/types.rs:152` | `pub struct Row` | set operations on rows |

Proposed: `effects::analysis::{Performed, Witness, check_bodies}` as the
architecture gives them.

## Files owned

- `crates/vibra-types/src/check/effects/analysis.rs`
- `crates/vibra-types/src/check/tests/ceilings.rs`
- `conformance/rules/m2-09.tsv`
- the case directories of the matrix below

## Ordered tasks

1. For each body with a procedure context (a `proc`, a default or `impl`
   `proc` member, a test) collect its `await` nodes in source order with
   the row of each operand's `Intent` type. The performed row is their
   union. Invariant: an application that is not awaited contributes
   nothing.
2. The ceiling of a `proc` or test is its written row; of a contract or
   `impl` member, the member's complete row. For each performed root
   outside the ceiling report one `@effect.outsideCeiling` at the earliest
   `await` that contributes it. Invariant: once per body and interface.
3. Related spans: the shortest chain of calls from that `await` to an
   operation member of the root, ties in source order, each with the note
   `performs <root> via <callee>`; a hop through a callback parameter or
   an interface member is marked indirect. The search uses an explicit
   work list over the call graph and never the host stack.
4. `@contract.unusedEffect`, a warning at the row entry, for a root of a
   written row that the body does not perform: on a `proc`, a test, and
   the additive `performs` of a default member; never on an `impl` member
   or on a member's owner root (Q5).
5. Remove the fail-closed rejection of step 04: an `await` of a plan with
   a non-empty row is checked, not unavailable.

## Positions

| Rule | Positions |
| --- | --- |
| performed row within the ceiling | module-level `proc`; `proc` member of a type; default member of an interface; `impl` member; test body |
| what contributes | a direct member await; an await of a `proc` with a row; an await of a stored plan; an await inside `if`, `match`, and a `let` initializer |
| unused root | `proc`; test; additive row of a default member; not: `impl` member, owner root |

## Test matrix

`workspaceCheck`, profile `staticV2`, unless a row says `workspaceTest`.
`L` is the preamble of step 08 with:

```vibra
type Memory of {lines: List(Str)}
  impl Log
    proc append takes {log: Self entry: Str} returns done: Void do
      return void
    endproc
  endimpl
  impl Clock
    proc now takes {clock: Self} returns at: I64 do return 0i64 endproc
  endimpl
endtype

proc note takes {m: Memory} returns done: Void performs {Log} do
  return await Log.append(m "x")
endproc
```

| Case id | Kind | Input, after `L` | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-EFFECT-ceiling-exact` | positive | `L` alone | accepted, no warning | 03 Effect ceilings: "requires it to be a subset of the ceiling" |
| `V2-EFFECT-ceiling-transitive` | positive | `proc outer takes {m: Memory} returns d: Void performs {Log} do return await note(m) endproc` | accepted | 03 Cold plans: "`await plan` contributes the plan's complete row" |
| `V2-EFFECT-outside-ceiling-direct` | negative | `proc bad takes {m: Memory} returns d: Void do return await Log.append(m "x") endproc` | `@effect.outsideCeiling` at the `await`, one related span `performs Log via Log.append` | 03 Effect ceilings: "it reports one `@effect.outsideCeiling` at the earliest `await` that contributes it" |
| `V2-EFFECT-outside-ceiling-transitive` | negative | `proc bad takes {m: Memory} returns d: Void do return await note(m) endproc` | `@effect.outsideCeiling` at the `await`; related: `note`, then `Log.append` | 07: "the shortest remaining chain of calls" |
| `V2-EFFECT-outside-ceiling-earliest` | negative | a `proc` with the empty row and two awaits of `Log.append` | one diagnostic, at the first | 07: "reported once per body and interface" |
| `V2-EFFECT-outside-ceiling-two-roots` | negative, two faults by rule | a `proc` with the empty row awaiting `Log.append` then `Clock.now` | two diagnostics, one per interface, in source order | the same |
| `V2-EFFECT-outside-ceiling-wrong-root` | negative | `performs {Clock}` on a body that awaits only `Log.append`, with `let _ be await Clock.now(m) endlet` first | `@effect.outsideCeiling` for `Log` only | 03 Effect ceilings: "no relation between roots" |
| `V2-EFFECT-outside-ceiling-stored-plan` | negative | `let plan be Log.append(m "x") endlet` then `await plan` in a `proc` with the empty row | `@effect.outsideCeiling` at the `await`, not at the `let` | 03 Cold plans: "Building a plan contributes only what evaluating its arguments performs" |
| `V2-EFFECT-outside-ceiling-interface-value` | negative | `proc bad takes {l: Log} returns d: Void do return await Log.append(l "x") endproc` | `@effect.outsideCeiling`; the related hop is marked indirect | 07: "a hop through a callback or interface member is marked indirect" |
| `V2-EFFECT-outside-ceiling-in-if` | negative | the await inside an `if` branch of a `proc` with the empty row | `@effect.outsideCeiling` at that `await` | 03 Effect ceilings |
| `V2-EFFECT-outside-ceiling-type-member` | negative | a `proc` member of `Memory` with the empty row that awaits `note(self)` | `@effect.outsideCeiling` | the same |
| `V2-EFFECT-outside-ceiling-default-member` | negative | a default member of `Log` that awaits `Clock.now` of a `Clock` parameter, with no `performs` | `@effect.outsideCeiling` for `Clock` | 03 Operation interfaces: the complete row |
| `V2-EFFECT-impl-forwards-wider` | negative | an `impl Log` member whose body awaits `Clock.now(log)` | `@effect.outsideCeiling` for `Clock` at the `await` | 03: "It MUST keep each member's row" |
| `V2-EFFECT-outside-ceiling-test` | negative | `test "t" do let _ be await main.note(m) endlet endtest` | `@effect.outsideCeiling` at the `await` | 03 Tests and model profiles: "the test's row MUST also name whatever it then performs" |
| `V2-EFFECT-recursion-closes` | positive | `proc loop takes {m: Memory n: I32} returns d: Void performs {Log} do if Ordered.greater(n 0i32) then return await loop(m I32.sub(n 1i32)) endif return await Log.append(m "x") endproc` | accepted | 03 Cold plans: "the row analysis closes such cycles" |
| `V2-EFFECT-mutual-recursion-outside` | negative | `proc a` with `performs {Log}` awaits `b`; `proc b` with the empty row awaits `a` | one `@effect.outsideCeiling`, in `b` | the same |
| `V2-CONTRACT-unused-effect-proc` | positive with warning | `proc idle takes {m: Memory} returns d: Void performs {Log} do return void endproc` | accepted; `@contract.unusedEffect` at `Log` | 03 Effect ceilings: "An interface in a ceiling that the body cannot perform is the warning" |
| `V2-CONTRACT-unused-effect-one-of-two` | positive with warning | `performs {Log Clock}` on a body that awaits only `Log.append` | the warning at `Clock` only | the same |
| `V2-CONTRACT-unused-effect-plan-only` | positive with warning | `performs {Log}` on a body that builds `Log.append(m "x")` and discards it | the warning at `Log` (Q5) | 03 Cold plans: "Building a plan contributes only…" |
| `V2-CONTRACT-unused-effect-test` | positive with warning, `workspaceTest` | `test "t" performs {Log} do assert.true(true) endtest` | `@test.passed`; the warning in the check diagnostics | 03 Effect ceilings |
| `V2-CONTRACT-unused-effect-impl-member` | positive | the `impl Log` of `L`, whose `append` awaits nothing | accepted, no warning (Q5) | 03: "It MUST keep each member's row" |
| `V2-CONTRACT-unused-effect-default-owner` | positive | a default member of `Log` whose body returns `void` | accepted, no warning (Q5) | 03 Operation interfaces |
| `V2-RUNTIME-model-impl-runs` | positive, `workspaceTest` | a `Counter` type implementing `Clock` with `now` returning `7i64`; `test "model" performs {Clock} do assert.equal(7i64 await Clock.now(counter)) endtest` | `@test.passed`; the trace has no event | 03 Tests and model profiles: "A package may implement an operation interface in Vibra" |
| `V2-RUNTIME-model-impl-interface-value` | positive, `workspaceTest` | the same through a `fn` parameter of type `Clock` | `@test.passed` | 03 Cold plans: "including through an interface value" |

`V2-TOOL-unavailable-await-with-row` of step 08 is deleted.

## Diagnostics and schema changes

Codes: `@effect.outsideCeiling`, `@contract.unusedEffect`. The related
spans carry the note text the chapter fixes. No schema change.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-workspace -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0. Every accepted case also passes `vibra-ir::effects`: a
case the checker accepts and the IR rejects appears as
`@runtime.invalidCheckedProgram` and is a defect of this step or of
step 03.

## Stays unavailable

`@tool.unavailable`: written `Proc` and `Intent` types, anonymous `proc`,
procedure values, a test with `takes`. The query expansion
`effectWitnesses` belongs to the query command, which this release's
owner table assigns outside M2.

## Excluded

Callback rows (10); the target's admission ceiling (25).

## Completion evidence

- [ ] `conformance/rules/m2-09.tsv` has one `active` row per case id.
- [ ] A unit test builds a 10,000-procedure call chain and gets its
      witness without overflow on a 256 KiB thread.
- [ ] For each negative case the report shows that the same lowered
      program, built by hand, is rejected by `vibra-ir::effects`.
- [ ] The validation commands exit 0.
