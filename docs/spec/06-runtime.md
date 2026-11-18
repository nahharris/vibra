# Vibra runtime and native execution

Status: normative target
Implementation status: this chapter describes how the rewritten language
executes. It does not claim that the new language, the native profile, the
static guard checker, or the scoped drivers exist. The
[roadmap](../roadmap/README.md) schedules them, and the
[conformance gate](07-diagnostics-and-conformance.md#full-conformance-gate)
decides when a release may claim them.

## Semantic reference

The meaning of a program does not depend on what executes it.

- **The interpreter stays.** The reference interpreter MUST remain available
  permanently, for reference execution, tests, bounded evaluation, and the
  compile-time evaluation of pure functions. It consumes the same resolved
  and checked IR as the compiler. It MUST NOT be a second source frontend.
- **The native pipeline** is: checked IR, verified execution IR, CPU object
  code, a runtime archive for the target, and the platform linker. The
  execution-IR stepper is a tool for locating lowering and runtime defects.
  It is not another dialect of the language.
- **Agreement.** For the supported executable profile, native execution and
  the interpreter MUST agree on results, canonical observations, the order
  of completed host events, typed failures, defect traps, and source
  origins. Expected results MUST be written from this specification and
  MUST NOT be regenerated from a backend's output. Agreement is evidence; it
  is not a proof that the compiler is correct.
- **No partial runs.** An executor that does not support an admitted
  operation MUST reject the program before running any of it. The result is
  unavailable or unsupported, with the profile and the reason. It MUST NOT
  be a success, a silently skipped case, or a different program.
- **Pure means pure.** A pure operation MUST NOT read the filesystem, the
  environment, a clock, a random source, or the network.

WebAssembly is a transition backend only. It is removed at the native
cutover, and [Wasm transition](#wasm-transition) states what it owes until
then. Source has no Wasm provider, no Wasm or WASI import, and no
interoperability bridge.

## Evaluation

Evaluation is strict and, apart from explicit host inputs, deterministic.

- **Order.** A callee is evaluated once, then every operand in **written
  order**. This includes labelled arguments, record fields, and dict keys
  and values. The evaluated operands are bound to their slots afterwards;
  declaration order and canonical storage order MUST NOT reorder evaluation.
- **Defaults** are checked constants. They are not computations deferred to
  the caller.
- **Statements** of a body run first to last. Every path that returns
  normally MUST use `return` or a supported `try`. The final expression of
  an `if`, `match`, or `do` expression is that block's value; it is not an
  implicit return from the enclosing callable.
- **`let`** evaluates its initializer once, before its binders exist. When
  the pattern matches, the binders exist after `endlet` for the rest of the
  directly enclosing sequence. When a refutable `let … else` fails, only the
  fallback runs, and every path of it MUST leave the sequence.
- **`return`** evaluates its operand and leaves the nearest callable
  activation, subject to
  [callable contract execution](#callable-contract-execution). The
  remaining statements are skipped.
- **`try`** evaluates its operand once and propagates only as the type
  chapter specifies. It converts no error.
- **`if`** runs only the selected branch. **`match`** evaluates its subject
  once and runs the first arm that matches; no other arm's body is
  evaluated.
- **Tuples, records, lists, and dicts** evaluate their operands in written
  order. Every key and value of a dict literal is evaluated, including a
  later duplicate key, whose value replaces the earlier one. A dict is
  traversed in `Ordered.compare` key order, never in insertion or
  hash-table order.
- **Construction** assembles a value and applies the type's `where`. It is
  not a method call. A discharged bare `where` leaves no run-time check.
- **Reads.** A field read yields the selected component. Tuple components
  are obtained by pattern; there is no tuple index. A lookup by application
  on a `List`, `Dict`, `Str`, or `Bytes` yields `Option.some(value: item)`
  or `Option.none()`. A missing key or an index out of range MUST NOT trap
  and MUST NOT produce a default.
- **Procedures.** Applying a `proc` builds a cold plan. `await` runs it, as
  [cold plans and invocation identity](#cold-plans-and-invocation-identity)
  describes.
- **Tail calls.** A call in tail position MUST reuse its activation,
  whatever its callee is. [Tail calls](#tail-calls) defines the position.

`Str` positions and counts are in Unicode scalars; `Bytes` positions and
counts are in bytes. A `Char` is a Unicode scalar: not a surrogate and not
above U+10FFFF. Conversion from `Str` to bytes is explicit UTF-8. `Void` has
exactly one value. A numeric suffix fixes a literal's type before execution,
and a literal out of range is rejected before checked IR exists. A suffix
does not change what arithmetic means.

### Module-value initialization

A module-level `let` is immutable and typed. The checker MUST reject a cycle
among module-value initializers before it produces an executable program.

- A dependency includes a read reached through a helper function, a
  closure, a callable alias, or indirect call flow.
- Recursion among functions alone is not an initializer cycle. A path that
  comes back to a module value still being initialized is one.
- A forward reference without a cycle is valid.
- The error is `@type.initializerCycle`. It names one `let` on the cycle
  and relates the cycle's edges in a deterministic order.
- Checking MUST NOT run an initializer to learn a type.

A module value is initialized lazily, on its first read, and exactly once
per execution. Later reads reuse the value. Each execution starts with fresh
module state. A runtime may track the states unread, evaluating, and ready;
the evaluating state is not a way to accept a cycle the checker should have
rejected.

## Callable contract execution

The [type chapter](02-type-system.md#guards) defines the static rules for
guards. Lowering MUST preserve the distinctions below, and MUST NOT
introduce a trap for a failed guard.

**Bare guards leave no code.** A bare `requires` is established at each
direct call, or where a plan is built. A bare `ensures` is established for
every return of the body. A bare `where` leaves the constructor returning
its own type. An obligation that is unproved or unsupported MUST NOT become
a run-time assertion. Proved, refuted, and "the tool ran out of budget" are
three different statuses.

**`requires C else V`.** On entry the runtime evaluates `C` once.

- False: `V` is evaluated once and returned at the declared result type.
  **The body and the `ensures` are both skipped.**
- True: the body runs with `C` established.

**`ensures C else V`.** The value the body returns, including one
propagated by `try`, is the candidate. It is bound to the result name, and
`C` is evaluated once.

- True: the candidate is returned.
- False: `V` is evaluated once, with the result name still bound to the
  candidate, and returned. `V` is not checked again, nothing is retried,
  and the guard is not re-entered.

So an `ensures … else` does not promise `C` of the final value, and a tool
MUST NOT present it as if it did. Specifying it needs no run-time equality
operation.

**`where P else E`.** The predicate is applied to the complete raw
representation before any value of the type exists.

- True: the constructor returns `Result.ok(value: validated)`.
- False: it returns `Result.err(error: E)`, where `E` is the written
  constant.

The constructor always has this `Result` type, even when a particular
construction is proven. Taking a valid value apart by pattern is infallible
and does not run the predicate again. A decoder or driver that produces a
value of the type MUST validate the raw data or hold checked evidence that
the predicate holds; it MUST NOT construct an invalid value by going around
`where`.

**Failures inside a guard.** Predicates and consequences are certified
expressions, which cannot fault. If a defect, or exhaustion of a host or
tool limit, nevertheless occurs inside a trusted helper, the runtime MUST
report that cause. It MUST NOT turn it into "the predicate was false" or
into an author's error value.

**Proven guards.** Proving a check true lets the toolchain remove the
predicate and the fallback. It MUST still evaluate the operands, keep the
result type, and run every remaining effect and continuation. Proving a
check false selects the written consequence; it does not delete it.

### Cold plans and invocation identity

A plan is an immutable, reusable value whose checked type, `Intent`, is
distinct from `Fn`. A runtime may give it the layout of a closure: the
resolved entry, the captured operands, the type arguments, and the
environment.

- **Building does nothing.** Building or copying a plan MUST NOT run a
  procedure's condition, body, or consequence, and MUST NOT perform a host
  operation.
- **Each `await` is a new invocation.** A `requires … else` runs on each
  one. A bare `requires` was already established on the captured values; it
  is not a permission check on a resource in the future.
- **A plan holds no run state.** The state of an invocation, a submission,
  or a retry lives in the execution and its driver. A plan has no
  "consumed" bit and no memoized result.
- **Conversion keeps the contract.** A callable converted to a callable type
  keeps its entry and exit behavior. The conversion never exposes the bare
  body.
- **The row survives.** A plan's complete row is preserved through capture,
  records, returns, and interface dispatch.
- **Where `await` runs.** Only in the execution of a `proc`, or in a test
  body, which runs in a procedure context the test runner owns. It never
  runs inside a `fn`.

One plan may contain several operation invocations, each with its own
delivery class. `@once` forbids automatic redispatch after a possible
submission within one invocation. A second explicit `await` is a separate
invocation, not such a retry. A completion that is unknown MUST NOT be
treated as "it did not run".

## Generic instantiation

A generic activation MUST behave as if its body had been written at its
actual type arguments.

- A `List(T)` built where `T` is `I32` is a `List(I32)` wherever it goes.
- Member selection uses the closed
  [substitution](02-type-system.md#substitution) of the type chapter: the
  instantiated receiver, the interface's positional arguments, and the
  selected implementation's choices. That substitution is the dispatch
  evidence of the activation.
- A default member runs under the substitution of the implementation
  selected for its own call, so an associated name in its body denotes that
  implementation's choice.
- A destination-dispatched conversion uses the instantiated, written
  destination type.
- A type parameter bound to a choice by label, as in
  `given {T: Ordered I: Iter(Item: T)}`, is instantiated at the choice of
  the selected implementation, and its own bound's evidence comes from that
  type.
- A closure and a plan keep the type arguments and the dispatch evidence of
  the activation that created them. A generic anonymous callable adds its
  own when it is called.

The first native profile passes type descriptors at run time. A later
optimization MAY specialize instead, only if no observation changes.
Dispatch through an interface value MUST keep the concrete implementation,
within the restrictions of the
[type chapter](02-type-system.md#interface-values). It MUST NOT invent a
conformance, and MUST NOT lose the row of a procedure or a plan.

## Tail calls

An expression is in **tail position** only when its value goes to the
caller of the enclosing activation with no work left to do.

| Expression | Tail position? |
| --- | --- |
| the operand of `return` | yes, if no `ensures … else` check, widening, or other continuation remains |
| the final value of a `do`, `if`, or `match` | only if the whole expression is in tail position |
| a condition, a `match` subject, any other operand | no |
| a `let` initializer; pattern matching | no |
| a `return` inside a `let … else` fallback | yes, under the same rule |
| the operand of `try` | no: the carrier is inspected after the call |

Widening to an interface value is work after the return. A bare `ensures`,
proved and erased, is none. An `ensures … else` that remains is work.

Every call in tail position MUST replace its activation. This holds for
module functions, anonymous functions, captured and parameter values,
functions held in fields or collections, and interface members, dispatched
statically or through an interface value. The captures and type arguments
become the callee's. Constructors, field reads, lookups, and primitives
create no activation and are not calls for this rule.

A backend MUST NOT keep a growing chain alive by moving continuations out
of the frame. Recursion that has real work after the call is not a tail
call. It may consume memory, and no constant-space promise covers it.

## Activations and memory

An instance owns its module state, activations, values, provider world, and
execution metadata.

- A call that is not a tail call creates a frame, which the runtime owns
  until the return. A tail call replaces the frame.
- The language has no activation-depth limit of its own.
- Frames live in storage the instance owns. The interpreter and the native
  executor MUST NOT consume host or engine stack in proportion to the depth
  of Vibra calls.
- The first native context addresses at most 4 GiB, because its internal
  value offsets are 32 bits wide. Frames and values count toward the
  context's configured memory limit, which may be lower. The limit belongs
  to the host and the profile; source cannot observe it.

**Memory exhaustion** is the host event `@runtime.memoryExhausted`.

- It is not a `Result`, a validation error, or a trap.
- It has no source location: primary span `0..0` and no source identity.
- The tool outcome is an operational failure, exit 3.
- A test run stopped by it MUST NOT report the tests that did not run as
  passed.
- Two runs that differ only because their memory limits differ are not a
  parity violation. A comparison MUST state the limits and compare like
  profiles.

Limits on work, time, and output that a tool or host enforces from outside
likewise give an explicit, incomplete outcome.

**No fuel.** The language has no fuel counter and no cost proof. The
scheduler's quota only suspends and resumes at checkpoints and never changes
what a sequential program observes. A primitive finishes atomically, and
its proportional work is charged at the next checkpoint. Neither the quota
nor cooperative cancellation interrupts a primitive or a blocking host call
in bounded time.

## Test assertions

A test body runs in an isolated procedure context that the runner owns,
against a deterministic fake world, under the row the test declares. Its
result is `Void`, and it cannot `return` a value or use `try` directly.

A test obtains fake grants only from the `TestWorld` it takes, as
[test worlds and grants](04-programs-and-packages.md#test-worlds-and-grants)
defines. A row alone MUST NOT create a grant or permit a real effect.

- An assertion is an ordinary typed call. It performs no host operation and
  has no registry symbol.
- A false assertion records `@test.assertionFailed` and stops only its own
  test. It is an outcome of the runner. It is not a `Result` and not a trap,
  and source cannot catch it.
- A type known to contain a function, a plan, a grant, or an interface
  value is rejected statically wherever a value must be observed. A value
  that holds one where the static type did not show it gives an explicit
  observation error, `@tool.unsupportedValue`, and never a trap.
- Defect traps and host events keep their own categories.

## Compiler intrinsic registry

Only the embedded standard library may bind a symbol of the closed
`@compiler` registry. Each row has a stable wire symbol, exact generic,
parameter, and result types, and pure deterministic semantics.

- An unknown symbol or provider, or a signature that does not match, MUST
  be rejected before execution.
- The registry is versioned. Changing the casing of a source name MUST NOT
  rename a wire string.
- A primitive implements what the core cannot express: scalar arithmetic,
  comparison, and conversion, and storage operations that depend on the
  representation.
- Every partial operation MUST return an `Option` or a `Result`. Plain
  integer division and remainder, which trapped on a zero divisor, do not
  exist in this language.

Three core types are used below:

| Type | Cases |
| --- | --- |
| `Ordering` | `less`, `equal`, `greater` |
| `ArithmeticError` | `overflow`, `divisionByZero`, `invalidShift` |
| `ConversionError` | `outOfRange`, `invalidFormat`, `unrepresentable` |

A case without fields is constructed by a call with no arguments, such as
`Ordering.less()`.

### Integer operations

`T` is any of `I8`, `I16`, `I32`, `I64`, `U8`, `U16`, `U32`, `U64`. Source
names are camelCase, and the registry maps each to a fixed wire symbol such
as `i32.add-checked`. A plain operation **saturates**; it never wraps. `neg`
and `negChecked` exist only for signed `T`.

| Member | Signature | Meaning |
| --- | --- | --- |
| `add`, `sub`, `mul` | `Fn({T T} T)` | the exact mathematical result, clamped to the bounds of `T` |
| `neg` | `Fn({T} T)` | exact negation; the minimum negates to the maximum |
| `shiftLeft` | `Fn({T U32} T)` | the value times 2^amount, clamped to the bounds of `T`, for every amount; zero stays zero |
| `shiftRight` | `Fn({T U32} T)` | floor division by 2^amount, for every amount; an amount of at least the width gives zero, or -1 for a negative value |
| `addChecked`, `subChecked`, `mulChecked` | `Fn({T T} Result(T ArithmeticError))` | the exact result, or `overflow` |
| `divChecked` | `Fn({T T} Result(T ArithmeticError))` | the quotient truncated toward zero; `divisionByZero` for a zero divisor; `overflow` for the signed minimum divided by -1 |
| `remChecked` | `Fn({T T} Result(T ArithmeticError))` | the remainder, with the sign of the dividend; `divisionByZero` for a zero divisor; the signed minimum with -1 gives zero |
| `negChecked` | `Fn({T} Result(T ArithmeticError))` | exact negation, or `overflow` for the minimum |
| `shiftLeftChecked` | `Fn({T U32} Result(T ArithmeticError))` | `invalidShift` if the amount is at least the width; otherwise the exact product, or `overflow` |
| `shiftRightChecked` | `Fn({T U32} Result(T ArithmeticError))` | `invalidShift` if the amount is at least the width; otherwise floor division |

The identity `left = quotient × right + remainder` holds for the
mathematical quotient, which for the signed minimum and -1 is outside `T`.

Comparison and conversion are reached only through the interfaces, never
through an extra static member:

| Implementation | Meaning |
| --- | --- |
| `Equatable.equal` on `T` | numeric equality |
| `Ordered.compare` on `T` | numeric order |
| `From(T)` on `Str` | the shortest decimal digits, with a leading minus only for a negative value, and no suffix |
| `TryFrom(Str)` on `T` | an optional minus (signed `T` only), then one or more ASCII decimal digits, and nothing else. Anything else is `invalidFormat`. A well-formed value outside `T` is `outOfRange` |
| `From(S)` or `TryFrom(S)` on `T`, for another integer type `S` | the same integer. `From` exists exactly when every value of `S` fits `T`; otherwise `TryFrom` returns `outOfRange` for a value that does not fit |

The destination of a conversion is its written expected type. The default
members of `Ordered` call `compare` once. `min` and `max` return the left
operand on a tie. `inRange` compares the lower bound first, and the upper
bound only if needed. The order laws of a user's implementation are that
implementation's obligation; the static guard checker does not assume them.

### Floating-point operations

For `F32` and `F64`:

- `add`, `sub`, `mul`, and `div` are IEEE 754, round to nearest, ties to
  even. `neg` is IEEE negation. Division produces infinities and NaNs as
  values; it never traps.
- `equal` is IEEE equality, so a NaN is unequal to itself. `compareTotal` is
  the IEEE `totalOrder` over canonicalized values. Both are static members.
  The float types implement neither `Equatable` nor `Ordered`, because IEEE
  equality is not reflexive.
- Parsing is `TryFrom(Str)` and formatting is `From` on `Str`. A parse
  accepts exactly the unsuffixed decimal float-literal grammar. Other input
  is `invalidFormat`. A value that rounds to infinity is `outOfRange`.

**NaN.** The sign and payload of a NaN MUST NOT be observable. Equality,
total comparison, formatting, value encoding, and assertion observation
first replace any NaN by the single positive quiet NaN with a zero payload
of its width. Arithmetic MAY produce any NaN, or canonicalize eagerly.
Negative zero is normalized only where an operation says so.

**Canonical text.** `From` to `Str` writes a float as `nan`, `inf`,
`-inf`, or the shortest decimal digits that round-trip.

- Zero, and a magnitude in [1e-4, 1e16), use decimal notation with at least
  one fractional digit: `100.0`, `-0.0`, `0.0025`.
- Any other magnitude uses scientific notation, with one integral digit and
  no plus sign in the exponent: `1e20`, `1.5e-7`.
- A [value encoding](#values) writes a finite float as this text with the
  width suffix, so signed zero is preserved as `-0.0f64`, and writes a
  non-finite float as a closed record, because `nan` and `inf` are not
  literals of the VIBON grammar.

### Character, sequence and dictionary operations

| Operation | Signature | Meaning |
| --- | --- | --- |
| `Char.toU32` | `Fn({Char} U32)` | the Unicode scalar value |
| `Char.fromU32` | `Fn({U32} Option(Char))` | none for a surrogate or a value above U+10FFFF |
| `Str.length` | `Fn({Str} U64)` | the count of Unicode scalars, not of bytes or graphemes |
| `Str.concat` | `Fn({Str Str} Str)` | concatenation |
| `Str.slice` | `Fn({Str U64 U64} Option(Str))` | a half-open range of scalars, start then end; none if the start exceeds the end or the end exceeds the length |
| `Str.toUtf8` | `Fn({Str} Bytes)` | the UTF-8 encoding |
| `Str.fromUtf8` | `Fn({Bytes} Result(Str ConversionError))` | `invalidFormat` for ill-formed UTF-8 |
| `Str(…)` and its pattern | `List(Char)` to and from `Str` | the characters in order |
| `Equatable` and `Ordered` on `Str` | | equal scalar sequences; lexicographic order by scalar |
| `Bytes.length` | `Fn({Bytes} U64)` | the byte count |
| `Bytes.concat` | `Fn({Bytes Bytes} Bytes)` | concatenation |
| `Bytes.slice` | `Fn({Bytes U64 U64} Option(Bytes))` | a half-open range of bytes, with the same bounds rule |
| `Bytes(…)` and its pattern | `List(U8)` to and from `Bytes` | the bytes in order |
| `Equatable` and `Ordered` on `Bytes` | | equal byte sequences; lexicographic order by byte |
| list literal | values to `List(T)` | the elements in written order; `T` comes from the expected type where inference cannot find it |
| `List.length` | `Fn({List(T)} U64)` | the element count |
| `List.append` | `Fn({List(T) T} List(T))` | a new list with one more element at the end |
| `List.concat` | `Fn({List(T) List(T)} List(T))` | the first list's elements, then the second's |
| `List.slice` | `Fn({List(T) U64 U64} Option(List(T)))` | a half-open range of elements, with the same bounds rule |
| dict literal | pairs to `Dict(K V)` | every pair evaluated; key order; a later duplicate wins; the empty literal is `[:]` |
| `From(List({K V}))` on `Dict(K V)` | | the same ordering and duplicate rule |
| `Dict.entries` | `Fn({Dict(K V)} List({K V}))` | the entries in key order |
| dict iteration primitive | | the pure primitive behind `Dict`'s nested `Iter` implementation; its remainder keeps the concrete type |

There is no `List.of`, `Dict.of`, or `Dict.fromEntries`.

- **One walk.** `Str` and `Bytes` delegate their lexicographic comparison to
  one generic helper, `fn compareLexicographic(T I)` with
  `takes {left: I right: I}`, `returns order: Ordering`, and
  `given {T: Ordered I: Iter(Item: T)}`, which walks both iterators with
  `Iter.next` and compares items with `Ordered.compare`. Their `Equatable`
  implementations stay explicit and small.
- **`Bool`.** `Bool.not`, `and`, `or`, and `xor` are ordinary members and
  evaluate both operands. Short-circuiting is written with `if`.
- **`Char`.** The ASCII classifiers are ordinary library definitions. The
  shared range test is `Ordered.inRange`; there is no `Char.inRange`.
- **`Str.trim`** trims ASCII whitespace. It is not a Unicode or grapheme
  algorithm.

**Iteration.** `Iter.next` is pure. It returns an item together with an
immutable remainder of the same concrete type.

- `List`, `Dict`, `Str`, `Option`, and `Bytes` declare `Iter` in nested
  `impl` blocks. `Result` MUST NOT implement `Iter`, which would silently
  drop errors.
- `map`, `filter`, `skip`, and `take` build their adapter values lazily.
  Building one MUST NOT pull from the source. `collect` drains in order.
- `fold` is strict and goes **left**: `step` receives the accumulator, then
  the item, once per item. An empty input returns `initial`. A toolchain
  MUST NOT reassociate or parallelize a fold because its callback is pure.
- A pure callback may build a plan as data. It cannot run it.
- `Option` and `Result` gain no container-preserving `map` member.
- A filter over an endless input need not produce an item. This chapter
  promises nothing about the termination of an arbitrary iterator.

## Native implementations

Only the embedded standard library may attach a closed native symbol to an
ordinary checked Vibra body.

- **The body is the meaning.** The native implementation MUST be
  observationally equivalent to the body for every input, including typed
  partial results, the absence of host events, and error behavior. A
  toolchain MAY always run the body instead.
- **A missing implementation is a defect.** If a required trusted native
  implementation is absent, that is an operational provenance defect. It is
  never a reason to fall back to host authority.
- **One implementation.** Toolchain-owned looping primitives and native
  implementations are written once, against the shared value-storage
  interface. The interpreter, the stepper, and the native executor either
  share that code or are held to the same independent vectors and to
  comparison with the body.
- **Callbacks keep their contract.** Code that calls back into Vibra MUST
  preserve the callable's checked entry and exit behavior, its captures,
  and its type arguments.
- **No user natives.** A user package cannot add a native symbol, a foreign
  declaration, or an ambient read. A native implementation MUST NOT perform
  a host operation on behalf of a `fn`.

## Value arena

A context owns a stack of activation cells, its module state, and one byte
arena.

- The first native store uses 32-bit offsets and power-of-two size classes
  from 32 bytes to 1 GiB. Offset zero is reserved as invalid.
- Internal offsets MUST NOT appear in a source identity, a value encoding,
  an audit projection, or a conformance snapshot.
- Arena and frame layout are private to the toolchain. Generated code calls
  reviewed runtime routines; it holds no arbitrary pointer into storage.

The value kinds are closed: scalar, `Atom`, `Str`, `Bytes`, tuple, `List`,
`Dict`, record, variant, wrapper, `Fn`, `Proc`, `Intent`, and sealed
authority.

- `Bool` has the meaning of its variant declaration. Unboxing it is a
  choice of representation, not a second meaning.
- `Never` has no value, no storage, and no encoding.
- Source cannot build an unsealed aggregate or mutate a sealed value.
- A function or procedure value keeps its code identity, captures, checked
  defaults, and type descriptors. A plan also keeps its operands and its
  own execution kind. The three checked types MUST NOT be erased into one
  pure callable that could run latent effects.
- A sealed authority envelope has a target kind, which is `HostResource` or
  `ActorEndpoint`; the identity of its provider and world; its scope and
  rights; its accounting lineage; and its lifetime and generation. Neither
  a source record nor decoded data can make one.

## Reclamation

Storage is reclaimed by precise, non-atomic reference counting over
immutable, acyclic values, with an iterative worklist for release.

- One context is touched by one worker at a time.
- A value is live while it is reachable from an activation, a module value,
  a live plan or callable that captured it, or a reviewed hold at the host
  boundary.
- The runtime MUST reclaim unreachable storage before a computation whose
  live data is bounded grows without bound. A tail loop that drops each
  earlier aggregate MUST keep live storage bounded, whatever the iteration
  count.
- Releasing an arbitrarily deep value MUST NOT recurse on the host stack.
- The runtime MUST expose live and peak storage to the conformance harness,
  including retained frames, values, and any boundary handle table. These
  measurements are not source values.

**Why values stay acyclic.** A value is built from older values. A cycle
among module values is rejected. A capture cannot refer to the `let` being
initialized. Named recursion refers to code by identity and does not own a
counted closure of itself. Every new value kind MUST keep this argument
true. An in-place update is permitted only when no live reference could
observe the change or a back edge.

**Authority.** The broker and kernel own the live resource and accounting
objects that sealed tokens refer to, without a strong ownership cycle. A
copy or child keeps its ancestors' accounting and keeps the backing
authority alive. Releasing a parent MUST NOT invalidate a legitimate live
child, and a stale token MUST NOT come to name a new resource when a slot
is reused.

**Not promised:** a general cycle collector, a close protocol, linear
destruction, revocation, and constant-time reclamation.

**Between contexts.** A message is copied as neutral data; arena offsets
are never shared. Passing sealed authority requires a delegation the broker
checks, and serializing a token transfers nothing. The first concurrency
scheduler uses a fixed pool, places a context before it is initialized,
pins workers, and neither migrates nor steals. These are initial
implementation defaults, not measured speedups.

## Representation latitude

A conforming implementation MAY unbox wrappers and fieldless cases, use a
compact layout for `Option`, omit slots of type `Never`, and update unshared
list or dict storage in place.

It MUST preserve nominal identities, invariants, argument order, observable
results, capture semantics, accounting, and the bounds on activation and
release above. It MUST NOT erase a guard to manufacture a tail call, and
MUST NOT claim a proof from an optimization. The name `List` promises no
linked list, no copy-on-write, and no constant-time operation.

## Canonical value encoding

A persistent observation is VIBON data. It is written with the data forms of
source and is never evaluated as source. Its field names, like every
schema's, are identifiers and never reserved words.

- A complete observation is a closed record with the fields
  `format: @value.v2`, `typeInfo: T`, and `value: v`, in that order.
- A type description alone is a closed record with `format: @type.v2` and
  `typeInfo: T`.
- The format version is part of every stored observation. An incompatible
  shape MUST NOT reuse a version.

```vibon
{format: @value.v2 typeInfo: @u8 value: 200u8}
```

### Types

`T` is the canonical type encoding. The compiler-owned scalar types are
written as the bare fixed wire atoms `@i8` through `@u64`, `@f32`, `@f64`,
`@char`, `@atom`, `@void`, and `@never`. These are protocol tags, not source
spellings. `@never` appears only inside a type, since `Never` has no value.

Every other type is a closed record whose first field `kind` names its
form. A declared type is described by its canonical path, its identity, and
its type arguments; its body is not expanded:

```vibon
{
  kind: @declared
  path: @example.count.Positive
  seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
  arguments: []
}
```

`seed` is the identity string that
[declared types](02-type-system.md#declared-types) defines. The pair
`{path: P seed: S}` alone is an **entity reference record**, which rows,
the index, and build records use to name a declaration.

Every type record has exactly these fields, in this order. `P` is a
canonical atom path, `S` an identity string, `T` a nested type encoding,
and `R` a row.

| Type | Fields |
| --- | --- |
| declared type | `kind: @declared`; `path: P`; `seed: S`; `arguments`: list of `T` in parameter order, written even when empty |
| `List` | `kind: @list`; `arguments`: list holding the element `T` |
| structural tuple | `kind: @tuple`; `arguments`: list of component `T` |
| structural record | `kind: @record`; `fields`: record of `name: T` in canonical name order |
| atom singleton | `kind: @atomSingleton`; `atom`: the atom |
| interface value | `kind: @interface`; `path: P`; `seed: S`; `arguments`: list of positional `T`, written even when empty; `choices`: dict from `Str` associated type name to `T`, one entry per associated type, written even when empty |
| `Fn` | `kind: @fn`; `parameters`: list of positional `T`; `labels`: dict from `Str` label to `T`; `result: T` |
| `Proc` | `kind: @proc`; `parameters`; `labels`; `result: T`; `effects: R` |
| `Intent` | `kind: @intent`; `result: T`; `effects: R` |

- `List` has the wire tag `@list`. `Bool`, `Str`, `Bytes`, `Dict`,
  `Option`, and `Result` are ordinary declared library types and are encoded
  with their declared identities.
- An interface value type names the interface and fixes every associated
  type, exactly as source must. `Iter(Item: U8)` and `Iter(Item: Str)` are
  two descriptors. An associated type never appears unfixed, because a
  checked signature names one only through a label or a type parameter
  bound to it, and a type parameter does not survive checking.
- A row `R` is a closed record with one field, `roots`: a list of entity
  reference records of operation interfaces, sorted by the UTF-8 bytes of
  `path`. It is written even when empty. `Fn` has no row.
- `labels` is a dict because it may be empty, and `{}` is a tuple.
- Default values belong to callable values, not to this description. No
  variadic or union descriptor exists.

```vibon
{
  format: @type.v2
  typeInfo: {
    kind: @fn
    parameters: [
      {
        kind: @interface
        path: @std.iter.Iter
        seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
        arguments: []
        choices: ["Item": @u8]
      }
    ]
    labels: [:]
    result: {kind: @list arguments: [@u8]}
  }
}
```

### Values

A scalar is written as its canonical literal, with its exact width suffix
where it has one. A `Str` value is written as its canonical string literal,
and a `Bytes` value as the bytes record below, although both source types
are library declarations. A non-finite float is written as the `@nonFinite`
record. Every other compound value is one of these closed records:

| Value | Fields |
| --- | --- |
| non-finite float | `kind: @nonFinite`; `value`: `@nan`, `@positiveInfinity`, or `@negativeInfinity` |
| bytes | `kind: @bytes`; `values`: list of `U8` literals |
| tuple | `kind: @tuple`; `typeInfo: T`, present only for a declared tuple; `values`: list of encoded components |
| list | `kind: @list`; `values`: list of encoded elements in order |
| dict | `kind: @dict`; `entries`: list of encoded key and value tuples in `Ordered` key order |
| record | `kind: @record`; `typeInfo: T`, present only for a declared record; `fields`: record of encoded fields |
| variant | `kind: @enum`; `typeInfo: T`; `variant`: the case name as an atom; `payload`: the encoded fields, only for a case that has fields |
| wrapper | `kind: @wrapper`; `typeInfo: T`; `value`: the encoded representation |

- A `@nonFinite` record is valid only where the enclosing type is `@f32` or
  `@f64`. There is one canonical NaN, written `@nan`, whatever the sign or
  payload of the value observed. A finite float is its literal with its
  suffix, and `0.0f32` and `-0.0f32` are two different values.
- A declared type's `typeInfo` field is its complete description, with its
  identity and arguments. An anonymous tuple or record has no `typeInfo`
  field, because the observation's own type already gives its shape.
- Declared fields and cases keep declaration order. Anonymous record fields
  use canonical name order.
- A case name is the resolved source name.
- A decoder MUST check the expected type, every width and scalar invariant,
  the completeness of fields and cases, the nominal identity, and any
  `where`, before it exposes a typed value.

```vibon
{format: @value.v2 typeInfo: @f64 value: {kind: @nonFinite value: @negativeInfinity}}
```

```vibon
{
  format: @value.v2
  typeInfo: {kind: @record fields: {ratio: @f32 scale: @f64}}
  value: {kind: @record fields: {ratio: -0.0f32 scale: 1e20f64}}
}
```

### What has no encoding

A function, a procedure, a plan, and a sealed authority have no value
encoding, and nothing decodes into one.

- A type statically known to contain one is rejected wherever an encoding
  is required.
- An interface value has no value encoding either: the type that chose its
  implementation is hidden, and an observation never recovers it. A value
  that holds one gives an explicit encoding error. It never gives a trap, a
  fake null, or a truncated value.
- A diagnostic tool MAY emit a separately versioned summary that cannot be
  decoded. It MUST be labelled as a summary, and a value decoder MUST NOT
  accept it.
- The grant projections of the [audit trace](#audit-trace) are data
  summaries. They are never the encoding of a usable token.

## External providers

Source has exactly two classes of trusted external provider, and only
declarations of the embedded standard library may bind them.

| Class | Binds |
| --- | --- |
| `@compiler` | a pure, deterministic primitive |
| `@host` | a registered effectful operation, in the interface that owns it |

- Each registry entry names a stable wire symbol, an exact checked
  signature, and a version. A host row also names its owning operation
  interface and its audit contract.
- An unknown symbol or provider, a wrong owner, or a wrong signature MUST
  be rejected before execution.
- A user package MUST NOT add a foreign function interface, WASI, a Wasm or
  native import, or a foreign driver through these declarations.
- A host operation takes values and returns a value, through its explicit
  sealed receiver. File contents, stream data, the environment, clocks, and
  randomness are ordinary typed results.
- Source gets scoped capabilities. It does not get raw operating-system
  handles, user-written close or destructor protocols, or general mutable
  references.
- A source implementation may model an interface using grants it holds.
  That does not create a host grant or select an unreviewed adapter.

## Host registry

Registry major version 2 has 17 host rows, each with a capability receiver,
one nominal owner per operation, and typed delivery outcomes. A binary built
against another major is not reinterpreted.

A row is a symbol, an owner, an exact signature, a `since` minor version,
and a delivery class. Every row below has `since` 0 and delivery `@once`.
The table gives the result of an invocation: applying the source `proc`
first builds a plan carrying the owner's row.

Each operation's first parameter is the grant, of the owning interface's
type. The grant MUST legitimately implement that interface; a record with
similar fields is not a grant. The
[effects chapter](03-effects.md#standard-operation-inventory) holds the
canonical mapping from source names to these rows.

| Wire symbol | Operation | Owner module | Further parameters | Result of an invocation |
| --- | --- | --- | --- | --- |
| `io.stdinText` | `StdinText.read` | `@std.io` | none | `Result(Str IoError)` |
| `io.stdinBytes` | `StdinBytes.read` | `@std.io` | none | `Result(Bytes IoError)` |
| `io.stdoutText` | `StdoutText.write` | `@std.io` | `value: Str` | `Result(Void IoError)` |
| `io.stdoutBytes` | `StdoutBytes.write` | `@std.io` | `value: Bytes` | `Result(Void IoError)` |
| `io.stderrText` | `StderrText.write` | `@std.io` | `value: Str` | `Result(Void IoError)` |
| `io.stderrBytes` | `StderrBytes.write` | `@std.io` | `value: Bytes` | `Result(Void IoError)` |
| `fs.readFile` | `DirReadBytes.read` | `@std.fs` | `at: RelativePath` | `Result(Bytes FsError)` |
| `fs.readText` | `DirReadText.read` | `@std.fs` | `at: RelativePath` | `Result(Str FsError)` |
| `fs.readEntries` | `DirList.entries` | `@std.fs` | `at: RelativePath` | `Result(List(Str) FsError)` |
| `fs.writeFile` | `DirWriteBytes.write` | `@std.fs` | `at: RelativePath`, `contents: Bytes` | `Result(Void FsError)` |
| `fs.writeText` | `DirWriteText.write` | `@std.fs` | `at: RelativePath`, `contents: Str` | `Result(Void FsError)` |
| `fs.metadataStat` | `DirStat.stat` | `@std.fs` | `at: RelativePath` | `Result(FsMetadata FsError)` |
| `env.readVar` | `EnvReadVar.read` | `@std.env` | `name: Str` | `Result(Str EnvError)` |
| `env.readArgs` | `EnvReadArgs.read` | `@std.env` | none | `Result(List(Str) EnvError)` |
| `time.nowWall` | `WallClock.read` | `@std.time` | none | `Result(I64 TimeError)` |
| `time.nowMonotonic` | `MonotonicClock.read` | `@std.time` | none | `Result(U64 TimeError)` |
| `random.generateBytes` | `RandomBytes.generate` | `@std.random` | `count: U64` | `Result(Bytes RandomError)` |

### Error types

Every type here is a nominal library declaration with fieldless cases.

| Type | Its own cases |
| --- | --- |
| `IoError` | `invalidText`, `closed`, `other` |
| `FsError` | `notFound`, `permissionDenied`, `notAFile`, `notADirectory`, `invalidPath`, `invalidText`, `other` |
| `EnvError` | `notPresent`, `notUnicode` |
| `TimeError` | `unavailable` |
| `RandomError` | `unavailable` |

Each of the five also has these four cases:

| Case | Meaning |
| --- | --- |
| `denied` | the grant's policy rejected the request; nothing was dispatched |
| `limitExceeded` | the broker could not reserve the limits; nothing was dispatched |
| `unsupported` | this adapter or action cannot be attempted under this profile; nothing was attempted |
| `outcomeUnknown` | the action may have run and its completion is unknown. It MUST NOT be reported as rejected before submission |

The older cases keep their meanings. An operating-system permission failure
stays `permissionDenied`, which is distinct from the policy case `denied`.
An ordinary domain failure is never a trap and never memory exhaustion.

`FsKind` has the cases `file`, `directory`, and `other`. `FsMetadata` is a
record of `kind: FsKind` and `size: U64`. A filesystem operation takes a
checked `RelativePath` under its grant; an absolute string never grants
access by itself.

### Closed operation semantics

**All operations.**

- A whole-value text read decodes UTF-8, and invalid bytes are
  `invalidText`. A text write encodes UTF-8, so its `Str` argument cannot be
  invalid text. Byte operations never inspect contents.
- A limit MUST be reserved before an operation could exceed its count or
  byte bound. A driver MUST NOT read, allocate, or send an unbounded
  payload first and check afterwards.
- A driver MUST NOT turn an ordinary failure into a process crash.

**Standard input.**

- A read consumes from the current position to the end.
- After a completed success or domain failure the position is at the end,
  and a second completed read returns the empty value.
- A rejection before dispatch (`denied`, `limitExceeded`, `unsupported`)
  consumes nothing.
- Input that cannot be read is `other`. Text that cannot be decoded is
  `invalidText`.

**Standard output and error.**

- A write delivers its whole value, in invocation order.
- A closed stream is `closed`; another failure is `other`.
- A failed write may have written a prefix. The outcome MUST still say
  accurately whether the write completed or was submitted.
- These reads and writes consume a stream and are not repeatable.

**Filesystem, common rules.**

- The driver MUST enforce the held root, the relative scope, the action,
  and the leaf-name selector before the data action, and at the actual
  operating-system use. It MUST NOT check a prefix and then reopen an
  absolute path.
- It MUST refuse to traverse a symbolic link or reparse point, and to cross
  a forbidden mount. A refusal MUST NOT fabricate the result of a followed
  target.
- A valid request outside the grant's scope is `denied`, with no external
  action.
- [Scope policies](03-effects.md#scope-policies) specifies `RelativePath`
  parsing, file-name tests, rooted traversal, and the limits on hard links.
- A grant narrowed by `sub` keeps the same acquired anchor and adds a path
  restriction. The driver MUST NOT cache a resolved child anchor between
  invocations. Each operation walks again from the anchor, releases its
  intermediate handles when the invocation ends, and works on the target it
  opened.
- Replacing the root at its old absolute path never moves the grant to the
  new object.

| `FsError` case | Meaning |
| --- | --- |
| `notFound` | the selected entry, or a parent it needs, does not exist |
| `permissionDenied` | the operating system refused |
| `notAFile` | a directory or other non-regular target where a file is required |
| `notADirectory` | a non-directory where a directory is required |
| `invalidPath` | a path the native adapter cannot interpret |
| `invalidText` | text or a name that is not Unicode where Unicode is required |
| `other` | any other ordinary host failure |

**`DirList.entries`** returns the names of the entries, without `.` and
`..` and without the directory's own path, sorted by Unicode scalar.

- It needs a `DirList` grant and that grant's directory selector.
  Permission to read files does not grant listing.
- A filtered listing applies its per-entry visibility rule before it
  exposes any name.
- A name that is not Unicode is `invalidText`.

**`DirStat.stat`** needs its own grant and selector. It returns the byte
length of a regular file, and zero for any other kind. It does not follow a
link, and so gains no wider traversal authority.

**File writes** create or replace the whole contents.

- They do not append and do not create parent directories.
- The policy must authorize the concrete action, create or replace.
- The driver MUST resolve the target without destroying anything before it
  checks kind and name. Authorization to create a new file requires
  exclusive creation; it MUST NOT fall back to truncating an existing file
  that collides by case or alias.
- A write is not transactional. An ordinary failure may leave a prefix
  written, and a `Result` implies no rollback.

**Environment.**

- `EnvReadVar.read` reads one permitted name. A missing name is
  `notPresent`. So is a name no environment can hold: an empty one, or one
  containing `=` or NUL. A value that is not Unicode is `notUnicode`.
- `EnvReadArgs.read` returns the program's arguments in their original
  order, without the executable's path, under the
  [program arguments](05-tooling.md#program-arguments-execution-and-evaluation)
  contract. An argument that is not Unicode is `notUnicode`.
- Neither grant exposes anything outside its reviewed scope.

**Clocks and randomness.**

- `WallClock.read` returns `I64` nanoseconds since the Unix epoch in UTC,
  ignoring leap seconds.
- `MonotonicClock.read` returns `U64` nanoseconds from an origin fixed for
  the instance, and never decreases within it.
- A clock that cannot be read, or whose value does not fit its type, is
  `unavailable`.
- `RandomBytes.generate` returns exactly `count` bytes from its bound
  generator, or `unavailable` when it has none.
- All three are operations. None is a pure function, and none can run
  during compile-time evaluation.

Memory exhaustion stays a host event and never becomes an error case.
Append, remove, and directory creation need new registry rows with their
own contracts. No symbol escapes the closed table.

## Host calls

Every host operation passes one shared dispatch boundary, `Context::host`,
in both the interpreter and native execution. Before any external action
the runtime MUST do the following, in order:

1. **Validate the grant:** its target kind, its world and provider, its
   lifetime and generation, and the identity of the registered operation.
2. **Check rights**, and derive the requested resource and action from the
   typed operands. An approval flag supplied by the author is never used.
3. **Enforce the whole policy**, including every constraint inherited from
   a parent, and reserve all shared count and byte limits atomically.
4. **Call only the typed adapter bound to the target**, and keep three
   outcomes distinct: known not submitted, completed, and may have run.
5. **Advance the continuation** consistently with the completion, and then
   append the completed event, if one is due.

**Suspension.** Suspending and resuming around a submitted host instruction
MUST NOT submit it again. The first, sequential protocol treats a
synchronous host action as atomic with respect to cooperative yielding.
Asynchronous completion, later, needs an explicit stored operation token
and a wait transfer; an ordinary call does not define it.

**Accounting.**

- A copy of a sealed authority shares its authority and its inherited
  accounting identity.
- Narrowing MUST NOT add a root, an operation, a resource extent, a driver
  or world, or a fresh budget.
- A child or sibling grant is bound by every ancestor's reservation limit,
  and its own tighter limits apply in addition.
- A reservation is released or refunded only when the capacity is known to
  be unused. A consumption that might have happened MUST NOT be refunded.
- A counter that would overflow MUST reject and MUST NOT wrap.
- A grant outlives the variable that introduced it for as long as a
  legitimate child or closure holds it.

**Two kinds of target.** A `HostResource` is checked at host dispatch. An
`ActorEndpoint` is checked at kernel dispatch, against its typed protocol
and its payload and resource policy. The shared envelope MUST NOT make the
two interchangeable, send an actor message through a filesystem adapter, or
give a sender the receiver's host rights.

## Registry versioning

The host registry is major 2 and starts at 2.0.

- A registry snapshot lists every symbol with its owner, exact signature,
  `since` minor, and delivery class.
- Adding a row increments the minor and records it as that row's `since`.
- Changing an existing row's signature, owner, error shape, delivery
  semantics, or audit meaning requires a new major, and admission metadata
  that is incompatible with the old one. An implementation MUST NOT
  silently reinterpret an old import or descriptor.
- A build records the exact symbols it requires and the derived registry
  minor: the greatest `since` among them, or zero for none.
- A runtime supports major M, minor N only if it implements every required
  row with the same meaning. A higher required minor, or a different major,
  MUST fail admission before any startup or program action.
- The runtime ABI, the registry, the observation, world, and audit formats,
  the source language, and the toolchain release are versioned
  independently.
- A checked-in registry snapshot and focused host tests MUST reject an
  unintended change to an existing row.

## Host world

`vibra run` selects the system provider and the binding recipes of the
target. It does not grant an operation merely because its interface is in
the target's `effects` ceiling.

Tests and conformance execution use isolated, deterministic provider
worlds, with fake sealed grants.

- A test receives the root grant of its own world as a `TestWorld` and
  derives every other fake grant from it.
- A model target MUST contain no real driver or grant, directly or
  transitively.
- Every executor and every test gets a fresh copy of the world. Effects
  change that copy only.
- Pure evaluation has no provider at all.
- A world document describes inputs. It is not authority: decoding one MUST
  NOT restore a live grant, and source cannot select the real provider or
  forge a grant by changing it.
- Each run pins the admitted operation interfaces, the fixture selection,
  and the versions of source, driver, policy, and world.

The closed data format is `@hostWorld.v2`, with these fields in this
order. Every field but `format` is optional and takes its default when
absent, under the [schema rules](04-programs-and-packages.md#schemas). An
unknown field or an invalid value is rejected.

| Field | Data | Default |
| --- | --- | --- |
| `format` | `@hostWorld.v2` | required |
| `stdin` | a `Str` literal or a [bytes record](#values) | empty text |
| `directories` | list of canonical rooted virtual directory paths, each a `Str` | empty |
| `files` | dict from `Str` canonical rooted virtual path to `Str` or bytes record content | empty |
| `env` | dict from `Str` name to `Str` value | empty |
| `args` | list of `Str` | empty |
| `clock` | record `wall: I64`, `monotonic: U64`, `step: U64`, each optional | `0i64`, `0u64`, `1u64` |
| `random` | record `seed: U64` | `0u64` |

```vibon
{
  format: @hostWorld.v2
  stdin: "x\ny"
  directories: ["/notes"]
  files: ["/notes/a.txt": "hello" "/notes/raw.bin": {kind: @bytes values: [0u8 255u8]}]
  env: ["HOME": "/home/vibra"]
  args: ["/notes"]
  clock: {wall: 1700000000000000000i64 monotonic: 0u64 step: 1000u64}
  random: {seed: 42u64}
}
```

**Paths.** The default world has a root directory and nothing else.

- A virtual path is absolute, slash-separated, and canonical. It has no
  empty, `.`, or `..` component, no backslash or NUL, and no drive, device,
  or alternate-stream spelling. No canonical entry repeats.
- The root cannot be a file. A conflict between a file and a directory,
  including an ancestor used as both, is invalid data. Missing ancestor
  directories are implied.
- These paths name the fixture. A source `RelativePath` stays subject to
  its own stricter name profile and to its grant.
- This world has no links. Link and race cases need dedicated adapter
  tests; their absence here is not evidence of confinement.

**Behavior.**

- The fake filesystem has no operating-system permission failures. An
  absent file is `notFound`. A write changes only the copy. Policy and
  broker outcomes (`denied`, `limitExceeded`, `unsupported`) still occur.
- Fault injection needs its own declared, versioned profile. It MUST NOT be
  smuggled into `@hostWorld.v2`.
- Streams, the environment, and arguments follow the row semantics. Writes
  are recorded in the fake trace and never reach the real process.
- `WallClock.read` always returns `clock.wall`.
- The first `MonotonicClock.read` returns `clock.monotonic`. Each later one
  adds `step`, and returns `unavailable` if the `U64` would overflow.
- `RandomBytes.generate` uses SplitMix64 from `seed`. Each call consumes
  ceil(count / 8) outputs, encodes each little-endian, keeps `count` bytes,
  discards the unused tail, and continues the state on the next call. A
  count of zero consumes nothing.
- Every integer literal in a world carries its exact width suffix. A
  literal out of range or a malformed field fails data decoding, not
  program execution.

A conformance case may supply the path of such a document. The shipped
launcher has no user-selectable world or replay switch; the harness profile
is separate, and its observation schema and version are checked.

## Audit trace

The trace is the ordered list of completed host calls and of the startup
steps that came before them. Its persistent format is `@auditTrace.v2`:

| Field | Type | Meaning |
| --- | --- | --- |
| `format` | `@auditTrace.v2` | required |
| `revision` | `Str` | the workspace revision of the source that ran |
| `target` | optional atom | the binary target; absent for a test or an evaluation |
| `drivers` | list of `{provider: Atom version: Str}` sorted by `provider` | every driver bound in the run; `provider` is `@hostWorld.v2` for a fake world's driver |
| `policies` | dict from `Str` policy hash to the normalized `@scopePolicy.v1` document | every policy a grant projection refers to |
| `redaction` | atom | `@none`, or the redaction profile applied |
| `events` | list of event records | in completion order |

A **startup event** records one leaf of the
[startup order](04-programs-and-packages.md#startup-order), and a **host
event** one completed host call. Both are closed records whose first field
is `kind`:

| Field | Meaning |
| --- | --- |
| `kind` | `@startup` |
| `binding` | the leaf's atom |
| `provider` | the issuer's atom |
| `status` | `@acquired`, `@absent` (refused, bound to `Option.none()`), or `@failed` |
| `grant` | optional, present when acquired: the grant projection |
| `failure` | optional, present when failed: `{code: Atom message: Str}` |

| Field | Meaning |
| --- | --- |
| `kind` | `@host` |
| `invocation` | the invocation's ordinal within the run, `U64` |
| `parent` | optional: the parent invocation's ordinal, `U64`; absent for a root invocation and for a startup read |
| `phase` | `@startup` for a read a recipe performs, `@entry` otherwise |
| `op` | the exact registered wire symbol, `Str` |
| `driver` | `{provider: Atom version: Str}`, the bound driver |
| `origin` | the source-origin ordinal, `U64`; zero means no source origin |
| `grant` | the grant projection |
| `args` | list of the canonical argument values after the grant, in parameter order |
| `result` | the canonical data of the operation's complete `Result` value |

A **grant projection** is a closed record of `binding` (atom, the leaf the
grant descends from); `policy` (`Str`, the hash of its normalized policy and
a key of `policies`, or the empty string for a provider without a policy);
`rights` (a one-entry list of entity reference records, the operation
interface it permits); `limits` (optional, absent when no bound is set,
otherwise a record of the bounds that are set among `invocations`,
`inputBytes`, and `outputBytes`, each `U64`); and `root` (`U64`, a
run-local ordinal of the root grant whose accounting it shares).

```vibon
{
  format: @auditTrace.v2
  revision: "sha256:33d677dc829d6e953467ca5c9f3ffe396839cb1184fbeaba1e34703ee2fa4ff1"
  target: @hello
  drivers: [{provider: @host.io.stdout version: "0.0.2"}]
  policies: [:]
  redaction: @none
  events: [
    {
      kind: @startup
      binding: @out
      provider: @host.io.stdout
      status: @acquired
      grant: {
        binding: @out
        policy: ""
        rights: [{path: @std.io.StdoutText seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"}]
        root: 1u64
      }
    }
    {
      kind: @host
      invocation: 2u64
      parent: 1u64
      phase: @entry
      op: "io.stdoutText"
      driver: {provider: @host.io.stdout version: "0.0.2"}
      origin: 1u64
      grant: {
        binding: @out
        policy: ""
        rights: [{path: @std.io.StdoutText seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"}]
        root: 1u64
      }
      args: ["hello\n"]
      result: {
        kind: @enum
        typeInfo: {
          kind: @declared
          path: @std.result.Result
          seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
          arguments: [
            @void
            {
              kind: @declared
              path: @std.io.IoError
              seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
              arguments: []
            }
          ]
        }
        variant: @ok
        payload: {value: void}
      }
    }
  ]
}
```

**Projections.**

- A grant projection links to the approved policy and to the run-time
  resource identity. It never encodes a usable broker handle, a credential,
  or a callable, and decoding one grants nothing.
- Sensitive path or credential detail may be redacted only under the
  `redaction` profile named in the trace, and authorized tooling must keep
  enough stable references to inspect the run.

**When an event is appended.**

- One `@host` event is appended after each completed call, after its
  continuation has advanced.
- A typed denial returned by the adapter is a completed result and is
  recorded.
- A driver that returns `outcomeUnknown` has completed its adapter call.
  That typed result is recorded, without claiming that the external action
  completed.
- A refusal before the adapter is called MUST NOT fabricate a completed
  host event.
- A trap, a host event, or an interrupted completion appends no completed
  success to fill the trace.
- The absence of a completed event MUST NOT be reported as proof that
  nothing external happened.

**Other kinds.**

- Startup is recorded by the `@startup` events and the `@host` events of
  phase `@startup`, in acquisition order, so configuration reads and
  approved broker effects before the entry are not hidden.
- Future kinds, for attempts, refusals, or actors, need their own specified
  fields and ordering.
- A consumer MUST keep an unknown kind as an opaque record. Adding a field
  to an existing kind, or changing its meaning, requires a new major
  format.
- A JSON envelope carries the same facts, with VIBON values in canonical
  form. It MUST NOT fall back to untyped event strings.

## Entry outcome

The entry of a binary target is a `proc` whose parameters are grants and
whose result is `ExitCode`, a wrapper over `U8`. The
[programs chapter](04-programs-and-packages.md#targets) defines its
signature and its binding recipes.

**Startup.**

- Recipes are checked for type, version, and authority at build time. A
  missing or invalid recipe fails the build, for a required and an optional
  leaf alike.
- Grants are acquired in the
  [startup order](04-programs-and-packages.md#startup-order) of the
  programs chapter, which also lists each issuer's refusals.
- A required grant that is refused fails startup before the entry runs,
  with `@grant.requiredUnavailable`.
- An optional grant that is refused is bound to `Option.none()`. A
  malformed recipe or an incompatible provider is an error; it never becomes
  an absence.
- Grants already acquired are released in reverse acquisition order.
  Release MUST NOT be presented as rollback of an external effect already
  dispatched.

**The run.** After startup the launcher builds the entry's plan and runs it
under the same contract and authority rules as any procedure.

| Situation | Outcome |
| --- | --- |
| the entry returns `ExitCode` zero | success |
| the entry returns a non-zero `ExitCode` | application failure |
| a built executable | returns that `U8` to the operating system |
| `vibra run` | keeps its command statuses 0 to 6. A non-zero code maps to `@command.programFailed`, exit 6, with the actual code in `applicationCode` and no fabricated trap |

Memory exhaustion, startup failure, and tool failure keep their own
categories and MUST NOT appear as a returned application code. `run`,
`test`, and `eval` execute on the interpreter and need no linker.

## Traps

No trap can be triggered by user data in supported checked source. Partial
primitives, `where … else`, operation refusals, and values that cannot be
encoded give typed results or explicit tool and test errors. A static guard
that fails rejects compilation. This is an obligation on the conformance
gate. It is not a claim that a process cannot crash, or that every
implementation is sound.

There are exactly two **defect traps**. Both mean the toolchain or the
runtime is wrong.

| Trap | Meaning |
| --- | --- |
| `@runtime.invalidCheckedProgram` | impossible typed IR, descriptor, or continuation, or another toolchain failure. An unknown engine trap is reported as this one |
| `@runtime.invalidHostValue` | a boundary value that is invalid, stale, foreign, or of the wrong kind; an unsealed access; an illegal builder mutation; an invalid scalar; or a malformed trusted result |

- Source cannot catch a defect trap, and one MUST NOT be reported as an
  ordinary environmental error.
- A trap keeps its source origin when one is known. Otherwise the
  diagnostic has no source identity and primary span `0..0`.
- The observation holds the exact trap code and origin, and every executor
  MUST agree on them.

`@runtime.divisionByZero` and `@runtime.unobservableFunction` are
removed and MUST NOT be emitted for a checked program. Major-1 transition
artifacts keep their own versioned contracts and are never reinterpreted
under these semantics. Memory exhaustion and termination from outside are
host events, not traps.

## Native execution

**Execution IR.** Native compilation lowers checked IR to a separate
execution IR, and verifies it before any code is emitted.

- The verifier checks references; function, block, and resume ranges;
  arities and classes; and slot ownership on every path: no read of an
  empty slot, no write over a full slot without a release, and no consumed
  operand used twice.
- Resource bounds are checked before the verifier allocates its own state.
- A new IR operation arrives together with its verification rule, its
  accepted and rejected tests, its stepper semantics, and its code
  generation.
- A generator consumes an immutable, verified program. It MUST reject an
  unsupported instruction and MUST NOT emit partial code.

**Checkpoints.** The first execution protocol has five checkpoint
transfers: Call, TailCall, Return, Host, and Global.

- Branches and jumps in a straight line stay within one step.
- A step runs from its saved resume block to the next checkpoint and leaves
  all resumable state in the context.
- The dispatcher owns push, replace, and pop. No generated Vibra call
  consumes another native stack frame directly.
- The stepper and generated code share these transfers.

**Code generation and linking.**

- The first code generator is Cranelift, pinned together with its target
  settings.
- Generated objects link with a static runtime archive that contains the
  Nexus kernel, the shared host and native implementations, and the
  launcher.
- One reviewed bridge is the only place in the workspace's source that
  contains unsafe or foreign-ABI operations. This restriction is not a
  proof that dependencies, the linker, the operating system, or native
  drivers are safe.
- Generated code names slots and calls runtime routines. Private layout
  and broker internals are not an ABI for source.

**Admission.** Before startup or any program action, the runtime MUST check
the runtime ABI; the registry major, minor, and required symbols; the
function and block counts; the entry's shape; the identity of the
source-origin mapping; the target and CPU profile; and the driver and
binding policy requirements.

- An old descriptor's guessed error-variant flag MUST NOT decide how
  `ExitCode` behaves.
- The semantic identities of source, claim, witness, and policy, and the
  identity of the artifact, are distinct and versioned. An incompatible
  change is rejected; it is never run on a best-effort basis.

**Profiles and platforms.**

- The shipped launcher uses the system profile and the approved binding
  recipes.
- A separate test archive may provide a harness profile, with deterministic
  worlds and a versioned observation document. Native tests in it are
  parity instruments. They do not make `vibra test` run natively.
- An executable MUST be qualified on the platform it targets: Windows x64,
  Linux x86-64, and macOS arm64. Emitting code for another platform is not
  evidence that it runs there.

**Scheduling.** The executable corpus is run both with an unrestricted
scheduling quota and with a quota of one. Results, traps and their origins,
typed outcomes, and the order of completed events MUST agree.

- A long primitive charges its proportional work after it completes.
- A yield is not a source error, and is not a proof of fairness or of a
  time bound.
- Concurrency later adds explicit wait and kernel transfers. It MUST keep
  exclusive ownership of a context, typed delegation, and deterministic
  replay at observable kernel actions. It does not require the interpreter
  and native code to take equal numbers of reduction steps.

## Wasm transition

The major-1 WebAssembly boundary of v0.0.1 is a narrow transition
substrate. It is not another source language and not a new grant ABI, and
it stays only until the native cutover.

- A program that needs the new entry, a grant receiver, the new error
  shapes, or registry major 2 is incompatible with it, and MUST be reported
  as unsupported unless an executor profile supports it explicitly.
- The roadmap does not require a replacement Wasm ABI to be invented only
  so that it can be retired.
- An unsupported case stays an explicit gap. A compatibility parser, an
  unsafe import shim, and a false parity pass are all forbidden.

**The module.** A transition module validates with core WebAssembly 2.0 MVP
instructions, function tables, multiple results, bulk memory, sign
extension, and non-trapping float-to-integer conversion. It MUST NOT
require tail calls, GC, exceptions, SIMD, threads, reference types,
memory64, or multiple memories. It has one 32-bit linear memory, exported
as `vibra_v1_memory`, and no exported global. Only toolchain-owned native
code reads its layout.

**Imports** are closed: the pure native routines of `vibra_native_v1` and,
where needed, the registry-major-1 host symbols of `vibra_v1`. Source
chooses no import.

- Scalar parameters use `i32`, `i64`, `f32`, and `f64`. A `Char` is a
  validated `i32` scalar. Smaller integers are sign- or zero-extended into
  `i32`.
- Every other value is an instance-local `U64` value ID. No guest pointer
  or offset crosses the boundary.
- An ID is non-zero, issued by its instance, never reused within it, and
  held live in the instance's handle table until it is released.
- Native in-process execution needs no such table; its offsets stay local
  to the context.

**Exports** are closed and carry the prefix `vibra_v1_`:

| Export | Signature and meaning |
| --- | --- |
| `entry` | no parameters, no result: run the old-profile program entry |
| `test` | `i32` index, no result: run one old-profile test in a fresh instance |
| `status` | to `i32`: 0 none, 1 recorded trap, 2 memory host event, 3 assertion failure |
| `trap_code` | to `i32`, historical: 1 invalidCheckedProgram, 2 unobservableFunction, 3 invalidHostValue, 4 divisionByZero |
| `origin` | to `i32`: zero for no origin, otherwise a table ordinal |
| `failure` | to `i32`: 1 `assert.true`, 2 `assert.false`, 3 `assert.equal` |
| `failure_expected`, `failure_actual`, `result` | to `i64`: typed scalar bits or a held value ID, according to the checked type |
| `live_size` | to `i64`: live arena and handle-table bytes |
| `release` | `i64` held ID, no result: drop the host's hold |
| `variant` | `i64` ID to `i32`: the historical variant or member index |
| `length` | `i64` ID to `i64`: the count of scalars, bytes, elements, entries, or components for the value's kind |
| `read_i32`, `read_i64`, `read_f32`, `read_f64` | `i64` ID and `i64` index, to the named scalar class |
| `read_id` | `i64` ID and `i64` index, to a new held child ID, which the host must release |

A program module has `entry` and a test module has `test`, never both.
Returning is completion. An engine stop without the matching recorded
status is a compiler defect. A host import borrows every parameter ID and
returns one owned, sealed `Result` value ID, which the module adopts. The
host MUST NOT release the borrowed inputs or the adopted result.

**Builders.** A module that imports host symbols also exports `new`,
`write_i32`, `write_i64`, `write_f32`, `write_f64`, `write_id`, and `seal`,
with the same prefix.

- `new` takes an `i32` kind, an `i64` component count, and an `i32` variant
  or member, and returns a held, unsealed `i64` ID.
- The historical kind codes are: atom 0, string 1, bytes 2, tuple 3,
  array 4, record 6, enum 7, wrapper 8, union 9. Code 9 does not bring
  unions back into source. This interface builds no dict and no function.
- A write routine takes an ID, an index, and a value of its class, and
  returns nothing. `write_id` accepts only a sealed child and keeps its own
  reference. Rewriting a slot releases the old child and keeps the last
  value.
- `seal` takes one ID and makes the finished value immutable.
- Each of these records `invalidHostValue`: a slot outside the kind, a
  wrong scalar class, an invalid byte or `Char`, a stale or unissued ID, a
  write to a sealed value, an unsealed child, and an incomplete seal.
- The host never receives a raw pointer. Every accessor checks instance,
  kind, and liveness.

**Removal.** A transition artifact records its ABI and registry
requirements and its source-origin mapping. Later build metadata MUST NOT
silently reuse an incompatible custom section or version. At the native
cutover this substrate, the differential profile, the emitter and runner
dependencies, and the packaging references leave the active toolchain
together. The shipped v0.0.1 evidence remains as history. It is not
rewritten into a claim about this language.

## Determinism and observability

**Execution.** For identical checked code and equivalent deterministic
worlds, sequential execution MUST produce the same result or failure, the
same canonical value encoding, the same output bytes, and the same
completed audit events. Ambient resources are observed only through
registered operations and held grants. A fake world is a fresh, complete
set of declared inputs. A system run depends on the actual state of the
host it touches.

**Compilation.** With an identical compiler version, checked program,
semantic inputs, target, CPU, and link profiles, and normalized options,
compilation MUST produce byte-identical objects, artifacts, and canonical
build data, before signing.

- No clock, absolute checkout path, address, hash-table iteration order, or
  thread schedule may change the output.
- Artifact identity and semantic identity stay separate.
- Optimization is permitted only after the unoptimized semantic profile
  passes, and MUST preserve conformance observations and accepted static
  obligations.

**Compile-time evaluation** is optional. It runs the interpreter on checked
pure functions with constant inputs and observable results, under the
compiler's work and memory limits.

- A successful fold preserves exact numeric, contract, and error behavior.
- An optional fold that fails, is unsupported, or runs out of budget leaves
  the expression for run time.
- A required constant or a static obligation that cannot be established
  reports its defined diagnostic or incomplete result. It MUST NOT become a
  run-time guard.
- Cache keys include the code, type, registry, and profile identities, and
  never a process-specific authority token.
- Checking a type never needs to run source.

## Exclusions

This chapter provides none of the following:

- a general proof solver, or unrestricted dependent contracts;
- information-flow security, or static identity of a future resource
  instance;
- affine, linear, or close protocols; revocation; automatic rollback;
- exactly-once external delivery, or universal journaling and recovery;
- shared mutable references, or cycle collection;
- a hard time bound;
- a foreign function interface, native or Wasm;
- an operating-system sandbox for foreign binaries.

Capability confinement in source is a contract of trusted compilation
together with reviewed issuers, runtime, and adapters. It is not safe
execution of arbitrary machine code, and not a proof that a provider is
correct.

Nothing in the historical wire encodings or the transition description
restores an untagged union, union injection, a narrowing pattern, a tuple
index, an ascription expression, a variadic call, or unchecked integer
division and remainder. There is no implicit `await`, no permission prompt,
no ambient grant, no widening of a resource, no default error substituted
for a bare `where`, no trap for a failed contract, and no erasing of a
proof obligation by sampling.
