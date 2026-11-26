# Step 08: semantic-facts

Kind: vertical. The checker records one fact per node where it decides it:
role, context, expected and observed type, the identity bound or referred
to, the application or pattern fact, the fact's status. Nothing reads them
yet but the checker's own tests; step 12 projects them.
Depends on: 01.
Crates: `vibra-types`.

## Prerequisites

- Read: [05 Structural position and semantic envelope](../../../../spec/05-tooling.md#structural-position-and-semantic-envelope)
  (roles, contexts, locals, identities, the application and pattern facts);
  [07 Recovery](../../../../spec/07-diagnostics-and-conformance.md#recovery)
  ("Each fact is `exact`, `recovered`, or `unavailable` on its own");
  [architecture](../architecture.md) "Boundary types" (Facts), risk 3.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-types/src/check/expr.rs:527` | `pub(crate) struct BodyChecker` | the checker of bodies; the hook of step 01 is called where a type is final |
| `crates/vibra-types/src/check/pattern.rs:307` | `pub(crate) fn match_coverage` | patterns and their bindings |
| `crates/vibra-types/src/check/declare.rs:779` | `pub(crate) fn declare` | headers |
| `crates/vibra-types/src/check/mod.rs:66` | `pub struct CheckResult` | `facts()` |
| `crates/vibra-resolve/src/snapshot.rs:180` | `pub struct VisibleLocals` | the locals the resolver already records per position |

Proposed: the body of `check::facts`.

## Files owned

- `crates/vibra-types/src/check/facts.rs`
- `crates/vibra-types/src/check/tests/facts.rs`

## Ordered tasks

1. `NodeFact` as the architecture shapes it; `Facts` keyed by source and
   span; recording through the three hooks of step 01 only. Invariant: the
   shared checker files make no fact decision; a fact is recorded once per
   node.
2. Roles by slot: `@atomValue`, `@entityReference`, `@codeReference`,
   `@literal`, `@localBinding`, `@discard`, `@declaration`, `@application`,
   `@unknown`; contexts by enclosing form (the fifteen of the chapter).
3. Types: `expected` with whether it was written, `observed`; both open
   type encodings over the enclosing signature's binders; a declared type
   by path and identity, never expanded.
4. Identities: a declaration's canonical identity; a local as
   `binder:<sourceId>:<start>-<end>`; `_` none.
5. Application fact `{kind, callee, calleeType, positional, labels,
   resultType, dispatch}` with the six kinds; `dispatch` `{contract,
   member, receiver, selection, destination}`; a `proc` application has
   kind `@procPlan` and an `await` a fact of its own with the plan's row.
   Pattern fact `{kind, scrutineeType, bindings, constructor}`.
6. Status: `recovered` for a node inside a body that has an error, or
   whose type the checker could not fix; `exact` otherwise. A rejected
   body's row facts are not recorded.
7. Measure: the checker's time and memory on the three largest corpus
   trees with and without recording, a host test asserting less than one
   fifth more of each.

## Positions

| Rule | Positions, each with a positive and a negative row |
| --- | --- |
| a fact per node | an expression; a pattern; a header; a `let` binder; a labelled argument; a `performs` entry; against: a node in a rejected body (recovered) |

## Test matrix

Host tests over `check_source` and small workspaces.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| literal role | positive | `1i32` as an argument | role `@literal`, context `argument`, observed `@i32`, expected `@i32` written false | 05: "A role comes from the grammar or schema slot" |
| atom value against entity reference | positive | `@std.io` in an expression and in a `use` | `@atomValue` then `@entityReference` | 05: "`@std.io` is an atom value in an expression and an entity reference in an import" |
| local binding identity | positive | `let x be 1i32 endlet` then `x` | the binder's identity `binder:src/app/main.vib:<start>-<end>` on both nodes | 05: "A local uses `binder:<sourceId>:<start>-<end>`" |
| discard | positive | `let _ be f() endlet` | role `@discard`, no identity | 05: "`_` has the discard role and no identity" |
| expected written | positive | `let x: I32 be 1i32 endlet` | the initializer's expected type written true | 05: "whether the expected type was written" |
| application fact | positive | `double(n)` | kind `@function`, callee `double`'s identity, `positional: [@i32]`, `resultType: @i32` | 05 Application fact |
| plan fact | positive | `double(n)` with `double` a `proc`, then `await` of it | `@procPlan` on the application; the `await` node's fact has the plan's type and row | 05: "Applying a `proc` member builds a plan and performs nothing. An `await` is reported separately" |
| constructor fact | positive | `User(name: "a" id: UserId(1u64))` | `@constructor`, `resultType` `User`; with a `where … else` type, `Result(T E)` | 05: "A constructor's `resultType` is `T` for a bare `where` and `Result(T E)` for `where … else`" |
| dispatch fact | positive | `Ordered.compare(a b)` at `I32` | `dispatch.selection: @static`, `destination: false`; a `From` destination call has `destination: true` | 05 Application fact, `dispatch` |
| lookup and projection | positive | `xs(0u64)` and `user.name` | `@collectionLookup`, `@recordProjection` | 05: the six kinds |
| pattern fact | positive | `case Shape.circle(radius: r)` | kind `@variant`, bindings `[r: @f64]`, constructor `circle`'s identity | 05 Pattern fact |
| contexts | positive | one node in each of the fifteen contexts | the context named | 05: the context list |
| recovered beside exact | positive | two declarations, one with a type error | the sound one's facts are `exact`, the broken body's `recovered` | 07 Recovery: "A valid node beside a recovery stays exact" |
| no fact outside the hooks | negative | `git grep` for `facts::record` | three call sites | architecture, "The seams of step 01" |
| cost | boundary | the three largest corpus trees | time and memory within one fifth of the baseline | architecture, risk 3 |

## Diagnostics and schema changes

None.

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

Read: all exit 0; the corpus is unchanged.

## Stays unavailable

Every semantic include: nothing projects the facts yet.

## Excluded

The projection (12); effects (15).

## Completion evidence

- [ ] Each row is a test, named in the report; the cost row's numbers are
      in the report.
- [ ] `git diff --stat` names only the two files owned.
- [ ] The validation commands exit 0.
