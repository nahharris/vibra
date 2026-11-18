# AGENTS.md

Guidance for contributors working on the Vibra soft reboot.

## Current phase

The active tree is specification-first. Do not restore, copy, or incrementally
repair the pre-v1 implementation. It is preserved under `archive/pre-v1/` only
for archaeology. New implementation work begins only through an active
milestone in a release plan indexed by `docs/roadmap/README.md` and must
target the active specification.

## Authority order

1. `docs/spec/00-charter.md`
2. the topic-specific documents in `docs/spec/`
3. the release roadmaps indexed by `docs/roadmap/README.md`
4. implementation and tests
5. `docs/roadmap/scratchpad/` and `archive/pre-v1/`, which are non-normative

If active documents disagree, stop and correct the specification before
implementing either interpretation.

## Specification changes

- State observable behavior with MUST, SHOULD, and MAY as defined by the
  charter.
- Keep one canonical spelling and one canonical machine contract.
- Update every affected specification chapter and roadmap gate together.
- Record exclusions explicitly; do not leave design questions hidden in code.
- Use `.vib` for source and `.vibon` for persistent data. In source, write
  types in PascalCase and values in camelCase, with keyword-led forms, typed
  closers, written effect ceilings, and explicit imports. Registered protocol
  tags keep their spelling. Do not add a compatibility bridge to an earlier
  surface.
- Prefer a compile-time check or an error as a value over a trap. Do not
  introduce a dependency cycle in imports, packages, initializers, bindings,
  or crates.

## Implementation changes

Once a roadmap milestone is active, each behavior change must include its
focused conformance case, diagnostics/schema updates, and canonical prose in
the same change. A feature is not complete when only its parser, checker,
runtime, CLI, or documentation exists.

The future repository must provide two independent suites: host-language tests
for the implementation and Vibra conformance tests for the language. Run both
before claiming a milestone gate is complete.

## Archive policy

Do not edit archived files except to repair archive metadata or make the
snapshot inspectable. Never cite archived behavior as current support.
