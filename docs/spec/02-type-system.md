# Vibra v1 type system

Status: normative target
Implementation status: not started

## Model

Vibra is statically typed, nominal, and value-oriented. Every expression has
one type before execution. Public declarations are checked from their written
signatures; callers never need a callee body to type-check a call.

The primitive types are:

```text
bool void str bytes atom
int8 int16 int32 int64
uint8 uint16 uint32 uint64
float32 float64
```

`void` has exactly one value, `unit`. A function returns `void` when successful
completion carries no information. `atom` contains interned atom values such
as `@ok`; every written atom also has a singleton type that can widen to
`atom`. There is no null value, truthiness conversion, implicit numeric
widening, or implicit string conversion.

## Nominal declarations

Every `deftype` introduces a new identity. Two types with identical structure
are different unless they are the same fully resolved declaration.

V1 type constructors are:

```ebnf
type-expr = primitive | symbol | "(", symbol, type-expr+, ")"
          | "(", "record", field-type+, ")"
          | "(", "enum", variant+, ")"
          | "(", "newtype", type-expr, ")"
          | "(", "tuple", type-expr*, ")"
          | "(", "array", type-expr, ")"
          | "(", "map", type-expr, type-expr, ")"
          | function-type ;
field-type = "(", symbol, type-expr, ")" ;
variant = "(", symbol, [ type-expr ], ")" ;
function-type = "(", "fn", "(", type-expr*, ")", type-expr,
                [ "labelled:", "(", { symbol, type-expr }, ")" ],
                [ "variadic:", variadic-type ],
                [ "effects:", effect-row ], ")" ;
```

`fn` denotes a function type. `lambda`, not `fn`, declares an anonymous
function. A function type records its required positional types, labelled
names and types, optional array or map variadic type, result, and exact closed
effect row. Effect-row entries are atom entity references resolved to nominal
effect roots. Defaults belong to the function value and are not repeated in
its type. An omitted function-type `effects:` row is empty.

Records have closed, named fields. Enums have closed, named variants with zero
or one payload type. Tuples, arrays, and maps are immutable values. Map keys
must implement the standard `hashable`, `equatable`, and `ordered` interfaces.
Recursive types MUST pass a finite-size check; recursion through a
variable-size container is permitted, while direct infinite expansion is
rejected.

Newtypes have a distinct identity and exactly one representation type. Their
constructor and unwrap operation are available only where visibility permits.
There are no structural aliases or transparent public casts.

## Namespaces and resolution

A declaration's identity is its package, module path, declaration kind, and
name. Source imports bind one explicit module alias from an atom entity
reference. An atom is resolved only in a position whose grammar or data schema
expects an entity reference; it remains an ordinary `atom` value in expression
position. Wildcard imports, re-exports, open namespaces, implicit prelude
names, and filesystem-dependent fallback resolution are forbidden.

A module has separate type, value, interface, and effect namespaces, but one
top-level form may not reuse a spelling already declared by another top-level
form in that module. Syntactic position selects the namespace. Tooling MUST
return the resolved kind and canonical identity; it must never expose a dotted
string as if textual coincidence were resolution.

Name shadowing is forbidden. A named parameter, `let` binder, pattern binder,
loop binder, or lambda parameter MUST NOT reuse any visible lexical name.
`-`, `@-`, and `-:` are equivalent discards, create no binding, and may repeat
in the same or nested scopes. Sibling scopes may reuse a named symbol when
neither declaration is visible from the other.

## Inference and checking

Inference is local:

- literal types may be constrained by their expression context;
- generic arguments may be inferred from written argument and result types;
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

Every generic name is declared by one flat `where:` entry. The value paired
with the name is one nominal interface bound or `any` when the parameter is
unconstrained.

```vibra
(defn first (items (array t)) (option t)
  where: (t storable)
  visibility: @public
  (array.first items))
```

Generic arguments are invariant. Calls infer the complete argument list when
unique; otherwise `types: (type...)` supplies every type argument in `where:`
order. Partial application, named type arguments, specialization by value,
multiple bounds on one parameter, and runtime type tests are not in v1.

Implementations may monomorphize, but specialization strategy is not
observable except through deterministic program and build output.

## Interfaces and methods

`defint` declares a nominal set of interface-qualified method signatures over
`self`. Conformance is always explicit. Matching method names and shapes are
insufficient.

A type declared in the current package lists its interfaces in the enclosing
`deftype` `implements:` attribute and supplies each interface-qualified method
inside that `deftype`. A regular method in the same declaration is qualified
by the type name. The checker rejects a method with the wrong qualifier.

The package that owns an interface may implement it for a foreign type by
placing `(impl foreign-type ...)` inside the owning `defint`. The `impl` target
is positional, and its method definitions are its variadic members; `for:` is
not part of implementation syntax. These two locations encode the orphan rule
directly: an implementation can be written only where the package owns the
type or owns the interface. There is no top-level implementation form.

Within an interface contract, a `deftype` method, or a nested `impl`, `self`
resolves to the applicable receiver type. In a `deftype`, that is the declared
type. In an `impl`, it is the positional target type. `self` is not visible in
module-level functions.

For each interface/type pair, the checker MUST find every contract member
exactly once, substitute the concrete type for `self`, preserve parameter and
result types, preserve labelled names and defaults and variadic shape, and
verify that the method performs no effect outside the contract ceiling.
Missing, extra, duplicate, or conflicting members are errors.

There is no inheritance between concrete types. Interface values use explicit
widening at a typed boundary and static, closed-world dispatch in v1. Operator
overloading is absent. Arithmetic, comparison, indexing, and conversion use
ordinary resolved functions or interface methods.

## Control flow and failure

`if` requires `bool` and both branches must have one common type. `match` is
checked for exhaustiveness over booleans, atoms when statically closed, enums,
and finite structural patterns. Unreachable arms are errors.

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
