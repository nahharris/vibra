# Vibra v1 type system

Status: normative target
Implementation status: M2 resolves and checks the documented primitive,
binding, fixed and labelled call, and monomorphic empty-effect function subset.
Nominal/generic types and exhaustive matching remain deferred.

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

### Language core and standard library

Vibra is library-first. The compiler owns only what the language cannot build
for itself:

- the scalar types `void`, `atom`, `char`, `i8` through `i64`, `u8` through
  `u64`, `f32`, and `f64`;
- the one generic immutable sequence `(array t)`, whose indexed storage no
  combination of records and enums can express in constant time;
- `fn` types and the structural `tuple`, `record`, `enum`, and `union` type
  constructors.

Every other type, including `bool`, `str`, `bytes`, `dict`, `option`, `result`,
`ordering`, and the standard error enums, is an ordinary `deftype` in the
embedded standard library, built on that core. The compiler never depends on
such a type's definition, only on the **language role** it plays. Every
operation over a library type is standard-library Vibra, which a toolchain may
accelerate with a native implementation whose meaning is still its Vibra body
(`docs/spec/06-runtime.md`, "Native implementations"). The names in the
primitive list above that are outside the core are library types that play a
role.

A language role is a closed, compiler-known position that syntax or checking
fills with a library type:

| Role | Played by | What depends on it |
| --- | --- | --- |
| `@bool` | `bool` | `true` and `false`, `if` conditions |
| `@str` | `str` | string literals |
| `@bytes` | `bytes` | `bytes` lookups |
| `@option` | `option` | collection lookups, `try` on an optional value |
| `@result` | `result` | `try`, unhandled-failure checking |
| `@dict` | `dict` | dict variadic tails and `dict.of` |
| `@iter` | `iter` | iteration contracts |

The associative type is named `dict`, not `map`, so that it shares no spelling
with the `iter` default member `map`, which transforms items.

The standard library declares each role exactly once, with the `role:`
attribute on the playing `deftype`; `role:` is admissible only in the embedded
standard library, under the same rule as `external:`. A missing or repeated
role is a toolchain defect reported as an operational provenance diagnostic.
Adding a role is a specification change to this table.

`@std.bool` declares `(deftype bool (enum false void true void) role: @bool)`,
`@std.text` declares `(deftype str (array char) role: @str)`, and `@std.bytes`
declares `(deftype bytes (array u8) role: @bytes)`. Their constructors and
patterns are the declared ones, so `(bool.true)`, `(str scalars)`, and
`(bytes items)` build and destructure values, while a toolchain represents the
values directly under the representation latitude of the runtime chapter. An
enum body admits `true` and `false` as variant names, which is how `bool` spells
its variants.

`@std.builtin` declares `(deftype dict (array (tuple k v)) where: (k ordered v
any) role: @dict)`: a dict is its entries sorted by the `ordered.compare` of
their keys, one entry per key. `dict` is a reserved form head, so this
declaration has no written constructor or pattern. A dict is built by `dict.of`
or a dict variadic tail, which sort and merge their entries, and read by lookup
and by `dict.entries`.

The compiler-owned types and the types that play a role are the only types a
program names without an import, and their names are reserved spellings;
every other standard-library declaration is reached through an explicit
import. This closed vocabulary is not a prelude: it cannot grow without a
specification change, and no name in it can be shadowed. Until each migration lands, the toolchain
may still implement a library type directly; the roadmap names the step that
moves it into the standard library, and no program can observe the
difference.

`void` has exactly one value, also spelled `void`. A function returns `void`
when successful completion carries no information. `char` contains exactly the
Unicode scalar values. `atom` contains interned atom values such as `@ok`;
every written atom also has a singleton type that can widen to `atom`. There is
no null value, truthiness conversion, implicit numeric widening, or implicit
string conversion.

The M2 executable subset is deliberately smaller than this complete type
surface. Its checker admits primitive names, `void`, monomorphic `fn` types,
direct local-name/discard bindings, immutable `def` values, `defn`, `lambda`,
`do`, `let`, `if`, and ordinary function application. Generic bounds and
`types:` arguments, nominal bodies, collection types and constructors,
destructuring/constructor patterns, `match`, `try`, `option`, `result`,
ascription, widening, narrowing, and conversion remain valid syntax but are
outside that profile and produce `@tool.unavailable` when semantic support is
requested. This is an implementation capability boundary, not a second source
dialect; the full rules below remain the v1 authority for later milestones.

M3 widens the profile in two stages. The Stage 3A profile adds `deftype`
declarations of every body form with their constructors, projections, and
nested non-interface methods; anonymous tuple, record, enum, and union types
with `tupleof`, `recordof`, and `enumof`; arrays and dicts with `array.of`,
`dict.of`, lookups, and variadic array and dict slots;
every pattern form with `match` and shared irrefutability; `option`, `result`,
`try`, and unhandled-failure checking; union and atom-singleton widening, `as`
ascription, and `as` narrowing; and generics whose every `where:` bound is the
predeclared `any`, with applied types, inference, and `types:`. Until the
Stage 3B profile, `defint`, nested `impl` blocks, a `where:` bound naming an
interface other than `any`, `any` or another interface written as a type, a
user-declared dict type whose key type is a generic parameter, and the
conversion interfaces remain `@tool.unavailable`. Stage 3B admits the remainder
of this chapter. Each behavior step moves forms from unavailable to supported;
none reclassifies a valid form as malformed.

M2 function declarations and `fn` types have no variadic slot. A variadic
parameter declaration or variadic function type is valid v1 syntax but is
outside the M2 profile and produces `@tool.unavailable`. Applications of a
variadic signature are likewise unavailable, including calls with no tail
operands. Extra operands supplied to a fixed signature remain ordinary
argument-binding errors; they do not make that fixed signature variadic.

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

## Overlap and non-unifiability

Several rules in this chapter require a written set of types to be free of
overlap. Written distinctness never satisfies them, because a generic name can
make two written types equal at some instantiation: `(array t)` and
`(array i32)` are different spellings of one type when `t` is `i32`.

Two type expressions are **unifiable** when some substitution of the generic
names in scope makes them the same fully resolved type. A set is **pairwise
non-unifiable** when no two of its members are unifiable.

