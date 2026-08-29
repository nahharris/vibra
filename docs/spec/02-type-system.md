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
union-type = "(", "union", type-expr, type-expr+, ")" ;
deftype-body = type-expr | union-type ;
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

`union-type` is admissible only as a `deftype` body, which is why the grammar
gives that position its own `deftype-body` production. A `(union ...)` form in
any other type position — a parameter, a result, a record field, a `def`
annotation, an `as` type, an `impl` target, or a `types:` argument — emits
`@type.anonymous-union`. V1 has no anonymous or structural union, so a union is
reachable only through the name its `deftype` introduces.

A union body lists at least two member types and declares no member names. The
`deftype` supplies the union's identity, and each member type's identity is its
discriminant, so a union is an enum whose variant names are its member types. A
member list shorter than two entries emits `@type.union-too-few-members`.

Members MUST be pairwise non-unifiable. Written distinctness is insufficient,
because `(union (array t) (array i32))` collides at `t = i32` and would leave
injection ambiguous after instantiation; overlap emits
`@type.union-member-overlap` at the declaration rather than after
monomorphization.

A member MUST be a concrete type expression. Another union, an interface, and a
bare generic parameter each emit `@type.union-member-not-concrete`. Unions do
not flatten: without this rule a three-way choice would have two spellings,
which the charter's decision order forbids. An interface member would likewise
leave injection ambiguous, because a member type that also implements that
interface could inject under either discriminant.

Unions widen in and narrow out through the two written forms defined later in
this chapter: a value of a member type widens to the union at a written typed
boundary, and `match` narrows a union through `as` patterns. There is no
subtyping between unions, no subset relation, and no computed least upper
bound.

A union `deftype` MAY declare nested methods and `impl` blocks exactly as any
other `deftype` does. Nothing is lifted from its members: a method, field, or
implementation common to every member is not thereby a member of the union, and
a union conforms to an interface other than `any` only by writing that
implementation. `any` is satisfied by every type without one, and the closed
`iter` registry is keyed by builtin constructor identity and so never covers a
union. A union is
a valid `(map k v)` key only when it explicitly implements `hashable`,
`equatable`, and `ordered`. Unions participate in the finite-size check on the
same terms as records and enums.

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

An enum value, union value, atom, number, or `void` is not applicable. A newtype
value does not delegate applicability to its representation, and a union value
does not delegate applicability to the member it holds. A union type is not a
constructor entity either: a member value reaches its union by widening at a
written boundary, never by applying the union. A value whose static type is
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
Selecting a written implementation from a written type is not invention: a
destination-dispatched contract member, defined in the interfaces section,
resolves its receiver from an expected type that the author wrote. Inference
MUST NOT synthesize an implementation that no package declared.
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
constructor types `(array t)`, `(map k v)`, `str`, and `(option t)` named in
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
applied targets MUST be pairwise non-unifiable, on the same terms as union
members: `(from t)` and `(from i32)` on one generic receiver are written
distinctly but collide at `t` = `i32`, and the pair emits
`@type.overlapping-implementation` at the declaration rather than producing two
candidates after instantiation. Selection uses the written operand types and the
written expected type; when two implementations of one interface remain
candidates at a call site, that site emits `@type.ambiguous-implementation`
rather than picking an order.

Dispatch normally selects an implementation from the receiver value, which a
contract member supplies by naming `self` as the type of a **fixed positional**
parameter. A variadic parameter does not qualify: an `(array self)` or
`(map k self)` tail may receive no operands at all, leaving a call with no
receiver value to select from. A labelled parameter does not qualify either,
since every labelled parameter requires a literal default.

A contract member whose `self` occurs **only** in its result type has no
receiver value and is instead **destination-dispatched**: the checker unifies
the written expected type at the call site with the member's written result
type and takes `self` from that unification. The expected type is therefore not
required to be `self` itself; an expected `(result u32 conversion-error)`
against a written result `(result self conversion-error)` yields `self` = `u32`.
When no written expected type reaches the application, the site emits
`@type.ambiguous-destination` and lists the candidate receivers; `as` is always
available to supply one. This selects a written implementation from a written
type and never synthesizes one, so the inference prohibition above is untouched.

The rule is general and is not limited to conversion. A contract member such as
`(defn empty () self)` is a factory of the same shape and is selected the same
way, from the expected type at its call site.

These two rules are exhaustive because every contract member MUST name `self` as
the type of a fixed positional parameter or within its result type. A member
doing neither is selectable by nothing and emits
`@type.contract-member-without-self` at its declaration. That covers both
`(defn version () str)`, which never mentions `self`, and
`(defn count-all () u64 variadic: (rest (array self)))`, which mentions it only
in a tail that may arrive empty. This is what `defint` declaring method
signatures **over `self`** already means; v1 adds no third selection rule and no
interface-level static member.

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

