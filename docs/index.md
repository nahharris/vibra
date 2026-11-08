# Vibra documentation

This index covers the active Vibra implementation, its specification, and
its release roadmaps.

## Normative specification

Read the chapters in order:

1. [Charter and specification boundary](spec/00-charter.md)
2. [Source language](spec/01-source-language.md)
3. [Type system](spec/02-type-system.md)
4. [Effects](spec/03-effects.md)
5. [Programs and packages](spec/04-programs-and-packages.md)
6. [CLI, MCP, and code tooling](spec/05-tooling.md)
7. [Runtime and WebAssembly](spec/06-runtime.md)
8. [Diagnostics and conformance](spec/07-diagnostics-and-conformance.md)

`MUST`, `MUST NOT`, `SHOULD`, `SHOULD NOT`, and `MAY` carry their RFC 2119
meanings where written in uppercase. Examples are normative when introduced
with “must accept”, “must reject”, or an explicit rule; otherwise they are
illustrative.

## Delivery

- [Release roadmap and versioning](roadmap/README.md)
- [v0.0.1 (shipped, M1–M4)](roadmap/v0.0.1.md)
- [v0.0.2 (provisional, M5–M7)](roadmap/v0.0.2.md)
- [Milestone execution model](roadmap/execution.md)
- [Milestone 1 step plan](roadmap/milestone-1/README.md)
- [Milestone 2 step plan](roadmap/milestone-2/README.md)
- [Milestone 3 step plan](roadmap/milestone-3/README.md)
- [Pre-M4 specification changes](roadmap/pre-m4/README.md)
- [Milestone 4 step plan](roadmap/milestone-4/README.md), including Step 21b
  and the [M4 exit evidence](roadmap/milestone-4/exit-evidence.md)
- [Research scratchpad](scratchpad/README.md) (unscheduled, non-normative)

The M2 [supported and deferred surface inventory](roadmap/milestone-2/supported-surface.md)
and [Step 14 exit-evidence report](roadmap/milestone-2/14-exit-evidence.md)
record the implemented subset, implementation revisions, and gate results.

The roadmap orders implementation. It cannot weaken the specification. If a
milestone exposes a missing or incoherent design, the specification is changed
and reviewed before implementation continues.

The execution model and the per-milestone step plans record how roadmap work
is decomposed, branched, and tracked. They are process guidance and carry no
normative weight over `spec/`. The scratchpad holds research proposals.
Implementation begins once an approved specification change and a release
roadmap define the work.

## Historical material

- [Archive policy and inventory](../archive/README.md)
- [Pre-v1 snapshot note](../archive/pre-v1/ARCHIVE.md)

Everything under `archive/` is non-normative. Similar names or syntax in the
archive do not imply compatibility.

## Change protocol

A proposal that changes observable language behavior must:

1. update the affected specification chapters;
2. state whether the change alters the specification boundary or a roadmap gate;
3. add or update conformance cases once the harness exists;
4. update CLI/MCP schemas when a machine contract changes; and
5. avoid implementing two accepted spellings or a silent migration bridge.

The specification remains authoritative when code is incomplete. For
behavior a release claims to support, divergence from the specification is
an implementation defect.
