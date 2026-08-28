## Pre-implementation findings — can values escape a scope?

Investigation of the arena-reclamation risk in `docs/plans/2026-08-02-execution-bounds.md`:
scope-tied regions require that no value outlives its allocating scope. Short version:
**values escape by every available route, the arena has no scope structure to tie regions
to, and there is no scope construct in the language to tie them to either.**

---

### 0. Confirming the arena appends and never frees

Confirmed. The arena is a single flat `Vec<RuntimeValue>` on `WasmHost`:

- field declaration — `src/wasm_backend.rs:269`
- initialised empty — `src/wasm_backend.rs:324`
- the allocator, in full — `src/wasm_backend.rs:331`:

```rust
fn alloc(&mut self, value: RuntimeValue) -> i32 {
    self.arena.push(value);
    self.arena.len() as i32
}
```

- the only reader — `src/wasm_backend.rs:335`, which does `handle.checked_sub(1)` then
  `self.arena.get(...)`.

Those five lines (269, 324, 332, 333, 338) are the **complete** set of `arena` references in
the file. No `pop`, `truncate`, `clear`, `remove`, `drain`, or `swap_remove` anywhere.

Two properties matter for regions, beyond the leak:

- **A handle is an absolute 1-based index**, so truncating the arena both invalidates every
  higher handle *and* causes the next `alloc` to hand out an index that a stale handle still
  names. That is a silent aliasing hazard, not a detectable error.
- **There is no generation tag.** `OperationToken` in `src/async_runtime.rs:26` carries
  `{ slot, generation }` precisely to solve this ABA problem for host operations. Arena
  handles have no equivalent. Any reclamation scheme will need to add one, or reclamation
  bugs will manifest as wrong values rather than crashes.

Allocation sites are pervasive: `seed` (`:341`), `constant` (`:354`), `read` (`:365`),
`construct` (`:481`), `call` (`:507`, `:517`), `binding` (`:568`), and iteration (`:616`).
Every expression evaluation in a loop body allocates a fresh slot per iteration, which is
the unbounded growth the plan describes.

---

### The escape routes

#### 1. Can a value allocated inside a scope be returned out of it? — **Yes, trivially.**

Every user function in the emitted module has type `[i32; arity] -> [i32]`
(`src/wasm_backend.rs:1061`). Arguments and results are arena handles. `Statement::Return`
emits the expression (which allocates) and then `Instruction::Return`
(`src/wasm_backend.rs:1297`), handing the callee's arena handle to the caller.

Verified empirically — this compiles and runs:

```
(defn make () (array int64) (let inner (array 1 2 3)) (return inner))
```

If the callee's frame were a region, the returned handle would dangle on return. This is the
common case, not an edge case.

#### 2. Can a `task` / `spawn` capture a value from an enclosing scope? — **Yes, and it is worse than capture.**

The `captures:` form does restrict *types*, but not lifetimes:

- `task` rejects `Mutable` and `Reference` captures — `src/lower.rs:5820`, `E-TASK-001`.
- `spawn` rejects those plus `JoinHandle` — `src/lower.rs:5742`.

That is an **aliasing** rule, not an escape rule. It says nothing about where the captured
value was allocated or how long it lives.

In the wasm backend the situation is more direct than "capture": **`captures:` is ignored
entirely at codegen.** `Statement::Task { body, .. } => self.emit_statements(body)` —
`src/wasm_backend.rs:1396`. The task body is emitted inline into the enclosing function,
sharing its locals and its arena. There is no child region to speak of, and no boundary
where one could be inserted without changing the lowering.

In the interpreter the captures are deep-`clone`d into a fresh child env
(`src/execute.rs:1080-1087`), which is closer to a region-friendly shape — but that path is
`#[cfg(test)]`-only for whole programs (`src/execute.rs:25`; `run_lowered` delegates to the
wasm backend at `src/execute.rs:20`).

#### 3. Can `join` return a value allocated in the child scope to the parent? — **Yes, by construction.**

Wasm backend:

- `Statement::Spawn` evaluates `value` eagerly and stores the resulting **arena handle** in
  a compiler-owned wasm local — `src/wasm_backend.rs:1397`.
- `Statement::Join` is a `LocalGet` / `LocalSet` pair copying that same handle into the
  result binding — `src/wasm_backend.rs:1405`.

So the joined value *is* the child's arena slot, aliased into the parent. Worse, the handle
sits in a wasm local across arbitrary intervening control flow between `spawn` and `join`,
so any region closed in that window would dangle it.

Interpreter: the value is retained in `SourceTaskRuntime::results` and moved into the parent
env on join — `src/execute.rs:1109`, `src/execute.rs:1119`. `spawn`'s value expression is
evaluated in the *child* env (`src/execute.rs:1108`), so allocations made there reach the
parent unchanged.

Verified empirically end-to-end — a value allocated in a callee, returned, carried through
`spawn`/`join`, and written into an outer `mut` via `set`, all in one program, runs clean.

#### 4. Other escape paths

- **`Rc`-aliased heap cells.** `RuntimeValue::Mutable(Rc<RefCell<RuntimeValue>>)` and
  `RuntimeValue::Reference { cell, .. }` — `src/lower.rs:103-107`. `WasmHost::set`
  (`src/wasm_backend.rs:519`) writes through such a cell, so a value constructed in an inner
  scope can be stored into a cell owned by an outer one. Dropping the inner arena slot only
  decrements a refcount; the value survives. Memory-safe, but it means **a region would not
  actually reclaim aliased values** — reclamation becomes silently partial, which is worse
  than not reclaiming, because the memory ceiling in phase 2 would then be enforced against
  a number that does not track reality.
