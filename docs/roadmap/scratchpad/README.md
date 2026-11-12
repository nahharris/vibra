# Vibra scratchpad

These notes collect unscheduled design ideas and open questions. A proposal can
move into a future v0.0.x [release roadmap](../README.md) once its scope
and prerequisites are clear. The pre-alpha releases will inform the goal for
v0.1.0, the first alpha. The sketches are non-normative, and their syntax is
illustrative.

| Topic | What it explores |
| --- | --- |
| [Foundations](foundations/README.md) | Shared effect extensions, resources, budgets, host operations, and reproducible dependencies |
| [Type system](type-system/README.md) | Agent-facing type guarantees, generic implementations, associated types, interface narrowing, and type-scoped constants |
| [Verification](verification/README.md) | Executable contracts, property tests, static verification, and proofs |
| [Concurrency and distribution](concurrency/README.md) | Isolated processes, supervision, simulation, parallelism, and clusters |
| [Interactive applications](interactive-apps/README.md) | Terminal, browser, and desktop UIs, accessibility, and graphics |
| [Execution targets](execution-targets/README.md) | Nexus follow-ons, earlier WebAssembly alternatives, compiler strategies, and backend parity |
| [Agent tooling](agent-tooling/README.md) | Live debugging, codebase extensions, and type-constrained decoding for source generation |
| [Blue sky](blue-sky/README.md) | An unconstrained rethink: programs as ledgers of claims, execution as a journal, authoring as refinement, with [paradigm field notes](blue-sky/paradigms.md), a [syntax note](blue-sky/syntax.md), and a [tooling note](blue-sky/tooling.md) |

## Open issue intake

| Issue | Scheduled work | Scratchpad |
| --- | --- | --- |
| [#409](https://github.com/nahharris/vibra/issues/409) | The [release map](../README.md) adopts versioned outcomes and exit gates. v0.0.2 leads with the agent workflow and v0.0.3 owns the native transition. | [Stronger type guarantees for agents](type-system/README.md#stronger-guarantees-for-agents) and [full dependency delivery](foundations/dependencies.md) |
| [#408](https://github.com/nahharris/vibra/issues/408) | [Portable build inputs](../v0.0.2/m5/README.md), [Nexus and the cutover](../v0.0.3/README.md), then [local actors](../v0.0.4/README.md). | [Nexus rationale and follow-on research](execution-targets/nexus.md) |

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
[charter](../../spec/00-charter.md#deliberate-exclusions). They weaken the
canonical spelling and local reasoning guarantees that agents depend on.
The bar for reopening them is higher than for the tracks above.

## Milestone dependencies

Some proposals build on decisions recorded in the release roadmaps. The
roadmaps and specification own those obligations.

| Research need | Roadmap obligation | Milestone |
| --- | --- | --- |
| Interface laws become proof obligations | Laws for `equatable`, `ordered`, and `iter` are written and exercised | [M3](../v0.0.1/m3/README.md) |
| Values move between instances and threads | Value indices never reach typed IR or build output | [M4](../v0.0.1/m4/README.md) |
| New host operations | Registry minor-version rule and recorded registry requirement | M4 |
| Scheduling and delivery replay | Versioned audit encoding that tolerates new event kinds | M4 |
| Effect-row polymorphism | No closed-literal assumption on rows in IR, queries, or build data | M4 |
| New target kinds and fields | Evolution rule for project, lock, and build formats | [Dependency research](foundations/dependencies.md) and the [native build contract](../v0.0.3/m3/README.md) |
| New expression forms | Review reserved spellings before a source change | Owning specification proposal |

Labelled additions such as `for-all:` on `test` need no reservation because
an unknown label is already rejected and accepting it later is additive.

## Moving a proposal into a release roadmap

1. Record the user evidence or charter goal that justifies the proposal.
2. Verify its prerequisites have shipped.
3. Draft the affected specification changes, including diagnostics, schemas,
   and conformance profiles.
4. Assign the work to a release and define its demo and exit gates using the
   [execution model](../execution.md).
