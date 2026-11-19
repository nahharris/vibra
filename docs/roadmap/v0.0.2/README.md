# Vibra v0.0.2

**Planned pre-alpha release.** The language of the active specification runs
on the reference interpreter with written effect ceilings and bound grants,
an agent can inspect, repair, and retest a project through structured tools,
and one checked slice runs as native code.

| Order | Milestone | Evidence |
| --- | --- | --- |
| M0 | [Specification and transition](m0/README.md) | No contradictory active rule; every supported form has a test, diagnostic, and schema owner; Windows file-scope feasibility recorded. |
| M1 | [Pure language](m1/README.md) | The pure language passes its corpus on the interpreter, including static contract discharge and rejection. |
| M2 | [Operations and authority](m2/README.md) | A command-line program runs with bound grants; forbidden and unbound operations are rejected or denied as specified. The model-writing pilot is run. |
| M3 | [Identity and repair loop](m3/README.md) | An agent starts from a seeded defect, inspects, repairs, and retests it; stale and replaced approvals are rejected. |
| M4 | [First native slice](m4/README.md) | One new checked slice agrees across interpreter, stepper, and a linked Windows executable. |

## Scope and priorities

The shipped v0.0.1 toolchain is the starting point: its workspace snapshot,
diagnostics model, test runner, interpreter, deterministic host world, and
the partial native pipeline in the workspace are kept and adapted. Its
source surface, project record, entry signature, and effect roots are
replaced; nothing accepts both.

Order of concerns:

1. Settle the written contracts first (M0). Implementation packages rely on
   them, so a gap found later returns to the specification.
2. Exercise each feature through one complete slice before widening it:
   parse, check, run, diagnose, format, and test.
3. The static contract checker is new and its rule set is small. Track every
   gate program it rejects; a rejection that the rules ought to accept is a
   specification question, never a reason to add a run-time check.
4. Reuse what exists below checked IR, but do not assume it is unaffected:
   plans, grants, guards with consequences, startup acquisition, and the
   entry result all cross that boundary.

After M0, M1 through M3 are ordered by their gates. Native foundation work
for M4 that depends only on M0's contracts may proceed alongside them; M4's
gate is claimed last.

The interpreter is the execution path of this release. The WebAssembly
backend runs the pure part of this language under the fail-closed transition
profile of the [runtime chapter](../../spec/06-runtime.md#wasm-transition),
beside the interpreter on the same checked IR; it is not extended to grants,
plans, or the new entry contract. This release does not ship `vibra build`
and does not claim the clean-machine distribution gate.

Out of this release: dependency acquisition and locking beyond local inputs,
MCP and editor adapters, concurrency, network operations, a content-addressed
store as an authority, persistent sessions with live redefinition, and native
execution beyond the M4 slice. Local dependencies, the `path` entries of
`@project.v2`, belong to M2; an M1 project has none.

## Release gate

Use one checked-in project: a command-line program that reads text files
under a directory chosen at run time, writes a log under another, and
optionally reports a metric.

1. Check, format, and test it. Its tests obtain every grant from
   `TestWorld`.
2. Run it with `vibra run` under the target's grant bindings. Show: a
   callback limited to the log grant cannot reach the reader; an unbound
   optional grant arrives as `Option.none()`; a required grant that cannot
   be acquired fails startup before the entry runs; a request outside a
   grant's policy returns a typed denial.
3. Start from an independently seeded defect and a failing test. Through
   machine-mode CLI only, read the failure, query the declaration, its type,
   contracts, and effects, preview a revision-checked edit, apply it, and
   rerun the selected test. Show that a stale edit, a replaced test, and a
   changed policy are each rejected.
4. Run the M4 native fixture and compare its result, exit code, and audit
   trace with the interpreter's.

Run both the host suite and the independent Vibra corpus for every supported
profile. Validate every machine output against its schema, including denied,
stale, malformed, and exhausted paths. Publish the supported-surface
inventory, the list of constructs still unavailable, and the release audit
before tagging.

Each milestone needs an approved contract and a step plan under the
[execution model](../execution.md) before its implementation begins.
