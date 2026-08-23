# 5. Expressions

Status: draft

## Grammar

```ebnf
expr    = atom
        | call
        | "(", "do", { expr }, ")"
        | "(", "let", symbol, expr, ")"
        | "(", "let-as", symbol, type-expr, expr, ")"
        | "(", "set", symbol, expr, ")"
        | "(", "return", [ expr ], ")"
        | "(", "if", expr, expr, expr, ")"
        | "(", "while", expr, body, ")"
        | "(", "for", symbol, expr, body, ")"
        | "(", "break", ")"
        | "(", "continue", ")"
        | "(", "match", expr, arm, { arm }, ")"
        | "(", "try", expr, ")"
        | "(", "fn", params, type-expr, { attribute }, body, ")"
        | "(", "record", { field-value }, ")"
        | "(", "tuple", expr, expr, { expr }, ")"
        | "(", "array", { expr }, ")"
        | "(", "map", { map-entry }, ")"
        | "(", "range", expr, expr, expr, ")"
        | "(", "field", expr, symbol, ")"
        | "(", "index", expr, expr, ")"
        | "(", "cast", expr, type-expr, ")"
        | "(", "mut", expr, ")"
        | "(", "ref", expr, ")" ;

call        = "(", symbol, { expr }, [ "types:", "(", { type-expr }, ")" ], ")" ;
arm         = "(", "case", pattern, body, ")" ;
body        = expr, { expr } ;
field-value = "(", symbol, expr, ")" ;
map-entry   = "(", expr, expr, ")" ;
```

## Bodies and sequencing

A **body** is one or more expressions evaluated in order; its value is the last
one. Bodies appear in `defn`, `fn`, `while`, `for` and `case`.

`do` turns a sequence into a single expression, for the two positions that take
exactly one: the branches of `if`.

```vibra-expr
(import io "@std/io.vib")

(let ready true)
(if ready
  (do
    (io.stdout.println "starting")
    (io.stdout.println "started"))
  (io.stderr.println "not ready"))
```

An empty `do` has the value `unit`.

## Bindings

```vibra-expr
(let count 0)
(let-as total int64 0)
```

`let` infers the type from the initializer. `let-as` states it, and the
initializer must match exactly (`E-TYPE-013`) — `let-as` never coerces.

Bindings are immutable unless the initializer is a `mut` cell:

```vibra-expr
(let seen (mut 0))
(set seen (add seen 1))
```

`set` targets only a `mut` binding (`E-MUT-001`). A binding's name must not
shadow an enclosing one (`E-SCOPE-001`); see [Modules and names](03-modules.md).

A named binding that is never read is `W-BIND-001`. Discard a value
deliberately by binding it to `_`.

## Control flow

`if` takes a `bool` condition and two branches of the same type
(`E-TYPE-014`). There is no one-armed `if`; when there is nothing to do in the
other branch, write `unit`.

`while` takes a `bool` condition and a body, and has type `void`. `for` iterates
an `(array t)` or a `range` and has type `void`. `break` and `continue` take no
operands and are legal only inside a loop (`E-FLOW-001`).

```vibra-expr
(import convert "@std/convert.vib")
(import io "@std/io.vib")

(for index (range 0 10 1)
  (io.stdout.println (convert.int64-to-str index)))
```

`(range start end step)` produces a half-open range. A zero `step` is
`E-FLOW-002`.

`return` exits the enclosing function. `(return)` with no operand returns
`unit` and is legal only in a `void` function (`E-TYPE-015`).

## Matching

```vibra-expr
(import io "@std/io.vib")

(let value (option.some "present"))
(match value
  (case (option.some (bind inner)) (io.stdout.println inner))
  (case (option.none) (io.stderr.println "absent")))
```

Each arm is a delimited `(case <pattern> <body>)` form, and a body is a
sequence, so no `do` is needed inside an arm.

> **Changed from the pre-reboot language,** which used bare alternating
> pattern/body pairs. Delimited arms cost four characters per arm and buy a
> property worth more: the boundary between a pattern and a body is explicit, so
> an author (or a decoder) reading a long `match` can tell locally which one the
> cursor is in, instead of counting forms from the top.

### Patterns

```ebnf
pattern = literal
        | atom-literal
        | "_"
        | "(", "bind", symbol, ")"
        | "(", symbol, pattern, ")"
        | "(", "record", { pattern-field }, ")"
        | "(", "tuple", pattern, pattern, { pattern }, ")"
        | "(", "array", { pattern }, ")" ;

pattern-field = "(", symbol, pattern, ")" ;
```

`(symbol pattern)` is an enum constructor pattern, using the same qualified
name as the constructor call. `_` is the only wildcard. `(bind name)` is the
only binding form, and its name obeys the shadowing rule.

Matches must be exhaustive. A non-exhaustive match is `E-MATCH-001`, and the
diagnostic names a concrete uncovered value. An arm that can never match is
`W-MATCH-001`.

There are no guards in v1: a pattern is a structural test and nothing else,
which keeps exhaustiveness decidable and the decoding automaton small.

## Failure

Vibra has no exceptions. Failure is a value.

`(result t e)` and `(option t)` are ordinary enums in `core`:

```vibra-expr
(import fs "@std/fs.vib")

(let-as found (result int64 fs.error) (result.ok 1))
(let-as missing (result int64 fs.error) (result.error (fs.error.not-found "/etc/nope")))
(let present (option.some "present"))
(let-as absent (option str) (option.none unit))
```

