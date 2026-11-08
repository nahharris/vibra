# Foundations

Status: design direction; not normative

Verification, concurrency, and interactive applications share several
prerequisites. These sketches describe the current restrictions and how
candidate extensions could keep accepted programs valid.

## Effect-row polymorphism

**Current restriction.** Effect variables and polymorphic rows are excluded, so a
higher-order function declares the exact callback row it accepts, and `iter`
`map`/`filter` accept only pure callbacks.

**Why every track needs it:** `process.spawn` must accept a body with any row
and perform that row. An OTP-style `server` interface must let each
implementation declare its own effects. `par.dict` must be generic over
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
  with existing widening rules, it fires only from a written expected type and never
  computes a least upper bound.

**Compatibility:** every currently specified program has only closed rows, which stay valid.
The `iter` defaults may later gain effect-polymorphic variants under new names.
Changing `map` itself would alter accepted signatures.

**Rejected alternative:** algebraic effect handlers. They answer a different
question (how an effect is implemented) and add resumable control flow that
complicates the interpreter/Wasm parity contract. Handlers stay unscheduled
until a track shows a need that injection-based test providers and processes
cannot meet.

## Resources and handles

**Current restriction.** Host operations are value-in/value-out, and there are no
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
single-process program owns all its handles through the
implicit root process.

**Study item:** an affine `once` qualifier for handles whose double use is a
logic error, such as a one-shot reply. It would be a checked refinement on top
of the ownership model above, not a replacement.

**Study item:** a static use-after-close check by in-danger propagation, which
taints every value that may share a closed handle and rejects its later use.
It would move some `@closed` results from test time to check time without
linearity. It starts with a spike on whether the sharing analysis stays
tractable over Vibra's value semantics, and it too refines the ownership model
rather than replacing it.

**Study item:** typestate for handles, a phantom state parameter on a handle
type such as connected or closed, so an operation in the wrong state is a type
error. It is sound only with the affine `once` qualifier listed above, because
without it the old-state value stays usable after the transition. It refines
the ownership model rather than replacing it.

## Network and child processes as values

The first network and process effects need no handles and fit the existing host ABI:

| Root | Operation shape |
| --- | --- |
| `net.http` | request record in, response record or typed error out |
| `net.dns` | name in, address array or typed error out |
| `subprocess.run` | program, argument array, stdin bytes, and environment dict in; exit status, stdout, and stderr out |

Each operation is one registry entry with an owning root and an audit event.
`subprocess.run` accepts no shell string: the program is a path and the arguments
are an array. Streaming variants wait for resources and processes described in [concurrency](../concurrency/README.md).

## Execution budgets and reductions

**Current restriction.** The language defines no fuel, memory, or
host-operation budgets.

The proposed preemptive scheduler counts **reductions**, one per function
application, as BEAM does. The same counter delivers budgets:

- a process may be spawned with a reduction budget, a mailbox bound, and a
  heap bound;
- exhausting a budget terminates that process with a structured exit reason,
  which its supervisor handles; and
- reductions are counted identically in the interpreter and in Wasm, so
  budget exhaustion is a portable, deterministic result rather than a host
  event.

For the root process of a single-process program, the outcome of deep
non-tail recursion follows the rule
[Milestone 4](../../v0.0.1/m4/README.md)
established in the runtime chapter.

## Packages and publishing

The specified dependency model uses local paths or exact Git revisions. A
registry and version solver are worth exploring if exact revisions become a
major adoption barrier. The candidate algorithm is **minimal version
selection**. Each dependency declares minimum versions, and the build picks
the smallest version that satisfies them all. It is deterministic, needs no SAT solver, and
the lock still records exact content hashes. Publishing must never run package
code. The published unit is the canonical source tree plus its `.vibon`
metadata, with proof status if [verification](../verification/README.md)
can report it.

## Foreign code

Native FFI, raw Wasm FFI, and user-defined host providers remain excluded. New
host capability arrives as toolchain-owned, versioned registries (for example
a `vibra_ui_v1` registry for UI hosts), each entry with an owning effect root and
audit shape. If evidence later shows that closed registries block adoption,
the candidate design is a sandboxed component with a declared effect
signature, checked at the component boundary. It is not an unchecked import.

## Other effect and host extensions

Environment writes and monotonic sleep could begin as additive registry
entries. Signals, streaming handles, and asynchronous operations need further
design. Sealed effect roots and declared dominance or sub-effect relations
would extend today's flat rows, which have an empty relation.

## Other distribution work

Signing, publishing build products, and multi-artifact application containers
are unscheduled. The v0.0.2 draft separately includes packaging the toolchain
for supported platforms.

## Candidate demo

A command-line tool fetches JSON over HTTPS and transforms it through a
[generic](../type-system/README.md), effect-polymorphic pipeline. Its
[property tests](../verification/README.md#v0--executable-contracts-and-properties)
shrink failing inputs to a minimal counterexample. This could fit an additive
source, registry, and project schema revision if existing programs and
artifacts keep their meaning.
