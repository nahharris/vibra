# Milestone 3 — WebAssembly products and pre-alpha release

Outcome: Vibra ships WebAssembly build products and a toolchain that meets the
`full-v1` conformance profile.

Deliverables:

- `vibra build` `.wasm` and canonical `@build.v1` `.build.vibon` products over
  the v0.0.1 M4 lowering, with the closed scalar-only `@host` ABI;
- deterministic source maps, required-effect metadata, and build fingerprints;
- byte-identical build tests across two clean environments;
- platform packaging for the explicitly supported matrix;
- fuzz, performance, and ABI-boundary audits, with performance compared against
  the v0.0.1 M4 baseline;
- the runtime chapter's representation latitude — wrapper erasure, compact enum
  layouts, and in-place update of unshared values — enabled only after
  unoptimized parity and measured against the v0.0.1 M4 baseline;
- release documentation containing only implemented commands and examples; and
- a forward-compatibility review against [the scratchpad](../../scratchpad/README.md#milestone-dependencies)
  that confirms the arena, registry, audit-trace, effect-row, format-evolution,
  and interface-law obligations through v0.0.2 M1, and decides whether any
  expression spelling must be reserved before v1.0.0. Under the stable
  compatibility policy, a new reserved expression form requires a source
  major version.

Exit gate:

- the [full conformance gate](../../../spec/07-diagnostics-and-conformance.md#full-conformance-gate)
  passes for the `full-v1` profile;
- two clean environments produce byte-identical artifacts;
- no raw Wasm, WASI, or unknown import crosses the specified ABI boundary;
- unoptimized parity is complete before any optimization is enabled;
- archive references are absent from release behavior and compatibility claims;
- the forward-compatibility review is recorded with its decisions; and
- the release audit is reviewed before v0.0.2 is tagged.
