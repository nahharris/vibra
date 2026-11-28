# Step 11: eval

Kind: vertical. `vibra eval SUBJECT [--arg VIBON]...`: one checked pure
module function called with decoded arguments against a fresh snapshot
under the published limits, with its result as a canonical value document
or an explicit observation error.
Depends on: 01; fix package E (`m2/fix-e`) merged, for the one spelling of
the reduction limit's name in `limits.reached` (Q26).
Crates: `vibra-workspace`, `vibra-cli`, `vibra-schema`, `vibra-conformance`,
`conformance/cases`.

## Prerequisites

- Read: [05 Program arguments, execution and evaluation](../../../../spec/05-tooling.md#program-arguments-execution-and-evaluation)
  ("Eval", "Limits"); [05 Command payloads](../../../../spec/05-tooling.md#command-payloads) (`eval`);
  [06 Values](../../../../spec/06-runtime.md#values), [06 What has no encoding](../../../../spec/06-runtime.md#what-has-no-encoding);
  [02 Bare guards](../../../../spec/02-type-system.md#bare-guards) GUARD-1;
  [architecture](../architecture.md) rows I15, I16;
  [questions](../spec-questions.md) Q23 to Q26, Q31.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-interp/src/evaluate.rs:313` | `pub fn evaluate` | the compile-time evaluator, with no host context |
| `crates/vibra-interp/src/lib.rs:85` | `pub struct LimitProfile` | `M1_DEFAULT` |
| `crates/vibra-interp/src/lib.rs:127` | `pub enum Evaluation` | a value, a reached limit, a defect |
| `crates/vibra-interp/src/lib.rs:47` | `pub struct MemoryBudget` | the default budget |
| `crates/vibra-types/src/check/mod.rs:235` | `pub fn functions` | the function index of a named declaration |
| `crates/vibra-types/src/check/mod.rs:216` | `pub fn obligations` | the obligations of the check |
| `crates/vibra-ir/src/encode.rs:838` | `pub fn value_document` | the `@value.v2` text of the result |
| `crates/vibra-cli/src/test.rs:54` | `pub fn run` | the model of a command that executes under a budget |
| `crates/vibra-conformance/src/interpret.rs:56` | `pub struct InterpretHandler` | the model of an `interpreterV2` handler |

Proposed: `eval::{eval, EvalRequest, EvalOutcome, decode_argument}`;
`vibra-cli::eval::run`; `vibra-schema::eval_payload::EvalPayload`;
`vibra-conformance::eval::EvalHandler`.

## Files owned

- `crates/vibra-workspace/src/eval.rs`
- `crates/vibra-workspace/tests/it/eval.rs`
- `crates/vibra-cli/src/eval.rs`
- `crates/vibra-cli/tests/it/eval.rs`
- `crates/vibra-schema/src/eval_payload.rs`
- `crates/vibra-schema/schemas/v2/payloads/eval.json`
- `crates/vibra-schema/tests/it/eval_payload.rs`
- `crates/vibra-conformance/src/eval.rs`
- `conformance/rules/m3-11.tsv`
- the case directories of the matrix below

## Ordered tasks

1. Subject: a canonical entity atom resolving to a top-level `fn` of any
   unit with no type parameters; a `proc`, a member, a generic, a type, or
   an unknown atom is invalid input (I15). A workspace whose check has an
   error is `@command.diagnostics`.
2. Arguments (Q23): one `--arg` per positional parameter, decoded as the
   `value` of a `@value.v2` encoding against the parameter type, with
   every width, invariant, and nominal identity checked; a wrong count or
   a value that does not decode is invalid input; labelled parameters take
   their defaults.
3. A bare `requires` of the subject is discharged at the checked call
   boundary as GUARD-1 discharges a call with constant operands (Q24):
   refuted is `@contract.refuted` at the predicate, related to the
   argument; not established is `@contract.unproved`; a discharge the
   derivation limit stops is `@tool.limitExceeded` at the predicate with
   status `unknown`; in each case nothing runs.
4. Evaluation through `vibra_interp::evaluate` with no `HostContext`,
   under `LimitProfile::M1_DEFAULT` and the default memory budget, against
   a snapshot taken for this command alone. A reached reduction limit is
   `@tool.limitExceeded` with no location, status `unknown`,
   `limits.reached: ["evaluationReductions"]` (the spelling fix E landed);
   memory exhaustion is `@runtime.memoryExhausted`,
   `@command.operationalFailure`, `["memory"]`.
5. The result: `value` as the canonical `@value.v2` text; a subject whose
   result type is not `Encodable` is invalid input; an interface value
   holding a callable, a plan, or a grant gives `observationError` with
   `@tool.unsupportedValue` and `value: null` (Q25).
6. `EvalPayload {subject, value, observationError}`, its schema, its
   exhausted form (`value: null`, `observationError: null`); the command;
   `EvalHandler` comparing `expect.eval` as a decoded `@value.v2` document
   (Q31).

## Positions

| Rule | Positions, each with a positive and a negative row |
| --- | --- |
| an argument decodes against its parameter type | a scalar; a string; a record; a wrapper with `where`; a variant with payload; a list; a dict; against: a wrong width, a refuted `where`, a wrong variant, a wrong field, a missing argument |
| the call boundary discharges a bare guard | a true predicate; against: refuted; not established; the derivation limit reached |

## Test matrix

Cases are `eval` under `interpreterV2`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-eval-scalar` | positive | `@app.math.double --arg 21i32` | `value` the `@value.v2` text of `42i32`; `subject` the identity | 05: "Eval calls one checked pure module function" |
| `V2-TOOL-eval-arg-record` | positive | a `fn` taking `User`; `--arg {kind: @record fields: {name: "a" id: {kind: @wrapper value: 1u64}}}` | the function's result | Q23 |
| `V2-TOOL-eval-arg-variant` | positive | `--arg {kind: @enum variant: @circle payload: {radius: 1.5f64}}` | the result | Q23 |
| `V2-TOOL-eval-arg-where-refuted` | negative | a `Positive` wrapper with `--arg {kind: @wrapper value: 0i32}` | `@command.invalidInput` | 06 Values: "A decoder MUST check … any `where`" |
| `V2-TOOL-eval-arg-wrong-width` | negative | `--arg 1u8` for an `I32` parameter | `@command.invalidInput` | 05: "An argument that cannot have the required type is an input error" |
| `V2-TOOL-eval-arg-count` | negative | one argument for two parameters | `@command.invalidInput` | the same |
| `V2-TOOL-eval-arg-labelled-default` | positive | a `fn` with a labelled parameter, no argument for it | the default is used | Q23 |
| `V2-TOOL-eval-generic-invalid` | negative | `fn first(T)` | `@command.invalidInput` | Q23 |
| `V2-TOOL-eval-proc-subject-invalid` | negative | a `proc` subject | `@command.invalidInput` | 05: "It MUST NOT run a procedure or a plan" |
| `V2-TOOL-eval-member-subject-invalid` | negative | `@app.model.User.nameLength` | `@command.invalidInput` | 05: "one checked pure module function" |
| `V2-TOOL-eval-requires-passed` | positive | `half --arg 2i32` with a bare `requires Ordered.greater(n 0i32)` | the value | Q24 |
| `V2-TOOL-eval-requires-refuted` | negative | `half --arg 0i32` | `@contract.refuted` at the predicate, `@command.diagnostics` | Q24; 02 GUARD-1 |
| `V2-TOOL-eval-requires-unproved` | negative | a `fn` whose bare `requires` names a user `Ordered` the rules do not read, with any argument | `@contract.unproved` at the predicate, `@command.diagnostics`, nothing evaluated | 07: "a bare guard is not established by the rules" |
| `V2-TOOL-eval-requires-exhausted` | negative | a `fn` whose bare `requires` needs more than 100,000 derivations | `@tool.limitExceeded` at the predicate, status `unknown`, nothing evaluated | 07: "the predicate when the limit stopped an obligation, whose status is then `unknown`" |
| `V2-TOOL-eval-check-error` | negative | a workspace with a type error elsewhere | `@command.diagnostics` with that error, no value | 05: "An unproved static obligation is an error for eval as for check" |
| `V2-TOOL-eval-std-subject` | positive | a parameterless top-level `fn` of the embedded library | its value | 05: "one checked pure module function" |
| `V2-TOOL-eval-unencodable-type-invalid` | negative | a `fn` returning a `Fn` | `@command.invalidInput` | Q25; 06: "A function, a procedure, a plan, and a sealed authority have no value encoding" |
| `V2-TOOL-eval-interface-value-observation-error` | positive | a `fn` returning `Any` that holds a function | `@command.ok`, `value: null`, `observationError.code: "@tool.unsupportedValue"` with the operand's type | 05: "A result that cannot be shown is an explicit tool outcome, not a trap" |
| `V2-TOOL-eval-plan-built-not-run` | positive | a `fn` returning `Any` that holds a plan built from a `proc` | `@command.ok`, `value: null`, the observation error; the host test shows no `await` ran and no host call was made | 05: "It MUST NOT run a procedure or a plan"; Q25 |
| `V2-TOOL-eval-reductions-limit` | negative | a `fn` that recurses without end in tail position | `@tool.limitExceeded`, status `unknown`, `limits.reached: ["evaluationReductions"]`, `complete: false` | 05 Limits: "Reaching a tool limit is an unknown outcome" |
| `V2-TOOL-eval-memory-exhausted` | negative | a `fn` that builds a list without bound in non-tail recursion | `@runtime.memoryExhausted`, `@command.operationalFailure`, exit 3, `["memory"]` | 06 Activations and memory |
| `V2-TOOL-eval-fresh-snapshot` | positive | two evaluations of a `fn` reading a module value with a side-free initializer | equal results; the host test shows two captures | 05: "Each call runs against a fresh snapshot" |
| `V2-TOOL-eval-value-changed-between-calls` | positive | the module value's initializer changed between two `eval` commands | the second result is the new value and its envelope names the new revision | 05: "Each call runs against a fresh snapshot"; Q26 |
| `ATK-eval-no-host-context` | negative | the host test: `eval` of a `fn` whose module imports `@std.io` | no `HostContext` is constructed; the evaluator is called with none | 05: "make a grant, or use ambient authority" |
| JSON validates | positive | every envelope above | validates | 05 Versioned envelope and results |

The row `V2-TOOL-eval-std-subject` uses a parameterless top-level `fn` of
the library if one exists at the step; if none does, the row is a `fn` of
a `@path` dependency and the report says so.

## Diagnostics and schema changes

Codes: `@contract.refuted`, `@contract.unproved`, `@tool.limitExceeded`,
`@tool.unsupportedValue`, `@runtime.memoryExhausted`. Schema: the `eval`
payload. The limit name `evaluationReductions` is the `limits.reached`
value fix E states in 05.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; `help` lists `eval` as supported.

## Stays unavailable

Labelled arguments on the command line; a generic subject; a member
subject.

## Excluded

Procedures, plans, grants, worlds.

## Completion evidence

- [ ] `conformance/rules/m3-11.tsv` has one `active` row per case id.
- [ ] Every row of "Positions" has its case; the report maps each.
- [ ] The report quotes the call of `evaluate` showing no host context and
      names fix E's commit.
- [ ] The validation commands exit 0.
