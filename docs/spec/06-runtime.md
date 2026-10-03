# Vibra v1 runtime and WebAssembly

Status: normative target
Implementation status: the reference interpreter executes successfully checked
IR for the complete pure language through M3 and the pre-M4 binding revision,
including tail calls and isolated tests, with an interim activation bound that
Stage 4A replaces. The Stage 4A contracts of this chapter (activations and
memory, the value arena, reclamation, the module contract, traps, and native
sources) are specified and unimplemented. The WebAssembly backend, host
operations, and effect checking remain unimplemented.

## Semantic reference

The language semantics are independent of a backend. A small reference
interpreter is the executable oracle for evaluation, types after erasure,
effect events, and failures. The production compiler emits WebAssembly. A v1
implementation is conforming only when both produce the same observable
behavior for the conformance corpus.

The interpreter is not a second frontend: it consumes the same resolved typed
IR as the Wasm backend. Parsing, name resolution, type checking, performed-row
calculation, and external-registry validation are shared.

Both backends consume only successfully checked IR. A toolchain's implemented
profile may be narrower than v1: a valid operation outside it is reported as
`@tool.unavailable` before lowering and is never reinterpreted as a generic
application. Neither backend reads ambient filesystem, environment, time, or
randomness state on behalf of an effect-free operation.

The reference interpreter is the oracle. The two backends are compared on
results, canonical value encodings, ordered audit traces, trap codes with their
origins, and the host events of the **Activations and memory** section, and on
nothing else: a backend's value layout, instantiation strategy, and stack use
are not observable. The conformance chapter defines how a runner executes one
case in both.

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
- a dict variadic tail builds one dict from alternating key/value forms and an
  odd tail is rejected before execution;
- function bodies and `do` forms evaluate from first to last; the value of a
  body sequence is that of its final element, or `void` if it is empty or ends
  in a `let` or `let-else`;
- `let` evaluates its pairs in order, matching each pattern against its value
  and binding its names before the next value is evaluated, and the bindings
  last until the enclosing body sequence ends;
- `let-else` evaluates its value once and matches its pattern; on a match its
  names are bound for the rest of the body sequence, and otherwise it evaluates
  its fallback, which never completes;
- `return` evaluates its operand and then leaves the innermost enclosing
  function activation with that value, skipping the rest of that activation;
- `if` evaluates only the selected branch;
- `match` evaluates its subject once and selects the first matching arm, and an
  `as` arm tests only the union discriminant;
- `tupleof`, `recordof`, and `enumof` operands evaluate from left to right,
  and `array.of` and `dict.of` follow the ordinary variadic order;
- `try` performs only its specified early-exit propagation; and
- a call in tail position reuses the current activation instead of growing
  language-level stack, whatever its callee is.

### Module-value initialization

Before accepting a checked program, the checker MUST build the dependency
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

`dict.of` and dict variadic tails share one construction rule. Every key and
value is evaluated even if a key repeats; the later pair replaces the earlier
value. Dict order is key order, by the `ordered.compare` of the key type, which
is canonical key order for every closed key type; it is never insertion or
hash-table order.

Constructor applications and the anonymous value forms assemble immutable
values;
they do not invoke a function body, add a function-call edge, or emit a host
event. Effects from evaluating their operands remain observable.

Tuple and record projection are lowered from their compile-time selector to a
direct component read. Array, dict, string, and byte application performs one
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

Floating-point operations follow IEEE 754. The sign and payload of a NaN are
never observable: every operation that observes a float, namely `equal`,
`compare-total`, `to-str`, `to-bits`, the canonical value encoding, and the
canonical equality of a test assertion, first replaces a NaN by the one quiet
NaN of its width, which has the positive sign and a zero payload. The other
float operations MAY return any NaN, so a backend whose engine leaves an
arithmetic NaN payload unspecified, as WebAssembly does, conforms without
canonicalizing each result. A backend MAY canonicalize eagerly as defence in
depth, since doing so is unobservable. Negative zero is normalized only where
the relevant standard operation explicitly says so.

