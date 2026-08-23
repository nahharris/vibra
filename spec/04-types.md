# 4. Types

Status: draft

Vibra's type system is nominal, static, and total: every accepted expression has
exactly one type, known at compile time, and there is no dynamic type, no `any`
value, and no escape hatch.

## Type expressions

```ebnf
type-expr = symbol
          | atom-literal
          | "self"
          | "(", symbol, type-expr, { type-expr }, ")"
          | "(", "record", field-type, { field-type }, ")"
          | "(", "tuple", type-expr, type-expr, { type-expr }, ")"
          | "(", "enum", enum-tag, { enum-tag }, ")"
          | "(", "array", type-expr, ")"
          | "(", "map", type-expr, type-expr, ")"
          | "(", "newtype", type-expr, ")"
          | "(", "handle", atom-literal, ")"
          | "(", "fn-type", "(", { type-expr }, ")", type-expr, ")"
          | "(", "mut", type-expr, ")"
          | "(", "ref", type-expr, ")" ;

field-type = "(", symbol, type-expr, ")" ;
enum-tag   = "(", symbol, type-expr, ")" ;
```

A bare symbol in type position is a type reference. `(constructor arg ...)` in
type position is generic application — the same list shape as everywhere else,
with no separate instantiation head. Built-in heads (`record`, `tuple`, `enum`,
`array`, `map`, `newtype`, `handle`, `fn-type`, `mut`, `ref`) are recognized
before user type constructors.

A generic type constructor referenced without its arguments is `E-TYPE-001`.
Wrong arity is `E-TYPE-002`.

### Primitives

| Group | Types |
| --- | --- |
| Signed integers | `int8` `int16` `int32` `int64` |
| Unsigned integers | `uint8` `uint16` `uint32` `uint64` |
| Floating point | `float32` `float64` |
| Other | `bool` `str` `void` `atom` |

`void` is inhabited by exactly one value, written `unit`. `str` is an immutable
UTF-8 string. `atom` is the supertype of every atom singleton type: `@ok` is a
type inhabited only by the value `@ok`, and it widens to `atom`.

There are **no implicit conversions**, including between integer widths. Every
conversion is an explicit call into `convert`, and every conversion that can
lose information returns a `result`.

## Declaring types

A type is introduced by `deftype`. There is exactly one shape.

```ebnf
deftype  = "(", "deftype", symbol, type-expr, { attribute }, { member }, ")" ;
member   = defn ;
```

Attributes: `where:`, `implements:`, `visibility:`, `doc:`.

```vibra
(deftype point
  (record
    (x float64)
    (y float64))
  doc: "A point in the plane."

  (defn point.origin () point
    (record (x 0.0) (y 0.0)))

  (defn point.translate ((self self) (dx float64) (dy float64)) point
    (record
      (x (add (field self x) dx))
      (y (add (field self y) dy)))))
```

The positional operand after the name is the type's **body** — one type
expression giving its structure. Members follow the labeled group.

### Type aliases do not exist

The body may not be a bare type reference (`E-TYPE-003`). Two names for one
type is exactly the ambiguity design rule 2 forbids: it gives an author two
correct spellings and makes diagnostics ambiguous about which one to print.

Where an alias is tempting, use `(newtype t)`, which creates a distinct type
with an explicit boundary.

### Members are namespaced by their block

Inside `(deftype t ...)`, every member's name must be either:

- `t.<name>` — an **inherent** member of `t`, or
- `i.<name>` where `i` appears in `implements:` — an **implementation** of an
  interface method.

Any other name is `E-TYPE-004`. This single rule holds for `deftype`, `defint`
and `deffect` alike: *inside a declaration block named `n`, members are named
`n.<member>`*, with the interface case as the one documented exception, and the
prefix is what tells a reader which of the two a member is.

When the type's own name equals a listed interface's name — which happens for
every module that names its error type `error` and implements the `error`
interface — **the interface reading wins**, and an inherent member by that name
cannot be declared (`E-TYPE-020`). Other qualified names under the same prefix
still resolve by the longest-prefix rule in
[Modules and names](03-modules.md), so a constructor such as `error.not-found`
is unaffected: the interface has no member by that name, so the reading falls
through to the type.

