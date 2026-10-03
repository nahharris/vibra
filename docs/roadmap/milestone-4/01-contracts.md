# Step 1 — freeze Stage 4A contracts

Prerequisite: `m4` at or after the bootstrap commit; M3 and the pre-M4 changes
integrated into `main`. This is a specification/infrastructure prerequisite. It
does not implement or claim any language feature and emits no WebAssembly. No
Stage 4A behavior step begins until every decision below has canonical prose,
registry/schema evidence, and review.

## Read before editing

- [Charter](../../spec/00-charter.md): all of it; the decision order governs
  every tie below.
- [Runtime](../../spec/06-runtime.md): all of it. **Tail calls**,
  **Native implementations**, **Representation latitude**, **Canonical value
  encoding**, **Traps**, **WebAssembly boundary**, and **Determinism and
  observability** each change in this step.
- [Types](../../spec/02-type-system.md): **Interfaces and methods**,
  **Generics**, **Iteration**, and the status line naming the forms M3
  reassigned.
- [Tooling](../../spec/05-tooling.md): **M2 command contract**, for the `run`
  and `test` payloads, result atoms, and exit mapping.
- [Projects](../../spec/04-programs-and-packages.md): **Toolchain
  standard-library input**, **Tests**, **Build products**.
- [Diagnostics](../../spec/07-diagnostics-and-conformance.md): the registry
  table, **Conformance corpus**, **Conformance profiles**, **Required
  implementation suites**.
