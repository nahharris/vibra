# Step 08: operation-interfaces

Kind: vertical. Slice 1 and the declaration half of slice 2: operation
interfaces as effect roots, rows as written, member rows, implementations
that keep them, and plans that carry a complete row. Awaiting a plan with a
row is step 09.
Depends on: 07.
Crates: `vibra-types`, `conformance/cases`.

## Prerequisites

- Read: [effects chapter](../../../../spec/03-effects.md#operation-interfaces)
  "Operation interfaces", "Effect ceilings"; [source
  chapter](../../../../spec/01-source-language.md#grammar) the productions
  `performs`, `row`, `interface-decl`, `impl`; [type
  chapter](../../../../spec/02-type-system.md#implementations)
  "Implementations", "Dispatch"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md#contract-effect-and-authority-diagnostics)
  the rows of `@effect.duplicateRoot` and `@effect.invalidReference`.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-types/src/check/interfaces.rs:390` | `member.performs().is_some()` | calls `effects::members::contract_member` (step 04) |
| `crates/vibra-types/src/check/interfaces.rs:1039` | `CallableKind::Proc` | calls `effects::members::impl_member` |
| `crates/vibra-types/src/check/declare.rs:1684` | `data.view.performs().is_some()` | calls `effects::procs::header`, which calls `effects::rows::resolve` |
| `crates/vibra-ir/src/types.rs:152` | `pub struct Row` | the resolved row |

Proposed: the bodies of `effects::rows::{resolve, is_operation_interface}`
and `effects::members::{contract_member, impl_member}`.

## Files owned

- `crates/vibra-types/src/check/effects/rows.rs`
- `crates/vibra-types/src/check/effects/members.rs`
- `crates/vibra-types/src/check/effects/procs.rs`
- `crates/vibra-types/src/check/tests/rows.rs`
- `conformance/rules/m2-08.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `is_operation_interface`: an interface that declares at least one
   `proc` member, abstract or default. Invariant: the identity of a root
   is the interface's declaration identity; an import name never
   identifies it.
2. `rows::resolve(row)`: each entry is a path that MUST resolve to an
   operation interface, else `@effect.invalidReference` at the entry,
   including a path that resolves to nothing. A second entry for one
   interface, under any spelling, is `@effect.duplicateRoot` at it,
   related to the first. The result is a `Row`, sorted by path bytes.
3. `procs::header` and `procs::test_header` accept a non-empty row
   through `resolve`.
4. `contract_member`: a `proc` member of an interface, abstract or with a
   body, has the complete row `{owner} ∪ performs`. The owner written in
   the member's own `performs` is `@effect.duplicateRoot` at that entry.
   A `fn` member of an operation interface is pure, as before.
5. `impl_member`: a `proc` member of an `impl` is compared with its
   contract member under the substitution, its written `performs`
   included: a differing row is `@type.mismatch` at the member's
   `performs` section, or at its name when it has none.
6. An application of a contract `proc` member, at a closed receiver, a
   type parameter, or an interface value, has the type `Intent(R {Row})`
   with the member's complete row.
7. Lower all of it. `effects::analysis::check_bodies` still rejects an
   `await` of a plan with a non-empty row (step 04).

## Positions

| Rule | Positions |
| --- | --- |
| a row entry resolves to an operation interface | `performs` of a module-level `proc`; of a `proc` member of a type; of an interface member; of an `impl` member; of a `test` |
| no interface twice | two spellings in one row; the owner in a member's own row; in an `impl` member's row |
| a member keeps its row | an `impl` nested in a type; an `impl` nested in an interface; a generic `impl` |

## Test matrix

`workspaceCheck`, profile `staticV2`, unless a row names another operation,
which has that operation's profile (Q34). `L`
abbreviates:

```vibra
interface Log
  proc append
    takes {log: Self entry: Str}
    returns done: Void
  endproc
endinterface

interface Clock
  proc now takes {clock: Self} returns at: I64 endproc
endinterface

interface Shown
  fn show takes {value: Self} returns text: Str endfn
endinterface
```

| Case id | Kind | Input, after `L` | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-EFFECT-operation-interface` | positive | `L` alone | accepted; `@types.v2` lists `Log.append` as `Proc({Self Str} Void {Log})` | 03 Operation interfaces: "The complete row of an operation member is its owning interface plus the member's written `performs`" |
| `V2-EFFECT-member-additive-row` | positive | `interface Audit proc record takes {a: Self entry: Str} returns done: Void performs {Log} endproc endinterface` | accepted; the member's row is `{Audit Log}` | the same |
| `V2-EFFECT-member-owner-in-row` | negative | the same with `performs {Audit}` | `@effect.duplicateRoot` at `Audit` in the row | 03: "Writing the owner in a member's own `performs` is `@effect.duplicateRoot`" |
| `V2-EFFECT-fn-member-pure` | positive | `interface Both proc go takes {b: Self} returns d: Void endproc fn name takes {b: Self} returns n: Str endfn endinterface` and a `fn` that calls `Both.name` | accepted | 03: "An operation interface may also declare `fn` members; they are pure" |
| `V2-EFFECT-row-on-proc` | positive | `proc p takes {m: Memory} returns d: Void performs {Log} do let _ be Log.append(m "x") endlet return void endproc` | accepted, with the warning of step 09 absent until then | 03 Effect ceilings: "A row is written with `performs` on a `proc`" |
| `V2-EFFECT-row-two-roots` | positive | `performs {Log Clock}` | accepted; the type's row is sorted by path | 03 Effect ceilings: "sorted by the UTF-8 bytes of their `path`" |
| `V2-EFFECT-row-not-operation-interface` | negative | `performs {Shown}` | `@effect.invalidReference` at `Shown` | 03 Operation interfaces: "An interface with only `fn` members is not a root" |
| `V2-EFFECT-row-names-type` | negative | `performs {Memory}` | `@effect.invalidReference` at `Memory` | 03 Effect ceilings: "A name that resolves to anything else, or to nothing, is `@effect.invalidReference`" |
| `V2-EFFECT-row-names-nothing` | negative | `performs {Nope}` | `@effect.invalidReference` at `Nope` | the same |
| `V2-EFFECT-row-duplicate` | negative | `performs {Log Log}` | `@effect.duplicateRoot` at the second, related to the first | 03 Effect ceilings: "A row that names one interface twice, under any spelling" |
| `V2-EFFECT-row-duplicate-alias` | negative | two modules; `use @app.log.Log as Journal enduse` and `performs {log.Log Journal}` | `@effect.duplicateRoot` at `Journal` | the same: "under any spelling" |
| `V2-EFFECT-row-on-test` | positive, `workspaceTest` | `test "row" performs {Log} do assert.true(true) endtest` | `@test.passed` | 03 Effect ceilings: "and a `test`" |
| `V2-EFFECT-row-on-test-invalid` | negative | `test "row" performs {Shown} do endtest` | `@effect.invalidReference` | the same |
| `V2-EFFECT-impl-keeps-row` | positive | `type Memory of {lines: List(Str)} impl Log proc append takes {log: Self entry: Str} returns done: Void do return void endproc endimpl endtype` | accepted | 03: "It MUST keep each member's row, types, and guards" |
| `V2-EFFECT-impl-row-wider` | negative | the `impl` member with `performs {Clock}` | `@type.mismatch` at the `performs` section | the same; 07: "an implementation member whose signature differs from the contract" |
| `V2-EFFECT-impl-row-narrower` | negative | an `impl Audit` whose `record` omits `performs {Log}` | `@type.mismatch` at the member's name | the same |
| `V2-EFFECT-impl-owner-in-row` | negative | the `impl Log` member with `performs {Log}` | `@effect.duplicateRoot` at `Log` | 03: "Writing the owner in a member's own `performs`" |
| `V2-EFFECT-impl-fn-for-proc` | negative | the `impl Log` member written as a `fn` | `@type.mismatch` at the member | 03: "It MUST keep each member's row, types, and guards" |
| `V2-EFFECT-impl-in-interface` | positive | `interface Log … impl Log for a second type nested in another interface` as the type chapter allows | accepted | 02 Implementations |
| `V2-EFFECT-member-plan-closed-receiver` | positive | `fn build takes {m: Memory} returns unit: Void do let _ be Log.append(m "x") endlet return void endfn` | accepted: a `fn` may build the plan | 03 Effect ceilings: "A `fn` performs nothing. It may build and return a plan" |
| `V2-EFFECT-member-plan-type-parameter` | positive | `fn build(T) takes {m: T} given {T: Log} …` building `Log.append(m "x")` | accepted | 02 Dispatch |
| `V2-EFFECT-member-plan-interface-value` | positive | `fn build takes {m: Log} …` building `Log.append(m "x")` | accepted | 03 Cold plans: "A plan's type always carries its complete row, including through an interface value" |
| `V2-EFFECT-default-proc-member` | positive | `interface Log … proc appendTwice takes {log: Self entry: Str} returns done: Void do let _ be Log.append(log entry) endlet return void endproc endinterface` | accepted; its row is `{Log}` | 03 Operation interfaces |
| `V2-EFFECT-generic-operation-interface` | positive | `interface Store(T) proc put takes {s: Self value: T} returns d: Void endproc endinterface`; a `proc` with `performs {Store}` and parameters `Store(Str)` and `Store(I32)` | accepted: one root | 03: "Every application of a generic operation interface is the same root" |
| `V2-TOOL-unavailable-await-with-row` | boundary | `proc p takes {m: Memory} returns d: Void performs {Log} do return await Log.append(m "x") endproc` | `@tool.unavailable` at the `await`; verdict `unsupported` | 07: "a valid … construct outside the tool's profile" |

## Diagnostics and schema changes

Codes: `@effect.invalidReference`, `@effect.duplicateRoot`,
`@type.mismatch`, `@tool.unavailable`. Schema: `@types.v2` callable types
carry rows, as "In metadata a row is a record with a `roots` list" states.

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

Read: all exit 0; the 25 cases above pass.

## Stays unavailable

`@tool.unavailable`: an `await` of a plan with a non-empty row; a written
`Proc` or `Intent` type; an anonymous `proc`; a `proc` or a contract
`proc` member named as a value; a test with `takes`; `external @host`
members, which only the library of step 19 declares.

## Excluded

Ceilings and witnesses (09); callable types (10); the standard interfaces
(19).

## Completion evidence

- [ ] `conformance/rules/m2-08.tsv` has one `active` row per case id.
- [ ] Every position of the table has a positive and a negative case; the
      report maps each.
- [ ] `V2-TOOL-unavailable-await-with-row` shows the fail-closed boundary.
- [ ] The validation commands exit 0.