Members are ordinary functions. There is no receiver magic: a member that
operates on a value takes it as a parameter. By convention and by requirement
for interface methods, that parameter is spelled exactly `(self self)` — the
name `self` with the type `self`. Any other spelling of an interface method's
first parameter is `E-INT-004`.

Members may not be `visibility: @private` while the type is public; member
visibility follows the type.

## Algebraic data types

### Records

```vibra
(deftype rectangle
  (record
    (width float64)
    (height float64)))
```

Field order is significant for construction and for the index. Duplicate field
names are `E-TYPE-005`.

### Tuples

```vibra
(deftype span (tuple uint32 uint32))
```

Tuples have at least two elements; a one-element tuple is `E-TYPE-006`.

### Enums

```vibra
(deftype shape
  (enum
    (circle float64)
    (square float64)
    (empty void)))
```

Each tag carries exactly one payload type. A tag with no data carries `void`;
there is no separate payload-free tag spelling. Constructors are called like
any other function: `(shape.circle 1.0)`, `(shape.empty unit)`.

Tag names are unique within their enum (`E-TYPE-007`).

### Newtypes

```vibra
(deftype user-id (newtype str))
```

A newtype is a distinct nominal type with the same representation as its
underlying type and none of its operations. Crossing the boundary is explicit:

```vibra
(deftype user-id (newtype str))

(defn round-trip () str
  visibility: @private
  (let id (cast "u-1024" user-id))
  (cast id str))
```

`cast` converts only between a newtype and its immediate underlying type.
Anything else is `E-CAST-001`.

### Handles

```vibra
(deftype reader (newtype (handle @read)))
```

A handle is an opaque, host-minted index. Handle types are **unforgeable**:
`cast` to, from, or between handle-backed newtypes is `E-CAST-002`. The only
way to obtain one is from an intrinsic owned by an effect operation; see
[Effects](06-effects.md).

Handle access atoms are `@read`, `@write`, `@read-write` and `@process`. A
handle may widen to a weaker generic capability — `@read-write` to `@read` or
`@write` — but never the reverse (`E-TYPE-008`).

Handles are copyable values. Copying an index does not duplicate the underlying
resource. Handle identifiers are monotonic within a run and never recycled, so
use-after-close and double-close are deterministic typed errors
(`stream.error.resource-closed`) and can never alias a later resource. Making
them *compile-time* errors is deferred to wave 3.

## Generics

Type parameters are declared with `where:`, as a list of parameter forms:

```ebnf
where-value = "(", { type-param }, ")" ;
type-param  = "(", symbol, { type-expr }, ")" ;
```

Each parameter is its own list: the name, followed by zero or more interface
bounds. Zero bounds means unbounded.

```vibra
(deftype pair
  (record
    (first a)
    (second b))
  where: ((a) (b)))
```

```vibra
(defn largest ((values (array t))) (option t)
  where: ((t comparable))
  visibility: @private
  (let-as best (mut (option t)) (option.none unit))
  (for value values
    (let replace
      (match best
        (case (option.none) true)
        (case (option.some (bind current))
          (match (comparable.compare value current)
            (case (ordering.greater) true)
            (case _ false)))))
    (if replace (set best (option.some value)) unit))
  best)
```

Reading a `mut` binding in expression position yields its current value, as
`best` does on the last line; only `set` writes to it.

There is no unbounded-bound spelling such as `any`: absence of a bound *is*
absence of a bound, and offering both would be two spellings for one meaning.

Generics are monomorphized at each use. Recursive instantiation that would not
terminate is `E-TYPE-009`.

### Explicit type arguments

Call sites infer type arguments. When inference cannot determine them, `types:`
supplies **all** of them in declaration order:

```vibra-expr
(let empty (array.empty types: (int64)))
```

Partial application and named type arguments are `E-TYPE-010`. Inference here
is local — it never crosses a module boundary — which is what keeps the search
a decoder has to perform bounded.

## Interfaces

An interface is introduced by `defint` and has the same block shape as
`deftype`.

```ebnf
defint = "(", "defint", symbol, { attribute }, { member }, ")" ;
```

