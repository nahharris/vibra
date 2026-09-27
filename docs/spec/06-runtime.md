# Vibra v1 runtime and WebAssembly

Status: normative target
Implementation status: M2 executes successfully checked IR for its supported
pure subset, including tail calls and isolated tests. WebAssembly and complete
v1 interpreter parity remain unimplemented.

## Semantic reference

The language semantics are independent of a backend. A small reference
interpreter is the executable oracle for evaluation, types after erasure,
effect events, and failures. The production compiler emits WebAssembly. A v1
implementation is conforming only when both produce the same observable
behavior for the conformance corpus.

The interpreter is not a second frontend: it consumes the same resolved typed
IR as the Wasm backend. Parsing, name resolution, type checking, performed-row
calculation, and external-registry validation are shared.

The M2 interpreter consumes only successfully checked IR from the M2 supported
surface. It has no host provider execution, ambient filesystem/environment/time
reads, or Wasm path. A valid later-v1 operation that is outside this profile is
reported as `@tool.unavailable` before lowering; it is never reinterpreted as a
generic application.

WebAssembly is a compiler output backend, not a source interoperability
surface. V1 source cannot import a `.wasm` module or name a WebAssembly
provider.

## Evaluation

Evaluation is strict and deterministic:

- an application evaluates its callee exactly once before any runtime operand;
- compile-time tuple-index and record-field selectors are not evaluated as
  values;
- function and constructor operands evaluate in resolved fixed-parameter
  order, labelled declaration order, then variadic source order;
- an array variadic tail builds one array from its values;
- a map variadic tail builds one map from alternating key/value forms and an
  odd tail is rejected before execution;
- function bodies and `do` forms evaluate from first to last; the value of a
  `do` is its last expression, or `void` if empty;
- `if` evaluates only the selected branch;
- `match` evaluates its subject once and selects the first matching arm, and an
  `as` arm tests only the union discriminant;
- `tuple.of` operands evaluate from left to right, and `array.of` and
  `map.of` follow the ordinary variadic order;
- `try` performs only its specified early-exit propagation; and
- a tail-position call to a function in the same recursive group reuses the
  current activation instead of growing language-level stack.

### M2 module-value initialization

Before accepting an M2 checked program, the checker MUST build the dependency
graph of its immutable module-level `def` initializers. An
initializer may depend on a module value directly or through a function call
or callable alias reached while evaluating that initializer. A cycle is
rejected only when the dependency cycle contains a module-level `def`; a
function-only recursion cycle reached from an initializer is not a module
initializer cycle. A module-value cycle is rejected with the error-level
`@type.initializer-cycle` before an executable checked program is produced or
any initializer is evaluated or program executed; no checked program is
produced. Its primary span is the complete source form of a `def` participating
in the cycle, selected by the deterministic dependency traversal. Acyclic
forward references remain valid. Type checking never evaluates an initializer
to infer its written type.

For an accepted program, a module value is evaluated lazily on its first read
and exactly once during that execution. Later reads reuse that value. A new
execution starts with fresh module-value state.

`map.of` and map variadic tails share one construction rule. Every key and
value is evaluated even if a key repeats; the later pair replaces the earlier
value. Map iteration order is canonical key order, not insertion or hash-table
order.

Nominal constructor applications and `tuple.of` assemble immutable values;
they do not invoke a function body, add a function-call edge, or emit a host
event. Effects from evaluating their operands remain observable.

Tuple and record projection are lowered from their compile-time selector to a
direct component read. Array, map, string, and byte application performs one
bounds-checked or presence-checked lookup and returns `option.some` or
`option.none`. A missing key or out-of-range index does not panic, trap, return
null, or synthesize a default value. These projection and lookup operations are
pure and generate no function-call edge or host event.

A union value carries one discriminant selecting the member type it holds,
together with that member's value. The discriminant set is closed and fixed at
declaration, and both backends MUST assign discriminants in written member
order so that build output and serialization are deterministic. Widening a
member value to a union attaches the discriminant, and an `as` pattern compares
it and yields the payload at the member type. Both are pure, add no
function-call edge, and emit no host event, exactly as a nominal constructor and
a projection do. A union carries no method table: dispatch on a union value
selects an implementation written for the union type itself, never one written
for a member.

