# Step 03: ir-procedures

Kind: infrastructure. Slice 1, below the checker: the checked IR of
procedures, `await`, and host bodies, with the validation that makes rows
and contexts facts of the IR; the dispatch trait; the origin table. Package
N4 of the native contract, IR half.
Depends on: 02.
Crates: `vibra-ir`, `vibra-interp`, `vibra-types`.

## Prerequisites

- Read: [effects chapter](../../../../spec/03-effects.md#effect-ceilings)
  "Effect ceilings", "Cold plans and performed rows", "Operation
  interfaces"; [runtime
  chapter](../../../../spec/06-runtime.md#cold-plans-and-invocation-identity)
  "Cold plans and invocation identity", "Host calls", "What has no
  encoding", "Audit trace" (the paragraph "Origins");
  [architecture](../architecture.md) "The checked IR" and the trust table.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-ir/src/types.rs:152` | `pub struct Row` | the row type, unchanged |
| `crates/vibra-ir/src/types.rs:319` | `pub struct ProcType` | the signature type of a procedure |
| `crates/vibra-ir/src/types.rs:332` | `pub struct IntentType` | the type of a plan |
| `crates/vibra-ir/src/expr.rs:364` | `pub enum Expr` | gains `Await` |
| `crates/vibra-ir/src/expr.rs:663` | `Type::Proc(signature)` | a call of a `Proc` already has an `Intent` type |
| `crates/vibra-ir/src/module_set.rs:42` | `pub enum FunctionBody` | gains `Host` |
| `crates/vibra-ir/src/module_set.rs:58` | `pub struct CheckedFunction` | gains `context` |
| `crates/vibra-ir/src/module_set.rs:482` | `pub fn try_new` | runs the new validation |
| `crates/vibra-ir/src/constant.rs:28` | `Type::Fn(_) \| Type::Proc(_)` | the types no constant has; gains operation interfaces |
| `crates/vibra-ir/src/origin.rs:12` | `pub struct SourceOrigin` | the element of the origin table |

Proposed: `vibra-ir::effects`, `vibra-ir::authority` (the types of the
architecture), `OriginTable` in `vibra-ir::origin`.

## Files owned

- `crates/vibra-ir/src/expr.rs`
- `crates/vibra-ir/src/check.rs`
- `crates/vibra-ir/src/module_set.rs`
- `crates/vibra-ir/src/effects.rs`
- `crates/vibra-ir/src/authority.rs`
- `crates/vibra-ir/src/origin.rs`
- `crates/vibra-ir/src/constant.rs`
- `crates/vibra-ir/src/encode.rs`
- `crates/vibra-ir/src/program.rs`
- `crates/vibra-ir/src/lib.rs`
- `crates/vibra-ir/tests/it/effects.rs`
- `crates/vibra-ir/tests/it/main.rs`
- `crates/vibra-interp/src/machine.rs`
- `crates/vibra-interp/src/types.rs`
- `crates/vibra-types/src/check/lower.rs`
- `crates/vibra-types/src/check/callables.rs`
- `crates/vibra-types/src/check/expr.rs`
- `crates/vibra-types/src/check/contract/facts.rs`
- `crates/vibra-types/src/check/contract/fragment.rs`
- `crates/vibra-types/src/check/contract/prove.rs`

The files outside `vibra-ir` gain only the arm a new variant forces: the
interpreter reports `Halt::Unavailable` for `Await` and for a `Host` body;
the checker's matches treat `Await` as outside the certified fragment and
build every function with `BodyContext::Pure`.

## Ordered tasks

1. Add `Expr::Await`, `FunctionBody::Host`, `HostBinding`, and
   `BodyContext`. Invariant: a module set built by M1 code is unchanged
   and still valid.
2. `effects::validate(set)`, called from `try_new`, with an explicit work
   list over each body:
   - an `Await` operand has an `Intent` type and the node has that
     `Intent`'s result type;
   - an `Await` is in a function, or a closure body, whose callable type
     is a `Proc`, or in a function whose `context` is `Procedure`;
   - the union of the rows of every awaited `Intent` in a body is
     contained in the body's ceiling: the `effects` of its `Proc` type,
     or the row a `Procedure` function states;
   - every row entry is an interface with at least one `proc` member;
   - a `Host` body is on a member of the interface its registry row
     names, the member's type equals the row's signature with the grant
     first, and its row is exactly `{owner}`.
   Invariant: each failure is `IrError`, never a panic, and names the
   function.
3. A type that is, or contains, an operation interface value type, a
   `Proc`, or an `Intent` is no constant type and no dict key type.
   Invariant: `Value` has no variant for a grant or a plan.
4. `authority.rs`: the data and the trait of the architecture, with no
   implementation. Invariant: the module names no type of `vibra-host`.
5. `OriginTable::of(program)`: every source origin of the functions
   reachable from the entry, sorted by source identity, start, end,
   numbered from 1; `ordinal(&SourceOrigin)` is 0 for an origin outside
   the table.
6. The canonical encoding of a set writes `Await`, `Host`, and `context`,
   and reads them back.

## Test matrix

Tests of `vibra-ir` over hand-built module sets. `I` is an interface with
one `proc` member `p: Proc({Self} Void {I})`; `J` is another.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| await in a procedure | positive | a function of type `Proc({I} Void {I})` whose body is `Await(Call(I.p, [arg0]))` | valid | 03 Cold plans: "`await` is valid only in a `proc` body and in a `test` body" |
| await in a test context | positive | a `Fn({} Void)` function with `context: Procedure` and row `{}` awaiting a plan of row `{}` | valid | the same |
| `ATK-ir-await-in-fn` | negative | the same body in a `Fn({I} Void)` function with `context: Pure` | `IrError`, naming the function | the same |
| await in a `Fn` closure inside a procedure | negative | a `Proc` function whose body holds a closure of type `Fn({} Void)` containing an `Await` | `IrError` | 06 Cold plans: "It never runs inside a `fn`" |
| await of a non-plan | negative | `Await(Literal(1i32))` in a procedure | `IrError` | 03 Cold plans: "`await plan` runs the plan" |
| row contained | positive | a `Proc({I J} Void {I J})` awaiting a plan of `{I}` then one of `{J}` | valid | 03 Effect ceilings: "requires it to be a subset of the ceiling" |
| `ATK-ir-await-outside-row` | negative | a `Proc({I} Void {})` awaiting a plan of `{I}` | `IrError` | the same |
| row of a callback | negative | a `Proc({Proc({} Void {J})} Void {I})` awaiting a call of its parameter | `IrError`: `J` is outside `{I}` | 03 Cold plans: "`await plan` contributes the plan's complete row" |
| building contributes nothing | positive | a `Fn({I} Intent(Void {I}))` whose body is `Call(I.p, [arg0])` with no `Await` | valid | 03 Effect ceilings: "A `fn` performs nothing. It may build and return a plan" |
| `ATK-ir-row-widened-at-call` | negative | a call passing a value of type `Proc({} Void {I J})` to a parameter of type `Proc({} Void {I})` | `IrError` | 03 Effect ceilings: "Callable types compare rows exactly" |
| a row entry that is no root | negative | a `Proc({} Void {K})` where `K` has only `fn` members | `IrError` | 03 Operation interfaces: "An interface with only `fn` members is not a root" |
| host body on its owner | positive | `FunctionBody::Host(Operation(FsReadText))` on `DirReadText.read` with the registry's signature and row `{DirReadText}` | valid | 03 Operation interfaces: "its signature and owner MUST match its registry row" |
| `ATK-ir-host-body-wrong-owner` | negative | the same body on a member of `DirList` | `IrError` | the same |
| host body, wrong signature | negative | the same body with `at: Str` | `IrError` | the same |
| host body with an additive row | negative | the same body on a member whose row is `{DirReadText DirList}` | `IrError` | 03 Operation interfaces: "Such a member has no body and no `performs`" |
| `ATK-ir-grant-constant` | negative | a `Constant` whose type is the interface value type `DirReadText` | `IrError` | 03 Capability values: "No source expression makes a grant" |
| a plan constant | negative | a `Constant` of type `Intent(Void {})` | `IrError` | 06 What has no encoding: "a plan … have no value encoding" |
| a grant dict key | negative | a `Dict(DirReadText Str)` type in a signature | `IrError` | 03 Capability values: "cannot be … used as a dict key" |
| origin table order | boundary | origins `(b.vib 0 4)`, `(a.vib 9 12)`, `(a.vib 2 30)`, `(a.vib 2 5)` | ordinals 4, 3, 2, 1 | 06 Audit trace: "sorted by source identity, then start, then end, numbered from 1" |
| origin outside the table | boundary | an origin of an unreachable function | ordinal 0 | 06 Audit trace: "zero means no source origin" |
| deep body | boundary | 100,000 nested single-statement sequences around one `Await`, validated on a 256 KiB thread | valid, no overflow | architecture, risk 5 |
| encode and decode | positive | a set with `Await`, a `Host` body, and a `Procedure` context | identical bytes after a round trip | 06 Canonical value encoding |

## Diagnostics and schema changes

No diagnostic code. `IrError` gains variants; a failure reaches a user as
`@runtime.invalidCheckedProgram`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-ir -p vibra-interp -p vibra-types -p vibra-host -p vibra-workspace -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; the corpus count is unchanged.

## Stays unavailable

A `proc` in source (`@tool.unavailable`, as in M1); `Await` and a `Host`
body in the interpreter (`RuntimeError::Unavailable`).

## Excluded

Lowering source into the new nodes (05); running them (06, 20); the
witness chain of a diagnostic, which is not a fact of the IR.

## Completion evidence

- [ ] Each of the 22 rows is a test, named in the report.
- [ ] `git grep -n "trusted" crates/vibra-ir/src/effects.rs` finds nothing:
      the module validates and trusts no caller.
- [ ] The files outside `vibra-ir` changed only by added `match` arms and
      the `context` field; the report shows the diff stat.
- [ ] The validation commands exit 0.