Attributes: `where:`, `implements:`, `visibility:`, `doc:`.

```vibra
(defint display
  doc: "A value with a canonical textual rendering."
  (defn display.show ((self self)) str))
```

A member with **no body** is *required*: every implementing type must provide
it. A member **with a body** is *provided*, and a type may not override it
(`E-INT-005`):

```vibra
(defint comparable
  implements: (display)
  (defn comparable.compare ((self self) (other self)) ordering)

  (defn comparable.less ((self self) (other self)) bool
    (match (comparable.compare self other)
      (case (ordering.less) true)
      (case _ false))))
```

The no-override rule is a direct application of design rule 2. Given a call to
`comparable.less`, there is exactly one body to read, in exactly one place, no
matter what the receiver's concrete type is. When per-type behavior is needed,
do not provide a body — leave the method required.

`implements:` on a `defint` declares **superinterfaces**: a type implementing
`comparable` must also implement `display`.

Every interface method's first parameter must be `(self self)` (`E-INT-004`).
`self` is a type that is legal only inside a `defint` or `deftype` block
(`E-TYPE-011`).

### Implementing an interface

A type declares conformance in `implements:` and provides the required members
inside its own block:

```vibra
(import convert "@std/convert.vib")
(import text "@std/text.vib")

(deftype celsius
  (newtype float64)
  implements: (display)

  (defn display.show ((self self)) str
    (text.concat (convert.float64-to-str (cast self float64)) " degC")))
```

Rules:

- Every required member of every listed interface, and of their
  superinterfaces, must be implemented (`E-INT-001`, listing what is missing).
- An implementation's signature must match the interface's, with `self`
  replaced by the implementing type (`E-INT-002`).
- An implementation's effect row must be within the interface method's declared
  ceiling (`E-EFFECT-003`). Different implementations may perform different
  effects, as long as each stays under the ceiling.
- Implementing a member of an interface not listed in `implements:` is
  `E-INT-003`.

### Implementations live with the type

There is no free-standing implementation form. An interface can only be
implemented **inside the `deftype` block of the implementing type**, which means
you may only implement an interface for a type you declare.

This is a real restriction, adopted knowingly:

- Coherence is structural rather than enforced by an orphan rule. There is
  exactly one place any implementation can be, so there is exactly one place to
  look for it and no possibility of two conflicting ones.
- It costs the ability to implement a third-party interface for a third-party
  type. The v1 answer is to wrap the foreign type in a `newtype` and implement
  on the wrapper.
- It also means **primitives cannot be given new implementations by user code.**
  Interface conformance for `int64`, `str` and the rest is fixed by
  [the standard library](07-stdlib.md). Wrap them to extend them.

Extension implementations are deferred, not rejected; see
[Deferred and rejected](10-deferred.md).

### Dispatch

An interface method is called through its interface namespace, and dispatches on
the runtime type of its `self` argument:

```vibra-expr
(import io "@std/io.vib")

(let temperature 21.5)
(io.stdout.println (display.show temperature))
```

Dispatch is nominal and total: because conformance is declared and checked, the
concrete implementation is always known to exist. Where the concrete type is
statically known — which is the common case, since generics are monomorphized —
the call is resolved statically.

An interface type may be used as a parameter type, which erases the concrete
type behind a dispatch table:

```vibra
(defn describe ((value display)) str
  (display.show value))
```

## Mutability and references

Values have value semantics: assigning or passing copies. Two type
constructors modify that:

- `(mut t)` — a mutable cell. Only a `mut` binding may be the target of `set`.
- `(ref t)` — a borrowed reference, valid for the duration of the call it is
  passed to. A `ref` may not be stored in a record, returned, or captured
  (`E-TYPE-012`).

Neither crosses the host boundary; see [Effects](06-effects.md).

## Subtyping

There is no general subtyping. The only widenings are:

| From | To |
| --- | --- |
| an atom singleton `@x` | `atom` |
| a handle newtype with access `@read-write` | the generic `@read` or `@write` handle |
| a concrete type | an interface it declares in `implements:` |

Everything else requires an explicit call. In particular no integer type widens
to another, and `int32` and `int64` are unrelated.
