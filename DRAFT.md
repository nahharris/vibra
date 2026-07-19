# Vibra language specification (draft)

Vibra is a functional, vibe-coding-first language: **YAML surface** (strict subset), **static types** (TypeScript familiarity + Rust-ish safety), compiles to **WebAssembly**, and favors **one obvious form** per construct so LLMs make fewer choices.

The **`vibra` CLI** in this repo implements a **reference subset** for bootstrapping (see [README.md](README.md)); the sections below remain the **authoritative language design**.

---

## 1. Design principles

- **LLM-first surface:** Regular, tree-shaped YAML; reserved compile-time keys use a **`$` prefix**; same invocation shape for intrinsics and **qualified imports** (`$io.println`).
- **One way to do things:** Normative rules in §4; violations are **linter errors** (see `schemas/linter-codes.json`).
- **No export lists:** **Public** symbols are top-level keys **not** starting with `-`. **Private** symbols start with `-` and are invisible to importers.
- **Minimal host, maximal `$wasm`:** The Rust compiler implements parse → import graph → macro expansion → typecheck → emit; **stdlib** should wrap **`$wasm`** instead of growing the host.
- **Functional core:** Functions are values; **no** separate method syntax—interface members are **names → types**, including function types.

---

## 2. Normative YAML subset

**File extension:** `.vibra` (optional alias `.vibra.yaml`).

**Parser:** YAML **1.2** core schema for scalars, with the following **additional restrictions** (reject with diagnostic code `E-YAML-001` …; see `schemas/linter-codes.json`).

| Rule | Normative |
|------|-----------|
| Root | Exactly one **mapping** at document root (the module). |
| Forbidden | Anchors/aliases (`&`, `*`), merge keys (`<<`), explicit tags (`!!`), `!!binary`, timestamp tags as language values. |
| Keys | String keys only; symbol names **kebab-case** at module level (see §3). |
| Comments | YAML `#` comments are forbidden (`E-YAML-002`). Use an attached `=comment` annotation. |
| Sequences | **Block sequences** (`- item`) are **canonical** for statement lists (`do:`). **Flow sequences** allowed only inside **type** positions where the spec shows `[$t1, $t2]` (tuple types). |
| Strings | User-visible text and symbol-like content: **double-quoted** in examples; unquoted scalars that could parse as `true`/`false`/`null`/number are **errors** (`E-YAML-003`). |
| `$` in strings | Leading `$` in a string literal: escape as **`$$`** (carries over from early draft). |

---

## 3. Module grammar

- A **module** is a YAML mapping: keys are **symbol names**, values define that symbol.
- **Privacy:** Keys beginning with **`-`** are **module-private** (not visible through `$import`). There is **no** `export` keyword or export list.
- **Entrypoint:** The symbol **`main`** is the module entrypoint when the module is used as a program.
- **Imports:** A single form. Top-level key = **alias**; value is a mapping with reserved key **`$import`** whose value is a **string path** to another module (relative or resolved per compiler host).

```yaml
io:
  $import: ./io.vibra
-main-helper:
  $function: $void
  return: $void
  do:
    - $io.println: "internal"
main:
  $function: $void
  return: $void
  do:
    - $io.println: "hello"
```

- **Qualified calls:** **`$alias.symbol`** resolves to public `symbol` in the module bound to `alias`. Same **invocation** shape as `$println` (mapping or scalar argument per callee).
- **Direct imports:** Every imported alias referenced by a module must be declared in that module. Imports are not re-exported or inherited transitively (`E-MOD-004`).
- **Module identity:** Compiler resolves paths to a canonical file URL or path; **cycles** in the import graph are **errors** (`E-MOD-003`).

---

## 4. One-way expression discipline (normative)

These rules are **mandatory** for conforming Vibra v1 source. Tooling **must** flag violations.

| ID | Rule |
|----|------|
| E-ONE-001 | **`$function` declarations** use canonical labeled shorthand: `$void` only for zero arguments, `$self` for a method receiver, or exactly one labeled primary argument mapping. Additional arguments belong in sibling `args:`. Nested `{ args, return, do }`, implicit scalar primary arguments, `$void` plus `args:`, and type-constructor wrapper primaries are forbidden. |
| E-ONE-002 | **`$let`:** at most **one** bound name per `$let` mapping (single key → value, optional single type ascription form). Chain with `$do` for multiple bindings. |
| E-ONE-003 | **`$function.do`:** value is a **block sequence** of expressions only. **Do not** wrap the body in an extra `$do` node inside `do:`. |
| E-ONE-004 | **Sequencing** elsewhere: use **`$do`** with a block sequence of expressions; last item’s value is the result. |
| E-ONE-005 | **Conditionals:** only **`$if`** with keys **`is`**, **`then`**, **`else`** (all required). Branches must unify to the same type. |
| E-ONE-006 | **String interpolation** in user code: **forbidden** unless a single stdlib macro documents it; prefer explicit concat (or a single blessed `$format` when defined). |
| E-ONE-007 | **`$match`:** use **`$match: <expr>`** with sibling **`when:`** arms; structured **`$match: { target, arms }`** is non-canonical. |
| E-ONE-008 | **`$match` arms:** use **`case:`** for the arm pattern; legacy **`pattern:`** is forbidden. |

**Reference inside function body:** Use **`$args.field`** for argument fields (record args).

