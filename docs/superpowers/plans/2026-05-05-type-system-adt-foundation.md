# Type System + ADT Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add first-class numeric types, explicit type annotations, nominal discriminated unions (`Option`/`Result`), and strong domain types (`Fd`, `Path`, `File`, `Dir`) across language + stdlib.

**Architecture:** Extend `src/lower.rs` from permissive runtime values into a typed AST/IR pipeline that can validate declarations and expressions before execution. Keep runtime bridge pragmatic by preserving current execution backend while introducing nominal type metadata and union constructors/matching at language level.

**Tech Stack:** Rust (`serde_yaml`, `anyhow`), existing Vibra YAML syntax, current runtime executor in `src/execute.rs`.

---

### Task 1: Add Type Model and Primitive Kinds

**Files:**
- Modify: `src/lower.rs`
- Test: `tests/integration.rs`

- [ ] **Step 1: Write failing tests for primitive type parsing**

Add tests that load temp modules containing type names:
- `$int8`, `$int16`, `$int32`, `$int64`
- `$uint8`, `$uint16`, `$uint32`, `$uint64`
- `$float32`, `$float64`

Expected now: parser/type resolver rejects unknown type names.

- [ ] **Step 2: Run tests to confirm fail**

Run: `cargo test --test integration`
Expected: failures mentioning unsupported type names.

- [ ] **Step 3: Implement `TypeRef` enum and parser**

In `src/lower.rs`:
- Add `TypeRef` with variants for all numeric kinds + `Void` + `Named(String)` + `Generic`.
- Add helper `parse_type_ref(&Value) -> Result<TypeRef>`.

- [ ] **Step 4: Re-run tests**

Run: `cargo test --test integration`
Expected: primitive type parsing tests pass.


### Task 2: Enforce Explicit Type Annotation Forms

**Files:**
- Modify: `src/lower.rs`
- Modify: `examples/hello.vibra`
- Modify: `examples/ask-name.vibra`
- Modify: `examples/fs-roundtrip.vibra`
- Test: `tests/integration.rs`

- [ ] **Step 1: Add failing tests for required explicit annotation behavior**

Add tests validating:
- zero-arg functions use `args: $void`
- non-zero args use mapping
- `return` must be explicit

- [ ] **Step 2: Run tests to confirm fail**

Run: `cargo test --test integration`

- [ ] **Step 3: Tighten lowerer validation**

In `src/lower.rs`:
- reject ambiguous/implicit arg forms,
- normalize and store function arg types in typed signature,
- keep backward compatibility only where explicitly intended.

- [ ] **Step 4: Update examples to canonical forms**

Ensure `main` uses:
```yaml
args: $void
```

- [ ] **Step 5: Re-run tests**

Run: `cargo test`


### Task 3: Introduce Nominal Discriminated Unions

**Files:**
- Modify: `src/lower.rs`
- Create: `stdlib/option.vibra`
- Create: `stdlib/result.vibra`
- Test: `tests/integration.rs`

- [ ] **Step 1: Write failing tests for union declarations**

Add tests for:
- union declaration parsing,
- variant constructor validation,
- wrong payload shape errors.

- [ ] **Step 2: Run tests to confirm fail**

Run: `cargo test --test integration`

- [ ] **Step 3: Implement union declaration structures**

In `src/lower.rs`:
- add union symbol table entries,
- add typed constructor expression form,
- add variant payload typing checks.

- [ ] **Step 4: Add `Option` and `Result` stdlib definitions**

Create:
- `stdlib/option.vibra`
- `stdlib/result.vibra`

Use Rust-inspired names:
- `Option<T>`: `Some(T) | None`
- `Result<T, E>`: `Ok(T) | Err(E)`

- [ ] **Step 5: Re-run tests**

Run: `cargo test`


### Task 4: Add `$match` for Union Destructuring

**Files:**
- Modify: `src/lower.rs`
- Test: `tests/integration.rs`
- Modify: `examples/ask-name.vibra` (optional follow-up variant)

- [ ] **Step 1: Write failing tests for `$match`**

Add tests for:
- matching all variants,
- binding payload variable,
- unknown variant diagnostics.

- [ ] **Step 2: Run tests to confirm fail**

Run: `cargo test --test integration`

- [ ] **Step 3: Implement `$match` typed lowering**

In `src/lower.rs`:
- parse `$match`,
- verify target expression is union type,
- verify arm variants and payload binding types.

- [ ] **Step 4: Re-run tests**

Run: `cargo test`


### Task 5: Add Strong Domain Types (`Fd`, `Path`, `File`, `Dir`)

**Files:**
- Modify: `stdlib/io.vibra`
- Modify: `stdlib/fs.vibra`
- Create: `stdlib/types.vibra`
- Modify: `src/lower.rs`
- Test: `tests/integration.rs`

- [ ] **Step 1: Write failing tests for domain type safety**

Add tests for:
- passing `str` where `Path` is required fails,
- passing `int` where `Fd` is required fails,
- valid constructors/conversions pass.

- [ ] **Step 2: Run tests to confirm fail**

Run: `cargo test --test integration`

- [ ] **Step 3: Define domain types in stdlib**

Create `stdlib/types.vibra` with nominal types:
- `Fd`
- `Path`
- `File`
- `Dir`

Then migrate signatures in:
- `stdlib/io.vibra`
- `stdlib/fs.vibra`

- [ ] **Step 4: Implement converter/checker hooks in lowerer**

Ensure typed call checks in `src/lower.rs` enforce domain type compatibility.

- [ ] **Step 5: Re-run tests**

Run: `cargo test`


### Task 6: Wire Runtime/Executor to Typed Domain Values

**Files:**
- Modify: `src/execute.rs`
- Modify: `src/main.rs`
- Test: `tests/integration.rs`

- [ ] **Step 1: Write failing runtime tests**

Add tests that use typed wrappers in io/fs flows and currently fail in executor.

- [ ] **Step 2: Run tests to confirm fail**

Run: `cargo test --test integration`

- [ ] **Step 3: Implement typed value handling in executor**

In `src/execute.rs`:
- extend runtime value model for wrappers/unions,
- add safe unwrap/construct helpers for domain types,
- keep preopen checks enforced.

- [ ] **Step 4: Re-run tests**

Run: `cargo test`


### Task 7: Documentation + Migration Notes

**Files:**
- Modify: `README.md`
- Modify: `DRAFT.md`

- [ ] **Step 1: Update docs for new type forms**

Document:
- primitive numeric families,
- `args: $void` rule,
- union syntax (`Option`/`Result`),
- domain types in io/fs.

- [ ] **Step 2: Add migration examples**

Show before/after for:
- raw `int` fd -> `Fd`,
- raw `str` path -> `Path`.

- [ ] **Step 3: Verify docs examples**

Run:
- `cargo test`
- `cargo run -- run examples/ask-name.vibra`


### Task 8: Final Verification and Cleanup

**Files:**
- Modify: `tests/integration.rs` (if needed)
- Modify: examples under `examples/`

- [ ] **Step 1: Run full suite**

Run: `cargo test`
Expected: all pass.

- [ ] **Step 2: Run executable scenarios**

Run:
- `cargo run -- run examples/ask-name.vibra`
- `cargo run -- run examples/fs-roundtrip.vibra --preopen .`

- [ ] **Step 3: Sanity-check diagnostics**

Ensure type errors are actionable (mention expected/actual type + location context).

- [ ] **Step 4: Commit in focused chunks**

Recommended sequence:
1. type model + annotation enforcement
2. unions + option/result
3. domain types + stdlib migration
4. runtime/executor typed support
5. docs/examples polish
