# Step 1 — freeze M2 contracts and corpus observations

Prerequisite: M1 integrated into `main`; use [the baseline checks](validation.md).
This is a specification/infrastructure prerequisite. It does not implement or
claim an executable language feature. No later step begins until the decisions
below have canonical prose, diagnostic/schema contracts, and review evidence.

## Read before editing

- [Source](../../spec/01-source-language.md): **Labels and applications**,
  **Declarations**, **External definitions**, **Functions and expressions**.
- [Types](../../spec/02-type-system.md): **Model**, **Namespaces and resolution**,
  **Functions as values**, **Inference and checking**, **Control flow and failure**.
- [Projects](../../spec/04-programs-and-packages.md): **Project file**,
  **Packages and targets**, **Dependencies and lock**, **Tests**.
- [Tooling](../../spec/05-tooling.md): **V1 CLI**, **Workspace queries**,
  **Transactional edit plans**, **Schemas and errors**.
- [Runtime](../../spec/06-runtime.md): **Evaluation**, **Tail calls**,
  **External providers**; [diagnostics](../../spec/07-diagnostics-and-conformance.md):
  **Diagnostics are a language surface**, **Conformance corpus**, **Conformance profiles**.

## Decision checklist

The starting specifications leave these details incomplete. The suggestions
below are review starting points, not new normative rules or permission to
invent a contract in implementation. Replace each open item with a link to its
reviewed canonical section in this step's PR.

| ID | Close before dependent code | Required decision and evidence |
| --- | --- | --- |
| C1 | All steps | Exact M2 supported-surface table: primitive `bytes`/`atom`, singleton widening, `as`, function types, labelled defaults, variadics, generic `types:`, nonempty effects, `result void e` entries. Keep collection/nominal/failure completion in M3; explicitly assign every overlap in `v1.md`. |
| C2 | 2–3, 11–12 | Full closed `@project.v1` field/type/requiredness/order and nested atom-slot role/kind tables. Version/name validation, path syntax, discovery start/stop rules, multiple/zero target selection, and diagnostics with origins. Schema decode accepts valid future dependency data without silently resolving or executing it. |
| C3 | 3–4, 12 | Checking scope, deterministic file/declaration/diagnostic order, root symlink/junction policy, layout failures, import cycles, private access, duplicate declarations/aliases and no-shadowing codes. Preserve specified unknown-path vs wrong-kind distinctions. |
| C4 | 5–7 | Codes and spans for type mismatch, ambiguous numeric inference, invalid condition/result/default, duplicate/missing/unknown arguments, shadowing, and unsupported features. Define recovery/cascade policy; reuse an existing code only if its condition matches. |
| C5 | 6 | `def` initialization order, visibility to later/earlier declarations, cycle detection through function calls, and no-body/empty-body values. Prefer an explicit acyclic constant dependency model for review; never silently use host evaluation order. |
| C6 | 7, 10–11 | Exact function-type equality/call contract including labels and defaults on indirect values; canonical binding/formatting and safe-fix representation. Syntax binding helpers do not supply a resolved signature by themselves. |
| C7 | 8–9 | Closed versioned M2 intrinsic table: symbol, signature, total semantics, float/Unicode edge cases, and source authority. No checked arithmetic that returns `result` until its nominal types exist. Include a legal terminating source workload for tail stress. |
| C8 | 8, 11–13 | Offline stdlib bootstrap and toolchain-signed provenance verification before M5's dependency delivery. Specify the exact trusted input, tamper checks and how source explicitly imports it. Never trust a filename, package name, source annotation, or case flag as signature evidence. |
| C9 | 13 | Test module identity, relationship to target roots, import/visibility rules, deterministic discovery/selection, duplicate test names and result contract. Define how pure assertions produce test failures without hidden host effects, general exceptions, generics, or fabricated `result` types. |
| C10 | 11–13 | Exact implemented command grammar/options/defaults, result atoms, numeric process exits, init destinations, fmt preview/write contract, and `run --format json` routing consistent with program-owned stdout. Preserve revision checking and atomic writes. |
| C11 | 4–14 | Canonical resolved/type/execution/test/query observations and schemas, source-file identity on multi-file spans, structured traps distinct from ordinary outcomes, exact/recovered/unavailable facts, and snapshot revision semantics. No Rust `Debug` wire format. |
| C12 | All steps | Partial M2 profile admission vs unsupported execution: explicit availability diagnostics for deferred valid forms, no dialect/profile renaming, no full-v1 claim, and no green gate produced by skipping unavailable cases. |

## Ordered work

1. Build a clause-to-decision table from the sections above. Distinguish a
   specification already deciding behavior from a missing contract. Do not
   reopen settled rules just because an implementation shortcut would be easier.
2. Close C1–C12 in their owning specification chapters and update affected
   roadmap boundaries together. If a decision changes the v1 boundary, resolve
   it through the documented specification change protocol before coding it.
3. Update the diagnostic registry and schema producer/consumer tests for the
   reviewed additions. Establish canonical fixtures for the planned observations.
4. Extend the neutral manifest/runner only where necessary: declared multi-file
   inputs and diagnostic origins, operation selection, and semantic snapshots.
   Preserve path confinement, unknown-field rejection, optional expectations,
   reader dispatch, and zero-unavailable exit behavior. Update corpus prose.
5. Test the infrastructure using synthetic observations: two files with the
   same byte offsets remain distinct; reordered/missing diagnostics fail;
   deliberately wrong type/value/audit/query snapshots fail; escaped paths fail.
   Do not add executable corpus cases for language handlers that do not exist.
6. Record the fixed subset and exact commands in this directory. All later
   guides must agree with those decisions. Run the [common checks](validation.md).

Done: all twelve items have reviewed specification links and machine-contract
evidence, infrastructure tests pass, and M1's independent corpus still passes.
An unresolved item keeps this step `in progress`; prose listing the questions
alone is not completion. The current planning bootstrap intentionally leaves
this step `not started`.
