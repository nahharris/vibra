# Vibra v1 type system

Status: normative target
Implementation status: not started

## Model

Vibra is statically typed, nominal, value-oriented, and expression-oriented.
Every expression has one type before execution. Public declarations are checked
from their written signatures; callers never need a callee body to type-check
a call.

The primitive types are:

```text
bool void char str bytes atom
i8 i16 i32 i64
u8 u16 u32 u64
f32 f64
```

`void` has exactly one value, also spelled `void`. A function returns `void`
when successful completion carries no information. `char` contains exactly the
Unicode scalar values. `atom` contains interned atom values such as `@ok`;
every written atom also has a singleton type that can widen to `atom`. There is
no null value, truthiness conversion, implicit numeric widening, or implicit
string conversion.

A numeric suffix is a complete type annotation on its literal. Integer
suffixes select one of `i8` through `i64` or `u8` through `u64`; float suffixes
select `f32` or `f64`. A suffixed literal MUST fit the selected type. An
unsuffixed integer or float is constrained by its local expected type and is an
ambiguity error when no unique numeric type follows. Suffixes never request an
implicit conversion, and platform-sized numeric types do not exist in v1.

For width `N`, `iN` contains the integers from `-2^(N-1)` through
`2^(N-1)-1`, and `uN` contains `0` through `2^N-1`. `f32` and `f64` are the
IEEE 754 binary32 and binary64 formats. Decimal float literals are rounded to
the selected format using round-to-nearest, ties-to-even; a finite source
literal that overflows to infinity is out of range. V1 has no source spelling
for infinity or NaN. Character equality and ordering use Unicode scalar value,
and conversion between `char` and an integer is always explicit.

## Nominal declarations

Every `deftype` introduces a new identity. Two types with identical structure
are different unless they are the same fully resolved declaration.

V1 type constructors are:

```ebnf
type-expr = primitive | symbol | "(", symbol, type-expr+, ")"
          | record-type | enum-type
          | "(", "newtype", type-expr, ")"
          | "(", "tuple", type-expr*, ")"
          | "(", "array", type-expr, ")"
          | "(", "map", type-expr, type-expr, ")"
          | function-type ;
record-type = "(", "record", local-name, type-expr,
              { local-name, type-expr }, ")" ;
enum-type = "(", "enum", local-name, type-expr,
            { local-name, type-expr }, ")" ;
function-type = "(", "fn", "(", type-expr*, ")", type-expr,
                [ "labelled:", "(", { local-name, type-expr }, ")" ],
                [ "variadic:", variadic-type ],
                [ "effects:", effect-row ], ")" ;
```

`fn` denotes a function type. `lambda`, not `fn`, declares an anonymous
function. A function type records its required positional types, labelled
names and types, optional array or map variadic type, result, and exact closed
effect row. Effect-row entries are lexical symbols resolved in the effect
namespace to nominal roots. Defaults belong to the function value and are not
repeated in its type. An omitted function-type `effects:` row is empty.

Record and enum bodies are flat and contain at least one name/type pair.
Records have closed, named fields. Every enum variant has one written payload
slot; `void` in that slot declares a nullary, payloadless constructor, while
any other type declares a unary constructor. Tuples, arrays, and maps are
immutable values. Map keys must implement the standard `hashable`,
`equatable`, and `ordered` interfaces. Recursive types MUST pass a finite-size
check; recursion through a variable-size container is permitted, while direct
infinite expansion is rejected.

Newtypes have a distinct identity and exactly one representation type. Their
constructor and unwrap operation are available only where visibility permits.
There are no structural aliases or transparent public casts.

## Application

Every non-reserved executable list is an application whose behavior is fixed
by the statically resolved callee. V1 has this closed set of applicable value
categories:

| Callee type | Required operand | Result | Application kind |
| --- | --- | --- | --- |
| `fn` | Its written positional, labelled, and variadic signature | Written result | `@function` |
| `(tuple t0 ... tn)` | One tuple-index literal | Exact selected component | `@tuple-projection` |
| Record type | One atom field selector | Exact selected field | `@record-projection` |
| `(array t)` | One `u64` index | `(option t)` | `@collection-lookup` |
| `(map k v)` | One value of exact type `k` | `(option v)` | `@collection-lookup` |
| `str` | One `u64` scalar index | `(option char)` | `@collection-lookup` |
| `bytes` | One `u64` byte index | `(option u8)` | `@collection-lookup` |

