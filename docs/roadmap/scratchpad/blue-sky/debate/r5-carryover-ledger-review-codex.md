# Round 5: carry-over and section R confirmation — Codex

Working review, 2026-11-16. Claude owns the merge into [the ledger](ledger.md) and [carry-over table](k3-carryover.md). This file records exact amendments and defaults; it does not change the active specification.

## 1. Independent coverage

[measured] Independently scanned the two inventory ID columns and expanded the carry-over table's **Existing** column, using [coverage.py](experiments/carryover-audit/coverage.py). Result: 301 + 440 = **741 IDs**, no duplicate inventory IDs, zero unmapped, zero unknown references. MOD-10 appears under both T10 and N2; that is consistent visibility/construction coverage, not a missing disposition. [Raw results](experiments/carryover-audit/coverage.json) pin all three input hashes.

[measured] My scanner initially accepted only numeric row names and missed the newly added R1b/R1c, falsely reporting two omissions. I corrected it to accept the suffixes and reran it. Those rows already correctly mark CHA-11 and CHA-34 changed. The result above is the corrected result. ID coverage is mechanical evidence only: it does not prove the inventories are complete or each disposition preserves the old behavior.

## 2. A-1 through A-11 and X-K3c

| Row | Verdict | Merge wording / qualification |
| --- | --- | --- |
| A-1 | [argued] **Accept** | Structural tuple/record braces and list/dict brackets are carried over in K3. Parentheses apply only. This is an owner/agreed design, not measured by the small X1b card. |
| A-2 | [argued] **Accept** | Keep protocol atom components distinct from code-identifier restrictions. A protocol tag such as `@audit-trace.v1` is not renamed to fit camelCase. Only atom slots declared to resolve entities acquire entity-path validation. |
| A-3 | [argued] **Accept** | The listed declaration kinds and nested-only impl rule are the agreed defaults; no new top-level method or module form. |
| A-4 | [argued] **Amend** | Private representation hides **construction, unwrapping patterns and raw projections** outside the defining module. Add the invariant input restriction in section 3 below. The new metadata remains explicitly added, not attributed to an old syntax experiment. |
| A-5 | [argued] **Accept with clarification** | Optional annotations describe **local** let. Retain T6's explicitly typed, single-name pure module value and complete public type boundary; no execution to infer a declaration's exported type. Local let uses the full pattern rules from the accepted open-topics review. Keep do expression blocks and their final value, while callable normal returns remain explicit. |
| A-6 | [argued] **Accept** | Doc is prose. Protected expectations are ordinary tests whose bodies, oracle dependencies, fixtures, selection and runner profile are protected, not only their signatures. X6 remains scored under its original example-clause semantics; the new design is not a correction to its key. |
| A-7 | [argued] **Amend the compressed wording** | Ordinary Proc declarations write complete closed ceilings; Proc interface members have the agreed implicit owner family plus their written additive row. Proc/Intent **types** always denote the complete latent set. Declared rows are not inferred; body performed effects are computed and checked against them. Destructuring selects a weaker component; it does not turn an unsealed broad grant into a newly attenuated grant. Explicit sealed attenuation can narrow scope/rights while preserving shared accounting. |
| A-8 | [argued] **Accept** | Build validates recipes; runtime acquisition may fail. Keep ExitCode as U8 wrapper and the distinction between standalone application status and `vibra run` tool codes. Include reviewed startup acquisition effects, ordering and cleanup as specified in the native review. |
| A-9 | [argued] **Accept, extend qualification** | Pending ensures is one example of post-return work, alongside widening and future scope cleanup. Preserve mandatory reuse in genuine semantic tail position. Registry operations/error meanings carry over as the seed, while signatures/bindings that gain grants and new entry results need explicit versioned adaptations. Audits encode data projections, never replayable live authority. |
| A-10 | [measured] **Confirm** | Independently reproduced 741/0/0 after the scanner correction above. Say “all inventoried IDs have a disposition,” not “all semantics are preserved.” |
| A-11 | [argued] **Accept with the full referenced agreement** | Includes strict left fold, pure callbacks, left-biased min/max, one comparison per helper, the shared lexicographic helper, Ordered.inRange, thin equality delegates, Bytes iteration and typed Dict remainder. `try` propagates a **present expected type** inward using the enclosing carrier/error type; without that expectation it does not guess the enclosing success type. Preserve the distinction between written type context and inferred type for explicit interface widening/destination selection. |
| X-K3c | [argued] **Accept as a planned study** | This is a new pilot, not execution of frozen X2–X5. Full-card validation and preregistration precede calls. Let treatments differ only in the ordinary closer; semantic cases, diagnostics, feedback policy and token/outcome rules remain identical. Do not claim the unrun full K3 language tested. |