The canonical float serialization writes NaN as `nan`, the infinities as `inf`
and `-inf`, and a finite value as the shortest decimal digits that round-trip
to it: in decimal notation with at least one fractional digit when its
magnitude is zero or in [1e-4, 1e16), as in `100.0`, `-0.0`, and `0.0025`, and
otherwise in scientific notation with one integral digit and no `+` sign, as in
`1e20` and `1.5e-7`. A value encoding appends the width suffix, as in
`100.0f64`; `to-str` does not. Every finite serialization is a valid float
literal body.

## Generic instantiation

Generics are not erased from observable behavior. Every activation of a
generic function runs at the type arguments its call instantiated, and the
result is as if the function had been written at those types:

- a value built in generic code has its instantiated type, so an `(array t)`
  built where `t` is `i32` is an `(array i32)` wherever it flows;
- a contract member call selects its implementation from instantiated types:
  the type the receiver holds, and for a generic interface the interface
  arguments of the call, because one receiver may implement the interface at
  several argument lists;
- a default member runs at the receiver type and the interface arguments of
  its own call, so a contract call in its body dispatches at them;
- a member selected by its destination whose destination is a generic
  parameter is selected from the type that parameter is instantiated to.

A closure runs at the type arguments of the activation that created it, and a
generic `lambda` adds its own at each call. An implementation MAY monomorphize
or pass type arguments at run time; a program cannot tell which.

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
- `let` and `let-else` have no body, so the values and patterns of a `let`, the
  value and pattern of a `let-else`, and the fallback of a `let-else` are all
  non-tail;
- the operand of `return` is in tail position, whether or not the `return` is,
  because it hands its value directly to the caller of the activation it exits;
- in `(if c t e)`, `t` and `e` may be in tail position only when the `if` is
  in tail position;
- in `(match s …)`, each arm's result expression may be in tail position only
  when the `match` is in tail position; and
- the operand of `try` is never in tail position, because `try` inspects that
  value before continuing.

Every other position is non-tail, including operands of applications (even when
they are the final expression inside a `do` that is itself an operand),
`let` and `let-else` values and patterns, `match` subjects, `if` conditions, and
`try` operands.

Every call in tail position MUST reuse the current activation, whatever its
callee is: a module-level `defn` of any module, a method of a declared type, a
`lambda` or closure, a function value held in a parameter, a binding, a record
field, or a collection, and a contract member, whether it is selected
statically or through an interface value. The callee replaces the caller in
that activation, with its own captures and type arguments, and the call's
result is the result of the activation. A program does not grow language-level
stack through calls in tail position, however many it makes and wherever they
lead. An application that creates no language activation, such as a
constructor, a projection, a lookup, a `@compiler` external, or a native
implementation, is not a call for this rule. This rule does not change which
positions are tail positions.

A backend MAY implement this with explicit tail-call instructions, a dispatch
loop, or any other strategy; the strategy is not observable except that
conforming programs do not grow language-level stack through calls in tail
position. Default `iter` method bodies MAY lower to internal loops; that
mutation is not a source feature.

## Activations and memory

V1 defines no activation-depth limit. A non-tail call creates an activation
that lasts until the call returns, and the depth of nested activations is
bounded only by the memory available to the instance, exactly as the size of
any other value is. A conforming backend MUST NOT consume host or engine stack
per language activation: it holds activations in the instance's own storage
(the arena of **The value arena**), so a program that recurses to any depth the
instance's memory admits behaves as if depth were unbounded, in both backends.
A call in tail position replaces its caller's activation in that storage, as
the **Tail calls** section requires.

Exhausting the memory available to an instance, whether by deep recursion,
by allocation, or by exhausting the value-ID space defined below, is a **host
event**: it is not a trap and not a portable semantic result of the program,
because the limit belongs to the host. It stops execution with the unlocated
error diagnostic `@runtime.memory-exhausted` (primary span `0..0`, no source
ID). `run` and `test` report it as `@command.operational-failure` (exit 3), and
`test` then reports zero selected, passed, and failed tests. A program that
completes under one memory limit and exhausts a smaller one has not
violated parity. Both backends MUST report the same host event when memory is
exhausted, and a conformance case that exhausts memory states only that event.

## Test assertions

