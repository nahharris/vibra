# Step 04: checker-effects-seam

Kind: infrastructure. It serves slices 1 to 3 and 5: it gives every checker
step of M2 a file of its own, so that no two of them edit `declare.rs`,
`interfaces.rs`, `expr.rs`, `lower.rs`, or `testing.rs`.
Depends on: 03.
Crates: `vibra-types`.

## Prerequisites

- Read: [architecture](../architecture.md) "The checker"; the survey of
  what M1 left `@tool.unavailable`, which the entry points below list.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-types/src/check/mod.rs:18` | `mod callables;` | where `mod effects;` is declared |
| `crates/vibra-types/src/check/declare.rs:1684` | `data.view.performs().is_some()` | the header rejection; becomes a call of `effects::procs::header` |
| `crates/vibra-types/src/check/declare.rs:1215` | `NoType::Unsupported` | the written `Intent` type; becomes `effects::values::written_type` |
| `crates/vibra-types/src/check/declare.rs:1212` | `TypeForm::Proc` | the written `Proc` type; the same hook |
| `crates/vibra-types/src/check/interfaces.rs:390` | `member.performs().is_some()` | becomes `effects::members::contract_member` |
| `crates/vibra-types/src/check/interfaces.rs:1039` | `CallableKind::Proc` | becomes `effects::members::impl_member` |
| `crates/vibra-types/src/check/expr.rs:1368` | `ExprForm::Await` | becomes `effects::procs::await_expr` |
| `crates/vibra-types/src/check/expr.rs:1873` | `an anonymous` | becomes `effects::values::anonymous` |
| `crates/vibra-types/src/check/expr.rs:1876` | `anonymous callable with a row` | the same hook |
| `crates/vibra-types/src/check/expr.rs:2707` | `fn unequatable_operands` | gains a call of `effects::grants::check_equatable` |
| `crates/vibra-types/src/check/declare.rs:965` | `pub(crate) fn key_fault` | gains a call of `effects::grants::check_dict_key` |
| `crates/vibra-types/src/check/testing.rs:101` | `test.view.takes().is_none()` | becomes `effects::procs::test_header` |
| `crates/vibra-types/src/check/lower.rs:868` | `pub(crate) fn check_program` | calls `effects::analysis::check_bodies` after bodies are typed |

Proposed: `crates/vibra-types/src/check/effects/{mod,rows,procs,members,
analysis,values,grants,entry}.rs`.

## Files owned

- `crates/vibra-types/src/check/mod.rs`
- `crates/vibra-types/src/check/declare.rs`
- `crates/vibra-types/src/check/interfaces.rs`
- `crates/vibra-types/src/check/expr.rs`
- `crates/vibra-types/src/check/lower.rs`
- `crates/vibra-types/src/check/testing.rs`
- `crates/vibra-types/src/check/tests.rs`
- `crates/vibra-types/src/check/effects/`

The step also creates, empty, the unit-test files the later steps own
(`check/tests/{procs,rows,ceilings,values,grants,entry}.rs`) and declares
them in `check/tests.rs`, so no later step edits that file.

## Ordered tasks

1. Create the eight files of `check/effects/`. Each hook is a function
   with its final signature and a body that reports the same
   `@tool.unavailable`, at the same span and with the same message, that
   the shared file reported. Invariant: the diagnostics of every existing
   case are byte-identical.
2. Replace each rejection of the table with a call of its hook. Invariant:
   after this step a shared file contains no `performs`, `Proc`, `Intent`,
   or `await` decision of its own.
3. `effects::procs::apply` is called where an application's callee has a
   `Proc` type; `effects::analysis::check_bodies` is called once per
   checked program. Until step 09 fills it, it reports `@tool.unavailable`
   at every `await` whose plan has a row that is not empty, so a ceiling
   that nothing checks is never accepted. No program has such a row before
   step 08.
4. Document in `effects/mod.rs` which later step owns which file.

## Test matrix

No new case: the step is behaviour-preserving, and the existing corpus is
its matrix.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| the M1 corpus | positive and negative | every case under `conformance/cases` | the same verdicts and diagnostics as before the step | each case's own anchor |
| existing `V2-TOOL-unavailable-proc` | boundary | the case as it is | still `@tool.unavailable` at the same span | 07 Runtime and tool failures: "a valid … construct outside the tool's profile" |
| existing `V2-EFFECT-await-in-fn` | negative | the case as it is | still `@effect.invalidAwait` | 01 `return`, `try`, `await`: "elsewhere it is `@effect.invalidAwait`" |
| unit tests of `vibra-types` | positive | `a_proc_member_is_unavailable_wherever_it_is_written`, `a_test_that_takes_or_performs_is_unavailable_over_the_test` | pass unchanged | the same |

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

Read: all exit 0; the corpus runner reports the same counts as before.

## Stays unavailable

Everything it was: this step moves the rejections, it lifts none.

## Excluded

Any change of behaviour; any new diagnostic.

## Completion evidence

- [ ] `git grep -n "ToolUnavailable" crates/vibra-types/src/check/effects`
      lists every rejection the table moved.
- [ ] `git grep -nE "performs\(\)|CallableKind::Proc"
      crates/vibra-types/src/check/declare.rs
      crates/vibra-types/src/check/interfaces.rs
      crates/vibra-types/src/check/testing.rs` shows only calls into
      `effects`.
- [ ] The corpus count before and after, in the report, is equal.
- [ ] The validation commands exit 0.
