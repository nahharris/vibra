# Vibra language reference (pre-alpha)

This reference covers the language that the current `vibra` toolchain accepts.
The normative source is the specification in the Vibra repository
(`docs/spec/01-source-language.md` through `07-*.md`). When this file and the
toolchain disagree, the toolchain wins. Check with `vibra check`.

## Contents

1. Lexical syntax
2. Declarations and attributes
3. Expressions and body sequences
4. Patterns
5. Types and constructors
6. Generics
7. Interfaces (`defint` / `impl`)
8. Conversion (`from` / `try-from`)
9. Unions and `as`
10. Effects and `deffect`
11. Modules, imports, and visibility
12. Canonical formatting

## 1. Lexical syntax

- A module is a UTF-8 file of forms. `( ... )` delimits lists, whitespace
  separates tokens, and `;` starts a comment to the end of the line. There are
  no brackets, braces, commas, quoting, or reader macros.
- **Names** are dot-separated kebab segments, each matching `[a-z][a-z0-9-]*`.
  `a`, `a1`, `user-id`, and `str.concat` are valid. `A`, `_x`, `x?`, `1a`,
  and `-a` are invalid (`@syntax.invalid-name`).
- **Atoms** are `@name`, for example `@info` or `@std.io`. An atom is a plain
  value, except in an `import` locator, a record selector `(r @field)`, and
  `project.vibon` reference slots.
- **Labels** are `name:` and take the one form that follows.
- **Discard** is `-` (also spelled `@-` and `-:`). It binds nothing and may
  repeat.
- **Strings** are `"..."` with the escapes `\" \\ \n \r \t \u{1F600}`. There
  is no interpolation, raw string, or multi-line literal syntax.
- **Characters** are a backslash followed by the character: `\a`, `\0`, and
  `\-`. Special characters use `\newline`, `\return`, `\space`, `\tab`, or
  `A`.
- **Integers** are decimal, with an optional `-` and a type suffix: `7u64`,
  `-3i32`, `255u8`. There is no hex, binary, `_` separator, `usize`, or
  `isize`. Without a suffix, the literal's type comes from context, and an
  ambiguous context is an error.
- **Floats** have a `.`, an exponent, or a suffix: `1.5`, `2e3`, `2f64`,
  `0.5f32`. There is no source spelling for NaN or infinity.
- **`void`** is both the unit type and its only value.
- **Booleans** have no literal syntax. `true` and `false` are prelude values
  of type `bool`.
- **Reserved heads**: `do let let-else if match return as try lambda tupleof
  recordof enumof`, plus the declaration heads. `while`, `for`, `break`, and
  `continue` are retired (`@syntax.retired-form`).

## 2. Declarations and attributes

```text
(import alias @unit.module)                 ; or @unit.module.declaration
(def name type value)                       ; immutable module value; must be pure
(defn name (p1 t1 p2 t2) result attrs... body...)
(deftype name type-body attrs... methods... impls...)
(defint name attrs... members... impls...)
(deffect name attrs... operations...)
(test "name" effects: (...) body...)        ; only under tests/
```

The function attributes go in this canonical order. All are optional.

| Attribute | Meaning |
| --- | --- |
| `where: (t any u ordered)` | Generic parameters, each with a bound interface (`any` means no requirement) |
| `labelled: (level atom @info strict bool false)` | Named optional parameters as flat name/type/default triples. Defaults must be constants |
| `variadic: (rest (array str))` or `(fields (dict str u64))` | One trailing variadic parameter |
| `visibility: @public` | Exported. Declarations are private otherwise |
| `effects: (io.stdout fs.read)` | Effect ceiling. Omitted means `()` |
| `doc: "..."` | Documentation string |

Type attributes go in the order `where:`, `visibility:`, `doc:`. `role:`,
`external:`, `symbol:`, and `native:` are only valid in the embedded standard
library.

Call syntax: positional operands first, then labelled operands, then the
variadic tail.

```vibra
(defn level (message str) atom
  labelled: (severity atom @info)
  severity)

(level "started")                     ; @info
(level "failed" severity: @error)     ; @error

(defn collect (first i32) (array i32)
  variadic: (rest (array i32))
  rest)

(collect 1i32 2i32 3i32)              ; rest = (array.of 2i32 3i32)
```