Test assertions are a separate closed test-runner outcome surface described
in the projects chapter. They evaluate through the ordinary typed call path,
perform no host operation, and have no compiler or host registry symbol. A
false assertion records `@test.assertion-failed` and stops only its current
test; it is not a trap and cannot be caught or converted into a Vibra
`result`. A trap raised by another runtime invariant remains `@test.trap` (or
`@command.trap` at the CLI boundary) with its origin and stable trap code.

## M3 compiler intrinsic registry

The Stage 3A `@compiler` registry is closed to the operations below, all under
the registry identity `vibra_v1`. Every operation is pure, total,
deterministic, and host-event free; none traps. A partial operation returns a
standard `option` or `result` instead. Composite behavior — boolean
connectives, character classes, searching, splitting, trimming, folds — is
ordinary standard-library Vibra over these operations and is specified by its
reviewed source, not by this table.

The registry has two tiers. A **primitive operation** is one the language core
cannot express in Vibra: arithmetic and comparison on the scalar types,
`char` conversions, and the construction, length, indexing, and slicing of
`(array t)`. It is declared with `external: @compiler` and has no body. Every
other row operates on a library type and is a **native implementation**
(below): its meaning is a standard-library Vibra body, and the row only names
the accelerated implementation a toolchain may run instead. Until a library
type moves into the standard library, its rows are primitive operations over
the toolchain's direct representation; the roadmap names each migration.

The `to-str` and `parse` rows of the integer and float types are expressible
in Vibra over the primitive operations, so they are native implementations
whose meaning is a Vibra body declared in `@std.builtin`, and the float bodies
read a float's bits through the primitive `to-bits` and `from-bits` rows. The
single source of every native implementation is its body, as the **Native
implementations** section states. Until Stage 4A moves them, a toolchain
implements the rows as primitive operations, which is observationally the same.

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
| `to-str` (native) | `T -> str` | Shortest decimal digits, with a leading `-` only for a negative value and no suffix |
| `parse` (native) | `str -> C T` | Accepts an optional `-` (signed `T` only) followed by one or more ASCII decimal digits and nothing else; `invalid-format` otherwise, `out-of-range` for a well-formed value outside `T` |
| `to-U`, for each other integer type `U` | `T -> U` when every `T` value is a `U` value, else `T -> C U` | The same integer as a `U`; `out-of-range` when `U` cannot hold it |

For `F` among `f32` and `f64`, the builtin type `F` has:

| Name | Signature | Semantics |
| --- | --- | --- |
| `add`, `sub`, `mul`, `div` | `F F -> F` | IEEE 754 operation in round-to-nearest, ties-to-even |
| `neg` | `F -> F` | IEEE 754 negation |
| `equal` | `F F -> bool` | IEEE 754 equality, so NaN is unequal to itself |
| `compare-total` | `F F -> ordering` | IEEE 754 `totalOrder` over canonicalized values |
| `to-bits` | `f32 -> u32`, `f64 -> u64` | The IEEE 754 interchange bits of the canonicalized value, so every NaN reads as the canonical quiet NaN |
| `from-bits` | `u32 -> f32`, `u64 -> f64` | The float with exactly those bits; a pattern that encodes a NaN yields a NaN, whose payload is unobservable |
| `to-str` (native) | `F -> str` | The canonical float serialization of this chapter, without a suffix |
| `parse` (native) | `str -> C F` | The unsuffixed decimal float literal grammar; `invalid-format` otherwise, `out-of-range` when the rounded value is infinite |

The remaining operations are bound by modules, except the `char.*`, `array.*`,
and `dict.*` rows. The `text.*` and `bytes.*` rows are native implementations of
the `@std.text` and `@std.bytes` functions, whose Vibra bodies over the scalars
and bytes a `str` or `bytes` value is written over are their meaning, as are
`array.of`, `array.fold`, and `dict.of`:

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
| `array.of` | `-> (array t)`, `variadic: (items (array t))` | The packed tail; `where: (t any)` |
| `dict.of` | `-> (dict k v)`, `variadic: (entries (dict k v))` | The packed tail, in key order, a later entry replacing an equal key; `where: (k ordered v any)` |
| `dict.entries` | `(dict k v) -> (array (tuple k v))` | The entries in key order |
| `array.length` | `(array t) -> u64` | Element count; `where: (t any)` |
| `array.append` | `(array t) t -> (array t)` | New array with one trailing element |
| `array.concat` | `(array t) (array t) -> (array t)` | Elements of the first, then the second |
| `array.slice` | `(array t) u64 u64 -> (option (array t))` | As `text.slice`, over elements |
| `array.fold` | `(array t) a (fn (a t) a) -> a` | The left fold: `step` applied to the accumulator and each element in order, starting from `initial`; `where: (t any)` on the type and `(a any)` on the member |

