---
title: Vibra v1 Spec Reboot — Design
category: plans
status: proposed
updated: 2026-08-22
summary: >-
  Design for the soft reboot of Vibra: archive the v0 prototype, author a
  layered v1 language specification with a conformance suite, and derive an
  implementation roadmap toward a usable v1.
---

# Vibra v1 spec reboot — design

## Problem

Vibra v0 was implemented feature-by-feature while the language was still being
discovered. The result is a working prototype with uneven maturity: some
subsystems are finished and tested, others were left mid-flight when attention
moved on, and the normative documentation drifted from the corpus and the
compiler (recorded in `docs/research/00-current-state.md`). A programming
language is a specification first; v0 inverted that order.

This reboot does **not** reimplement the language. Its deliverables are:

1. A complete, layered v1 specification.
2. An implementation roadmap toward a usable v1.
3. A clean archive of everything else, checkpointed at the `v0-archive` tag.

## Decisions

These were settled with the project owner on 2026-08-22 and govern all spec
work. Each is binding until explicitly revisited.

| # | Decision |
| --- | --- |
| D1 | **Greenfield authority.** The v1 spec may re-decide anything. v0 contracts, research, and code are evidence and inputs, not constraints. |
| D2 | **S-expression surface is retained.** The reader/grammar foundation stays; v1 fixes the measured ergonomic costs (deep nesting, qualified-name stutter, verbose fallible-call handling) inside the S-expression design space. |
| D3 | **v1 success criterion.** An LLM agent, given only the spec and the tooling, can build, test, and run a real small program (a CLI tool or API client) end-to-end without human intervention. |
| D4 | **Layered spec.** Normative prose + full grammar (EBNF) + semi-formal typing and effect rules + a conformance test suite any implementation must pass. |
| D5 | **Same repository.** `v0-archive` (annotated tag at `2d3cabf`) is the prototype checkpoint. `main` is free to change without preserving the v0 layout. |
| D6 | **v1 scope.** The five pillars — syntax, type system, effect system, CLI/MCP tooling, runtime — plus LSP/editor support, dependency management and packaging, and an FFI/host-plugin story. |
| D7 | **Explicitly deferred past v1.** Runtime capability enforcement, async/structured concurrency, and bounded execution (fuel, memory ceilings, deadlines). The spec must not paint these into a corner, but v1 ships without them. |

## Approach

Archive sweep first, then spec documents in dependency order, then the
roadmap. Chosen over a walking-skeleton spec (which recreates the
implementation-ahead-of-spec failure mode) and over a standalone decision
ledger (decisions made without spec-writing pressure get revisited anyway).

### Phase 1 — Archive sweep

Clear `main` so that everything remaining is either current or clearly marked
input material. All removed content stays reachable through the `v0-archive`
tag and git history.

- Remove the v0 implementation from `main`: `src/`, `tests/`, `examples/`,
  `schemas/`, `Cargo.toml`/`Cargo.lock`, and the `stdlib` submodule.
- Move v0 plans, status reports, and superseded decisions under
  `docs/archive/v0/`. The research corpus (`docs/research/`) stays live as
  spec input.
- Rewrite `README.md`, `AGENTS.md`, and `CLAUDE.md` for the reboot (the
  current ones still describe the pre-cutover YAML surface — a live instance
  of the drift this reboot exists to end).
- Update `docs/index.md` to reflect the new source-of-truth map.
- Prune the stale worktrees accumulated during v0 development.

### Phase 2 — Spec documents

The spec lives in `docs/spec/`, one numbered document per subsystem, written
in dependency order:

| Doc | Scope |
| --- | --- |
| `00-overview` | Language thesis, design principles (successor to `philosophy.md`), spec conventions, conformance rules |
| `01-surface` | Reader, grammar (EBNF), canonical formatting, source encoding |
| `02-types` | Nominal types, ADTs, generics, interfaces, inference boundaries |
| `03-effects` | Effect declaration, inference vs. ceilings, host-boundary rules |
| `04-stdlib-host-abi` | Core library surface, host ABI, FFI/plugin contract |
| `05-tooling` | CLI, MCP, LSP, diagnostics, schemas — the machine interfaces |
| `06-packaging` | Manifests, dependencies, version solving, distribution format |

Rules for every spec document:

- **Opens with a decisions section** that resolves the v0 tensions relevant to
  it (T1–T7 from `docs/research/00-current-state.md`, plus the D2 ergonomic
  fixes), each with rationale. The decision ledger lives inside the spec.
- **Carries its slice of the conformance suite**: named, machine-readable test
  vectors under `docs/spec/conformance/`. A vector states source input and
  required outcome (accept/reject, diagnostic code, formatted form, or
  evaluation result). The conformance suite is the executable meaning of
  "spec complete."
- Uses the v0 research corpus and the archived prototype as evidence when
  choosing between designs.

### Phase 3 — Roadmap

`docs/spec/roadmap.md`, derived after the spec documents exist. Implementation
is sequenced as a **walking skeleton**: the first milestone is the thinnest
end-to-end slice (parse → typecheck → run "hello world" + one fallible host
call, with `fmt`/`test` working), and every later milestone deepens one
subsystem while keeping the end-to-end path green against the conformance
suite. The final gate is D3: the agent end-to-end demo.

## Non-goals

- Reimplementing the compiler or runtime in this effort.
- Formal operational semantics or machine-checked soundness proofs (D4 stops
  at semi-formal rules; rigor can deepen post-v1).
- Preserving compatibility with v0 source, schemas, or CLI flags.

## Risks

- **Spec written without implementation feedback can overreach.** Mitigation:
  conformance vectors are concrete enough to falsify designs on paper, and
  the archived v0 implementation serves as an evidence base for feasibility.
- **Scope creep back into v0's surface area.** Mitigation: D6/D7 are explicit;
  anything not listed in D6 needs a decision-table amendment before it enters
  the spec.
- **The ergonomics fixes (D2) are design work, not transcription.** The
  `01-surface` and `03-effects` documents carry the genuinely open problems
  (result propagation shape, effect inference vs. declaration, name stutter)
  and should be planned with correspondingly more room.