Methods are `defn` forms nested in a `deftype`, and you call them through the
type path:

```vibra
(deftype stack (record items (array t))
  where: (t any)
  visibility: @public
  (defn push (value self item t) self
    visibility: @public
    (stack items: (array.append (value @items) item))))

(stack.push (stack items: (as (array u64) (array.of))) 1u64)
```

An empty `(array.of)` or `(dict.of)` needs an expected type, so wrap it in
`as` when nothing else fixes the type (`@type.ambiguous-inference`).

`self` names the enclosing type. A nested method inherits its owner's
generics. Method visibility is set per method.

Entry functions, the `entry` of a binary target, take no parameters and
return `void` or `(result void e)` for a nominal error type `e`. They don't
have to be named `main` or be public.

## 3. Expressions and body sequences

A **body** is a sequence of elements, used by `defn`, `lambda`, `do`, and
`test`. Its value is the last element. If the last element is a `let`, the
value is `void`. An empty body is `void`.

| Form | Notes |
| --- | --- |
| `(let p1 e1 p2 e2 ...)` | Flat pairs. Names are visible to later pairs and later elements. Irrefutable patterns only |
| `(let-else pat expr fallback)` | Refutable `pat`. `fallback` must leave, for example with `(return ...)`, and has type `never` |
| `(do body...)` | Groups a sequence where one expression is expected |
| `(if cond then else)` | `cond : bool`. Both branches are required and must have the same type |
| `(match subject pat1 res1 pat2 res2 ...)` | Exhaustive and checked for reachability. Each arm has one result, so use `do` for several |
| `(try expr)` | On `(result t e)`, gives `t` or returns `(result.err e)`. On `(option t)`, gives `t` or returns `(option.none)`. The function must return the same container and error type |
| `(return expr)` | Leaves the innermost `defn` or `lambda`. Not allowed in tail position or in a test body. A `void` function writes `(return void)` |
| `(as type expr)` | Ascription: widens to a union member, fixes inference, or names a conversion's destination. Never narrows or casts |
| `(lambda (p t) result attrs... body...)` | Anonymous function. It captures by value and may write `where:`, `labelled:`, `variadic:`, and `effects:` |
| `(tupleof a b)` `(recordof x: a y: b)` `(enumof tag: v)` | Anonymous values. `enumof` needs an expected type |
| `(f args...)` | Application. The callee's static type decides what it means |

Application kinds:

| Callee | Operand | Result |
| --- | --- | --- |
| function value | its signature | its result |
| tuple | literal index `0`, `1`, ... | the component |
| record | atom selector `@field` | the field |
| `(array t)` | `u64` index | `(option t)` |
| `(dict k v)` | key of type `k` | `(option v)` |
| `str` | `u64` scalar index | `(option char)` |
| `bytes` | `u64` byte index | `(option u8)` |
| a type name | fields or payload | constructs a value |

Naming a `defn` gives a function value. Constructors, projections, and
lookups are not function values; wrap them in a `lambda`. A function with no
parameters is called as `(f)`.

Binding rules: a binder must not repeat any visible name, including a
module-level name or an earlier `let` in the same body. This is
`@name.redeclaration`. Binders spelled like the prelude (`str`, `option`,
`true`, `array`, and so on) are `@name.reserved-declaration`.

## 4. Patterns

| Pattern | Matches |
| --- | --- |
| `name` | Anything, and binds it, unless `name` resolves to a module value (a constant pattern, for example `true`) |
| `-` | Anything, without binding |
| `0u64` `"text"` `\a` `@atom` | Equal literal (refutable) |
| `(option.some p)` `(result.err p)` `(shape.circle p)` | Enum variant, with a payload pattern. A `void` payload has no operand: `(option.none)`, `(result.ok)` |
| `(point x: p y: q)` `(recordof x: p)` | Record. Omitted fields are ignored |
| `(pair p q)` `(tupleof p q)` | Tuple, exact arity |
| `(user-id n)` | Wrapper, which unwraps |
| `(array p q)` | Array of exactly that length |
| `(as type p)` | Union member `type`, binding `p` at that type |