The numeric rows and the `char.*`, `array.*`, and `dict.*` rows are static
methods declared in the embedded module `@std.builtin`: in the `intrinsic-type`
declarations of the builtin types, and in the `dict` declaration that plays
`@dict`. They are reached through the type path with no import, exactly as
those types themselves need none, so `(i32.add-checked left right)` needs no
`import`.

`@std.option` declares `(deftype option (enum some t none void) where: (t any))`
and `@std.result` declares `(deftype result (enum ok t err e) where: (t any)
(e any))`; lookups, `try`, and unhandled-value checking recognize exactly these
two declarations by canonical identity. A dict is read by lookup and by
`dict.entries`, its one primitive operation: the entries a dict is declared
over.

A registry signature is checked exactly, including its generic parameter list,
against the trusted declaration that binds it. Adding, removing, or changing a
row is a specification change.

## Native implementations

A standard-library function MAY carry `native: "symbol"` beside its ordinary
Vibra body. The body is the function's meaning: checking, effects, the
canonical value encoding, and every observable result come from it. The native
implementation is a toolchain-owned replacement for executing that body, and
it MUST be observationally identical to it for every input, including the
result of every partial operation and the absence of traps and host events.

Native implementations are closed exactly as the primitive operations are:
only the embedded standard library may write `native:`, the standard-library
manifest lists every native symbol, and a symbol the toolchain does not
implement is a toolchain defect reported as an operational provenance
diagnostic. A toolchain MAY execute the body instead of the native
implementation, so a native implementation is never needed for a correct
result.

The body is the single source of every native implementation. The WebAssembly
backend executes the body: it has no native implementation of its own, a
native symbol is never lowered into a module, and so the two backends cannot
drift apart. A reference interpreter MAY run its toolchain-owned native
implementation instead of the body. The conformance suite holds that native
implementation to its body over the same inputs, and holds the body to itself
across backends: for every sample input, the interpreter's native
implementation, the interpreter's body, and the module's body produce the same
result. This is not a foreign-function interface: packages cannot declare
`native:`, and a native implementation reaches no host operation.

## The value arena

An **instance** is one execution of a program: it owns its module-value state,
its activations, and every value it creates, and it shares none of them with
another instance. A value is never observed across instances, and a value
that has crossed a host boundary is observed only through the operations that
boundary defines.

A value is a **scalar** or an **arena value**. The scalars are the values of
`void`, `char`, the integer types, and the float types. `bool` is the
standard-library enum of the `@bool` role, so it is an `enum` arena value like
any other. Every other
value is an arena value of one of the closed kinds `atom`, `str`, `bytes`,
`tuple`, `array`, `dict`, `record`, `enum`, `wrapper`, `union`, and `function`.
The arena is the instance's own value storage. A representation MAY keep a
value of some other kind unboxed, and the kinds name what a host can see, not
what a layout stores.

A backend with a host boundary assigns each arena value that crosses the
boundary a **value ID** and gives the host only that ID:

- an ID is an unsigned 64-bit integer, and `0` is never a valid ID;
- an ID is assigned by the instance, valid for that instance alone, and never
  reused within it, even after its value is released;
- the instance keeps a handle table in which each ID the host holds keeps its
  value live; and
- an ID names a value and not a place: no ID, offset, or address of the arena
  appears in typed IR, in a canonical value encoding, in an audit event, or in
  any conformance snapshot.

Within a WebAssembly module the arena lives in the module's own linear memory
and is managed by compiler-emitted code. Values refer to one another by offset
and never move, so an offset is stable for the life of its value. No offset
crosses the guest/host boundary. The reference interpreter's arena is its own
heap and needs no IDs until it serves a host.

## Reclamation

