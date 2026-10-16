# Step 8d — a native's checked body in typed IR

Prerequisite: Step 8b merged. Stage 4A behavior step, typed IR, checker, and
WebAssembly backend. It exists because Step 8b found that a module cannot run the
Vibra body of a native that applies a function value, and the specification says
it must. Step 9 requires it, since `V1-RUNTIME-tail-calls-through-flow` uses
`array.fold`.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **Native implementations** (the last
  sentences of the second paragraph: a native whose meaning applies a function
  value has no import, "a module runs its body", and the differential holds the
  interpreter's native code to it), **M3 compiler intrinsic registry**
  (`array.fold`), **Tail calls**.
- [Types](../../spec/02-type-system.md): **Generics**, for the `where:` of
  `array.fold`.
- [Step 8b guide](08b-collections.md), its as-built section, and
  [ledger](decision-ledger.md) rows D10.1, D10.3, D10.4, and D10.5.
- `stdlib/src/std/builtin.vib`, the body of `array.fold`.

## The defect

`CheckedFunction::new_external` wraps every native in a one-expression function
whose body is `Expr::External`, and the checker never checks the Vibra body that
the standard library declares beside `native:`. The interpreter runs the wrapper,
which is right for it: its Rust continuation is the native. A module cannot,
because `array.fold` has no import, and it has no body to run either, so Step 8b
reports `external:array.fold` as not lowered and `V1-RUNTIME-library-roles` stays
open.

The checker has no check of an AST body for a builtin member today. A builtin
member (`array.of`, `array.fold`, `dict.entries`) gets a function header from the
embedded `builtin.vib` only for its signature and registry symbol
(`standard::builtin_members`), and its body is never parsed into the program. The
single-source checker (`check_ast`) and the workspace checker
(`resolved.rs`) each build those headers, so each needs the change.

## Scope

A native that applies a function value is checked with its body, and the checked
function carries both: the wrapper that the interpreter runs as the native, and
the checked body that a module lowers as ordinary code. Today that is only
`array.fold`.

## Invariants

- **One checked body.** The IR holds exactly one checked Vibra body for the
  native, the one the standard library writes. It is checked from
  `builtin.vib`'s own text by the ordinary function checker, in the module's own
  environment. No Rust-built copy of a body is allowed, and no emitter lowers a
  hand-written loop for a named row: the emitter's lowering of the body is the
  lowering of any function.
- **The interpreter's native stays held to it.** The interpreter keeps running its
  native continuation for `array.fold`; the body/native harness
  (`natives_m3_step4b`) keeps running the interpreter's native, the interpreter's
  body, and the module's lowering of the body, and they must agree.
- **No new rule.** A native with an import is lowered as today. The only native
  without one is the one whose meaning applies a function value, by the table
  `NATIVE_ROWS` and the manifest, and a test fails if a native gets neither.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-ir/src/lib.rs`: `CheckedFunction` | A field for the checked body of a native, beside the wrapper, with its slot count and validation |
| `crates/vibra-types/src/lib.rs`: `check_functions`, `standard.rs`: `builtin_members` | Check the body of a builtin member whose native has no import, in the single-source path |
| `crates/vibra-types/src/resolved.rs` | The same for the workspace path |
| `crates/vibra-wasm/src/lower.rs`, `classify.rs` | Lower the body of a native that has no import; report `external:` for nothing |
| `crates/vibra-interp/src/machine.rs`: `fold_start` | Unchanged: the native continuation |

## Ordered tasks

1. A host test that fails today: `array.fold` in a program lowers and a module
   returns what the interpreter returns.
2. The IR field and its validation (the body has the signature of the wrapper;
   its recursion is a direct call of the wrapper).
3. The single-source checker checks the body of `array.fold` from the embedded
   module, with the paths it names (`array.slice`, `array.length`, itself) and the
   `option` role constructors it matches.
4. The workspace checker does the same.
5. The emitter lowers the body for a native with no import, and the interpreter
   still runs the native; the canonical form of the program records the body.
6. Move `V1-RUNTIME-library-roles` to `matched`; extend the native harness's
   `array.fold` samples to the module path.

## Test matrix

- Positive: `array.fold` over an empty array, one element, many elements, a
  closure that captures, an accumulator that is an array and a record; a fold
  inside a tail loop; the harness on all paths.
- Negative: no new source diagnostic.
- Recovery: not applicable.
- Boundary: a ten-thousand-element fold at constant frame depth; a step function
  that is a module function; a fold nested in a fold.
- Formatter: no change.

## Diagnostic and schema changes

None. The canonical form of a checked program changes (it records the body), and
`typed_ir_identity_m4_step4` keeps holding it to be independent of any value ID
or offset.

## Validation

The [Step 4–12 row](validation.md#focused-checks) and the full
[pre-merge list](validation.md#before-merging-each-step).

## Excluded

Any other native that applies a function value (none exists in v1); lowering
`array.fold` as a hand-written loop; accelerating a body.

## Completion evidence

`V1-RUNTIME-library-roles` is `matched`; the native harness runs `array.fold` on
all four paths; no program that names `array.fold` is reported as not lowered.

## As built

Landed as `landed`, conditional on its pull request merging; the base is Step 8c (PR #365, merge `d51cc34`).

- **The IR.** `CheckedFunction::new_external_with_body` carries one `Expr` and its slot count beside the wrapper. `native_body()` is `None` for every other function; `module_body()` is the body a module lowers (the native body, else the function's own). The constructor refuses a row for which `CompilerIntrinsic::runs_body()` is false. The program constructor validates the body's shape against the wrapper's signature, its declared types, its tail calls, and its static references; the body's recursion is a direct call of the wrapper. The canonical form records it as `native-body:`.
- **Which natives.** `CompilerIntrinsic::runs_body()` is `is_native() && native_import().is_none()`, read from the table of native imports with nothing written for a named row. `array.fold` is the only one, and `every_native_has_an_import_or_a_checked_body_and_never_both` fails if a native gets neither or both.
- **The checkers.** Both paths check the embedded `builtin.vib` text with the ordinary function checker (`check_function_body`, now shared by the three call sites), in the module's own environment: source identity `@std.builtin`, and the builtin members as the names it sees. `standard::members_with_bodies` adds the members that a body names (`array.slice`, `array.length`) to the program with the native, so the body's calls are ordinary direct calls. The single-source path is `Checker::check_native_body`; the workspace path does the same in `check_resolved`. The application facts of the body go to a scratch list, not to the checked source's.
- **A tail call inside the body.** A call of a native from its own body, in tail position, is a tail call (`CheckEnvironment::own_body`): the body is the native's meaning, and the runtime chapter says a call in tail position reuses its activation. A call of `array.fold` from user code keeps the existing flags, so Step 9's `tail-calls-through-flow` is not blocked: the module lowers every direct call of the wrapper as a call of the lowered body.
- **The emitter.** `lower.rs` lowers `function.module_body()` and `classify.rs` walks it, so nothing reports `external:array.fold`. The interpreter still runs the native continuation (`fold_start`), unchanged.
- **Tests.** `native_bodies_m4_step8d` (fold over an empty, one, and many elements, a capturing closure, a module-function step, array and record accumulators, a fold in a tail loop, a nested fold, the workspace path, the one-body guard). `natives_m3_step4b` now requires every native's program to lower and agree. The fixtures that used `array.fold` as the example of a program that does not lower now use a contract call, which Step 9 lowers.
- **Depth and live size.** A fold of 10,000 elements runs on a 64 KiB engine stack and returns the interpreter's result; its live size starts at 0 and ends at 64 bytes, the same as the one-element baseline. The fold's recursion is a tail call on an arena frame, and applying the step is an indirect call on an arena frame. The test takes about 40 seconds in a debug build because the body slices the array at each step, as the standard library writes it; accelerating a body is excluded.
- **Parity.** `V1-RUNTIME-library-roles` is `matched`: 85 matched, 40 not lowered (22 owned by Step 9, 18 by Step 11). `V1-RUNTIME-tail-calls-through-flow` stays with Step 9.
- **No specification text changed.**