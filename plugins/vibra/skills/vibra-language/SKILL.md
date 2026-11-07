---
name: vibra-language
description: Write, read, review, and fix Vibra source code (`.vib` files) for the Vibra pre-alpha. Use whenever a task touches a `.vib` file, a `project.vibon` file, or asks for a program, function, type, test, or effect in Vibra, the S-expression language with static effects. Covers syntax, types, patterns, failure handling with `result`/`option`, effects, interfaces, the standard library, and the idioms that replace loops and operators.
---

# Writing Vibra

Vibra is a statically typed, immutable, S-expression language with static
effects. It is **pre-alpha**: the language and its tools change between
releases. Vibra is not Lisp, Clojure, or Scheme. Do not carry over macros,
quoting, `car`/`cdr`, `nil`, truthiness, `+`, `=`, or `cond`. None of them
exist.

Pair this skill with **vibra-toolchain** to run `vibra`. The whole loop is:
edit, `vibra fmt FILE --write`, `vibra --format json check`, `vibra test`.

## Ground rules

1. **No loops and no mutation.** There is no `while`, `for`, assignment, or
   mutable variable. Iterate with tail recursion, `array.fold`, or the `iter`
   adapters. A call in tail position runs in constant stack depth.
2. **No operators.** Arithmetic and comparison are functions on the numeric
   type: `(u64.add-checked a b)` returns `(result u64 core.arithmetic-error)`,
   `(u64.equal a b)` returns `bool`, and `(u64.compare a b)` returns
   `core.ordering`. Floats use `f64.add`, `f64.mul`, and so on, which do not
   fail.
3. **No exceptions.** Failure is a value: `(result t e)` or `(option t)`. A
   `result` you ignore is a compile error (`@type.unhandled-fallible`). Match
   it, `try` it, return it, or discard it with `(let - expr)`.
4. **Effects are declared.** A function that performs I/O writes
   `effects: (io.stdout ...)`. Leaving out `effects:` means the function
   performs no effects. Effects are not inferred. A binary's entry is also
   limited by the `effects` array of its target in `project.vibon`.
5. **Everything is explicit.** Numeric literals carry their type (`1u64`,
   `-3i32`, `2.5f64`) unless context fixes it. Imports have aliases. Nothing is
   public unless it says `visibility: @public`.
6. **Names are kebab-case**: lowercase ASCII, digits, and `-`, starting with a
   letter. Dots separate path segments (`text.concat`) and are never field
   access. `?`, `!`, `_`, and uppercase are invalid in names.
7. **Don't guess.** If a form or function isn't in this skill or its
   references, assume it does not exist. Check it with `vibra check` before
   relying on it.

## Shape of a module

Each `.vib` file is a module. Its identity comes from its path:
`src/app/model.vib` in target `@app` is `@app.model`. The file never declares
its own name. The top-level forms are `import`, `deftype`, `defint`,
`deffect`, `def`, `defn`, and `test`. Comments start with `;`.

```vibra
(import io @std.io)

(import text @std.text)

; A greeting for `name`.
(defn greeting (name str) str
  visibility: @public
  (text.concat "Hello, " (text.concat name "!\n")))

(defn main () (result void io.io-error)
  effects: (io.stdout)
  (io.stdout.text (greeting "world")))
```

`(defn NAME (PARAM TYPE ...) RESULT-TYPE ATTRIBUTES... BODY...)`. The
parameters are a flat list of name/type pairs with no inner parentheses. The
body is a sequence: `let` forms bind names, and the last element is the
result. Attributes come before the body, in this order: `where:`, `labelled:`,
`variadic:`, `visibility:`, `effects:`, `doc:`.

## The core forms

```vibra
(let name (compute) other (more name))        ; flat pairs; visible to later forms
(let-else (option.some head) (items 0u64) (return fallback)) ; refutable bind or leave
(do (let x 1u64) (use x))                     ; group a sequence where one form is expected
(if condition then-value else-value)          ; condition must be `bool`; both branches required
(match value
  (option.some found) (use found)
  (option.none) default)                      ; alternating pattern/result; exhaustive
(try (might-fail))                            ; unwrap ok/some, or return err/none early
(return value)                                ; early exit only, never in tail position
(lambda (item u64) bool (u64.equal item 0u64)) ; anonymous function
(as (array u64) (array.of))                   ; type ascription (widening/fixing, never a cast)
```

- `let` and `let-else` may appear only directly in a body sequence. Inside an
  `if` branch or `match` arm, wrap them in `(do ...)`.
- Binders never shadow. Reusing a visible name is `@name.redeclaration`, so
  pick a new name.
- `-` is the discard pattern: `(result.err -)`, `(let - (log-it))`.
- In `match`, a bare lowercase name *binds*. A literal (`0u64`, `"yes"`, `\a`,
  `@atom`) or a module value such as `true` *compares*.
- `(return x)` in tail position is an error. The last expression already is
  the result.

## Types at a glance

| Want | Write |
| --- | --- |
| Integers / floats | `i8 i16 i32 i64 u8 u16 u32 u64 f32 f64` |
| Text, character, bytes | `str` (`"..."`), `char` (`\a`, `\newline`, `\space`), `bytes` |
| Boolean | `bool`, values `true` / `false` |
| Nothing | type and value `void` |
| Collections | `(array t)`, `(dict k v)`, `(tuple a b)` |
| Optional / fallible | `(option t)`, `(result t e)` |
| Functions | `(fn (u64 str) bool)`, `(fn (str) void effects: (io.stdout))` |