Unification is **bound-agnostic**: a substitution need not satisfy the generic's
declared interface bound. Where `t` is bound by `printable`, `(array t)` and
`(array i32)` overlap even if `i32` does not implement `printable` today. This
is deliberately the conservative reading. Under the bound-respecting
alternative, a package that later writes `(impl i32 ...)` inside `printable`
would turn a distant, already-accepted declaration into an overlapping one, so
whether a union or an implementation set is legal would depend on implementation
decisions made in packages its author cannot see. Bound-agnostic unification
keeps every overlap decision local to the declaration that makes it, at the cost
of rejecting some sets that are provably disjoint under today's bounds; those
are written with distinct types instead.

Every rule below that forbids overlap requires pairwise non-unifiability, and
each reports it at the declaration rather than after monomorphization, so an
accepted set is unambiguous at every instantiation. Three rules use it: union
members, the applied interface targets of one receiver, and the source targets
of one receiver's `from` and `try-from` implementations taken together. Each
names its own diagnostic.

## Nominal declarations

Every `deftype` introduces a new identity. Two declared types with identical
structure are different unless they are the same fully resolved declaration.
A `deftype` whose body is a structural type expression, such as
`(deftype pair (tuple i32 str))`, is therefore a new type distinct from that
structure. There are no transparent aliases and no implicit conversion between
a declared type and its body.

V1 type constructors are:

```ebnf
type-expr = primitive | type-name | "(", type-name, type-expr+, ")"
          | tuple-type | record-type | enum-type | union-type
          | function-type ;
deftype-body = type-expr | intrinsic-type ;
tuple-type = "(", "tuple", type-expr*, ")" ;
record-type = "(", "record", local-name, type-expr,
              { local-name, type-expr }, ")" ;
enum-type = "(", "enum", local-name, type-expr,
            { local-name, type-expr }, ")" ;
union-type = "(", "union", type-expr, type-expr+, ")" ;
intrinsic-type = "(", "intrinsic-type", atom, ")" ;
type-name = symbol - reserved-type-head ;
reserved-type-head = "tuple" | "record" | "enum" | "union" | "intrinsic-type"
                   | "fn" ;
function-type = "(", "fn", "(", type-expr*, ")", type-expr,
                [ "labelled:", "(", { local-name, type-expr }, ")" ],
                [ "variadic:", variadic-type ],
                [ "effects:", effect-row ], ")" ;
```

`fn` denotes a function type. `lambda`, not `fn`, declares an anonymous
function. A function type records its required positional types, labelled
names and types, optional array or dict variadic type, result, and exact closed
effect row. Effect-row entries are lexical symbols resolved in the effect
namespace to nominal roots. Defaults belong to the function value and are not
repeated in its type. An omitted function-type `effects:` row is empty.

`tuple`, `record`, `enum`, and `union` take a variable number of arguments, so
they are type constructor forms rather than generic types. Each is an ordinary
type expression and MUST be accepted in every type position: a parameter, a
result, a record field, an enum payload, a union member, a `def` annotation, an
`as` type, and a `types:` argument. `array` and `dict` take a fixed number of
arguments, so they are ordinary generic builtin types, and `(array t)` and
`(dict k v)` are ordinary applied types. An applied type supplies exactly the
complete generic parameter list of its head, and a bare generic head is an
application with no arguments; any other count is
`@type.type-argument-mismatch`.

Record and enum types are flat and contain at least one name/type pair whose
names are pairwise distinct; a repeated name emits `@name.member-collision`.
Records have closed, named fields. Every enum variant has one written payload
slot; `void` in that slot declares a nullary, payloadless variant, while any
other type declares a unary one. A generic payload slot instantiated to `void`
is nullary in that instantiation: `(maybe.some)` constructs it, a zero-operand
application fixes the slot's generic argument to `void`, and a `void` operand
is rejected like any operand of a nullary variant. Tuples, arrays, dicts,
records, enums, and unions are immutable values.

A structural type written outside a `deftype` body is anonymous and its
identity is its structure. Two anonymous tuple types are the same type when
they have the same arity and equal component types in order. Two anonymous
record types are the same when they have the same set of field names with
equal types, two anonymous enum types when they have the same set of variant
names with equal payload types, and two anonymous union types when they have
the same member set, in each case regardless of written order. Their canonical
order, which fixes discriminants, rendering, and key order, sorts record fields
and enum variants by the UTF-8 bytes of their names and union members by the
bytes of their canonical type encoding. The formatter works on syntax alone, so
it rewrites an anonymous type with fields and variants sorted by name and union
members sorted by the whitespace-normalized text of their canonical spelling;
an anonymous type containing a comment keeps its written order. A declared
record, enum, or union keeps its declaration order, because its identity is
the declaration.

An anonymous type has no owner. It declares no methods, receives no `impl`
block, and conforms to no interface other than `any` and the closed registries
below. It cannot refer to itself; recursion needs a `deftype` name. Recursive
declared types MUST pass a finite-size check; recursion through a
variable-size container (an array or a dict) or through a function type, whose
values do not embed the type, is permitted, while direct infinite expansion is
rejected with `@type.infinite-size` at the `deftype` whose expansion first
repeats in declaration order, relating the member or payload through which it
repeats.

Dict keys must implement the standard `ordered` interface. A dict is ordered by
its keys, so `compare` alone decides both where a key goes and whether two keys
are the same key; v1 has no hashed collection and therefore no `hashable`
interface. `@std.core` declares the two comparison contracts as ordinary nominal
interfaces:

```vibra
(defint equatable
  visibility: @public
  (defn equal (left self right self) bool))

(defint ordered
  visibility: @public
  (defn compare (left self right self) ordering))
```

An `ordered` implementation MUST be a total order, and two keys are the same key
exactly when `compare` answers `equal`; an `equatable` implementation of the
same type MUST agree with it. Stated as laws over all values `a`, `b`, and `c`
of the implementing type:

- **`equatable`.** `(equal a a)` is `true`; `(equal a b)` is `(equal b a)`; and
  `(equal a b)` with `(equal b c)` gives `(equal a c)`.
- **`ordered`.** `(compare a a)` is `equal`; `(compare a b)` is `less` exactly
  when `(compare b a)` is `greater`; and `(compare a b)` with `(compare b c)`
  both `less`, or both `equal`, gives `(compare a c)` the same answer.