---

## 5. Expression core

### Invocations

A **prefixed symbol** is a YAML key starting with `$` (after merge into a mapping). The key selects the **callee**; the value is the argument (mapping, sequence, or scalar).

- **Unqualified:** `$println`, `$function`, `$add`, etc.
- **Qualified:** `$io.println` — treats `io` as import alias and `println` as symbol in that module.

After `$`, the callee name must be an **identifier** (letters, digits, `-` per symbol rules for unqualified names). **`$+` and other punctuation-only** keys are **invalid** (`E-SYN-001`); use e.g. **`$add`** for addition.

For zero-arg functions (`args: $void`): a bare symbol reference like `$that-func` denotes the function value, while invocation is explicit and canonical as `{ $that-func: null }`.

### `$function`

Creates a function. The `$function` value declares the primary argument: `$void` for no arguments, `$self` for a method receiver, or a singleton labeled mapping. Additional arguments use sibling `args:`. Every argument is referenced through `$args.<name>`.

```yaml
$function:
  x: $int64
args:
  y: $int64
return: $int64
do:
  - $add: [$args.x, $args.y]
```

Keys: **`$function`** (canonical primary argument), optional **`args`** (additional named arguments), **`return`** (type), **`do`** (block sequence of expressions).

### `$let`

Single binding per §4: the mapping has **exactly one** key (the new symbol); the value is the initializer expression. Type is inferred or fixed by the enclosing context; explicit ascription uses **`$as`** when needed:

```yaml
$let:
  a:
    $as: $int64
    $init: 2
```

Simple inference:

```yaml
$let:
  a: 1
```

Chained lets use **`$do`**:

```yaml
$do:
  - $let:
      a: 1
  - $let:
      b: 2
  - $add: [$a, $b]
```

### Mutable values and references

Values are immutable and copied by default. `$mut` is an explicit expression and
type wrapper that allocates mutable storage, while `$set` updates one existing
writable symbol using the same singleton mapping shape as `$let`:

```yaml
- $let: {count: {$mut: 0}}
- $let: {snapshot: $count}
- $set: {count: 1}
```

Reading `$count` copies its current inner value, so `snapshot` remains `0`.
`{$mut: $count}` reuses existing mutable storage; wrapping an ordinary expression
allocates a new program-instance arena cell.

`$ref` creates a transparent reference. A plain target creates a read-only
reference, while wrapping the target in `$mut` creates or reuses writable storage:

```yaml
- $let: {reader: {$ref: $count}}
- $let: {writer: {$ref: {$mut: $count}}}
- $let: {temporary: {$ref: {$mut: 0}}}
- $set: {writer: 2}
```

Reading a reference copies its pointee. `$set` through a mutable reference writes
the pointee; no separate dereference syntax exists. The corresponding type forms
are `{$mut: T}`, `{$ref: T}`, and `{$ref: {$mut: T}}`. Mutable values may copy
into plain `T` positions, but creating mutable storage or references is always
explicit. Mutable cells and references may be passed, returned, or stored in
composites. v1 uses program-instance arena lifetime and permits aliases in its
single-threaded execution model.

### `$if`

```yaml
$if:
  is: $args.positive
  then: $args.a
  else: $args.b
```

### Value constructors

Composite values use explicit forms in expression context:

```yaml
$record: { name: "Ada", age: 37 }
$tuple: ["ok", 1]
$array: ["a", "b"]
$map:
  - key: "lang"
    value: "vibra"
```

### Core collection contract

`stdlib/src/collections.vibra` is the single canonical operational surface for
v1 arrays and maps. Arrays are generic ordered values. The initial map is a
generic-value, `$str`-keyed map; broader key types require a future explicit
equality contract and are not inferred implicitly.

Collection functions never mutate an input value. `set`, `append`, `insert`,
and `remove` return a copied collection, preserving Vibra's value-copy model.
Array lookup returns `option<T>`; invalid bounds from modifying or slicing
operations return `result<_, collection-error>`. Appending and inserting use
the runtime `max-alloc-len` ceiling and return `limit-exceeded` instead of
trapping. Array slices use the half-open range `[start, end)`.

Maps preserve first-insertion order deterministically. Inserting an existing
key replaces its value at the same position; the same last-value-wins rule
canonicalizes duplicate keys in a literal. Missing lookup returns `none`, and
removing a missing key returns `none` (the caller already retains the original
value), while a successful removal returns `some<map>`. `$for` exposes this order.

### Canonical traversal and ranges

Traversal has one statement form: `$for: <binding>` with required sibling
`in:` and `do:` keys. The source expression is evaluated once and copied before
the first iteration, so body mutation cannot change the traversal in progress.

```yaml
- $for: number
  in:
    $range: {start: 0, end: 10, step: 1}
  do:
  - $if: {$equal: [$number, 5]}
    then:
    - $break: null
    else: []
```

`$range` is half-open and contains exactly `start`, `end`, and `step`, all
`$int64`. Positive steps advance below `end`; negative steps advance above it.
Direction mismatches produce an empty traversal, zero step is `E-ITER-002`,
checked addition prevents overflow, and traversal length is bounded by the
runtime `max-alloc-len` limit.

