# Vibra v1 runtime and WebAssembly

Status: normative target
Implementation status: not started

## Semantic reference

The language semantics are independent of a backend. A small reference
interpreter is the executable oracle for evaluation, types after erasure,
effect events, and failures. The production compiler emits WebAssembly. A v1
implementation is conforming only when both produce the same observable
behavior for the conformance corpus.

The interpreter is not a second frontend: it consumes the same resolved typed
IR as the Wasm backend. Parsing, name resolution, type checking, performed-row
calculation, and external-registry validation are shared.

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
- `tuple.of`, `array.of`, and `map.of` operands evaluate from left to right;
- `try` performs only its specified early-exit propagation; and
- a tail-position call to a function in the same recursive group reuses the
  current activation instead of growing language-level stack.

`map.of` and map variadic tails share one construction rule. Every key and
value is evaluated even if a key repeats; the later pair replaces the earlier
value. Map iteration order is canonical key order, not insertion or hash-table
order.

Nominal and closed native constructor applications assemble immutable values;
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
results, and are outside interpreter/Wasm parity. Default `iter` method bodies
MAY lower to internal loops; that mutation is not a source feature.

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

Host responses that are ordinary environmental outcomes use typed `result`
errors. ABI mismatch, impossible typed IR, invalid host value IDs, and runtime
invariant violation are traps. A trap has a stable code and source origin but
is not catchable by user code.

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