- **Agreement.** For a type implementing both, `(equal a b)` is `true` exactly
  when `(compare a b)` is `equal`.

Every member is pure, so the same operands always give the same answer. The
toolchain cannot check these laws. A program whose implementation breaks them
still evaluates deterministically, but which entry of a dict a lookup finds is
then not defined by this specification. V1 declares no `hashable`, so it has no
law. The following types receive closed toolchain
conformance to both, keyed by type identity in the same way as the closed
`iter` registry; they are admissible dict keys without a written
implementation:

- `bool`, `char`, `str`, `bytes`, `atom`, and every atom singleton type;
- `i8` through `i64` and `u8` through `u64`; and
- an anonymous tuple, record, enum, or union type whose every component,
  field, payload, or member type is itself an admissible key.

The standard library writes these conformances as ordinary implementations:
`equatable` and `ordered` carry them for the builtin integers and `char`, and
`bool`, `str`, and `bytes` implement both where they are declared. The closed
registry is their native implementation, so it MUST answer as they do. The
atom types have no declaration to carry one, and the rule for anonymous types
is the language's own structural rule; both conform through the registry
alone. The list is closed so that no other package can add to it.

`void`, `f32`, `f64`, `fn` types, arrays, dicts, and options are not admissible
keys. A `deftype` is admissible only through its own written `ordered`
implementation. An inadmissible key type in any written or inferred
`(dict k v)` emits `@type.invalid-dict-key` at the key type expression, or at the
constructor application when the dict type is inferred; an `fn` key keeps the
more specific `@type.function-not-equatable`.

Canonical key order is a total order over admissible key values and is the
only order in which a dict is traversed, rendered, or iterated. For the closed
key types it is: `false` before `true`; numeric order for integers; Unicode
scalar value for `char`; lexicographic by scalar for `str` and by byte for
`bytes`; lexicographic by the UTF-8 bytes of the canonical spelling for atoms;
component-wise lexicographic for tuples and, in canonical field order, for
records; canonical variant order and then payload for enums; and canonical
member order and then value for unions. A `deftype` key uses its `ordered`
implementation. Hash-table order is never observable.

A `deftype` whose body is any type expression other than a structural `tuple`,
`record`, `enum`, or `union` form — a primitive, an applied or declared type,
an array, a dict, or a function type — declares a **wrapper type**: a distinct
identity over exactly one representation type. `(deftype celsius f64)` is not
`f64`, and nothing converts between them implicitly. There is no separate
`newtype` form, because every `deftype` already introduces an identity. A
wrapper's constructor and its unwrapping constructor pattern are available only
where visibility permits.

The compiler-owned types of the language core are declared by the toolchain in
its embedded standard-library modules with an `intrinsic-type` body, which
binds the declaration to a closed registry of builtin type identities:
`(deftype i32 (intrinsic-type @i32) …)`. The atom MUST name a registry entry
whose spelling is the declaration's own name, otherwise the declaration emits
`@external.unknown-symbol`. Such a declaration supplies the builtin type's
static methods as ordinary nested members, which are reached through the type
path with no import, exactly as the builtin type needs none. `intrinsic-type`
is admissible only in the toolchain-embedded package, under the same rule as
`external:`; users cannot declare or extend a builtin type.

A library type that plays a role is an ordinary `deftype` with a `role:`
attribute instead, never an `intrinsic-type`. Its static methods and
constructors are reached through its type path in the same way, such as
`(dict.of k v)` and `(option.some value)`, and users cannot extend it either.

The applied form `(symbol type-expr+)` would otherwise read `(record …)` or
`(union …)` as the application of a type with that name, so the head of an
applied type is `type-name`, written as the exception
`symbol - reserved-type-head`: any symbol that is not one of those six
spellings. This is the grammar's only use of exception notation. Reserved type
forms are recognized before the applied-type production, exactly as reserved
expression forms are recognized before application.

The reservation reaches exactly the declarations that must be usable as a bare
`type-name` head: a `deftype`, a `defint`, and a generic name in a `where:`
clause. One of those spelled with a reserved type head, or with the name of a
builtin type outside an `intrinsic-type` declaration, emits
`@name.reserved-declaration`. It reaches no other namespace and no member,
because a member is only ever reached through a qualified path and is never a
bare type head. A nested method named `dict` therefore stays legal exactly as
the source-language chapter states. The separate value-namespace rule on
builtin type names keeps its own `@name.reserved-value-spelling`. A generic name spelled `self`,
which names the receiver type, or `any` also emits `@name.reserved-declaration`.

A union type lists at least two member types and declares no member names. A
declared union's identity is its `deftype`, an anonymous union's is its member
set, and each member type's identity is its discriminant, so a union is an enum
whose variant names are its member types. A member list shorter than two
entries emits `@type.union-too-few-members`.

Members MUST be pairwise non-unifiable, as the overlap section defines.
Otherwise `(union (array t) (array i32))` would leave injection ambiguous at
`t` = `i32`. Overlap emits `@type.union-member-overlap` at the union type
expression.

A member MUST be a concrete type expression. Another union type, whether
declared or anonymous, an interface, and a bare generic parameter each emit
`@type.union-member-not-concrete`. Unions do not flatten: without this rule a
three-way choice would have two spellings, which the charter's decision order
forbids. An interface member would likewise leave injection ambiguous, because
a member type that also implements that interface could inject under either
discriminant.

A member of an anonymous union MUST NOT name a generic parameter anywhere,
as in `(union (tuple t i32) str)`; it emits `@type.union-member-not-concrete`.
An anonymous union's discriminants follow its canonical member order, which
would differ between a generic declaration and each of its instantiations. A
declared union keeps its written member order, so its members may name its
own generic parameters.

Unions widen in and narrow out through the written forms defined later in this
chapter: a value of a member type widens to the union at a written typed
boundary, a declared union's name applied to one member value injects it, and
`match` narrows a union through `as` patterns. There is no subtyping between
unions, no subset relation, and no computed least upper bound.

A union `deftype` MAY declare nested methods and `impl` blocks exactly as any
other `deftype` does. Nothing is lifted from its members: a method, field, or
implementation common to every member is not thereby a member of the union, and
a declared union conforms to an interface other than `any` only by writing that
implementation. `any` is satisfied by every type without one, and the closed
`iter` registry is keyed by builtin constructor identity and so never covers a
union. A declared union is a valid `(dict k v)` key only when it explicitly
implements `ordered`. Unions participate in the
finite-size check on the same terms as records and enums.