`as` in expression position is erased. It fixes types during checking and lowers
to its operand, so it emits no typed-IR node, performs no runtime check, and can
never fail at run time. A destination-dispatched conversion is an ordinary
static call to the implementation the checker selected; the interfaces it
implements add no runtime type information to any value.

String and byte indexing is bounds-checked. Strings are Unicode scalar
sequences at the language level; each scalar is a `char`, and byte conversion
is explicit UTF-8. A runtime MUST reject a character representation in the
Unicode surrogate range or above U+10FFFF. `void` carries no runtime
information and has one observable value, spelled `void` when serialized.

Numeric suffixes are erased after fixing the literal's primitive type in typed
IR. They do not alter the runtime representation or arithmetic semantics of
that type. Literal range errors are rejected before execution.

Floating-point operations follow IEEE 754. Serialization and equality
canonicalize all NaN payloads to one quiet NaN per width and normalize negative
zero only where the relevant standard operation explicitly says so.

## Tail calls

Tail position is defined inductively relative to an enclosing activation
(`defn`, `lambda`, or nested body form). An expression is in tail position when
its value is returned directly to that activation's caller without further
computation.

The final expression of a module-level `defn` body or `lambda` body is in tail
position. For every other form, tail status propagates inward only when the form
itself is in tail position:

- in `(do e1 … en)`, only `en` may be in tail position, and only when the `do`
  is in tail position;
- in `(let p v e1 … en)`, only `en` may be in tail position, and only when the
  `let` is in tail position;
- in `(if c t e)`, `t` and `e` may be in tail position only when the `if` is
  in tail position;
- in `(match s …)`, each arm's result expression may be in tail position only
  when the `match` is in tail position; and
- the operand of `try` is never in tail position, because `try` inspects that
  value before continuing.

Every other position is non-tail, including operands of applications (even when
they are the final expression inside a `do` that is itself an operand),
`let`/`match` bindings and subjects, `if` conditions, and `try` operands.

The **recursive group** of a module-level `defn` is the set of module-level
`defn`s in that module reachable from it through static function-call edges,
including mutual recursion. Calls in tail position whose callee resolves to a
member of the current function's recursive group MUST reuse the current
activation. The interpreter and WebAssembly backend MAY implement this with
explicit tail-call instructions or an internal trampoline; the strategy is not
observable except that conforming programs do not overflow language-level stack
on such tail recursion.

V1 defines no portable stack-depth limit. Non-tail recursion and non-tail calls
that exhaust an embedding host's stack are host events, not portable semantic
results, and are outside interpreter/Wasm parity. The M2 reference interpreter
runs on a host thread with a fixed stack and bounds live language activations
so that exhaustion never aborts the process. Reaching that bound stops
execution with the unlocated error diagnostic
`@runtime.host-stack-exhausted` (primary span `0..0`, no source ID). It is not
a trap: `run` and `test` report it as `@command.operational-failure` (exit 3),
and `test` then reports zero selected, passed, and failed tests. Default `iter` method bodies
MAY lower to internal loops; that mutation is not a source feature.

## M2 compiler intrinsic profile

The M2 `@compiler` registry is closed to two pure operations; its versioned
identity is `vibra_v1`. A trusted standard-library declaration may bind
`text.concat` with signature `str str -> str` and `text.length` with signature
`str -> u64`. Concatenation preserves Unicode scalar order; length counts
Unicode scalars rather than UTF-8 bytes. Both operations are total,
deterministic, and host-event free.
They accept no ambient input and have no runtime trap outcome.

No integer or floating compiler operation is admitted in M2. The v1
`integer.add-checked` declaration remains a valid source spelling but is
`@tool.unavailable` until its nominal `result` contract and overflow behavior
are implemented. `integer.increment` and `integer.to-str` likewise require
their own reviewed signatures. The checker must report availability before
lowering instead of wrapping, trapping, or fabricating a private result type.