Arrays yield copied elements. `$str` yields one Unicode scalar represented as a
`$str`, deliberately neither byte nor grapheme-cluster iteration. String-key
maps yield `$tuple: [key, value]` entries in first-insertion order. `$break:
null` and `$continue: null` are valid only within `$for` or `$while`, target the
nearest loop, and are the sole early-loop-control forms. Iteration bindings and
body-local bindings do not escape the loop.

`$str` values are always valid UTF-8. `text.scalar-len`, `text.scalar-at`, and
`text.find` use Unicode-scalar units, matching `$for`; `text.byte-len` is the
explicit UTF-8 byte measurement. No API exposes an ambiguously named string
index. Grapheme clusters and locale-sensitive collation/casing are not v1
units. `bytes.decode-utf8` returns a typed `invalid-utf8` error and there is no
lossy decoding operation.

### `$match`

`$match` uses one canonical ordered arm sequence: the target expression is the `$match` value, and sibling `when:` contains the ordered arms. Each arm has `case` and `do`.

```yaml
- $match: $result
  when:
    - case:
        $result.result.ok:
          $bind: value
      do:
        - $io.println: $value
    - case:
        $result.result.err:
          $bind: err
      do:
        - $io.eprintln: "failed"
```

Pattern forms are scalar literals, enum-constructor patterns, `$record`, `$tuple`, `$array`, `$map`, `$newtype`, `$interface`, `{ $bind: name }`, and `{ $wildcard: null }`. Matches must be total: enum matches cover all tags or include wildcard; open-ended targets such as strings, numbers, records, arrays, and maps require wildcard unless a single literal target is trivially exhaustive. Bindings introduced in an arm are scoped to that arm and do not leak after the match. Runtime interface patterns use nominal `=impl` satisfaction.

### Primitive operations

Primitive operations are compiler intrinsics and use the same canonical named-call
shape as functions. Binary operations take exactly one two-item block sequence;
unary operations take their operand directly. There are no symbolic aliases.

| Family | Canonical forms | Operand / result rule |
|---|---|---|
| Arithmetic | `$add`, `$subtract`, `$multiply`, `$divide`, `$remainder` | Two identical numeric types; result has that type |
| Unary numeric | `$negate` | Signed integer or float; result has the operand type |
| Comparison | `$equal`, `$not-equal`, `$less-than`, `$less-or-equal`, `$greater-than`, `$greater-or-equal` | Identically typed numerics, or strings; equality also accepts booleans; result is `$bool` |
| Boolean | `$and`, `$or`, `$not` | `$bool`; result is `$bool` |
| Bitwise | `$bit-and`, `$bit-or`, `$bit-xor`, `$bit-not` | Identically typed integers; result has that type |
| Shifts | `$shift-left`, `$shift-right` | Identically typed integers; result has the left operand type |
| Conversion | `$convert: <number>` with siblings `into: <numeric-type>` and `or: <literal>` | Exact numeric conversion, or the statically representable fallback |

No implicit numeric widening or narrowing occurs: mixed primitive types are
`E-OP-001`, and callers must perform an explicit supported conversion. Integer
arithmetic is checked and reports `E-OP-002` on overflow. Integer division and
remainder by zero report `E-OP-003`; the signed minimum divided by `-1` is
overflow. Shift counts must be in `0..bit-width` and otherwise report
`E-OP-004`; signed right shift is arithmetic. Boolean operations do not
short-circuit because both operands are expressions evaluated before the
intrinsic.

`$convert` is the only primitive numeric-conversion form. It never traps and
never silently loses precision: integer targets require an in-range integral
source; integer-to-float and float narrowing require exact representation.
Failure yields the required `or` literal, which the compiler verifies is
exactly representable by the target type. The result always has the `into`
type. Use richer stdlib parsing and result APIs when callers need an error
reason rather than a deterministic fallback.

Floating arithmetic follows IEEE 754 at the declared width (`$float32` rounds
each result to binary32). Division by zero produces the corresponding infinity
or NaN. NaN is unequal to every value including itself, and every ordered
comparison involving NaN is false. String ordering compares Unicode scalar
values lexicographically; it is locale-independent.

```yaml
$add:
- $args.x
- $args.y
```

### `$cast`

Explicitly crosses a `$newtype` boundary.

```yaml
$cast: $args.raw
into: $path
```

In v1, casts are allowed only for the two directions between a `$newtype` and its declared inner type. Transparent aliases already coerce implicitly, and other semantic conversions must be expressed through explicit conversion interfaces such as `$from.from` or `$into.into`. All other casts are invalid (`E-CAST-001`). `$cast` attaches runtime type metadata so `$newtype` and nominal `$interface` patterns can test the value later; primitive host operations still consume the inner representation. `$cast` cannot target `$capability` types or aliases whose body is `$capability` (`E-CAP-001`).

### `$do`

```yaml
$do:
  - $let:
      a: 1
  - $let:
      b: 2
  - $add: [$a, $b]
```

---

## 6. Type system

### Primitives

`$int8/$int16/$int32/$int64`, `$uint8/$uint16/$uint32/$uint64`, `$float32/$float64`, `$bool`, `$void`, `$str`.

**Reserved type `$self`:** A self-reference to the enclosing type. Valid **only** in two contexts:

- Inside an `$interface` body — an existential placeholder bound to each implementing type at impl time (Rust's `Self`-in-trait semantics).
- Inside a type's `=defs` or `=impl` annotation (introduced in later phases) — resolves to the enclosing type during lowering.

Anywhere else (record fields, free-standing function signatures, generic instantiations, top-level type bodies, …) `$self` is a parse-time error (`E-SELF-001`).

### Constructors (YAML forms)

| Form | Meaning |
|------|---------|
| `$literal` | Literal type: `{ $literal: "ok" }` |
| `$newtype` | Nominal wrapper: `{ $newtype: T }`. Unlike transparent aliases, a `$newtype` is distinct from `T` and crosses to/from `T` only through `$cast`. |
| `$policy` | Opaque runtime-injected authority type. User code cannot mint policy values and may only attenuate them through explicit narrowing. |
| `$record` | Concrete product: `{ $record: { f: T, ... } }` |
| `$map` | Homogeneous map: `{ $map: { key: K, value: V } }` |
| `$tuple` | Tuple of types: `{ $tuple: [$t1, $t2] }` — **type positions only** |
| `$array` | Homogeneous array type: `{ $array: T }` |
| `$union` | `{ $union: [T1, T2, ...] }` — discriminated unions should use a **tag** field in `$record` variants |
| `$intersect` | `{ $intersect: [T1, T2] }` — compose interface requirements |
| `$interface` | **Go-like structural interface:** `{ $interface: { name: T, ... } }` — each member is a **type**; function members use **`$fn-type`**. Inside the body the reserved `$self` type stands for the implementing type. |
| `$fn-type` | `{ $fn-type: { args: { $record: ... }, return: R } }` — **one** function type constructor |

**Aliases vs newtypes.** Top-level type definitions using the structural constructors (`$record`, `$tuple`, `$array`, `$map`, `$union`, `$enum`, `$interface`, `$intersect`, `$fn-type`, `$literal`) remain transparent aliases: a value of the alias body can flow where the alias is expected, and vice versa, subject to normal compatibility rules. A top-level definition using `$newtype` is nominal: the alias name is the type identity, not the body. Implicit coercion between a newtype and its inner type is rejected (`E-NEWTYPE-001`); use `$cast` explicitly. Numeric and other non-newtype casts are out of scope for v1.

**Generics — `=where` annotation (v1):** Generic type parameters are declared at the **module-symbol level** via the `=where` annotation (see §13). The mapping's key order defines the positional order of type parameters. A bound list is a sequence of interface references (`$some-iface`, `$mod.iface`, or `$intersect` of those); the substituted type at every call site and type-position instantiation must have an explicit `=impl` block for each iface in the list (`E-BOUND-001`). Empty list `[]` means unbounded. `=where` is valid alongside any type-form key (`$newtype`, `$enum`, `$union`, `$record`, `$tuple`, `$array`, `$map`, `$intersect`, `$interface`, `$fn-type`, `$literal`) **and** alongside `$function`. Type-parameter names are in scope for the form value (function `args` / `return` / `do:` body, or the type expression body).

**Use-site instantiation (v1):** Every reference to a **generic type alias** at a type position must be an explicit instantiation: `{ $alias: { tparam: T, ... } }`. A bare `$alias` reference for a generic alias is an error (`E-GEN-001`). Non-generic aliases continue to be referenced as bare `$alias`. Mismatched arity, unknown parameter names, or missing parameters at instantiation are `E-GEN-002`.

**Generic functions (v1):** `$function` may carry `=where`. Type parameters in `=where` are in scope for `args`, `return`, and the function body. Both **entry** `.vibra` modules and **imported** modules may define user-bodied functions (normal `do:` sequences) or stdlib-style functions whose `do:` is exactly one `$wasm` statement.

**Calls to generic functions — explicit type arguments:** The call payload is a single mapping whose keys are the **names from `=where`** (type arguments, values are type expressions like `$int64`) plus the function's **value argument** names. Example: `{ $identity: { t: $int64, x: 7 } }`. Every type parameter must appear; there is no inference in v1.

**Generic enum constructors — payload-driven inference:** Constructors at value sites (`$m.result.ok: 7`) infer their type arguments from the payload. This applies only at value sites; type-position uses still require explicit instantiation.

**`$return` (user functions):** User-defined functions (non-`main`) with a non-`$void` return must terminate by **`$return: <expr>`** as the last statement of the function body, or by **`$match`** whose every arm’s `do:` ends with `$return` in the same sense. Functions with `return: $void` may omit `$return`. **`$return` is not allowed in `main`.**

**Null safety (v1):** `null` is valid only for type `$void`, and is the only source-level value of `$void`. Optional values use the tagged generic enum from `stdlib/src/option.vibra`; raw payloads and `null` do not coerce into option values. Construct `$option.option.some: <value>` or `$option.option.none`, then narrow with `$match`.

### Interface satisfaction

Vibra distinguishes **two** ways an `$interface` is matched. They look similar but apply in different contexts and do not subsume each other.

**Structural satisfaction (used as a type).** A `$record` type **structurally satisfies** an `$interface` type when, for every member `n: T` in the interface, the record has a field `n: U` with `U` a subtype of `T` per v1 rules (width subtyping toward the interface for records). This is the rule that lets a value flow into a position annotated with an interface type — function arguments, return types, record fields, etc. No `=impl` block is required. This is the existing Go-like behavior.

**Nominal satisfaction (used as a bound or as a dispatch target).** A type **nominally satisfies** an interface only when its definition includes an explicit `=impl: { $iface: ... }` block (see §13.2). Nominal satisfaction is what `=where` bounds and interface-qualified dispatch (`$iface.method`) require:

- A type argument passed to a `=where`-bounded generic parameter (`E-BOUND-001`).
- The `$self`-typed dispatch argument of an interface-qualified call (also `E-BOUND-001`).

Structural satisfaction is **not enough** to clear a `=where` bound or be a dispatch target. The asymmetry is intentional: structural matching is convenient at value sites where the relationship is local and obvious, while bounds and dispatch demand a coherent, opt-in registration so the impl table can be populated and the orphan rule can be enforced.

| Context | Required | Mechanism |
|---------|----------|-----------|
| Type-annotated parameter / return / field | structural | width subtyping |
| `=where: { t: [$iface, ...] }` bound | nominal | `=impl: { $iface: ... }` |
| Interface-qualified call (`$iface.method`) | nominal | `=impl: { $iface: ... }` |
| Type-qualified call (`$type.iface.method`) | nominal | `=impl: { $iface: ... }` |

**Variance:** **v1:** function types are **invariant** in arguments and **covariant** in returns unless the compiler documents otherwise (`E-TY-VARIANCE` audit).

---

## 7. Standard library surface (builtins whitelist)

**Host-reserved** keys (expand before user macros where applicable):

- **Module system:** `$import` (compile-time only, appears only under import alias mapping).
- **Core:** `$function`, `$let`, `$mut`, `$set`, `$ref`, `$if`, `$do`, `$macro`, `$wasm`, `$return`, `$as` (type ascription for `$let`), `$cast`.
- **Types:** primitive symbols and `$newtype`, `$record`, `$array`, `$fn-type`, `$interface`, `$union`, `$enum`, etc.

**Effectful** IO and host calls should live in **`stdlib`** modules implemented atop **`$wasm`**, e.g. `io.println`, not as unlimited new host opcodes.

---

## 8. Metaprogramming: `$macro` and `$wasm`

### `$macro`

- Declares **compile-time** expansion from surface AST → core AST.
- **Staging:** **After parse, before typecheck** unless a single documented **typed** stage is added later.
- Uses a function-shaped declaration with one labeled syntax input, an explicit
  syntax-category return, and a deterministic `do` body.
- `$quote`, `$unquote`, and `$splice` compose structural syntax values.
  Introduced bindings are hygienic; `$capture` is the explicit caller-scope
  escape hatch.
- Compile-time execution has no `$wasm`, policy, filesystem, environment,
  network, clock, or randomness authority.
- Expansion limits are 64 nested invocations, 1,000,000 evaluation steps, and
  100,000 generated nodes. Typed/post-typecheck macros are not part of v1.

### `$wasm`

- **Intrinsic** node carrying **opaque WASM** or a **structured opcode list**—**exactly one** encoding is enabled per compiler build (see §10).
- Every `$wasm` occurrence must have a **fully explicit** type signature in the typed IR (no implicit unsafe).

**v1 structured stub (WASI):** import module + function name (no ad-hoc `env.*` host for stdio):

```yaml
$wasm:
  import:
    module: wasi_snapshot_preview1
    name: fd_write
  args:
    - $const.1
    - $args.msg
```

Current compiler behavior validates stdlib signatures and forwards call-site arguments into declared `$wasm.args` entries (`$args.*`/`$const.*`) before execution.

### Imports as directives

`$import` is resolved at **compile time** but uses the **same `$`-keyed mapping** style as other builtins; use sites remain **`$alias.symbol`**.

---

## 9. Bootstrap architecture

**Rust host (minimal):**

1. Parse YAML subset → surface AST.
2. Build **import graph**, detect cycles.
3. Expand **`$macro`** (trusted + optional user) to **core** AST.
4. Typecheck core.
5. Lower **`$wasm`** + core → WASM.

**Vibra-written layers:**

- **`lang/`** — macro tables, optional sugar (`function:` → `$function` if desired).
- **`stdlib/`** — `io`, math, etc., mostly **`$wasm`** wrappers.

**Bootstrap seed:** First toolchain may **embed** a snapshot of `lang/core` until load-from-disk is stable.

---

## 10. WASM target (v1)

**Pipeline:** YAML → surface AST → expanded core → typed IR → **wasm32** (MVP).

**Memory:** **Linear memory** + **bump/arena** allocator strategy recommended for v1; no GC requirement.

Plain scalar values use wasm locals or direct parameters. Strings, arrays, and
maps use `{ptr: i32, len: i32}` descriptors; records and tuples use aligned
field layouts; enums use an `i32` tag followed by an aligned payload. Mutable
values that are address-taken, returned, stored in composites, or forwarded as
references are promoted to aligned bump-arena slots. Both `mut<T>` handles and
references are represented as `i32` addresses. `$wasm.args` forwards ordinary
scalars directly, copied aggregates by pointer, and explicit mutable/reference
values as their arena address. The v1 arena is reclaimed only when the program
instance exits.

**WASI imports (`wasi_snapshot_preview1`, preview1):**

| Import | Signature (wasm32) | Notes |
|--------|-------------------|-------|
| `fd_write` | `(i32 fd, i32 iovs_ptr, i32 iovs_len, i32 nwritten_ptr) -> i32` | errno; UTF-8 via `ciovec` list in linear memory |
| (others) | per [WASI preview1](https://github.com/WebAssembly/WASI/blob/main/legacy/preview1/docs.md) | `stdlib/src/fs.vibra` lists representative names |

The embedded runner uses **wasmer-wasix** (requires a Tokio 1.x runtime). **Preopened directories** map host paths into the guest; stdio does not require preopens.

**Security policies:** Authority roots receive aggregate, unforgeable `$policy`
values. `$policy.narrow` produces a domain-specific `$capability.<domain>` value;
privileged helpers accept only those narrowed values. The runtime checks dynamic
targets against the live capability at use. Policy groups may mix mandatory and
optional scopes, and filesystem scopes use canonical ancestry.

**Typed host boundary:** `$wasm` declarations bind only to the closed,
versioned host registry. Complete parameter, capability-domain, and return types
are checked statically; wrapper declarations confer no authority.

**`$wasm` encoding (pick one per build):**

- **A)** **Structured list** of opcodes + locals + types (preferred for tooling), or
- **B)** **Opaque** WASM fragment + type signature.