In the contract, the second `self` component is the remaining iterator value and
has the same static type as the receiver. At a concrete implementation site the
checker substitutes the concrete iterator type; for adapter values it substitutes
the adapter `deftype` or `(iter item)` after widening.

The standard-library declaration adds these default members, each with a body
that MUST match the semantics below:

| Member | Signature |
| --- | --- |
| `map` | `(value self f (fn (item) item) (iter item))` |
| `filter` | `(value self pred (fn (item) bool) (iter item))` |
| `skip` | `(value self n u64) (iter item)` |
| `take` | `(value self n u64) (iter item)` |
| `collect` | `(value self) (array item)` |

`next` is abstract. Default bodies MUST NOT be redeclared in user `impl` blocks.
At an implementation site, `self` is the concrete iterator type and `item` is that
implementation's element type. `next` returns `(option (tuple item self))`:
the second component is the remaining iterator and has the same static type as
the receiver. There is no mutating cursor.

An `(iter item)` in type position is the interface value for one element type;
it is reached by explicit widening at a typed boundary and MUST NOT be written
bare as `iter`.

Default-method semantics:

- `map` returns a `mapped-iter` value seen as `(iter item)`. Each `next` on the
  adapter calls `next` on the underlying iterator and, when an element is
  present, yields `(tuple (f x) remaining)` with `remaining` typed as
  `(iter item)`.
- `filter` returns a `filtered-iter` value seen as `(iter item)` that yields
  only elements for which `pred` returns `true`.
- `skip` returns a `skipped-iter` value seen as `(iter item)` that discards the
  first `n` elements, then forwards the rest.
- `take` returns a `taken-iter` value seen as `(iter item)` that yields at most
  `n` elements and then stops.
- `collect` eagerly drains the receiver through `next` and returns `(array item)`.

The static result type of `map`, `filter`, `skip`, and `take` is always
`(iter item)`, never the concrete receiver type.

### Standard-library adapter types

Default methods construct these public stdlib `deftype`s. Each declares
`where: (item any)` and implements `(iter item)` through a nested
`(impl (iter item) …)` block supplying only `next`. They are **not** part of the
closed registry exception above:

| Type | Role |
| --- | --- |
| `mapped-iter` | Holds `(iter item)` source and `(fn (item) item)`; lazy `map` |
| `filtered-iter` | Holds `(iter item)` source and `(fn (item) bool)`; lazy `filter` |
| `skipped-iter` | Holds `(iter item)` source and remaining skip count; lazy `skip` |
| `taken-iter` | Holds `(iter item)` source and remaining take count; lazy `take` |

Each adapter's `next` returns `(option (tuple item self))` with `self` equal to
the adapter type `(mapped-iter item)`, `(filtered-iter item)`, and so on.
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
| `(map k v)` | `(tuple k v)` | Canonical key order; each step yields one entry |
| `str` | `char` | Unicode scalar order |
| `(option t)` | `t` | On `none`, `next` returns `none`; on `some v`, one step yields `(tuple v none)` where the remaining iterator is the exhausted `none` value |

`(result t e)` does not implement `iter` in v1. Iterating a fallible value
requires an explicit `match` or conversion to `(option t)` first.

Heterogeneous tuples do not implement `iter` in v1. Users cannot add methods or
`impl` blocks to `array` or `map`. The associative `map` type MUST NOT declare a
method named `map`.

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

## Type ascription and widening

V1 has exactly three widening relations. A concrete type widens to an interface
it conforms to, a member type widens to a union that lists it, and the singleton
type of a written atom widens to `atom`.

Conformance in the first relation is exactly the conformance defined earlier in
this chapter, so widening is available through each of its sources: a written
`impl` block, the predeclared empty interface `any` that every type satisfies
without one, and the closed toolchain `iter` registry for `(array t)`,
`(map k v)`, `str`, and `(option t)`. Atom widening needs no declaration at all,
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
- a record constructor field type, or an enum or newtype constructor payload
  type;
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
- the operand widens to that type by one of the two relations above; or
- the type constrains an otherwise ambiguous inference, such as an unsuffixed
  numeric literal, an empty `array.of` or `map.of`, or a generic result.

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
(deftype celsius (newtype f64)
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

Across `from` and `try-from` together, a receiver's source targets MUST be
pairwise non-unifiable, on the same terms as its applied targets within one
interface. The same written source in both is the obvious case, and `(from t)`
with `(try-from i32)` is the same defect deferred to `t` = `i32`, where the
conversion would be both total and partial. Either pair emits
`@type.redundant-conversion` at the declaration. A conversion is one or the
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
