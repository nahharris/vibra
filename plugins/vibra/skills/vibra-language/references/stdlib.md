# Vibra standard library (pre-alpha, `vibra-stdlib` 0.3.0)

The standard library is embedded in the toolchain under the unit `@std`.
Nothing else is available: package dependencies are not implemented yet. This
file lists every public member. If a function is not here, it does not exist.

Usage: `(import ALIAS @std.MODULE)`, then `(ALIAS.member ...)`. The builtin
types (`array`, `dict`, the integer, float, and `char` types) need no import.

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

Every type `N` has the members below. The `iN` types also have
`N.neg-checked (value N) -> (result N core.arithmetic-error)`.

| Member | Signature |
| --- | --- |
| `N.add-checked` `N.sub-checked` `N.mul-checked` `N.div-checked` `N.rem-checked` | `(left N right N) -> (result N core.arithmetic-error)` |
| `N.shift-left-checked` `N.shift-right` | `(value N amount u32) -> (result N core.arithmetic-error)` |
| `N.equal` | `(left N right N) -> bool` |
| `N.compare` | `(left N right N) -> core.ordering` |
| `N.to-str` | `(value N) -> str` |
| `N.parse` | `(text str) -> (result N core.conversion-error)` |
| `N.to-M` | widening: `(value N) -> M`. Narrowing or a sign change: `-> (result M core.conversion-error)` |

`core.arithmetic-error` is `(enum overflow void division-by-zero void
invalid-shift void)`.

### Floats: `f32 f64`

`add sub mul div : (left F right F) -> F`, `neg : (value F) -> F`,
`equal : -> bool`, `compare-total : -> core.ordering`, `to-str : -> str`,
`parse : (text str) -> (result F core.conversion-error)`. Float operations
don't fail. There is no float/integer conversion member.

### `char`

`char.to-u32 (value char) -> u32`, `char.from-u32 (scalar u32) -> (option char)`.

## `@std.core`: ordering, errors, and the core interfaces

```vibra
(import core @std.core)
```

- `core.ordering`, `(enum less void equal void greater void)`: match on
  `(core.ordering.less)` and the other variants.
- `core.arithmetic-error`, `core.conversion-error` (`out-of-range`,
  `invalid-format`, `unrepresentable`).
- Interfaces, each imported as a declaration so you can bound by it,
  implement it, or call it:
  - `(import equatable @std.core.equatable)`: `(equatable.equal left right) -> bool`
  - `(import ordered @std.core.ordered)`: `(ordered.compare left right) -> core.ordering`
  - `(import from @std.core.from)`: `(from.convert value) -> self`.
    Lossless. The destination comes from the expected type
  - `(import try-from @std.core.try-from)`: `(try-from.convert value) -> (result self core.conversion-error)`

## `@std.bool`

`true` and `false` need no import. The connectives are ordinary functions, so
both operands are always evaluated. For short-circuiting, use `if`. Don't use
`bool` as the alias, because it is reserved.

```vibra
(import logic @std.bool)
```

`logic.not (value bool) -> bool`, `logic.and`, `logic.or`, `logic.xor`
`(left bool right bool) -> bool`.

## `@std.text`: `str` is a sequence of Unicode scalars

```vibra
(import text @std.text)
```

| Member | Signature |
| --- | --- |
| `text.concat` | `(left str right str) -> str`. Exactly two operands, so nest calls for more |
| `text.length` | `(value str) -> u64` (scalars, not bytes) |
| `text.equal` | `(left str right str) -> bool` |
| `text.compare` | `(left str right str) -> core.ordering` |
| `text.slice` | `(value str start u64 end u64) -> (option str)` |
| `text.starts-with` `text.ends-with` `text.contains` | `(value str other str) -> bool` |
| `text.index-of` | `(value str needle str) -> (option u64)` |
| `text.split` | `(value str separator str) -> (array str)` |
| `text.trim` `text.trim-start` `text.trim-end` | `(value str) -> str` (ASCII whitespace) |
| `text.to-chars` | `(value str) -> (array char)` |
| `text.from-chars` | `(scalars (array char)) -> str` |
| `text.to-utf8` | `(value str) -> bytes` |
| `text.from-utf8` | `(encoded bytes) -> (result str core.conversion-error)` |
| lookup | `(value index-u64) -> (option char)` |

To turn a number into text, use `u64.to-str`, `i64.to-str`, `f64.to-str`, and
so on. There is no formatting or interpolation.

## `@std.char`: ASCII classes

```vibra
(import classes @std.char)
```

`classes.equal`, `classes.compare (left char right char)`;
`classes.is-ascii-digit`, `is-ascii-upper`, `is-ascii-lower`,
`is-ascii-letter`, `is-ascii-whitespace (value char) -> bool`.

## `@std.bytes`

```vibra
(import octets @std.bytes)
```

`octets.length -> u64`, `octets.concat`, `octets.equal`, `octets.compare`,
`octets.slice (value bytes start u64 end u64) -> (option bytes)`,
`octets.to-array (value bytes) -> (array u8)`,
`octets.from-array (values (array u8)) -> bytes`. Looking up a `bytes` value
gives `(option u8)`. Don't use `bytes` as the alias, because it is reserved.

## `@std.iter`: lazy iteration

```vibra
(import iter @std.iter.iter)
```

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