- **Argument frames.** `frame_push` stores a raw handle into the current frame
  (`src/wasm_backend.rs:374`); `take_frame_values` reads them back later
  (`src/wasm_backend.rs:382`). A handle can outlive the construct that allocated it while
  parked in a frame belonging to an outer call.
- **`self.bindings`.** Pattern-match bindings are cloned out of the arena into a host-side
  `Vec<RuntimeValue>` (`src/wasm_backend.rs:557`) and re-`alloc`d on demand
  (`src/wasm_backend.rs:562`). This vector is not region-scoped and is overwritten by the
  next `matches` call — an independent lifetime axis the region scheme would have to model.
- **Constants and seeds** are *not* an escape path, notably: `constant`
  (`src/wasm_backend.rs:354`) and `seed` (`src/wasm_backend.rs:341`) clone from the plan /
  `seed_env` into a **fresh** arena slot each time. They allocate into whatever region is
  current, which is fine. This is the one place the design is already region-compatible.
- **Host handles outlive everything.** `FileTable` (`src/execute.rs:138`) is created once per
  `WasmHost` (`src/wasm_backend.rs:312`) and is removed from only by explicit `fd_close`
  (`src/execute.rs:278`, `src/execute.rs:2389`). Handles are plain integers inside
  `RuntimeValue`s, so reclaiming an arena slot holding a handle silently orphans the OS
  resource rather than closing it. Regions and host-handle lifetime are orthogonal problems.

#### 5. The structural blocker underneath all of the above

**There is no scope construct in the language to tie regions to.**

- `src/ast/surface.rs` defines `task` (`:1797`), `spawn` (`:1809`), `join` (`:1818`). Grep
  finds no `scope` form.
- `Scheduler::open_scope` / `close_scope` (`src/async_runtime.rs:358`, `:819`) have **zero**
  call sites outside `src/async_runtime.rs` and its own tests.
- The one production-adjacent consumer, `SourceTaskRuntime`, uses only
  `self.scheduler.root()` — `src/execute.rs:956` — and never opens or closes a child scope.
- `docs/reference/async-structured-concurrency.md:190` states milestones 1–5 are not
  implementation-complete.

The plan's premise — "regions align with machinery Vibra already has —
`src/async_runtime.rs` scopes with monotone narrowing and deterministic teardown" — is
accurate about the *design* of `async_runtime.rs` and inaccurate about its *reach*. That
machinery is a well-tested standalone model with nothing plugged into it.

---

### Verdict

**Scope-tied region reclamation is not viable without escape analysis. Escape analysis is
required, and per the plan's own words that is a material scope increase warranting its own
issue.**

The counterexample class the plan hoped to rule out is not an edge case — it is the
mainstream path:

| Route | Escapes? | Evidence |
|---|---|---|
| Return a value out of a function | Yes | `src/wasm_backend.rs:1297`, `:1061` |
| `task` captures | Yes — `captures:` is dropped at codegen; body is inlined | `src/wasm_backend.rs:1396` |
| `join` result | Yes — the joined value *is* the child's arena handle | `src/wasm_backend.rs:1397`, `:1405` |
| `Rc`-aliased mutable cells | Yes, and defeats reclamation rather than crashing | `src/lower.rs:103`, `src/wasm_backend.rs:519` |
| Argument frames / pattern bindings | Yes, on independent lifetime axes | `src/wasm_backend.rs:374`, `:557` |
| Constants / seeds | No — cloned into a fresh slot per read | `src/wasm_backend.rs:341`, `:354` |
| Host handles | Outlive everything; orthogonal problem | `src/execute.rs:138`, `:278` |

Two further blockers that are arguably larger than escape analysis itself:

1. **There is no scope to tie a region to.** Regions cannot be implemented before a scope
   construct exists in the executed language, which is unshipped milestone work
   (`docs/reference/async-structured-concurrency.md:190`).
2. **Arena handles are un-tagged absolute indices** (`src/wasm_backend.rs:331-340`). Any
   reclamation makes stale handles alias newly allocated values. A generation tag, on the
   `OperationToken` model (`src/async_runtime.rs:26`), is a prerequisite for reclamation of
   *any* flavour — and it is the cheap change that turns reclamation bugs from wrong answers
   into detectable errors.

**Suggested resequencing of #251 phase 1.** In descending order of value-per-risk:

- **1a. Generation-tagged arena handles.** Small, self-contained, no semantic change, and a
  hard prerequisite for every reclamation strategy. Worth landing on its own merits.
- **1b. Function-frame reclamation instead of scope regions.** Function call boundaries
  *do* exist in the executed language today (`src/wasm_backend.rs:1061`) — unlike scopes.
  The only escape at that boundary is the single returned handle, which can be copied
  forward into the caller's region. That is a genuinely small escape analysis (one value)
  versus the general one, and it addresses the plan's stated motivating case — the
  long-running loop that grows without bound — because loop-body allocations are frame-local.
  Note that `Rc` aliasing still leaks under this scheme; that limitation should be stated
  rather than discovered later.
- **1c. Full scope-tied regions** — blocked on a scope construct existing, and on real
  escape analysis. Its own issue.

**Uncertainty, stated plainly:** I have not established whether frame-tied reclamation is
sound against the `Rc`-aliasing route (`src/lower.rs:103`) or the `frames` /`bindings` side
tables (`src/wasm_backend.rs:374`, `:557`). Those need a dedicated pass before 1b is
committed to. What is *not* uncertain is the headline: values escape today by return, by
join, and by inlined task bodies, and no amount of care in the region implementation changes
that without an analysis pass that does not exist.