A projection or lookup application accepts exactly one unlabelled operand and
no labelled or variadic operands.

A tuple index is an unsuffixed decimal integer literal written canonically
without a leading zero except for `0`. It is checked at compile time, MUST be
within the tuple arity, and cannot be supplied by a variable or computed
expression. V1 has no `value.0` postfix spelling. A record selector is exactly
one written unqualified atom such as `@name`; it is resolved contextually to a
field identity and MUST name a visible field of the statically known record
type. In this position the atom is a selector, not an entity reference or an
applicable value.

Array, map, string, and byte lookups accept a runtime operand and return the
standard nominal `option`; absence and out-of-bounds access never trap, return
an implicit default, or produce null. String indices count Unicode scalar
values, not UTF-8 bytes. Projection and lookup are pure. Evaluating their
callee or operand may perform effects, but the application itself contributes
no effect and no function-call edge.

An enum value, atom, number, or `void` is not applicable. A newtype value does
not delegate applicability to its representation. A value whose static type is
an unconstrained generic is not applicable; v1 has no callable interface or
user-defined applicability bound. Tuples and records are not subtypes of `fn`;
producing an accessor as a higher-order value requires an explicit `lambda`.

Resolved nominal record types, enum variants, and newtype constructors are
also applicable constructor entities. Record constructors accept their closed
set of labelled fields, enum constructors accept zero operands for a `void`
payload slot or one operand of the written payload type, and newtype
constructors accept their representation value. Constructor application is
pure and has kind `@constructor`.

The closed native entities `tuple.of`, `array.of`, and `map.of` construct
collection values and have application kind `@constructor`. `tuple.of` derives
one heterogeneous tuple type from its operands. `array.of` requires every
operand to have one exact element type, and `map.of` requires alternating
operands of one exact key type and one exact value type. Empty array and map
construction requires an expected type; an odd map arity is an error. The
source forms `(tuple ...)`, `(array ...)`, and `(map ...)` are type or pattern
forms, never collection value constructors.

## Namespaces and resolution

A declaration's identity is its package, module path, declaration kind, and
name. Source imports bind one explicit module alias from an atom entity
reference. An atom is resolved only in a position whose grammar or data schema
expects an entity reference; it remains an ordinary `atom` value in expression
position. Wildcard imports, re-exports, open namespaces, implicit prelude
names, and filesystem-dependent fallback resolution are forbidden.

Token spelling never heuristically selects entity resolution. In source,
symbols name lexical code entities and the surrounding grammar selects the
type, value, interface, or effect namespace. Type position is the one exception
and selects the type and interface namespaces together, as the interfaces
section defines. Atoms are values unless a closed
source grammar position, such as the module locator in `import`, explicitly
requires an entity reference. In `.vibon`, the typed data schema makes the same
choice field by field. Resolution converts an entity-reference token to one
canonical identity before type checking; it does not turn atoms into
first-class modules, effects, diagnostics, or declarations.

A module has separate type, value, interface, and effect namespaces, but one
top-level form may not reuse a spelling already declared by another top-level
form in that module. Syntactic position selects the namespace for a symbol
reference, and that flat top-level spelling space is what keeps the one
two-namespace position deterministic. An atom path needs no such selection:
each component likewise resolves to at most one declaration. Tooling MUST
return the resolved kind and canonical identity; it must never expose a dotted
string as if textual coincidence were resolution.

Every code entity named in a module's declaration tree has exactly one canonical
atom path, and every atom path resolves to at most one entity. A path is
`@unit.c1...cn`: its first component names a unit, the programs-and-packages
chapter defines the walk from that unit's root to one module, and the components
remaining after that module resolve against its declarations. A module-level
form takes one component, and a member of one takes one further component. Thus
`@app.m.user` is a type, `@app.m.user.name-length` is one of its methods, and
`@app.m.fs.read.file` is an effect operation.

