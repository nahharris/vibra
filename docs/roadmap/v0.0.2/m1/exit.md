# M1 exit record

The evidence that M1 meets the exit lines of its
[README](README.md#exit-evidence). Each line names the command that shows it and
that command's result. The commands ran on Windows 11 in the worktree of
`m1/29-m1-exit`, at the commit that merges `v0.0.2/m1-lang` at `d17acaca`, with
`--locked --offline`, `CARGO_BUILD_JOBS=4`, and `RUSTFLAGS=-D warnings` for
clippy and the tests.

## Exit lines

| Exit line | Status | Evidence |
| --- | --- | --- |
| positive, negative, recovery, and format cases for every form of the slices, in both suites | met | [Coverage audit](#coverage-audit), [Both suites](#both-suites) |
| contract cases: discharged, refuted, and unproved obligations with their diagnostics; guard consequence values; a handled constant construction error; no run-time check emitted for a bare guard | met | [Contract evidence](#contract-evidence) |
| deep tail and non-tail recursion; invariant checks on every ingress path | met | [Recursion and ingress](#recursion-and-ingress) |
| a list of every gate-program construction the contract rules rejected, with the repair used | met; the list is empty | [Gate program and pilot](#gate-program-and-pilot) |
| the reference solutions and controls of the pilot's pure tasks pass | met | [Gate program and pilot](#gate-program-and-pilot) |
| no step of M1 has an open specification question | met | [Specification questions](#specification-questions) |
| the validation commands of step 29 exit 0 | met | [Both suites](#both-suites) |

## Both suites

`cargo run -p vibra-conformance --bin vibra-conformance -- --root conformance/cases`
exits 0:

| Profile | Passed | Failed | Unknown | Unavailable |
| --- | --- | --- | --- | --- |
| `readerV2` | 178 | 0 | 0 | 0 |
| `staticV2` | 456 | 0 | 0 | 0 |
| `interpreterV2` | 217 | 0 | 0 | 0 |
| `toolingV2` | 9 | 0 | 0 | 0 |
| total | 860 | 0 | 0 | 0 |

`cargo test --workspace --all-targets --all-features --no-fail-fast`, run alone at
`007b5755`, exits 0. It prints 53 `test result:` lines, all `ok`, with 1815
tests passed, 0 failed, and 0 ignored.

The other commands:

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | exit 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit 0, no warning |
| `cargo doc --workspace --no-deps --all-features` with `RUSTDOCFLAGS=-D warnings` | exit 0 |
| `cargo run --release -p vibra-conformance --bin fuzz -- --target all` | exit 0; `fuzz: 4 targets, 320000 iterations, deep limit 20000, 0 findings` |
| `cargo run -p vibra-cli --bin vibra -- --workspace examples/gate/gate-pure test` | exit 0; `passed 14, failed 0, selected 14` |
| `cargo run -p vibra-cli --bin vibra -- --workspace examples/pilot-pure test` | exit 0; `passed 26, failed 0, selected 26` |

## Coverage audit

`cargo test -p vibra-conformance --test it -- owner_coverage --nocapture`
passes. The host test `owner_coverage` reads the [owner table](../m0/owners.md),
the two EBNF blocks of the [source
chapter](../../../spec/01-source-language.md), the trigger tables of the
[diagnostics chapter](../../../spec/07-diagnostics-and-conformance.md),
`conformance/rules.tsv`, and the corpus. It fails on a missing case id, on an M1
form without an accepted case, a rejected case where it has an M1 code or is a
production, or formatter evidence where it is a production, on a guard rule or
integer fact without an accepted and a rejected case, and on an M1 code that no
case expects. It has no exemption list. Its output:

```text
owner rows of M1 checked against the corpus: 162
host tests named by owner rows, each found: 9
chapter 01 productions, each with an owner row: 70
GUARD-1: accepted V2-CONTRACT-evaluation-reads-passed V2-CONTRACT-evaluation-reads-value V2-CONTRACT-member-requires-constant V2-CONTRACT-rule1-constant-true V2-CONTRACT-where-bare-construction V2-CONTRACT-where-generic-bare; rejected V2-CONTRACT-evaluation-reads-refuted V2-CONTRACT-ingress-constant V2-CONTRACT-member-requires-refuted V2-CONTRACT-rule1-constant-refuted V2-CONTRACT-where-generic-refuted
GUARD-2: accepted V2-CONTRACT-rule2-invariant; rejected V2-CONTRACT-invariant-guarded-predicate V2-CONTRACT-rule2-wrong-invariant V2-CONTRACT-where-bare-record-unproved
GUARD-3: accepted V2-CONTRACT-member-requires-branch V2-CONTRACT-member-requires-open-branch V2-CONTRACT-read-arm-selected V2-CONTRACT-rule3-if-true-branch V2-CONTRACT-rule3-match-fact V2-CONTRACT-rule3-negation V2-CONTRACT-where-bare-branch; rejected V2-CONTRACT-read-arm-list-literal V2-CONTRACT-read-arm-possible V2-CONTRACT-read-arm-tuple-literal V2-CONTRACT-rule3-if-false-branch V2-CONTRACT-rule3-outside-branch
GUARD-4: accepted V2-CONTRACT-label-default-same V2-CONTRACT-label-order-matching V2-CONTRACT-rule4-alias V2-CONTRACT-rule4-substitution; rejected V2-CONTRACT-label-default-different V2-CONTRACT-label-order-unproved V2-CONTRACT-rule4-text-not-enough
GUARD-5: accepted V2-CONTRACT-rule5-and-introduction V2-CONTRACT-rule5-and-projection; rejected V2-CONTRACT-rule5-or-insufficient
GUARD-6: accepted V2-CONTRACT-in-range-relation V2-CONTRACT-rule3-negation; rejected V2-CONTRACT-min-no-fact V2-CONTRACT-no-arithmetic-fact V2-CONTRACT-no-float-fact V2-CONTRACT-no-user-ordered-fact
GUARD-7: accepted V2-CONTRACT-label-order-result V2-CONTRACT-rule7-certified-result; rejected V2-CONTRACT-label-order-result-unproved V2-CONTRACT-rule7-requires-not-discharged
INT-1: accepted V2-CONTRACT-int1; rejected V2-CONTRACT-int1-nearest-failing
INT-2: accepted V2-CONTRACT-int2; rejected V2-CONTRACT-int2-nearest-failing
INT-3: accepted V2-CONTRACT-int3; rejected V2-CONTRACT-int3-nearest-failing
INT-4: accepted V2-CONTRACT-int4; rejected V2-CONTRACT-int4-nearest-failing
INT-5: accepted V2-CONTRACT-int5; rejected V2-CONTRACT-int5-nearest-failing
INT-6: accepted V2-CONTRACT-int6; rejected V2-CONTRACT-int6-nearest-failing
INT-7: accepted V2-CONTRACT-int7 V2-CONTRACT-proof-budget-chain; rejected V2-CONTRACT-int7-nearest-failing
INT-8: accepted V2-CONTRACT-int8; rejected V2-CONTRACT-int8-nearest-failing
registry codes M1 owns: 95
chapter 07 rows whose codes M1 owns: 111
corpus cases: 860
owned by the host tests their owner row names: 39
  Grammar productions / `role`: the accepted side
  Grammar productions / `role`: the formatter side
  Grammar productions / `external`: `V2-SRC-DECL-fn-external-with-body`
  Grammar productions / `external`: `V2-SRC-DECL-fn-external-bad-tag`
  Grammar productions / `external`: the accepted side
  Grammar productions / `external`: the formatter side
  Grammar productions / `symbol`: the accepted side
  Grammar productions / `symbol`: the formatter side
  Grammar productions / `native`: the accepted side
  Grammar productions / `native`: the formatter side
  Type system rule families / Language core and standard library; the prelude: the accepted side
  Programs, packages, and data / The standard library (embedding, manifest, roles): every side
  Runtime and tooling forms / Compiler intrinsic registry; integer, float, character, sequence, and dictionary operations: the rejected side
  Runtime and tooling forms / Diagnostic documents, ordering, and the registry: every side
  Runtime and tooling forms / Conformance manifest, rules, and profiles: every side
  Runtime and tooling forms / Command line: grammar, envelope, `help`: every side
  the code @external.ownerMismatch
  the code @external.signatureMismatch
  the code @external.unknownSymbol
  the code @module.caseCollision
  the code @module.ioError
  the code @module.pathEscape
  the code @module.sourceIdCollision
  the code @project.ioError
  the code @project.notFound
  the code @runtime.invalidCheckedProgram
  chapter 07, Availability, discovery and resolution: no `project.vibon` at or above the workspace: the rejected side
  chapter 07, Availability, discovery and resolution: other project input or output failure: the rejected side
  chapter 07, Availability, discovery and resolution: module enumeration or read failure, unreadable or dangling entry: the rejected side
  chapter 07, Availability, discovery and resolution: two source paths that differ only by case: the rejected side
  chapter 07, Availability, discovery and resolution: a link that leaves the source root: the rejected side
  chapter 07, Availability, discovery and resolution: one source identity for two modules, including dependency inputs: the rejected side
  chapter 07, Contract, effect and authority diagnostics: an external symbol not in the registry: the rejected side
  chapter 07, Contract, effect and authority diagnostics: an external symbol not in the registry: the accepted side
  chapter 07, Contract, effect and authority diagnostics: an external member on the wrong interface: the rejected side
  chapter 07, Contract, effect and authority diagnostics: an external member on the wrong interface: the accepted side
  chapter 07, Contract, effect and authority diagnostics: an external member whose signature or row differs from the registry: the rejected side
  chapter 07, Contract, effect and authority diagnostics: an external member whose signature or row differs from the registry: the accepted side
  chapter 07, Runtime and tool failures: impossible typed IR, descriptor, or continuation; an unknown engine trap; a defect of the embedded standard library found at load: the rejected side
named by a step guide of M2: 21
  Grammar productions / `proc-decl`: `V2-EFFECT-proc-*`
  Grammar productions / `proc-decl`: `V2-RUNTIME-plan-*`
  Grammar productions / `test`: `V2-EFFECT-await-in-test`
  Grammar productions / `test`: `V2-EFFECT-row-on-test`
  Grammar productions / `test`: `V2-PROJECT-test-takes-*`
  Grammar productions / `test`: `V2-GRANT-test-without-world`
  Grammar productions / `performs`: `V2-EFFECT-row-*`
  Grammar productions / `performs`: `V2-EFFECT-ceiling-*`
  Grammar productions / `performs`: `V2-EFFECT-outside-ceiling-*`
  Grammar productions / `performs`: `V2-EFFECT-anon-fn-performs`
  Grammar productions / `row`: `V2-EFFECT-row-two-roots`
  Grammar productions / `row`: `V2-EFFECT-row-duplicate`
  Grammar productions / `row`: `V2-EFFECT-row-duplicate-alias`
  Grammar productions / `row`: `V2-EFFECT-proc-type-row-duplicate`
  Grammar productions / `row`: `V2-EFFECT-proc-type-row-invalid`
  Grammar productions / `row`: the rejected side
  Grammar productions / `unary`: `V2-EFFECT-proc-await-direct`
  Grammar productions / `anon-proc`: `V2-EFFECT-anon-proc-*`
  Grammar productions / `anon-proc`: `V2-RUNTIME-anon-proc-captures`
  Grammar productions / `type-expr`: `V2-EFFECT-proc-type-*`
  Grammar productions / `type-expr`: `V2-EFFECT-intent-type-result`
```

The sides listed as owned by host tests belong to rows of the owner table whose
cases cell cites host tests. Nine tests are named there by `module::function`,
and the audit finds each one. The other rows cite the host tests of a step (02,
09, 13, 14, or 24) without a name, and the audit takes the citation as it is
written: those sides are shown by the host suite passing, not by a named test.
The ids named only by a step guide of M2 belong to `proc`, effects, and grants,
which M1 excludes.

Recovery has no column in the owner table, so the audit counts a form's rejected
case as its recovery case. A rejected reader case expects its exact list of
diagnostics, which a recovery that cascades would break; 6 of the 87 rejected
reader cases also expect the recovered tree. These cases show recovery followed
by a valid form:
[`V2-SRC-READER-recovery-then-valid`](../../../../conformance/cases/SRC-READER/V2-SRC-READER-recovery-then-valid/case.toml),
[`V2-SRC-READER-fn-recovery`](../../../../conformance/cases/SRC-READER/V2-SRC-READER-fn-recovery/case.toml),
[`V2-SRC-READER-type-recovery`](../../../../conformance/cases/SRC-READER/V2-SRC-READER-type-recovery/case.toml),
[`V2-SRC-READER-expr-recovery`](../../../../conformance/cases/SRC-READER/V2-SRC-READER-expr-recovery/case.toml),
[`V2-SRC-READER-closer-by-kind`](../../../../conformance/cases/SRC-READER/V2-SRC-READER-closer-by-kind/case.toml),
[`V2-SRC-READER-closer-wrong`](../../../../conformance/cases/SRC-READER/V2-SRC-READER-closer-wrong/case.toml),
[`V2-SRC-READER-closer-eof`](../../../../conformance/cases/SRC-READER/V2-SRC-READER-closer-eof/case.toml),
and
[`V2-TOOL-format-recovered`](../../../../conformance/cases/TOOL/V2-TOOL-format-recovered/case.toml).
The fuzz target `source` checks recovery on 100,000 generated inputs.

## Contract evidence

### One discharged obligation per rule

`cargo test -p vibra-conformance --test it -- every_discharge_rule --nocapture`
passes. The host test
`exit_evidence::every_discharge_rule_and_integer_fact_discharges_an_obligation`
checks every accepted contract case and prints, for each rule, the first case
with a passed obligation whose `rules` list that rule:

| Rule | Case |
| --- | --- |
| `GUARD-1` | [`V2-CONTRACT-anonymous-ensures-ok`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-anonymous-ensures-ok/case.toml) |
| `GUARD-2` | [`V2-CONTRACT-rule2-invariant`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-rule2-invariant/case.toml) |
| `GUARD-3` | [`V2-CONTRACT-anonymous-requires-ok`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-anonymous-requires-ok/case.toml) |
| `GUARD-4` | [`V2-CONTRACT-anonymous-ensures-ok`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-anonymous-ensures-ok/case.toml) |
| `GUARD-5` | [`V2-CONTRACT-int1`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-int1/case.toml) |
| `GUARD-6` | [`V2-CONTRACT-in-range-relation`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-in-range-relation/case.toml) |
| `GUARD-7` | [`V2-CONTRACT-anonymous-ensures-ok`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-anonymous-ensures-ok/case.toml) |
| `INT-1` | [`V2-CONTRACT-int1`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-int1/case.toml) |
| `INT-2` | [`V2-CONTRACT-int2`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-int2/case.toml) |
| `INT-3` | [`V2-CONTRACT-int3`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-int3/case.toml) |
| `INT-4` | [`V2-CONTRACT-int4`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-int4/case.toml) |
| `INT-5` | [`V2-CONTRACT-int5`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-int5/case.toml) |
| `INT-6` | [`V2-CONTRACT-int6`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-int6/case.toml) |
| `INT-7` | [`V2-CONTRACT-int7`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-int7/case.toml) |
| `INT-8` | [`V2-CONTRACT-int8`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-int8/case.toml) |

The audit output above lists, for each rule, every accepted and every rejected
case named for it.

### Refuted and unproved obligations

The corpus runner passes each of these `workspaceCheck` cases with its one
diagnostic. A refuted obligation is verdict `refuted`, and an unproved one is
verdict `unknown`.

| Diagnostic | Cases |
| --- | --- |
| `@contract.refuted` | [`V2-CONTRACT-rule1-constant-refuted`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-rule1-constant-refuted/case.toml), [`V2-CONTRACT-ensures-refuted`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-ensures-refuted/case.toml), [`V2-CONTRACT-evaluation-reads-refuted`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-evaluation-reads-refuted/case.toml), [`V2-CONTRACT-ingress-constant`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-ingress-constant/case.toml), [`V2-CONTRACT-where-generic-refuted`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-where-generic-refuted/case.toml), [`V2-CONTRACT-anonymous-requires-refuted`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-anonymous-requires-refuted/case.toml), [`V2-CONTRACT-anonymous-ensures-refuted`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-anonymous-ensures-refuted/case.toml), [`V2-CONTRACT-member-requires-refuted`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-member-requires-refuted/case.toml), [`V2-CONTRACT-member-requires-open-refuted`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-member-requires-open-refuted/case.toml), [`V2-CONTRACT-member-requires-open-label-refuted`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-member-requires-open-label-refuted/case.toml), [`V2-CONTRACT-member-requires-generic-impl-refuted`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-member-requires-generic-impl-refuted/case.toml), [`V2-CONTRACT-member-guard-obligation-refuted`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-member-guard-obligation-refuted/case.toml) |
| `@contract.unproved` | [`V2-CONTRACT-bare-requires-unproved`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-bare-requires-unproved/case.toml), [`V2-CONTRACT-where-bare-unproved`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-where-bare-unproved/case.toml), [`V2-CONTRACT-where-bare-record-unproved`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-where-bare-record-unproved/case.toml), [`V2-CONTRACT-where-generic-unproved`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-where-generic-unproved/case.toml), [`V2-CONTRACT-ingress-pattern-rebuild`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-ingress-pattern-rebuild/case.toml), [`V2-CONTRACT-ensures-every-return`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-ensures-every-return/case.toml), [`V2-CONTRACT-ensures-bare-through-try`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-ensures-bare-through-try/case.toml), [`V2-CONTRACT-rule2-wrong-invariant`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-rule2-wrong-invariant/case.toml), [`V2-CONTRACT-rule3-outside-branch`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-rule3-outside-branch/case.toml), [`V2-CONTRACT-rule3-if-false-branch`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-rule3-if-false-branch/case.toml), [`V2-CONTRACT-rule4-text-not-enough`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-rule4-text-not-enough/case.toml), [`V2-CONTRACT-rule5-or-insufficient`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-rule5-or-insufficient/case.toml), [`V2-CONTRACT-rule7-requires-not-discharged`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-rule7-requires-not-discharged/case.toml), [`V2-CONTRACT-min-no-fact`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-min-no-fact/case.toml), [`V2-CONTRACT-no-arithmetic-fact`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-no-arithmetic-fact/case.toml), [`V2-CONTRACT-no-user-ordered-fact`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-no-user-ordered-fact/case.toml), [`V2-CONTRACT-int1-nearest-failing`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-int1-nearest-failing/case.toml) to [`V2-CONTRACT-int8-nearest-failing`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-int8-nearest-failing/case.toml), [`V2-CONTRACT-member-requires-unproved`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-member-requires-unproved/case.toml), [`V2-CONTRACT-member-requires-open-unproved`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-member-requires-open-unproved/case.toml), [`V2-CONTRACT-anonymous-requires-unproved`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-anonymous-requires-unproved/case.toml), [`V2-CONTRACT-unproved-notes`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-unproved-notes/case.toml) |
| `@contract.obligationLost` | [`V2-CONTRACT-obligation-lost`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-obligation-lost/case.toml), [`V2-CONTRACT-anonymous-obligation-lost`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-anonymous-obligation-lost/case.toml), [`V2-CONTRACT-anonymous-capture-lost`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-anonymous-capture-lost/case.toml), [`V2-CONTRACT-where-named-requires-lost`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-where-named-requires-lost/case.toml), [`V2-CONTRACT-where-anonymous-requires-lost`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-where-anonymous-requires-lost/case.toml) |
| `@tool.limitExceeded` | [`V2-CONTRACT-proof-budget-exceeded`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-proof-budget-exceeded/case.toml), [`V2-CONTRACT-constant-limit`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-constant-limit/case.toml), [`V2-CONTRACT-guard-else-limit`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-guard-else-limit/case.toml) |

### Guard consequence values

Each of these `interpret` cases runs to its expected value:
[`V2-CONTRACT-requires-else-true`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-requires-else-true/case.toml),
[`V2-CONTRACT-requires-else-false`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-requires-else-false/case.toml),
[`V2-CONTRACT-requires-else-skips-ensures`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-requires-else-skips-ensures/case.toml),
[`V2-CONTRACT-ensures-else`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-ensures-else/case.toml),
[`V2-CONTRACT-ensures-else-once`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-ensures-else-once/case.toml),
[`V2-CONTRACT-ensures-else-candidate-bound`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-ensures-else-candidate-bound/case.toml),
[`V2-CONTRACT-where-else`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-where-else/case.toml),
[`V2-CONTRACT-where-generic-else`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-where-generic-else/case.toml),
[`V2-CONTRACT-else-converts-freely`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-else-converts-freely/case.toml),
[`V2-CONTRACT-guard-else-folds`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-guard-else-folds/case.toml),
[`V2-CONTRACT-member-else-copied`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-member-else-copied/case.toml),
[`V2-CONTRACT-anonymous-requires-else`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-anonymous-requires-else/case.toml),
[`V2-CONTRACT-anonymous-ensures-else`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-anonymous-ensures-else/case.toml),
and
[`V2-CONTRACT-anonymous-else-converts`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-anonymous-else-converts/case.toml).
A consequence of the wrong type is `@contract.invalidConsequence` in
[`V2-CONTRACT-requires-else-wrong-type`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-requires-else-wrong-type/case.toml),
[`V2-CONTRACT-ensures-else-wrong-type`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-ensures-else-wrong-type/case.toml),
and
[`V2-CONTRACT-where-else-nonconstant`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-where-else-nonconstant/case.toml).

### The handled constant construction error

[`V2-CONTRACT-constant-construction-err-matched`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-constant-construction-err-matched/case.toml)
matches `Positive(0i32)`, which the checker evaluates to `Result.err`, and runs
to `42i32` through the `err` arm. Its counter-cases are
[`V2-CONTRACT-constant-construction-ok`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-constant-construction-ok/case.toml),
which runs to `5i32` through the `ok` value, and
[`V2-CONTRACT-constant-construction-err`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-constant-construction-err/case.toml),
where `let Result.ok(value: p) be Positive(0i32) endlet` is
`@pattern.refutableBinding`.

### No run-time check for a discharged bare guard

The host test `exit_evidence::a_discharged_bare_guard_leaves_no_guard_node`
passes. It checks the source of
[`V2-CONTRACT-no-runtime-check`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-no-runtime-check/case.toml),
whose function `pick` has a bare `requires` discharged at its one call. The
lowered `pick` has no `entry_guard` and no `exit_guard`. The same source with
`else 0i32` added to the `requires` lowers `pick` with an `entry_guard`. The
case itself runs to `5i32`.

## Recursion and ingress

### Recursion

| Case | Result | Host test in `vibra-conformance/tests/it/interpret.rs` |
| --- | --- | --- |
| [`V2-RUNTIME-tail-loop`](../../../../conformance/cases/RUNTIME/V2-RUNTIME-tail-loop/case.toml) | `100000i32` | `the_tail_loop_runs_at_depth_one`: depth 1, 100,001 tail transfers |
| [`V2-RUNTIME-tail-loop-long`](../../../../conformance/cases/RUNTIME/V2-RUNTIME-tail-loop-long/case.toml) | `10000000i32` | `the_long_tail_loop_runs_at_depth_one`: depth 1, 10,000,001 tail transfers |
| [`V2-RUNTIME-tail-through-value`](../../../../conformance/cases/RUNTIME/V2-RUNTIME-tail-through-value/case.toml) | `100000i32` | `the_tail_call_through_a_field_runs_at_depth_one`: depth 1 |
| [`V2-RUNTIME-tail-through-closure`](../../../../conformance/cases/RUNTIME/V2-RUNTIME-tail-through-closure/case.toml) | `100000i32` | `the_tail_call_through_a_closure_runs_at_depth_one`: depth 1 |
| [`V2-RUNTIME-non-tail-depth`](../../../../conformance/cases/RUNTIME/V2-RUNTIME-non-tail-depth/case.toml) | `100000i32` under the published memory budget | `the_non_tail_recursion_keeps_one_activation_per_call`: depth 100,001 |
| [`V2-RUNTIME-memory-exhausted`](../../../../conformance/cases/RUNTIME/V2-RUNTIME-memory-exhausted/case.toml) | the host event `@runtime.memoryExhausted`, not a trap | — |

### Ingress of an invariant-carrying type

| Path | Case | Outcome |
| --- | --- | --- |
| the constructor | [`V2-CONTRACT-ingress-constructor`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-ingress-constructor/case.toml) | `Positive(0i32)` with `where … else` is `Result.err(error: PositiveError.notPositive())` |
| a `TryFrom` result | [`V2-CONTRACT-ingress-conversion`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-ingress-conversion/case.toml) | the error for 0 and the value for 1 |
| a constant | [`V2-CONTRACT-ingress-constant`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-ingress-constant/case.toml) | `@contract.refuted` at the predicate `isPositive`, related to `Positive(0i32)` |
| an interface value | [`V2-CONTRACT-ingress-interface-value`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-ingress-interface-value/case.toml) | the same value; the host test `interpret::a_value_through_an_interface_value_is_not_checked_again` shows the run takes the same reductions as the source with no `where` |
| a rebuild after a pattern | [`V2-CONTRACT-ingress-pattern-rebuild`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-ingress-pattern-rebuild/case.toml) | `@contract.unproved` at `Positive(I32.sub(raw 1i32))`, related to the type's name |

The constant is refuted at its predicate, as the diagnostics chapter places
`@contract.refuted`, and not at the construction the step guide names.

## Gate program and pilot

[`examples/gate/gate-pure/`](../../../../examples/gate/gate-pure/) holds the pure modules
of the release gate program: the errors, path, line, and metric modules, with
tests. [gate-rejections.md](gate-rejections.md) states that the contract rules
rejected none of their constructions. The one bare guard is discharged by
`GUARD-3`.

[`examples/pilot-pure/`](../../../../examples/pilot-pure/) holds the reference
solutions and tests of the pilot's pure tasks T1 to T6, T9, and T10, and six
negative controls. The host tests in `vibra-cli/tests/it/examples.rs` pass:

| Host test | Shows |
| --- | --- |
| `gate_pure_tests` | `check` and `test` in `examples/gate/gate-pure` exit 0 with status `passed` |
| `gate_pure_format` | `fmt` on every file of both examples reports `changed: false` |
| `pilot_pure_tests` | `check` and `test` in `examples/pilot-pure` exit 0 with status `passed` |
| `pilot_controls_rejected` | each control, checked as the one module of a library, exits 1 with exactly its diagnostic |

| Control | Diagnostic |
| --- | --- |
| `positiveZero.vib` | `@contract.refuted` |
| `positiveUnchecked.vib` | `@contract.unproved` |
| `orderUnguarded.vib` | `@contract.unproved` |
| `orderSwapped.vib` | `@contract.unproved` |
| `orderConstantFalse.vib` | `@contract.refuted` |
| `optionalObligationLost.vib` | `@contract.obligationLost` |

The registered pilot draft gives no diagnostic for its controls, so each
diagnostic above is the one the type chapter's rules give.

## Specification questions

No step of M1 has an open specification question:

- The questions of steps 02 to 12 were settled by `m1/spec-batch-1`, and those
  of the later steps by `m1/spec-batch-2`.
- Of the questions `m1/spec-batch-2` left open, C2, C6, C7, and C8 were closed
  by `m1/checker-gaps` and `m1/checker-gaps-2`. C1, C3, C4, C5, C9, C10, C11,
  C14, C15, and C16 are written in the specification by step 29.
- C12 (a toolchain record in the workspace revision) and C13 (the operations of
  each conformance profile) belong to M2.
- C17 does not occur: a source-graph fault, such as
  `@module.fileDirectoryCollision`, ends `vibra test` before any item, with no
  items. An import cycle is a resolution error, so every selected test is
  `@test.invalid`, as the tooling chapter states.

## Known limitations

Each limitation below is a program that the checker answers short of the
specification, and the answer it gives.

1. **A default member's contract call on the interface's own type parameter,
   when its bound leaves an associated type open, is unavailable.** Both calls
   in `weigh` are `@tool.unavailable`, and `vibra check` exits 4:

   ```vibra
   interface Holder
     type Item endtype
     fn first takes {value: Self} returns r: Item endfn
     fn weigh takes {value: Self item: Item} returns r: I32 endfn
   endinterface

   interface Weigher(T) given {T: Holder}
     fn weigh takes {value: Self t: T} returns r: I32 do return Holder.weigh(t Holder.first(t)) endfn
   endinterface

   type Box of I32
     impl Holder
       type Item of I32 endtype
       fn first takes {value: Self} returns r: I32 do let Box(v) be value endlet return v endfn
       fn weigh takes {value: Self item: I32} returns r: I32 do return item endfn
     endimpl
   endtype

   type Unit of I32 impl Weigher(Box) endimpl endtype

   fn main returns r: I32 do return Weigher.weigh(Unit(0i32) Box(4i32)) endfn
   ```

   An implementation in a generic type that carries such a bound is supported.

2. **An implementation's identity depends on how its target is spelled.** With
   `src/app/shapes.vib` holding a public `interface Sized` whose `size` has
   `ensures Ordered.greaterOrEqual(n 0i32)`, this `src/app/main.vib` checks:

   ```vibra
   use @app.shapes enduse

   type Box of I32
     impl shapes.Sized
       fn size takes {value: Self} returns n: I32 do return 1i32 endfn
     endimpl
   endtype
   ```

   The obligation's `declaration` is
   `sha256:ff0c5562410c08cc05a3d962f970b9a0a907635d6e76e2e6326e0f7b1b38c029.size`.
   With `use @app.shapes as geo enduse` and `impl geo.Sized` it is
   `sha256:648e17639da8a144c60c4a77bf7ec8859491cc5118ba7d33b07202a89fd26e73.size`,
   and with `impl shapes .Sized` it is
   `sha256:d621fda2bca73234c41e762a263cba94b434c4113bf0267ff582be19b75c3001.size`.
   The identity hashes the target's text with its white space normalized,
   because the resolver has no checked types.

3. **The proof budget closes chains of about 25 integer facts.** This function,
   with `n` parameters `a0` to `a(n-1)` and `n - 1` nested
   `if Ordered.lessOrEqual(ai a(i+1))`, calls `leReq(a0 a(n-1))`:

   ```vibra
   fn leReq takes {a: I32 b: I32} returns r: I32
     requires Ordered.lessOrEqual(a b)
     do return a
   endfn
   ```

   Under the limit profile `m1Default`, a chain of 26 facts (27 parameters) is
   discharged by `INT-7`. A chain of 27 facts (28 parameters) is
   `@tool.limitExceeded` at the predicate, with status `unknown`.
   [`V2-CONTRACT-proof-budget-chain`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-proof-budget-chain/case.toml)
   (20 facts) and
   [`V2-CONTRACT-proof-budget-exceeded`](../../../../conformance/cases/CONTRACT/V2-CONTRACT-proof-budget-exceeded/case.toml)
   pin the two sides.

## Step rows

Every row of the step table says `landed`. For the three rows that did not,
`git merge-base --is-ancestor <merge> v0.0.2/m1-lang` exits 0 for the step's
merge: 01 at `e9effeef`, 21 at `ffc6fb6e`, and 22 at `3a3b0499`. Step 29 lands
with this record.

## Packages after the steps

`git log --merges --oneline v0.0.2/m1-lang` shows each package merged into the
integration branch:

| Package | Merge | What it closed |
| --- | --- | --- |
| `m1/ir-fixes` | `6026a0ca` | the review findings on the checked IR and the interpreter |
| `m1/reader-fixes` | `d727e767` | the review findings on the reader and the formatter |
| `m1/checker-fixes` | `33ee3a20` | the review findings on the checker |
| `m1/guard-fixes` | `eafaa801` | the review findings on static discharge, with the proof budget |
| `m1/member-access` | `e8c18623` | the access of members, and the comment padding rule |
| `m1/callable-rulings` | `02396d33` | bound checks at every position, the callable rulings, and library follow-ups |
| `m1/checker-gaps` | `3736f35d` | the checker's remaining gaps: bounded types, generic invariants, constant patterns, members as values |
| `m1/checker-gaps-2` | `2fb357f0` | guarded anonymous callables, open receivers, open associated types, implementation member identities |
| `m1/spec-batch-1` | `15c86c8b` | the specification questions of steps 02 to 12 |
| `m1/spec-batch-2` | `7a182a25` | the specification questions of steps 13 to 26 |
| `m1/drop-wasm` | `4ee42800` | the removal of the WebAssembly backend; type depth and expanding instantiations |
