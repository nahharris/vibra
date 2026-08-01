---
title: Vibra Documentation Index
category: index
status: current
updated: 2026-08-01
---

# Vibra documentation

This directory is Vibra's linked, repository-local knowledge base. Start here
when you need to understand a language contract, locate an operational guide,
or update documentation as the compiler evolves.

## Start here

- [Project README](../README.md) — user-facing overview, quick start, and the
  implemented CLI surface.
- [Philosophy](decisions/philosophy.md) — design principles for the language and its
  tooling.
- [S-expression language contract](decisions/s-expression-language.md) — the
  accepted source-language contract.
- [Effect-system contract](decisions/effect-system.md) — the accepted static
  effect rules and host-import boundary.
- [Project layout](reference/project-layout.md) — manifests, targets, imports,
  dependencies, and test discovery.
- [Agent skills](reference/agent-skills.md) — the two repository-owned skills
  distributed from `skills/`.
- [Hot cache](hot.md) — the shortest current orientation for returning agents.
- [Documentation log](log.md) — append-only structural maintenance history.

## Source-of-truth map

| Topic | Canonical source | Status |
| --- | --- | --- |
| User-facing commands and implemented examples | [`README.md`](../README.md) | Current |
| Design principles and language trade-offs | [`decisions/philosophy.md`](decisions/philosophy.md) | Current decision contract |
| Language syntax and typing | [`decisions/s-expression-language.md`](decisions/s-expression-language.md) | Accepted contract |
| Static effects | [`decisions/effect-system.md`](decisions/effect-system.md) | Accepted contract |
| Project manifests, imports, and dependency workflow | [`reference/project-layout.md`](reference/project-layout.md) | Current reference |
| WebAssembly and host boundary | [`reference/wasm-abi.md`](reference/wasm-abi.md) and [`reference/static-wasm-ffi.md`](reference/static-wasm-ffi.md) | Current reference |
| Machine-readable interfaces | [`schemas/`](../schemas/) | Tooling contract |
| Standard library API | [`stdlib/README.md`](../stdlib/README.md), `stdlib/src/`, and matching tests | Current implementation |
| Test conventions | [`tests/README.md`](../tests/README.md) | Current reference |
| Roadmaps, migration reports, and gap analyses | [`status/`](status/) | Non-normative snapshots |
| Superseded designs and handoffs | [`archive/`](archive/) | Historical only |

When code, tests, schemas, and prose disagree, treat executable behavior and
the relevant schema/test contract as evidence of the current implementation,
then update the prose or explicitly record the transition. Do not use a status
or archive document as the current language specification.

## Documentation map

### Reference

Stable guides for implemented behavior and integrations live in
[`reference/`](reference/):

- [Conditional compilation](reference/conditional-compilation.md)
- [Project layout and dependencies](reference/project-layout.md)
- [`.vapp` format](reference/vapp-format.md)
- [Version solving proposal](reference/version-solving.md)
- [WebAssembly ABI](reference/wasm-abi.md)
- [Static WebAssembly FFI](reference/static-wasm-ffi.md)
- [Typed runtime plugins](reference/runtime-plugins.md)
- [Async host operations and structured concurrency](reference/async-structured-concurrency.md)
- [MCP server](reference/mcp.md)
- [Editor support](reference/editor-support.md)
- [Container images](reference/containers.md)
- [Native distribution](reference/distribution.md)
- [Agent skills](reference/agent-skills.md)

### Decisions

Accepted contracts and durable design decisions live in
[`decisions/`](decisions/). A decision is not current merely because it is
well-written: its status line must say that it is accepted, and implementation
and tests must be kept aligned with it.

- [Vibra philosophy](decisions/philosophy.md)

### Status

Time-sensitive reports live in [`status/`](status/). Each report must include a
date or explicit status and should be updated, replaced, or archived when its
claims stop describing the current state.

- [Open issue roadmap](status/open-issues-roadmap.md)
- [Kernel and standard-library gap analysis](status/kernel-stdlib-gap-analysis.md)
- [S-expression migration status](status/s-expression-migration-status.md)

### History and working records

Superseded specifications, migration transcripts, and handoffs live in
[`archive/`](archive/). Agent implementation plans remain under
[`plans/`](plans/) as working records; they are not
language contracts. Machine-readable fixtures remain under
[`test-vectors/`](test-vectors/).

- [Retired YAML-surface draft](archive/yaml-surface-draft.md)
- [Reverted capability-host ABI design](archive/capability-host-abi-design.md)
- [S-expression migration handoff](archive/2026-07-25-s-expression-handoff.md)
- [Dependency-solver vectors](test-vectors/dependency-solver-vectors.json)

### Plans

Implementation plans live in [`plans/`](plans/) and are execution aids rather
than current contracts:

- [Plan directory guide](plans/README.md)
- [Type-system and ADT foundation plan](plans/2026-05-05-type-system-adt-foundation.md)

## Update protocol

When a change affects the language, standard library, compiler CLI, runtime,
schemas, or documented workflow:

1. Find the canonical document in the source-of-truth map before editing.
2. Update that document in the same change as the implementation or test.
3. Update [`README.md`](../README.md) when the user-facing quick start or
   command surface changes.
4. Update the relevant schema and focused tests when an interface or behavior
   changes; a status note is not a substitute for either one.
5. Add new prose to the appropriate folder, link it from this index, and avoid
   new loose documentation files at the repository root.
6. Move superseded material to `archive/` and mark historical/status documents
   clearly instead of leaving competing source-of-truth copies.

Agents and contributors should read this file and [`AGENTS.md`](../AGENTS.md)
before changing language behavior or creating documentation.