Paths are built from the ownership tree, never from a declaration's spelling,
because every declaration name is one unqualified segment. One entity kind is
not named in that tree at all: the interfaces section defines the pair identity
of an implementation and its members.

The addressable members of one owner form a single flat namespace covering
record fields, enum variants, methods, and effect operations. Their names MUST
be pairwise distinct within that owner, so a field and a method cannot share a
spelling; a collision emits `@name.member-collision`. Interface implementations
contribute no name to this namespace.

A slot that expects an entity reference decides only whether an atom is a
reference and which entity kind the resolved entity must have. It never decides
how the path is read, so one spelling denotes one entity in every position. A
path resolving to an entity of the wrong kind emits `@name.wrong-entity-kind`
and names the entity it found, rather than reporting the path as unknown.

Name shadowing is forbidden. Every name introduced anywhere inside a
positional-parameter, `let`, or `match` pattern MUST NOT reuse any visible
lexical name. Labelled and variadic parameter names follow the same rule. A
pattern cannot introduce the same name twice. `-`, `@-`, and `-:` are
equivalent discards, create no binding, and may repeat in the same or nested
scopes. Sibling scopes may reuse a named symbol when neither declaration is
visible from the other.

A module-level `def`, `defn`, or import alias MUST NOT be spelled `map`,
`array`, or `tuple`. A top-level use of one of those spellings as a value or
alias emits `@name.reserved-value-spelling`.

## Functions as values

A resolved module-level `defn` path or nested method path in expression position
has a `fn` type and is a first-class function value. Application is `(path …)`
with the receiver or operands required by that signature. Constructors,
projections, lookups, and enum tags are not `fn` values.

`fn` values are not `equatable` and MUST NOT be used as a `(map k v)` key. Using
one as a key emits `@type.function-not-equatable`.

A module-level `defn` MAY refer to itself and to other module-level `defn`s in
the same module by name. Same-module mutual recursion among module-level
`defn`s is valid, and forward reference within a module is permitted. A
`lambda` has no self-name and MUST NOT refer to itself; it MAY appear in mutual
recursion only as the callee of a named module-level `defn`. A call in tail
position to a function in the same module's recursive group MUST NOT consume
additional language-level stack; the runtime chapter defines tail position and
the recursive group. Exhaustion of a host stack limit on non-tail recursion is
not a portable Vibra semantic result.

## Inference and checking

Inference is local:

- unsuffixed numeric literal types may be constrained by their expression
  context, while character, boolean, string, atom, `void`, and suffixed numeric
  literals have fixed types;
- generic arguments may be inferred from written operand and result types;
- effects performed by a function body are computed to check its written or
  default-empty ceiling; and
- local expression types need not be annotated when the result is unique.

Inference MUST NOT invent a parameter, result, generic bound, interface
implementation, numeric conversion, effect ceiling, or error conversion.
Ambiguous inference is an error with candidate explanations, not a default.

Every public function, `def`, type parameter, interface member, and effect
operation has a complete written type. The checker validates a body against
that contract and never rewrites the contract from observed implementation.

## Generics

Every generic name is declared by one flat `where:` entry on a `deftype`,
`defint`, `defn`, or nested method. The value paired with the name is one
nominal interface bound; the predeclared empty interface `any` is the bound
that constrains nothing.

```vibra
(defn first (items (array t)) (option t)
  where: (t storable)
  visibility: @public
  (array.first items))
```

Generic arguments are invariant. Function and constructor applications infer
the complete argument list when unique; otherwise `types: (type...)` supplies
every type argument in `where:` order. Partial application, named type
arguments, specialization by value, multiple bounds on one parameter, and
runtime type tests are not in v1.

A nested method sees the generic names of its enclosing `deftype` and declares
only additional ones in its own `where:`. Redeclaring an inherited name is
`@name.generic-redeclaration`, so each generic name still has exactly one
declaration site. An `impl` block is not a binding site: it has no `where:`
clause, so an `impl` nested in a `deftype` passes that type's names through
unchanged, and the target of an `impl` nested in a `defint` MUST be a closed
type expression. A free generic name in an `impl` target is
`@name.unknown-symbol`, and generic implementations are a post-v1 concern.

