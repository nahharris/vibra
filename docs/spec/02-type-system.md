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

`void` has the sole value `unit`. `atom` contains interned atom values such as
`@ok`; every written atom also has a singleton type that can widen to `atom`.
There is no null value, truthiness conversion, implicit numeric widening, or
implicit string conversion.

## Nominal declarations

Every `deftype` introduces a new identity. Two types with identical structure
are different unless they are the same fully resolved declaration.

V1 type constructors are:

```ebnf
type = primitive | name | "(", name, type+, ")"
     | "(", "record", field-type+, ")"
     | "(", "enum", variant+, ")"
     | "(", "newtype", type, ")"
     | "(", "tuple", type*, ")"
     | "(", "array", type, ")"
     | "(", "map", type, type, ")"
     | "(", "resource", atom-name, ")"
     | "(", "fn", "(", type*, ")", type, "effects:", effect-row, ")" ;
```

Records have closed, named fields. Enums have closed, named variants with zero
or one payload type. Tuples, arrays, and maps are immutable values. Map keys
must implement the standard `hashable`, `equatable`, and `ordered` interfaces.
Recursive types MUST pass a finite-size check; recursion through a variable-size
container is permitted, while direct infinite expansion is rejected.

Newtypes have a distinct identity and exactly one representation type. Their
constructor and unwrap operation are available only where visibility permits.
There are no structural aliases or transparent public casts.

## Namespaces and resolution

A declaration's identity is its package, module path, declaration kind, and
name. Source imports bind one explicit module alias. Wildcard imports,
re-exports, open namespaces, implicit prelude names, and filesystem-dependent
fallback resolution are forbidden.

A module has separate type, value, interface, and effect namespaces, but one
top-level form may not reuse a spelling already declared by another top-level
form in that module. Syntactic position selects the namespace. Tooling MUST
return the resolved kind and canonical identity; it must never expose a dotted
string as if textual coincidence were resolution.

Lexical shadowing is an error. A parameter, `let`, pattern binder, loop binder,
or resource binder may not reuse a visible lexical name. `_` is exempt because
it never binds. Sibling scopes may reuse a name.

## Inference and checking

Inference is local:

- literal types may be constrained by their expression context;
- generic arguments may be inferred from written argument and result types;
- private function effect rows may be inferred as specified by the effect
  chapter; and
- local expression types need not be annotated when the result is unique.

Inference MUST NOT invent a public parameter, result, generic bound, interface
implementation, numeric conversion, effect ceiling, or error conversion.
Ambiguous inference is an error with candidate explanations, not a default.

Every public function, constant, type parameter, interface member, and effect
operation has a complete written type. The checker validates a body against
that contract and never rewrites the contract from observed implementation.

## Generics

Type parameters are explicit on declarations and may have nominal interface
bounds:

```vibra
(defn first ((items (array t))) (option t)
  params: ((t type))
  where: ((t storable))
  visibility: @public
  effects: ()
  (array.first items))
```

Generic arguments are invariant. Calls infer the complete argument list when
unique; otherwise `types: (type...)` supplies every type argument in declaration
order. Partial application, named type arguments, specialization by value, and
runtime type tests are not in v1.

Implementations may monomorphize, but specialization strategy is not observable
except for deterministic output and resource budgets.

## Interfaces

`defint` declares a nominal set of method signatures over `self`. Conformance
exists only through an explicit `impl`. Matching method names and shapes are
insufficient. An interface implementation MUST provide every member exactly
once, preserve parameter and result types, and perform no effect outside the
member's declared ceiling.

There is no inheritance between concrete types. Interface values use explicit
widening at a typed boundary and static, closed-world dispatch in v1. An
implementation conflict for the same interface/type pair is an error; orphan
implementations are forbidden unless the package owns the interface or type.

Operator overloading is absent. Arithmetic, comparison, indexing, and
conversion use ordinary, resolved functions or interface methods.

## Control flow and failure

`if` requires `bool` and both branches must have one common type. `match` is
checked for exhaustiveness over booleans, atoms when statically closed, enums,
and finite structural patterns. Unreachable cases are errors.

`option t` and `result t e` are ordinary nominal standard-library enums with
compiler recognition only for `try` and unhandled-value checking. A fallible
value in discarded position is an error. `discard` is explicit intent and is
allowed, but lint may still warn when the discarded error carries a
must-handle marker.

Arithmetic is checked. Overflow, division by zero, invalid shifts, and failed
numeric conversions return typed results from their standard operations; they
do not wrap or trap implicitly. Floating-point behavior follows IEEE 754 with
canonical serialization rules defined by the runtime chapter.

## Resources

Host resources have nominal types whose `deftype` representation is
`(resource @registry-kind)`. Only a toolchain-signed standard-library module
may use this representation, and the kind must exist in the closed host
registry. A resource value:

- is introduced only by `with-resource`;
- may be passed by temporary immutable borrow to operations in that lexical
  scope;
- cannot be copied, compared, hashed, stored in another value, returned,
  captured, or bound outside the scope; and
- is closed by the runtime on all normal, propagated-error, budget, and trap
  exits.

This is a restricted lexical resource system, not general ownership or
borrowing. The checker needs only prove non-escape and scope membership. It
MUST reject use after the lexical scope and any operation on a resource owned
by a different runtime instance.

## Value semantics

Bindings and ordinary values are immutable. Passing a value preserves the
logical value for the caller. An implementation may share immutable storage or
apply copy-on-write as long as identity is not observable and budgets are
charged according to the runtime contract.

V1 has no user-visible references, pointers, object identity, destructors, or
shared mutable cells. Host resource identity is observable only through the
operations explicitly provided by its nominal resource type.