A value is **live** while it is reachable from a live activation, from a
module-level value, or from a value ID the host holds. The storage of a value
that is no longer live MAY be released at any time, and a backend MUST release
it before it lets a program whose live data stays bounded grow without bound.
Reclamation is unobservable apart from memory use: no result, trap, audit
event, or ID changes because storage was or was not released.

In particular, a tail-recursive loop that allocates a fresh compound value on
each iteration and keeps none of the earlier ones live runs with a bounded live
arena: the live arena after a hundred thousand iterations exceeds the live arena after
ten thousand by at most a constant that depends on the program and not on the
iteration count. A WebAssembly backend MUST be able to report its live arena
size, in bytes of storage held by live values and by the handle table, so a
conformance harness can measure this. Releasing a value
MUST use bounded engine stack however deeply that value is nested, so a
backend releases through an explicit worklist and never through recursion on
the host stack.

Running out of memory, or of IDs, while allocating is the host event
`@runtime.memory-exhausted` of **Activations and memory**. It is not a trap.

No value can reach itself, so a backend MAY reclaim by counting references to
each value and releasing a value when its count reaches zero, without a
cycle collector:

- values are immutable and are built only from values that already exist, so a
  value refers only to values older than itself;
- a module-level `def` cannot reach itself, because an initializer cycle is
  rejected with `@type.initializer-cycle`;
- a module function is named by its identity and never held by a counted
  reference, and a closure holds only the local values it captured and its
  type arguments, because a module-level value its body names is read from the
  instance's module state when the body runs;
- a `let` or `let-else` binding is visible only from the end of its own pair, so
  a `lambda` cannot capture the value that is being bound to it; and
- an in-place update, where a backend performs one, applies only to a value
  that no other live reference reaches, so it cannot make that value refer to a
  younger value that refers back to it.

## Representation latitude

The language core fixes observable behavior, not representation. A toolchain
MAY, without any observable difference and without any promise to do so:

- represent a wrapper type exactly as its representation;
- represent an enum whose variants all have `void` payloads as a small integer,
  and choose compact layouts for other enums, such as a nullable reference for
  an `option` of a reference type; and
- update a value in place when no other reference to it can observe the update,
  so that a standard-library operation over an unshared array or dict avoids a
  copy.

These are implementation strategies, not guarantees; v1 makes no
optimization promise.

`never` has no value and therefore no representation. No checked program
allocates, stores, passes, or encodes one: an expression of that type ends its
activation with `return`, or never completes, and no later evaluation consumes
a value from it. A representation that lays out a type, such as an enum payload
or a function result slot, may omit a slot or a variant of type `never`.

## Canonical value encoding

Execution results, assertion failure `expected` and `actual` strings, and the
`programResult` of `run` use one canonical VIBON encoding of a value. A
primitive value is its canonical literal. `bytes` is
`(record kind: @bytes values: (array b...))` with `u8` literals. Every other
value is a `record` whose first field is `kind:`. A value of a declared type
carries `type: P` as its second field; an anonymous structural value omits it:

| Value | Encoding |
| --- | --- |
| tuple | `(record kind: @tuple type: P values: (array v...))` |
| array | `(record kind: @array values: (array v...))` |
| dict | `(record kind: @dict entries: (array (tuple k v)...))`, in canonical key order |
| record | `(record kind: @record type: P fields: (record name: v...))` |
| enum | `(record kind: @enum type: P variant: @name)`, adding `payload: v` for a non-`void` slot |
| wrapper | `(record kind: @wrapper type: P value: v)` |
| union | `(record kind: @union type: P member: T value: v)` |

Declared record fields appear in declaration order and anonymous record fields
in canonical order.