The complete type-argument list of a `deftype` method is its type's parameters
in declaration order followed by the method's own, and `types:` supplies that
whole list:

```vibra
(deftype ring (record items (array t) head u64)
  where: (t any)
  visibility: @public
  (defn empty () (ring t)
    visibility: @public
    (ring items: (array.of) head: 0u64)))
```

```vibra
(ring.empty types: (str))
```

`types:` is a reserved call-site label. A declaration MUST NOT introduce a
labelled parameter named `types`, which would otherwise make the label
ambiguous between a type-argument list and an ordinary labelled operand; such a
declaration emits `@name.reserved-label`.

`types:` is always defined by the entity the call site addresses, never by the
entity dispatch selects. A call through an interface contract member therefore
supplies the contract's parameters, and the receiver's own generic names stay
lexical: they scope an implementation body and are fixed by unification with the
receiver, never written at a call site. Implementations of one contract may
belong to owners of different generic arity, so an implementation member has no
`types:` contract of its own.

`types:` is written among an application's labelled operands but is not one: it
is neither an operand of the callee nor visible to its body, and it has no
declaration-order slot. Canonical form places it before every ordinary labelled
operand. The recovery parser accepts it in any unambiguous position, the
formatter moves it, and a noncanonical position is `@style.argument-order`
rather than a compile error.

A `types:` list whose length differs from the complete parameter list is
`@type.type-argument-mismatch` rather than a partial application. Supplying
`types:` where inference already succeeds is permitted and checked for
agreement; a supplied argument that contradicts the inferred one emits the same
code.

Implementations may monomorphize, but specialization strategy is not
observable except through deterministic program and build output.

## Interfaces and methods

`defint` declares a nominal set of method signatures over `self`. Conformance is
always explicit. Matching method names and shapes are insufficient.

A symbol in type position resolves across the type and interface namespaces
together, and resolving to an interface denotes an interface value reached by
explicit widening at a typed boundary. This union is deterministic rather than a
namespace ambiguity: one top-level form may not reuse a spelling already
declared by another in the same module, so a name is a type or an interface and
never both. It is the one position whose grammar admits two namespaces, and it
admits them because interface values would otherwise be unspellable. A symbol
resolving there to a value or an effect root remains `@name.wrong-entity-kind`.

`any` is the predeclared empty interface. It declares no contract member, and no
package may declare or shadow the spelling, which emits
`@name.reserved-declaration`. It is an interface in every other respect and
takes no special case in the grammar: it is a generic bound wherever a bound is
written and a type expression wherever a type is written, exactly as any
declared interface is.

Every type satisfies `any` without writing an implementation. This is the single
exception to explicit conformance, and it is vacuous rather than structural: the
contract has no member, so nothing is inferred from a type's shape and the rule
that matching names and shapes are insufficient is untouched. No other interface
acquires an implementation implicitly, and the exception is fixed to this one
predeclared name rather than extended to any empty interface a package might
declare.

The second exception is closed toolchain `iter` conformance for the builtin
constructor and standard-library types named in the iteration section. Those
implementations are keyed by constructor identity in a closed registry; they are
not user `impl` blocks and not generic `defint` implementations. Every other
interface still requires an explicit `impl`.

The exception needs no rule barring a written implementation for `any`, because
`interface-implementation` requires at least one member and an empty contract
admits none: every candidate member is an extra member, which is already an
error. `(impl any ...)` is therefore unwritable by construction rather than by
prohibition.

The two positions carry opposite information and are not interchangeable. A
generic parameter bound by `any` keeps its concrete type at every instantiation.
An `any` in type position is an interface value that erases which type it holds,
and because the contract is empty and v1 has no runtime type test, such a value
can only be passed along, never inspected. A signature that needs the concrete
type MUST use a generic parameter.

Every `defn` name is one unqualified segment in its owner's scope, because the
enclosing form already names that owner. A declaration therefore never spells
its own path prefix, and no import alias can enter a declaration name.