The other mode is **disabled** in v1 builds (`E-WASM-001` if wrong form).

**Unsupported in v1 (non-exhaustive):** threads, exception handling, GC proposal, SIMD (unless explicitly enabled).

---

## 11. Tooling and diagnostics

- **Schemas:** See [`schemas/`](schemas/) — including
  `code-path.schema.json`, `code-form.schema.json`, `code-query.schema.json`,
  `code-change-set.schema.json`, `macro.schema.json`, and the diagnostic,
  module, function, type, and expression schemas.
- **Stable errors:** Each diagnostic has **`code`**, **`message`**, **`severity`**, **`span`**, optional **`related`**, optional **`fix`** (JSON Patch RFC 6902).
- **Structural tooling / `vibra code`:** Queries return canonical key/index
  paths, node fingerprints, forms, source, and semantic metadata. Transactions
  require exact document revisions and fingerprints; line/column is diagnostic
  metadata only and is never an edit locator.

**Annotation / generics codes (added with §13):**

| Code | Severity | Summary |
|------|----------|---------|
| `E-MOD-004` | error | A module references an import alias that it does not declare directly. Imports are not re-exported transitively. |
| `E-ANNO-001` | error | Unknown annotation key on a definition (recognised `=`-prefixed annotations: `=doc`, `=where`, `=defs`, `=impl`). |
| `E-ANNO-002` | error | Legacy un-prefixed annotation key (`where:`, `doc:`); v1 annotations must use the `=` prefix (rename to `=where`, `=doc`). |
| `E-WHERE-002` | error | `=where` bound list element does not resolve to an interface (or `$intersect` of interfaces). |
| `E-BOUND-001` | error | A generic call site or type-position instantiation passes a type argument that does not satisfy its declared `=where` bound (no matching nominal `=impl`). Also raised by interface-qualified dispatch when the dispatch argument's type has no `=impl`. |
| `E-CALL-IFACE-NOSELF` | error | Interface-qualified call (`$iface.method`) targets a method with no `$self`-typed argument; use the type-qualified form. |
| `E-DISPATCH-001` | error | Interface-qualified call's `$self` argument has a generic static type. Pending monomorphisation. |
| `E-DOC-001` | error | `=doc` annotation must be a string scalar. |
| `E-MUT-001` | error | Malformed `$mut` expression or type wrapper. |
| `E-SET-001` | error | `$set` is not a singleton symbol-to-value mapping. |
| `E-SET-002` | error | `$set` targets an unknown, immutable, or read-only symbol. |
| `E-SET-003` | error | `$set` value is incompatible with the target pointee type. |
| `E-REF-001` | error | Malformed `$ref` expression or type wrapper. |
| `E-REF-002` | error | `$ref` target cannot be resolved. |
| `E-REF-003` | error | Reference access mode is invalid for its target. |
| `E-GEN-001` | error | Bare reference to a generic type alias requires explicit instantiation. |
| `E-GEN-002` | error | Generic alias instantiation is malformed (unknown alias / param, missing param, arity mismatch). |
| `E-NEWTYPE-001` | error | Implicit coercion between a `$newtype` and its inner type is forbidden; use `$cast`. |
| `E-NEWTYPE-002` | error | Malformed `$newtype` definition body. |
| `E-CAST-001` | error | `$cast` has no valid v1 cast path between source and target types. |
| `E-CAST-002` | error | Malformed `$cast` payload; expected `$cast: <expr>` with sibling `into: <type>`. |
| `E-CAP-001` | error | Capability values are runtime-minted and cannot be created with `$cast` or literals. |
| `E-SELF-001` | error | Reserved `$self` type used outside an `$interface` body or a type's `=defs` / `=impl` annotation. |
| `E-DEFS-001` | error | Invalid `=defs` annotation (placed on a non-type definition, entry is not a `$function`, or duplicate name). |
| `E-IMPL-001` | error | Invalid `=impl` annotation (non-type definition, malformed payload, or method binding that is neither a `$function` envelope nor a qualified function alias). |
| `E-IMPL-002` | error | `=impl` keyed by an alias that does not resolve to a registered `$interface` type. |
| `E-IMPL-003` | error | `=impl` block missing a binding for one of the interface's `=where` type-parameters or one of its methods. |
| `E-IMPL-004` | error | `=impl` payload contains an unexpected key (not `=where`, an iface type-arg, or an iface method name). |
| `E-IMPL-005` | error | `=impl` method signature does not match the interface declaration (after `$self` and iface type-arg substitution). |
| `E-IMPL-006` | error | `=impl` method alias does not resolve to a registered function. |

