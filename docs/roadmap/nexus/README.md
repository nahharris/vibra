# Nexus implementation plan

Status: planning record, non-normative.

This directory is the design and delivery plan for Nexus, the runtime that a
native Vibra executable links, and for the compiler path that leads to it. It
spans [v0.0.2 M4](../v0.0.2/m4/README.md) (a first native slice),
[v0.0.3](../v0.0.3/README.md) (native executables), and
[v0.0.4](../v0.0.4/README.md) (local concurrency). It sits at the roadmap's
level of the [authority order](../../../AGENTS.md#authority-order): the
specification and the release roadmaps win over it, and where they disagree
with it this directory is corrected. It changes no observable language
behavior.

| Document | Holds |
| --- | --- |
| [design.md](design.md) | The architecture: pipeline, crates, execution IR, the step protocol, values, guards, plans, grants, host calls, linking, and the concurrency layer |
| [decisions.md](decisions.md) | The decision ledger: what was decided, by whom, why, and what was rejected or later replaced |
| [plan-v0.0.2.md](plan-v0.0.2.md) | Work packages X2-01 to X2-04: the native slice of v0.0.2 M4 |
| [plan-v0.0.3.md](plan-v0.0.3.md) | Work packages X3-08 to X3-29: the whole language, three platforms, build and cutover |
| [plan-v0.0.4.md](plan-v0.0.4.md) | Work packages X4-S1 to X4-09, each with its validation criteria |
| [validation.md](validation.md) | The commands, the standing invariants, and the map from gate clause to evidence |

## The plan in three sentences

The interpreter stays permanently, a native compiler and runtime are built,
and the Wasm backend is removed at the native cutover (N39). The native
track follows the release order: one slice in v0.0.2 M4, the whole language
and three platforms in v0.0.3 (N40). Concurrency in v0.0.4 is built on an
actor kernel, with scoped children and a bounded parallel map as the first
things a program author meets (N50).

## What exists now

These crates are in the workspace and covered by the architecture test. They
were built before the language was redesigned (N41), no command of the CLI
reaches them, and none of them is evidence for a gate. Package X2-01
classifies each as kept, changed, or removed.

| Crate | What it is |
| --- | --- |
| `vibra-exec` | The execution IR, its text form, and its verifier (rules V1–V6, and the frame bound of N27) |
| `vibra-nexus` | The runtime core: the value store, runtime frames, the five transfers, the dispatcher with quota and yield, traps, host dispatch through `vibra-host`, and admission |
| `vibra-nexus-eval` | The reference stepper: runs verified execution IR on Nexus with no generated code |
| `vibra-lower` | Lowering of the core forms: literals, sequences, bindings, `if`, calls and tail calls, function values, module values, host calls, `return` |
| `vibra-nexus-bridge` | The C ABI, the launcher with its `system` and `harness` profiles, and the static archive |
| `vibra-codegen` | Cranelift object emission for the slice's instructions, x86-64 only |
| `vibra-build` | MSVC and SDK discovery and the Windows link driver |
| `vibra-actors` | The mailbox, lifecycle, reply-slot, and simulation-scheduler core |

The toolchain observations (Cranelift, object formats, the MSVC link) are in
the [feasibility record](../v0.0.3/m1/feasibility.md). Linux and macOS were
never run.

What does not exist: lowering for the checked IR of the redesigned language,
guards with consequences, plans, grants and the broker, startup acquisition,
the new entry contract, registry major 2, type descriptors, observation of a result,
the Linux and macOS paths, `vibra build`, and the concurrency surface.

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
v0.0.2  M0 contracts ── X2-01 rebaseline ── X2-02 guards, plans ─┐
                                            X2-03 grants, entry ─┴─ X2-04 slice exit (M4)

v0.0.3  X3-10..19, X3-28, X3-29 the whole language (M1)
        X3-08, X3-20..22 three platforms (M2) ── X3-23..27 build, cutover (M3)

v0.0.4  X4-S1 spec ── X4-01..04 kernel + interpreter simulation ─┐   (independent of v0.0.3)
                       X4-05..07 Nexus executor, workers ────────┴─ X4-08..09 gate
                       (X4-05 onward needs v0.0.3 M2)
```

X2-01 depends only on the contracts of v0.0.2 M0 and may start while the
language milestones M1 to M3 are in flight. The v0.0.2 M4 gate is still
claimed after M3's (N40). Stages 1 and 2 of v0.0.4 touch the checker, the
interpreter, and the backend-neutral kernel, and need no native code.
