# Nexus and native execution

The [v0.0.2 handoff](../../v0.0.2/m5/README.md) and
[v0.0.3 roadmap](../../v0.0.3/README.md) schedule the initial compiler and
runtime work. [v0.0.4](../../v0.0.4/README.md) schedules local actors,
message capabilities, and multicore scheduling. This note captures the
rationale and unscheduled extensions from the open proposal.
[GitHub #408](https://github.com/nahharris/vibra/issues/408)

## Initial boundary

Compile shared checked IR to verified execution IR, then target objects.
Link those objects with a small launcher and Nexus. Keep runtime frames off
the native stack. A generated step returns completion, yield, or trap at
defined points. A future waiting result needs separate wakeup semantics.
Nexus owns the runtime context and its value handles. The closed registries
own host operation identities, audit behavior, and errors. Native ABI,
registry, build format, and semantic input identity evolve separately.

The motivating fixture is a compiled, effectful program with an indirect
tail call and a forced yield around one audited operation. Test that the
resume point and trap origins match the interpreter and the operation runs
once. Compare a dispatcher with resumable blocks to a simpler native-stack
design against activation bounds and the later scheduling requirements.
Qualify link reproducibility and parity on Windows x64, Linux x86-64,
and macOS arm64 before retiring the Wasm path.

## Follow-on research

| Track | Question and promotion evidence |
| --- | --- |
| Stronger capability and ownership rules | Find a protocol that requires holder binding, revocation, or host-resource scope. Compare checked local actor addresses with stronger rules for transfer, cancellation, and cleanup. |
| Scheduler scaling and supervision | Replay cancellation and a worker crash. Measure parked-process memory and wakeup latency before setting thresholds. |
| Ordered CPU fork/join | Can a pure two-thunk primitive match left-to-right results and trap precedence when run on multiple workers? Show bounded queues and isolation from actor scheduling before adding ordered bulk map or lawful reduce. |
| Interaction nets for pure regions | Can a net reducer run a named pure subset soundly, with the interpreter's trap precedence and origins? Show that it beats the native single-thread baseline and explicit fork/join on array and text workloads. See the [interaction nets note](interaction-nets.md). |
| GPU placement | Can one checked pure region lower to a CPU fallback and an eligible device kernel? Test integer parity and measure upload, kernel, and readback together before admitting floating point or adding a second device target. |
| Distribution | Can nodes match a semantic build to their own compatible artifacts and exchange versioned typed values? Replay a worker loss, partition, and late duplicate with bounded work and explicit retry semantics. |
| Isolation | What OS-enforced boundary is needed for hostile native workloads? Specify a separate threat model and demonstrate the boundary independently of language-level process isolation. |

See the [concurrency scratchpad](../concurrency/README.md) for earlier actor
and distribution sketches and the [type-system scratchpad](../type-system/README.md)
for agent-facing guarantees.