Patterns nest: `(option.some (tupleof first rest))`. Parameters and `let`
accept irrefutable patterns, such as `((tupleof a b) (tuple u64 u64))`.

## 5. Types and constructors

| Body | Declaration | Construct | Read |
| --- | --- | --- | --- |
| record | `(deftype p (record x i64 y i64))` | `(p x: 1i64 y: 2i64)` | `(v @x)` |
| tuple | `(deftype pr (tuple str u64))` | `(pr "a" 1u64)` | `(v 0)` |
| enum | `(deftype s (enum a u64 b void))` | `(s.a 1u64)` `(s.b)` | `match` |
| union | `(deftype u (union i32 str))` | `(u 5i32)` | `match` with `as` |
| wrapper | `(deftype id u64)` | `(id 5u64)` | pattern `(id n)` |

Structural types can be written inline: `(tuple u64 str)`,
`(record name str)`, `(enum yes void no void)`, `(union i32 str)`, and
`(fn (u64) bool)`. Nominal types with the same structure are still different
types. There is no implicit numeric widening, null, or truthiness.

`(fn (a b) r labelled: (n t) variadic: (array t) effects: (row))` is a
function type. Two function types are equal only when their effect rows are
equal.

## 6. Generics

```vibra
(defn first (items (array t) fallback t) t
  where: (t any)
  (let-else (option.some head) (items 0u64) (return fallback))
  head)

(defn biggest (items (array t) fallback t) t
  where: (t ordered)
  (array.fold
    items
    fallback
    (lambda (best t item t) t
      (match (ordered.compare item best) (core.ordering.greater) item - best))))
```

