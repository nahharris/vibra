## Pre-implementation findings

Phase 2 of the plan ("audit `vibra_v1`") was run ahead of implementation. The
headline: **the privileged host ABI is already scalars-only, and by a wider
margin than the plan assumes** — but the audit found the violation the plan's
risk section predicted, plus a written contract that contradicts the rule.

**Verdict: MIXED.** One real pointer/shared-memory crossing (static Wasm FFI),
one normative document declaring pointer layouts (cheap fix, most dangerous if
left), one latent type-level hole. Runtime plugins are clean.

### The distinction that decides this audit

`schemas/host-abi.json` lists `str`, `bytes`, `array`, `record`, and
`string-map` as ABI value types, which reads like a violation. It is not. Those
names describe the **shape of a host-owned arena slot**, not a wasm parameter.

The Vibra guest never holds any of them:

- `src/wasm_backend.rs:1-5` — "Values use opaque i32 arena addresses; the
  versioned host ABI constructs dynamic values and performs privileged stdlib
  calls."
- `src/wasm_backend.rs:173-222` — all 15 `vibra_v1` imports are built with
  `HostFunction::new_typed` over `i32` only. There is no `i64`, no `f64`, and
  no memory in the import set.
- `src/wasm_backend.rs:269` — `arena: Vec<RuntimeValue>` lives entirely
  host-side.
- `src/wasm_backend.rs:331-340` — `alloc` returns `len() as i32` (1-based, so
  zero is never valid) and `get` bounds-checks with `checked_sub(1)` before
  every access.
- The emitted guest module **declares no memory section at all**. A grep for
  `MemorySection` in `src/wasm_backend.rs` returns nothing; the only
  `wasmer::Memory` in the file is the static-FFI one discussed below.

So `fd_write(write-handle, str)` does not pass a string. It passes two i32
arena indices. Host handles are likewise opaque indices, not references:

- `src/lower.rs:122-126` — `HostHandle { id: u64, access: HandleAccess }`.
- `src/execute.rs:138-149` — `FileTable { next: u64, handles: HashMap<u64, …> }`,
  resolved only on the owning (host) side.
- `src/execute.rs:300-309` — IDs are monotonic and never recycled, so an absent
  ID is classified exactly (`Closed` vs `Invalid`) rather than aliasing a
  later resource.

This is precisely the "opaque indices resolved on the owning side" the plan
asks for. Phase 2 can be closed as **already satisfied for `vibra_v1`**; the
work is to pin it with the phase-3 test, not to change it.

### Violation 1 — static Wasm FFI passes pointers into a shared linear memory

Real, active, and exactly where the plan's risk section predicted.