```vibra
(deftype number (union i32 f32)
  visibility: @public)
```

## Application

Every non-reserved executable list is an application whose behavior is fixed
by the statically resolved callee. V1 has this closed set of applicable value
categories:

| Callee type | Required operand | Result | Application kind |
| --- | --- | --- | --- |
| `fn` | Its written positional, labelled, and variadic signature | Written result | `@function` |
| Tuple type, declared or anonymous | One tuple-index literal | Exact selected component | `@tuple-projection` |
| Record type, declared or anonymous | One atom field selector | Exact selected field | `@record-projection` |
| `(array t)` | One `u64` index | `(option t)` | `@collection-lookup` |
| `(dict k v)` | One value of exact type `k` | `(option v)` | `@collection-lookup` |
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

Array, dict, string, and byte lookups accept a runtime operand and return the
standard nominal `option`; absence and out-of-bounds access never trap, return
an implicit default, or produce null. String indices count Unicode scalar
values, not UTF-8 bytes. Projection and lookup are pure. Evaluating their
callee or operand may perform effects, but the application itself contributes
no effect and no function-call edge.

An enum value, union value, atom, number, or `void` is not applicable. A wrapper
value does not delegate applicability to its representation, and a union value
does not delegate applicability to the member it holds. A value whose static
type is an unconstrained generic is not applicable; v1 has no callable
interface or user-defined applicability bound. Tuples and records are not
subtypes of `fn`; producing an accessor as a higher-order value requires an
explicit `lambda`.

A declared type is also an applicable constructor entity, and its application
is pure and has kind `@constructor`:

| Declaration body | Constructor | Operands |
| --- | --- | --- |
| record | `(z a: f b: g)` | Its closed set of labelled fields |
| tuple | `(z f g h)` | One positional operand per component, in order |
| enum | `(z.a f)` | Zero operands for a `void` payload, else one of the payload type |
| union | `(z f)` | One operand whose type is exactly one member; this injects it |
| any other type expression (a wrapper) | `(z f)` | One operand of the representation type |

A union constructor operand that could inject under more than one member, such
as an unsuffixed literal, emits `@type.ambiguous-inference`; one whose type is
no member is `@type.argument-mismatch`.

Anonymous structural values are built by three reserved expression forms,
recognized before the general application production:

```ebnf
anonymous-value = "(", "tupleof", { expr }, ")"
                | "(", "recordof", label, expr, { label, expr }, ")"
                | "(", "enumof", label, expr, ")" ;
```

`(tupleof e…)` has the anonymous tuple type of its operand types in order.
`(recordof a: e …)` has the anonymous record type of its fields, whose labels
MUST be distinct. `(enumof a: e)` selects one variant and so cannot determine
the rest of its type: it MUST be checked against a written expected type that
is an anonymous enum declaring variant `a`, exactly as `as` supplies one, and
otherwise emits `@type.ambiguous-inference`; a `void` payload is written
`(enumof a: void)`. An anonymous union has no constructor: a member value
reaches it by widening at a written boundary or through `as`. All three forms
are pure, have kind `@constructor`, evaluate their operands from left to right,
and are mirrored by the patterns `(tupleof p…)`, `(recordof a: p …)`, which
may omit fields, and `(enumof a: p)`.

Array and dict values are built by static methods of the builtin `array` and
`dict` types, reached by path exactly as a `deftype`'s nested method is. They
are ordinary toolchain-declared members, not special syntax, and their
applications are ordinary `@function` applications:

```vibra
(defn of () (array t)
  variadic: (items (array t)))
```

`array.of` has that signature with the type's `where: (t any)`, and `dict.of`
the corresponding one with `variadic: (entries (dict k v))`, so each is a
first-class `fn` value, its element types come from ordinary generic
inference, an empty call needs an expected type like any uninferable generic
result, and an odd dict tail is an ordinary variadic-binding error. The key rule
applies to each instantiated `(dict k v)`. Users cannot declare further members
on a builtin type. The type forms `(tuple …)`, `(record …)`, `(enum …)`, and
`(union …)` and the array pattern `(array …)` are never value constructors.

## Namespaces and resolution

A declaration's identity is its package provenance, unit, module path, owner
path, declaration kind, and name. Ordinary local package provenance is the
package name and exact version from the project record. The embedded
standard library instead takes its fixed package name and exact version from
its manifest; project data cannot supply or override either value. A resolver
MUST preserve those fields in the identity; source order, a vector position,
and spelling alone are not identities. Source imports bind
one explicit module alias from an atom entity reference. An atom is resolved
only in a position whose grammar or data schema expects an entity reference;
it remains an ordinary `atom` value in expression
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
because every declaration name is one unqualified segment. Two entity kinds are
not named in that tree at all: the interfaces section defines the type-keyed
identity of an `impl` block and of each of its members.

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

Name shadowing is forbidden. A repeated top-level declaration, import alias,
or lexical binding emits `@name.redeclaration` at the later introduction and
relates the earlier introduction. Each later introduction emits exactly one
such diagnostic. A lexical binding's primary span is its binder name, not the
enclosing parameter, labelled entry, or `let` form. The related span is the
nearest earlier visible introduction's binder: the innermost enclosing lexical
binder, including one a lambda captures, else the module-level declaration.
The shadowing binder still binds for the rest of its scope, so a further
repetition relates it instead. Members of one owner's flat namespace use
`@name.member-collision` instead. Every name introduced anywhere inside a
positional-parameter, `let`, or `match` pattern MUST NOT reuse any visible
lexical name. Labelled and variadic parameter names follow the same rule. A
pattern cannot introduce the same name twice. `-`, `@-`, and `-:` are
equivalent discards, create no binding, and may repeat in the same or nested
scopes. Sibling scopes may reuse a named symbol when neither declaration is
visible from the other.

A module-level `def`, `defn`, or import alias MUST NOT be spelled as a builtin
type name: a primitive type, `array`, `dict`, or `tuple`. Builtin types own
static methods reached by dotted path, so such an alias or value would make
`i32.add-checked` or `array.of` ambiguous. A top-level use of one of those
spellings as a value or alias emits `@name.reserved-value-spelling`.