---

## 12. Hello world (updated)

```yaml
io:
  $import: ./stdlib/src/io.vibra
main:
  $function: $void
  return: $void
  do:
    - $io.println: "Hello, World!"
```

(Early examples used bare `$println`; **normative** style is **stdlib via `$import`**, with `io` wrapping **`$wasm`** host glue per §9.)

---

## 13. Annotations (`=doc`, `=where`, …)

A top-level symbol's value is a **definition envelope**: a mapping with **exactly one** `$`-form key (`$function`, `$import`, or one of the type constructors in §6) and **zero or more** `=`-prefixed annotation siblings. In addition to semantic definition annotations, every source mapping may carry ignored `=comment` and tooling-only `=lint` annotations.

> **Annotation prefix is normative.** Every annotation key starts with `=`. The pre-1.0 spelling without the prefix (`where:`, `doc:`) is **rejected** with `E-ANNO-002` (rename to `=where`, `=doc`).

| Annotation | Value | Purpose |
|------------|-------|---------|
| `=doc` | `$str` (YAML `|` block scalar recommended for multiline markdown) | Compile-time documentation attached to the symbol. Stored on the lowered `FunctionSig` / `TypeAlias`; not yet emitted to runtime or LSP output. |
| `=comment` | `$str` | Ignored source commentary. It must share the mapping containing the syntax it describes and is retained only by structural tooling. |
| `=lint` | `{ disable: [CODE, ...] }` | Suppresses matching lint diagnostics for the annotated mapping and its structural descendants. `all` suppresses every lint rule; syntax/compiler errors cannot be suppressed. |
| `=where` | `{ <name>: [<iface>, ...], ... }` | Declares ordered generic type parameters. The mapping's **key order** is the positional order of type parameters. Each list element is an interface reference (`$some-iface`, `$mod.iface`, or `$intersect` of those); the substituted type must have a nominal `=impl` for every iface listed. Empty `[]` means unbounded. |
| `=defs` | `{ <name>: $function-envelope, ... }` | Inherent operations on the enclosing type. Each entry registers a function under the qualified key `<mod>.<type>.<name>`, callable as `$<mod>.<type>.<name>: { ... }`. Inside the function `$self` resolves to the enclosing type. Only valid alongside a type-form key (not on `$function` or `$import`). |
| `=impl` | `{ $iface-alias: <impl-payload>, ... }` | Explicit nominal interface implementations. The payload binds the interface's `=where` type-arguments by name, supplies one method binding per interface method (either a fresh `$function` envelope or a `$qualified.name` string reference), and may declare impl-local type parameters via `=where`. Each impl populates the global impl table and registers fresh methods under `<mod>.<type>.<iface>.<method>`. |

