# Vibra type system

Status: normative target
Implementation status: none of this chapter is implemented in the surface it
is written in. The shipped v0.0.1 toolchain implements the previous
surface's nominal types, generics, interfaces, patterns, conversion, and
iteration; the rules marked "carried over" below restate that behavior.
Guards, type invariants, representation visibility, typed local bindings,
contextual `try`, and the removal of unions and ascription are new and
unimplemented. The [release roadmap](../roadmap/README.md) schedules them.

## Model

Vibra is statically typed, nominal, and value-oriented. Every expression has
one type before execution. Public declarations are checked from their written
signatures; a caller never needs a callee's body to type-check a call.

The predeclared types are:

```text
Bool Void Never Char Str Bytes Atom
I8 I16 I32 I64
U8 U16 U32 U64
F32 F64
```

There is no platform-sized integer and no unsized `Int`. There is no null
value, truthiness, implicit numeric widening, or implicit string conversion.

### Language core and standard library

Vibra is library-first. The compiler owns only what the language cannot build
for itself:

- the scalar types `Void`, `Atom`, `Char`, `I8` through `I64`, `U8` through
  `U64`, `F32`, `F64`, and the uninhabited type `Never`;
- the one generic immutable sequence `List(T)`;
- the `Fn`, `Proc`, and `Intent` types and the structural tuple and record
  types.

Every other type, including `Bool`, `Str`, `Bytes`, `Dict`, `Option`,
`Result`, `Ordering`, and the standard error types, is an ordinary `type` in
the embedded standard library. The compiler depends only on the **language
role** such a type plays. A role is a closed, compiler-known position:

| Role | Played by | What depends on it |
| --- | --- | --- |
| `@bool` | `Bool` | `if` conditions; guard predicates; `true` and `false` |
| `@str` | `Str` | string literals |
| `@bytes` | `Bytes` | byte lookups |
| `@option` | `Option` | lookups; `try` on an optional value; optional grants |
| `@result` | `Result` | `try`; unhandled-failure checking; fallible construction |
| `@dict` | `Dict` | dict literals |
| `@iter` | `Iter` | iteration contracts |
| `@equatable` | `Equatable` | structural equality of key types |
| `@ordered` | `Ordered` | dict keys |

The standard library declares each role exactly once with the `role`
section. A missing or repeated role is a toolchain defect. Adding a role is a
change to this table.

The standard library declares `Bool` as a variant type with the cases
`false` and `true`, `Str` as a wrapper over `List(Char)`, and `Bytes` as a
wrapper over `List(U8)`. Their constructors and patterns are the declared
ones: `Bool.true()`, `Str(scalars)`, `Bytes(items)`. The `Bool` module also
declares the two public module values `true` and `false`, which are the only
way source writes a boolean. `Dict(K V)` is a wrapper over `List({K V})`,
sorted by the `Ordered.compare` of its keys with one entry per key; its
representation is private, so it is built by a dict literal or by conversion
and read by lookup and `Dict.entries`. A toolchain may represent any of these
values directly, as the [runtime chapter](06-runtime.md) allows.

Every operation over a library type is standard-library Vibra, which a
toolchain may accelerate with a native implementation whose meaning stays its
Vibra body.

### The prelude

The compiler-owned types, the types and interfaces that play a role, `Any`,
`Self`, `Ordering`, and the values `true` and `false` are the only
declarations a program names without an import. This closed set is the
**prelude**. It is not extensible. A prelude name resolves to its real
declaration in every module, and a path that starts with one reaches its
members: `Str.concat(a b)`, `Ordered.compare(a b)`. No module outside the
embedded standard library can introduce a prelude name. Declaring a type or
interface with one, or binding one, is `@name.reservedDeclaration`. Using
one as the name of a module value, a function, or an import is
`@name.reservedValueSpelling`.

### Scalars

`Void` has exactly one value, `void`. `Char` contains exactly the Unicode
scalar values. `Atom` contains the atom values; every written atom also has a
singleton type, written as the atom itself in type position, which widens to
`Atom`.

For width `N`, `IN` contains the integers from `-2^(N-1)` through
`2^(N-1)-1`, and `UN` contains `0` through `2^N-1`. `F32` and `F64` are the
IEEE 754 binary32 and binary64 formats. A suffixed literal MUST fit its type.
An unsuffixed literal takes its type from its expected type and is
`@type.ambiguousInference` when none follows. A decimal float literal is
rounded to nearest, ties to even; a finite literal that would round to
infinity is out of range. There is no source spelling for infinity or NaN.

