# Step 10: callable-rows

Kind: vertical. Slice 1 completed: `Proc` and `Intent` types as written,
procedures as values, anonymous `proc`, exact row comparison, and a
callback's contribution to the row of the body that awaits it.
Depends on: 09.
Crates: `vibra-types`, `conformance/cases`.

## Prerequisites

- Read: [type chapter](../../../../spec/02-type-system.md#functions-as-values)
  "Functions as values", "Guards and callable types", "Application";
  [source chapter](../../../../spec/01-source-language.md#grammar) the
  productions of `Proc`, `Intent`, and the anonymous callable, and the
  sentence on `@type.suspiciousRow`; [effects
  chapter](../../../../spec/03-effects.md#effect-ceilings) "Effect
  ceilings"; [questions](../spec-questions.md) Q6;
  [callable rulings](../../m1/README.md).

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-types/src/check/declare.rs:1212` | `TypeForm::Proc` | calls `effects::values::written_type` (step 04) |
| `crates/vibra-types/src/check/expr.rs:1865` | `fn start_closure` | calls `effects::values::anonymous` |
| `crates/vibra-types/src/check/expr.rs:2016` | `fn closure_instance` | the closure a named `proc` value becomes |
| `crates/vibra-ir/src/expr.rs:574` | `pub struct ClosureBody` | an anonymous `proc` lowers to a closure of a `Proc` type |

Proposed: the bodies of `effects::values::{written_type, anonymous,
named_value}`.

## Files owned

- `crates/vibra-types/src/check/effects/values.rs`
- `crates/vibra-types/src/check/tests/values.rs`
- `conformance/rules/m2-10.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `written_type`: `Proc({inputs} R {Row})` and `Intent(R {Row})`, the row
   optional and resolved by `effects::rows::resolve`. Invariant: an
   omitted row is the empty row, never an unknown one.
2. `@type.suspiciousRow`, a warning at the type, when the row is omitted
   and the result operand is a tuple with at least one component, each an
   operation interface (Q6).
3. `named_value`: a module-level `proc`, a `proc` member of a type, and a
   contract `proc` member at a closed receiver, named without application
   where a `Proc` type is expected, is a value of its `Proc` type under
   the rules `fn` values follow: no labelled parameter that lacks a
   default survives, guards with `else` travel with the value, a bare
   `requires` is refused as the type chapter states for `fn`.
4. `anonymous`: `proc takes … returns … performs {Row} do … endproc` is a
   closure of a `Proc` type with its written row as ceiling; its body is a
   procedure context analysed by step 09; captures follow the rules of an
   anonymous `fn`. An anonymous `fn` with `performs` is
   `@syntax.invalidSection`, as the reader already reports.
5. Callable types compare rows exactly, at arguments, returns, `let`
   annotations, fields, and `impl` members.
6. Awaiting an application of a callback parameter contributes the
   parameter type's row, marked indirect in a witness.

## Positions

| Rule | Positions |
| --- | --- |
| a `Proc` or `Intent` type is written | parameter; result; `let` annotation; record field; type argument; alias |
| rows compare exactly | argument to a parameter; returned value; field initializer; `let` with annotation |
| a procedure is a value | module-level `proc`; type member; anonymous `proc`; captured in a closure |

## Test matrix

`workspaceCheck`, profile `staticV2`, unless stated; a row that names another
operation has that operation's profile (Q34). The preamble is `L` of
step 09, with `Sink` abbreviating `Proc({Str} Void {Log})`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-EFFECT-proc-type-parameter` | positive | `proc emit takes {line: Str sink: Proc({Str} Void {Log})} returns d: Void performs {Log} do return await sink(line) endproc` | accepted | 03 Effect ceilings: "A callback's row is therefore visible in the signature that accepts it" |
| `V2-EFFECT-proc-type-result` | positive | `fn pick takes {m: Memory} returns sink: Proc({Str} Void {Log}) do return proc takes {line: Str} returns d: Void performs {Log} do return await Log.append(m line) endproc endfn` | accepted | 03 Effect ceilings: "an anonymous `proc`" |
| `V2-EFFECT-intent-type-result` | positive | `fn prepare takes {m: Memory} returns plan: Intent(Void {Log}) do return Log.append(m "x") endfn` | accepted | 03 Cold plans: "`Intent(R {Row})`" |
| `V2-EFFECT-proc-type-field` | positive | `type Hooks of {onLine: Proc({Str} Void {Log})} endtype` and a `proc` that awaits `hooks.onLine("x")` | accepted | 03 Cold plans: "or a stored callable" |
| `V2-EFFECT-proc-type-empty-row` | positive | `Proc({I32} I32)` accepting `double` of step 05 | accepted | 03 Effect ceilings: "An omitted row is empty" |
| `V2-EFFECT-callback-row-exact` | positive | `emit("x" proc takes {line: Str} returns d: Void performs {Log} do return await Log.append(m line) endproc)` | accepted | 03 Effect ceilings: "accepts only a procedure with exactly that row" |
| `V2-EFFECT-callback-row-wider` | negative | the argument's row is `{Log Clock}` | `@type.argumentMismatch` at the argument | 03: "the types differ, and the call is `@type.argumentMismatch`" |
| `V2-EFFECT-callback-row-narrower` | negative | the argument is `proc takes {line: Str} returns d: Void do return void endproc` | `@type.argumentMismatch` at the argument | 03: "no row subtyping" |
| `V2-EFFECT-callback-row-wrapped` | positive | the same procedure wrapped: `proc takes {line: Str} returns d: Void performs {Log} do return await quiet(line) endproc` | accepted, with `@contract.unusedEffect` at `Log` | 03: "A procedure that performs less is wrapped in an anonymous `proc` that declares the wider row" |
| `V2-EFFECT-return-row-mismatch` | negative | `fn pick … returns sink: Proc({Str} Void {})` returning the `{Log}` closure | `@type.mismatch` at the returned expression | 03: "Callable types compare rows exactly" |
| `V2-EFFECT-field-row-mismatch` | negative | `Hooks(onLine: quiet)` where `quiet` has the empty row | `@type.argumentMismatch` at the argument | the same |
| `V2-EFFECT-intent-row-mismatch` | negative | `let plan: Intent(Void {}) be Log.append(m "x") endlet` | `@type.mismatch` at the initializer | the same |
| `V2-EFFECT-fn-for-proc` | negative | a `fn` value where `Proc({Str} Void {})` is expected | `@type.argumentMismatch` | 02 Functions as values: `Fn` and `Proc` are distinct types |
| `V2-EFFECT-proc-for-fn` | negative | `double` of step 05 where `Fn({I32} I32)` is expected | `@type.argumentMismatch` | the same |
| `V2-EFFECT-outside-ceiling-callback` | negative | `emit` with the empty row on itself | `@effect.outsideCeiling` at `await sink(line)`; the related hop is indirect, `performs Log via sink` | 07: "a hop through a callback or interface member is marked indirect" |
| `V2-EFFECT-named-proc-value` | positive | `emit("x" appendTo)` where `proc appendTo takes {line: Str} returns d: Void performs {Log}` | accepted | 02 Functions as values |
| `V2-EFFECT-named-proc-value-bare-requires` | negative | the same where `appendTo` has a bare `requires` | `@contract.obligationLost` at the conversion | 02 Guards and callable types: "otherwise the conversion is `@contract.obligationLost`" |
| `V2-EFFECT-anon-proc-row-invalid` | negative | `proc … performs {Shown} do … endproc` | `@effect.invalidReference` at `Shown` | 03 Effect ceilings |
| `V2-EFFECT-anon-proc-outside-ceiling` | negative | an anonymous `proc` with no `performs` whose body awaits `Log.append` | `@effect.outsideCeiling` at the `await` | 03: "A ceiling is never inferred" |
| `V2-EFFECT-anon-proc-await-in-fn-body` | negative | an anonymous `fn` inside an anonymous `proc`, awaiting | `@effect.invalidAwait` | 01 `return`, `try`, `await` |
| `V2-EFFECT-anon-fn-performs` | negative | `fn takes {line: Str} returns d: Void performs {Log} do … endfn` | `@syntax.invalidSection` at the section | 01 Grammar: a `fn` has no `performs` |
| `V2-EFFECT-proc-type-row-duplicate` | negative | `Proc({Str} Void {Log Log})` | `@effect.duplicateRoot` at the second | 03 Effect ceilings |
| `V2-EFFECT-proc-type-row-invalid` | negative | `Intent(Void {Memory})` | `@effect.invalidReference` | the same |
| `V2-TYPE-NAMES-suspicious-row` | positive with warning | `Proc({Str} {Log})` | accepted; `@type.suspiciousRow` at the type | 01 Grammar: "the result operand of a written `Proc` or `Intent` type whose row is omitted is a tuple of at least one component" |
| `V2-TYPE-NAMES-suspicious-row-returns` | positive | `fn pair takes {a: Log b: Clock} returns both: {Log Clock}` | accepted, no warning (Q6) | the same |
| `V2-TYPE-NAMES-suspicious-row-with-row` | positive | `Proc({Str} {Log} {Log})` | accepted, no warning | the same |
| `V2-RUNTIME-callback-awaited` | positive, `workspaceTest` | a test with `performs {Clock}` passing an anonymous `proc` that awaits `Clock.now(counter)` to a `proc` that awaits it twice | `@test.passed` | 03 Cold plans |
| `V2-RUNTIME-anon-proc-captures` | positive, `workspaceTest` | an anonymous `proc` capturing a local, returned from a `fn`, awaited in the test | `@test.passed` | 02 Functions as values: captures |

The M1 cases `V2-SRC-DECL-anon-proc` and `V2-SRC-DECL-proc-basic` stay
reader cases and are unchanged.

## Diagnostics and schema changes

Codes: `@type.argumentMismatch`, `@type.mismatch`, `@type.suspiciousRow`,
`@effect.outsideCeiling`, `@effect.invalidReference`,
`@effect.duplicateRoot`, `@effect.invalidAwait`, `@syntax.invalidSection`,
`@contract.unusedEffect`, `@contract.obligationLost`. No schema change.

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

Read: all exit 0; the 28 cases pass.

## Stays unavailable

`@tool.unavailable`: a test with `takes`; a target with an entry. A
contract `proc` member named as a value at a type parameter or interface
value receiver follows whatever M1 left for `fn` members there.

## Excluded

Grants (22); the entry (25).

## Completion evidence

- [ ] `conformance/rules/m2-10.tsv` has one `active` row per case id.
- [ ] Every position of the table has a positive and a negative case.
- [ ] `git grep -n "ToolUnavailable" crates/vibra-types/src/check/effects`
      lists only `test_header`, `grants.rs`, and `entry.rs`.
- [ ] The validation commands exit 0.