Declare nominal types with `deftype`:

```vibra
(deftype point (record x i64 y i64) visibility: @public)      ; (point x: 1i64 y: 2i64)
(deftype shape (enum circle u64 square u64 empty void))      ; (shape.circle 3u64) (shape.empty)
(deftype user-id u64)                                         ; wrapper: (user-id 7u64)
(deftype failure (union parse-error io.io-error))            ; (failure some-parse-error)
(deftype pair (tuple str u64))                               ; (pair "a" 1u64)
(deftype box (record item t) where: (t any))                 ; generic
```

Read data by applying the value. `(p @x)` reads a record field, `(pair 0)`
projects a tuple component (literal index), and `(items 2u64)`, `(table key)`,
or `(name 0u64)` looks up an element and returns an `option`. Lookups never
trap. Anonymous values use `(tupleof a b)`, `(recordof x: 1i64 y: 2i64)`, and
`(enumof tag: value)`.

## Failure: the everyday pattern

Make one error union per program boundary. Convert each callee's error into it
with `match`. Then chain the steps with `try`:

```vibra
(deftype app-error (union usage env.env-error fs.fs-error io.io-error))

(defn read-source (path str) (result str app-error)
  effects: (fs.read)
  (match
    (fs.read.text (fs.path path))
    (result.ok contents) (result.ok contents)
    (result.err cause) (result.err (app-error cause))))

(defn main () (result void app-error)
  effects: (env.read fs.read io.stdout)
  (let path (try (first-argument)))
  (let source (try (read-source path)))
  (print (render source)))
```

`try` needs the enclosing function to return the same container *and the same
error type*, because there is no implicit conversion. A `main` that returns
`err` makes `vibra run` exit with code 6.

## Effects

Standard effect roots: `io.stdin`, `io.stdout`, `io.stderr` (`@std.io`);
`fs.read`, `fs.write`, `fs.metadata` (`@std.fs`); `env.read` (`@std.env`);
`time.now` (`@std.time`); `random.generate` (`@std.random`). An operation is
`alias.root.operation`, for example `(io.stdout.text "hi")` or
`(fs.read.text (fs.path "a.txt"))`.

- A function's `effects:` row must cover everything its body performs,
  transitively. A declared but unused effect is a warning.
- A function type carries its row exactly, and rows have no subtyping. A
  callback of type `(fn (str) void)` cannot perform I/O.
- The `iter` adapters `map` and `filter` take only pure callbacks. Write an
  effectful walk as a tail-recursive function over `iter.next` with an explicit
  `effects:`.
- Declare your own effect with `deffect`, whose operations have Vibra bodies.
  The target array names it by its full path, for example
  `@app.main.log`. See [references/language.md](references/language.md).

## Iteration without loops

```vibra
; Tail recursion: the accumulator carries state, and the call is in tail position.
(defn sum-to (n u64 total u64) u64
  (match
    (u64.sub-checked n 1u64)
    (result.err -) total
    (result.ok next) (sum-to next (plus total n))))

; Fold an array.
(array.fold items 0u64 (lambda (total u64 item u64) u64 (plus total item)))

; Lazy adapters over anything iterable (arrays, dicts, str, option, your types),
; after (import iter @std.iter.iter):
(iter.collect
  (iter.map
    (iter.filter items (lambda (item u64) bool (u64.equal item 0u64)))
    (lambda (item u64) str (u64.to-str item))))
```

Write small saturating or defaulting helpers such as `plus` and `minus` around
the checked operations. The standard library does the same. An example is in
[examples/word-stats](examples/word-stats/src/stats/count.vib).

## Tests

Test modules live under `tests/` (module `@tests.NAME`), import
`@std.assert`, and import what they test by target path:

```vibra
(import assert @std.assert)

(import count @stats.count)

(test "an empty line has no words"
  (assert.equal 0u64 (count.count-words "")))
```

The assertions are `assert.equal` (any non-function type), `assert.true`, and
`assert.false`. Failures report the **first** operand as `expected`, so write
the expected value first. Tests can only reach `visibility: @public`
declarations. Effectful tests declare `effects:` and run in an empty,
deterministic host world: no files, no arguments, no environment, and output
captured.

## Gaps in the pre-alpha

Don't look for these. They don't exist yet:

- String formatting or interpolation. Build text with `text.concat` and
  `u64.to-str`, `i64.to-str`, or `f64.to-str`.
- Inserting into or removing from a `dict`. Dicts come only from
  `(dict.of k v ...)`. Use an `(array (tuple k v))` that you append to, or
  rebuild the dict.
- Sorting, `array.map`, `array.filter`, or `array.reverse`. Use `iter` or
  write a recursive helper.
- Package dependencies. `dependencies:` must be `(dict)`, and only the
  embedded `@std` library is available.
- Concurrency, FFI, macros, and a REPL.

## References

- [references/language.md](references/language.md): the full form reference.
  It covers declarations and attributes, labelled and variadic parameters,
  generics, interfaces (`defint`/`impl`), conversion, patterns, unions and
  `as`, user effects, and canonical formatting.
- [references/stdlib.md](references/stdlib.md): every standard-library module
  and builtin member with its signature.
- [references/pitfalls.md](references/pitfalls.md): common mistakes, with the
  diagnostic each produces and the fix.
- [examples/word-stats](examples/word-stats): a complete, checked project
  with a library target, a binary target, error unions, effects, and tests.