An implementation is written as an `impl` block whose positional target supplies
whichever half of the interface/type pair its parent does not: inside a
`deftype` the target is the interface, and inside a `defint` it is the receiver
type. Its members are unqualified and its target is resolved to a canonical
identity, so an implementation is never spelled through an alias. `for:` is not
part of implementation syntax, there is no top-level implementation form, and
there is no `implements:` attribute — the `impl` blocks of a `deftype` are the
list of interfaces it implements.

Each target slot requires exactly one entity kind, which the grammar's shared
`type-expr` does not enforce now that a type expression may name an interface. A
`deftype` target MUST resolve to an interface and a `defint` target MUST resolve
to a concrete type; the other kind emits `@name.wrong-entity-kind`. A receiver
is therefore never an interface value, so an implementation is always selected
from a concrete type at a widening boundary rather than layered on another
interface.

These two locations encode the orphan rule directly: an implementation can be
written only where the package owns the type or owns the interface.

An `impl` block inside a `defint` MUST NOT target a type declared in the same
module, and a same-module target emits `@type.redundant-implementation`. Within
one module a type and an interface always see each other without an import, so
the `deftype` placement is always available there and the `defint` placement
would be a second spelling of one implementation. Across modules both placements
remain legal, because requiring the `deftype` placement there could demand an
import that completes a cycle and leave the implementation unwritable.

An implementation has no name in any declaration tree: it is keyed by an
interface and a receiver type rather than by a spelling. Its identity, and the
identity of each of its members, is therefore the pair of the corresponding
contract member and the receiver type, exactly as an effect operation is
identified by its root and operation. This is the only v1 entity kind that no
single atom addresses, and it needs no atom: an implementation is never named at
a use site, because dispatch selects it from the receiver.

Within an interface contract, a `deftype` method, or an `impl` block, `self`
resolves to the applicable receiver type. In a `deftype` and in an `impl` nested
in one, that is the declared type. In an `impl` nested in a `defint`, it is the
positional target type. `self` is not visible in module-level functions.

A method is called through a dotted path naming the entity, never through a
receiver-first syntax: `(user.name-length value)` applies the `name-length`
method of type `user`, and `(printable.render value)` applies the `printable`
contract member, with dispatch selecting the implementation from the receiver.
Vibra has no method-call operator and no implicit receiver; a method application
is an ordinary application whose callee is a resolved path.

For each interface/type pair, the checker MUST find every **abstract** contract
member exactly once, substitute the concrete type for `self`, preserve
parameter and result types, preserve labelled names and defaults and variadic
shape, and verify that the method performs no effect outside the contract
ceiling. A contract member with a body is a **default method**. An
implementation MUST supply every abstract member and MUST NOT redeclare a
default member; doing so emits `@type.default-override`. A missing abstract
member emits `@type.missing-abstract-member`. Extra, duplicate, or conflicting
members are errors.

Default methods are checked against the interface contract ceiling. Their bodies
are available to every conforming type without being written again in each
`impl`. The compiler MAY specialize a default body when `self` is known; that
specialization is not a second source method and is not overridable.

There is no inheritance between concrete types. Interface values use explicit
widening at a typed boundary and static, closed-world dispatch in v1. Operator
overloading is absent. Arithmetic, comparison, and conversion use ordinary
resolved functions or interface methods. Application-based tuple/record
projection and array/map/string/byte lookup are the closed indexing surface
defined above and cannot be overloaded.

## Iteration

Pure collection iteration uses the standard `iter` interface. Effectful walks
use recursive module-level functions over `(iter.next it)` with an explicit
written effect ceiling. There is no separate loop or foreach form.

### Canonical `iter` contract

`iter` is a generic interface over one item type:

```vibra
(defint iter
  where: (item any)
  visibility: @public
  (defn next (value self) (option (tuple item self))))
```

The standard-library declaration adds these default members, each with a body
that MUST match the semantics below:

| Member | Signature |
| --- | --- |
| `map` | `(value self f (fn x item) item) self` |
| `filter` | `(value self pred (fn x item) bool) self` |
| `skip` | `(value self n u64) self` |
| `take` | `(value self n u64) self` |
| `collect` | `(value self) (array item)` |

