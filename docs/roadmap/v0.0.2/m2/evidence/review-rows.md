# M2 step 11 — review of rows, plans and ceilings

Revision `ae09aacc23859013183da4397159c99ca6bf71ac` on
`m2/11-review-rows`.

The three-crate test command exits 0. The corpus command exits 0 with 978
passes and zero failed, unknown or unavailable cases. Both use
`CARGO_BUILD_JOBS=4`, `RUSTFLAGS=-D warnings`, `--locked` and `--offline`.
The source probes use the public source checker and the project probes use
the workspace checker. The scratch drivers link the worktree's compiled
crates and use `CheckedModuleSet::try_new` to revalidate lowered parts.

## Probes

Paths in the table label the scratch sources quoted below. Code references
in the table point to fixtures exercised by the prescribed commands.

| Source | Expected | Observed | Verdict |
| --- | --- | --- | --- |
| [`direct`](#direct-with-guard-prefix) | Accept direct certified result | Accepted and IR validates | Pass |
| [`bareDirect`](#baredirect-with-guard-prefix) | Accept with bare requires discharged | Accepted and IR validates | Pass |
| [`bareRequires`](#barerequires-with-guard-prefix) | `@contract.refuted` when building | One `@contract.refuted` | Pass |
| [`bound`](#bound-with-guard-prefix) | `@contract.unproved` | One `@contract.unproved` | Pass |
| [`twice`](#twice-with-guard-prefix) | Second run has no first-run fact | One `@contract.unproved` | Pass |
| [`returned`](#returned-with-guard-prefix) | No fact from a function returning a plan | One `@contract.unproved` | Pass |
| [`field`](#field-with-guard-prefix) | No fact through a field | One `@contract.unproved` | Pass |
| [`option`](#option-with-guard-prefix) | No fact through `Option` | One `@contract.unproved` | Pass |
| [`callback`](#callback-with-guard-prefix) | No fact through a callback | One `@contract.unproved` | Pass |
| [`namedValue`](#namedvalue-with-guard-prefix) | No fact through procedure value | One `@contract.unproved` | Pass |
| [`anonymous`](#anonymous-with-guard-prefix) | No proven `ensures` available to caller | One `@contract.unproved` | Pass |
| [`closed`](#closed-with-guard-prefix) | No fact through closed contract receiver | One `@contract.unproved` | Pass |
| [`open`](#open-with-guard-prefix) | No fact through open contract receiver | One `@contract.unproved` | Pass |
| [`requiresElse`](#requireselse-with-guard-prefix) | No fact when entry guard can skip body | One `@contract.unproved` | Pass |
| [`shadow`](#shadow-with-guard-prefix) | Reject shadowing | One `@name.redeclaration` | Pass |
| [`rebind`](#rebind-with-guard-prefix) | No fact from a plan after introducing another binding | One `@contract.unproved` | Pass |
| [`recursive`](#recursive-with-guard-prefix) | Do not use unproved recursive `ensures` | `@contract.unproved` at declaration | Pass |
| [`mutual`](#mutual-with-guard-prefix) | Mutual recursion through value cannot certify either result | Two `@contract.unproved` at declarations | Pass |
| [`lastFn`](#lastfn) | Reject value statement | `@type.mismatch` and `@type.missingReturn` | Pass |
| [`lastProc`](#lastproc) | Reject value statement | `@type.mismatch` and `@type.missingReturn` | Pass |
| [`lastTest`](#lasttest) | Reject value statement | One `@type.mismatch` | Pass |
| [`fallback`](#fallback) | One `@type.mismatch` at value statement | Two `@type.mismatch` at bytes `84..88` | F3 |
| [`branch`](#branch) | Return the selected branch value | Accepted and IR validates | Pass |
| [`matchBranch`](#matchbranch) | Return the selected match arm value | Accepted and IR validates | Pass |
| [`alias`](#alias-with-row-prefix) | `@type.argumentMismatch` | One `@type.argumentMismatch` | Pass |
| [`generic`](#generic-with-row-prefix) | Reject applying unconstrained `T` | `@type.notApplicable` | Pass |
| [`aliasGeneric`](#aliasgeneric-with-row-prefix) | Reject mixing rows after unwrapping nominal representation | `@type.argumentMismatch` | Pass |
| [`genericInferred`](#genericinferred-with-row-prefix) | Reject different rows inferred for same `T` | `@type.argumentMismatch` | Pass |
| [`argument`](#argument-with-row-prefix) | Reject wider argument | `@type.argumentMismatch` | Pass |
| [`narrower`](#narrower-with-row-prefix) | Reject narrower argument | `@type.argumentMismatch` | Pass |
| [`annotation`](#annotation-with-row-prefix) | Reject wider annotation value | `@type.mismatch` | Pass |
| [`returnRow`](#returnrow-with-row-prefix) | Reject wider result | `@type.mismatch` | Pass |
| [`fieldRow`](#fieldrow-with-row-prefix) | Reject wider field initializer | `@type.mismatch` | Pass |
| [`listRow`](#listrow-with-row-prefix) | Reject wider list element | `@type.mismatch` | Pass |
| [`optionRow`](#optionrow-with-row-prefix) | Reject wider `Option` payload | `@type.argumentMismatch` | Pass |
| [`dictRow`](#dictrow-with-row-prefix) | Reject mismatched dict value type | `@type.mismatch` | Pass |
| [`defaultValue`](#defaultvalue-with-row-prefix) | Reject mismatched default type | `@type.mismatch` | Pass |
| [`capture`](#capture-with-row-prefix) | Reject a capture returned at a different row | `@type.mismatch` | Pass |
| [`reordered`](#reordered-with-row-prefix) | Accept reordered set | Accepted and IR validates | Pass |
| [`duplicate`](#duplicate-with-row-prefix) | Reject duplicate at second name | One `@effect.duplicateRoot` with first entry related | Pass |
| [`fnForProc`](#fnforproc-with-row-prefix) | Reject `Fn` as `Proc` | `@type.mismatch` | Pass |
| [`procForFn`](#procforfn-with-row-prefix) | Reject `Proc` as `Fn` | `@type.mismatch` | Pass |
| [`optionCeiling`](#optionceiling-with-row-prefix) | Reject unwrapped effectful callback with indirect hop | `@effect.outsideCeiling` for Log via `f` (indirect) | Pass |
| [`listCeiling`](#listceiling-with-row-prefix) | Reject destructured effectful callback with indirect hop | `@effect.outsideCeiling` for Log via `f` (indirect) | Pass |
| [`assoc`](#assoc-with-row-prefix) | Reject effectful callback supplied as associated choice | `@effect.outsideCeiling` for Log via `f` (indirect) | Pass |
| [`stored`](#stored-with-row-prefix) | Reject and stop at stored plan | `@effect.outsideCeiling` for Log via `p` (indirect) only | Pass |
| [`callbackCeiling`](#callbackceiling-with-row-prefix) | Reject and name callback | `@effect.outsideCeiling` for Log via `sink` (indirect) | Pass |
| [`defaultFn`](#defaultfn-with-row-prefix) | Reject and stop at plan returned by pure member | `@effect.outsideCeiling` for Log via `Builder.make(x m)` (indirect) | Pass |
| [`defaultProc`](#defaultproc-with-row-prefix) | Reject a default case outside its owner's row | `@effect.outsideCeiling` for Log via `loud` → `Log.emit` (indirect) | Pass |
| [`nested`](#nested-with-row-prefix) | Reject at anonymous proc's own ceiling | `@effect.outsideCeiling` for Log via `loud` → `Log.emit` (indirect) | Pass |
| [`selfReceiver`](#selfreceiver-with-row-prefix) | Default member may await its own root on `Self` | Accepted and IR validates | Pass |
| [`selfMissing`](#selfmissing-with-row-prefix) | Reject unbound Self receiver and missing root | `@type.unsatisfiedBound` and `@effect.outsideCeiling` for Log via `Log.emit` (indirect) | Pass |
| [`selfProper`](#selfproper) | Reject missing additive root through open receiver | `@effect.outsideCeiling` for Clock via `Log.emit` (indirect) | Pass |
| [`library`](#library) | Embedded effectful proc supplies row (guide) | Embedded `Iter.next` is pure and yields `@effect.invalidAwait` | F6 |
| [`closedNotes`](#closednotes-with-row-prefix) | Mark interface member hop indirect per chapter 07 | `@effect.outsideCeiling` for Log via `Log.emit` with no indirect marker | F2 |
| [`cycleNotes`](#cyclenotes) | Reject and retain at least the reporting operand | `@effect.outsideCeiling` with zero related spans | F1 |
| [`twoRoots`](#tworoots-with-row-prefix) | Report both missing roots and their chains | Log via `both` → `Log.emit` (indirect), Tick via `both` → `Tick.tick` (indirect) | Pass |
| [`twoRootsIR`](#tworootsir-with-row-prefix) | Reject transplanted body | IR rejects with first missing root `Log` | F5 |
| [`guardAwait`](#guardawait-with-guard-prefix) | One `@effect.invalidAwait` | One `@effect.invalidAwait` | Pass |
| [`constAwait`](#constawait) | One `@effect.invalidAwait` in initializer | One `@effect.invalidAwait` | Pass |
| [`constPlan`](#constplan) | Guide asks for a constant diagnostic | Accepted and IR validates | F4 |
| [`constDefault`](#constdefault) | Reject plan in constant-required default | `@type.notConstant` | Pass |
| [`deepRows`](#deeprows) | Accept or diagnose a documented bound | Accepted, 1,000 unused-root warnings, IR validates | Pass |
| [`twoClosures`](#twoclosures) | Two separate invocations of shared plan | Assertion passes with 5 activations (test, 2 closures, 2 plan runs) | Pass |
| [`anonWarning`](#anonwarning-with-row-prefix) | Suppress enclosing unused warning on nested error | One `@type.mismatch` and zero unused warnings | Pass |
| [`argAwait`](#argawait-with-row-prefix) | Only argument evaluation contributes Log | `@effect.outsideCeiling` at argument await via `Log.emit` (indirect) | Pass |
| [`selfAdditive`](#selfadditive) | Reject missing additive root through `Self` | `@effect.outsideCeiling` for Clock via `Log.emit` (indirect) | Pass |
| [`openAssoc`](#openassoc-with-row-prefix) | Reject unspecified associated choice on interface value | `@type.typeArgumentMismatch` | Pass |
| [`ceilings.rs`, 10,000-procedure chain](../../../../../crates/vibra-types/src/check/tests/ceilings.rs#L590) | No host stack overflow | Passes on 256 KiB stack with 10,001 related spans | Pass |
| [`ceilings.rs`, body transplants](../../../../../crates/vibra-types/src/check/tests/ceilings.rs#L692) | Hand-built IR rejects bad ceilings | Passes for 11 splices and the separate impl/test splices | Pass |
| [`effects.rs`, IR negatives](../../../../../crates/vibra-ir/tests/it/effects.rs#L224) | Reject pure await, non-plan await, wider argument row and invalid row root | Prescribed tests pass | Pass |
| [`plans.rs`, construction and reuse](../../../../../crates/vibra-interp/tests/it/plans.rs#L222) | Dropping a plan enters no body, two awaits invoke twice, each await runs guards | Invocation-count checks and guard checks pass | Pass |

## Specification coverage

Chapter [03](../../../../spec/03-effects.md#operation-interfaces) supplies the
requirements below. Corpus case names in this section take the prefix
`V2-EFFECT-` unless otherwise given. The corpus and tests provide executable
coverage alongside the scratch probes.

| Chapter and clause | Evidence |
| --- | --- |
| Operation root is the declaration, shared by generic applications; names and members are not identities | `generic-operation-interface`, `row-duplicate-alias`, `row-not-operation-interface` |
| Owner plus additive row; owner is a duplicate; ordinary procedure writes complete ceiling | `member-additive-row`, `member-owner-in-row`, `impl-owner-in-row`, `outside-ceiling-direct` |
| An impl keeps types, rows and guards; member body fits its row | `impl-keeps-row`, `impl-row-wider`, `impl-row-narrower`, `impl-forwards-wider`; `closed` enforces inherited guard behavior, and generic type and guard checks are in chapter 02 suites |
| Only the embedded library binds host members; body and additive row forbidden, matching owner, known symbol, and exact signature required | Reader external-outside-stdlib case and IR `host_body_on_its_owner`, `atk_ir_host_body_wrong_owner`, `host_body_wrong_signature`, `host_body_with_an_additive_row`; source host diagnostic coverage gap F6 |
| Host/narrowing members are abstract in ordinary impls; held values select their own member | IR `an_implementation_without_the_host_bound_member_is_rejected`, closed model cases, and F6 |
| Row entries resolve to operation interfaces only; duplicates attach to later entry | `row-names-nothing`, `row-names-type`, `row-not-operation-interface`, `row-duplicate`, `row-duplicate-alias`, and `duplicate` |
| Written sites, omitted empty row, purity of fn and module initializer | `row-on-proc`, `row-on-test`, `test-row-empty-written`, `proc-type-empty-row`, `intent-type-result`, `anon-proc-row-invalid`, `anon-fn-performs`, `await-in-fn-not-plan`, `constAwait` |
| Earliest await per missing root and related chain | `outside-ceiling-earliest`, `outside-ceiling-two-roots`, `twoRoots`, `stored`, `callbackCeiling`, F1 and F2 |
| Unused-root warning when written on eligible body; no warning on impl, owner, or type rows, and plan construction contributes nothing | `row-two-roots`, `row-on-test`, `plan-discarded`, `default-proc-member`, `ceilings::no_warning_for_an_impl_member_or_an_owner`, `anonWarning` |
| Exact callable equality, neither subtyping nor row variables | `argument`, `narrower`, `generic`, `aliasGeneric`, `reordered`, and the other value positions in the probe table |
| Metadata canonical path ordering | `row-two-roots` compares its `@types.v2` snapshot |
| Plan application evaluates operands, is cold, and stores callee and arguments | `proc-labelled`, `plan-discarded`, `plans::build_does_not_run`, `plans::a_plan_through_a_record`; host cost stays deferred in F6 |
| Plan can be stored, passed, returned and repeatedly awaited | `proc-plan-bound`, `intent-type-result`, `proc-await-twice`, `twoClosures` |
| Plan retains complete row through interface and stored callable | `member-plan-interface-value`, `defaultFn`, `assoc`, `optionCeiling`, `listCeiling`, `stored` |
| Only proc/test can await; bad context skips operand, bad operand yields invalidAwait | `await-in-test`, `await-in-fn-not-plan`, `await-not-plan`, `guardAwait`, `constAwait`, IR pure-context negatives |
| Await contributes whole plan row; building contributes only work done evaluating operands | `ceiling-exact`, `outside-ceiling-stored-plan`, `plan-discarded`, `twoRoots`, `argAwait` |
| Bare requires is discharged at build; guards with else run on each await | `bareRequires`, `bareDirect`, `requiresElse`, interpreter `requires_else_runs_on_each_await` and `ensures_else_runs_on_await` |
| Named recursion closes for rows | `recursion-closes`, `cycleNotes`, `ceilings::mutual_recursion_reports_in_the_narrower_body`, 10,000-procedure boundary |

## Checker and checked IR

The table includes all 36 accepted `EFFECT` manifests. The source checker
exposes diagnostics and lowered IR. Its performed sets are private
([`Performed::of`](../../../../../crates/vibra-types/src/check/effects/analysis.rs#L122)). The checker column
uses source await sites, resolved signatures and unused warnings as
observable evidence. IR roots are the union of `Facts::awaited` per body
in the revalidated module and test sets. A matching row is agreement on the
observable roots. It does not establish equality of two independently
captured internal sets in bodies exempt from unused-effect warnings (F7).
All 72 sets revalidate.

| Accepted case (prefix `V2-EFFECT-`) | Checker evidence | Nonempty IR rows | Verdict |
| --- | --- | --- | --- |
| [`await-in-if`](../../../../../conformance/cases/EFFECT/V2-EFFECT-await-in-if/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`await-in-match-and-do`](../../../../../conformance/cases/EFFECT/V2-EFFECT-await-in-match-and-do/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`await-in-test`](../../../../../conformance/cases/EFFECT/V2-EFFECT-await-in-test/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`callback-row-exact`](../../../../../conformance/cases/EFFECT/V2-EFFECT-callback-row-exact/case.toml) | Declared `Log` used at listed named bodies | `note`, `emit`, `run` → `Log` | Match on observed rows |
| [`callback-row-wrapped`](../../../../../conformance/cases/EFFECT/V2-EFFECT-callback-row-wrapped/case.toml) | Declared `Log` used at listed named bodies | `note`, `emit`, `run` → `Log` | Match on observed rows |
| [`ceiling-exact`](../../../../../conformance/cases/EFFECT/V2-EFFECT-ceiling-exact/case.toml) | Declared `Log` used at listed named bodies | `note` → `Log` | Match on observed rows |
| [`ceiling-transitive`](../../../../../conformance/cases/EFFECT/V2-EFFECT-ceiling-transitive/case.toml) | Declared `Log` used at listed named bodies | `note`, `outer` → `Log` | Match on observed rows |
| [`default-proc-member`](../../../../../conformance/cases/EFFECT/V2-EFFECT-default-proc-member/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`fn-member-pure`](../../../../../conformance/cases/EFFECT/V2-EFFECT-fn-member-pure/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`generic-operation-interface`](../../../../../conformance/cases/EFFECT/V2-EFFECT-generic-operation-interface/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`impl-in-interface`](../../../../../conformance/cases/EFFECT/V2-EFFECT-impl-in-interface/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`impl-keeps-row`](../../../../../conformance/cases/EFFECT/V2-EFFECT-impl-keeps-row/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`intent-type-result`](../../../../../conformance/cases/EFFECT/V2-EFFECT-intent-type-result/case.toml) | Declared `Log` used at listed named bodies | `note` → `Log` | Match on observed rows |
| [`member-additive-row`](../../../../../conformance/cases/EFFECT/V2-EFFECT-member-additive-row/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`member-plan-closed-receiver`](../../../../../conformance/cases/EFFECT/V2-EFFECT-member-plan-closed-receiver/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`member-plan-interface-value`](../../../../../conformance/cases/EFFECT/V2-EFFECT-member-plan-interface-value/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`member-plan-type-parameter`](../../../../../conformance/cases/EFFECT/V2-EFFECT-member-plan-type-parameter/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`named-proc-value`](../../../../../conformance/cases/EFFECT/V2-EFFECT-named-proc-value/case.toml) | Declared `Log` used at listed named bodies | `note`, `emit`, `appendTo`, `run` → `Log` | Match on observed rows |
| [`operation-interface`](../../../../../conformance/cases/EFFECT/V2-EFFECT-operation-interface/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`plan-discarded`](../../../../../conformance/cases/EFFECT/V2-EFFECT-plan-discarded/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`proc-await-direct`](../../../../../conformance/cases/EFFECT/V2-EFFECT-proc-await-direct/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`proc-await-twice`](../../../../../conformance/cases/EFFECT/V2-EFFECT-proc-await-twice/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`proc-generic`](../../../../../conformance/cases/EFFECT/V2-EFFECT-proc-generic/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`proc-labelled`](../../../../../conformance/cases/EFFECT/V2-EFFECT-proc-labelled/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`proc-member-of-type`](../../../../../conformance/cases/EFFECT/V2-EFFECT-proc-member-of-type/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`proc-plan-bound`](../../../../../conformance/cases/EFFECT/V2-EFFECT-proc-plan-bound/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`proc-recursive`](../../../../../conformance/cases/EFFECT/V2-EFFECT-proc-recursive/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |
| [`proc-type-empty-row`](../../../../../conformance/cases/EFFECT/V2-EFFECT-proc-type-empty-row/case.toml) | Declared `Log` used at listed named bodies | `note` → `Log` | Match on observed rows |
| [`proc-type-field`](../../../../../conformance/cases/EFFECT/V2-EFFECT-proc-type-field/case.toml) | Declared `Log` used at listed named bodies | `note`, `fire` → `Log` | Match on observed rows |
| [`proc-type-parameter`](../../../../../conformance/cases/EFFECT/V2-EFFECT-proc-type-parameter/case.toml) | Declared `Log` used at listed named bodies | `note`, `emit` → `Log` | Match on observed rows |
| [`proc-type-result`](../../../../../conformance/cases/EFFECT/V2-EFFECT-proc-type-result/case.toml) | Declared `Log` used at listed named bodies | `note` → `Log` | Match on observed rows |
| [`recursion-closes`](../../../../../conformance/cases/EFFECT/V2-EFFECT-recursion-closes/case.toml) | Declared `Log` used at listed named bodies | `note`, `loop` → `Log` | Match on observed rows |
| [`row-on-proc`](../../../../../conformance/cases/EFFECT/V2-EFFECT-row-on-proc/case.toml) | `Log` unused in `p` | Empty | Match on observed rows |
| [`row-on-test`](../../../../../conformance/cases/EFFECT/V2-EFFECT-row-on-test/case.toml) | `Log` unused in test | Empty | Match on observed rows |
| [`row-two-roots`](../../../../../conformance/cases/EFFECT/V2-EFFECT-row-two-roots/case.toml) | `Log` and `Clock` unused in `p` | Empty | Match on observed rows |
| [`test-row-empty-written`](../../../../../conformance/cases/EFFECT/V2-EFFECT-test-row-empty-written/case.toml) | No nonempty root from a local await | Empty | Match on observed rows |

For the rejection direction the source checker reports Log and Tick for
`twoRoots`. Replacing the empty-ceiling procedure's body with its twin's
lowered body yields `EffectFault::OutsideCeiling` for Log. The rejection
agrees and the reports differ in cardinality (F5). The prescribed IR
tests and body transplants test rejection independently of source
checking. F7 records the remaining coverage limitation.

## Findings

### F1. Defect — a witness cycle drops every related span

[`analysis.rs:458`](../../../../../crates/vibra-types/src/check/effects/analysis.rs#L458) revisits a cyclic
procedure, skips it, then has neither `reached` nor `dead_end`, so the
diagnostic at `await loop()` has an empty chain. The smallest probe is
`cycleNotes`.

[Chapter 03](../../../../spec/03-effects.md#effect-ceilings) requires
“one `@effect.outsideCeiling` at the earliest `await` that contributes it,
with the chain of calls as related spans.” Step 09's ruling starts the
chain at the reporting await's operand. Retain the prefix up to the first
repeated hop when no terminal call is reachable.

### F2. Defect — a closed interface member loses the indirect marker

[`analysis.rs:190`](../../../../../crates/vibra-types/src/check/effects/analysis.rs#L190) makes the marker
depend on an interface-valued or open receiver. In `closedNotes`, the note
is `performs Log via Log.emit`. [Chapter 07](../../../../spec/07-diagnostics-and-conformance.md#contract-effect-and-authority-diagnostics)
states “a hop through a callback or interface member is marked indirect.”
Step 09 explicitly calls out open and `Self` cases and chapter 07 also
covers this closed call. Mark the interface member hop indirect.

### F3. Defect — a value-ending fallback gets two errors at one span

[`expr.rs:4771`](../../../../../crates/vibra-types/src/check/expr.rs#L4771) checks the unused
statement and the fallback check independently. Both issue
`@type.mismatch` at `1i32` in `fallback`.
[Chapter 01](../../../../spec/01-source-language.md#bodies-and-statements)
states “An expression statement has type `Void` or `Never`” and requires
every path of the fallback to leave. The step 10 ruling asks for one
diagnostic for this single-statement case. Suppress the second diagnostic
at a fallback statement that already has the non-`Void` error.

### F4. Question — the guide's module-value constant premise

`constPlan` is a pure module-value initializer and the source checker
lowers it to a global ([`lower.rs:1510`](../../../../../crates/vibra-types/src/check/lower.rs#L1510)).
[Chapter 06](../../../../spec/06-runtime.md#what-has-no-encoding) says
“A type statically known to contain one is rejected wherever an encoding
is required.” [Chapter 01](../../../../spec/01-source-language.md#module-values)
permits a runtime initializer, and the program asks for no encoding.
`constDefault` provides the control and gets `@type.notConstant`. The
question for the guide is whether this test should name a constant-required
site, such as a labelled default.

### F5. Accepted limit — checked IR returns the first missing root

[`effects.rs:161`](../../../../../crates/vibra-ir/src/effects.rs#L161) short-circuits.
`twoRoots` and its `twoRootsIR` transplant show both violations in
source and only Log at the IR boundary. [Chapter 03](../../../../spec/03-effects.md#effect-ceilings)
requires a report “for each interface outside the ceiling” from the
checker. The validator is a separate fail-fast boundary. A normal source
user still sees both violations.

### F6. Accepted limit — host operations are outside slice 1 coverage

The `library` probe reaches the pure embedded iterator; operation roots
that bind host members have no source-level embedded example at this gate.
[`effects.rs:243`](../../../../../crates/vibra-ir/src/effects.rs#L243) checks hand-built host
owners and signatures. The hand-built tests cover acceptance, wrong owner,
wrong signature, an additive row, and a missing abstract member. Source
checks for the exact host diagnostics (including
`@external.unknownSymbol`), sealed receiver dispatch, narrowing and
before-await host dispatch and charge still need integration evidence.
[Chapter 03](../../../../spec/03-effects.md#operation-interfaces) requires
that a host binding's “signature and owner MUST match its registry row.”
The smallest executed library surrogate is `library`.

### F7. Accepted limit — uncovered comparison

[`analysis.rs:228`](../../../../../crates/vibra-types/src/check/effects/analysis.rs#L228) exposes the checker
row indirectly through diagnostics, and the checked sets expose
`Facts::awaited`. The [guide](../steps/11-review-rows.md#ordered-tasks)
asks to “Compare the checker's performed row with the IR validator's on
every accepted corpus case of prefix `EFFECT`.” The public comparison
above cannot observe exact checker roots of exempt bodies, and the
hand-built suite tests the listed transplants and IR negatives rather
than one malformed IR per negative source case in steps 05–10.
`selfReceiver` is a small illustration of an implicit owner root
without an unused-root observation.

## Scratch sources

Each probe below is a standalone module unless shown with a shared prefix.
Prepend the named prefix verbatim with a newline. The two test sources go
in `tests/run.vib` using the `await-in-test` fixture's project shape.

**Guard prefix**

```vib
fn need takes {n: I32} returns r: I32 requires Ordered.greater(n 0i32) do return n endfn
proc one returns r: I32 ensures Ordered.greater(r 0i32) do return 1i32 endproc
```

**Row prefix**

```vib
interface Log proc emit takes {m: Self} returns d: Void endproc endinterface
interface Tick proc tick takes {m: Self} returns d: Void endproc endinterface
proc loud takes {m: Log} returns d: Void performs {Log} do return await Log.emit(m) endproc
proc quiet takes {m: Log} returns d: Void do return void endproc
```

### `direct` with guard prefix

```vib
proc run returns r: I32 do return need(await one()) endproc
```

### `bareDirect` with guard prefix

```vib
proc echo takes {n: I32} returns r: I32 requires Ordered.greater(n 0i32) ensures Ordered.greater(r 0i32) do return n endproc
proc run returns r: I32 do return need(await echo(1i32)) endproc
```

### `bareRequires` with guard prefix

```vib
proc echo takes {n: I32} returns r: I32 requires Ordered.greater(n 0i32) ensures Ordered.greater(r 0i32) do return n endproc
fn build returns p: Intent(I32) do return echo(0i32) endfn
```

### `bound` with guard prefix

```vib
proc run returns r: I32 do let p be one() endlet return need(await p) endproc
```

### `twice` with guard prefix

```vib
proc run returns r: I32 do let p be one() endlet let a be await p endlet if Ordered.greater(a 0i32) then return need(await p) endif return 0i32 endproc
```

### `returned` with guard prefix

```vib
fn build returns p: Intent(I32) do return one() endfn
proc run returns r: I32 do return need(await build()) endproc
```

### `field` with guard prefix

```vib
proc run takes {box: {p: Intent(I32)}} returns r: I32 do return need(await box.p) endproc
```

### `option` with guard prefix

```vib
proc run takes {p: Option(Intent(I32))} returns r: Option(I32) do return Option.some(value: need(await try p)) endproc
```

### `callback` with guard prefix

```vib
proc run takes {f: Proc({} I32)} returns r: I32 do return need(await f()) endproc
```

### `namedValue` with guard prefix

```vib
proc run returns r: I32 do let f be one endlet return need(await f()) endproc
```

### `anonymous` with guard prefix

```vib
proc run returns r: I32 do let f be proc returns r: I32 ensures Ordered.greater(r 0i32) do return 1i32 endproc endlet return need(await f()) endproc
```

### `closed` with guard prefix

```vib
interface Positive proc get takes {x: Self} returns r: I32 ensures Ordered.greater(r 0i32) endproc endinterface
type Box of I32 impl Positive proc get takes {x: Self} returns r: I32 do return 1i32 endproc endimpl endtype
proc run takes {x: Box} returns r: I32 performs {Positive} do return need(await Positive.get(x)) endproc
```

### `open` with guard prefix

```vib
interface Positive proc get takes {x: Self} returns r: I32 ensures Ordered.greater(r 0i32) endproc endinterface
proc run(T) takes {x: T} returns r: I32 performs {Positive} given {T: Positive} do return need(await Positive.get(x)) endproc
```

### `requiresElse` with guard prefix

```vib
proc half takes {n: I32} returns r: I32 requires Ordered.greater(n 0i32) else 0i32 ensures Ordered.greater(r 0i32) do return n endproc
proc run returns r: I32 do return need(await half(0i32)) endproc
```

### `shadow` with guard prefix

```vib
proc echo takes {n: I32} returns r: I32 ensures Equatable.equal(r n) do return n endproc
proc run returns r: I32 do let n be 0i32 endlet let p be echo(n) endlet let n be 1i32 endlet return need(await p) endproc
```

### `rebind` with guard prefix

```vib
proc echo takes {n: I32} returns r: I32 requires Ordered.greater(n 0i32) ensures Ordered.greater(r 0i32) do return n endproc
proc run returns r: I32 do let n be 1i32 endlet let p be echo(n) endlet let other be 0i32 endlet return need(await p) endproc
```

### `recursive` with guard prefix

```vib
proc loop takes {b: Bool} returns r: I32 ensures Ordered.greater(r 0i32) do if b then return await loop(false) endif return 1i32 endproc
```

### `mutual` with guard prefix

```vib
proc a takes {b: Bool} returns r: I32 ensures Ordered.greater(r 0i32) do if b then let f be other endlet return await f(false) endif return 1i32 endproc
proc other takes {b: Bool} returns r: I32 ensures Ordered.greater(r 0i32) do if b then return await a(false) endif return 1i32 endproc
```

### `lastFn`

```vib
fn run returns r: Void do 1i32 endfn
```

### `lastProc`

```vib
proc run returns r: Void do 1i32 endproc
```

### `lastTest`

```vib
use @std.assert enduse
test "last" do 1i32 endtest
```

### `fallback`

```vib
fn run takes {p: Option(I32)} returns r: I32 do let Option.some(value: n) be p else 1i32 endlet return n endfn
```

### `branch`

```vib
fn run takes {b: Bool} returns r: I32 do return if b then 1i32 else 2i32 endif endfn
```

### `matchBranch`

```vib
fn run takes {p: Option(I32)} returns r: I32 do return match p case Option.some(value: n) n case Option.none() 0i32 endmatch endfn
```

### `alias` with row prefix

```vib
type Loud of Proc({Log} Void {Log}) endtype
fn accept takes {f: Proc({Log} Void)} returns d: Void do return void endfn
fn run takes {f: Loud} returns d: Void do return accept(f) endfn
```

### `generic` with row prefix

```vib
proc run(T) takes {f: T} returns d: Void do return await f() endproc
```

### `aliasGeneric` with row prefix

```vib
type Callback of Proc({Log} Void {Log}) endtype
fn same(T) takes {x: T y: T} returns d: Void do return void endfn
fn run takes {wrapped: Callback} returns d: Void do let Callback(f) be wrapped endlet return same(quiet f) endfn
```

### `genericInferred` with row prefix

```vib
fn both(T) takes {x: T y: T} returns d: Void do return void endfn
fn run returns d: Void do return both(loud quiet) endfn
```

### `argument` with row prefix

```vib
fn accept takes {f: Proc({Log} Void)} returns d: Void do return void endfn
fn run returns d: Void do return accept(loud) endfn
```

### `narrower` with row prefix

```vib
fn accept takes {f: Proc({Log} Void {Log})} returns d: Void do return void endfn
fn run returns d: Void do return accept(quiet) endfn
```

### `annotation` with row prefix

```vib
fn run returns d: Void do let f: Proc({Log} Void) be loud endlet return void endfn
```

### `returnRow` with row prefix

```vib
fn run returns f: Proc({Log} Void) do return loud endfn
```

### `fieldRow` with row prefix

```vib
fn run returns d: Void do let box: {f: Proc({Log} Void)} be {f: loud} endlet return void endfn
```

### `listRow` with row prefix

```vib
fn run returns d: Void do let f: List(Proc({Log} Void)) be [loud] endlet return void endfn
```

### `optionRow` with row prefix

```vib
fn run returns d: Void do let f: Option(Proc({Log} Void)) be Option.some(value: loud) endlet return void endfn
```

### `dictRow` with row prefix

```vib
fn run takes {x: Dict(Str Proc({Log} Void {Log}))} returns y: Dict(Str Proc({Log} Void)) do return x endfn
```

### `defaultValue` with row prefix

```vib
fn run labelled {f: Proc({Log} Void) be loud} returns d: Void do return void endfn
```

### `capture` with row prefix

```vib
fn run returns outer: Fn({} Proc({Log} Void)) do let f be loud endlet return fn returns inner: Proc({Log} Void) do return f endfn endfn
```

### `reordered` with row prefix

```vib
fn run takes {f: Proc({Log} Void {Tick Log})} returns g: Proc({Log} Void {Log Tick}) do return f endfn
```

### `duplicate` with row prefix

```vib
fn run takes {f: Proc({Log} Void {Log Log})} returns g: Void do return void endfn
```

### `fnForProc` with row prefix

```vib
fn pure takes {m: Log} returns d: Void do return void endfn
fn run returns f: Proc({Log} Void) do return pure endfn
```

### `procForFn` with row prefix

```vib
fn run returns f: Fn({Log} Void) do return quiet endfn
```

### `optionCeiling` with row prefix

```vib
proc run takes {p: Option(Proc({Log} Void {Log})) m: Log} returns r: Option(Void) do let f be try p endlet return Option.some(value: await f(m)) endproc
```

### `listCeiling` with row prefix

```vib
proc run takes {p: List(Proc({Log} Void {Log})) m: Log} returns r: Void do let [f] be p else return void endlet return await f(m) endproc
```

### `assoc` with row prefix

```vib
interface Supply type Item endtype fn get takes {s: Self} returns f: Item endfn endinterface
proc run takes {s: Supply(Item: Proc({Log} Void {Log})) m: Log} returns d: Void do let f be Supply.get(s) endlet return await f(m) endproc
```

### `stored` with row prefix

```vib
proc run takes {m: Log} returns r: Void do let p be loud(m) endlet return await p endproc
```

### `callbackCeiling` with row prefix

```vib
proc run takes {sink: Proc({Log} Void {Log}) m: Log} returns d: Void do return await sink(m) endproc
```

### `defaultFn` with row prefix

```vib
interface Builder fn make takes {x: Self m: Log} returns p: Intent(Void {Log}) do return loud(m) endfn endinterface
proc run takes {x: Builder m: Log} returns d: Void do return await Builder.make(x m) endproc
```

### `defaultProc` with row prefix

```vib
interface Sink proc send takes {s: Self m: Log} returns d: Void do return await loud(m) endproc endinterface
```

### `nested` with row prefix

```vib
proc run takes {m: Log} returns d: Void do let f be fn returns d: Void do let g be proc returns d: Void do return await loud(m) endproc endlet return void endfn endlet return void endproc
```

### `selfReceiver` with row prefix

```vib
interface Sink proc send takes {s: Self m: Log} returns d: Void endproc proc call takes {s: Self m: Log} returns d: Void do return await Sink.send(s m) endproc endinterface
```

### `selfMissing` with row prefix

```vib
interface Sink proc call takes {s: Self} returns d: Void do return await Log.emit(s) endproc endinterface
```

### `selfProper`

```vib
interface Clock proc tick takes {m: Self} returns d: Void endproc endinterface
interface Log proc emit takes {m: Self} returns d: Void performs {Clock} endproc endinterface
proc run(T) takes {m: T} returns d: Void performs {Log} given {T: Log} do return await Log.emit(m) endproc
```

### `library`

```vib
use @std.iter enduse
proc run takes {xs: Iter(Item: I32)} returns d: Void do let _ be await Iter.next(xs) endlet return void endproc
```

### `closedNotes` with row prefix

```vib
type Model of I32 impl Log proc emit takes {m: Self} returns d: Void do return void endproc endimpl endtype
proc run takes {m: Model} returns d: Void do return await Log.emit(m) endproc
```

### `cycleNotes`

```vib
interface Log proc emit takes {m: Self} returns d: Void endproc endinterface
proc loop returns d: Void performs {Log} do return await loop() endproc
proc run returns d: Void do return await loop() endproc
```

### `twoRoots` with row prefix

```vib
proc both takes {m: Log t: Tick} returns d: Void performs {Log Tick} do let _ be await Log.emit(m) endlet return await Tick.tick(t) endproc
proc run takes {m: Log t: Tick} returns d: Void do return await both(m t) endproc
```

### `twoRootsIR` with row prefix

```vib
proc both takes {m: Log t: Tick} returns d: Void performs {Log Tick} do let _ be await Log.emit(m) endlet return await Tick.tick(t) endproc
proc run takes {m: Log t: Tick} returns d: Void do return void endproc
proc twin takes {m: Log t: Tick} returns d: Void performs {Log Tick} do return await both(m t) endproc
```

### `guardAwait` with guard prefix

```vib
proc run returns r: I32 ensures Ordered.greater(r 0i32) else await one() do return 0i32 endproc
```

### `constAwait`

```vib
proc one returns r: I32 do return 1i32 endproc
fn build returns p: Intent(I32) do return one() endfn
let n: I32 be await build() endlet
```

### `constPlan`

```vib
proc one returns r: I32 do return 1i32 endproc
let p: Intent(I32) be one() endlet
```

### `constDefault`

```vib
proc one returns r: I32 do return 1i32 endproc
fn run labelled {p: Intent(I32) be one()} returns r: I32 do return 0i32 endfn
```

### `twoClosures`

```vib
use @std.assert enduse
proc one returns r: I32 do return 1i32 endproc
test "two closures" do
  let p be one() endlet
  let f be proc returns r: I32 do return await p endproc endlet
  let g be proc returns r: I32 do return await p endproc endlet
  assert.equal(2i32 I32.add(await f() await g()))
endtest
```

### `anonWarning` with row prefix

```vib
proc outer returns d: Void performs {Log} do let f be proc returns d: Void do let x: I32 be "x" endlet return void endproc endlet return void endproc
```

### `deepRows`

The full source is generated in scratch by this literal expansion.

```powershell
$roots = 0..999 | ForEach-Object {
  "interface E$_ proc p takes {m: Self} returns d: Void endproc endinterface"
}
$row = (0..999 | ForEach-Object { "E$_" }) -join ' '
($roots -join "`n") + "`nproc wide returns d: Void performs {$row} do return void endproc"
```

For `twoRootsIR`, copy the lowered `twin` body into `run` in
`ModuleSetParts.functions` and call `CheckedModuleSet::try_new` with the
unchanged `run` signature and context. Both procedures have the same two
input slots. The unchanged parts also revalidate.

### `argAwait` with row prefix

```vib
proc make takes {x: Void} returns d: Void do return void endproc
proc run takes {m: Log} returns p: Intent(Void) do return make(await Log.emit(m)) endproc
```

### `selfAdditive`

```vib
interface Clock proc tick takes {m: Self} returns d: Void endproc endinterface
interface Log
  proc emit takes {m: Self} returns d: Void performs {Clock} endproc
  proc call takes {m: Self} returns d: Void do return await Log.emit(m) endproc
endinterface
```

### `openAssoc` with row prefix

```vib
interface Supply type Item endtype fn get takes {s: Self} returns f: Item endfn endinterface
proc run takes {s: Supply m: Log} returns d: Void do let f be Supply.get(s) endlet return await f(m) endproc
```
