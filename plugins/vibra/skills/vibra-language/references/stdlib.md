# Vibra standard library (pre-alpha, `vibra-stdlib` 0.3.0)

The standard library is embedded in the toolchain under the unit `@std`.
Nothing else is available: package dependencies are not implemented yet. This
file lists every public member. If a function is not here, it does not exist.

Usage: `(import ALIAS @std.MODULE)`, then `(ALIAS.member ...)`. The prelude
needs no import: the builtin types (`array`, `dict`, the integer and float
types, `char`) and the types and interfaces that play a language role (`str`,
`bytes`, `bool`, `option`, `result`, `iter`, `equatable`, `ordered`). Their
members are reached through the name itself, such as `(str.concat a b)` or
`(ordered.compare a b)`.

Each operation has one spelling. Comparison is the `equatable` and `ordered`
contracts, so no integer, `char`, `str`, or `bytes` has its own `equal` or
`compare`; only the floats keep IEEE `equal` and `compare-total`, because they
implement neither contract. Conversion is `from` and `try-from`, so no number
has a `parse`, `to-str`, or `to-i64`-style member.

## Builtin members (no import)

### `array`: `(array t)`, immutable

| Member | Signature |
| --- | --- |
| `array.of` | `(array.of item...) -> (array t)`. An empty call needs an expected type |
| `array.length` | `(items (array t)) -> u64` |
| `array.append` | `(items (array t) item t) -> (array t)` (returns a new array) |
| `array.concat` | `(left (array t) right (array t)) -> (array t)` |
| `array.slice` | `(items (array t) start u64 end u64) -> (option (array t))`, end exclusive |
| `array.fold` | `(items (array t) initial a step (fn (a t) a)) -> a`, left to right |
| lookup | `(items index-u64) -> (option t)` |

### `dict`: `(dict k v)`, sorted by key, one entry per key; `k` must be `ordered`

| Member | Signature |
| --- | --- |
| `dict.of` | `(dict.of k1 v1 k2 v2 ...) -> (dict k v)`. A later duplicate key wins |
| `dict.entries` | `(value (dict k v)) -> (array (tuple k v))`, in key order |
| lookup | `(table key) -> (option v)` |

There is no insert, remove, keys, or values member. To change a dict, build
a new one, or keep an `(array (tuple k v))`.

### Integers: `i8 i16 i32 i64 u8 u16 u32 u64`

Each operation comes in two forms. The plain one returns `N` and
**saturates**: a result past the type is its nearest bound, so
`(u8.add 250u8 10u8)` is `255u8` and `(u8.sub 3u8 5u8)` is `0u8`. The
`-checked` one returns `(result N core.arithmetic-error)` instead. Unchecked
`div` and `rem` **trap** with `@runtime.division-by-zero` on a zero divisor, so
use `div-checked` and `rem-checked` when the divisor may be zero.

| Member | Signature |
| --- | --- |
| `N.add` `N.sub` `N.mul` `N.div` `N.rem` | `(left N right N) -> N` |
| `N.neg` (signed `iN` only) | `(value N) -> N` |
| `N.shift-left` `N.shift-right` | `(value N amount u32) -> N`. Past the width, a shift left saturates and a shift right leaves `0` or `-1` |
| `N.add-checked` `N.sub-checked` `N.mul-checked` `N.div-checked` `N.rem-checked` | `(left N right N) -> (result N core.arithmetic-error)` |
| `N.neg-checked` (signed `iN` only) | `(value N) -> (result N core.arithmetic-error)` |
| `N.shift-left-checked` `N.shift-right-checked` | `(value N amount u32) -> (result N core.arithmetic-error)` |

`core.arithmetic-error` is `(enum overflow void division-by-zero void
invalid-shift void)`. Every integer type also implements:

- `equatable` and `ordered`: `(equatable.equal a b)`, `(ordered.compare a b)`.
- `(from M)` for every integer type `M` whose values it holds, and
  `(try-from M)` for every other one: `(from.convert x)` widens and
  `(try-from.convert x)` narrows or changes sign, returning
  `(result N core.conversion-error)`.
- `(try-from str)`: `(try-from.convert "42")` parses decimal text.

### Floats: `f32 f64`

`add sub mul div : (left F right F) -> F`, `neg : (value F) -> F`,
`equal : (left F right F) -> bool` (IEEE: NaN is unequal to itself),
`compare-total : -> core.ordering`. Float operations don't fail. A float is
not `equatable` or `ordered`, so it is not a dict key. It implements
`(try-from str)`. There is no float/integer conversion.