- M3's [decision ledger](../milestone-3/decision-ledger.md) rows D17.3, D19.4,
  D22.2, D27.1, D29.1, and D30.1, its
  [exit evidence](../milestone-3/exit-evidence.md#reassigned-to-later-milestones),
  and the [pre-M4 changes](../pre-m4/README.md) with
  [their checklist](../pre-m4/01-bindings-return-never.md#known-gaps).
- [Beyond v1](../beyond-v1/README.md#what-v1-carries-for-later-lines): the four
  obligations M4 carries, and
  [foundations](../beyond-v1/foundations.md) on reductions and process heaps,
  which bound how D1 and D3 may be decided.

## Decision checklist

The suggestions below are review starting points, not normative rules. Each row
is closed by a link to its reviewed canonical section, recorded in an M4
decision ledger (`decision-ledger.md` in this directory) with one disposition
and one proof owner per atomic row.

| ID | Close before | Required decision and evidence |
| --- | --- | --- |
| D1 | 3, 6 | Deep non-tail recursion (gap G1). Choose (a) a portable activation-depth limit or (b) activations held in the arena, and replace the host-event rule in the runtime, tooling, and conformance chapters. For (a): the limit's value, what counts as one activation, the trap code that replaces `@runtime.host-stack-exhausted`, its command and test results, and the argument that the chosen engine's stack cannot run out first, including for a module run by a host Vibra does not control. For (b): the exhaustion outcome when memory, not depth, is the bound. Suggestion: (a), because it keeps non-tail calls as direct Wasm calls and is the smaller change; (b) is the direction the beyond-v1 process model points, so record why it is not needed now. Needs a maintainer decision. |
| D2 | 5 | The arena (gap G2). State where compound values live, what an index denotes and its width, the closed set of value kinds, what "instance-owned" means for a module with no import, and how a host reads and builds a compound value across a scalar-only boundary. State which exports a v1 module has in Stage 4A and that none is a guest pointer. The Stage 4B host ABI is written against this section, so it must not assume an import. |
| D3 | 5, 6 | The reclamation rule (gap G3): when an index stops being live and its storage is released; that reclamation is unobservable apart from memory use; that an index is never reused within an instance; the outcome of exhausting the index space and of exhausting memory, and whether either is a trap or a host event; and that releasing a deeply nested value uses bounded engine stack. State the bound the exit gate measures for a long allocating tail loop. |
| D4 | 4 | The Wasm module contract for M4 (gap G6): the feature baseline, including whether tail position uses the tail-call instructions or a trampoline; the versioned entry export; no import in Stage 4A; which custom sections M4 emits, if any, and which wait for M7; and byte-identical emission for an identical checked program. |
| D5 | 4 | Dependencies (gap G6): the encoder and the engine, each checked against the pinned toolchain, `--locked --offline` builds, the three CI platforms, licence, and build time. State which crate may depend on the engine. Needs a maintainer decision; a dependency choice is recorded in the README with its reason. |
| D6 | 4 | The Wasm conformance contract (gap G4): how an executable case is run in both backends, how a mismatch is reported, how the runner reports per-backend counts, and how `wasm-v1` relates to `interpreter-v1`. Suggestion: no new expected snapshot; an `interpret`, `workspace-run`, or `workspace-test` case is run by both backends against its one expected result and trace, and a Wasm disagreement fails the case. Define the parity inventory and the disposition a not-yet-lowered case carries until its step. |
| D7 | 4 | Command exposure (gap G5). Suggestion: `run` and `test` execute the reference interpreter in M4 and take no backend option; `build` stays `@tool.unavailable`. |
| D8 | 5, 11 | Traps (gap G9): the closed map from an engine trap to a stable `@runtime.*` code, how a trap carries its source origin without M7's source maps, and the parity rule for `@runtime.invalid-checked-program`, `@runtime.unobservable-function`, and the D1 outcome. |
| D9 | 2, 6, 9 | Instantiation and the reassigned forms (gap G8): how typed IR expresses an abstract contract member with its own generic parameters, a contract member call with labelled operands and written `types:`, and a dict variadic tail on a contract member; and the instantiation strategy the Wasm backend uses, so the IR carries what both backends need. Update the type chapter's status line when Step 2 implements them. |
| D10 | 8, 10 | Native and primitive sources (gap G7): the one form in which a native implementation is written so the interpreter and the Wasm backend both run it, and how a primitive row with no Vibra body, such as float `to-str`, is given code inside a module. State how the body/native differential joins the interpreter/Wasm harness. |
| D11 | 8 | NaN (gap G10): where a NaN is canonicalized so that every observation agrees across backends. |
| D12 | 2–12 | The M4 availability boundary: one M4 surface inventory giving every AST variant and every inherited row an owning step, with a test that fails on an undisposed variant, modelled on `crates/vibra-conformance/tests/m3_contract_inventory.rs`. |
| D13 | 3, 5 | Diagnostic registry changes for Stage 4A: the code D1 introduces or retires, any arena-exhaustion code from D3, and the conditions under which `@runtime.invalid-host-value` is reported. Fix each code's level and origin. Reuse an existing code only when its condition matches. |
| D14 | 2, 21 | Inherited items (gap G11): assign the reserved-word local binder defect and the slow deeply nested value to a step, or reassign each by name. Suggestion: the binder fix joins Step 2, and the nested-value cost is investigated in Step 21 before the baseline is recorded. |
| D15 | All | Status lines: every chapter whose "Implementation status" names M2 or M3 as the current boundary is brought up to date, and the runtime chapter's M2-scoped paragraphs that Stage 4A replaces are rewritten rather than left beside the new rule. |

## Ordered work

1. Build a clause-to-decision table from the sections above, separating rules
   the specification already decides from missing contracts. Do not reopen
   settled rules because an implementation shortcut would be easier.
2. Put D1, D2, D3, and D5 to the maintainer with a recommendation and the
   evidence for it before writing their prose. They constrain every Stage 4A
   step and cannot be closed by an implementer alone.
3. Close D1–D15 in their owning chapters. Following
   [`AGENTS.md`](../../../AGENTS.md), update every affected chapter, the
   diagnostic registry, the conformance rule table, and `v1.md` in this one PR.
4. Update the diagnostic registry crate and schema producer/consumer tests for
   the reviewed codes. Add the M4 inventory and its test.
5. Extend neutral conformance infrastructure only where the new observations
   require it, and test it with synthetic observations. Add no Wasm handler
   and no executable case for a backend that does not exist yet.
6. Write the guides for Steps 2–12 using the
   [required guide contents](../execution.md#required-contents-of-an-implementation-guide),
   an M4 `validation.md` that updates the M3 commands for `m4`, the decision
   ledger, and the surface inventory.
7. Run the M3 [pre-merge checks](../milestone-3/validation.md#before-merging-each-step)
   and confirm the corpus still reports 414 passed, 0 failed, 0 unavailable.
   Refresh `docs/roadmap/milestone-1/syntax-examples.tsv` for the lines the
   specification edits moved, and review the refreshed rows.

## Excluded

- Every Stage 4B contract: the host registry, audit events, registry
  versioning, test providers, entry outcomes, and effect diagnostics. Step 13
  owns them. D2 must leave room for the host ABI without designing it.
- Any optimization, build product, custom section beyond what D4 admits, or
  source map. These are M7's.
- Implementation of any decision. A specification prerequisite does not count
  as implementation of the behavior it defines.

## Done

Every atomic ledger row links to reviewed canonical prose and has registry or
schema evidence; the inventory test passes; the guides for Steps 2–12 and the
M4 `validation.md` exist; the M3 corpus and host suites still pass. A row left
open keeps this step `in progress`. D1, D2, D3, and D5 in particular cannot be
closed by an implementer alone.

## Outcome

The [decision ledger](decision-ledger.md) records D1–D15 with their canonical
anchors and proof owners, the [surface inventory](supported-surface.md) and its
test close D12, [validation](validation.md) replaces the borrowed M3 commands, and
the guides for Steps 2–12 are linked from the [README](README.md#steps). Two
planned steps were split (5 into 5a and 5b, 8 into 8a–8c), with the reason
recorded there. Three items are left to the maintainer's review of the pull
request, not settled by an implementer:

- **D10** (the single source for natives and primitives) chose the Vibra body and
  reclassifies 20 registry rows; the options and costs are in the ledger.
- **The engine version.** Only Wasmtime 47 builds on the pinned 1.94.1 toolchain,
  so Step 4 either starts on 47 or raises the toolchain; see the
  [dependency evidence](README.md#dependency-evidence).
- **The interim bound.** Until Step 3, the interpreter reports its 4,096-activation
  bound as `@runtime.memory-exhausted`.
