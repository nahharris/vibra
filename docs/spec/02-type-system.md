# Vibra type system

Status: normative target
Implementation status: none of this chapter is implemented. The
[release roadmap](../roadmap/README.md) schedules it.

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

- the representation of the scalar types `Void`, `Atom`, `Char`, `I8`
  through `I64`, `U8` through `U64`, `F32`, `F64`, and the uninhabited type
  `Never`;
- the representation of the one generic immutable sequence `List(T)`;
- the `Fn`, `Proc`, and `Intent` types and the structural tuple and record
  types.

The **builtin types** of the first two groups are nevertheless declared in
Vibra, in the embedded module `@std.builtin`, each as a `type` whose body is
`Intrinsic(@tag)` with the type's wire tag, with its static members bound to
`@compiler` intrinsics and its implementations in nested `impl` blocks like
any other type:

```vibra
type List(T) of Intrinsic(@list)
  visibility @public
  fn length
    takes {items: Self}
    returns count: U64
    visibility @public
    external @compiler
    symbol "list.length"
  endfn
  impl Iter
    type Item of T endtype
    fn next
      takes {value: Self}
      returns step: Option({T Self})
      external @compiler
      symbol "list.next"
    endfn
  endimpl
endtype
```

`Intrinsic` gives the compiler the representation and nothing else: the
declaration's members, implementations, visibility, and identity are those
of its source like any other type's. `@std.builtin` is in every checked
graph and is never imported; its public declarations are reached through the
prelude. Only it may write `Intrinsic`.

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
section, on a declaration of the shape the role requires: `@bool` on a
variant type with exactly the cases `false` and `true`; `@str`, `@bytes`,
`@dict`, `@option`, and `@result` on the types with the constructors and
cases this chapter names; `@iter` on an interface whose members are exactly
the associated type `Item`, the abstract `next` of
[Iteration](#iteration), and default members; `@equatable` and `@ordered` on
the two interfaces of
[Dict keys, equality, and order](#dict-keys-equality-and-order). A `role`
section outside the embedded library is `@syntax.invalidSection`.
A missing, repeated, or wrong-shaped role in the embedded library is a
toolchain defect, reported as `@runtime.invalidCheckedProgram` when the
library is loaded, never as a fallback. Adding a role is a change to this
table.

The standard library declares `Bool` as a variant type with the cases `false`
and `true`, `Option(T)` as one with the cases `none` and `some {value: T}`, in
that order, `Str` as a wrapper over `List(Char)`, and `Bytes` as a wrapper over
`List(U8)`. Their constructors and patterns are the declared ones:
`Bool.true()`, `Str(scalars)`, `Bytes(items)`. The `Bool` module also declares
the two public module values `true` and `false`, which are the only way source
writes a boolean. `Dict(K V)` is a wrapper over `List({K V})`, sorted by the
`Ordered.compare` of its keys with one entry per key; its representation is
private, so it is built by a dict literal or by conversion and read by lookup
and `Dict.entries`. A toolchain may represent any of these values directly, as
the [runtime chapter](06-runtime.md) allows.

Every operation over a library type is standard-library Vibra, which a
toolchain may accelerate with a native implementation whose meaning stays its
Vibra body.

### The prelude

The compiler-owned types, the types and interfaces that play a role, the
conversion interfaces `From` and `TryFrom`, `Any`, `Self`, `Ordering`, and
the values `true` and `false` are the only declarations a program names
without an import. This closed set is the
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
infinity is out of range. There is no source spelling for infinity or NaN;
arithmetic produces them, and the
[canonical value encoding](06-runtime.md#values) represents them as data.

Plain integer arithmetic saturates. Division and remainder exist only in
their checked forms, which return a `Result`, as do the other checked
operations (`addChecked` and so on). The runtime chapter gives each
operation's exact meaning. No arithmetic operation traps.

## Declared types

Every `type` declaration introduces a new identity. Two declared types with
the same structure are different types. There are no transparent aliases and
no implicit conversion between a declared type and its representation.

A declaration's identity is the hash of its logical anchor: the package name
and version, the module path (which begins with the unit), the owner path,
the declaration kind, and the name. The anchor does not include the
declaration's body, so a type's identity does not depend on its fields,
members, or guards. Renaming or moving a declaration changes its identity.
The identity is the string `sha256:` followed by 64 lower-case hexadecimal
digits of the SHA-256 of these bytes, in order:

1. the ASCII text `vibraDeclarationIdentityV2`;
2. one zero byte;
3. six byte strings, each preceded by its length as a big-endian 64-bit
   integer: the package name, the package version, the module path atom with
   its `@`, the owner path (the dotted member path within the module, empty
   for a top-level declaration), the declaration kind atom, and the name.

The declaration kinds are `@module`, `@type`, `@interface`, `@case`, `@fn`,
`@proc`, `@value`, `@test`, and `@associatedType`. A module's anchor has an
empty owner path and its own path's last part as the name. Test vector:
package `hello` version `0.1.0`, module `@hello.main`, no owner, kind
`@proc`, name `main` gives
`sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad`.

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

The record fields, cases, and nested `fn` and `proc` members of one type
share one flat member namespace, and so do the members of one interface,
its associated types included. Two members of one owner with the same name
are `@name.memberCollision`. Implementations add no name to it.

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
type is `@type.invalidInvariant`. The operand is an expression like any
other function value: an anonymous `fn`, or the name of a `fn`, which
resolves in the flat module scope and so may be declared after the type.

```vibra
type Percent of U8
  where fn
    takes {value: U8}
    returns valid: Bool
    do
      return Ordered.lessOrEqual(value 100u8)
  endfn
endtype

type Port of U16
  where isUserPort
endtype

fn isUserPort
  takes {value: U16}
  returns valid: Bool
  do
    return Ordered.greaterOrEqual(value 1024u16)
endfn
```

The [Guards](#guards) section defines what a bare `where` and a
`where … else` mean. In both forms:

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

A type nests at most 64 levels. A type with no component is one level, and
every other type is one level more than its deepest component: an element,
a tuple component, a field, a parameter, a result, a type argument, or the
choice of an associated type. A written type nested deeper, and an
expression whose type is, is `@type.depthExceeded` at the type or the
expression. A declared callable's own type is a type of the program too, so
a parameter or result type of depth 64 makes it 65 deep, which is
`@type.depthExceeded` at the deepest written parameter or result type. The
limit is part of the language and is the same in every tool.

An anonymous type has no owner: it declares no members and receives no
`impl`. It conforms to `Any` and, under the structural rule of
[Dict keys, equality, and order](#dict-keys-equality-and-order), to
`Equatable` and `Ordered`; to no other interface. It cannot refer to itself.

### Recursive types

A declared type may refer to itself and to other declared types of its module,
by name. A reference to a type is symbolic and is not a dependency that must be
complete first. Every declared type MUST have a finite size: recursion through
`List`, `Dict`, or a callable type is permitted, and direct infinite expansion
is `@type.infiniteSize` over the whole declaration, `type` to `endtype`, of the
type whose expansion first repeats. Values are immutable and are built from
values that already exist, so no value contains itself.

### Callable types

`Fn({A B} R)` is the type of a pure function with required positional types
`A` and `B` and result `R`. Labelled parameters follow the positional ones in
the input vector: `Fn({I32 verbose: Bool} Bool)`. Defaults belong to the
function value and are not part of its type. Each label names one input: a
label written twice in the input vector of a `Fn` or `Proc` type is
`@type.typeArgumentMismatch` at the second.

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
anonymous tuple or record whose every component implements `Ordered` conform
to `Equatable` and `Ordered` **structurally**, with no declaration. This
structural conformance is ordinary evidence: it satisfies a bound, it admits
the type as a dict key, and it is the implementation `Ordered.compare` and
`Equatable.equal` dispatch to for such a type. `Void`, `Never`, `F32`,
`F64`, callable types, `List`, `Dict`, and `Option` are not admissible keys.
A declared type is admissible only through its own `Ordered`
implementation. An inadmissible key type is `@type.invalidDictKey`; a
callable key is `@type.functionNotEquatable`.

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
atom, number, interface value, or a value of a type parameter. Applying one
is `@type.notApplicable` at the callee. There is no callable interface.

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
callable type. Constructors, field reads, and lookups are not. A declared
type or case named in value position without being applied is
`@type.mismatch` at the name, whether a function value, another type, or no
type is expected there; as an argument it does not fit its slot. An
anonymous `fn` wraps a constructor where a function value is needed.

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
interface implementation, a numeric conversion, an effect ceiling, or an error
conversion, and never uses a later use to type an earlier binding. Inference
never invents `Never`, but a written type that fixes a type argument to `Never`
binds it: `Result.ok(value: void)` checked against a written `Result(Void
Never)` is accepted. When no unique type follows for a literal, an empty `[]` or
`[:]`, an absent `Option`, or a generic argument, the checker reports
`@type.ambiguousInference` with one note per missing constraint and the places
where a written type would supply it.

There are no call-site type arguments. A call whose type arguments its
operands and written expected type do not determine is rejected.

The type arguments of one call are solved together: the checker unifies the
callee's parameter types with the types of all written operands and, when
one is written, the callee's result type with the expected type. An operand
whose own type arguments are open, such as the construction
`Result.ok(value: 2u64)` whose error type nothing inside it fixes, takes them
from that unification, so `assert.equal(Result.ok(value: 2u64) actual)`
is well-typed when `actual` has a complete `Result` type. A variable that
is still open after every operand and the expected type have been used is
`@type.ambiguousInference`.

Every public `fn`, `proc`, module value, type parameter, and interface member
has a complete written type. The checker validates a body against that
contract and never rewrites the contract from the body.

## Generics

Type parameters are declared in a declaration's header: `type Pair(A B)`,
`fn first(T)`, `interface From(Source)`. `given {T: Printable}` gives a
parameter one interface bound; a parameter without an entry has the bound
`Any`. A `given` key MUST be a type parameter of the same header: an entry
for any other name, an inherited parameter included, is
`@name.unknownSymbol` at the key, and a repeated key is
`@name.redeclaration` at the later entry, related to the earlier.

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
parameter list; any other count is `@type.typeArgumentMismatch` at the
application. Every type argument MUST implement its parameter's bound, or
the application is `@type.unsatisfiedBound` at the argument, related to the
bound. When an application's shape is itself invalid, only
`@type.typeArgumentMismatch` is reported for it. Partial application,
named or defaulted type arguments, multiple bounds on one parameter, and
run-time type tests do not exist.

A generic callable MUST NOT reach itself at a type built around one of its
own type parameters. Each call of a generic callable, and each reference to
one as a value, inside the body or the guards of a generic callable is an
instantiation. So is each member of the implementation that a contract
call's receiver selects there, and of each implementation that satisfies a
bound of the callable being instantiated. A type argument of an
instantiation carries a type parameter of the enclosing callable to the
parameter it is for when it mentions it, and expands it when it mentions it
and is not that parameter alone. Every chain of instantiations that leads a
type parameter back to itself MUST hand it on unchanged at every step; a
type argument that mentions no type parameter is on no such chain. An
expanding type argument on such a chain is `@type.expandingInstantiation` at
its call or reference, related to the other instantiations of the chain.
A `fn f(T)` that calls `f` with a `List(T)` operand is rejected, and so is
one that calls a `g(U)` at `List(T)` when `g` calls `f` at `U`. A call of
`f` at `T` or at `U8` inside `f` is accepted, and so are two callables that
exchange their parameters unchanged.

### Bounds

A bound is an interface, written exactly as the interface is written in any
type position: bare, applied to positional arguments, or applied with the
labels of [Associated types](#associated-types).

```vibra
fn largest(T)
  takes {items: List(T)}
  returns largest: Option(T)
  given {T: Ordered}
  visibility @public
  do
    return Iter.fold(
      items
      Option.none()
      fn
        takes {best: Option(T) item: T}
        returns next: Option(T)
        do
          match best
            case Option.none()
              return Option.some(value: item)
            case Option.some(value: current)
              return Option.some(value: Ordered.max(current item))
          endmatch
      endfn
    )
endfn
```

- A bound that does not resolve to an interface, bare or applied, is
  `@name.wrongEntityKind` at the bound. Structural, callable, and interface
  value types are not bounds; the bound `Any` is written by omitting the
  entry.
- `given {T: From(I16)}` admits every type that implements `From(I16)`.
- `given {I: Iter(Item: U8)}` admits every type whose `Iter`
  implementation chooses `U8` for `Item`.
- A label may name another parameter of the same header:
  `given {T: Ordered I: Iter(Item: T)}`. This is how a generic names an
  associated type: it binds the choice to a parameter and uses the
  parameter. At each use of the generic the checker selects the `Iter`
  implementation of the actual `I` and takes `T` from that implementation's
  choice. If another operand or the expected type also fixes `T`, the two
  MUST agree, or the operand is `@type.mismatch`; and `T` MUST then satisfy
  its own bound, or the application is `@type.unsatisfiedBound`.
- A positional argument of a bound may likewise name a parameter of the same
  header: `given {T: From(S)}`. The bound's implementation is selected by
  its [key](#implementations): the interface, the actual type of the bounded
  parameter as receiver, and the actual positional arguments.
- At each use, bounds are resolved after the ordinary
  [inference](#inference-and-checking) of type arguments from the operands
  and the expected type. A bound is **resolvable** when its parameter and
  every parameter in its positional arguments are determined. Resolving it
  selects one implementation by key, and that implementation's choices
  determine every parameter the bound's labels name, which may make another
  bound resolvable. The set of implementations never determines a receiver
  or a positional argument: a parameter still undetermined when no bound is
  resolvable is `@type.ambiguousInference` at the application. A resolvable
  bound with no implementation for its key is `@type.unsatisfiedBound`.
- Inside the generic, a parameter has exactly the members of its bound, and
  a call on it dispatches to the actual type's implementation. The evidence
  for that dispatch, including every choice taken from it, is part of the
  activation, as the [runtime chapter](06-runtime.md#generic-instantiation)
  requires.

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
implementation. The only other conformance without an `impl` is the
structural `Equatable` and `Ordered` conformance of atoms and of anonymous
tuples and records of
[Dict keys, equality, and order](#dict-keys-equality-and-order). `Never`
satisfies `Any` and no other interface.

`Self` may be written anywhere in a contract member's parameter types. In a
member's written result type it may appear only in a position of the
**`Self` map**: the result itself, and recursively a component of a tuple or
record, an argument of `Option`, `Result`, or `List`, and the result of the
`Intent` a `proc` member builds. A `Self` written anywhere else in a result
type, such as in `Dict(Str Self)`, in an argument of another generic type,
or in a callable type, is `@type.unsupportedSelfPosition` at the result
type. The check runs when the interface declaration is checked, on the
written occurrences and before any substitution, for abstract and default
members alike, whether or not the interface is ever used as an
[interface value](#interface-values).

The **receiver slot** of a contract member is its first positional parameter
whose written type is exactly `Self`. A member has at most one receiver
slot; a later parameter whose type is or contains `Self` is an ordinary
input. A member with no receiver slot has none, however its other parameter
types mention `Self`.

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

The **key** of an implementation has three parts: the interface, the
receiver type, and the interface's positional arguments. Both nesting forms
give all three:

| Written in | Interface | Receiver | Positional arguments |
| --- | --- | --- | --- |
| a `type` | the head of the target | the enclosing type, applied to its own parameters | the arguments written in the target |
| an `interface` | the enclosing interface | the target | the interface's own parameters, passed through |

So `impl From(I16)` in `type Cents` has the key `From`, `Cents`, `I16`, and
one receiver may implement `From(I16)` and `From(I8)`. An implementation
written in a generic interface covers every application of that interface
to its receiver. A key may be open in the parameters of its enclosing
declaration and in nothing else. The arguments written in a target inside a
`type` are type arguments of the interface, so each MUST implement its
parameter's bound, where a parameter of the enclosing type has no bound:
`impl Feed(Plain)`, where `Feed(S)` has `given {S: Ordered}` and `Plain`
implements no `Ordered`, is `@type.unsatisfiedBound` at `Plain`, related to
the bound. A target admits no label
(`@type.typeArgumentMismatch` at the label), so a key is always visibly
positional. Two keys of the same interface whose receivers and positional
arguments unify [overlap](#overlap): the later implementation is
`@type.overlappingImplementation`, related to the earlier. Keys of
different interfaces never overlap, so `Iter` and `Ordered` on one receiver
are two keys.

For each implementation the checker MUST find every abstract contract member
exactly once and verify that, under the implementation's
[substitution](#substitution), the parameter and result types, labelled
names and default values, the member's own type parameters and their
bounds, guards, and effect row are those of the contract, the member's
type parameters matched in order whatever their names.

| Condition | Diagnostic |
| --- | --- |
| an abstract callable member or an associated type the implementation does not supply | `@type.missingAbstractMember`, the `impl` head, related to the contract member |
| a member or choice whose name is no member of the interface | `@name.unknownSymbol`, the name |
| a callable member supplied twice in one `impl` | `@name.memberCollision`, the later name, related to the first |
| an associated type chosen twice in one `impl` | `@name.redeclaration`, the later choice, related to the first |
| a default member supplied again | `@type.defaultOverride`, the member's name, related to the default |
| a member whose signature differs from the contract under the substitution | `@type.mismatch`, the differing type, default value, or written bound (the type parameter when none is written), or the member's name for a differing count of parameters or set of labels, related to the contract's counterpart |

An implementation member declares no guard of its own: the contract's
`requires` and `ensures` are its guards, as
[Guards and callable types](#guards-and-callable-types) states.

Default members are checked once, against the abstract contract with the
interface's parameters and associated types open and bounded only as the
interface declares. At each use their signatures, bodies, and labelled
constant defaults are instantiated under the substitution of the selected
implementation. A toolchain may specialize a default body; that is not a
second member.

### Substitution

Every rule that relates a contract member to a use goes through one closed
**substitution**, formed from a selected implementation. In an activation
every type is closed, and selecting an implementation unifies its key with a
closed receiver and closed positional arguments, which fixes every parameter
the key is open in. Where a generic body is checked, a receiver may still be
a type parameter; [Dispatch](#dispatch) then checks the call against the
parameter's bound, and the key closes when the body is activated. Under the
instantiation of a selected implementation:

- `Self` maps to the key's receiver;
- each parameter of the interface maps to the corresponding positional
  argument of the key;
- each associated type maps to the implementation's choice.

Nothing else is substituted, and the result contains no open name of the
interface or of the implementation's enclosing declaration. Where an
implementation is checked rather than used, the same three maps are taken
with the enclosing declaration's parameters left open. The substitution is
used, and only it, for signature conformance
of implementation members, for instantiating default members, for
[dispatch](#dispatch), for [widening](#widening) to an interface value, and
as the run-time evidence the
[runtime chapter](06-runtime.md#generic-instantiation) carries in
activations, closures, and plans.

### Associated types

An interface parameter is chosen by whoever uses the interface, so one
receiver may implement `From(I16)` and `From(I8)`. An **associated type** is
chosen by the implementation, once: a receiver that is an iterator has one
item type, and a conversion from one source has one error type. The prelude
interface `Iter` of [Iteration](#iteration) declares the associated type
`Item`; `Once` below implements it:

```vibra
type Once of Option(U8)
  impl Iter
    type Item of U8 endtype
    fn next
      takes {value: Self}
      returns step: Option({Item Self})
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

- In an interface, `type Name endtype` declares an associated type. It is a
  member of the interface, in the interface's member namespace, and it is
  not bounded where it is declared: a `given` entry names only a header
  parameter. A use that needs the choice to satisfy an interface binds it to
  a bounded parameter, as [Bounds](#bounds) shows.
- In an `impl`, `type Name of T endtype` makes the choice. It names the
  existing type `T`; it declares no new type. An implementation MUST choose
  every associated type of its interface exactly once, under the
  diagnostics of [Implementations](#implementations).
- Associated types are not part of an implementation's key. One receiver
  implements `Iter` at most once, and `TryFrom(Str)` at most once.

An associated type has exactly two ways to be named, and there is no
projection path through a type or a parameter:

1. **Its bare name.** Inside the interface that declares it, the name
   denotes the choice of `Self`. Inside an `impl` of that interface, in the
   member signatures and bodies, the name denotes the chosen type, so
   `Once.next` above returns `Option({Item Self})` and `Item` is `U8`
   there. A choice is resolved before the associated names of its own
   `impl` are in scope, so a choice cannot mention an associated type of
   the same `impl`; such a name is `@name.unknownSymbol`. An associated
   type MUST NOT have the name of a type parameter visible where it comes
   into scope: declared in an interface with a parameter of that name, it
   is `@name.genericRedeclaration` at the associated type, and brought into
   an `impl` where a parameter of that name is visible, it is
   `@name.genericRedeclaration` at the `impl` head, each related to the
   parameter. An associated type with the name of another member of its
   interface is `@name.memberCollision`, as for any two members.
2. **A label in a type application**, after the positional arguments, which
   fixes the choice: `Iter(Item: U8)`, `TryFrom(Str Error: ParseError)`. A
   label that names no associated type, a label repeated, or a label in an
   `impl` target is `@type.typeArgumentMismatch` at the label.

So `I.Item` and `Once.Item` are not type paths. A type parameter has the
callable members of its bound and a type has its own members, and neither
owns an associated type name. In `fn f(I) takes {x: I.Item}` the part
`Item` resolves to nothing and is `@name.unknownSymbol` at `Item`, related
to the type parameter, whatever the bound of `I` is.

### Dispatch

`Self` is the receiver type inside an interface, a `type`, and an `impl`. It
is not visible in a module-level callable.

A call of a contract member on a concrete type selects one implementation
by its [key](#implementations) and only then checks the call under that
implementation's [substitution](#substitution). Selection never looks at an
associated type, because no choice exists before an implementation is
selected.

1. **Receiver constraint.** A member with a
   [receiver slot](#interfaces) is dispatched on that argument (receiver
   dispatch): the argument's type is the receiver. A member with no
   receiver slot is dispatched on the expected type at the call
   (destination dispatch), from whichever declaration or annotation
   supplies it: any [written expected type](#widening), the parameter type
   of an enclosing call and the field type of an enclosing construction
   included. The checker matches that type against the
   member's written result type, structurally and only through the
   positions of the `Self` map, and collects the type found at every
   position where `Self` is written. All of them MUST be one type, which is
   the receiver. An expected type whose shape has no such position, or
   positions that give different types, as when `{Self Self}` meets
   `{U8 U16}`, is `@type.mismatch` at the application. Only a call with no
   expected type is `@type.ambiguousDestination`.
2. **Positional constraints.** Wherever a parameter of the interface is
   written in the member's parameter types or result type, the matching
   operand type or part of the expected type constrains it, by the ordinary
   [inference](#inference-and-checking) of type arguments. Wherever an
   associated type is written, nothing is constrained: the position is
   recorded as a **deferred equality** between the choice and the type
   found there.
3. **Bound evidence.** When the receiver is a type parameter in scope, no
   implementation is selected where the body is checked. The parameter has
   exactly the members of its [bound](#bounds), so the interface and the
   positional constraints MUST be those of the bound, or the call is
   `@type.unsatisfiedBound` at the receiver operand or the expected type.
   The call is checked in step 5 against the **evidence view**: the
   signature with `Self` replaced by the parameter, each parameter of the
   interface by the bound's positional argument, and each associated type
   by the type the bound's label fixes. An associated type the bound leaves
   unfixed stays abstract: it equals itself and no other type, so a
   deferred equality with anything else is `@type.mismatch`. The key closes
   at activation, when the parameter has its actual type and the
   implementation selected for it is the activation's evidence. Step 4 is
   skipped.
4. **Selection.** Otherwise the receiver and the positional constraints
   select the implementations of this interface whose key unifies with them
   without fixing a type parameter in scope. None is
   `@type.unsatisfiedBound` at the receiver operand or the expected type,
   related to the interface, unless a positional constraint fixes a
   parameter of the interface to a type that does not implement that
   parameter's bound: the call is then `@type.unsatisfiedBound` at the
   operand or the expected type that fixes it, related to the bound. A
   parameter that only the expected type fixes is checked against its bound
   once it is fixed, like any other. More than one, which happens only when a
   positional argument is still undetermined, is
   `@type.ambiguousImplementation` at the application.
5. **Checking.** The selected implementation's substitution, or the
   evidence view of step 3, is applied to the complete signature. Then, and
   only then, every operand is
   checked against its parameter type, every deferred equality is compared,
   and the complete result type is compared with the expected type, like
   any other call. A parameter outside the receiver slot whose written type
   is `Self` therefore requires an argument of the receiver's type, and an
   argument of another type is `@type.argumentMismatch` at that argument. A
   deferred equality or a result that disagrees is `@type.mismatch`.

**Member-local parameters.** A member's own type parameters, such as `Acc`
in `Iter.fold(Acc)`, are no part of any key and play no part in steps 1 to
4. They are inferred in step 5 by the ordinary joint
[inference](#inference-and-checking) of type arguments, from the operands
and the written expected type, against the signature as step 5 instantiated
it: under the selected substitution, under the evidence view, or, for a call
through an interface value, under the
[public view](#interface-values). A member-local parameter the call does
not determine is `@type.ambiguousInference`, whichever implementation was
selected.

The four cases below are worked through the steps. `once` has the type
`Once` of [Associated types](#associated-types), and `box` has the type
`Iter(Item: U8)`.

| Call | Constraints | Selection | Checking and result |
| --- | --- | --- | --- |
| `Iter.next(once)` | The receiver slot is parameter 0, so the receiver is `Once`. `Iter` has no positional argument. The written `Item` defers nothing here, since no expected type is written. | The key `Iter`, `Once`, no arguments selects the `impl Iter` in `Once`. | `Self` is `Once` and `Item` is `U8`. The operand is checked against `Once`, and the result is `Option({U8 Once})`. |
| `TryFrom.convert("12")` with the written expected type `Result(U8 ConversionError)` | No receiver slot. The written result `Result(Self Error)` is matched with the expected type: the one `Self` position gives the receiver `U8`, and the `Error` position defers `Error` equal to `ConversionError`. The operand constrains `Source` to `Str`. | The key `TryFrom`, `U8`, `Str` selects one implementation. | `Self` is `U8`, `Source` is `Str`, `Error` is the implementation's choice `ConversionError`. The operand is checked against `Str`, the deferred equality holds, and the complete result `Result(U8 ConversionError)` equals the expected type. |
| `Iter.next(box)` | The receiver is an interface value, so the call is checked against the public view: `Self` is `Iter(Item: U8)` and `Item` is `U8`. | None at the call site. At run time the call unboxes `box` and runs `next` under the substitution stored when the held value was widened. | The operand is checked against `Iter(Item: U8)`. The activation returns the held type's `Option({U8 Self})`; the `Self` position is reboxed with the same implementation, and the result is `Option({U8 Iter(Item: U8)})`. |
| `Iter.fold(once initial step)` | The receiver slot is parameter 0, so the receiver is `Once`. `Acc` is member-local and constrains nothing. | The key `Iter`, `Once`, no arguments, exactly as for `next`. With `box` in place of `once`, the public view and no selection. With a receiver of a parameter type `I` under `given {I: Iter(Item: T)}`, the evidence view, where `Item` is `T`. | The signature becomes `Fn({Once Acc Fn({Acc U8} Acc)} Acc)`. `Acc` is then inferred jointly from `initial`, from `step`, and from a written expected result when there is one, so `step` must be a `Fn({Acc U8} Acc)` for that `Acc`. In a generic caller `Acc` may be one of the caller's own parameters, fixed by its written operand types. |

A member with no receiver slot whose written result type does not contain
`Self` can be selected by neither constraint and is
`@type.undispatchableContractMember` at its declaration.

A member is called through its path: `User.nameLength(value)`,
`Printable.render(value)`. There is no inheritance between concrete types
and no overloading.

### Interface values

An interface name in type position is an interface value type: a value that
hides which conforming type it holds. A value reaches it only by widening at
a written expected type. A generic interface is written applied, and every
associated type MUST be fixed by label, since the value hides the type that
chose it: `Iter(Item: U8)`. An interface value type that leaves one open is
`@type.typeArgumentMismatch` at the type.

- **The public view.** A contract member is called through an interface
  value in its receiver slot. The call is checked against the member's
  **public view**: its signature with `Self` replaced by the interface value
  type, each parameter of the interface by the written positional argument,
  and each associated type by its fixed label. The view is what the call
  site sees and checks. It selects no implementation and is not the
  substitution any activation runs under.
- **Unboxing.** The value holds a concrete value together with the closed
  [substitution](#substitution) of the implementation selected when it was
  [widened](#widening). The call passes the held value in the receiver slot
  and runs the member under that substitution alone. Because widening
  required the positional arguments and every label to equal that
  implementation's key and choices, every part of the view that does not
  mention `Self` is the same type under both.
- **Reboxing.** Each `Self` in the member's result type stands in a position
  of the [`Self` map](#interfaces), so the activation's result holds a value
  of the held type there. The call widens each such value again to the same
  interface value type, with the same implementation. So `Iter.next` called
  through an `Iter(Item: U8)` returns `Option({U8 Iter(Item: U8)})`, and the
  remainder never shows the hidden type.
- A member with no receiver slot cannot be called through an interface
  value. A parameter outside the receiver slot whose type is or contains
  `Self` cannot be supplied through one either, since no argument is known
  to hold the same type: the call is `@type.argumentMismatch` at that
  argument.
- An interface value type implements no interface, itself included. It
  satisfies only the bound `Any`, and it never widens again.
- No pattern, test, or conversion recovers the held type. A value of type
  `Any` can only be passed along.

A signature that needs the concrete type uses a type parameter instead.

### Overlap

Two type expressions are **unifiable** when some substitution of the type
parameters in scope makes them the same type. Unification ignores bounds, and
a type expression contains no associated type unless a label fixes it, so an
overlap decision never depends on implementations written elsewhere. Two
rules require a set to be pairwise non-unifiable: the
[keys](#implementations) of one interface, each taken as its receiver
together with its positional arguments, and the source types of one
receiver's `From` and `TryFrom` implementations taken together.

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
adapter types `MappedIter(In Out)`, `FilteredIter(T)`, `SkippedIter(T)`, and
`TakenIter(T)`, widened to the interface value at the member's result. Each
adapter is an ordinary declared record that stores its source as the
interface value `Iter(Item: T)` (`Iter(Item: In)` for `MappedIter`) beside
its callback or count, with a nested `impl Iter` that chooses `Item` and
supplies `next` by calling `Iter.next` on the stored source:

```vibra
type FilteredIter(T) of {source: Iter(Item: T) keep: Fn({T} Bool)}
  visibility @public
  representation @private
  impl Iter
    type Item of T endtype
    fn next
      takes {value: Self}
      returns step: Option({Item Self})
      do
        let FilteredIter(source: inner keep: keep) be value endlet
        let Option.some(value: {item rest}) be Iter.next(inner) else
          return Option.none()
        endlet
        let remainder be FilteredIter(source: rest keep: keep) endlet
        if keep(item) then
          return Option.some(value: {item remainder})
        else
          return Iter.next(remainder)
        endif
    endfn
  endimpl
endtype
```

The default `filter` constructs `FilteredIter(source: value keep: keep)`:
the field type `Iter(Item: T)` is a written expected type, so the receiver
widens there, and the adapter's public type arguments never name the
source's concrete type. `collect` and `fold` drain the receiver; `fold`
applies `step` to the accumulator and each item in order, starting from
`initial`.

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
nested `impl` that chooses `Item` and supplies `next`. A shared algorithm
over a collection is written once, as a member of an interface or as a
generic function, and never once per collection type; `length`, `concat`,
and `slice`, whose cost depends on the representation, are static members of
each type.

## Control flow and failure

### Joins

`if` requires a `Bool` condition. When the value of an `if` or `match` is used,
the branches that can complete MUST have one common type, which is the type of
the form; branches of type `Never` are skipped, and a form all of whose branches
have type `Never` has type `Never`. When the form has no expected type, the
first branch that completes with a type fixes it, and each later branch is
checked at that type as its expected type. This is the only join the checker
computes. It never searches for an interface that covers two differing types.

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
in pattern spelling with `_` for a position only a binder can cover. A case
is spelled as its constructor pattern, so `Bool`'s is `Bool.false()`; a
record lists every field, in declaration order or, for a structural record,
canonical order, as in `{a: _ b: Bool.false()}`; and a list is spelled at
the shortest length no arm covers, as `[_ _]` after `[]` and `[_]`. An arm
no value can reach is `@pattern.unreachableArm`, relating the earliest arm
that covers it. A refutable pattern where an irrefutable one is required is
`@pattern.refutableBinding`; an irrefutable pattern in a `let … else` is
`@pattern.irrefutableLetElse`. When the checker cannot establish coverage
within its limits it reports `@pattern.unsupportedCoverage`, at the
`match` or at the `let` or parameter pattern; it never
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
expanded value. The pattern of a string, character, integer, or atom is its
literal; that of a tuple, record, or list is the tuple, record, or list
pattern of its components' patterns; and that of a construction of a case
or a declared type is the constructor pattern of its fields' patterns. A
float, `void`, or a dict has no pattern form, so a constant whose value is
or holds one is `@pattern.unsupportedCoverage` at the pattern: coverage has
no shape to expand it to.

Labelled defaults are constant expressions. Two defaults are the same when
they denote the same value. The value of a constant dict is the value its
literal evaluates to: its entries in canonical key order, where a later
duplicate key replaces the earlier value.

A default is checked at its label's type with the callable's type parameters
in it, so a default whose type mentions a type parameter is valid exactly
when it checks at that type for every instantiation, as `[]` at `List(T)`
and `Option.none()` at `Option(T)` do. Such a value holds nothing of the
parameter's type, and at each instantiation of the callable the default is
that value at the instantiated label type. A default that holds a value
where the parameter's type is required, such as `[1i32]` at `List(T)`, is
`@type.mismatch` at that value.

### `try`

`try` takes an operand of type `Option(T)` or `Result(T E)` inside a callable
whose result type is the same carrier: `Option(U)`, or `Result(U E)` with the
identical `E`. Its value has type `T`. On `none` or `err` it leaves the
innermost enclosing callable with that absence or error. Any other use is
`@type.invalidTry`. `try` never converts an error type and is always invalid
directly in a `test` body.

When a `try` expression has an expected type `T`, whatever supplies it, a
written type or the type an earlier branch fixed for a [join](#joins), its
operand is checked at the expected type `Option(T)` or `Result(T E)` of the
enclosing carrier, so a destination-dispatched operand needs no extra
annotation:

```vibra
let port: U16 be try TryFrom.convert(text) endlet
let other be if quick then 80u16 else try TryFrom.convert(text) endif endlet
```

A `try` with no expected type checks its operand with none.

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
after a statement of type `Never` is `@type.unreachableCode` as well. The
operand of `return` is the exception: `return spin(n)`, where `spin` returns
`Never`, is a tail call and is accepted, and a later statement is reported
only as after any `return`, never because of the call.

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
a `fn` whose body is certified and that is not recursive, directly or
through other functions. The primitive comparison is a call of a member of
the interfaces playing `@equatable` and `@ordered`, at a closed receiver,
whose selected implementation is declared in the embedded library. A call
whose selected implementation is a program's own `impl` is an ordinary call
of the member it selects, certified only when that member is. A recursive
`fn` is outside the fragment, and a
predicate that calls one is `@contract.unsupportedPredicate` at the call. A
certified expression cannot fault. A predicate that is
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

The checker discharges an obligation with these rules and no others. Each
has the identifier shown, which an
[obligation record](07-diagnostics-and-conformance.md#persistent-observations)
lists when the rule is used:

1. **Constants (`GUARD-1`).** The predicate over constant operands
   evaluates to `true`.
2. **Type invariant (`GUARD-2`).** A value of a type with an invariant
   supplies that type's own predicate over its representation.
3. **Dominating branch (`GUARD-3`).** Inside the true branch of an `if`,
   its condition
   holds; inside the false branch, its negation. A `match` arm supplies the
   case and field facts of its pattern. Facts hold only within the branch.
4. **Substitution (`GUARD-4`).** Renaming, immutable `let` aliases, literal
   equality, and substitution of actual arguments. Two predicates match
   only when their resolved callee, type arguments, and operands are the
   same; equal source text is not enough.
5. **Boolean structure (`GUARD-5`).** Introduction and projection of
   `Bool.and`, and the branch rules of the fixed `Bool` operations.
6. **Listed integer facts (`GUARD-6`).** The exact-width comparison facts
   the [conformance chapter](07-diagnostics-and-conformance.md#integer-facts)
   lists, such as `Ordered.greater(x y)` implying that `x` and `y` are not
   equal. The record lists `GUARD-6` and then the identifiers of the facts
   used.
7. **Certified results (`GUARD-7`).** After a call returns, the callee's
   proven `ensures` holds for the result, provided its `requires` was
   discharged.

An obligation is formed by substituting the actual arguments into the
predicate; that substitution is not a rule use. A fact the body's own
`requires` supplies is that predicate substituted at the parameters, so it
is recorded as `GUARD-4`. The negation a false branch supplies is read
through the defining relations of the
[integer facts](07-diagnostics-and-conformance.md#integer-facts): the
negation of `Ordered.greater(x y)` is x ≤ y, which uses no `INT` fact.

A discharged guard leaves no code. An obligation the rules refute is
`@contract.refuted`; one they cannot establish is `@contract.unproved`, and
the diagnostic names the predicate and the repairs: branch on the same
predicate, pass a value of a type whose invariant states it, or give the
guard an `else`. An obligation is never moved to run time, and a sampled test
never discharges one.

`@contract.unproved` is kept for an obligation the rules examined and could
not establish. When a tool limit stops the evaluation of a predicate before
the rules reach a verdict, the report is `@tool.limitExceeded` alone, at the
predicate; the obligation's status is `unknown`, and no
`@contract.unproved` is added.

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
`ensures`. A contract guard with `else` runs on every call of every
implementation, as part of the member: the checker copies it into each
implementation's checked function, so static and dynamic dispatch behave
alike. An implementation member declares no guard of its own; a `requires`,
`ensures`, or `where` section on one is `@syntax.invalidSection`, and the
checks it needs are written in its body.

## Widening

Vibra has exactly two widening relations: a concrete type widens to an
interface it conforms to, and the singleton type of an atom widens to `Atom`.
Neither is subtyping. Each applies once, at a **written expected type**,
never through a container and never from an interface value. A concrete
type widens to an applied interface value type only when it implements the
interface at those positional arguments and every label of the expected type
equals the choice of that implementation, under the implementation's
[substitution](#substitution); a differing label is `@type.mismatch` at the
widened expression. The written expected types are:

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

Both members are destination-dispatched: the expected type at the call
fixes the destination. There is no `Into`. Each `TryFrom`
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
- projection paths from a type or a type parameter to an associated type,
  and bounds on an associated type at its declaration;
- narrowing an interface value, run-time type tests;
- guards on variant types, quantified predicates, user-supplied proofs, and
  any guard checked by sampling;
- dependent types beyond the invariant and guard rules above, and an
  ownership or borrowing system;
- hashed collections and a hashing interface.
