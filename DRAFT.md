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

### Constructors (YAML forms)

| Form | Meaning |
|------|---------|
| `$literal` | Literal type: `{ $literal: "ok" }` |
| `$record` | Concrete product: `{ $record: { f: T, ... } }` |
| `$map` | Homogeneous map (if in v1): `{ $map: { key: K, value: V } }` |
| `$tuple` | Tuple of types: `{ $tuple: [$t1, $t2] }` — **type positions only** |
| `$array` | Homogeneous array type: `{ $array: T }` |
| `$list` | **Deprecated** for new code; use **`$array`** for types and YAML sequences for values. |
| `$dict` | **Deprecated** for product types; use **`$record`** or **`$map`**. |
| `$union` | `{ $union: [T1, T2, ...] }` — discriminated unions should use a **tag** field in `$record` variants |
| `$option` | `{ $option: T }` — optional convenience form (equivalent to `{ $union: [$void, T] }`) |
| `$intersect` | `{ $intersect: [T1, T2] }` — compose interface requirements |
| `$interface` | **Go-like structural interface:** `{ $interface: { name: T, ... } }` — each member is a **type**; function members use **`$fn-type`** |
| `$fn-type` | `{ $fn-type: { args: { $record: ... }, return: R } }` — **one** function type constructor |
| `$forall` | Generics v1: `{ $forall: { types: [t, u], where: {}, in: <type> } }` — `types` declares type parameter names in order for the body; only names in `types` are treated as generics inside `in`. Optional `where` is reserved for future bounds (ignored if present). |

**`$forall` semantics (v1):** Type parameters listed in `types` are the only symbols that resolve to type variables within `in`. At use sites, enum constructors unify payload types with the declared variant field types, producing a fully instantiated enum type (`Instantiated`) whose type arguments are ordered like `types`. Nested `$forall` in `in` shadows outer names.

**Null safety (v1):** `null` is valid only for type `$void`, and is the only source-level value of `$void`. A value of type `T` can be coerced into a union containing `T` (e.g. `$union: [$void, T]`), but a union value cannot be coerced back to `T` without explicit narrowing (for tagged unions this is `$match`).

### Interface satisfaction

- A **`$record` type** **satisfies** an **`$interface` type** if for every field `n: T` in the interface, the record has **`n`** with a type **`U`** such that **`U` is a subtype of `T`** per v1 rules (width subtyping toward the interface for records).
- **Variance:** **v1:** function types are **invariant** in arguments and **covariant** in returns unless the compiler documents otherwise (`E-TY-VARIANCE` audit).

---

## 7. Standard library surface (builtins whitelist)

**Host-reserved** keys (expand before user macros where applicable):

- **Module system:** `$import` (compile-time only, appears only under import alias mapping).
- **Core:** `$function`, `$let`, `$if`, `$do`, `$macro`, `$wasm`, `$as` (type ascription for `$let`).
- **Types:** primitive symbols and `$record`, `$array`, `$fn-type`, `$interface`, `$union`, `$option`, etc.

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

## Appendix: legacy draft examples

Tuple-typed args and `$dict` in the original draft are **superseded** by §4 and §6; migrate to **named record `args`** and **`$record`**.

---

## 13. Typed stdlib conventions (current)

- **Numeric primitives:** `$int8/$int16/$int32/$int64`, `$uint8/$uint16/$uint32/$uint64`, `$float32/$float64`.
- **No-arg function convention:** use `args: $void` (not empty mapping).
- **Unions:** use direct arrays, e.g. `integer: { $union: [$int64, $int32, $int16, $int8] }`.
- **Enums:** use direct tag map, e.g. `number: { $enum: { int: $integer, float: $decimal } }`.
- **Early stdlib typing:** io/fs currently use raw primitives (`$int64` and `$str`) instead of nominal wrapper types.
- **Rust-inspired unions:** `stdlib/option.vibra` (`Option`) is modeled as `$forall + $union` (`$union: [$void, $t]`), while `stdlib/result.vibra` (`Result`) uses `$forall + $enum` constructors and `$match`.
- **Naming policy:** kebab-case is recommended for every symbol category; non-kebab symbols produce warnings.
