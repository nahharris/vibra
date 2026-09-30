# Step 4a — library architecture contracts

Prerequisite: Step 4 merged. Specification prerequisite; it claims no language
behavior.

## Why

Step 4 made `array` and `map` builtin types and left `bool`, `str`, `bytes`,
`option`, and `result` compiler-owned or compiler-recognized by a hardcoded
identity. The product direction is the opposite: the language is a small core,
and everything else is standard-library Vibra that the toolchain may
accelerate. Steps 7, 8, and 11 build the library, so the architecture has to
be settled before them.

## Contracts

Recorded in the [decision ledger](decision-ledger.md) as D17.1–D17.5, closing
gaps G17–G20:

- **Library-first core (D17.1).** The compiler owns the scalar types,
  `(array t)`, `fn` types, and the structural type constructors. `(array t)`
  stays in the core because no combination of records and enums gives
  constant-time indexed storage in a language without mutation. `bool`, `str`,
  `bytes`, `map`, `option`, `result`, `ordering`, and the error enums become
  standard-library `deftype`s.
- **Language roles (D17.2).** Syntax and checking need some library types:
  `if` needs `bool`, a string literal needs `str`, a lookup and `try` need
  `option` and `result`. Each such need is a closed role that one
  standard-library `deftype` claims with `role:`. The compiler binds the role,
  never the definition. Role types need no import; the set is closed, so it is
  a fixed vocabulary, not a prelude.
- **Native implementations (D17.3).** A standard-library function may add
  `native: "symbol"` beside its Vibra body. The body stays the meaning; the
  native implementation is a toolchain-owned accelerator, single-sourced for
  the interpreter and the Wasm backend and checked against the body. The
  `@compiler` registry keeps only primitive operations the core cannot express.
- **Single-declaration imports (D17.4).** `(import ordering @std.core.ordering)`
  binds one public declaration, so a single-type module reads naturally
  without a prelude.
- **Representation latitude (D17.5).** Wrapper erasure, compact enum layouts,
  and in-place update of unshared values are allowed and unobservable, never
  promised, and enabled only after unoptimized parity (M7).

## Deliberate limits

- This is not a foreign-function interface: packages cannot write `native:`,
  `role:`, or `intrinsic-type`, and a native implementation performs no host
  operation. The charter's exclusion of native FFI and of optimization promises
  stands.
- The migration is staged. Until a type moves, the toolchain may keep
  implementing it directly; no program observes the difference.

## Migration owners

| Type or mechanism | Owner |
| --- | --- |
| Declaration imports, `role:` binding, `native:` mechanism, registry split | [Step 4b](04b-library-core.md) |
| `option` claims `@option` | Step 4b |
| `result` claims `@result` | Step 7 |
| `bool`, `str`, `bytes`, `ordering`, error enums; text and bytes operations as natives | Step 8 |
| `map` over sorted arrays under `@map`; key conformances as implementations | Step 11 |
| `iter` claims `@iter` | Step 14 |
| Native lowering into Wasm | M4 |
| Representation latitude | M7 |

## Done

The type, source, packages, and runtime chapters, the charter, the v1 roadmap,
this README, and the decision ledger agree; the specification-example
inventory is refreshed; validation passes.