- `docs/reference/static-wasm-ffi.md:46-59` states the contract explicitly:
  "caller-owned linear-memory buffers and an explicit `(pointer: int32,
  length: int32)` pair", and "The artifact must import a memory named
  `vibra_ffi.memory`".
- `src/wasm_backend.rs:673-702` computes `next_pointer`, emits
  `wasmer::Value::I32(next_pointer as i32)` plus a length, per buffer argument.
- `src/wasm_backend.rs:703-712` grows the memory and writes the bytes into it.
- `src/wasm_backend.rs:661-670` creates the `wasmer::Memory` and supplies it as
  the `vibra_ffi.memory` import.
- `src/project.rs:857-873` is the check-time counterpart, permitting exactly
  that one import.

**Important scoping nuance before this is treated as fatal.** The pointer does
not cross the *Vibra program's* boundary. The Vibra guest passes an arena index
to the host; the **host** then allocates a memory it owns and hands a pointer to
the foreign module. The crossing is host to dependency-provided-code, not
guest to host.

The mitigations are real and worth recording rather than discarding:

- `src/wasm_backend.rs:632` — a fresh `Store` per call.
- `src/wasm_backend.rs:661-664` — a fresh `Memory` per call, created by the
  host, never reused.
- `src/wasm_backend.rs:713` — a fresh `Instance` per call, so a callee has no
  surviving state in which to retain a pointer.
- `src/project.rs:860-866` — every other import is rejected outright.

**Recommendation: documented exception, not a fix.** Removing it deletes the
only way to pass bytes to foreign code, and the plan's own stated reason to
avoid exceptions ("a single grandfathered pointer-crossing makes every future
isolation claim false") is satisfied differently here: the isolation claim can
be stated over the `vibra_v1` compartment boundary, with the static-FFI
boundary named as a separate, weaker one. The exception must say so in the
decision contract, in those terms, with the four mitigations above as its
rationale. If the project prefers no exception at all, the alternative is
deleting the buffer path from `static-wasm-ffi.md` and reverting FFI to the
scalar-only subset, which the code already supports
(`src/wasm_backend.rs:654-660` makes the memory conditional on `has_buffer`).

### Violation 2 — a normative document declares pointer layouts

This is the cheapest fix and the one most likely to kill the rule if left.

`docs/reference/wasm-abi.md:32-37` says the layouts in `src/wasm_abi.rs`
"remain normative", including "strings/buffers use 32-bit pointer/length
descriptors" and "mutable/reference values use arena addresses". The module
delivers on that:

- `src/wasm_abi.rs:19-24` — `StorageClass::{Direct, CopiedPointer, ArenaAddress}`.
- `src/wasm_abi.rs:38-49` — `Mutable`/`Reference` map to `ArenaAddress`;
  `String`/`Array`/`Map` map to `CopiedPointer` with a `[0, 4]` pointer/length
  field pair.

**The module is dead code in the live pipeline.** Its only references are the
`pub mod` at `src/lib.rs:36` and two tests
(`tests/integration.rs:448`, `tests/integration.rs:467`). Nothing in lowering,
emission, or execution calls `layout_of`.

So the repository currently ships a *written, normative, tested* pointer-passing
ABI that no code implements and that directly contradicts the rule this issue
wants to adopt. **Fixable, and it should be fixed in this issue**: either delete
`src/wasm_abi.rs` and the paragraph, or demote the paragraph to
non-normative planning material and move the module under a name that says so.
Leaving it is how the rule dies quietly — a future contributor implementing
against the reference doc would introduce Violation 1 everywhere.

### Latent hole 3 — the `Any` ABI slot accepts every type

`src/lower.rs:3649`:

```rust
A::Any => true,
```

`ValueKind::Any` is used by `array_set`, `array_append`, `array_insert`,
`array_contains`, and `map_insert` (`src/host_abi.rs:266-331`). The match arm
accepts **every** `TypeRef`, including `TypeRef::Mutable` and
`TypeRef::Reference`, which at runtime are `Rc<RefCell<RuntimeValue>>`
(`src/lower.rs:103-107`) — genuine shared references.

Note that `contains_host_backed_newtype` (`src/lower.rs:3683-3718`) *does*
recurse into `Mutable`/`Reference`, but it is only applied to host-import
**return** types (`src/lower.rs:3480`, `src/lower.rs:3542`). Nothing equivalent
guards parameters.

I could not reach this from surface syntax: reads auto-deref, so a `mut`/`ref`
collapses to its pointee before it can enter a value position. Probes putting a
`ref`/`mut` into an array and through `collections.array-append` all failed with
`E-TY-001`. So this is **latent, not currently exploitable** — but it is
exactly the check phase 3's ABI test should tighten. Cheap fix: reject
`Mutable`, `Reference`, and `FnType` in the `Any` arm.

### Clean — typed runtime plugins

`docs/reference/runtime-plugins.md` describes the surface and the
implementation matches. No violation:

- `src/plugin.rs:108-116` — `scalar_type` accepts only `bool`, `int32`,
  `uint32`, `int64`, `uint64`, `float32`, `float64`, and bails on anything else
  with `E-PLUGIN-001`. There is no string, buffer, or memory case.
- `src/plugin.rs:54-60` — any import at all is rejected with `E-PLUGIN-004`.
- `src/plugin.rs:93` — instantiated with `wasmer::Imports::new()`, so no memory
  can be supplied even if one were declared.

The plan's risk section named the plugin surface as a plausible violation site.
It is not one; the static FFI is.

### One stale allowance worth folding into phase 3

`src/wasm_backend.rs:1791-1805` (`validate_imports`) whitelists 45
`wasi_snapshot_preview1` names (`src/wasm_backend.rs:61-108`), every one of
which is pointer-based. It is currently harmless: `run_wasm_inner` supplies
only the 15 `vibra_v1` i32 functions (`src/wasm_backend.rs:215-222`), so such a
module fails at instantiation instead of validation. But it also contradicts
`docs/reference/wasm-abi.md:50-52`, which claims "arbitrary Preview 1 imports
and unknown Vibra symbols are rejected before instantiation." Dropping
`WASI_IMPORTS` from the whitelist makes the code match the doc and makes the
phase-3 test meaningful.

### Suggested revision to the phases

Phase 2 is done and its answer is "clean for `vibra_v1`". The remaining work is
larger than "audit" but still small:

1. Decision contract (phase 1, unchanged) — must name the static-FFI boundary
   as a separate, weaker boundary rather than claiming one uniform rule.
2. Delete or demote `src/wasm_abi.rs` and `docs/reference/wasm-abi.md:32-37`.
3. Tighten `A::Any` (`src/lower.rs:3649`) to reject `Mutable`/`Reference`/
   `FnType`, and drop `WASI_IMPORTS` from `validate_imports`.
4. Phase 3 ABI test (unchanged) — it now has something real to assert against.
