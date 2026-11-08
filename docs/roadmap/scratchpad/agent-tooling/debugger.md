# Live debugging

Schedule live debugging after the [Nexus cutover](../../v0.0.3/README.md)
and the [local actor runtime](../../v0.0.4/README.md). The first design
should use the source and revision identities from v0.0.2 and the
execution contracts demonstrated by Nexus and the actor simulator.

## Runtime contract to settle

- Bind source breakpoints to executable locations in a pinned revision.
  Define where generated code can stop and which source locations and
  live locals the compiler can recover there.
- Define logical frames and stepping across tail transfers, yields, and
  parked actors. Decide which actors pause on a breakpoint and how a
  stopped run handles in-flight host operations. Use stop-scoped frame
  and value handles that expire on resume.
- Preserve ordered host effects across pause and resume. Pin source
  origins even if the workspace changes during the run. Qualify the
  interpreter and native behavior against the same stop contract.

## Agent and editor workflows

Expose launch of a named run or test, breakpoints, pause, continue,
stepping, and bounded inspection of frames, typed locals, nested values,
and output through structured MCP and machine CLI interfaces. Keep
session ownership and cancellation tied to the corresponding run or
test permission. Map the same service to a local DAP adapter for
editor debugging. [Debug Adapter Protocol overview](https://microsoft.github.io/debug-adapter-protocol/overview.html)

Start with a fixture that launches a failing test, binds a breakpoint
inside a callee, inspects a typed local, and steps to the failure. Compare
source locations and stop reasons through MCP and DAP. Add a case where
one actor parks while another reaches a breakpoint, and verify that
resuming produces the expected message and host events.
