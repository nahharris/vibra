## Pre-implementation findings — early return vs. scope teardown

Investigation of the risk that `(try expr)`'s early return bypasses structured-concurrency
scope teardown and leaks resources. Short version: **the risk as stated does not exist,
because there is no scope teardown on any executed path.** What the investigation *did*
find is a different, real, pre-existing hole that `try` will make far more frequent.

---

### 1. How `return` is implemented today

Two independent mechanisms, one per backend.

**Interpreter** (`src/execute.rs`, `#[cfg(test)]`-only for whole programs — see below):
a four-variant signal enum threaded through the statement walker.

- `enum ExecFlow { Next, Return(RuntimeValue), Break, Continue }` — `src/execute.rs:925`
- `Statement::Return(expr)` evaluates and yields `ExecFlow::Return` — `src/execute.rs:997`
- `run_block` short-circuits on any non-`Next` flow — `src/execute.rs:1426`
- `$while` / `$for` re-propagate it outward verbatim — `src/execute.rs:1060`, `src/execute.rs:1075`
- `exec_call` converts it to the call's value at the function boundary — `src/execute.rs:1903`

Propagation is pure early-`return` out of Rust functions. **There is no unwind hook, no
`Drop` guard, no defer list, and no cleanup callback anywhere on this path.** A statement
that wants teardown-on-exit has nowhere to register it.

**Wasm backend** (`src/wasm_backend.rs`, the production path): `Statement::Return` emits
the expression then a raw `Instruction::Return` — `src/wasm_backend.rs:1297`. A wasm
`return` unwinds every enclosing `block`/`loop`/`if` in the frame at once. Again, no
epilogue is emitted, so there is no place a teardown call would currently land.

**Which backend actually runs:** `execute::run_lowered` delegates straight to the wasm
backend (`src/execute.rs:20`). `run_lowered_interpreted` is gated `#[cfg(test)]`
(`src/execute.rs:25`). The tree-walking interpreter's task machinery therefore only runs
under `cargo test`. `exec_call` is still called from the backend (`src/wasm_backend.rs:510`)
but bails on user-function bodies (`src/wasm_backend.rs:497`), so it only services host imports.

### 2. What closing an async scope actually does

`src/async_runtime.rs` implements a full, careful teardown. `Scheduler::close_scope` —
`src/async_runtime.rs:819`:

1. counts non-terminal tasks for the report;
2. `cancel_scope(scope, ScopeClosed)` — `src/async_runtime.rs:763` — recurses into child
   scopes as `ParentCancelled`, calls `cancel_operations`, and forces every non-terminal
   task to `TaskOutcome::Cancelled`;
3. `cancel_operations` — `src/async_runtime.rs:691` — pushes `HostCommand::Cancel` per
   pending op; `Guaranteed` ops are dropped and their token slot released,
   `DrainRequired` ops move to `OperationState::Draining`;
4. recurses into child scopes;
5. closes resources **in reverse creation order**, skipping any resource still retained by
   a draining operation and counting it as `leaked_resources` — `src/async_runtime.rs:857`;
6. emits `ScopeCompleted` only when nothing is still draining — `src/async_runtime.rs:876`.

Retained resources are swept later by `finalize_closed_scope` — `src/async_runtime.rs:886` —
once the last late completion drains, which then recurses to the parent.

Deadline path: `advance_to` collects scopes whose deadline has elapsed and cancels each with
`CancelReason::Deadline` **before** delivering task completions at that instant —
`src/async_runtime.rs:725`. Covered by `deadline_wins_over_completion_and_propagates`
(`src/async_runtime.rs:1130`) and
`deadline_cancellation_drains_late_completion_before_resource_cleanup`
(`src/async_runtime.rs:1279`).

**This machinery is not wired to anything.** `open_scope` / `open_scope_with_limits` /
`close_scope` / `open_resource` / `start_operation` have **zero** call sites outside
`src/async_runtime.rs` and its own `mod tests`. Verified by grep across `src/` and `tests/`.