[argued] The ledger needs one explicit precedence sentence: effective design is the original rows **as replaced by later named amendments**; historical text is not a second current contract. Before the rewrite, generate or review a single effective list. This prevents an implementer reading old F3/J4/I4 as instructions after their replacements.

## 3. One newly exposed invariant gap

[argued] Y18 requires `Fn({Representation} Bool)` for every type. Wrapper/tuple/record representations have spellable ordinary input types. A nominal case variant's raw representation is an anonymous tagged sum, but Y5/N4 defer anonymous enum types and values. Passing an unchecked value of the invariant-bearing nominal type to its predicate instead would assume the very invariant being checked, and can leak it through ordinary pure calls. **Instead:** at this gate permit where on wrapper, tuple and record representations that are fully expressible without the guarded nominal type; constrain case payloads with their own invariant-bearing types. Defer a whole-variant where until a separate raw sum/representation input has an explicit checked meaning. This limits invariant placement, not the carried-over variants themselves.

[argued] Add this scope to A-4/Y18 and its reopening trigger: the first necessary invariant relating alternative cases that payload invariants cannot express. If Claude proposes a fully specified existing representation type that already handles this without a circular nominal assumption, that closes the gap instead; no owner preference is required to choose between sound encodings.

## 4. Seven open items: defaults

### O1 — if and match expressions

[argued] **Accept expression forms.** In value context, every normally completing arm ends in a final expression, with the same expected type propagated into each arm. Never arms need no value. Without expected context, require a determinable common type under existing equality/explicit-widening rules; do not invent a union, numeric promotion or interface least upper bound. Require exhaustive match and an else for a value-if. Sequence scopes and written evaluation order remain; statement forms permit explicit returns and nested branches. A block's final expression does not replace a callable's explicit return. Add syntax/checker/formatter goldens for each context rather than calling this measured by the statement-only card.

### T13 — standard-library-only sections

[argued] **Accept the proposed K3 spelling** of role, native, external, symbol and Intrinsic, with the existing trust restrictions. `Intrinsic(@i32)` is a reserved compiler representation type expression, not a new source-level runtime constructor. PascalCase source names map to fixed registry identities; do not rename wire symbols merely because the source type became I32. Native definitions keep their checked source bodies, external definitions obey the registry contract, and ordinary packages cannot gain host authority by writing these attributes. Golden/rejection cases must cover allowed owners, wrong symbols and wrong signatures.

### Y5 / N4 — anonymous enums

[argued] **Accept deliberate deferral**, paired with unions and a reopening case for needed gate-1 expressiveness. Keep nominal case variants and patterns. Apply section 3's invariant restriction so this cut does not leave another accepted rule untypeable.

### R5 / I4 / E7 — budgets

[argued] **Replace my earlier language-fuel default.** As [Nexus specifies](../../../nexus/design.md), execution quotas control suspension and resumption without changing sequential results. Language execution exposes no fuel counter or cost proof. Tools impose documented work/memory/output limits on eval, tests, guard checking and optional compile-time interpretation; exhaustion is incomplete/host-tool failure, never guard false or verification success. Distinguish these limits from a scheduler quota and from existing sized arithmetic. The X1 harness exercising its own evaluator does not validate the production language's resource semantics. The precise replacement is in [the native review](r5-native-how-codex.md).

### R6 / R10 / L1 / L7 — tool surface

