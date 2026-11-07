# Vibra pitfalls: symptom, diagnostic, fix

Every diagnostic below was produced by the pre-alpha `vibra check`. Human
output shows only `code: message`. Run `vibra --format json check` to get the
file, line, and column (`primarySpan.sourceId` and `startPosition`).

## Habits from other languages

| You wrote | Diagnostic | Write instead |
| --- | --- | --- |
| `(+ a b)`, `(= a b)`, `(< a b)` | `@syntax.invalid-name` or `@name.unknown-symbol` | `(u64.add a b)`, which saturates, or `(u64.add-checked a b)`, which gives a `result`. Also `(equatable.equal a b)` and `(match (ordered.compare a b) (core.ordering.less) ...)` |
| `(while ...)`, `(for ...)`, `break` | `@syntax.retired-form` | A tail-recursive helper with an accumulator, `array.fold`, or `iter` |
| `(set! x ...)`, reassigning with `let` | `@name.redeclaration` | A new name. Bindings never shadow, even across nested scopes |
| `(if cond value)` without else | `@syntax.invalid-form` | Both branches. Use `void` for "nothing": `(if c (do-it) void)` |
| `(if n ...)` on a number or option | `@type.mismatch` ("expected bool") | An explicit test such as `(equatable.equal n 0u64)`, or a `match` |
| `fooBar`, `foo_bar`, `valid?`, `Foo` | `@syntax.invalid-name` | `foo-bar`, `is-valid` |
| `(str.concat a b c)` | `@type.argument-mismatch` ("unexpected positional operands") | `(str.concat a (str.concat b c))` |
| `(u64.equal a b)`, `(text.equal a b)`, `(i32.to-str n)` | `@name.unknown-symbol` ("the builtin type `u64` has no member") | `(equatable.equal a b)`, `(from.convert n)` with `str` expected |
| `(match (u64.add a b) (result.ok sum) ...)` | `@type.mismatch` | `u64.add` saturates and returns `u64`. Use `u64.add-checked` to match on overflow |
| `(u64.div total count)` where `count` may be `0` | The run stops with the trap `@runtime.division-by-zero` (exit 5) | `(u64.div-checked total count)`, and handle `(result.err -)` |
| `(let x 5)` | `@type.ambiguous-inference` | `5u64`, or a typed context |
| `-1u8`, `256u8` | `@type.numeric-out-of-range` | A type that fits |
| Mixing `u64` and `i32` | `@type.argument-mismatch` | Convert explicitly: `(try-from.convert n)` returns a `result` for `i32` to `u64`, `(from.convert n)` widens `u32` to `u64` |
| `(match (try-from.convert n) ...)` or `(let x (from.convert n))` | `@type.ambiguous-destination` | Name the destination: `(as (result u64 core.conversion-error) (try-from.convert n))` |
| `(import bool @std.bool)`, `(import bytes @std.bytes)` | `@name.reserved-value-spelling` | An alias that isn't a builtin type name (`logic`). `str`, `bytes`, and `char` need no import at all |

## Failure handling

| You wrote | Diagnostic | Write instead |
| --- | --- | --- |
| Called something that returns `result` and ignored it | `@type.unhandled-fallible` | `match`, `try`, return it, or `(let - (call))` to discard it on purpose |
| `try` over a `result` whose error type differs from the function's | `@type.invalid-try` | Match it and wrap the error: `(result.err cause) (result.err (my-error cause))`, where `my-error` is a union that includes the callee's error |
| `(match opt (option.some n) n)` | `@pattern.non-exhaustive` | Cover `(option.none)`, or use `-` as the last arm |
| `(return x)` as the last expression | `@type.redundant-return` | Just `x` |
| `(return ...)` directly in a `test` | `@type.invalid-return` | Restructure with `match`. Tests have no function to return from |
| `(result.ok void)` or `(option.none void)` | `@type.argument-mismatch` ("has a `void` payload and takes no operand") | `(result.ok)` and `(option.none)`, in both expressions and patterns |

## Bindings and structure

| You wrote | Diagnostic | Write instead |
| --- | --- | --- |
| `let` inside an `if` branch or `match` arm | `@syntax.misplaced-binding` | Wrap that branch in `(do (let ...) result)` |
| A parameter or `let` with the same name as a module function | `@name.redeclaration` ("shadows a visible module binding") | Rename one of them |
| Missing record field: `(p x: 1u64)` | `@type.argument-mismatch` ("missing field") | Supply every field. Records have no defaults |
| `(array.of)` or `(dict.of)` with nothing to fix its type | `@type.ambiguous-inference` | `(as (array u64) (array.of))` |
| `where: (t core.ordered)` | `@syntax.invalid-form` ("generic bounds must be symbols") | `where: (t ordered)`: `ordered`, `equatable`, and `iter` are in the prelude |
| Using `core.ordering`, `from.convert`, or `fs.*` without importing | `@name.unknown-symbol` | `(import core @std.core)`, `(import from @std.core.from)`, `(import fs @std.fs)`, ... |
| Calling a private function from another module or a test | `@name.private-access` | Add `visibility: @public` to the declaration |

## Effects

| You wrote | Diagnostic | Write instead |
| --- | --- | --- |
| I/O in a function without `effects:` | `@effect.outside-ceiling` ("this body performs ...") | Add `effects: (io.stdout)` to that function *and* to every caller up to `main` |
| `effects: (@std.io.stdout)` | `@effect.invalid-reference` | `effects: (io.stdout)` with `(import io @std.io)` |
| An effectful call inside an `array.fold`, `iter.map`, or `iter.filter` lambda | `@effect.outside-ceiling` | Write a tail-recursive walk over `iter.next` or the array index, declaring `effects:` |
| `main` performs a root that the target `effects` array lacks | `@effect.outside-ceiling` ("the target `effects` array does not contain") | Add the root atom, such as `@std.io.stdout`, to the target in `project.vibon` |
| A root in `effects:` that is never performed | `@contract.unused-effect` (warning) | Remove it |

## Tests

| You wrote | Diagnostic | Write instead |
| --- | --- | --- |
| A test module without `(import assert @std.assert)` | `@module.missing-required-import` | Add the import |
| A `test` form under `src/` | `@tool.unavailable` (exit 4) | Put tests under `tests/` |
| A test that reads real files or arguments | The test sees an empty world, so you get `not-found` or empty args | Keep logic pure and test the pure functions. Pass data in as parameters |
| `(assert.equal actual expected)` | The failure report swaps the labels | `(assert.equal expected actual)` |
