# Foundations

Status: design direction; not normative
Line: mostly [1.x](README.md#1x--foundations); resources and budgets in
[Vibra 2](README.md#vibra-2--concurrent-services)

Verification, concurrency, and interactive applications all hit the same v1
restrictions first. This document designs the shared extensions, so that no
track solves them privately. Each section names the v1 rule it relaxes and why
the relaxation keeps accepted programs valid.

## Effect-row polymorphism

**V1 rule relaxed:** effect variables and polymorphic rows are excluded, so a
higher-order function declares the exact callback row it accepts, and `iter`
`map`/`filter` accept only pure callbacks.

**Why every track needs it:** `process.spawn` must accept a body with any row
and perform that row. An OTP-style `server` interface must let each
implementation declare its own effects. `par.map` must be generic over
callbacks, even though it requires them to be pure. UI commands carry work
with arbitrary rows. Without row variables, each of these becomes a family of
monomorphic copies.

**Design direction:** a row variable is a generic name whose bound is the
predeclared kind `row`. It is declared in the same flat `where:` list and
used inside effect rows:

```vibra
(defn each (items (array t) visit (fn (t) void effects: (e))) void
  where: (t any e row)
  effects: (e)
  ...)
```

- A row variable appears only inside `effects:` rows and function types.
- An effect row becomes a set of roots plus at most one row variable. Two
  variables in one row would need row unification with disjointness
  constraints, which gives no benefit for the targeted APIs.
- Instantiation substitutes a closed row, so a binary target's performed row
  is always closed and target admission is unchanged.
- Inference fills row arguments from callback types exactly as generic type
  arguments are inferred, and `types:` may supply them explicitly.
- Interface contracts may take row parameters, so `(defint server where: (e
  row) ...)` bounds every callback of one implementation by one row.
- A function value widens to a function type with a larger written row at a
  typed boundary, as a fourth widening relation. This is sound, because a row
  bounds what may happen. It is needed so that callbacks with different rows
  can share one collection, such as a supervisor's child specifications. As
  with every v1 widening, it fires only from a written expected type and never
  computes a least upper bound.

**Compatibility:** every v1 program has only closed rows, which stay valid.
The `iter` defaults may later gain effect-polymorphic variants under new names.
Changing `map` itself would alter accepted signatures.

**Rejected alternative:** algebraic effect handlers. They answer a different
question (how an effect is implemented) and add resumable control flow that
complicates the interpreter/Wasm parity contract. Handlers stay unscheduled
until a track shows a need that injection-based test providers and processes
cannot meet.

## Generic implementations and associated types

**V1 rule relaxed:** an `impl` target must be closed, so `(impl (from t) ...)`
over a generic receiver parameter is unwritable, and associated types are
excluded.

Generic implementations follow the existing ownership and bound-agnostic
overlap rules unchanged: an implementation over `(box t)` overlaps with one
over `(box i32)`, and overlap is still decided at the declaration. This is a
1.x candidate because it needs no new identity kind.

Associated types stay deferred for the reason recorded in `v1.md`: they create
per-implementation identities that no atom path addresses. The track reopens
only if the `try-from` error-type limitation shows up as a real cost in user
code. The expected fix then is a contract-level type parameter with a
default, which keeps every identity addressable.

## Resources and handles

**V1 rule relaxed:** host operations are value-in/value-out, and there are no
handles, streams, sockets, or scoped lifetimes.

Sockets, file streams, timers, windows, and GPU surfaces all need a long-lived
host object. Vibra has no ownership system, and adding affine or linear types
would be the largest type-system change in the roadmap. The recommended design
avoids that change by following BEAM ports: **every handle is owned by exactly
one process.**

- A handle is an opaque nominal value (`(socket s)`, `file-stream`) created by
  an effect operation and owned by the process that created it.
- Using a handle from a process that does not own it, or after it is closed,
  returns a typed error (`@not-owner`, `@closed`). It never traps or reaches
  undefined behavior.
- Ownership can be transferred explicitly by an effect operation
  (`handle.transfer`), which is how an acceptor hands a connection to a worker.
- When a process exits, the runtime closes every handle it owns, in a defined
  order that is recorded in the audit trace.
- Handles can be sent in messages only as a transfer. A copy of a handle value
  that is not the owner's reference is inert.

This keeps values immutable and needs no borrow checker. Lifetime errors
become typed, testable failures, and supervision cleans up after crashes. A
single-process program (every v1 program) owns all its handles through the
implicit root process.

**Study item:** an affine `once` qualifier for handles whose double use is a
logic error, such as a one-shot reply. It would be a checked refinement on top
of the ownership model above, not a replacement.

## Network and child processes as values

The first network and process effects need no handles and fit the v1 ABI:

| Root | Operation shape |
| --- | --- |
| `net.http` | request record in, response record or typed error out |
| `net.dns` | name in, address array or typed error out |
| `subprocess.run` | program, argument array, stdin bytes, and environment map in; exit status, stdout, and stderr out |

Each operation is one registry entry with an owning root and an audit event.
`subprocess.run` accepts no shell string: the program is a path and the arguments
are an array. Streaming variants wait for resources and processes in line 2.

## Execution budgets and reductions

**V1 rule relaxed:** no language-defined fuel, memory, or host-operation
budgets.

Line 2's preemptive scheduler counts **reductions**, one per function
application, as BEAM does. The same counter delivers budgets:

- a process may be spawned with a reduction budget, a mailbox bound, and a
  heap bound;
- exhausting a budget terminates that process with a structured exit reason,
  which its supervisor handles; and
- reductions are counted identically in the interpreter and in Wasm, so
  budget exhaustion is a portable, deterministic result rather than a host
  event.

The v1 statement that stack exhaustion from non-tail recursion is a host event
stays true for the root process of a single-process program.

## Packages and publishing

V1 dependencies are local paths or exact Git revisions. If a registry is
added, the recommended selection algorithm is **minimal version selection**:
each dependency declares minimum versions, and the build picks the smallest
version that satisfies them all. It is deterministic, needs no SAT solver, and
the lock still records exact content hashes. Publishing must never run package
code. The published unit is the canonical source tree plus its `.vibon`
metadata and, from line 3 onward, its proof status.

## Foreign code

Native FFI, raw Wasm FFI, and user-defined host providers remain excluded. New
host capability arrives as toolchain-owned, versioned registries (for example
a `vibra_ui_v1` registry in line 4), each entry with an owning effect root and
audit shape. If evidence later shows that closed registries block adoption,
the candidate design is a sandboxed component with a declared effect
signature, checked at the component boundary. It is not an unchecked import.