Plain integer arithmetic saturates. Division and remainder exist only in
their checked forms, which return a `Result`, as do the other checked
operations (`addChecked` and so on). The runtime chapter gives each
operation's exact meaning. No arithmetic operation traps.

## Declared types

Every `type` declaration introduces a new identity. Two declared types with
the same structure are different types. There are no transparent aliases and
no implicit conversion between a declared type and its representation.

A declaration's identity is the hash of its logical anchor: the package and
unit, the module path, the owner path, the declaration kind, the name, and
the identity scheme version. The anchor does not include the declaration's
body, so a type's identity does not depend on its fields, members, or guards.
Renaming or moving a declaration changes its identity.

| Body | Declares | Constructor | Pattern |
| --- | --- | --- | --- |
| `of {name: T …}` | declared record | `User(name: n id: i)` | `User(name: p)` |
| `of {T U}` | declared tuple | `Pair(a b)` | `Pair(p q)` |
| `case` list | variant type | `Shape.circle(radius: r)`, `Shape.empty()` | same spelling |
| `of` any other type | wrapper | `Celsius(21.5f64)` | `Celsius(p)` |

A record body has at least one field and its field names are distinct. A
variant type has at least one case and its case names are distinct; a case
has named fields or none. A repeated field or case name is
`@name.memberCollision`. A wrapper has exactly one representation type; it
does not delegate application, field reads, or interface conformance to it.

A declared type keeps its declaration order for fields and cases; that order
fixes discriminants, rendering, and comparison order.

The record fields, cases, and nested `fn` members of one type share one flat
member namespace, and so do the members of one interface. Two members of one
owner with the same name are `@name.memberCollision`. Implementations add no
name to it.

### Representation visibility

`representation @private` hides a type's constructor, its unwrapping and
destructuring patterns, and its field reads from every other module.
`representation @public` exposes them wherever the type is visible. When the
section is omitted, the representation has the declaration's own visibility.
A public type with a private representation is the way to export a type whose
values only its own module can make. Use of a hidden constructor, pattern, or
field is `@name.inaccessibleRepresentation`.

### Type invariants

