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
| Comments | Line comments `#` allowed. Block comments are **forbidden** in v1 (`E-YAML-002`). |
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
  $function:
    args: {}
    return: $void
    do:
      - $io.println: "internal"
main:
  $function:
    args: {}
    return: $void
    do:
      - $io.println: "hello"
```

- **Qualified calls:** **`$alias.symbol`** resolves to public `symbol` in the module bound to `alias`. Same **invocation** shape as `$println` (mapping or scalar argument per callee).
- **Module identity:** Compiler resolves paths to a canonical file URL or path; **cycles** in the import graph are **errors** (`E-MOD-003`).

---

## 4. One-way expression discipline (normative)

These rules are **mandatory** for conforming Vibra v1 source. Tooling **must** flag violations.

| ID | Rule |
|----|------|
| E-ONE-001 | **`$function` arguments** use a **named record** only: `args:` is a mapping `name: $type` or `name: expr-default` as extended later; **no** tuple `args: [$int, $int]` in user modules (stdlib may use stricter internal forms only if the spec allows a “trusted” zone). |
| E-ONE-002 | **`$let`:** at most **one** bound name per `$let` mapping (single key → value, optional single type ascription form). Chain with `$do` for multiple bindings. |
| E-ONE-003 | **`$function.do`:** value is a **block sequence** of expressions only. **Do not** wrap the body in an extra `$do` node inside `do:`. |
| E-ONE-004 | **Sequencing** elsewhere: use **`$do`** with a block sequence of expressions; last item’s value is the result. |
| E-ONE-005 | **Conditionals:** only **`$if`** with keys **`is`**, **`then`**, **`else`** (all required). Branches must unify to the same type. |
| E-ONE-006 | **String interpolation** in user code: **forbidden** unless a single stdlib macro documents it; prefer explicit concat (or a single blessed `$format` when defined). |
| E-ONE-007 | **`$match`:** use **`$match: <expr>`** with sibling **`when:`** arms; structured **`$match: { target, arms }`** is non-canonical. |

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

Creates a function.

```yaml
$function:
  args:
    x: $int64
    y: $int64
  return: $int64
  do:
    - $add: [$args.x, $args.y]
```

Keys: **`args`** (record type / bindings), **`return`** (type), **`do`** (block sequence of expressions).

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

### `$match`

`$match` uses one canonical ordered arm sequence: the target expression is the `$match` value, and sibling `when:` contains the ordered arms. Each arm has `pattern` and `do`.

```yaml
- $match: $result
  when:
    - pattern:
        $result.result.ok:
          $bind: value
      do:
        - $io.println: $value
    - pattern:
        $result.result.err:
          $bind: err
      do:
        - $io.eprintln: "failed"
