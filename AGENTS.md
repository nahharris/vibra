# AGENTS.md

Guidance for contributors working on the Vibra soft reboot.

## Current phase

The active tree is specification-first. Do not restore, copy, or incrementally
repair the pre-v1 implementation. It is preserved under `archive/pre-v1/` only
for archaeology. New implementation work begins only through an active
milestone in `docs/roadmap/v1.md` and must target the active specification.

## Authority order

1. `docs/spec/00-charter.md`
2. the topic-specific documents in `docs/spec/`
3. `docs/roadmap/v1.md`
4. implementation and tests, once they exist
5. `archive/pre-v1/`, which is never normative

If active documents disagree, stop and correct the specification before
implementing either interpretation.

## Specification changes

- State observable behavior with MUST, SHOULD, and MAY as defined by the
  charter.
- Keep one canonical spelling and one canonical machine contract.
- Update every affected specification chapter and roadmap gate together.
- Record exclusions explicitly; do not leave design questions hidden in code.
- Use `.vib` for source, `.vibon` for persistent data, kebab-case symbols,
  native language forms, nominal effects, and explicit namespaces. Do not add
  a pre-v1 compatibility bridge.

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
