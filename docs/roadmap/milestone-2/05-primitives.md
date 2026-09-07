# Step 5 — typed primitive functions and reference execution

Requires Step 4 and C1/C4/C11/C12. Read types **Model**, **Application**,
**Inference and checking**; runtime **Semantic reference**, **Evaluation**,
**Determinism and observability**; source **Declarations**:
[types](../../spec/02-type-system.md), [runtime](../../spec/06-runtime.md),
[source](../../spec/01-source-language.md).

## Implementation sequence

1. Add `vibra-ir`, `vibra-types`, and `vibra-interp` for one complete small path:
   a fully typed module-level function returning a primitive literal. Follow
   [the dependency map](implementation.md); do not put types in wire schemas.
2. Represent C1's exact primitive subset and signatures in IR. Check written
   function parameter/result types before bodies. This step supports nullary
   functions/literal bodies; remaining call/binding forms stay unavailable.
3. Convert lexical numerics using expected types and exact range checks. Do
   not parse all integers through `i64`, default ambiguous numerics, widen
   suffix types, or round decimal `f32` via `f64` if that changes the result.
4. Preserve Unicode scalars, immutable string/byte values as admitted by C1,
   and the one `void` value. Reject malformed/recovered input before lowering.
5. Interpret checked literal/sequence/return IR with explicit values and source
   origins. Make successful checked-program construction a controlled boundary;
   an arbitrary parsed AST is not executable input.
6. Wire real static and interpreter handlers, canonical value observations and
   explicit empty audit traces. Keep the reader handler and broaden corpus CI
   labelling to describe the actual profiles without claiming complete v1.

| Positive | Negative / boundary |
| --- | --- |
| Every supported primitive literal/type, `void`/empty body per C5 | Wrong return type; body/signature with deferred types |
| Each signed min/max and unsigned max | One below/above bounds, negative unsigned, huge digit strings |
| Context-constrained unsuffixed numerics | Unconstrained ambiguity; suffix/context mismatch; implicit conversion |
| `f32`/`f64` rounding ties and finite boundary cases | Finite literal overflow; no source NaN/infinity spelling |
| BMP and astral character/string values | Invalid scalar rejected; Unicode byte/display origin preserved |
| Repeat same checked input and obtain same value/empty trace | No environment, clock, filesystem, random or host callbacks in execution |

Use `@type.numeric-out-of-range` and Step 1's exact mismatch/availability
contracts. Lower no arithmetic operations or `@host` calls. Do not claim Wasm
parity. Run [common validation](validation.md) plus
`cargo test --locked --offline -p vibra-types -p vibra-ir -p vibra-interp`.
Done requires independent `V1-TYPE-INFER-*` and `V1-RUNTIME-*` observations,
including proof a bad expected value/trace causes a corpus failure.

## Step 5 implementation evidence

The Step 5 slice adds three backend-independent nodes:

- `vibra-ir` owns primitive types, exact primitive values, signatures, source
  origins, literal/sequence expressions, and the checked-program boundary.
  Typed observations use only VIBON data records and arrays: literal bodies
  are `kind: @literal` records and direct function-body sequences are
  `kind: @sequence` records under the versioned `@types.v1` envelope. They do
  not embed executable `do` forms.
- `vibra-types` checks source through the shared M1 AST and lowers only
  nullary module-level `defn` declarations with primitive result types to that
  IR. Integer ranges are checked from decimal magnitudes, and decimal `f32`
  literals are parsed directly as `f32`.
- `vibra-interp` evaluates checked literal and sequence IR from left to right.
  It returns the final value (or `void` for an empty sequence) and an explicit
  empty audit trace.

The conformance adapter exposes `type-check` through `static-v1` and
`interpret` through `interpreter-v1`. The independent cases are
`V1-TYPE-INFER-primitives`, `V1-TYPE-INFER-context-numeric`,
`V1-TYPE-INFER-boundaries`, `V1-TYPE-INFER-range-errors`,
`V1-TYPE-INFER-float-boundaries`, `V1-TYPE-INFER-float-overflow`,
`V1-TYPE-INFER-out-of-range`, `V1-TYPE-INFER-mismatch`,
`V1-TYPE-INFER-recovery`, `V1-TYPE-INFER-deferred-do`,
`V1-RUNTIME-literal`,
`V1-RUNTIME-sequence`, `V1-RUNTIME-void`, `V1-RUNTIME-unicode`,
`V1-RUNTIME-rejected`, and `V1-RUNTIME-wrong-trace`. Host tests also use
intentionally incorrect type and trace expectations to prove the runner does
not compare an observation with itself.

The slice deliberately excludes explicit `do` expressions, calls, bindings,
effects, host providers, collections, CLI/Wasm paths, and later declaration
forms. Direct multi-expression function bodies are the admitted sequence form;
explicit `do` receives `@tool.unavailable` until Step 6. The source grammar has
no standalone bytes literal, so the bytes IR and its data-only observation are
covered by host round-trip tests without inventing a new source spelling. All
valid syntax outside the admitted subset receives `@tool.unavailable`; rejected
or recovered input never crosses the checked-program boundary.
