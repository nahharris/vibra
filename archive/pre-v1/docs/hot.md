---
title: Vibra Documentation Hot Cache
category: orientation
status: current
updated: 2026-08-01
---

# Vibra hot cache

This is the short orientation page for agents and contributors returning to the
repository with limited context. Update it when a major language or repository
contract changes; use [`log.md`](log.md) for the append-only audit trail.

## Current language

- Vibra source and manifests use regular S-expressions; the retired YAML draft
  is historical and lives in [`archive/`](archive/).
- The accepted source contract is [`decisions/s-expression-language.md`](decisions/s-expression-language.md).
- Static effects are specified by [`decisions/effect-system.md`](decisions/effect-system.md).
- The user-facing command and validation surface starts at [`../README.md`](../README.md).

## Repository orientation

- Stable operational guides: [`reference/`](reference/)
- Accepted contracts: [`decisions/`](decisions/)
- Dated reports and migration snapshots: [`status/`](status/)
- Superseded material: [`archive/`](archive/)
- Implementation plans: [`plans/`](plans/)
- Repository-owned agent skills: [`../skills/`](../skills/)

## Maintenance rule

When behavior changes, update the canonical document and focused tests in the
same change. Read [`index.md`](index.md) and [`../AGENTS.md`](../AGENTS.md)
before adding or relocating documentation.