The only production-adjacent consumer is `SourceTaskRuntime` (`src/execute.rs:933`), which
constructs `Scheduler::new([])` and uses **only the root scope** — `self.scheduler.root()`
at `src/execute.rs:956`. It never opens a child scope and never calls `close_scope`. The
whole runtime is a thread-local (`src/execute.rs:980`) reset by wholesale replacement
(`src/execute.rs:985`), which drops the `Scheduler` without teardown.

There is also **no `scope` surface form** in the language. Grep of `src/ast/surface.rs`
turns up `task`, `spawn`, `join` (lines 1797, 1809, 1818) and no scope construct.
`docs/reference/async-structured-concurrency.md:190` is explicit that milestones 1–5 are
not implementation-complete.

### 3. Does an early `return` from a task / spawn / open scope run cleanup today?

Taking the three cases separately:

**Inside `(task ...)` — statically impossible.** `task_body_has_escaping_control`
(`src/lower.rs:2895`) treats `Return`, `Break`, and `Continue` as escaping and recurses
through `if` / `match` / `while` / `for` / nested `task`. Lowering rejects the body:
`E-TASK-002: '$task' body cannot return or use loop control across its task boundary`
(`src/lower.rs:5844`). The interpreter has a belt-and-braces runtime check with the same
code at `src/execute.rs:1088`. Verified empirically — `(task (return 5) captures: ())`
fails to compile with exactly that message.

**Inside `(spawn ...)` — syntactically impossible.** `spawn`'s `value` is parsed with
`parse_expr` (`src/lower.rs:5751`), not as a block. `return` is a statement, so it cannot
appear there.

**Past a live spawn handle — allowed, and it leaks.** This is the real finding.
`validate_task_handles` (`src/body_semantics.rs:23`) tracks live handles through
`spawn`/`join`, requires both `if` branches and all `match` arms to consume the same set,
and asserts the live set is empty **at the end of the whole body** (`src/body_semantics.rs:78`).
It has no `Statement::Return` arm — `Return` falls into the `_ => {}` catch-all at
`src/body_semantics.rs:73`. `return` is not modelled as a scope exit at all.

So this compiles and runs clean today (verified against the built binary):

```
(defn early (flag bool) int64
  (spawn h 42 captures: ())
  (if flag (do (return 1)) (do (let x 0)))
  (join h r)
  (return r)
)
(defn main () void (let v (early true)))
```

`vibra run` exits 0. The `if` arms agree (`{h}` on both sides), the trailing `join`
empties the set, the end-of-body check passes — and the `return 1` path exits the function
with `h` never joined. In the interpreter that permanently retains an entry in
`SourceTaskRuntime::handles` and `::results` (`src/execute.rs:936`) plus a live `Task` in
the `Scheduler`. In the wasm backend `spawn` is just `LocalSet` and `join` just `LocalGet`
(`src/wasm_backend.rs:1397`, `src/wasm_backend.rs:1405`), so nothing leaks there yet —
but the affine contract the compiler claims to enforce is already violated on the source level.

**No test covers this.** `tests/lang-tasks.vib:72`
(`spawned-task-must-be-joined-before-scope-exit`) covers falling off the end of a body with
an unjoined handle. `src/execute.rs:3437`
(`source_spawn_join_uses_scheduler_and_consumes_handles`) asserts `handles`/`results` are
empty after a *balanced* program. Neither exercises an early exit.

### 4. Host handles: are they closed on teardown?

**No. Only explicit `close` closes them — this is pre-existing, and unrelated to scopes.**

- `FileTable` (`src/execute.rs:138`) is a flat `HashMap<u64, FileHandle>` covering files,
  child processes, child stdin/stdout/stderr pipes, TCP streams, TCP listeners, and UDP
  sockets (`src/execute.rs:121`).
- It is created once per `WasmHost` (`src/wasm_backend.rs:312`) and lives for the whole
  program run. It has no scope association of any kind.
- The only removal path is `FileTable::close` (`src/execute.rs:278`), reachable solely
  through the `fd_close` host import (`src/execute.rs:2389`).
- `Scheduler::open_resource` (`src/async_runtime.rs:482`) mints a **synthetic** `ResourceId`
  with no link to any OS handle, and is never called from `execute.rs` or `wasm_backend.rs`.
  `CleanupReport::closed_resources` (`src/async_runtime.rs:246`) counts bookkeeping IDs, not
  file descriptors.

