# 03 Types

Status: draft specification; not normative

## Values

- Every value is immutable and has exactly one type.
- Values have no identity. Two values of a type are equal when their parts
  are equal. Functions and resources have no equality.
- There is no null, no subtyping, no implicit conversion, and no
  overloading. A name denotes one cell.
- Evaluation order inside a pure body is unobservable.

## Built-in types

| Type | Values | Notes |
| --- | --- | --- |
| `bool` | `true`, `false` | |
| `int` | All integers | Unbounded. The default for arithmetic and for claims |
| `u8` … `u64`, `i8` … `i64` | Fixed-width integers | For layouts, hashing, and interchange |
| `f64` | IEEE 754 binary64 | Deterministic. See below |
| `text` | Sequences of Unicode scalar values | Stored as UTF-8 |
| `bytes` | Sequences of `u8` | |
| `unit` | One value | |
| `(list t)` | Finite sequences | Persistent |
| `(array t)` | Finite sequences | Contiguous. Constant-time indexing |
| `(map k v)`, `(set k)` | Finite maps and sets | `k` is `ordered`. Iteration is in key order |
| `(option t)` | `some` with `value`, or `none` | |
| `(result t e)` | `ok` with `value`, or `error` with `error` | |
| `(uncertain t)` | `known` with `value`, or `unknown` | Outcome of a `once` operation ([05](05-execution.md#delivery)) |
| `(pair a b)` | `first` and `second` | The only anonymous product |
| `(fn a … -> r)` | Functions | Pure and total |
| `(intent t)` | A request to a port with outcome `t` | Data. See [05](05-execution.md) |

Hash-based collections, whose iteration order would depend on a hash seed,
do not exist.

### Numbers

Pure code cannot fail, so every partial operation has three forms.

| Form | Example | Behavior |
| --- | --- | --- |
| Proved | `(u32.add a b)`, `(int.div a b)`, `items[i]` | Generates an obligation: no overflow, divisor not zero, index in range |
| Checked | `(u32.add-checked a b)`, `(array.get items i)` | Returns an `option` |
| Defined | `(u32.add-wrapping a b)`, `(u32.add-saturating a b)` | Total by definition |

- `int` arithmetic other than division generates no obligation.
- A conversion that can lose information is `T.from` with an obligation or
  `T.try-from` returning an `option`. There are no casts.
- `f64` operations follow IEEE 754 with round-to-nearest-even and MUST
  give bit-identical results on every backend. Functions beyond the basic
  five come from one portable library, not from the host. Open: what
  claims about `f64` can be proved instead of tested.

## Declared types

Every `type` declaration creates a new nominal type. Two declarations with
the same shape are different types.

### Wrapped types

```text
type cents
  wraps int
  where (int.ge self 0)
end cents
```

- `(cents.of e)` constructs a value and generates the obligation that the
  `where` clauses hold of `e`. `(cents.try-of e)` returns an `option`.
- `c.value` reads the wrapped value. Every `where` clause is a known fact
  about it.
- A wrapped type is not interchangeable with what it wraps.

A wrapped type with a `where` clause is a refinement type. Preconditions
SHOULD be expressed this way instead of by `requires` when they describe
a kind of value and not a relation between parameters.

### Records and variants

```text
type decoded
  field scalar: u32
  field width: u64
end decoded

type shape
  case circle
    field radius: length
  case rectangle
    field width: length
    field height: length
end shape
```

- A record is built with its type name and labeled fields. A variant value
  is built with a qualified case.
- `x.field` reads a field. On a variant it is allowed only where the case
  is known: inside `case k` of a `match` on `x`.
- `match` MUST list every case exactly once. Adding a case to a type is a
  claim change that puts every `match` on it into the worklist.
- `match` applies only to variants. Numbers and text are compared with
  `if`.

### Generic types

```text
type tree
  generic t
  case leaf
  case node
    field left: (tree t)
    field value: t
    field right: (tree t)
end tree
```

Recursive types MUST be well founded: every type has a case that does not
contain the type itself.

## Functions

- A function has a fixed number of typed parameters and one result.
- Functions are values. A function is named by its cell. A nested `fn`
  may read enclosing `let` bindings and MUST NOT read a `var`.
- There is no partial application and no variadic call.

## Interfaces

An interface is a set of member signatures over one type parameter, with
laws.

- `impl i for t` MUST define every member and MUST carry evidence for
  every law.
- There is at most one implementation of an interface for a type, and it
  MUST be declared in the module of the interface or of the type.
- A generic parameter is bounded as `generic k: ordered`. Members are
  called through the interface name and resolved statically.
- Interfaces do not inherit. An interface value does not exist: dispatch
  is never dynamic.

## Checking

- Checking is bidirectional. Every `let`, `var`, parameter, field, and
  result states its type, so every leaf is checked against an expected
  type.
- Type arguments of a generic call are determined from the argument types
  and the expected type. If they are not determined, the call is an error
  and the fix is a typed `let`.
- A type error is reported with the expected type, the found type, and
  the values in scope that have the expected type.

## Totality

Every function, lemma, procedure body, and machine handler terminates.

- A cell that calls itself, directly or through other cells, MUST state
  `decreases e`. `e` is an `int` leaf or a `(measure a b …)` of them,
  ordered lexicographically. Each recursive call generates the obligation
  that the measure is non-negative and strictly smaller.
- A `for` loop runs over a finite collection and needs no measure.
- A `while` loop MUST state `decreases`.
- `match` is exhaustive and `if` chains used as the last statement end in
  `else`.
- A machine never terminates by itself. Each of its handlers does.

A computation with no known measure is admitted only with a `diverge`
debt ([04](04-claims.md#debts)).

## Local state

A body may declare variables with `var` and reassign them with `set`.

- A variable is visible only in its body. It cannot be captured, stored,
  or passed by reference. Passing it passes its current value.
- A loop body that changes variables SHOULD state the `invariant` clauses
  that relate them. Invariants are obligations at loop entry and after
  each iteration.
- A body with variables denotes the same pure function as its translation
  to bindings and recursion ([07](07-compiler.md#the-core-language)).

## Ownership

Values are shared freely by default. Two features add ownership.

### Owned parameters

```text
fn array.set
  generic t
  takes own items: (array t), index: int, value: t
  returns (array t)
  requires (int.lt index (array.length items))
```

- Passing a binding to an `own` parameter **consumes** it. Using a
  consumed binding is an error. A `var` may be consumed and assigned
  again in one `set`.
- A function that updates an owned collection MUST do so in place. This
  is observable only as cost: the cost model charges a constant for an
  update of an owned collection and its length for an update of a shared
  one ([04](04-claims.md#cost)).
- `(array.copy items)` makes an owned copy and is charged its length.

### Resources

A type marked `resource` is linear.

- A resource value MUST be consumed exactly once on every path.
- It cannot be copied, compared, captured by a nested function, or stored
  in a type that is not itself a resource.
- Resources model things that must not be dropped or duplicated: a
  reservation, a one-time token, an obligation to reply.

## Labels

A label marks data whose movement is restricted.

```text
label secret
end secret

type api-key
  wraps (secret text)
end api-key
```

- `(l t)` is the type of a `t` carrying label `l`. A type carries every
  label that appears in its definition.
- **Lifting.** An argument of type `(l a)` may be passed where `a` is
  expected. The result of that application then has type `(l r)`.
- **Conditions.** Under a condition of type `(l bool)`, every value
  bound, assigned, returned, awaited, or emitted carries `l`.
- **Sinks.** An intent may carry label `l` only if the parameter types of
  its port operation carry `l`. A port's parameter types are therefore
  the complete statement of what may leave.
- **Release.** Only a function whose claim states `releases l` can return
  a value without `l` from an argument that carries it. Every such
  function carries a `release` debt that an intent authority accepts or
  rejects.

Label checking is purely static and has grade `proved` when it passes.

## Standard equality and order

- Every type without functions or resources has `T.eq`.
- `ordered` is the standard interface for total orders. Built-in scalar
  types, `text`, and `bytes` implement it.
- There is no generic `to-text`. Display is a function declared per type.