```yaml
result:
  $enum:
    err: $e
    ok: $t
  =where: {t: [], e: []}
  =doc: |
    # `result`
    Tagged success / error. `t` is the success payload; `e` is the error payload.

identity:
  $function:
    x: $t
  return: $t
  do:
    - $return: $args.x
  =where: {t: []}
  =doc: "Identity function: returns its argument unchanged."

pair:
  $tuple: [$a, $b]
  =where: {a: [], b: []}
```

**Validation (v1):**

- Unknown `=`-prefixed annotation key → `E-ANNO-001`.
- Bare un-prefixed annotation (`where:`, `doc:`) → `E-ANNO-002`.
- Bound list element that is not an interface (or `$intersect` of interfaces) → `E-WHERE-002`.
- Non-string `=doc` value → `E-DOC-001`.
- Duplicate `=where` key → error.
- Empty `=where: {}` is valid and equivalent to no annotation; `=where: { t: [] }` declares an unbounded `t`.
- Type argument supplied to a generic call/instantiation that is missing an `=impl` for an iface in the bound list → `E-BOUND-001`.

**Scope:** Type-parameter names declared in `=where` are in scope for the symbol's form value (function `args` / `return` / `do:`, or a type-constructor body). They do **not** leak to other symbols.

**Out of scope (v1):** `=doc` on `$import` aliases or on `main`; bound enforcement; type-arg inference at type positions; emission of `=doc` to LSP / generated documentation.

### 13.1 `=defs` — inherent operations

Inherent ops live directly on a type definition and are dispatched via type-qualified calls. There is no distinction between "instance" and "static" methods; `self` in `args` is purely a convention.

```yaml
box:
  $record:
    value: $int64
  =defs:
    identity:
      $function:
        self: $self
      return: $self
      do:
        - $return: $args.self
```

Calling: `$<mod>.box.identity: $b` (single-arg shorthand) or `$<mod>.box.identity: { self: $b }`. Inside the op, `$self` resolves to the enclosing type — `Named("<mod>.box")` here, or `Instantiated { base, type_args }` for generic types.

### 13.2 `=impl` — interface implementations

`=impl` lives on the implementing type and binds it to one or more interfaces. Each entry is keyed by a `$<iface-alias>` and carries a payload that:

- **Binds the interface's `=where` type-arguments** by name (e.g. `t: $int64`).
- **Provides one binding per interface method**, either as a fresh `$function` envelope or as a `$qualified.name` string reference to an already-registered function (an `=defs` op, a free function, or another impl method). The supplied signature must equal the interface's declaration after `$self` and iface-type-arg substitution.
- **Optionally declares impl-local type-parameters** via a `=where` sibling (used when one of the iface type-args is itself a generic in the impl scope).

```yaml
box:
  $record:
    value: $int64
  =defs:
    show:
      $function:
        x: $self
      return: $str
      do:
        - $return: "shown"
  =impl:
    $display:
      fmt: $box.show               # direct alias to the inherent op
    $from-iface:
      t: $int64                    # iface type-arg binding
      from:                         # fresh `$function` envelope
        $function:
          x: $t
        return: $int64
        do:
          - $wasm: { ... }
```

Each impl populates the lowered program's `impls` table keyed by `(implementing_type, interface)`. Because `=impl` lives on the type definition, only the module that defines the type can author the impl — this is Vibra's syntactic **orphan rule**.

### 13.3 Calling interface methods

There are **two** call-site shapes for methods declared on an interface:

| Shape | Form | Use when |
|-------|------|----------|
| Type-qualified | `$<implementing-type>.<iface>.<method>: { ... }` | The interface method has no `$self`-typed argument (e.g. constructors like `from`), or you want to be explicit about the implementing type. Resolves directly via the registered impl-method sig key. |
| Interface-qualified | `$<iface>.<method>: { <self-arg-name>: <expr>, ... }` | The interface method has a `$self`-typed argument. The compiler reads the static type of the value passed for that argument and dispatches to the matching `=impl` block. |

Interface-qualified calls do **static** dispatch in v1: the dispatch argument's static type must be a concrete `Named` or `Instantiated` type with a registered `=impl` for the called interface. Specifically:

- An interface method with no `$self`-typed argument cannot be invoked through the interface-qualified form (`E-CALL-IFACE-NOSELF`). Use the type-qualified form instead.
- A dispatch argument with a *generic* static type (e.g. `$args.x: $t` where `t: [$display]`) is rejected with `E-DISPATCH-001` until monomorphisation lands.
- A dispatch argument whose static type has no `=impl` for the target interface is rejected with `E-BOUND-001`.

Both call shapes are valid in **statement** position (the body of a `do:` step or the value of a `$let`). Interface-qualified calls are not yet supported in arbitrary expression positions (e.g. directly inside `$return`); bind to a local with `$let` first.

---

## 14. Typed stdlib conventions (current)

- **Numeric primitives:** `$int8/$int16/$int32/$int64`, `$uint8/$uint16/$uint32/$uint64`, `$float32/$float64`.
- **No-arg function convention:** use `args: $void` (not empty mapping).
- **Unions:** use direct arrays, e.g. `integer: { $union: [$int64, $int32, $int16, $int8] }`.
- **Enums:** use direct tag map, e.g. `number: { $enum: { int: $integer, float: $decimal } }`.
- **Text and conversion:** `text` provides scalar-aware queries and bounded
  split/trim/replace/join/case transformations; `bytes` provides safe byte
  lookup/build/concat/search and fallible UTF-8 decoding. `convert` parses with
  typed `invalid`/`overflow` failures and formats primitives without locale.
  Float special values have exactly `nan`, `inf`, and `-inf` spellings.
- **Display/debug contracts:** the `display` module declares separate
  `display` and `debug` interfaces for nominal types. Built-in primitives use
  the explicit `convert.format-*` functions until primitive interface impls
  are supported; implicit interpolation remains forbidden.
- **Typed io/fs:** `stdlib/src/fs.vibra` uses `$newtype` wrappers for `path`, `bytes`, and mode-specific file handles (`read-file`, `write-file`, `append-file`, `read-write-file`). File operations return `result<T, fs-error>` and capability interfaces (`readable`, `writable`, `appendable`, `closeable`) make invalid mode use unrepresentable. `stdlib/src/io.vibra` exposes stdin/stdout/stderr as fs file abstractions and provides string-only helpers such as `print`, `println`, and `readln`.
- **Security policies:** privileged host modules consume narrowed domain
  capabilities; roots alone receive aggregate `$policy` values.
- **Rust-inspired unions:** `stdlib/src/option.vibra` (`Option`) is the tagged `$enum: { some: $t, none: $void }` with `=where: {t: []}`; `stdlib/src/result.vibra` (`Result`) is `$enum: { err: $e, ok: $t }` with `=where: {t: [], e: []}`. Both use qualified constructors and `$match`.
- **Naming policy:** kebab-case is recommended for every symbol category; non-kebab symbols produce warnings.

---

## Appendix: removed forms

The following early-draft forms were removed and have **no compatibility path**:

- **`$forall`** — superseded by the `=where` annotation (§13).
- **`$list`** — use `$array`.
- **`$dict`** — use `$record` or `$map`.
- **Tuple-typed `args:`** — use a named record (`args: { name: T, ... }`).
- **Legacy `variants:`** under `$union` — use `$union: [...]` or `$enum: { ... }`.
- **Structured `$match: { target, arms }`** — use `$match: <expr>` with sibling `when:` arms.
- **`$option: T` and `$union` with a direct `$void` member** — import and instantiate the tagged `stdlib/src/option.vibra` enum. These forms are rejected with `E-OPTION-001`.