## Functions as values

A resolved module-level `defn` path or nested method path in expression position
has a `fn` type and is a first-class function value. Application is `(path …)`
with the receiver or operands required by that signature. Constructors,
projections, lookups, and enum tags are not `fn` values.

A contract member named as a value is instantiated from its written expected
`fn` type, exactly as a generic function is: that type fixes the receiver, and
calling the value selects the implementation as a call written at that
receiver would. Without a written `fn` type it is `@type.ambiguous-inference`.
A bounded generic function or `lambda` named as a value has each bound checked
at the argument its expected type fixes.

`fn` values are not `equatable` and MUST NOT be used as a `(dict k v)` key. Using
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
Selecting a written implementation from a written type is not invention: a
destination-dispatched contract member, defined in the interfaces section,
resolves its receiver from an expected type that the author wrote. Inference
MUST NOT synthesize an implementation that no package declared.
Ambiguous inference is an error with candidate explanations, not a default.
When no unique type follows for an unsuffixed numeric literal, an empty
`array.of` or `dict.of`, or a generic argument, the checker emits
`@type.ambiguous-inference` at that literal, application, or generic
application, with one note per candidate or missing constraint. A
destination-dispatched call with no written expected type keeps the more
specific `@type.ambiguous-destination`.

An operand that does not fit the parameter or constructor slot it binds to, a
wrong arity, an unknown, duplicate, or missing label or record field, an odd
`dict.of` operand count, and `array.of` or `dict.of` operands with no single
element type are `@type.argument-mismatch` at the operand or application.
Every other disagreement between an expression's type and its written or
required expected type — a `def` annotation, a result
type, an `if` condition, differing branch or arm types, a pattern and its
scrutinee — is `@type.mismatch` at the expression, relating the written type
when it has a source span.

Every public function, `def`, type parameter, interface member, and effect
operation has a complete written type. The checker validates a body against
that contract and never rewrites the contract from observed implementation.

## Generics

Every generic name is declared by one flat `where:` entry on a `deftype`,
`defint`, `defn`, nested method, or `lambda`. The value paired with the name is
one nominal interface bound; the predeclared empty interface `any` is the bound
that constrains nothing.

```vibra
(defn first (items (array t)) (option t)
  where: (t any)
  visibility: @public
  (array.first items))
```

Generic arguments are invariant. Function and constructor applications infer
the complete argument list when unique; otherwise `types: (type...)` supplies
every type argument in `where:` order. Partial application, named type
arguments, specialization by value, multiple bounds on one parameter, and
runtime type tests are not in v1.

A bound is one name, so it cannot apply a generic interface: `where: (t iter)`
emits `@type.type-argument-mismatch`. A parameter typed as the interface value,
such as `(values (iter item))`, takes that role, and a generic `item` there is
inferred from the one way the operand conforms.

Every type argument of an application MUST implement the bound of its
parameter: a concrete type through an implementation of that interface, and a
generic name of the enclosing declaration through the same bound. A violation,
like a contract member applied to a receiver whose type implements none of its
interface, is `@type.unsatisfied-bound`.

A nested method sees the generic names of its enclosing `deftype` and declares
only additional ones in its own `where:`. Redeclaring an inherited name is
`@name.generic-redeclaration`, so each generic name still has exactly one
declaration site. An `impl` block is not a binding site: it has no `where:`
clause, so an `impl` nested in a `deftype` passes that type's names through
unchanged, and the target of an `impl` nested in a `defint` MUST be a closed
type expression. A free generic name in an `impl` target is
`@name.unknown-symbol`, and generic implementations are a post-v1 concern.

A `lambda` sees every generic name of its enclosing declarations and lambdas,
and its own `where:` declares only additional ones under the same
redeclaration rule. A generic `lambda` is generic like a generic `defn`: bound
by `let`, the binding stays generic wherever it is visible, including through
a capture, and each application infers or takes through `types:` its own
complete argument list, in the lambda's `where:` order. A generic `lambda`
applied directly is one such application. Anywhere else, a generic `lambda` or
a generic binding is instantiated from its written expected `fn` type, exactly
as a generic function named as a value is, and emits
`@type.ambiguous-inference` when that type does not fix every argument.
Function types themselves are never generic.

```vibra
(let pick (lambda (left t right t) t
            where: (t any)
            left)
  (pick "x" (pick types: (str) "y" "z")))
```

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
constructor types `(array t)`, `(dict k v)`, `str`, and `(option t)` named in
the iteration section. Those implementations are keyed by constructor identity
in a closed registry; they are not user `impl` blocks and not generic `defint`
implementations. Standard-library iterator adapters are ordinary `deftype`s
with explicit nested `impl (iter item)` blocks instead. Every other interface
still requires an explicit `impl`.

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

A contract member is called through an interface value by its receiver, and the
implementation is the one for the type the value holds. Where the member's
signature names `self` in its result, the result is the same interface value
type. A member that names `self` in another parameter cannot be called through
an interface value, because the value erases the type those operands must
share; the call emits `@type.mismatch` at that operand. The same holds for an
abstract and a default member alike. A member whose result takes `self` as an
operand of a function type cannot be called through an interface value either,
because the returned function would accept any value of the interface; the
call emits `@type.mismatch`. An interface value type
implements no interface, itself included, so it never satisfies a generic
bound and never widens again.

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
interface. A `defint` target MUST also be a declared type or a builtin
type: an anonymous structural type has no owner to carry an implementation, so
writing one as a target emits `@name.wrong-entity-kind`.

These two locations encode the orphan rule directly: an implementation can be
written only where the package owns the type or owns the interface.

An `impl` block inside a `defint` MUST NOT target a type declared in the same
module, and a same-module target emits `@type.redundant-implementation`. Within
one module a type and an interface always see each other without an import, so
the `deftype` placement is always available there and the `defint` placement
would be a second spelling of one implementation. Across modules both placements
remain legal, because requiring the `deftype` placement there could demand an
import that completes a cycle and leave the implementation unwritable.

An implementation has no name in any declaration tree: it is keyed by types
rather than by a spelling, and a block and a member are keyed differently. An
`impl` **block**'s identity is the pair of its applied interface target and its
receiver type; the block has no corresponding contract member, so no member
belongs in its key. An implementation **member**'s identity is the triple of the
corresponding contract member, that applied target, and that receiver type, much
as an effect operation is identified by its root and operation.

