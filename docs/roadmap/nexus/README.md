# Nexus implementation plan

Status: planning record, non-normative. Written 2026-11-11 on branch
`worktree-nexus-design` from `main` at `c81a2836`.

This directory is the design and delivery plan for Nexus across
[v0.0.3](../v0.0.3/README.md) (native executables) and
[v0.0.4](../v0.0.4/README.md) (local actors). It sits at the roadmap's level
of the [authority order](../../../AGENTS.md#authority-order): the
specification and the release roadmaps win over it, and where they disagree
with it this directory is corrected. It changes no observable language
behavior. Every item that would is listed as a specification prerequisite and
stays open until Hannah approves the specification text.

| Document | Holds |
| --- | --- |
| [goals.md](goals.md) | Every roadmap deliverable and gate clause for Nexus, each with an ID the plans cite |
| [design.md](design.md) | The architecture: pipeline, crates, execution IR, the step protocol, values, host calls, linking, and the actor layer |
| [decisions.md](decisions.md) | The decision ledger: what was decided, by whom, why, what was rejected, and what is still open for Hannah |
| [plan-v0.0.3.md](plan-v0.0.3.md) | Work packages X3-S1 to X3-27, each with its validation criteria |
| [plan-v0.0.4.md](plan-v0.0.4.md) | Work packages X4-S1 to X4-09, each with its validation criteria |
| [validation.md](validation.md) | The commands, the standing invariants, and the map from gate clause to evidence |

## What exists now

The foundation is three crates, in the workspace and covered by the
architecture test. No command of the CLI reaches them.

| Crate | What it is | Proof |
| --- | --- | --- |
| `vibra-exec` | The execution IR and its verifier (rules V1–V6) | `cargo test -p vibra-exec --test it`: one accepted and one rejected program per rule |
| `vibra-nexus` | The runtime core: the value store, runtime frames, the five transfers, the dispatcher with quota and yield, traps, host dispatch through `vibra-host`, and admission | `cargo test -p vibra-nexus`: store balance, bounded-stack release, limit as host event |
| `vibra-nexus-eval` | The reference stepper: runs verified execution IR on Nexus with no generated code | `cargo test -p vibra-nexus-eval --test it`: the M1 demo in execution IR |

The stepper tests already evidence, at the execution-IR level, the M1 clauses
that do not depend on a code generator: an indirect tail transfer between
functions with bounded activation growth, a forced yield at every checkpoint
with no change of observation, exactly one audit event for a host operation
across those yields, a trap with its origin ordinal, depth bounded only by
memory with no native stack, the memory host event, and refusal of invalid
execution IR, an incompatible runtime identity, and an unknown registry
symbol. [validation.md](validation.md#foundation-evidence) maps test to
clause.

What does not exist: lowering from checked IR, the intrinsic and native rows,
type descriptors, observation of a result, the bridge, code generation,
linking, the launcher, and everything of v0.0.4. Those are the work packages.

## How to delegate a work package

Each package is cut so that an agent with no memory of this session can do it.
Give the agent exactly this:

1. the package's section, verbatim;
2. the instruction to read `AGENTS.md`, [execution.md](../execution.md),
   [design.md](design.md), and the ledger rows the package names, and the
   specification sections it names, before editing;
3. the branch to work on and the package's "Depends" packages, already merged;
4. the [standing rules](validation.md#standing-rules) and the
   [pre-merge commands](validation.md#before-merging-a-package).

A package is done only when every line under its **Validation** holds and the
evidence it names is recorded. An agent that cannot make a validation line
hold stops and reports which line and why. It does not weaken the line, change
an expectation to match its output, or mark a case unavailable.

Packages marked **spec** produce specification text for Hannah's review and
are not delegated past a draft. Packages marked **L** are large: split them by
the sub-items they list, one PR each, in the listed order.

## Sequencing in one view

```text
v0.0.3  X3-S1 spec ─┬─ X3-01..04 lowering slice on the stepper ─┐
        X3-S2 feas ─┴─ X3-05..08 bridge, codegen, link ─────────┴─ X3-09 M1 exit
                       X3-10..22 parity by feature (M2) ── X3-23..27 build, cutover (M3)

v0.0.4  X4-S1 spec ── X4-01..04 kernel + interpreter simulation ─┐   (independent of v0.0.3)
                       X4-05..07 Nexus executor, workers ────────┴─ X4-08..09 gate
                       (X4-05 onward needs X3 M2)
```

The two releases overlap in one place only: X4-05 onward runs actors on the
native path that X3 delivers. Stages 1 and 2 of v0.0.4 touch the checker, the
interpreter, and a new backend-neutral crate, and can proceed while v0.0.3 is
in flight ([decisions.md](decisions.md), N1).
