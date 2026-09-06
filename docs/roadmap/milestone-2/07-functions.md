# Step 7 — function values, closures, and labelled application

Requires Step 6 and C1/C6. Read source **Labels and applications**,
**Declarations**, **Functions and expressions**; types **Application**,
**Functions as values**, **Inference and checking**; runtime **Evaluation**:
[source](../../spec/01-source-language.md), [types](../../spec/02-type-system.md),
[runtime](../../spec/06-runtime.md).

## Implementation sequence

1. Complete the admitted monomorphic, empty-effect `fn` signature model, including
   C6's label/default contract. Check public and private written signatures;
   never infer a missing parameter, result, or ceiling.
2. Resolve module-level function paths as function values. Resolve lambda free
   variables once and capture immutable values/IDs with explicit lifetime
   ownership. A lambda cannot acquire a fabricated self-name.
3. Type-check arbitrary callee expressions by their static category. Function
   application takes one path for named functions, returned functions and
   lambdas. Atoms/numerics are not callable; deferred constructors/collections
   are not misclassified as functions.
4. Feed resolved signature facts into M1's binding helpers. Validate fixed
   arity, default types, missing/unknown/duplicate labels and indirect-call
   signatures; build one ordered operand vector shared by IR and formatting.
5. Evaluate the callee once, then operands in fixed and labelled declaration
   order. Defaults are typed literals. Store closure environments separately
   from activations so a returned closure cannot refer to a popped stack slot.
6. Emit `@style.argument-order` only when the signature proves a safe canonical
   binding. Expose the binding facts for Step 11's format plans; do not make
   syntax or formatting crates depend on the checker.

| Positive | Negative / boundary |
| --- | --- |
| Pass/return function paths and lambdas; nested callee list | Literal/atom-headed application: `@type.not-applicable` |
| Captures after defining activation returns; nested captures | Capture of out-of-scope name, shadowing, lambda self-reference |
| Fixed and labelled calls, omitted defaults, reordered labels | Arity/type/default/label errors, duplicate binding |
| Same signature through named and indirect function values | Incompatible `fn` signature or nonempty effects under C1 |
| Format/reparse/check/execute gives same result | Callee duplicated or operands rebound during formatting |

Use host evaluator instrumentation to count callee/operand evaluations without
adding language effects. Pair it with source result cases; instrumentation
alone is not the corpus oracle. Variadic array/map construction, generics,
methods and collection application remain assigned by C1 to later work.
Recursive execution remains unavailable until Step 9; function values must not
bypass that admission boundary through an indirect call. Step 1's admission
contract must cover this restriction without reinterpreting valid v1 source.

Run [common validation](validation.md) and focused resolver/type/interpreter/fmt
tests. Done includes resolved function/type/value observations, capture lifetime
tests, warning spans, and independently authored canonical argument snapshots.
