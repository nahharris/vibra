# Pre-M4 specification changes

Status: process guidance; not normative
Applies between: [Milestone 3](../v1.md#milestone-3--complete-nominal-static-core) and
[Milestone 4](../v1.md#milestone-4--webassembly-spine-static-effects-and-host-operations)
Last updated: 2026-10-02

Milestone 3 is merged and Milestone 4 has not started. Three changes were
agreed on 2026-10-02 to land before M4 does. This document records them and the
decisions behind them. It carries no normative weight over `spec/`; each change
that alters observable behavior is a specification change under the
[change protocol](../../index.md#change-protocol).

## Why these land between M3 and M4

- Roadmap rule 6: when implementation exposes a design gap, specification
  review happens before more code. M3 exposed gaps in bindings, early exit,
  and tail calls, and the Wasm spine would otherwise be designed against rules
  that are about to change.
- Roadmap rule 7: milestone numbers are stable and a milestone is never
  inserted. These changes are therefore not a milestone. They have no demo, no
  step table, and no exit gate of their own; M4's exit gate stays the only
  merge gate for M4 work.

## Changes

Status values follow the convention in
[the execution model](../execution.md#progress-tracking). A row for a pull
request that has not merged is conditional on that merge.

| # | Change | Status |
| --- | --- | --- |
| 1 | Bindings, early exit, and `never`: `let` becomes a parent-scope binding form `(let pattern value pattern value ...)` valid only as an element of a body sequence; a new `(let-else pattern value fallback)` refutable binding whose fallback must diverge; a new `(return expr)` early-exit form; and a predeclared uninhabited type `never`; checklist: [`01-bindings-return-never.md`](01-bindings-return-never.md) | `landed`, conditional on [PR #352](https://github.com/nahharris/vibra/pull/352) merging |
| 2 | Tail-call guarantee: every call in tail position reuses the current activation, whatever the callee, replacing the narrower rule for a recursive group within one module | `landed`, conditional on [PR #351](https://github.com/nahharris/vibra/pull/351) merging; see [`02-tail-calls.md`](02-tail-calls.md) |
| 3 | Roadmap coverage: this document, the M4 deliverable for deep non-tail recursion, and the beyond-v1 notes | `landed`, conditional on this PR merging |

## Decisions

Hannah made these decisions on 2026-10-02.

### A static ban on non-tail recursion is rejected

A ban was studied and rejected. Three probe programs were written tail-only:

- A nested-data decoder became shorter: one shift/reduce function.
- A pre-order tree printer grew from about 5 to about 14 lines with a
  worklist. Children must be pushed in reverse, a mistake that still
  type-checks.
- A post-order expression evaluator grew from about 6 to about 21 lines: a
  frame enum, two mutually tail-recursive functions, and two frames per binary
  variant. A standard-library post-order fold does not rescue it, because the
  link between a variant and its number of children becomes dynamic.

A ban also cannot cover recursion through function values or interface values
without rejecting the standard `iter` adapters, so any ban would be partial.
The decision is no ban rather than a partial ban. A solver-free recursion
classification stays available as an informational fact; see
[verification](../beyond-v1/verification.md#v0--executable-contracts-and-properties).

### Deep non-tail recursion stays legal, so its outcome is specified first

Because non-tail recursion remains legal, the outcome of exhausting stack depth
must be specified before the Wasm backend exists. Today it is a host event
outside interpreter/Wasm parity. Milestone 4 gains a deliverable that replaces
that rule; see
[`v1.md`](../v1.md#milestone-4--webassembly-spine-static-effects-and-host-operations).

### `never` is added now, as a real type

`never` is not deferred. Rust's never-type history shows the cost of delay: it
came from inference fallback, where an unconstrained type beside a diverging
expression defaulted to unit, and from a stand-in empty type (`Infallible`)
that later had to be unified with the real one. Vibra therefore never infers
`never` for a generic parameter and ships no stand-in empty type.

### `else:` as a label on `let` is rejected

The formatter orders labelled arguments before the bindings, which reads
poorly for a binding form. `let-else` is a separate form with three fixed
operands instead.