### `char`

| Member | Signature |
| --- | --- |
| `char.to-u32` | `(value char) -> u32` |
| `char.from-u32` | `(scalar u32) -> (option char)` |
| `char.in-range` | `(value char low char high char) -> bool`, inclusive |
| `char.is-ascii-digit` `char.is-ascii-upper` `char.is-ascii-lower` `char.is-ascii-letter` `char.is-ascii-whitespace` | `(value char) -> bool` |

`char` implements `equatable` and `ordered` (by scalar value).

## Comparison and conversion

`equatable` and `ordered` are in the prelude. Bound by them, implement them, or
call them with no import:

- `(equatable.equal left right) -> bool`
- `(ordered.compare left right) -> core.ordering`

```vibra
(defn biggest (items (array t) fallback t) t
  where: (t ordered)
  (array.fold
    items
    fallback
    (lambda (best t item t) t
      (match (ordered.compare item best) (core.ordering.greater) item - best))))
```

The conversion contracts need an import:

- `(import from @std.core.from)`: `(from.convert value) -> self`. Lossless.
- `(import try-from @std.core.try-from)`:
  `(try-from.convert value) -> (result self core.conversion-error)`.

The destination comes from the expected type: a written result type, a
parameter type, or an `as`. In a `match` scrutinee nothing is expected, so
write the `as`:

```vibra
(defn describe (count u64) str (str.concat "count: " (from.convert count)))

(defn port (value i64) u16
  (match
    (as (result u16 core.conversion-error) (try-from.convert value))
    (result.ok narrow) narrow
    (result.err -) 0u16))
```

## `@std.core`: ordering and errors

```vibra
(import core @std.core)
```

- `core.ordering`, `(enum less void equal void greater void)`: match on
  `(core.ordering.less)` and the other variants.
- `core.arithmetic-error`, `core.conversion-error` (`out-of-range`,
  `invalid-format`, `unrepresentable`).
- `core.from` and `core.try-from` are the conversion contracts above.

## `@std.bool`

`true` and `false` need no import, and `bool` implements `equatable` and
`ordered` (`false` before `true`). The connectives are ordinary functions, so
both operands are always evaluated. For short-circuiting, use `if`. Don't use
`bool` as the alias, because it is reserved.

```vibra
(import logic @std.bool)
```

`logic.not (value bool) -> bool`, `logic.and`, `logic.or`, `logic.xor`
`(left bool right bool) -> bool`.

## `str`: a sequence of Unicode scalars (no import)

| Member | Signature |
| --- | --- |
| `str.concat` | `(left str right str) -> str`. Exactly two operands, so nest calls for more |
| `str.length` | `(value str) -> u64` (scalars, not bytes) |
| `str.slice` | `(value str start u64 end u64) -> (option str)` |
| `str.starts-with` `str.ends-with` `str.contains` | `(value str other str) -> bool` |
| `str.index-of` | `(value str needle str) -> (option u64)` |
| `str.split` | `(value str separator str) -> (array str)` |
| `str.trim` `str.trim-start` `str.trim-end` | `(value str) -> str` (ASCII whitespace) |
| `str.to-utf8` | `(value str) -> bytes` |
| `str.from-utf8` | `(encoded bytes) -> (result str core.conversion-error)` |
| lookup | `(value index-u64) -> (option char)` |

`str` implements `equatable` and `ordered` (by scalar), and `(from N)` for every
number type, so `(from.convert 42u64)` with `str` expected renders `"42"`.
There is no formatting or interpolation.

The wrapper constructor and pattern convert between a `str` and its scalars:
`(str scalars)` builds one from an `(array char)`, and `(let (str scalars) text)`
takes it apart.

## `bytes` (no import)

`bytes.length (value bytes) -> u64`, `bytes.concat (left bytes right bytes) ->
bytes`, `bytes.slice (value bytes start u64 end u64) -> (option bytes)`.
Looking up a `bytes` value gives `(option u8)`. `bytes` implements `equatable`
and `ordered` (by byte). `(bytes items)` builds one from an `(array u8)`, and
`(let (bytes items) value)` takes it apart.
## `@std.iter`: lazy iteration

`iter` is in the prelude: call `(iter.collect ...)` or write
`(impl (iter t) ...)` with no import.

Arrays, dicts (as `(tuple k v)` items), `str` (as `char` items), `option`,
and any type with `(impl (iter t) (defn next (value self) (option (tuple t self)) ...))`
are iterable.

