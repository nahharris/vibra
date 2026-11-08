# Vibra scratchpad

These notes collect unscheduled design ideas and open questions. A proposal can
move into a future v0.0.x [release roadmap](../roadmap/README.md) once its scope
and prerequisites are clear. The pre-alpha releases will inform the goal for
v0.1.0, the first alpha. The sketches are non-normative, and their syntax is
illustrative.

| Topic | What it explores |
| --- | --- |
| [Foundations](foundations/README.md) | Shared effect extensions, resources, budgets, host operations, and package publishing |
| [Type system](type-system/README.md) | Generic implementations, associated types, interface narrowing, and type-scoped constants |
| [Verification](verification/README.md) | Executable contracts, property tests, static verification, and proofs |
| [Concurrency and distribution](concurrency/README.md) | Isolated processes, supervision, simulation, parallelism, and clusters |
| [Interactive applications](interactive-apps/README.md) | Terminal, browser, and desktop UIs, accessibility, and graphics |
| [Execution targets](execution-targets/README.md) | WebAssembly, native backends, compiler strategies, and backend parity |
| [Agent tooling](agent-tooling/README.md) | Type-constrained decoding for source generation |

## North star

Vibra makes an agent's change locally checkable along three axes: types,
effects, and machine-readable tooling. Research explores three ways to extend those checks and the programs they
support.

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

## Design principles

These follow from the charter and constrain every track below.

- **Library entities before syntax.** New capabilities arrive as standard
  library types, interfaces, and effect roots wherever possible. Reserving an
  expression form can collide with existing user names. Under the stable
  compatibility policy, that requires a major source version. Processes,
  parallelism, UI, and most verification ideas can use existing forms.
- **Effects stay static and complete.** Spawning, messaging, networking, UI,
  and GPU work are nominal effect roots. A binary target's effect array remains
  its complete consent, including every process it may start.
- **Deterministic concurrency.** Concurrency is deterministic
  for fixed source, inputs, and ordered host responses. The scheduler's choices
  and the network's deliveries become ordered host responses, which makes
  deterministic simulation and replay possible.
- **One canonical spelling.** The process proposal expresses concurrency
  through library operations.
- **Additive first.** A proposal prefers changes that turn previously rejected
  programs into accepted ones. Under the stable compatibility policy, a
  breaking change to source, an ABI, or a persistent format bumps that
  component's major version.
- **Agent-facing evidence.** Every new check reports through the diagnostic
  registry, every new fact is available to `vibra query`, and every new failure
  mode has a structured counterexample or trace, never only prose.
- **Decodable types.** Prefer type-system features whose well-typed
  continuations stay cheap to compute from a source prefix. Explicit
  signatures and closed dispatch keep that search small; a feature that makes
  it expensive needs evidence that outweighs the loss.

Macros, reader extensions, and runtime plugins remain excluded by the
[charter](../spec/00-charter.md#deliberate-exclusions). They weaken the
canonical spelling and local reasoning guarantees that agents depend on.
The bar for reopening them is higher than for the tracks above.

## Milestone dependencies

Some proposals build on decisions recorded in the release roadmaps. The
roadmaps and specification own those obligations.

| Research need | Roadmap obligation | Milestone |
| --- | --- | --- |
| Interface laws become proof obligations | Laws for `equatable`, `ordered`, and `iter` are written and exercised | [M3](../roadmap/v0.0.1.md#milestone-3--complete-nominal-static-core) |
| Values move between instances and threads | Value indices never reach typed IR or build output | [M4](../roadmap/v0.0.1.md#milestone-4--webassembly-spine-static-effects-and-host-operations) |
| New host operations | Registry minor-version rule and recorded registry requirement | M4 |
| Scheduling and delivery replay | Versioned audit encoding that tolerates new event kinds | M4 |
| Effect-row polymorphism | No closed-literal assumption on rows in IR, queries, or build data | M4 |
| New target kinds and fields | Evolution rule for project, lock, and build formats | [M5](../roadmap/v0.0.2.md#milestone-5--reproducible-projects-and-dependencies) |
| New expression forms | Decision on reserved spellings, defaulting to none | [M7](../roadmap/v0.0.2.md#milestone-7--webassembly-products-and-pre-alpha-release) |

Labelled additions such as `for-all:` on `test` need no reservation because
an unknown label is already rejected and accepting it later is additive.

## Moving a proposal into a release roadmap

1. Record the user evidence or charter goal that justifies the proposal.
2. Verify its prerequisites have shipped.
3. Draft the affected specification changes, including diagnostics, schemas,
   and conformance profiles.
4. Assign the work to a release and define its demo and exit gates using the
   [execution model](../roadmap/execution.md).