A `where` section on a wrapper, declared tuple, or declared record states an
invariant. Its operand is a predicate: a pure function value of type
`Fn({R} Bool)`, where `R` is the representation type. A `where` on a variant
type is `@type.invalidInvariant`. The [Guards](#guards) section defines what
a bare `where` and a `where … else` mean. In both forms:

- constructor arguments are evaluated once in written order, the raw
  representation is assembled, and the predicate is applied to it before any
  value of the type exists;
- every other way a value of the type can come into being (decoding,
  conversion, a native implementation) MUST apply the same predicate or carry
  checked evidence that it holds;
- a value of the type that exists satisfies the predicate, so taking it apart
  by pattern never fails; and
- an invariant never proves a `match` exhaustive or an arm unreachable.

### Structural types

A tuple type `{T U}` and a record type `{name: T …}` written outside a `type`
body are anonymous, and their identity is their structure. Two tuple types
are the same when they have the same arity and equal component types in
order. Two record types are the same when they have the same set of field
names with equal types, in any written order; a record's canonical field
order sorts names by their UTF-8 bytes. `{}` is the empty tuple. Both forms
are accepted in every type position.

An anonymous type has no owner: it declares no members, receives no `impl`,
and conforms to no interface other than `Any` and the structural key rule
below. It cannot refer to itself.

### Recursive types

A declared type may refer to itself and to other declared types of its
module, by name. A reference to a type is symbolic and is not a dependency
that must be complete first. Every declared type MUST have a finite size:
recursion through `List`, `Dict`, or a callable type is permitted, and direct
infinite expansion is `@type.infiniteSize` at the declaration whose
expansion first repeats. Values are immutable and are built from values that
already exist, so no value contains itself.

### Callable types

`Fn({A B} R)` is the type of a pure function with required positional types
`A` and `B` and result `R`. Labelled parameters follow the positional ones in
the input vector: `Fn({I32 verbose: Bool} Bool)`. Defaults belong to the
function value and are not part of its type.

`Proc({A} R {Row})` is the type of a procedure, and `Intent(R {Row})` the
type of a plan; the [effects chapter](03-effects.md) defines both. A row is a
closed set of operation interfaces. An omitted row is empty. Two callable
types are equal only when their input vectors, results, and rows are equal;
there is no row subtyping. Callable types are never generic.

## Dict keys, equality, and order

The standard library declares two comparison interfaces in the prelude:

```vibra
interface Equatable
  role @equatable
  visibility @public
  fn equal
    takes {left: Self right: Self}
    returns same: Bool
  endfn
endinterface

interface Ordered
  role @ordered
  visibility @public
  fn compare
    takes {left: Self right: Self}
    returns order: Ordering
  endfn
endinterface
```

`Ordered` also declares the default members `less`, `lessOrEqual`,
`greater`, `greaterOrEqual` (each `{left: Self right: Self}` to `Bool`),
`min`, `max` (to `Self`), and `inRange` (`{value: Self low: Self high: Self}`
to `Bool`, inclusive at both ends). Each calls `compare` once per pair it
needs; `min` and `max` return the left operand on a tie.

An `Ordered` implementation MUST be a total order, and an `Equatable`
implementation of the same type MUST agree with it:

- **`Equatable`.** `equal(a a)` is `true`; `equal(a b)` is `equal(b a)`; and
  `equal(a b)` with `equal(b c)` gives `equal(a c)`.
- **`Ordered`.** `compare(a a)` is `equal`; `compare(a b)` is `less` exactly
  when `compare(b a)` is `greater`; and `compare(a b)` with `compare(b c)`
  both `less`, or both `equal`, gives `compare(a c)` the same answer.
- **Agreement.** `equal(a b)` is `true` exactly when `compare(a b)` is
  `equal`.

The toolchain cannot check these laws. A program whose implementation breaks
them still evaluates deterministically, but which entry a dict lookup finds
is then not defined. There is no hashed collection and no hashing interface.

Dict keys MUST implement `Ordered`. The standard library writes the
implementations of `Bool`, `Char`, `Str`, `Bytes`, and the integer types
where each type is declared. `Atom`, every atom singleton type, and an
anonymous tuple or record whose every component is an admissible key conform
structurally, with no declaration. `Void`, `Never`, `F32`, `F64`, callable
types, `List`, `Dict`, and `Option` are not admissible keys. A declared type
is admissible only through its own `Ordered` implementation. An inadmissible
key type is `@type.invalidDictKey`; a callable key is
`@type.functionNotEquatable`.

Canonical key order is the only order in which a dict is traversed, rendered,
or iterated: `false` before `true`; numeric order for integers; scalar value
for `Char`; lexicographic by scalar for `Str` and by byte for `Bytes`;
lexicographic by the UTF-8 bytes of the spelling for atoms; component-wise
for tuples and, in canonical field order, for records; and a declared type's
own `Ordered` implementation.

## Application

An application is classified by its statically resolved callee:

| Callee | Arguments | Result | Kind |
| --- | --- | --- | --- |
| a `Fn` value | its signature | its result | `@function` |
| a `Proc` value | its signature | an `Intent` | `@procPlan` |
| a declared type or case | per the table under Declared types | the type, or a `Result` under `where … else` | `@constructor` |
| `List(T)` | one `U64` index | `Option(T)` | `@collectionLookup` |
| `Dict(K V)` | one `K` | `Option(V)` | `@collectionLookup` |
| `Str` | one `U64` scalar index | `Option(Char)` | `@collectionLookup` |
| `Bytes` | one `U64` byte index | `Option(U8)` | `@collectionLookup` |

A lookup takes exactly one positional argument. Absence and an out-of-range
index yield `Option.none()`; they never trap. String indices count Unicode
scalars. Construction and lookup are pure and add no call edge.

No other value is applicable: not a tuple, record, variant value, wrapper,
atom, number, or a value of an unconstrained generic type. There is no
callable interface.

An argument that does not fit its slot, a wrong arity, and an unknown,
duplicate, or missing label are `@type.argumentMismatch`. Any other
disagreement between an expression's type and the type its position requires
is `@type.mismatch`.

## Namespaces and resolution

A module has type, value, and interface namespaces. Operation interfaces live
in the interface namespace; there is no separate effect namespace. One
top-level declaration may not reuse a name another top-level declaration or
import of the module already uses, so a name denotes one entity in every
position. A type position resolves across the type and interface namespaces
together, and a name resolving there to an interface denotes an interface
value type.

Shadowing is forbidden. A repeated top-level name, import name, or lexical
binding is `@name.redeclaration` at the later introduction, relating the
earlier one; the later binder still binds so the error does not cascade.
Sibling scopes may reuse a name when neither sees the other.

Every code entity has exactly one canonical atom path, `@unit.c1…cn`, built
from the ownership tree: the unit, the module path, then one component per
declaration and member. Every atom path resolves to at most one entity. A
position that expects an entity reference decides only which entity kind is
required; a path that resolves to another kind is `@name.wrongEntityKind`
and names what it found. Tooling MUST return the resolved kind and identity,
never a dotted string.

An `impl` block has no name. Its identity is the pair of its applied
interface target and its receiver type, and a member's identity adds the
contract member. Nothing addresses an implementation at a use site; dispatch
selects it.

## Functions as values

A `fn` or `proc` path in expression position is a first-class value of its
callable type. Constructors, field reads, and lookups are not.

A contract member named as a value is instantiated from its written expected
callable type, which fixes the receiver exactly as a call's operands would;
without one it is `@type.ambiguousInference`.

Callable values are not `Equatable` and are not dict keys.

A named `fn` or `proc` may refer to itself and to other named declarations
of its module; forward reference and mutual recursion within a module are
permitted. An anonymous callable has no name for itself. A call in tail
position reuses the current activation, as the runtime chapter defines.
Recursion has no depth limit of its own.

## Inference and checking

Inference is local:

- an unsuffixed numeric literal takes its type from its expected type;
- generic arguments are inferred from written operand and result types;
- a local `let` without a written type takes the type of its initializer;
- the operations a `proc` body performs are computed, to check its written
  ceiling.

Inference MUST NOT invent a parameter or result type, a generic bound, an
interface implementation, a numeric conversion, an effect ceiling, or an
error conversion, and never uses a later use to type an earlier binding.
`Never` is never inferred. When no unique type follows for a literal, an
empty `[]` or `[:]`, an absent `Option`, or a generic argument, the checker
reports `@type.ambiguousInference` with one note per missing constraint and
the places where a written type would supply it.

There are no call-site type arguments. A call whose type arguments its
operands and written expected type do not determine is rejected.

Every public `fn`, `proc`, module value, type parameter, and interface member
has a complete written type. The checker validates a body against that
contract and never rewrites the contract from the body.

## Generics

Type parameters are declared in a declaration's header: `type Pair(A B)`,
`fn first(T)`, `interface From(Source)`. `given {T: Printable}` gives a
parameter one nominal interface bound; a parameter without an entry has the
bound `Any`. A `given` entry for a name the header does not declare is
`@name.unknownSymbol`.

```vibra
fn first(T)
  takes {items: List(T)}
  returns head: Option(T)
  visibility @public
  do
    return items(0u64)
endfn
```

Generic arguments are invariant. An applied type supplies exactly its head's
parameter list; any other count is `@type.typeArgumentMismatch`. A bound is
one name and cannot apply a generic interface. Every type argument MUST
implement its parameter's bound, or the application is
`@type.unsatisfiedBound`. Partial application, named or defaulted type
arguments, multiple bounds on one parameter, and run-time type tests do not
exist.

A nested member sees the type parameters of its owner and declares only
additional ones; redeclaring an inherited name is
`@name.genericRedeclaration`. An `impl` block declares none: inside a `type`
it passes the type's parameters through, and inside an `interface` its target
MUST be a closed type expression. An anonymous callable sees the parameters
of every enclosing declaration and may declare its own; a generic anonymous
callable bound by `let` stays generic, and anywhere else it is instantiated
from its written expected type.

Specialization strategy is not observable.

## Interfaces

An `interface` declares a nominal set of member signatures over `Self`.
Conformance is always explicit: matching names and shapes are not enough.

`Any` is the predeclared empty interface. Every type satisfies it without an
implementation, and it is the only interface satisfied implicitly. `Never`
satisfies `Any` and no other interface.

### Implementations

An implementation is an `impl` block nested in the `type` that owns the
receiver, where its target names the interface, or in the `interface`, where
its target names the receiver. These two places are the orphan rule: an
implementation can be written only where the package owns the type or the
interface. The target inside a `type` MUST resolve to an interface; the
target inside an `interface` MUST resolve to a declared or builtin concrete
type, never an anonymous structural type or an interface. The other kind is
`@name.wrongEntityKind`. An `impl` inside an `interface` that targets a
type of the same module is `@type.redundantImplementation`.

A generic interface is keyed by its applied type, so one receiver may
implement `From(I16)` and `From(I8)`. The applied targets of one receiver
MUST be pairwise non-unifiable (see [Overlap](#overlap)), or the declaration
is `@type.overlappingImplementation`.

For each implementation the checker MUST find every abstract contract member
exactly once and verify, after replacing `Self` with the receiver, that the
parameter and result types, labelled names and default values, guards, and
effect row are those of the contract. An implementation MUST NOT redeclare a
default member (`@type.defaultOverride`), omit an abstract one
(`@type.missingAbstractMember`), or add a member the contract lacks. An
implementation member cannot add or strengthen a `requires`, and its
`ensures` is the contract's.

Default members are checked against the contract and are available to every
conforming type. A toolchain may specialize a default body; that is not a
second member.

### Associated types

An interface parameter is chosen by whoever uses the interface, so one
receiver may implement `From(I16)` and `From(I8)`. An **associated type** is
chosen by the implementation, once: a receiver that is an iterator has one
item type, and a conversion from one source has one error type.

```vibra
interface Iter
  type Item endtype
  fn next
    takes {value: Self}
    returns step: Option({Item Self})
  endfn
endinterface

type Once of Option(U8)
  impl Iter
    type Item of U8 endtype
    fn next
      takes {value: Self}
      returns step: Option({U8 Self})
      do
        match value
          case Once(Option.some(value: item))
            return Option.some(value: {item Once(Option.none())})
          case Once(Option.none())
            return Option.none()
        endmatch
    endfn
  endimpl
endtype
```

- In an interface, `type Name endtype` declares an associated type. A
  `given` entry may bound it. Inside the interface the name denotes the
  choice of `Self`.
- In an `impl`, `type Name of T endtype` makes the choice. It names the
  existing type `T`; it declares no new type. An implementation MUST choose
  every associated type of its interface exactly once. A missing choice is
  `@type.missingAbstractMember`.
- Associated types are not part of an implementation's key. One receiver
  implements `Iter` at most once, and `TryFrom(Str)` at most once.
- Through a type, an associated type is a path: `Once.Item` is `U8`,
  and for a parameter with `given {I: Iter}`, `I.Item` is the item type of
  whatever `I` is.
- A type expression may fix an associated type by label, after the
  positional arguments: `Iter(Item: U8)`, `TryFrom(Str Error: ParseError)`.
  In a bound, `given {I: Iter(Item: U8)}` admits only iterators of `U8`. A
  label that names no associated type is `@type.typeArgumentMismatch`.

### Dispatch

`Self` is the receiver type inside an interface, a `type`, and an `impl`. It
is not visible in a module-level callable.

A contract member that takes `Self` as the type of a required positional
parameter is dispatched on that argument. A member with no such parameter
whose result type contains `Self` is **destination-dispatched**: the checker
unifies the written expected type at the call with the member's result type
and takes `Self` from it. With no written expected type the call is
`@type.ambiguousDestination`. A member selectable by neither rule is
`@type.undispatchableContractMember` at its declaration. When two
implementations remain candidates at a call, it is
`@type.ambiguousImplementation`.

A member is called through its path: `User.nameLength(value)`,
`Printable.render(value)`. There is no inheritance between concrete types
and no overloading.

### Interface values

An interface name in type position is an interface value type: a value that
hides which conforming type it holds. A value reaches it only by widening at
a written expected type. A generic interface is written applied, and every
associated type MUST be fixed by label, since the value hides the type that
chose it: `Iter(Item: U8)`. An interface value type that leaves one open is
`@type.typeArgumentMismatch`.

- A contract member is called through an interface value by its receiver;
  the implementation is that of the held type.
- `Self` in a member's result type is replaced by the same interface value
  type, wherever it occurs in a supported position: as the result itself,
  as a component of a tuple or record, as an argument of `Option`, `Result`,
  or `List`, and inside the `Intent` a `proc` member builds. So `Iter.next`
  called through an `Iter(Item: U8)` returns
  `Option({U8 Iter(Item: U8)})`, and the remainder never shows the hidden
  type.
- A member that takes `Self` in another parameter, or whose result is a
  callable taking `Self`, cannot be called through an interface value; the
  call is `@type.mismatch`.
- An interface value type implements no interface, itself included. It
  satisfies only the bound `Any`, and it never widens again.
- No pattern, test, or conversion recovers the held type. A value of type
  `Any` can only be passed along.

A signature that needs the concrete type uses a type parameter instead.

### Overlap

Two type expressions are **unifiable** when some substitution of the type
parameters in scope makes them the same type. Unification ignores bounds, so
an overlap decision never depends on implementations written elsewhere. Two
rules require a set to be pairwise non-unifiable: the applied interface
targets of one receiver, and the source types of one receiver's `From` and
`TryFrom` implementations taken together.

## Iteration

Pure iteration uses the standard `Iter` interface. An effectful walk is a
recursive `proc` over `Iter.next`.

```vibra
interface Iter
  role @iter
  visibility @public
  type Item endtype
  fn next
    takes {value: Self}
    returns step: Option({Item Self})
  endfn
endinterface
```

The second component of a step is the remaining iterator, of the receiver's
own type. There is no mutating cursor. `next` is the only abstract member.
The declaration adds these default members:

| Member | Takes | Returns |
| --- | --- | --- |
| `map(Out)` | `{value: Self f: Fn({Item} Out)}` | `Iter(Item: Out)` |
| `filter` | `{value: Self keep: Fn({Item} Bool)}` | `Iter(Item: Item)` |
| `skip` | `{value: Self count: U64}` | `Iter(Item: Item)` |
| `take` | `{value: Self count: U64}` | `Iter(Item: Item)` |
| `collect` | `{value: Self}` | `List(Item)` |
| `fold(Acc)` | `{value: Self initial: Acc step: Fn({Acc Item} Acc)}` | `Acc` |

`map`, `filter`, `skip`, and `take` return the public standard-library
adapter types `MappedIter(Item Out)`, `FilteredIter(Item)`,
`SkippedIter(Item)`, and `TakenIter(Item)`, widened to the interface value at
the member's result. Each adapter is an ordinary `type` with a nested `impl`
supplying `next`. `collect` and `fold` drain the receiver; `fold` applies
`step` to the accumulator and each item in order, starting from `initial`.

Laws, over a finite iterator `xs` unless stated:

- **`next` is a function of its iterator.** Applying `next` again to the same
  value gives the same answer.
- **`collect` is the walk.** `collect(xs)` is the items of successive `next`
  steps, in order, and `fold(xs i s)` applies `s` over exactly those items.
- **`map` preserves structure**, **`filter` selects in order**, and
  **`take` then `skip` split** `collect(xs)`.
- **The adapters are lazy.** They call `next` on their source only when
  `next` is called on them, so `take` over an endless iterator is finite.

The standard library declares these implementations, each as a nested `impl`
where the type is declared:

| Type | `Item` | Order |
| --- | --- | --- |
| `List(T)` | `T` | index order |
| `Dict(K V)` | `{K V}` | canonical key order |
| `Str` | `Char` | scalar order |
| `Bytes` | `U8` | byte order |
| `Option(T)` | `T` | zero or one item |

`Result` does not implement `Iter`. A user type implements `Iter` with a
nested `impl` that chooses `Item` and supplies `next`. A shared algorithm over a collection is
written once, as a member of an interface or as a generic function, and never
once per collection type; `length`, `concat`, and `slice`, whose cost depends
on the representation, are static members of each type.

## Control flow and failure

### Joins

`if` requires a `Bool` condition. When the value of an `if` or `match` is
used, the branches that can complete MUST have one common type, which is the
type of the form; branches of type `Never` are skipped, and a form all of
whose branches have type `Never` has type `Never`. This is the only join the
checker computes. It never searches for an interface that covers two
differing types.

### Coverage

`match` is checked for exhaustiveness and reachability; irrefutability of a
binding pattern is the same check with one arm.

- A variant type is covered when every inhabited case is. `Bool` is covered
  by `true` and `false`.
- Tuple, record, declared tuple, declared record, and wrapper patterns cover
  their type when every written component pattern covers its own.
- A fixed-length list pattern never covers `List(T)`.
- Literal patterns never cover `Str`, `Bytes`, `Char`, or a numeric type;
  atom patterns never cover `Atom`. A binder or `_` must cover the rest. An
  atom singleton type is covered by its one atom.
- A position whose type is uninhabited has nothing to cover.

A `match` that does not cover its subject is `@pattern.nonExhaustive` at the
`match`, with a note naming the first uncovered shape in declaration order,
in pattern spelling with `_` for a position only a binder can cover. An arm
no value can reach is `@pattern.unreachableArm`, relating the earliest arm
that covers it. A refutable pattern where an irrefutable one is required is
`@pattern.refutableBinding`; an irrefutable pattern in a `let … else` is
`@pattern.irrefutableLetElse`. When the checker cannot establish coverage
within its limits it reports `@pattern.unsupportedCoverage`; it never
assumes coverage.

### Constant expressions and constant patterns

A **constant expression** is a literal, an atom, or `void`; a construction,
tuple, record, list, or dict whose operands are all constant expressions and
whose type has no `where … else`; or the name of a module value whose
initializer is a constant expression. A function call is never a constant
expression, so the property is decidable without evaluation.

A name in pattern position that resolves to a constant module value is a
constant pattern: it means the pattern of that value and binds nothing.
Adding a module value named `x` therefore turns an existing pattern `x` from
a binder into a value match. Coverage treats a constant pattern as its
expanded value.

Labelled defaults are constant expressions. Two defaults are the same when
they denote the same value.

### `try`

`try` takes an operand of type `Option(T)` or `Result(T E)` inside a callable
whose result type is the same carrier: `Option(U)`, or `Result(U E)` with the
identical `E`. Its value has type `T`. On `none` or `err` it leaves the
innermost enclosing callable with that absence or error. Any other use is
`@type.invalidTry`. `try` never converts an error type and is always invalid
directly in a `test` body.

When a `try` expression has a written expected type `T`, its operand is
checked at the expected type `Option(T)` or `Result(T E)` of the enclosing
carrier, so a destination-dispatched operand needs no extra annotation:

```vibra
let port: U16 be try TryFrom.convert(text) endlet
```

### Fallible values

A value of type `Result(T E)` is fallible unless `E` is uninhabited. An
expression statement that leaves a fallible value unused is
`@type.unhandledFallible`; `let _ be expr endlet` states the intent. An
`Option` is not fallible.

### `return` and `Never`

The operand of `return` is checked at the enclosing callable's written result
type as a written expected type. `return` has type `Never`.

`Never` has no value. An expression has type `Never` when it is a `return`,
a call of a callable whose result type is `Never`, or an `if`, `match`, or
`do` every completing path of which has type `Never`. An expression of type
`Never` is admitted at any expected type; this is not widening. It may appear
only as a statement, including the last statement of a branch, arm, block, or
fallback; in an operand position it is `@type.unreachableCode`. A statement
after a statement of type `Never` is `@type.unreachableCode` as well.

A type is **uninhabited** when it is `Never`, a tuple, record, or wrapper
with an uninhabited component, or a variant type all of whose cases have an
uninhabited field. Every other type is inhabited, including `List`, `Dict`,
callable types, interface values, and type parameters. A case with an
uninhabited field needs no arm, so a `match` on `Result(T Never)` needs only
the `ok` arm.

## Guards

`requires`, `ensures`, and `where` share one rule: a guard without `else` is
a static obligation, and a guard with `else` states its consequence. No guard
traps, and no run-time check exists that the author did not write.

### Predicates

A guard's predicate and its `else` consequence MUST be **certified**
expressions: built only from parameters and constants, constructions, field
reads, `if` and `match` with full coverage, the primitive comparison and
`Bool` operations, and calls of certified functions. A certified function is
a `fn` whose body is certified and whose recursion, if any, the checker can
show terminates. A certified expression cannot fault. A predicate that is
not of type `Bool`, or a `where` operand that is not a `Fn({R} Bool)`, is
`@contract.invalidPredicate`; a consequence of the wrong type, or a
`where … else` consequence that is not constant, is
`@contract.invalidConsequence`. An expression outside the fragment is
`@contract.unsupportedPredicate`; ordinary validation code
that needs more returns a `Result` explicitly and is not a guard.

A predicate concerns immutable values. It cannot mention the state of a file,
a clock, a quota, or any other live resource; those are run-time outcomes of
operations.

### Bare guards

| Guard | Obligation | Checked at |
| --- | --- | --- |
| `requires C` | `C` holds for the actual arguments | every call; for a `proc`, where its plan is built |
| `ensures C` | `C` holds for the returned value | every `return` of the body |
| `where P` | `P` holds for the assembled representation | every construction |

The checker discharges an obligation with these rules and no others:

1. **Constants.** The predicate over constant operands evaluates to `true`.
2. **Type invariant.** A value of a type with an invariant supplies that
   type's own predicate over its representation.
3. **Dominating branch.** Inside the true branch of an `if`, its condition
   holds; inside the false branch, its negation. A `match` arm supplies the
   case and field facts of its pattern. Facts hold only within the branch.
4. **Substitution.** Renaming, immutable `let` aliases, literal equality, and
   substitution of actual arguments. Two predicates match only when their
   resolved callee, type arguments, and operands are the same; equal source
   text is not enough.
5. **Boolean structure.** Introduction and projection of `Bool.and`, and the
   branch rules of the fixed `Bool` operations.
6. **Listed integer facts.** The exact-width comparison facts the
   [conformance chapter](07-diagnostics-and-conformance.md#integer-facts) lists, such as
   `Ordered.greater(x y)` implying that `x` and `y` are not equal.
7. **Certified results.** After a call returns, the callee's proven `ensures`
   holds for the result, provided its `requires` was discharged.

A discharged guard leaves no code. An obligation the rules refute is
`@contract.refuted`; one they cannot establish is `@contract.unproved`, and
the diagnostic names the predicate and the repairs: branch on the same
predicate, pass a value of a type whose invariant states it, or give the
guard an `else`. An obligation is never moved to run time, and a sampled test
never discharges one.

A bare `where` leaves the constructor's result type unchanged.

### Guards with `else`

`requires C else V`. `C` is evaluated once on entry. If it is false, `V` is
evaluated and returned; the body and the `ensures` are skipped. `V` MUST
have the callable's result type. A caller has no obligation.

`ensures C else V`. The body's returned value is the **candidate**, bound to
the result name. `C` is evaluated once on the candidate. If it is true, the
candidate is returned; otherwise `V` is evaluated once, with the result name
still bound to the candidate, and returned. `V` is not checked against `C`
again. The clause therefore promises that the returned value is either a
candidate that satisfied `C` or the value `V` produced; it does not promise
`C` of the returned value.

`where P else E`. The constructor's result type becomes
`Result(T ErrorType)`, where `ErrorType` is the static type of `E`. `E` MUST
be a constant expression. When `P` is false of the assembled representation,
construction yields `Result.err(error: E)`; otherwise `Result.ok(value: …)`.
An error that depends on the input is produced by an ordinary function that
returns a `Result`.

A declaration has at most one `requires` and one `ensures`. Several
conditions are one predicate joined with `Bool.and`; a callable that needs
different consequences for different failed conditions tests them in its body
and returns the values there.

When the checker can prove a guard that has an `else`, it removes the check
and the consequence. No type changes: a `where … else` constructor returns a
`Result` whether or not a particular construction is proven. A constant
construction known to fail is a legal `Result.err` value. A `let` pattern
`Result.ok(value: x)` over a construction is irrefutable only when rule 1
proves that construction succeeds.

### Guards and callable types

A callable type has no place for a caller obligation. A callable with a bare
`requires` converts to a callable type only when its parameter types alone
establish that `requires`; otherwise the conversion is
`@contract.obligationLost`. A callable whose `requires` has an `else`
converts freely, and the resulting value keeps its entry and exit checks.

A bare `requires` on a `proc` is discharged where the plan is built, from the
arguments the plan captures. A `requires … else` and an `ensures … else` on a
`proc` run on each `await` of its plan.

A contract member's guards are part of its contract. Each implementation is
checked under the contract's `requires` and MUST establish the contract's
`ensures`.

## Widening

Vibra has exactly two widening relations: a concrete type widens to an
interface it conforms to, and the singleton type of an atom widens to `Atom`.
Neither is subtyping. Each applies once, at a **written expected type**,
never through a container and never to an interface value. The written
expected types are:

- a parameter type, required or labelled;
- a written result type, for the operand of each `return`;
- the type of a module value;
- the written type of a local `let`;
- a declared field type, case field type, or wrapper representation type, at
  its constructor.

A type that inference fixed is not a written expected type. Generic arguments
are invariant: `List(I32)` does not widen to `List(Printable)`. Where no
expected type is written, no widening occurs. Widening is pure.

## Conversion

Conversion between unrelated types is an ordinary call. The standard library
declares two interfaces, generic over the source and implemented on the
destination:

```vibra
interface From(Source)
  visibility @public
  fn convert
    takes {value: Source}
    returns converted: Self
  endfn
endinterface

interface TryFrom(Source)
  visibility @public
  type Error endtype
  fn convert
    takes {value: Source}
    returns converted: Result(Self Error)
  endfn
endinterface
```

Both members are destination-dispatched: the written expected type at the
call fixes the destination. There is no `Into`. Each `TryFrom`
implementation chooses its `Error`. The standard library's implementations
all choose `ConversionError`, a public variant type with the cases
`outOfRange`, `invalidFormat`, and `unrepresentable`.

For two distinct integer types, the destination implements `From(Source)`
exactly when it holds every value of the source, and `TryFrom(Source)`
otherwise. Every number type implements `TryFrom(Str)`, which parses decimal
text, and `Str` implements `From(N)` for every number type. `Dict(K V)`
implements `From(List({K V}))`; a later duplicate key replaces the earlier
entry. The floating-point types convert to and from other number types
through no interface.

Across `From` and `TryFrom` together, one receiver's source types MUST be
pairwise non-unifiable, or the pair is `@type.redundantConversion`.

## Value semantics

Bindings and values are immutable. Passing a value preserves it for the
caller. An implementation may share storage as long as identity is not
observable. There are no user-visible references, pointers, object identity,
destructors, or shared mutable cells.

Authority to act on the world is carried by grant values, which are sealed:
no source expression constructs, copies the contents of, or encodes one. The
effects chapter defines them.

## Exclusions

- untagged unions, anonymous variant types, and any computed common
  supertype;
- an ascription expression and a narrowing pattern;
- call-site type arguments, partial type application, multiple bounds;
- tuple index projection and application of tuples or records;
- blanket implementations, generic `impl` targets inside an interface,
  inheritance, and overloading;
- narrowing an interface value, run-time type tests;
- guards on variant types, quantified predicates, user-supplied proofs, and
  any guard checked by sampling;
- dependent types beyond the invariant and guard rules above, and an
  ownership or borrowing system;
- hashed collections and a hashing interface.
