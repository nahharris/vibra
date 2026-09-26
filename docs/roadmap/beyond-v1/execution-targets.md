# Execution targets

Status: design direction; not normative
Question: how far does WebAssembly carry Vibra's post-v1 goals, and when is a
native backend justified?

## Summary

WebAssembly limits Vibra less than it first appears to, because v1 already put
the important boundary somewhere else. Vibra does not use WASI or raw Wasm
imports. It imports only its own closed, versioned `vibra_vN` host registry
through a scalar-only ABI. **Processes, windows, sockets, and GPUs are
implemented by the Vibra host runtime, not by Wasm.** Wasm limits only what
the *guest* can do cheaply: suspend, share memory, and run SIMD or GPU
kernels.

| Goal | What Wasm constrains | Direction |
| --- | --- | --- |
| Many lightweight processes | Guest stack suspension | Compiler-generated resumable functions at statically known suspension points, with stack switching as a later optimization |
| Multicore parallelism | Shared memory between threads | Share nothing. One instance per scheduler thread, with values copied or transferred through the host arena |
| Distribution | Nothing specific | Networking is host-side, behind effect roots |
| GUI | Nothing in the browser. Outside it, the host owns windows | UI trees cross the ABI as arena values, and the host renders them |
| 2D graphics | Throughput of draw-list building | Immutable draw lists rendered by the host |
| GPU compute and shaders | Wasm cannot target GPUs | A separate lowering of a pure subset to a shader language (horizon) |
| Peak CPU performance | JIT/AOT quality of the embedding engine | A native backend from the same typed IR, only when benchmarks justify it |

## Proposal status is a moving target

Several Wasm proposals bear on this design: tail calls, garbage collection
(WasmGC), threads and atomics, stack switching (typed continuations), the
component model, and asynchronous WASI. Their status in browsers and
standalone engines changes release by release. When a track starts, its
specification MUST record the proposal status it relies on, verified at that
time. Vibra's semantics MUST NOT depend on any of them, because the v1 rule
that the interpreter is the semantic oracle keeps every backend strategy
unobservable.

## Processes over WebAssembly

A process must suspend at a blocking receive, a call, or a timer wait, and
resume later on possibly another scheduler thread. Three strategies are
available. The effect system makes the first one unusually cheap:

1. **Resumable functions (recommended first).** Only a function whose effect
   row contains `process.receive` (directly or transitively) can suspend. The
   checker already computes that row. The compiler transforms exactly those
   functions into resumable state machines whose frames live in the value
   arena. Pure and non-blocking code is compiled as it is today, with no
   overhead. This is the benefit `async` coloring gives other languages,
   derived from rows that already exist rather than from new syntax.
2. **Wasm stack switching.** When it is broadly available, suspension can use
   native continuations and the transform becomes an optimization choice.
   Semantics do not change.
3. **Instance per process.** This gives the strongest isolation, which
   untrusted code needs, at a memory cost far above a BEAM process. It is kept
   for sandboxed or tenant-separated processes, not as the default.

The scheduler is host code. It owns run queues, reductions, timers, and
mailboxes, and it runs one guest instance per scheduler thread. Processes on
one thread share that instance's code but never share values except through
the host.

## Moving values between instances

The v1 ABI already passes composite values as opaque indices into an
instance-owned arena. Line 2 extends it, as the `vibra_v2` ABI, with:

- **transfer**, which moves a value graph from one instance's arena to
  another's, as a deep copy or a transfer of an immutable buffer. Because
  values are immutable and identity is unobservable, a copy and a share are
  indistinguishable;
- **suspension**, the entry points and custom sections that resumable
  functions and the scheduler need; and
- **scheduler events** in the audit trace, which record choices for replay.

The v1 rule that value indices never leak instance identity into typed IR or
build output is what makes transfer possible. It is one of the
[obligations v1 carries for later lines](README.md#what-v1-carries-for-later-lines).

## Parallel pure computation

`par.map` splits work across scheduler threads. Each chunk's inputs are
transferred into a worker instance and the outputs transferred back. Copying
makes fine-grained parallelism expensive, so the runtime owns chunk sizing
and falls back to sequential execution below a threshold. WasmGC or shared
immutable buffers can lower the copy cost later without changing semantics.

## User interfaces and graphics

In the browser, Wasm is at home. The host registry maps UI operations to the
DOM or a canvas, as described in
[Interactive applications](interactive-apps.md#hosts). On the desktop and in
the terminal, the Vibra host runtime owns the window or terminal and renders the
UI tree it receives. Neither case needs anything from Wasm beyond passing
values across the ABI.

## When a native backend is justified

A native AOT backend (for example through Cranelift) from the shared typed IR
is a horizon track. Any of these would justify it:

- benchmarks showing that the embedding engine's code quality, not the
  algorithm or copy cost, dominates realistic services;
- startup or memory constraints (embedded devices, many short-lived CLIs) that
  a Wasm engine cannot meet; or
- a platform without a suitable Wasm engine.

It would carry the same obligations as Wasm: consuming only typed IR, lowering
the same closed registries, meeting the tail-call and reduction rules, and
matching the interpreter across the whole corpus. It is a third backend under
one semantics, not an escape hatch to native FFI.

## Pass ordering

V1 ships no optimization or hardening pass. When either arrives, on any
backend, two constraints apply from the start because they are cheap to adopt
and expensive to retrofit:

- **Hardening runs last.** Composing passes preserves only the intersection of
  the properties each one preserves, so a hardening pass placed before an
  optimizing pass can be silently undone. The pipeline order is recorded and a
  test fails when it changes.
- **Two claims stay separate.** A pass may claim that it preserves conformance
  observations. It never claims, and documentation never implies, that it stops
  an attack. The v1 runtime chapter already makes no verified-compilation or
  host-safety claim.

The scalar-only ABI that such work would rely on is already a v1 rule in the
runtime chapter.