The applied target belongs in both keys rather than being a detail of either. A
receiver carrying both `(from i16)` and `(from i8)` has two blocks and two
`convert` members; without the target each pair would collapse to one identity.
These are the only v1 entity kinds that no single atom addresses, and they need
no atom: an implementation is never named at a use site, because dispatch
selects it from the receiver or from a written expected type.

A generic interface is keyed by its applied type, so one receiver MAY implement
`(from i16)` and `(from i8)` as two implementations of one `defint`. The
applied targets MUST be pairwise non-unifiable, as the overlap section defines.
Otherwise `(from t)` and `(from i32)` on one generic receiver would produce two
candidates at `t` = `i32`. Overlap emits `@type.overlapping-implementation`.
Selection uses the written operand types and the
written expected type; when two implementations of one interface remain
candidates at a call site, that site emits `@type.ambiguous-implementation`
rather than picking an order.

Dispatch normally selects an implementation from the receiver value, which a
contract member supplies by naming `self` as the type of a **fixed positional**
parameter. A variadic parameter does not qualify: an `(array self)` or
`(dict k self)` tail may receive no operands at all, leaving a call with no
receiver value to select from. A labelled parameter does not qualify either,
since every labelled parameter requires a literal default.

A contract member with no fixed positional `self` parameter but with `self`
somewhere in its result type has no receiver value and is instead
**destination-dispatched**. The classification turns on the absence of a fixed
receiver rather than on `self` appearing nowhere else, so a member naming `self`
in both its result and a variadic tail is destination-dispatched too, and the
tail is an ordinary operand.

The checker unifies the written expected type at the call site with the member's
written result type and takes `self` from that unification. The expected type is
therefore not required to be `self` itself; an expected
`(result u32 conversion-error)` against a written result
`(result self conversion-error)` yields `self` = `u32`. When no written expected
type reaches the application, the site emits `@type.ambiguous-destination` and
lists the candidate receivers; `as` is always available to supply one. This
selects a written implementation from a written type and never synthesizes one,
so the inference prohibition above is untouched.

The rule is general and is not limited to conversion. A contract member such as
`(defn empty () self)` is a factory of the same shape and is selected the same
way, from the expected type at its call site.

The two rules partition every contract member, because each member either has a
fixed positional `self` parameter or does not. A member with no such parameter
and no `self` in its result is selectable by neither rule and emits
`@type.undispatchable-contract-member` at its declaration. The code names the
condition rather than the absence of the spelling, because a rejected member may
still contain `self`: it covers `(defn version () str)`, which never mentions
`self`, and equally `(defn count-all () u64 variadic: (rest (array self)))`,
which mentions it only in a tail that may arrive empty. This is what `defint`
declaring method signatures **over `self`** already means; v1 adds no third
selection rule and no interface-level static member.

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

Completeness is checked once per block identity, not once per nominal interface,
so `(from i16)` and `(from i8)` on one receiver are checked separately. For each
applied-interface-target and receiver-type pair, the checker MUST find every
**abstract** contract member exactly once, substitute the concrete type for
`self`, preserve parameter and result types, preserve labelled names and
defaults and variadic shape, and verify that the method performs no effect
outside the contract ceiling.

A contract member with a body is a **default method**. An implementation MUST
supply every abstract member and MUST NOT redeclare a default member; doing so
emits `@type.default-override`. A missing abstract member emits
`@type.missing-abstract-member`. Extra, duplicate, or conflicting members are
errors.

Default methods are checked against the interface contract ceiling. Their bodies
are available to every conforming type without being written again in each
`impl`. The compiler MAY specialize a default body when `self` is known; that
specialization is not a second source method and is not overridable.

There is no inheritance between concrete types. Interface values use explicit
widening at a typed boundary and static, closed-world dispatch in v1. Operator
overloading is absent. Arithmetic, comparison, and conversion use ordinary
resolved functions or interface methods. Application-based tuple/record
projection and array/dict/string/byte lookup are the closed indexing surface
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

In the contract, the second `self` component is the remaining iterator value and
has the same static type as the receiver. At a concrete implementation site the
checker substitutes the concrete iterator type; for adapter values it substitutes
the adapter `deftype` or `(iter item)` after widening.

The standard-library declaration adds these default members, each with a body
that MUST match the semantics below:

| Member | Parameters | Result | Member generics |
| --- | --- | --- | --- |
| `map` | `(value self f (fn (item) out))` | `(iter out)` | `where: (out any)` |
| `filter` | `(value self pred (fn (item) bool))` | `(iter item)` | — |
| `skip` | `(value self n u64)` | `(iter item)` | — |
| `take` | `(value self n u64)` | `(iter item)` | — |
| `collect` | `(value self)` | `(array item)` | — |

`map` is the one member with its own generic parameter, `out`, the element type
it produces; a callback that keeps the element type instantiates `out` as
`item`.

`next` is abstract. Default bodies MUST NOT be redeclared in user `impl` blocks.
At an implementation site, `self` is the concrete iterator type and `item` is that
implementation's element type. `next` returns `(option (tuple item self))`:
the second component is the remaining iterator and has the same static type as
the receiver. There is no mutating cursor.

An `(iter item)` in type position is the interface value for one element type;
it is reached by explicit widening at a typed boundary and MUST NOT be written
bare as `iter`.

Default-method semantics:

- `map` returns a `mapped-iter` value seen as `(iter out)`. Each `next` on the
  adapter calls `next` on the underlying iterator and, when an element is
  present, yields `(tuple (f x) remaining)` with `remaining` typed as
  `(iter out)`.
- `filter` returns a `filtered-iter` value seen as `(iter item)` that yields
  only elements for which `pred` returns `true`.
- `skip` returns a `skipped-iter` value seen as `(iter item)` that discards the
  first `n` elements, then forwards the rest.
- `take` returns a `taken-iter` value seen as `(iter item)` that yields at most
  `n` elements and then stops.
- `collect` eagerly drains the receiver through `next` and returns `(array item)`.

The static result type of `map` is always `(iter out)`, and of `filter`, `skip`,
and `take` always `(iter item)`, never the concrete receiver type.

An `iter` implementation and the defaults obey these laws, over a finite
iterator `xs` unless stated:

