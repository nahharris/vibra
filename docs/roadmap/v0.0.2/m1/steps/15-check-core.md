# Step 15: check-core

Kind: vertical. The first programs run: scalars and literals, `fn` with
bare-name parameters, `return`, application of function values, `if` on
`Bool`, `match` over fieldless variant cases, module values with the
initializer-cycle check, tail calls, and the `@std.bool` module checked as
part of every graph. The checker, its lowering to IR, the workspace check,
and the `workspaceCheck` and `interpret` handlers are created here.

## Prerequisites

- Steps 11, 13, and 14 landed.
- Read: [type chapter](../../../../spec/02-type-system.md) "Model", "Scalars",
  "Application" (the `@function` row), "Functions as values", "Inference and
  checking", "Control flow and failure" ("Joins", "`return` and `Never`");
  [runtime chapter](../../../../spec/06-runtime.md) "Module-value
  initialization", "Tail calls"; [source
  chapter](../../../../spec/01-source-language.md) "`fn` and `proc`", "`return`,
  `try`, `await`", "`if`, `match`, `do`"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md) "Binding, return
  and pattern diagnostics", `@types.v2` under "Persistent observations",
  the `interpret` and `workspaceCheck` rows; [runtime
  chapter](../../../../spec/06-runtime.md) "Open signatures"; [tooling
  chapter](../../../../spec/05-tooling.md) "One source alone" and the
  `check` rule of "Command surface".

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-types-v1/src/source.rs:117` (`check_source`), `resolved.rs:360` (`check_resolved`) | the two entries and the "one module of a one-module library workspace" rule |
| `crates/vibra-types-v1/src/lib.rs:214` (`check_function_body`), `infer.rs` | the environment-with-slots design; rewrite for the new grammar |
| `crates/vibra-workspace-v1/src/semantic.rs:273` (`check_scope`) | the scope rule: a target and its import closure, plus every prelude module |
| `crates/vibra-conformance-v1/src/{types.rs,workspace_semantic.rs}` | the `interpret` and `workspaceCheck` handler shapes |

Proposed: `vibra-types::{declare, infer, expr, lower, check}`;
`vibra-workspace::semantic`; `vibra-conformance::{interpret, workspace_check}`.

## Ordered tasks

1. Headers: collect every `fn` signature and module `let` type; `proc`,
   `test`, `interface`, `impl`, labelled parameters, generics, and
   non-scalar types are `@tool.unavailable` over the construct until their
   steps. `type` declarations: variant types with fieldless cases only
   (needed for `Bool`); other bodies unavailable until 16.
2. Literals: suffixed literal fixes its type and MUST fit
   (`@type.numericOutOfRange`); an unsuffixed literal takes its expected
   type or is `@type.ambiguousInference` with a note per missing
   constraint; string literals have the `@str` role type; `void` is `Void`;
   atoms are their singleton type.
3. Application of a `Fn` value: positional arguments to parameters in
   order; wrong arity or a label on a positional-only callee is
   `@type.argumentMismatch`; any other disagreement `@type.mismatch`. The
   callee is any expression; one whose type cannot be applied is
   `@type.notApplicable` at the callee. Constructor application for
   fieldless cases (`Bool.true()`, `Ordering.less()`).
4. Bodies: statements in order; `return` checked at the written result
   type and typed `Never`; a body that can run off its end is
   `@type.missingReturn`; a statement after a `Never` statement, or
   `Never` in an operand position, is `@type.unreachableCode` (one per
   form); `return` in a module-value initializer is `@type.invalidReturn`.
   `if` requires `Bool`; as a statement each branch is `Void` or `Never`;
   as a value both branches share one type and `else` is required
   (`@type.mismatch` at the form without it). `match` over a variant with
   fieldless case patterns and `_`; coverage for variants and `Bool`
   (`@pattern.nonExhaustive`, `@pattern.unreachableArm`); richer patterns
   in 16 and 17. `do` blocks. Local `let` with a bare name and optional
   written type (patterns in 17).
5. Module values: type mandatory, initializer checked at it, lazy; the
   initializer-cycle analysis through functions, closures, and function
   values (`@type.initializerCycle` naming one `let`, edges related).
6. Lowering to IR and `CheckedModuleSet`; `check_source` entering the
   first function with no parameters for `interpret`.
7. Workspace check: resolve the graph with the stdlib overlay; check a
   target's units and import closure plus the prelude modules. With no
   target named, every local target and the `@tests` unit are covered; a
   named target covers that target only. A target whose kind the profile
   does not support (`@bin` until M2) still decodes: it is reported
   `@tool.unavailable` at its target record with the status
   `unsupported`, and the other targets are checked. The `@types.v2`
   document is the closed schema of the diagnostics chapter:
   `moduleValues` of `{declaration typeInfo}`, `callables` of
   `{declaration signature}` with the signature record of step 12,
   `implementations`, and `obligations`, the last two empty in this step.
   A host test in `vibra-conformance` reads every encoding example of
   step 12's host tests back through `vibra-syntax` and compares decoded
   values, which proves the two writers and the reader agree.
8. The `@std.bool` module checks clean under this step's rules (its members
   use `match` on fieldless cases); `@std.core`, `@std.option`,
   `@std.result`, `@std.iter`, `@std.builtin` are loaded and only the
   declarations this step supports are checked; a stdlib declaration a step
   does not support yet is skipped with a recorded note, never reported as
   unavailable to the user.
9. Cases.

## Test matrix

Inputs are complete modules; `interpret` cases name the first parameterless
function. The result is `@value.v2` text; here the value is shown in source
spelling.

| Case id | Kind | Input | Expected |
| --- | --- | --- | --- |
| `V2-TYPE-INFER-literal-suffix` | positive, interpret | `fn main returns r: U8 do return 7u8 endfn` | `7u8` |
| `V2-TYPE-INFER-literal-expected` | positive, interpret | `fn main returns r: I64 do return -5 endfn` | `-5i64` |
| `V2-TYPE-INFER-literal-ambiguous` | negative | `fn main returns r: I32 do let a be 1 endlet return a endfn` | `@type.ambiguousInference` at `1` with one note |
| `V2-TYPE-INFER-out-of-range` | negative | `fn main returns r: U8 do return -1u8 endfn` and `… 300u8 …` | `@type.numericOutOfRange` at each literal |
| `V2-TYPE-INFER-float-rounds-to-infinity` | negative | `fn main returns r: F32 do return 1e39f32 endfn` | `@type.numericOutOfRange` |
| `V2-TYPE-INFER-no-promotion` | negative | `fn add takes {a: I32 b: I64} returns r: I64 do return I64.add(a b) endfn` | `@type.mismatch` at `a` |
| `V2-SRC-CALLS-function-value` | positive, interpret | `fn inc takes {x: I32} returns r: I32 do return I32.add(x 1i32) endfn\nfn apply takes {f: Fn({I32} I32) v: I32} returns r: I32 do return f(v) endfn\nfn main returns r: I32 do return apply(inc 41i32) endfn` | `42i32` |
| `V2-SRC-CALLS-callee-expression` | positive, interpret | `fn pick takes {b: Bool} returns f: Fn({I32} I32) do if b then return inc else return dec endif endfn … return pick(true)(1i32)` with `inc`/`dec` | `2i32` |
| `V2-SRC-CALLS-arity` | negative | `inc(1i32 2i32)` | `@type.argumentMismatch` at the application |
| `V2-SRC-CALLS-label-on-positional` | negative | `inc(x: 1i32)` | `@type.argumentMismatch` |
| `V2-SRC-CALLS-order` | positive, interpret | callee and arguments each a call that extends a `List(I32)` in a module value chain is not expressible without state, so: `fn main returns r: {I32 I32} do return {first() second()} endfn` where `first`/`second` return constants; evidence of order is the interpreter's trace host test | `{1i32 2i32}` |
| `V2-TYPE-CONTROL-missing-return` | negative | `fn f takes {b: Bool} returns r: I32 do if b then return 1i32 endif endfn` | `@type.missingReturn` at the body |
| `V2-TYPE-CONTROL-return-type` | negative | `fn f returns r: I32 do return "x" endfn` | `@type.mismatch` at `"x"` |
| `V2-TYPE-CONTROL-unreachable-after-return` | negative | `fn f returns r: I32 do return 1i32 return 2i32 endfn` | `@type.unreachableCode` at the second `return`, related to the first |
| `V2-TYPE-CONTROL-never-operand` | negative | `fn f returns r: I32 do let a be return 1i32 endlet return a endfn` | `@type.unreachableCode` at the `return` operand |
| `V2-TYPE-CONTROL-if-value-no-else` | negative | `fn f takes {b: Bool} returns r: I32 do let a be if b then 1i32 endif endlet return a endfn` | `@type.mismatch` at the `if` |
| `V2-TYPE-CONTROL-if-branch-types` | negative | `let a be if b then 1i32 else "x" endif endlet` | `@type.mismatch` at `"x"` |
| `V2-TYPE-CONTROL-if-statement-void` | positive, interpret | `fn f takes {b: Bool} returns r: I32 do if b then return 1i32 endif return 2i32 endfn` with `main` calling `f(false)` | `2i32` |
| `V2-TYPE-CONTROL-if-not-bool` | negative | `if 1i32 then …` | `@type.mismatch` at `1i32` |
| `V2-TYPE-CONTROL-never-branch-skipped` | positive, interpret | `let a be if b then 1i32 else return 0i32 endif endlet return a` | `1i32` for `b` true |
| `V2-TYPE-CONTROL-match-bool` | positive, interpret | `match b case true 1i32 case false 2i32 endmatch` as a value | `2i32` for `false` |
| `V2-TYPE-CONTROL-match-non-exhaustive` | negative | `match o case Ordering.less() 1i32 case Ordering.equal() 2i32 endmatch` | `@pattern.nonExhaustive` at the `match`, note `Ordering.greater()` |
| `V2-TYPE-CONTROL-match-unreachable` | negative | `… case _ 0i32 case Ordering.less() 1i32 …` | `@pattern.unreachableArm` at the second arm's pattern |
| `V2-TYPE-CONTROL-do-value` | positive, interpret | `let a be do let x be 1i32 endlet I32.add(x x) enddo endlet return a` | `2i32` |
| `V2-TYPE-CONTROL-do-ends-in-let` | positive, interpret | `fn main returns r: Void do let v be do let x be 1i32 endlet enddo endlet return v endfn` | `void` |
| `V2-TYPE-CONTROL-let-written-type` | positive, interpret | `let count: U64 be 0u64 endlet return count` | `0u64` |
| `V2-TYPE-CONTROL-let-written-type-mismatch` | negative | `let count: U64 be 1i32 endlet` | `@type.mismatch` at `1i32` |
| `V2-TYPE-CONTROL-module-value` | positive, interpret | `let defaultRetries: U8 be 3u8 endlet\nfn main returns r: U8 do return defaultRetries endfn` | `3u8` |
| `V2-TYPE-CONTROL-module-value-forward` | positive, interpret | `let a: U8 be b endlet\nlet b: U8 be 1u8 endlet` with `main` returning `a` | `1u8` |
| `V2-TYPE-CONTROL-initializer-cycle` | negative | `let a: U8 be helper() endlet\nfn helper returns r: U8 do return a endfn` | `@type.initializerCycle` at `a`, the edge related |
| `V2-TYPE-CONTROL-initializer-return` | negative | `let a: U8 be return 1u8 endlet` | `@type.invalidReturn` at `return` |
| `V2-RUNTIME-tail-loop` | positive, interpret | `fn count takes {n: I32 acc: I32} returns r: I32 do if Equatable.equal(n 0i32) then return acc else return count(I32.sub(n 1i32) I32.add(acc 1i32)) endif endfn\nfn main returns r: I32 do return count(100000i32 0i32) endfn` | `100000i32`; the host test asserts `max_activation_depth` 1 |
| `V2-RUNTIME-non-tail-depth` | positive, interpret | `fn sum takes {n: I32} returns r: I32 do if Equatable.equal(n 0i32) then return 0i32 else return I32.add(1i32 sum(I32.sub(n 1i32))) endif endfn` with `main` calling `sum(100000i32)` | `100000i32` under the runner's budget |
| `V2-RUNTIME-memory-exhausted` | host event | `fn loop takes {n: I32} returns r: I32 do return I32.add(1i32 loop(n)) endfn` | the host event `@runtime.memoryExhausted`; no result |
| `V2-PROJECT-workspace-check-library` | positive, workspaceCheck | a two-module library where `main.vib` imports `@app.util` and calls a public function | accepted; the `types` snapshot is a `@types.v2` document with two `callables`, each `{declaration signature}` |
| `V2-PROJECT-workspace-check-bin-unavailable` | boundary, workspaceCheck | a project with one `@lib` target and the chapter's `@bin` target | `@tool.unavailable` at the `@bin` target's record; the library target is checked and accepted |
| `V2-SRC-CALLS-not-applicable-scalar` | negative | `fn f returns r: I32 do let n be 1i32 endlet return n(2i32) endfn` | `@type.notApplicable` at the callee `n` |
| `V2-TOOL-unavailable-proc` | boundary | a module with a `proc` | `@tool.unavailable` over the `proc`; other declarations exact |

The `Equatable.equal` and `I32.add` calls use the `@std.builtin`
declarations of step 14 through their roles; dispatch on `Equatable` for
integers is implemented as an intrinsic-backed implementation in this step
(the general interface machinery is step 19).

## Diagnostics and schema changes

Codes: `@type.numericOutOfRange`, `@type.ambiguousInference`,
`@type.mismatch`, `@type.argumentMismatch`, `@type.missingReturn`,
`@type.invalidReturn`, `@type.unreachableCode`, `@pattern.nonExhaustive`,
`@pattern.unreachableArm`, `@type.initializerCycle`, `@tool.unavailable`,
`@type.notApplicable`, `@runtime.memoryExhausted`. Schema: `@types.v2`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-workspace -p vibra-interp -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: `interpreterV2 conformance: N passed, 0 failed, 0 unavailable`; the
`V2-RUNTIME-*` cases print `PASS`; a host test asserts the tail case's
depth and transfer counts.

## Excluded

Products, collections, declared types with fields (16); patterns beyond
bare names and fieldless cases, `let … else`, `try` (17); labelled
parameters, anonymous callables (18); interfaces (19); guards (21, 22).

## Completion evidence

- [ ] `conformance/rules.tsv` has one `active` row per case id above.
- [ ] `@std.bool` is checked in every `interpret` case (a host test asserts the
      module set contains its functions).
- [ ] The architecture test lists `vibra-types` with `vibra-interp` added.
- [ ] Validation commands exit 0.