`P` is the declaration's canonical atom path, and `T` is the canonical type
encoding: a primitive's atom such as `@i32` or `@never`, where `@never` appears
only inside the encoding of another type, such as `(result t never)`, because no
value has type `never` and none is encoded; a declared type's canonical atom
path; `(record type: P arguments: (array T...))` for an applied generic type,
including `@array` and `@dict`; `(record type: @tuple arguments: (array T...))`
for an anonymous tuple; `(record type: @record fields: (record name: T...))`,
`(record type: @enum variants: (record name: T...))`, and
`(record type: @union members: (array T...))` for the other anonymous types,
in canonical order; `(record type: @atom-singleton atom: @name)` for the
singleton type of a written atom; and `(record type: @fn parameters: (array T...) labelled:
(record name: T...) result: T)` for a function type, with labelled parameters
in name order and `variadic: T` before `result` when the function has a
variadic tail. A result observation is `(record type: T value: v)`. Function
values have no value encoding and are never an observable result: the checker
rejects a type that names a function where a value is observed, and a
function hidden behind `any` or an interface that reaches a test assertion or
the entry's result is the trap `@runtime.unobservable-function`.

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
when its failing source span is known; otherwise it has no origin. The trap
codes are closed:

| Code | Raised when | Origin |
| --- | --- | --- |
| `@runtime.invalid-checked-program` | The checked program has no executable entry, a body violates the checked-IR invariants, or the toolchain itself fails, including every engine trap that generated code did not raise | None |
| `@runtime.unobservable-function` | A value that holds a function reaches an observation | The assertion call in a test; none for the entry's result |
| `@runtime.invalid-host-value` | A host operation receives a value ID that is zero, was never issued by the instance, has been released, or names a value of a kind the operation does not admit, or an index outside the value | None |

A failure with no origin uses an unlocated diagnostic primary at `0..0` with no
source ID. The CLI `trapCode` is the exact diagnostic-code spelling as a
string, and its `origin` is `null` when the trap has none. `run` reports the
trap the same way `test` does: the result is `@command.trap`, the diagnostic is
in the envelope, and the payload's `trap` holds the code.

An engine reports traps by its own rules, such as an unreachable instruction,
an out-of-bounds access, a mismatched indirect call, an arithmetic trap, or an
exhausted call stack. None of these is a language outcome, because every
partial operation of the language returns an `option` or a `result` and no
checked program performs an operation that traps by an engine's rules. Code a
backend generates raises the registered traps itself: it records the trap code
and its origin in the instance before it stops, and a host reads that record.
An engine trap with no such record is a defect of the toolchain, reported as
`@runtime.invalid-checked-program` with no origin, and no conformance case
expects it. Exhausting memory is the host event of **Activations and memory**,
never a trap. A module carries no source map before Milestone 7, so it names a
trap's origin by an ordinal into an origin table that the toolchain produces
with the module and that maps each ordinal to one source span. Both backends
MUST report the same code and the same origin for every registered trap.

`@runtime.unobservable-function` is the trap of a value that holds a function
reaching an observation. A test assertion compares canonical value encodings
and `run` reports the entry's result as one, and a function has none. The
checker rejects an observed type that is or contains a `fn` type, so the trap
is left to a function hidden behind `any` or an interface, or held by a
library type such as `iter`. In a test its origin is the assertion call and
the item is `@test.trap`; for the entry's result it has no origin and `run`
ends with `@command.trap`.

Traps are not catchable by user code.

## WebAssembly boundary

A WebAssembly module the toolchain emits is a **v1 module**. It uses only the
core features of WebAssembly 2.0 that this paragraph names: the MVP
instruction set and function tables, multiple results, bulk memory operations,
sign-extension operators, and non-trapping float-to-integer conversions. It
does not use the tail-call, garbage-collection, exception-handling, SIMD,
threads, reference-type, memory64, or multi-memory features, so no conforming
engine needs an optional proposal and a module does not depend on the engine's
stack or collector. A v1 module validates under exactly that feature set.

A module has one defined 32-bit linear memory, which holds its arena and is
never exported, and no exported global. Its imports are empty in Stage 4A: it
calls no host operation and reads no ambient state. From Stage 4B it imports
only compiler-generated `@host` entries from `vibra_v1`. There is no
source-level Wasm FFI, dependency-selected import module, or user-declared
import. The guest/host boundary is scalar-only: values crossing it are
fixed-width primitive scalars or value IDs of the instance's arena. A `char`
crosses as a validated Unicode scalar in an `i32` slot, and the integer types
narrower than 32 bits cross in an `i32` slot, signed ones sign-extended and
unsigned ones zero-extended. Guest pointers, offsets into linear memory, and
host internals do not cross the boundary.