Adding a compiler symbol requires a specification change to this table and its
registry tests; a string in source or a copied declaration cannot authorize an
operation. The M3 registry below replaces this profile as the Stage 3A steps
implement it; until then these two symbols are the implemented set.

## M3 compiler intrinsic registry

The Stage 3A `@compiler` registry is closed to the operations below, all under
the registry identity `vibra_v1`. Every operation is pure, total,
deterministic, and host-event free; none traps. A partial operation returns a
standard `option` or `result` instead. Composite behavior — boolean
connectives, character classes, searching, splitting, trimming, folds — is
ordinary standard-library Vibra over these operations and is specified by its
reviewed source, not by this table.

The error and ordering types are standard-library enums in `@std.core`, each
variant with a `void` payload:

| Type | Variants, in declaration order |
| --- | --- |
| `ordering` | `less`, `equal`, `greater` |
| `arithmetic-error` | `overflow`, `division-by-zero`, `invalid-shift` |
| `conversion-error` | `out-of-range`, `invalid-format`, `unrepresentable` |

In the signatures, `R t` abbreviates `(result t arithmetic-error)` and `C t`
abbreviates `(result t conversion-error)`.

For each integer type `T` among `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`,
and `u64`, the builtin type `T` has these static methods, with registry symbols
spelled `T.<name>`:

| Name | Signature | Semantics |
| --- | --- | --- |
| `add-checked`, `sub-checked`, `mul-checked` | `T T -> R T` | Exact result, or `overflow` when it lies outside `T` |
| `div-checked` | `T T -> R T` | Quotient truncated toward zero; `division-by-zero` for a zero divisor; `overflow` for the signed minimum divided by `-1` |
| `rem-checked` | `T T -> R T` | Remainder with the dividend's sign, so `left = quotient * right + remainder`; `division-by-zero` for a zero divisor; the signed minimum and `-1` yield `0` |
| `neg-checked` (signed `T` only) | `T -> R T` | Negation, or `overflow` for the minimum |
| `shift-left-checked` | `T u32 -> R T` | `left * 2^amount`; `invalid-shift` when the amount is at least the bit width, else `overflow` when the product lies outside `T` |
| `shift-right` | `T u32 -> R T` | `floor(left / 2^amount)`; `invalid-shift` when the amount is at least the bit width |
| `equal` | `T T -> bool` | Numeric equality |
| `compare` | `T T -> ordering` | Numeric order |
| `to-str` | `T -> str` | Shortest decimal digits, with a leading `-` only for a negative value and no suffix |
| `parse` | `str -> C T` | Accepts an optional `-` (signed `T` only) followed by one or more ASCII decimal digits and nothing else; `invalid-format` otherwise, `out-of-range` for a well-formed value outside `T` |

For `F` among `f32` and `f64`, the builtin type `F` has:

| Name | Signature | Semantics |
| --- | --- | --- |
| `add`, `sub`, `mul`, `div` | `F F -> F` | IEEE 754 operation in round-to-nearest, ties-to-even |
| `neg` | `F -> F` | IEEE 754 negation |
| `equal` | `F F -> bool` | IEEE 754 equality, so NaN is unequal to itself |
| `compare-total` | `F F -> ordering` | IEEE 754 `totalOrder` over canonicalized values |
| `to-str` | `F -> str` | The canonical float serialization of this chapter, without a suffix |
| `parse` | `str -> C F` | The unsuffixed decimal float literal grammar; `invalid-format` otherwise, `out-of-range` when the rounded value is infinite |

The remaining operations are bound by modules, except the `array.*` rows:

| Symbol | Signature | Semantics |
| --- | --- | --- |
| `char.to-u32` | `char -> u32` | Unicode scalar value |
| `char.from-u32` | `u32 -> (option char)` | `none` for a surrogate or a value above U+10FFFF |
| `text.concat` | `str str -> str` | Scalar concatenation |
| `text.length` | `str -> u64` | Scalar count |
| `text.equal` | `str str -> bool` | Equal scalar sequences |
| `text.compare` | `str str -> ordering` | Lexicographic by scalar value |
| `text.slice` | `str u64 u64 -> (option str)` | Scalars in the half-open range; `none` when start exceeds end or end exceeds the length |
| `text.to-chars` | `str -> (array char)` | Scalars in order |
| `text.from-chars` | `(array char) -> str` | Scalars in order |
| `text.to-utf8` | `str -> bytes` | UTF-8 encoding |
| `text.from-utf8` | `bytes -> C str` | `invalid-format` for ill-formed UTF-8 |
| `bytes.length` | `bytes -> u64` | Byte count |
| `bytes.concat` | `bytes bytes -> bytes` | Concatenation |
| `bytes.equal` | `bytes bytes -> bool` | Equal byte sequences |
| `bytes.compare` | `bytes bytes -> ordering` | Lexicographic by byte |
| `bytes.slice` | `bytes u64 u64 -> (option bytes)` | As `text.slice`, over bytes |
| `bytes.to-array` | `bytes -> (array u8)` | Bytes in order |
| `bytes.from-array` | `(array u8) -> bytes` | Bytes in order |
| `array.length` | `(array t) -> u64` | Element count; `where: (t any)` |
| `array.append` | `(array t) t -> (array t)` | New array with one trailing element |
| `array.concat` | `(array t) (array t) -> (array t)` | Elements of the first, then the second |
| `array.slice` | `(array t) u64 u64 -> (option (array t))` | As `text.slice`, over elements |

The numeric rows and the `array.*` rows are static methods of builtin types,
declared by the toolchain together with `array.of`, `map.of`, and `tuple.of` in
the embedded module `@std.builtin`. They are reached through the type path with
no import, exactly as the builtin types themselves need none, so
`(i32.add-checked left right)` needs no `import`.

`@std.option` declares `(deftype option (enum some t none void) where: (t any))`
and `@std.result` declares `(deftype result (enum ok t err e) where: (t any)
(e any))`; lookups, `try`, and unhandled-value checking recognize exactly these
two declarations by canonical identity. Map operations other than `map.of` and
lookup need a generic key parameter and belong to the Stage 3B registry.

A registry signature is checked exactly, including its generic parameter list,
against the trusted declaration that binds it. Adding, removing, or changing a
row is a specification change.

## Canonical value encoding

Execution results, assertion failure `expected` and `actual` strings, and the
`programResult` of `run` use one canonical VIBON encoding of a value. A
primitive value is its canonical literal. `bytes` is
`(record kind: @bytes values: (array b...))` with `u8` literals. Every other
value is a `record` whose first field is `kind:`:

| Value | Encoding |
| --- | --- |
| tuple | `(record kind: @tuple values: (array v...))` |
| array | `(record kind: @array values: (array v...))` |
| map | `(record kind: @map entries: (array (tuple k v)...))`, in canonical key order |
| record | `(record kind: @record type: P fields: (record name: v...))`, in declaration order |
| enum | `(record kind: @enum type: P variant: @name)`, adding `payload: v` for a non-`void` slot |
| newtype | `(record kind: @newtype type: P value: v)` |
| union | `(record kind: @union type: P member: T value: v)` |

`P` is the declaration's canonical atom path, and `T` is the canonical type
encoding: a primitive's atom such as `@i32`; a nominal declaration's canonical
atom path; `(record type: P arguments: (array T...))` for an applied generic
type; `(record type: @tuple arguments: (array T...))`, and likewise `@array`
and `@map`, for the builtin constructors; and
`(record type: @fn parameters: (array T...) labelled: (record name: T...)
result: T)` for a function type. A result observation is
`(record type: T value: v)`. Function values have no value encoding and are
never an observable result.

M2 test assertions are a separate closed test-runner outcome surface described
in the projects chapter. They evaluate through the ordinary typed call path,
perform no host operation, and have no compiler or host registry symbol. A
false assertion records `@test.assertion-failed` and stops only its current
test; it is not a trap and cannot be caught or converted into a Vibra
`result`. A trap raised by another runtime invariant remains `@test.trap` (or
`@command.trap` at the CLI boundary) with its origin and stable trap code.

## External providers

The unified source declaration surface has exactly two toolchain-owned external
providers:

- `@compiler` names a pure intrinsic with checked language semantics. It lowers
  to typed IR and never creates a runtime import.
- `@host` names an effectful host operation owned by its enclosing `deffect`.
  It lowers to the closed `vibra_v1` runtime registry.

Each provider has a closed, versioned symbol registry. Every entry declares:

- a stable string symbol and exact argument and result types;
- for `@host`, one owning standard effect root and deterministic audit-event
  shape; and
- for `@compiler`, pure deterministic semantics shared by the interpreter and
  Wasm lowering.

Unknown symbols, providers, Wasm imports, and WASI imports are rejected before
execution. User source cannot add registry entries. Composite behavior belongs
in Vibra standard-library code over small external operations.

The v1 inventory is value-in/value-out. Filesystem operations read or write
complete values for a supplied path; console, environment, clock, and random
operations likewise exchange ordinary typed values. There are no user-visible
file or stream handles, scoped resources, close operations, or resource
lifetime semantics in v1.

## Traps

Host responses that are ordinary environmental outcomes use typed `result`
errors. ABI mismatch, impossible typed IR, invalid host value IDs, and runtime
invariant violation are traps. A trap has a stable code and a source origin
when its failing source span is known; otherwise it has no origin. M2 failures
at the checked-program execution boundary (no executable entry or a body that
violates checked-IR invariants) use `@runtime.invalid-checked-program`. These
failures have no source origin and use an unlocated diagnostic primary at
`0..0` with no source ID. The CLI `trapCode` is the exact diagnostic-code
spelling as a string, and its `origin` is `null`. Traps are not catchable by
user code.

## WebAssembly boundary

The emitted module imports only compiler-generated `@host` entries from
`vibra_v1`. There is no source-level Wasm FFI, dependency-selected import
module, or user-declared import. The guest/host boundary is scalar-only: values
crossing it are fixed-width primitive scalars or checked opaque indices into an
instance-owned value arena. A `char` crosses as a validated Unicode scalar in
an `i32` slot. Guest pointers, shared linear-memory pointers, and host internals
do not cross the boundary.

The host validates every opaque value index for instance, kind, and liveness.
Index zero is invalid and IDs are not reused within an instance. The module
exports a versioned entry function and embeds deterministic custom sections for
source/build fingerprint, required registry entries, required effects, and
source-origin mapping.

Required-effect metadata is descriptive. The runtime validates ABI shape and
value types but receives no grant table, applies no effect-root policy, and
does not restrict path values. A conforming `vibra run` has already checked the
selected target's source-level effect ceiling.

An incompatible import signature, custom-section shape, value representation,
or entry contract requires `vibra_v2`; a runtime MUST NOT reinterpret it as v1.

## Determinism and observability

Given the same typed program, input values, and ordered host responses, a run
produces the same:

- return value or failure;
- stdout/stderr bytes; and
- ordered host-effect audit events.

Wall clock, environment, filesystem, and randomness are host inputs and are
observable only through their registered operations in source accepted by the
checker. Test hosts inject deterministic or recorded responses. The runtime
never reads ambient host state on behalf of an effect-free operation.

V1 defines no fuel, logical-memory, host-operation, or handle-count budget. An
embedding host may enforce external process or platform limits, but termination
by such a limit is a host event rather than a portable Vibra semantic result.

Compilation is deterministic: identical compiler version, typed program, and
options produce byte-identical Wasm and build data. Optimization is permitted
only after unoptimized interpreter/Wasm parity exists, and every optimization
must preserve conformance observations.

## Claims and limits

V1 claims source-level effect checking, closed `@compiler` and `@host`
registries, a closed compiler-generated host ABI, and interpreter/Wasm
conformance parity. It does not claim that an effectful program is host-safe,
provide a runtime sandbox, police paths within a declared effect, prevent
nontermination, verify compilation, ensure constant-time execution, prove
host-provider correctness, isolate native code, or safely execute arbitrary
foreign Wasm.

Running a checked binary target is consent to every host operation covered by
its declared roots. Deployments that need finer isolation may add an external
sandbox, but that policy is outside the v1 language and ABI contract.
