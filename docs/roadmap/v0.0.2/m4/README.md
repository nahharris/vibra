# Milestone 4 — Agent debugger and DAP

Vibra ships a debugger interface for agents, with the kind of interactive
control a GDB MCP offers. Editors drive the same debugger through DAP.

Close the debugger state and stop contracts in tooling, runtime, and
conformance before implementation. Review the charter's JSON interoperability
boundary for DAP. Keep protocol-specific identifiers and position conversions
in the adapter.

## Scope

- Add a debugger service over the interpreter and session engine. Pin the
  debuggee's revision, map breakpoints to executable source locations, and
  provide pause, continue, step in, step over, step out, and terminate. Report
  mapped traps and test assertion failures with their stop reason and origin.
- Return logical frames, source addresses, typed locals, and bounded
  read-only value inspection. Define breakpoint binding, stop-scoped value
  handles, and stepping across tail transfers. Show the live activation
  rather than keeping discarded frames alive.
- Ship Vibra-owned MCP tools and a machine CLI interface for launching a
  named run or test from a project or evaluation revision, setting and
  clearing breakpoints, controlling execution, waiting for a stop with a
  bounded wait, and inspecting frames, locals, nested values, and output.
  Define session ownership, cancellation, and cleanup on disconnect. Use the
  corresponding run or test permission and keep the debuggee's static effect
  consent.
- Ship a local, launch-only DAP adapter for editor breakpoint, step, stack,
  and variable inspection. Advertise only supported capabilities.
  [Debug Adapter Protocol overview](https://microsoft.github.io/debug-adapter-protocol/overview.html)

## Gates

- **Demo.** Over MCP, launch a selected failing test, bind a breakpoint
  inside its callee, wait for the stop, inspect a typed argument and the
  source revision, step, and reach the failing assertion. Run the same
  sequence through DAP and compare the stop reasons and source locations.
- **Exit.** A source edit during a stop cannot remap the running revision.
  Resuming invalidates stale frame/value handles. Stepping across a tail call
  observes replacement of the activation, and pausing and resuming produces
  the same canonical outcome and ordered host audit as ordinary execution.
  A read-only MCP server cannot start or control a debuggee.

Remote attach, writable locals, arbitrary evaluation in a stopped frame,
reverse stepping, and native executable debugging stay outside v0.0.2.