So: **an early `return` past an un-`close`d file, socket, or process pipe leaks that handle
until process exit, today, with no `try` involved.** `(try expr)` makes this dramatically
more frequent because the whole point of the form is to short-circuit out of the middle of
a function that just opened something. It is an amplification of a pre-existing bug, not a
new one — the #248 implementation should say so and not be held responsible for fixing
handle lifecycle, which is #255's subject
(`docs/plans/2026-08-02-handle-lifecycle-spike.md`).

### 5. Tests an implementer must write

**Must write for #248:**

1. **`try` inside a `task` body is rejected.** `(task (try e) captures: (...))` must fail
   with `E-TASK-002` for the same reason `return` does. `task_body_has_escaping_control`
   (`src/lower.rs:2895`) is an exhaustive `match` over `Statement`, so adding a `Try`
   statement (or a `Return`-lowering of `try`) will produce a compile error there — good.
   If `try` lowers to `Statement::Return` this is free; **if it lowers to a new statement
   kind, this check must be extended or the boundary silently opens.** Add a `.vib` case
   in `tests/lang-tasks.vib` with `expect-error: (@compile E-TASK-002 ...)`.
2. **`try` past a live spawn handle is rejected.** Rust test over `body_semantics::validate_task_handles`:
   a body that spawns, then `try`s a failing result, then joins, must be a compile error.
   This requires teaching `validate_task_handles` that `Return` (and `Try`) is a scope exit
   and that the live set must be empty **there**, not only at end-of-body.
3. **The same test for plain `return`** — the regression that already exists. It should be
   written and land red, then green, as part of item 2's fix. Put it in
   `tests/lang-tasks.vib` alongside `spawned-task-must-be-joined-before-scope-exit`
   (`tests/lang-tasks.vib:72`).
4. **Both backends agree.** `try` returning early must produce the same observable result
   under `run_lowered_interpreted` and the wasm backend, because `Return` is implemented
   twice (`src/execute.rs:997` vs `src/wasm_backend.rs:1297`).
5. **`try` interacts correctly with `#247`'s unhandled-result diagnostic** — the plan calls
   this out; it needs its own case, not fallout.

**Explicitly NOT required for #248:** any test asserting that `try` runs scope teardown.
There is nothing to run. Adding a teardown hook is #251 / #255 work.

**Existing coverage to reuse:** `tests/lang-tasks.vib:33-79` for spawn/join affinity and
the unjoined-handle diagnostic; `src/async_runtime.rs:1182`
(`scope_close_cancels_tasks_and_closes_resources_in_reverse_order`) as the model for what a
teardown test looks like *once teardown is reachable*.

---

### Verdict

**The stated risk is not real, but only because the thing it would break is not connected.**
`src/async_runtime.rs` has a correct, tested scope-teardown implementation; nothing in the
executed language ever opens or closes a scope, so early return cannot bypass it. `(try expr)`
can be implemented on the existing `ExecFlow::Return` / `Instruction::Return` mechanism without
any concurrency-teardown work.

**Two real issues surfaced, both pre-existing:**

- `body_semantics::validate_task_handles` does not model `return` as a scope exit
  (`src/body_semantics.rs:23-83`), so an early return past a live spawn handle compiles and
  runs. Verified empirically. `try` multiplies the exposure. **This should be fixed in #248**
  — it is small, it is squarely in the blast radius, and shipping `try` without it widens a
  known hole.
- Host handles are closed only by explicit `fd_close` (`src/execute.rs:278`, `src/execute.rs:2389`);
  no scope, function, or block boundary closes anything. Early return already leaks them.
  **This is #255's problem, not #248's** — but #248 should state it rather than let it look
  like a regression introduced by `try`.

**Uncertainty:** whether `try` will lower to `Statement::Return` or a new statement kind is
the deciding factor for how much of item 1 and 2 above is automatic versus manual. If it
lowers to `Return`, the exhaustive matches in `src/lower.rs:2895` and
`src/body_semantics.rs:23` do most of the work for free.