Constructing one variant leaves the other side's type parameter unconstrained —
`(result.ok 1)` says nothing about the error type, and `(option.none unit)` says
nothing about the payload — so those bindings state their type. Where the
surrounding context already fixes it, such as a `return` operand or an argument
to a typed parameter, `let` alone is enough.

### Propagation

`(try expr)` is the single propagation form.

- For a `(result u e)` operand, the enclosing function must return
  `(result t e)` with the **identical** error type. Success continues with `u`;
  an error returns from the enclosing function unchanged.
- For an `(option u)` operand, the enclosing function must return
  `(option t)`. `some` continues; `none` returns.
- Mixing the two is `E-TRY-001`. A mismatched error type is `E-TRY-002`, and
  the fix is an explicit conversion call — Vibra never infers one.
- `try` outside a function whose return type is a `result` or `option` is
  `E-TRY-003`.

```vibra
(import fs "@std/fs.vib")
(import text "@std/text.vib")

(defn read-trimmed ((path str)) (result str fs.error)
  effects: (fs.read)
  (let contents (try (fs.read.to-str path)))
  (result.ok (text.trim contents)))
```

There is no `?` suffix and no reader macro. `try` is a form like every other
form, which is what keeps the reader free of operator characters.

### Unhandled failures

An expression in **non-final** position in a body whose type is a `result` or
an `option` is `W-RESULT-001` unless its value is bound, returned, or
propagated with `try`.

The final expression of a body and the operand of `return` are the body's
value, and are exempt.

Discard deliberately:

```vibra-expr
(import io "@std/io.vib")

(let _ (io.stdout.println "best effort"))
```

Silently dropping a typed failure is the exact mistake an author focused on the
happy path makes, and it is the one place where the language's safe-by-default
claim would otherwise be false.

## Function values

`fn` is an anonymous function. Its type is `(fn-type (param-type ...) return)`.

```vibra-expr
(let double (fn ((value int64)) int64 (mul value 2)))
```

An `fn` captures the bindings it references by value; it may not capture a
`mut` cell or a `ref` (`E-TYPE-016`). Its effects are inferred, not declared.

## Primitive operations

The following unqualified names are primitives in every module, resolved before
any user definition. This list is closed; v1 adds no others.

| Group | Names |
| --- | --- |
| Arithmetic | `add` `sub` `mul` `div` `rem` `neg` |
| Comparison | `equal` `not-equal` `less` `less-equal` `greater` `greater-equal` |
| Logic | `and` `or` `not` |
| Bitwise | `bit-and` `bit-or` `bit-xor` `bit-not` `shift-left` `shift-right` |

Arithmetic and comparison are defined on matching primitive numeric types only;
there is no promotion, so `(add x y)` with `x : int32` and `y : int64` is
`E-OP-001`.

`and` and `or` short-circuit.

### Arithmetic never wraps silently

Integer overflow, underflow, and division or remainder by zero **trap**: the
program terminates deterministically with a runtime fault naming the operation
and its span. They are not undefined, they do not wrap, and they do not produce
a poison value.

An operation on literal operands that would trap is caught at compile time as
`E-OP-002`.

When failure must be handled rather than fatal, use the explicit forms in
`math`, which return `result`:

```vibra-expr
(import math "@std/math.vib")

(let a 9223372036854775807)
(let b 1)
(let total
  (match (math.add-checked-int64 a b)
    (case (result.ok (bind sum)) sum)
    (case (result.error (bind _)) 0)))
```

Wrapping arithmetic is available as `math.add-wrapping-int64` and peers. It is
never the default and never implicit.

Float arithmetic follows IEEE 754 and does not trap.

## Collections

```vibra-expr
(let names (array "ana" "bo" "cy"))
(let scores (map ("ana" 3) ("bo" 5)))
(let point (record (x 1.0) (y 2.0)))
(let bounds (tuple 0 10))
```

An `(array t)` is immutable and homogeneous. A `(map k v)` requires `k` to
implement `hashable`. An empty literal has no inferrable element type and needs
`types:`, or a typed binding:

```vibra-expr
(let-as empty (array int64) (array))
```

Records are constructed with every field present, in declaration order
(`E-TYPE-017` for a missing, extra, or misordered field).

### Access

```vibra-expr
(let point (record (x 1.0) (y 2.0)))
(let horizontal (field point x))
(let names (array "ana" "bo" "cy"))
(let first (index names 0))
```

`(field expr name)` reads a record field. The name is a bare symbol naming a
declared field of the operand's type; it is not an expression and not a string.
An unknown field is `E-TYPE-019`.

`(index expr position)` reads from an `(array t)` at a `uint64` position, or
from a `(map k v)` at a key. Array indexing is bounds-checked and traps out of
range; use `array.get` for a bounds-checked `(option t)`. Map indexing returns
`(option v)`, because a missing key is an ordinary outcome rather than a fault.

`field` and `index` are forms, not calls, because their second operand is not a
value. Tuples are read by matching, not by position index.

## Evaluation order

Operands evaluate left to right, before the call. Labeled operands evaluate in
source order, after the fixed positional operands. Evaluation is strict
everywhere except the second operand of `and` and `or`, and the untaken branch
of `if` and `match`.