`next` is abstract. Default bodies MUST NOT be redeclared in user `impl` blocks.
At an
implementation site, `self` is the concrete iterator type and `item` is that
implementation's element type. `next` returns `(option (tuple item self))`:
`item` is the yielded element and `self` is the remaining iterator value. There
is no mutating cursor.

Default-method semantics:

- `map` returns a lazy adapter. Each `next` on the adapter calls `next` on the
  receiver and, when an element is present, yields `(tuple (f x) remaining)`.
- `filter` returns a lazy adapter that yields only elements for which `pred`
  returns `true`, calling `next` on the receiver as needed.
- `skip` returns a lazy adapter that discards the first `n` elements of the
  receiver, then forwards subsequent elements unchanged.
- `take` returns a lazy adapter that yields at most `n` elements and then
  returns `none` on further `next` calls even if the receiver continues.
- `collect` eagerly drains the receiver through `next` and returns `(array item)`.

All defaults are pure and their callback parameters MUST have `effects: ()`.

Call shape matches every other method: receiver first.

```vibra
(iter.map xs f)
(iter.filter xs pred)
(iter.skip xs n)
(iter.take xs n)
(iter.collect xs)
```

### Closed builtin conformance

The following types receive closed toolchain `iter` conformance from the registry
keyed by constructor identity:

| Type | `item` | `next` behavior |
| --- | --- | --- |
| `(array t)` | `t` | Index order from `0`; remaining is the suffix not yet yielded |
| `(map k v)` | `(tuple k v)` | Canonical key order; each step yields one entry |
| `str` | `char` | Unicode scalar order |
| `(option t)` | `t` | `none` yields `none`; `some v` yields one `(tuple v none)` |
| `(result t e)` | `t` | `err _` yields `none`; `ok v` yields one `(tuple v none)` |

Heterogeneous tuples do not implement `iter` in v1. Users cannot add methods or
`impl` blocks to `array` or `map`. The associative `map` type MUST NOT declare a
method named `map`.

User `deftype`s MAY implement `iter` with a nested `impl iter` block supplying
only `next`. The checker infers `item` from that member's result type. Generic
`impl` targets such as `(array t)` inside a `defint` remain post-v1.

## Control flow and failure

`if` requires `bool` and both branches must have one common type. `match` is
checked for exhaustiveness over booleans, atoms when statically closed, enums,
and finite structural patterns. Unreachable arms are errors.

The same exhaustiveness engine determines whether a binding pattern is
irrefutable: the single pattern MUST cover every value of its expected type.
`let` and fixed positional function or lambda parameters require an irrefutable
pattern; `match` permits refutable patterns and checks all arms together. Tuple patterns have exact arity and are irrefutable when every
component is. Record patterns may omit fields and are irrefutable when every
written field pattern is. A fixed-length array pattern is refutable for the
variable-length array type. A newtype constructor pattern is irrefutable when
its payload pattern is. An enum constructor pattern is refutable unless its
expected enum has exactly that one variant and its payload pattern is
irrefutable. A bare unqualified name always binds; it never pins or compares a
visible value.

`option t` and `result t e` are ordinary nominal standard-library enums with
compiler recognition only for `try` and unhandled-value checking. A fallible
value in an ignored position is an error unless intent is explicit as
`(let - expression)` or another discard spelling. Tooling MAY still emit a
contract warning when an explicitly ignored error carries a must-handle marker.

Arithmetic is checked. Overflow, division by zero, invalid shifts, and failed
numeric conversions return typed results from their standard operations; they
do not wrap or trap implicitly. Floating-point behavior follows IEEE 754 with
canonical serialization rules defined by the runtime chapter.

## Host values

V1 host operations accept and return ordinary Vibra values. The language has
no `resource` type constructor, lexical host-handle scope, user-visible close
protocol, or ownership rule for host objects. APIs that would require a
long-lived file, socket, or stream handle are deferred; the v1 filesystem and
console APIs are value-in/value-out operations.

## Value semantics

Bindings and ordinary values are immutable. Passing a value preserves the
logical value for the caller. An implementation may share immutable storage or
apply copy-on-write as long as identity is not observable.

V1 has no user-visible references, pointers, object identity, destructors, or
shared mutable cells.