| Member | Signature |
| --- | --- |
| `iter.next` | `(value self) -> (option (tuple item self))`: one item and the rest |
| `iter.map` | `(value self transform (fn (item) out)) -> (iter out)`. The callback must be pure |
| `iter.filter` | `(value self keep (fn (item) bool)) -> (iter item)`. The callback must be pure |
| `iter.skip` `iter.take` | `(value self count u64) -> (iter item)` |
| `iter.collect` | `(value self) -> (array item)` |

Write an effectful walk as a tail-recursive function over `iter.next`:

```vibra
(defn write-all (items (iter str)) (result void io.io-error)
  effects: (io.stdout)
  (match
    (iter.next items)
    (option.some (tupleof first rest)) (match
      (io.stdout.text first)
      (result.ok) (write-all rest)
      (result.err failure) (result.err failure))
    (option.none) (result.ok)))
```

## `@std.option`, `@std.result`

You don't need to import them. Use `(option.some x)`, `(option.none)`,
`(result.ok x)`, `(result.ok)` (for a `void` payload), and `(result.err e)`.
Neither type has helper members (no `map`, `unwrap`, or `unwrap-or`), so use
`match`, `try`, or `let-else`.

## Effect modules

Each operation returns a `result` and must be declared in the caller's
`effects:` row and in the binary target's `effects` array.

### `@std.io`: `(import io @std.io)`

| Root (target atom) | Operations |
| --- | --- |
| `io.stdin` (`@std.io.stdin`) | `(io.stdin.text) -> (result str io.io-error)`, `(io.stdin.bytes) -> (result bytes io.io-error)`. Each reads all of standard input |
| `io.stdout` (`@std.io.stdout`) | `(io.stdout.text value) -> (result void io.io-error)`, `(io.stdout.bytes value)` |
| `io.stderr` (`@std.io.stderr`) | `(io.stderr.text value)`, `(io.stderr.bytes value)` |

`io.io-error` is `(enum invalid-text void closed void other void)`.
`stdout.text` doesn't add a newline, so include `\n` yourself.

### `@std.fs`: `(import fs @std.fs)`

Paths are the wrapper `fs.path` over `str`, constructed as
`(fs.path "notes.txt")`.

| Root (target atom) | Operations |
| --- | --- |
| `fs.read` (`@std.fs.read`) | `(fs.read.text at) -> (result str fs.fs-error)`, `(fs.read.file at) -> (result bytes fs.fs-error)`, `(fs.read.entries at) -> (result (array str) fs.fs-error)` |
| `fs.write` (`@std.fs.write`) | `(fs.write.text at contents-str) -> (result void fs.fs-error)`, `(fs.write.file at contents-bytes)` |
| `fs.metadata` (`@std.fs.metadata`) | `(fs.metadata.stat at) -> (result fs.fs-metadata fs.fs-error)` |

`fs.fs-error` is `(enum not-found void permission-denied void not-a-file void
not-a-directory void invalid-path void invalid-text void other void)`.
`fs.fs-metadata` is `(record kind fs.fs-kind size u64)`, and `fs.fs-kind` is
`(enum file void directory void other void)`.

### `@std.env`: `(import env @std.env)`

| Root | Operations |
| --- | --- |
| `env.read` (`@std.env.read`) | `(env.read.var name) -> (result str env.env-error)`, `(env.read.args) -> (result (array str) env.env-error)` |

`env.env-error` is `(enum not-present void not-unicode void)`. `args` holds
only the program arguments, the ones after `--` in `vibra run -- a b`.

### `@std.time`: `(import time @std.time)`

| Root | Operations |
| --- | --- |
| `time.now` (`@std.time.now`) | `(time.now.wall) -> (result i64 time.time-error)` (Unix nanoseconds, UTC), `(time.now.monotonic) -> (result u64 time.time-error)` |

### `@std.random`: `(import random @std.random)`

| Root | Operations |
| --- | --- |
| `random.generate` (`@std.random.generate`) | `(random.generate.bytes count-u64) -> (result bytes random.random-error)` |

## `@std.assert`: tests only

You may import it only from modules under `tests/`, and every test module
must import it.

| Member | Use |
| --- | --- |
| `(assert.equal expected actual)` | Any type except a function. Compares canonical values |
| `(assert.true value)` | `value` must be `true` |
| `(assert.false value)` | `value` must be `false` |