- **`next` is a function of its iterator.** Applying `next` again to the same
  iterator value gives the same answer, for any `xs`, so an iterator a program
  kept can be walked again.
- **`collect` is the walk.** `(collect xs)` is the items of successive `next`
  steps, in order.
- **`map` preserves structure.** `(collect (map xs f))` is `f` applied to each
  item of `(collect xs)` in order, so mapping the identity changes nothing and
  mapping `f` then `g` equals mapping their composition.
- **`filter` selects in order.** `(collect (filter xs keep))` is the items of
  `(collect xs)` that `keep` accepts, in their original order.
- **`take` and `skip` split.** `(collect (take xs n))` followed by
  `(collect (skip xs n))` is `(collect xs)`.
- **The adapters are lazy.** `map`, `filter`, `skip`, and `take` call `next` on
  their source only when `next` is called on them, so `take` over an iterator
  that never ends is finite.

### Standard-library adapter types

Default methods construct these public stdlib `deftype`s. `mapped-iter` declares
`where: (item any out any)` and implements `(iter out)`; each other adapter
declares `where: (item any)` and implements `(iter item)`. Each implementation
is a nested `impl` block supplying only `next`. They are **not** part of the
closed registry exception above:

| Type | Role |
| --- | --- |
| `mapped-iter` | Holds `(iter item)` source and `(fn (item) out)`; lazy `map` |
| `filtered-iter` | Holds `(iter item)` source and `(fn (item) bool)`; lazy `filter` |
| `skipped-iter` | Holds `(iter item)` source and remaining skip count; lazy `skip` |
| `taken-iter` | Holds `(iter item)` source and remaining take count; lazy `take` |

Each adapter's `next` returns `(option (tuple item self))` with `self` equal to
the adapter type `(mapped-iter item out)`, `(filtered-iter item)`, and so on,
whose element is `out` for `mapped-iter` and `item` for the others.
Widening to `(iter item)` happens at the default-method result boundary.

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
| `(dict k v)` | `(tuple k v)` | Canonical key order; each step yields one entry |
| `str` | `char` | Unicode scalar order |
| `(option t)` | `t` | On `none`, `next` returns `none`; on `some v`, one step yields `(tuple v none)` where the remaining iterator is the exhausted `none` value |

`(result t e)` does not implement `iter` in v1. Iterating a fallible value
requires an explicit `match` or conversion to `(option t)` first.

Heterogeneous tuples do not implement `iter` in v1. Users cannot add methods or
`impl` blocks to `array` or `dict`. The associative `dict` type MUST NOT declare a
method named `dict`.

User `deftype`s MAY implement `(iter item)` with a nested `(impl (iter item) …)`
block supplying only `next`. The owner MUST declare `item` in its `where:`
clause, and the `impl` target MUST spell the full application `(iter item)`;
bare `iter` is invalid. The `next` member MUST name that same `item` in its
result type. Generic `impl` targets such as `(array t)` inside a `defint`
remain post-v1.

## Control flow and failure

`if` requires `bool` and both branches must have one common type. `match` is
checked for exhaustiveness over booleans, atoms when statically closed, enums,
unions, and finite structural patterns. Unreachable arms are errors. One common
type means one written or already-identical type; the checker MUST NOT search
for a union or interface that covers two differing branch types.

A scrutinee of type `atom` is never statically closed: its atom-literal arms
are refutable and a binder or discard must cover the remainder. A scrutinee of
an atom singleton type is closed by the one arm for that atom. Literal patterns
are admitted for every primitive except `f32`, `f64`, and `void`, and an arm
set of literals covers its type only for `bool`; any other literal arm set
needs a covering binder or discard. The arm set of `str`, `bytes`, and every
numeric type is therefore never closed by literals alone.

A `match` whose arms do not cover the scrutinee type emits
`@pattern.non-exhaustive` at the complete `match` form, with one note naming
one uncovered value shape in canonical pattern spelling, chosen as the first
uncovered shape in declaration order of variants, members, and fields. In that
shape, a position that only a binder or discard can cover (a value of an
infinite space such as `str`, `atom`, or a number) is spelled `-`, as is a
record, tuple, or wrapper whose every component is `-`. A refutable binding
pattern's `@pattern.refutable-binding` carries the same kind of note. An arm
that no value can reach because earlier arms cover it emits
`@pattern.unreachable-arm` at that arm's pattern, relating the earliest arm
that alone covers it when one exists. Arms are examined in source order, so
only the later of two identical arms is unreachable.

`try` applies to an operand of type `(option t)` or `(result t e)` inside a
function, `lambda`, or test whose written result type is the same standard
container: `(option u)` for an `option` operand, or `(result u e)` with the
identical error type `e` for a `result` operand. The success type `u` of the
enclosing result need not equal `t`. The `try` expression has type `t`. On
`none` or `err`, evaluation leaves the innermost enclosing function, `lambda`,
or test body with that `none` or that error wrapped in the enclosing result
type. A `try` in any other context, over any other type, or against a
different container or error type emits `@type.invalid-try` at the `try` form,
relating the enclosing written result type when there is one. A test body has
result type `void`, so `try` is always invalid there.

A value whose static type is `(result t e)` is fallible. A fallible value is
ignored when it is evaluated in a non-final element of a function, `lambda`,
test, `do`, or `let` body; such an expression emits
`@type.unhandled-fallible` at the expression unless it is written as
`(let - expression)` or with another discard spelling. `option` is not
fallible: absence is a value, and ignoring one needs no discard.

An `(as type-expr pattern)` pattern narrows a union. Its scrutinee MUST have a
union type, and a scrutinee of any other type emits `@type.narrowing-non-union`.
Its written type MUST be one member of that union under the same identity used
for the discriminant; any other type emits `@type.not-a-union-member`. The arm
binds the payload at the member type, not at the union type. A union `match` is
exhaustive when every member has an arm or when a binder or discard covers the
remainder.

Because a union has at least two members, an `as` pattern can never cover every
value of its expected type and is therefore always refutable. It is valid in
`match` and invalid in `let`, in a fixed positional function parameter, and in a
lambda parameter, where it emits the existing `@pattern.refutable-binding`.