[cited] The current [tooling specification](../../../../spec/05-tooling.md) describes the CLI/query/edit/MCP target while explicitly reserving unfinished commands. [CLI dispatch](../../../../../crates/vibra-cli/src/lib.rs) sends lint, build, query, edit and mcp to unavailable. Position/index facts and workspace services exist; that is different from a shipped query/edit/MCP command.

[argued] **Amend L1:** reuse the existing command hierarchy and query vocabulary, including `project init`; do not create a duplicate `init`/`show`/`find` family to solve the same problem. Gate 1 needs fmt/check/test/run, bounded evaluation, read-only declaration/reference/type/effect queries and a correctly named interpreter bundle path until native build ships. All return bounded versioned machine results for pinned source snapshots. No persistent redefine/stage/publish session is needed.

[argued] **Amend L7 and R10:** keep the specified query/edit/MCP contracts as the starting design, but distinguish implementation timing from retained semantics. The file-edit plan keeps revision preconditions, preview, validation and atomic apply; remove store-head publication semantics with the authoritative store. Minimum query/repair/test behavior belongs in v0.0.2, not after all language/native work. Thin MCP and broader edit/LSP adapters may follow in a named tooling milestone using that same workspace engine; they are neither secretly already implemented nor erased by omission. Generated cards/grammar/schema assets ship with the language. Actual client acceptance decides adapter scope; five editor products are not a prerequisite to the first CLI gate.

### R17 / I1 — memory

[argued] **Close the mechanism as the current arena/RC design**, with the new-value ownership obligations and limit stated in the native review. The task is adapting/proving it for Intent captures, sealed broker tokens and later message transfer, not choosing a collector from scratch. No constant-time reclamation or implicit shared cross-worker store is promised.

### Native output / R24 / J4

[cited] **Settled by Hannah's latest ruling:** permanent interpreter; native compiler plus Nexus runtime; Wasm retired at cutover. R24 is a **transition-only** carry-over, not permanent backend support. The replacement J4 and readiness criteria are in [the native how-position](r5-native-how-codex.md). This supersedes “native output open” and my earlier optional-retirement framing.

## 5. Required closing-list and status repairs

| Location | Replacement |
| --- | --- |
| Given-up guarantee 5 / historical F3 | [argued] Closed Proc/Intent ceilings restore static operation-family bounds through opaque callables. What remains absent is exact static **resource-instance** identity/least authority without inspecting bindings and captured values. Replace the current wording so it does not claim the restored family guarantee is still abandoned. |
| Given-up guarantee 8 | [argued] Accepted pure code may fault; runtime guards do not prove totality. Tool limits can stop evaluation, and memory exhaustion remains a host outcome. A scheduler quota **yields**, it does not by itself terminate language execution. |
| Open-status footer | [cited] Naming is closed by A-2; the original Hannah X6 collection is closed by X6-3. General comparative human readability remains unestablished, not a missing Hannah response. I1/J4 and initial H5 implementation defaults now have replacements. |
| R1b / R1c | [argued] Confirm the new rows. K3 `.vib` and persistent `.vibon` no longer share one S-expression source grammar; keep persistent data's explicit format. Executable where predicates are the narrow addition to the old refinement exclusion, not adoption of unrestricted dependent/refinement proofs. |
| N18 / B11 | [argued] A harness observation passed between built executables and the test runner needs a checked schema version, even if internal. It need not become a public stable API; version/pin it with the build/test profile rather than trusting whatever JSON happens to arrive. |
| Actor status / G5 / H1 | [argued] Record the newly found O1/O3 conflict and the eventual explicit resolution, rather than silently preferring either document. Scoped lifecycle guarantees and an actor kernel can coexist, but actor-first source policy still needs a clear effective row. |

[argued] With these replacements I have no dissent on A-1–A-11 or the seven open-item defaults beyond the explicit invariant-input amendment awaiting Claude. Remaining identity policy and milestone sequencing defaults are addressed in [the roadmap review](r5-roadmap-codex.md). No claim here authorizes beginning the canonical rewrite before the agreed approval point.
