# Beyond v1

Status: long-term direction; not normative
Applies after: the 1.0 release gate in [`../v1.md`](../v1.md)
Last updated: 2026-09-23

This directory records where Vibra goes after v1 and why, so the v1 design can
avoid closing doors that later lines need. It ranks below the specification
and the v1 roadmap in the authority order. It defines no language behavior. No
feature named here may appear in the v1 implementation, which the charter's
exclusion rule forbids, and each one begins with a specification change
reviewed under the index's change protocol.

The documents are:

| Document | Question it answers |
| --- | --- |
| This file | Which release line delivers what, in which order, and what v1 must decide first |
| [Foundations](foundations.md) | Which language extensions every later track depends on |
| [Verification](verification.md) | How Vibra programs carry machine-checked contracts and proofs |
| [Concurrency and distribution](concurrency.md) | How Vibra adopts BEAM-style processes, supervision, parallelism, and clusters |
| [Interactive applications](interactive-apps.md) | How Vibra builds GUIs, terminal UIs, and graphics |
| [Execution targets](execution-targets.md) | How far WebAssembly carries these goals, and when a native backend is justified |

## North star

V1 makes an agent's change locally checkable along three axes: types,
effects, and machine-readable tooling. Every later line adds an axis without
weakening the existing ones:

1. **Stronger checks.** Contracts, termination, and proofs let an agent state
   what code means and have the toolchain verify it, not just what shape the
   code has.
2. **Wider programs.** Supervised processes, parallel pure computation, and
   clusters let Vibra write the services its charter targets, in the
   fault-tolerant style Erlang and Elixir proved.
3. **New surfaces.** Interactive applications and graphics bring the same pure,
   testable, queryable model to programs with a user interface.

The thesis stays the same: the language spends complexity on checkable
semantics and tooling, not on surface convenience.

## Principles for every post-v1 line

These follow from the charter and constrain every track below.

- **Library entities before syntax.** New capabilities arrive as standard
  library types, interfaces, and effect roots wherever possible. A new reserved
  expression form would collide with existing user names, so it needs a major
  source version. Processes, parallelism, UI, and most verification features
  below need no new expression form.
- **Effects stay static and complete.** Spawning, messaging, networking, UI,
  and GPU work are nominal effect roots. A binary target's effect array remains
  its complete consent, including every process it may start.
- **Determinism extends. It is not traded away.** Concurrency is deterministic
  for fixed source, inputs, and ordered host responses. The scheduler's choices
  and the network's deliveries become ordered host responses, which makes
  deterministic simulation and replay possible.
- **One canonical spelling.** A post-v1 feature that duplicates an existing
  idiom, such as an `async` keyword beside processes, is rejected.
- **Additive first.** A line prefers changes that turn previously rejected
  programs into accepted ones. A change that alters the meaning of accepted
  source, the ABI, or a persistent format bumps that component's major version,
  as the compatibility policy requires.
- **Agent-facing evidence.** Every new check reports through the diagnostic
  registry, every new fact is available to `vibra query`, and every new failure
  mode has a structured counterexample or trace, never only prose.

## Release lines

After 1.0 the source language, project-data format, machine schemas, artifact
format, and host ABI version independently. A **release line** is a product
milestone that states which component majors it changes. A **1.x** release is
additive in every component. A **Vibra N** line may bump component majors, and
it must list each bump.

```text
             1.0
              |
    1.x  Foundations ------------------------------+
    (effect polymorphism, contracts in tests,      |
     property tests, network/process values,       |
     generic impls, package publishing)            |
              |                                    |
    2    Concurrent services <--- vibra_v2 ABI     |
    (resources, processes, supervision, timers,    |
     sockets, simulation, pure parallelism,        |
     static contracts and termination)             |
              |                                    |
       +------+-------------+                      |
       |                    |                      |
    3  Distributed        4  Interactive           |
       and proven            applications          |
    (clusters, wire       (MVU apps, TUI, web,     |
     types, lemmas,        desktop, 2D graphics)   |
     interface laws)                               |
       |                    |                      |
       +---------+----------+                      |
                 |                                 |
            Horizon (evidence-driven)  <-----------+
    (GPU subset, native backend, hot upgrade,
     Lean bridge, automatic parallelism)
```

Lines 3 and 4 depend on line 2, not on each other. They are numbered in
priority order because the charter targets services first. Maintainers may swap
them. Nothing in line 4 requires distribution.

### 1.x — Foundations

Theme: remove the v1 restrictions that every later track hits first, without
changing the ABI major or the meaning of accepted programs.

Scope:

- effect-row polymorphism for higher-order functions and interface contracts
  (see [Foundations](foundations.md#effect-row-polymorphism));
- generic `impl` blocks, and a decision on associated types;
- `def` inside `deftype`;
- contracts (`requires:`, `ensures:`) checked during `vibra test`, and
  property-based tests through `for-all:` on `test` (see
  [Verification stage V0](verification.md#v0--executable-contracts-and-properties));
- value-in/value-out network requests and child-process runs as new roots and
  registry entries (`net.http`, `subprocess.run`), with no handles;
- environment writes and monotonic sleep; and
- package publishing and a version solver, if user evidence shows exact Git
  revisions are the main adoption barrier.

Demo gate: a command-line tool that fetches JSON over HTTPS, transforms it
through a generic, effect-polymorphic pipeline, and ships property tests whose
failing inputs shrink to a reported minimal counterexample.

Component impact: source minor, registry minor within `vibra_v1`, project
schema minor (see [decisions](#decisions-to-close-before-10)).

### Vibra 2 — Concurrent services

Theme: long-running, fault-tolerant services in the Erlang/Elixir tradition,
with static effects and deterministic testing, plus the first statically
verified code.

Scope:

- scoped resources and handles, which sockets, timers, and windows require
  (see [Foundations](foundations.md#resources-and-handles));
- lightweight isolated processes, typed mailboxes, typed request/reply,
  links, monitors, and exit reasons;
- supervisors and the `server` behaviour interface, following OTP;
- timers, TCP listeners, and an HTTP server built on processes;
- reduction-based preemptive scheduling, which also delivers the v1-excluded
  execution budgets;
- a deterministic simulation scheduler with fault injection and replayable
  traces;
- explicit pure data parallelism (`par.map`, `par.reduce`);
- static verification of contracts, termination checking, and invariant
  newtypes (see [Verification stage V1](verification.md#v1--static-contracts-and-termination));
  and
- the `vibra_v2` host ABI, which adds suspension, arena transfer, and
  scheduler events.

Demo gate: a supervised HTTP key-value service whose request parser is
statically verified. A worker crash is restarted by its supervisor. A
simulation test explores message interleavings, finds a seeded ordering bug,
and its recorded schedule replays the failure deterministically in both
backends.

Component impact: host ABI major (`vibra_v2`), artifact format major, source
minor.

### Vibra 3 — Distributed and proven

Theme: clusters of Vibra nodes with typed, evolvable messages, plus proofs
beyond single-function contracts.

Scope:

- nodes, location-transparent process addresses, and cluster membership;
- `wire` types, a binary VIBON wire encoding, and message-type evolution
  rules;
- distributed links, monitors, supervision, and process groups;
- authenticated transport with node identities rather than a shared cookie;
- simulation with partitions, delay, reordering, and node crash;
- lemmas, induction, and interface laws as proof obligations (see
  [Verification stage V2](verification.md#v2--lemmas-and-interface-laws));
  and
- proof status recorded in lock and build metadata, so a dependency's verified
  claims are auditable.

Demo gate: a three-node replicated store survives a partition in simulation.
A law-carrying `ordered` implementation is proven. A dependency's proof
status appears in `vibra query`.

### Vibra 4 — Interactive applications

Theme: user interfaces as pure model-view-update programs over processes.

Scope:

- the `app` interface (init, update, view, subscriptions) and a `@app` target
  kind;
- a declarative UI tree with a mandatory accessibility tree;
- a terminal host first, then a browser host, then a desktop host;
- a 2D canvas as immutable draw lists, with animation frames as messages;
- declared, hashed assets; and
- headless, deterministic UI tests that act through the accessibility tree.

Demo gate: one application source runs as a terminal UI and in a browser.
Its headless tests pass identically on both hosts. An agent drives it through
accessibility-tree queries alone.

### Horizon

These tracks are unscheduled. Each needs evidence before it gets a line:

- a GPU shader and compute subset of pure Vibra;
- a native AOT backend from the same typed IR;
- hot code upgrade for long-running services;
- a Lean bridge for obligations that the SMT solver cannot discharge;
- automatic parallel evaluation of pure code in the style of interaction-net
  runtimes such as HVM and Bend;
- verified or translation-validated compilation; and
- self-hosting.

Macros, reader extensions, and runtime plugins remain excluded. They weaken the
canonical-spelling and local-reasoning guarantees that agents depend on. The
bar for reopening them is higher than for any track above.

## Decisions to close before 1.0

The pre-1.0 policy allows deliberate breaks, which later lines will not have.
The M7 forward-compatibility review MUST close each item below, either by
changing the v1 specification or by recording why the item can wait for a
major version.

1. **Additive evolution of persistent formats.** `@project.v1`,
   `@project-lock.v1`, and `@build.v1` reject unknown fields and newer majors.
   Nothing yet defines how a 1.x toolchain adds an optional field or a target
   kind (`@service`, `@app`) without a major bump. Decide on minor versions or
   explicitly accept a major bump per addition.
2. **Registry evolution within `vibra_v1`.** Decide whether adding a `@host`
   or `@compiler` registry entry is a registry minor version, and how a build
   records the minimum registry version it needs.
3. **Audit-trace extensibility.** The ordered audit trace becomes the replay
   log for scheduling and network delivery in line 2. Decide now that its
   encoding is versioned and tolerates new event kinds.
4. **Effect-row representation.** The effects chapter already notes that flat
   rows relax additively. Keep typed IR, query schemas, and build metadata
   free of assumptions that a row is always a closed literal set, so row
   variables can be added in 1.x.
5. **Reserved expression spellings.** New expression forms need a source major.
   Record that post-v1 tracks use library entities, and list any spelling the
   maintainers still want reserved. Candidates are none by default.
6. **Interface laws.** Confirm that the laws M3 documents for `equatable`,
   `ordered`, `hashable`, and `iter` are the ones line 3 will turn into
   obligations.
7. **Value-arena transfer.** Confirm that opaque value indices never leak
   instance identity into typed IR or build output, so line 2 can move values
   between instances and threads.
8. **Test grammar.** Confirm that labelled additions to `test` (`for-all:`)
   stay additive under the canonical formatter.

## Evaluating and scheduling a track

A track moves from this directory into a real roadmap when:

1. user evidence or a charter goal justifies it over the alternatives;
2. its prerequisites have shipped;
3. a specification change is drafted for every affected chapter, including
   diagnostics, schemas, and conformance profiles; and
4. a line roadmap with milestones, demo gates, and exit gates replaces the
   sketch here, following the [execution model](../execution.md).

The syntax in these documents is illustrative. It shows that a design fits the
v1 grammar. It does not reserve or specify a spelling.