The same exhaustiveness engine determines whether a binding pattern is
irrefutable: the single pattern MUST cover every value of its expected type.
`let` and fixed positional function or lambda parameters require an irrefutable
pattern; `match` permits refutable patterns and checks all arms together.
Tuple patterns, anonymous or declared, have exact arity and are irrefutable
when every component is. Record patterns may omit fields and are irrefutable
when every written field pattern is. A fixed-length array pattern is refutable for the
variable-length array type. A wrapper constructor pattern is irrefutable when
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

## Type ascription and widening

V1 has exactly three widening relations. A concrete type widens to an interface
it conforms to, a member type widens to a union that lists it, and the singleton
type of a written atom widens to `atom`.

Conformance in the first relation is exactly the conformance defined earlier in
this chapter, so widening is available through each of its sources: a written
`impl` block, the predeclared empty interface `any` that every type satisfies
without one, and the closed toolchain `iter` registry for `(array t)`,
`(dict k v)`, `str`, and `(option t)`. Atom widening needs no declaration at all,
because the singleton types and `atom` are builtin, but it obeys the same
boundary rule as the other two: `(array.of @ok @err)` has no single element
type and is an error, while `(as (array atom) (array.of @ok @err))` supplies
one.

No relation is subtyping: each applies at a boundary, not structurally and not
through a container, and none is ever inferred from a type's shape. Generic
arguments remain invariant, so `(array i32)` does not widen to `(array number)`
and `(option i32)` does not widen to `(option number)`.

Widening fires only against a **written expected type**. The complete set of
written expected types is:

- a fixed positional, labelled, or variadic parameter type;
- a written result type;
- a `def` type annotation;
- a declared record or tuple field type, a declared enum payload type, or a
  wrapper representation type at its constructor, and the payload type of an
  `enumof` variant in the written anonymous enum it is checked against;
- a type supplied through `types:`; and
- the type written in an `as` expression.

Where no expected type is written, no widening occurs. The checker MUST NOT
compute a least upper bound: two `if` or `match` branches typed `i32` and `f32`
are an error unless an enclosing boundary writes a union containing both. This
preserves the rule that inference invents nothing, because a union is only ever
the type an author wrote.

Widening applies at most once at a boundary and does not chain. Reaching an
interface from a union member requires the union itself to implement that
interface. Widening is pure: it contributes no effect and no function-call edge,
exactly as projection and lookup do.

### Ascription

`(as type-expr expr)` checks its operand at the written type and is the general
way to write a typed boundary where no declaration supplies one. It admits
exactly three outcomes:

- the operand already has that exact type, which is a legal no-op;
- the operand widens to that type by one of the three relations above, so
  `(as atom @ok)` is admitted; or
- the type constrains an otherwise ambiguous inference, such as an unsuffixed
  numeric literal, an empty `array.of` or `dict.of`, or a generic result.

Anything else emits `@type.invalid-ascription`. In particular, ascription never
requests a conversion and never narrows:

```vibra
(as number 1i32)          ; widening: i32 is a member of number
(as (array u32) (array.of))  ; constrains an empty collection
(as u32 (from.convert 42u8))  ; names a conversion destination
(as str "Hello world")    ; legal no-op

(as i64 3i32)             ; error: no implicit numeric widening
(as i32 some-number)      ; error: as never narrows a union
```

Ascription is static. It has no runtime representation, performs no check, and
carries no cost; the runtime chapter defines it as an erased form. A redundant
ascription is valid and MUST NOT emit a style diagnostic, because writing the
expected type is a legitimate way to state intent locally.

## Conversion

Conversion between unrelated types is always an ordinary call and never a
widening. The standard library declares two nominal interfaces, both generic
over the source type and both implemented on the destination:

```vibra
(defint from
  where: (source any)
  visibility: @public
  (defn convert (value source) self
    effects: ()))

(defint try-from
  where: (source any)
  visibility: @public
  (defn convert (value source) (result self conversion-error)
    effects: ()))
```

`self` is the destination in both contracts and occurs only in the result type,
so both members are destination-dispatched under the general rule in the
interfaces section: the
call site's written expected type unifies with that result type and fixes the
destination. Both contracts declare `effects: ()`, so every conversion is pure.

```vibra
(deftype celsius f64
  visibility: @public
  (impl (from f64)
    (defn convert (value f64) self
      (celsius value))))

(as celsius (from.convert 21.5f64))
```

A written result type supplies the destination just as well, which is the usual
spelling for `try-from`:

```vibra
(defn parse-port (text str) (result u32 conversion-error)
  visibility: @public
  (try-from.convert text))
```

Placing the implementation on the destination is what makes the orphan rule
work. Because the receiver is the destination, a package converting a foreign
type into its own type owns the receiver and may write the `impl` in its own
`deftype`. The reverse spelling, an `into` interface dispatching on the source,
would leave exactly that case unwritable, and v1 has no blanket implementations
with which to derive one direction from the other. V1 therefore declares `from`
and `try-from` only; there is no `into`.

`conversion-error` is a public standard-library enum with a closed variant set:

```vibra
(deftype conversion-error
  (enum out-of-range void
        invalid-format void
        unrepresentable void)
  visibility: @public)
```

The error type is fixed rather than chosen per implementation, because v1 has no
associated types on an interface contract. A conversion needing a richer error
is an ordinary `defn` returning `(result t e)`, which requires no new machinery.
Per-implementation error types are a post-v1 concern recorded in the roadmap.

The builtin integer types conform to both contracts through a closed toolchain
registry, keyed by the pair of source and destination types, as the closed
`iter` registry is keyed by constructor identity. For two distinct integer
types, the destination implements `(from source)` exactly when it holds every
value of the source, and `(try-from source)` otherwise, with `out-of-range` for
a value it cannot hold. The registry is the `to-U` family of the runtime
chapter, and a call selects its operation directly. An unsuffixed integer
literal fits more than one source and is therefore
`@type.ambiguous-implementation`. The floating-point types have no registry
conversion in v1.

Across `from` and `try-from` together, a receiver's source targets MUST be
pairwise non-unifiable, as the overlap section defines. The same written source
in both is the obvious case; `(from t)` with `(try-from i32)` is the same defect
at `t` = `i32`, where the conversion would be both total and partial. Either
pair emits `@type.redundant-conversion`. A conversion is one or the
other, and offering both spellings would give one idea two canonical forms.

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