A Stage 4A module exports exactly the functions below. Every export name is
prefixed `vibra_v1_`, which is the module's version: an incompatible change to
a name, signature, or meaning requires `vibra_v2`. A program module exports
`vibra_v1_entry` and a test module exports `vibra_v1_test`; no module exports
both.

| Export | Type | Meaning |
| --- | --- | --- |
| `vibra_v1_entry` | `() -> ()` | Runs the binary target's entry |
| `vibra_v1_test` | `(i32) -> ()` | Runs the test at that zero-based position in canonical discovery order; a host runs each test in a fresh instance |
| `vibra_v1_status` | `() -> i32` | What the last call recorded: `0` nothing, `1` a trap, `2` the memory host event, `3` a failed assertion |
| `vibra_v1_trap_code` | `() -> i32` | After status `1`: `1` for `@runtime.invalid-checked-program`, `2` for `@runtime.unobservable-function`, `3` for `@runtime.invalid-host-value` |
| `vibra_v1_origin` | `() -> i32` | After status `1` or `3`: the origin ordinal of the trap or the assertion call, `0` for none |
| `vibra_v1_failure` | `() -> i32` | After status `3`: `1` for `assert.true`, `2` for `assert.false`, `3` for `assert.equal` |
| `vibra_v1_failure_expected`, `vibra_v1_failure_actual` | `() -> i64` | After status `3` and `assert.equal`: the two operands, as the bits of a scalar or as a value ID, by the operand type that the origin table records |
| `vibra_v1_result` | `() -> i64` | After a completed entry: the ID of its result, `0` when the result is `void` |
| `vibra_v1_live_size` | `() -> i64` | The live arena size in bytes, as the **Reclamation** section requires |
| `vibra_v1_release` | `(i64) -> ()` | The host drops its hold on a value ID |
| `vibra_v1_variant` | `(i64) -> i32` | The variant index of an enum, in declaration order, or the member index of a union, in written order |
| `vibra_v1_length` | `(i64) -> i64` | The scalar count of an `atom` or `str`, the byte count of `bytes`, the element count of an `array`, the entry count of a `dict`, and the component count of a `tuple` or `record` |
| `vibra_v1_read_i32`, `vibra_v1_read_i64`, `vibra_v1_read_f32`, `vibra_v1_read_f64` | `(i64, i64) -> T` | The scalar component at an index: a character of a `str` or `atom`, a byte of `bytes`, an element, a tuple or record component in field order, or the payload of an enum, wrapper, or union |
| `vibra_v1_read_id` | `(i64, i64) -> i64` | A compound component at an index, as a new ID that the host MUST release; the entry at an index of a `dict` is a two-component `tuple` |

A call to `vibra_v1_entry` or `vibra_v1_test` that returns has completed. A call
that stops, as an engine trap, did so because generated code recorded one of
the statuses above before it stopped, and the host reads the record with the
accessors; a stop with status `0` is a toolchain defect, reported as
`@runtime.invalid-checked-program`. An ID, index, or kind that an accessor does
not admit records the trap `@runtime.invalid-host-value`. A function value has
no components, so a host that reaches one while observing reports
`@runtime.unobservable-function`. Stage 4B adds the operations by which a host
builds compound values and the host imports, and writes them against this
interface.

The host validates every value ID for instance, kind, and liveness. Index zero
is invalid and IDs are not reused within an instance. A v1 module the
toolchain emits before `vibra build` exists embeds no custom section. The
origin table is produced beside the module and is not part of its bytes. A
build product embeds deterministic custom sections for source/build
fingerprint, required registry entries, required effects, and the
source-origin mapping that replaces the origin table.

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

V1 defines no fuel, host-operation, or handle-count budget, and no portable
memory limit: the limit that produces `@runtime.memory-exhausted` belongs to the
embedding host. An embedding host may enforce other external process or
platform limits, but termination by such a limit is a host event rather than a
portable Vibra semantic result.

Compilation is deterministic: identical compiler version, typed program, and
options produce byte-identical Wasm and build data. A module depends on no
clock, absolute path, address, hash-table order, or thread, so emission from an
identical checked program is byte-identical from the first module the toolchain
emits, before any build command exists. Optimization is permitted
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