```

Pattern forms are scalar literals, enum-constructor patterns, `$record`, `$tuple`, `$array`, `$map`, `$newtype`, `$interface`, `{ $bind: name }`, and `{ $wildcard: null }`. Matches must be total: enum matches cover all tags or include wildcard; open-ended targets such as strings, numbers, records, arrays, and maps require wildcard unless a single literal target is trivially exhaustive. Bindings introduced in an arm are scoped to that arm and do not leak after the match. Runtime interface patterns use nominal `=impl` satisfaction.

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
| `$option` | `{ $option: T }` — desugars at parse time to `{ $union: [$void, T] }` |
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

**Null safety (v1):** `null` is valid only for type `$void`, and is the only source-level value of `$void`. A value of type `T` can be coerced into a union containing `T` (e.g. `$union: [$void, T]`), but a union value cannot be coerced back to `T` without explicit narrowing (for tagged unions this is `$match`).

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
- **Core:** `$function`, `$let`, `$if`, `$do`, `$macro`, `$wasm`, `$return`, `$as` (type ascription for `$let`), `$cast`.
- **Types:** primitive symbols and `$newtype`, `$record`, `$array`, `$fn-type`, `$interface`, `$union`, `$option`, etc.

**Effectful** IO and host calls should live in **`stdlib`** modules implemented atop **`$wasm`**, e.g. `io.println`, not as unlimited new host opcodes.

---

## 8. Metaprogramming: `$macro` and `$wasm`

### `$macro`

- Declares **compile-time** expansion from surface AST → core AST.
- **Staging:** **After parse, before typecheck** unless a single documented **typed** stage is added later.
- **v1 recommendation:** **declarative** rewrite tables (pattern → template) in trusted `lang/`; arbitrary procedural macros optional later.

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

**WASI imports (`wasi_snapshot_preview1`, preview1):**

| Import | Signature (wasm32) | Notes |
|--------|-------------------|-------|
| `fd_write` | `(i32 fd, i32 iovs_ptr, i32 iovs_len, i32 nwritten_ptr) -> i32` | errno; UTF-8 via `ciovec` list in linear memory |
| (others) | per [WASI preview1](https://github.com/WebAssembly/WASI/blob/main/legacy/preview1/docs.md) | `stdlib/fs.vibra` lists representative names |

The embedded runner uses **wasmer-wasix** (requires a Tokio 1.x runtime). **Preopened directories** map host paths into the guest; stdio does not require preopens.

**Security policies:** Privileged code receives unforgeable `$policy` values as ordinary arguments. `main` owns the root requested policy, helpers may only receive explicitly narrowed subpolicies, and runtime checks dynamic targets against the policy value at the point of use. Policy groups may mix mandatory and optional scopes per domain. Filesystem scopes use canonical ancestry checks so sibling string-prefix escapes are invalid.

**Known escape hatch:** arbitrary `$wasm` declarations remain accepted in this slice. The grant model applies to grant-aware stdlib APIs, not to untrusted modules that define their own `$wasm` shims. Future work should make `$wasm` trusted-stdlib-only or require explicit unsafe/trust policy.

**`$wasm` encoding (pick one per build):**

- **A)** **Structured list** of opcodes + locals + types (preferred for tooling), or
- **B)** **Opaque** WASM fragment + type signature.

The other mode is **disabled** in v1 builds (`E-WASM-001` if wrong form).

**Unsupported in v1 (non-exhaustive):** threads, exception handling, GC proposal, SIMD (unless explicitly enabled).

---

## 11. Tooling and diagnostics

- **Schemas:** See [`schemas/`](schemas/) — `diagnostic.schema.json`, `query-response.schema.json`, `module-surface.schema.json`, `type-expr.schema.json`, `expression.schema.json`, `linter-codes.json`.
- **Stable errors:** Each diagnostic has **`code`**, **`message`**, **`severity`**, **`span`**, optional **`related`**, optional **`fix`** (JSON Patch RFC 6902).
- **LSP / `vibra query`:** Custom request **`vibra/contextAt`** (or equivalent): given `uri` + `position`, return **`QueryResponse`** (schema) with **expected keys**, **symbol**, **type**, **imports**, **macro schema** if applicable.

**Annotation / generics codes (added with §13):**

| Code | Severity | Summary |
|------|----------|---------|
| `E-ANNO-001` | error | Unknown annotation key on a definition (recognised `=`-prefixed annotations: `=doc`, `=where`, `=defs`, `=impl`). |
| `E-ANNO-002` | error | Legacy un-prefixed annotation key (`where:`, `doc:`); v1 annotations must use the `=` prefix (rename to `=where`, `=doc`). |
| `E-WHERE-002` | error | `=where` bound list element does not resolve to an interface (or `$intersect` of interfaces). |
| `E-BOUND-001` | error | A generic call site or type-position instantiation passes a type argument that does not satisfy its declared `=where` bound (no matching nominal `=impl`). Also raised by interface-qualified dispatch when the dispatch argument's type has no `=impl`. |
| `E-CALL-IFACE-NOSELF` | error | Interface-qualified call (`$iface.method`) targets a method with no `$self`-typed argument; use the type-qualified form. |
| `E-DISPATCH-001` | error | Interface-qualified call's `$self` argument has a generic static type. Pending monomorphisation. |
| `E-DOC-001` | error | `=doc` annotation must be a string scalar. |
| `E-GEN-001` | error | Bare reference to a generic type alias requires explicit instantiation. |
| `E-GEN-002` | error | Generic alias instantiation is malformed (unknown alias / param, missing param, arity mismatch). |
| `E-NEWTYPE-001` | error | Implicit coercion between a `$newtype` and its inner type is forbidden; use `$cast`. |
| `E-NEWTYPE-002` | error | Malformed `$newtype` definition body. |
| `E-CAST-001` | error | `$cast` has no valid v1 cast path between source and target types. |
| `E-CAST-002` | error | Malformed `$cast` payload; expected `$cast: <expr>` with sibling `into: <type>`. |
| `E-CAP-001` | error | Capability values are runtime-minted and cannot be created with `$cast` or literals. |
| `E-SELF-001` | error | Reserved `$self` type used outside an `$interface` body or a type's `=defs` / `=impl` annotation. |
| `E-DEFS-001` | error | Invalid `=defs` annotation (placed on a non-type definition, entry is not a `$function`, or duplicate name). |
| `E-IMPL-001` | error | Invalid `=impl` annotation (non-type definition, malformed payload, or method binding that is neither a `$function` envelope nor a `$ref` string). |
| `E-IMPL-002` | error | `=impl` keyed by an alias that does not resolve to a registered `$interface` type. |
| `E-IMPL-003` | error | `=impl` block missing a binding for one of the interface's `=where` type-parameters or one of its methods. |
| `E-IMPL-004` | error | `=impl` payload contains an unexpected key (not `=where`, an iface type-arg, or an iface method name). |
| `E-IMPL-005` | error | `=impl` method signature does not match the interface declaration (after `$self` and iface type-arg substitution). |
| `E-IMPL-006` | error | `=impl` method binding is a `$ref` string that does not resolve to a registered function. |

---

## 12. Hello world (updated)

```yaml
io:
  $import: ./stdlib/io.vibra
