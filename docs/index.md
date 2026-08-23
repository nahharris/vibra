# Vibra v1 documentation

This index is the source-of-truth map for the Vibra soft reboot. The active
documents specify the language that will be built; they do not describe the
archived compiler.

## Normative specification

Read the chapters in order:

1. [Charter and v1 boundary](spec/00-charter.md)
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

- [Roadmap to a usable v1](roadmap/v1.md)

The roadmap orders implementation. It cannot weaken the specification. If a
milestone exposes a missing or incoherent design, the specification is changed
and reviewed before implementation continues.

## Historical material

- [Archive policy and inventory](../archive/README.md)
- [Pre-v1 snapshot note](../archive/pre-v1/ARCHIVE.md)

Everything under `archive/` is non-normative. Similar names or syntax in the
archive do not imply compatibility.

## Change protocol

A proposal that changes observable language behavior must:

1. update the affected specification chapters;
2. state whether the change alters the v1 boundary or a roadmap gate;
3. add or update conformance cases once the harness exists;
4. update CLI/MCP schemas when a machine contract changes; and
5. avoid implementing two accepted spellings or a silent migration bridge.

The specification remains the authority when code is incomplete. After v1 is
released, implementation divergence is a defect in the implementation, not an
implicit amendment to the language.
