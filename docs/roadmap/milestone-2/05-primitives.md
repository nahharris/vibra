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