Each generic name appears once in `where:` with its bound. The bound is
either `any` (no requirement) or a **bare** interface name. A dotted bound
such as `core.ordered` is `@syntax.invalid-form` ("generic bounds must be
symbols"). `ordered`, `equatable`, and `iter` are in the prelude, so they need
no import. Bound by another module's interface by importing it as a
declaration, such as `(import from @std.core.from)`, or use a `defint` from the
same module. Type arguments are inferred. When inference can't decide, pass
`types: (i32)` as a labelled operand at the call:
`(mapper.map-to types: (i32) box f)`. A `dict` key type must implement
`ordered`.

## 7. Interfaces (`defint` / `impl`)

```vibra
(defint priced
  visibility: @public
  (defn cents (value self) u64)                 ; abstract: no body
  (defn above (value self limit u64) bool       ; default method: has a body
    (match (ordered.compare (priced.cents value) limit) (core.ordering.greater) true - false)))

(deftype part (record name str unit-cents u64 count u64)
  visibility: @public
  (impl priced
    (defn cents (value self) u64 (times (value @unit-cents) (value @count)))))
```

- An `impl` goes inside the `deftype` that owns the type, or inside the
  `defint` that owns the interface (for example, to implement your interface
  for `i32`). There is no top-level `impl`.
- An implementation supplies every abstract member and must not redefine a
  default.
- Call a member through the interface path: `(priced.cents x)`. Interface
  values (the interface written as a type) dispatch at run time.
- Standard interfaces: `equatable` (`equal`), `ordered` (`compare`, which
  returns `core.ordering`), and `iter` (`next`) are in the prelude: implement
  them, bound by them, or call them with no import. `@std.core.from` and
  `@std.core.try-from` need an import as a declaration under their own name,
  such as `(import from @std.core.from)`. The builtin integers, `char`, `bool`,
  `str`, and `bytes` implement `equatable` and `ordered`; there is no
  `u64.equal` or `str.compare`.
- Implementing `iter` for your type makes the adapters `map`, `filter`,
  `skip`, `take`, and `collect` available on it:

```vibra
(deftype countdown (record from u64)
  (impl (iter u64)
    (defn next (value self) (option (tuple u64 self))
      (match
        (u64.sub (value @from) 1u64)
        (result.ok lower) (option.some (tupleof (value @from) (countdown from: lower)))
        (result.err -) (option.none)))))
```

## 8. Conversion (`from` / `try-from`)

```vibra
(import from @std.core.from)

(import try-from @std.core.try-from)

(as u32 (from.convert 42u8))                                  ; lossless
(as (result u8 core.conversion-error) (try-from.convert 300u32)) ; may fail
```

The destination is chosen by the expected type, whether that is a written
result type, a parameter type, or an `as`; with none, the call is
`@type.ambiguous-destination`. Implement `(impl (from i16) ...)` on your own
type to make it a conversion destination. These are the only conversions:

- between integer types, `from` when the destination holds every source value
  and `try-from` otherwise;
- parsing, `(try-from str)` on every number type: `(try-from.convert "42")`;
- rendering, `(from n)` on `str` for every number type: `(from.convert 42u64)`
  where a `str` is expected.

## 9. Unions and `as`

```vibra
(deftype number (union i32 str))

(defn render (value number) str
  (match value (as i32 n) (from.convert n) (as str text) text))
```

Construct with `(number 5i32)` or widen with `(as number 5i32)`. An
unsuffixed literal that fits more than one member is ambiguous. A union of
error types is the usual way to combine failures, and the union constructor
injects a member: `(result.err (app-error cause))`.

## 10. Effects and `deffect`

- Rows hold symbols that resolve through imports: `io.stdout` means root
  `stdout` of the module imported as `io`. Writing an atom in a row is
  `@effect.invalid-reference`.
- The checker computes the transitive performed row and requires it to fit
  the written ceiling (`@effect.outside-ceiling`). Calling through a function
  value contributes the row of its type. Calling an interface member
  contributes the contract's row.
- An implementation member writes exactly its contract member's row.
- A `def` initializer must be pure.

User effects are nominal roots whose operations have Vibra bodies:

```vibra
(import io @std.io)

(deffect log
  (defn line (message str) (result void io.io-error)
    effects: (io.stdout)
    (io.stdout.text message)))

(defn main () (result void io.io-error)
  effects: (io.stdout log)
  (log.line "hi\n"))
```

An operation performs its owner root (`log`) and its additive row
(`io.stdout`), so its callers declare both. The target's effects array names
the user root by its full path, for example
`effects: (array @app.main.log @std.io.stdout)` when this module is
`src/app/main.vib` of target `@app`. A `deffect` takes no `effects:` of its
own, and user code cannot declare `external: @host`.

## 11. Modules, imports, and visibility

- Module identity is the target name plus the path under the target root,
  without `.vib`. For example, `src/app/net/http.vib` under root `src/app` is
  `@app.net.http`. Every path segment must be a kebab name.
- `(import alias @unit.path)` imports a module. Use its public members as
  `alias.name`, and effect operations as `alias.root.op`.
  `(import ordering @std.core.ordering)` imports one public declaration
  directly.
- The units are each local target (`@app`, `@stats`, ...), `@std`, and
  `@tests` (test modules only). There are no relative, string, glob, or
  re-export imports, and import cycles are errors.
- Aliases may not be builtin type names, so `(import bool @std.bool)` is
  rejected. Use an alias such as `logic`.
- These are in scope without an import: the primitive types and their
  members (`u64.add`, `f64.mul`, `char.is-ascii-digit`, ...), `array` and
  `dict` with their members (`array.of`, `array.fold`, `dict.of`, ...),
  `option`, `result`, `bool`, `str` and `bytes` with their members
  (`str.concat`, `bytes.length`, ...), the interfaces `equatable`, `ordered`,
  and `iter`, and `true` and `false`. Everything else needs an import,
  including `core.ordering` (`@std.core`) and `from` and `try-from`
  (`@std.core.from`, `@std.core.try-from`).
- Everything is private unless it declares `visibility: @public`. This applies
  to methods and effect operations individually. Importing a private
  declaration is `@name.private-access`.

## 12. Canonical formatting

`vibra fmt FILE --write` produces the only canonical layout. It uses two-space
indentation, one blank line between top-level forms, lines of up to 88
columns, and attributes in canonical order. Always run it instead of
hand-formatting. It also formats `project.vibon`. If the file has a syntax
error, the formatter leaves it byte-for-byte unchanged.