main:
  $function:
    args: $void
    return: $void
    do:
      - $io.println: "Hello, World!"
```

(Early examples used bare `$println`; **normative** style is **stdlib via `$import`**, with `io` wrapping **`$wasm`** host glue per §9.)

---

## 13. Annotations (`=doc`, `=where`, …)

A top-level symbol's value is a **definition envelope**: a mapping with **exactly one** `$`-form key (`$function`, `$import`, or one of the type constructors in §6) and **zero or more** `=`-prefixed annotation siblings. v1 currently recognises four annotations: `=doc`, `=where`, `=defs` (inherent ops on a type), and `=impl` (explicit interface implementations).

> **Annotation prefix is normative.** Every annotation key starts with `=`. The pre-1.0 spelling without the prefix (`where:`, `doc:`) is **rejected** with `E-ANNO-002` (rename to `=where`, `=doc`).

| Annotation | Value | Purpose |
|------------|-------|---------|
| `=doc` | `$str` (YAML `|` block scalar recommended for multiline markdown) | Compile-time documentation attached to the symbol. Stored on the lowered `FunctionSig` / `TypeAlias`; not yet emitted to runtime or LSP output. |
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
    args:
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
        args:
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
        args:
          x: $self
        return: $str
        do:
          - $return: "shown"
  =impl:
    $display:
      fmt: $box.show               # method-as-ref to the inherent op
    $from-iface:
      t: $int64                    # iface type-arg binding
      from:                         # fresh `$function` envelope
        $function:
          args:
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
- **Typed io/fs:** `stdlib/fs.vibra` uses `$newtype` wrappers for `path`, `bytes`, and mode-specific file handles (`read-file`, `write-file`, `append-file`, `read-write-file`). File operations return `result<T, fs-error>` and capability interfaces (`readable`, `writable`, `appendable`, `closeable`) make invalid mode use unrepresentable. `stdlib/io.vibra` exposes stdin/stdout/stderr as fs file abstractions and provides string-only helpers such as `print`, `println`, and `readln`.
- **Security policies:** privileged host modules consume `$policy` values; supported domains include `fs`, `env`, `net`, `process`, `time`, `random`, and `sys`.
- **Rust-inspired unions:** `stdlib/option.vibra` (`Option`) is `$union: [$void, $t]` with `=where: {t: []}`; `stdlib/result.vibra` (`Result`) is `$enum: { err: $e, ok: $t }` with `=where: {t: [], e: []}`, used at value sites via `$match`.
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
