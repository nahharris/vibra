# v0.0.2 M3: architecture

The design record of M3. The [step table](README.md#step-table) and the
guides under [steps/](steps/) implement it; the
[specification questions](spec-questions.md) list what it assumes. The
specification is the authority: where this record and a chapter disagree,
the chapter wins and the disagreement is a finding.

## 1. Where each mechanism lives

### Crates and edges

M3 adds no crate. It adds one dependency edge, in step 01:
`vibra-workspace` → `vibra-fs-scope`, so that the edit engine writes through
the held workspace directory with the guarantees the M2 drivers have.
`vibra-host` already depends on `vibra-fs-scope`, so the graph stays acyclic
and the architecture test gains one permitted arrow.

```text
vibra-diagnostics ← vibra-syntax ← vibra-ir ← vibra-interp ← vibra-types
        ↑               ↑                                          ↑
   vibra-schema     vibra-fmt        vibra-fs-scope ← vibra-host ← vibra-workspace
        ↑                                   ↑                           ↑
        └──────────────── vibra-cli ────────┴───── vibra-conformance ───┘
```

`vibra-workspace` is the one workspace engine of the tooling chapter: it
holds the snapshot, the revision, the identities, the approvals, the query
projections, the edit plans, the evaluator entry, and the presentation of
authority. `vibra-cli` and `vibra-conformance` are adapters: they parse a
command line or a case, call the engine, and render. Neither computes a
fact of its own, which is the rule "no second checker or second symbol
index". `vibra-schema` renders and validates the JSON contracts and depends
on `vibra-diagnostics` only, as before.

The language crates (`vibra-syntax`, `vibra-resolve`, `vibra-ir`,
`vibra-interp`, `vibra-types`) change in three places only, each a module
that records or exposes a fact the checker already has:

| Crate | New module | What it gives the engine |
| --- | --- | --- |
| `vibra-syntax` | `position` | the smallest node that contains an offset, the node's grammar category, and the forms and labels the grammar permits there |
| `vibra-fmt` | `fragment` | the canonical source text of one node ([Q2](spec-questions.md#q2-canonical-source-text)) |
| `vibra-types` | `check/facts` | one fact per checked node: role, context, expected and observed types, the identity it binds or refers to, its application or pattern fact, its status |

`vibra-ir`, `vibra-interp`, and `vibra-host` are not changed by M3. The
claim uses the signature and implementation records `vibra-ir` already
encodes; the witness uses the references `vibra-resolve` already produces;
`eval` calls `vibra_interp::evaluate` as the checker does at compile time;
the presentation of authority reads the `BindingPlan` and the recipe
identity of `vibra-host`. This is also what keeps M3 clear of M4, whose
work on the native track and the fix package E, which walks sequences in
`vibra-ir` and every consumer, may run beside it. M3 must not restructure:
`crates/vibra-ir/` (every module, `expr.rs`, `module_set.rs`, `types.rs`,
`program.rs`, `authority.rs`, `origin.rs`, `encode.rs` included),
`crates/vibra-interp/` (`machine.rs`, `evaluate.rs`, `host.rs`, `plan.rs`),
`crates/vibra-host/`, `crates/vibra-lower/`, `crates/vibra-exec/`,
`crates/vibra-codegen/`, `crates/vibra-native/`, `crates/vibra-nexus*/`,
`crates/vibra-build/`, `crates/vibra-actors/`, the five `-v1` crates, and
`stdlib/`. A guide that needs a change there stops and reports it.

### Mechanisms

| Mechanism | Crate and module | Machine document | Step |
| --- | --- | --- | --- |
| Canonical source text of a node | `vibra-fmt::fragment` | none; the text is hashed | 02 |
| Claim identity, `@claim.v2` | `vibra-workspace::identity::claim` | `@claim.v2` (VIBON, hashed) | 03 |
| Witness identity, recursive groups, `@witness.v2` | `vibra-workspace::identity::witness` | `@witness.v2` (VIBON, hashed) | 04 |
| Query subjects, the `query` command, the code subject | `vibra-workspace::query::subject`, `vibra-cli::query`, `vibra-schema::query_payload` | `urn:vibra:schema:v2:query` | 05 |
| Structural position record | `vibra-syntax::position`, `vibra-workspace::query::structural` | `sourcePositionQuery` v2, a `$def` of the query schema | 06 |
| Edit plan data and schema | `vibra-workspace::edit::plan`, `vibra-schema::edit_plan` | `urn:vibra:schema:v2:edit-plan` | 07 |
| Facts per node | `vibra-types::check::facts` | none | 08 |
| Approvals, `@approval.v2`, their validation at `check` | `vibra-workspace::approval` | `@approval.v2` (VIBON) | 09 |
| Bounded evaluation, `eval` | `vibra-workspace::eval`, `vibra-cli::eval`, `vibra-schema::eval_payload` | the `eval` payload | 10 |
| `edit replace` preview | `vibra-workspace::edit::replace`, `vibra-cli::edit` | `EditPlan` | 11 |
| Semantic envelope, entity subjects | `vibra-workspace::query::semantic` | `workspacePositionQuery` v2, a `$def` of the query schema | 12 |
| Presentation of authority, `approve` | `vibra-workspace::present`, `vibra-cli::approve`, `vibra-schema::approve_payload` | the `approve` payload | 13 |
| Atomic apply, the journal, confinement | `vibra-workspace::edit::apply`, `vibra-workspace::edit::journal` | `@editJournal.v2` (VIBON, on disk during a publish) | 14 |
| Effects projection, `effectWitnesses` | `vibra-workspace::query::effects` | the `effects` member of the query payload | 15 |
| Expansions | `vibra-workspace::query::expansions` | the `expansions` member | 16 |
| `edit rename` | `vibra-workspace::edit::rename` | `EditPlan` | 19 |
| `edit imports` | `vibra-workspace::edit::imports` | `EditPlan` | 20 |
| `edit fix`, fix capabilities | `vibra-workspace::edit::fix`, `vibra-diagnostics::registry` | `EditPlan`, `fixes[]` of a diagnostic | 21 |
| The index, `@index.v2` | `vibra-workspace::query::index`, `vibra-schema::index` | `@index.v2`, `urn:vibra:schema:v2:index` | 22 |
| Test work limit, limit names | `vibra-workspace::test_runner` (one budget field), `vibra-cli::test` | the `test` payload | 23 |
| `lint` | `vibra-workspace::edit::lint`, `vibra-cli::lint` | the `lint` payload | 24 |
| Generated reference data, the generator | `vibra-conformance::bin::reference` | `docs/reference/data/*` | 25 |
| The syntax card and checked examples | `vibra-conformance::bin::reference`, `docs/reference/` | `docs/reference/syntax-card.md` | 26 |
| The `query`, `eval`, `edit` case handlers | `vibra-conformance::{query,eval,edit}` | case expectations | 01, 05, 10, 11 |
| Output bound of every command | `vibra-cli::render` | every envelope | 01 |

### The seams of step 01

M2's lesson is that parallel steps collide in shared files. Step 01 puts a
seam everywhere two M3 steps would otherwise edit one file, before any
feature lands, and changes no observable behaviour:

| Shared file | Seam |
| --- | --- |
| `crates/vibra-workspace/src/lib.rs` | `mod` lines and re-exports for `identity`, `approval`, `present`, `eval`, `query`, `edit`; each new module is an empty file a later step owns |
| `crates/vibra-workspace/src/snapshot.rs` | one call, `approval::capture(root)`, that returns no record until step 09 |
| `crates/vibra-workspace/src/semantic.rs` | one call, `approval::validate(&snapshot, &check)`, that returns no diagnostic until step 09 |
| `crates/vibra-types/src/check/expr.rs`, `pattern.rs`, `declare.rs` | one call each into `check::facts`, which records nothing until step 08; `CheckResult::facts()` returns an empty table |
| `crates/vibra-syntax/src/lib.rs`, `crates/vibra-fmt/src/lib.rs` | `mod position;`, `mod fragment;`, each an empty module |
| `crates/vibra-cli/src/lib.rs` | one dispatch arm per new command (`query`, `eval`, `approve`, `lint`, `edit replace`, `edit rename`, `edit imports`, `edit fix`) into a module that answers `@command.unavailable` until its step lands |
| `crates/vibra-cli/src/inventory.rs` | `supported` is read from each command module's `SUPPORTED` constant, so a step flips its own module only; `OPERATION_PROFILES` gains `eval` and `edit` |
| `crates/vibra-cli/src/args.rs` | the operands and options of every new command, `--expect-revision` included, parsed into `Invocation` now; an unavailable command's operands are not validated, as the chapter says |
| `crates/vibra-cli/src/render.rs` | the output bound of [Q28](spec-questions.md#q28-the-output-limit-of-every-command): `OUTPUT_LIMIT`, the `outputBytes` name, and the assertion in `emit` |
| `crates/vibra-schema/src/envelope.rs` | one `Payload` variant per new payload, each a `Box` of a struct in a module the command's step owns |
| `crates/vibra-schema/schemas/v2/command-result.json` | the payload alternatives move to one file each under `schemas/v2/payloads/`, referenced by `$ref`, so a step edits its own payload schema |
| `crates/vibra-conformance/src/profile.rs`, `standard.rs`, `manifest.rs` | the operations `eval` and `edit` ([Q31](spec-questions.md#q31-case-operations-for-eval-and-edit)), the manifest keys of [Q30](spec-questions.md#q30-the-query-case-inputs-expectation-comparison) and Q31, and three handlers (`query`, `eval`, `edit`) that answer unavailable until their steps |
| every `tests/it/main.rs` of the five tooling crates | the `mod` line of every test file a later step owns |

After step 01 no M3 step owns `lib.rs`, `snapshot.rs`, `semantic.rs`,
`expr.rs`, `pattern.rs`, `declare.rs`, `args.rs`, `render.rs`,
`envelope.rs`, `command-result.json`, `profile.rs`, `standard.rs`,
`manifest.rs`, or a `main.rs`.

### Boundary types

These are proposals, not claims that the APIs exist. Two agents build the
two sides of each boundary against these shapes; a change to one is a change
to both.

**Identity** (`vibra-workspace::identity`, steps 02 to 04):

```rust
// vibra-fmt/src/fragment.rs
/// The canonical source text of one node: the formatter's tokens, comments
/// dropped, laid out by the one-space rule of canonical bytes (Q2).
pub fn canonical_text(document: &Document, node: &CstNode) -> Option<String>;

// vibra-workspace/src/identity/mod.rs
pub struct Identities {
    /// Every declaration of the local package and of its dependencies, by
    /// declaration identity.
    pub declarations: BTreeMap<String, DeclarationIdentities>,
}
pub struct DeclarationIdentities { pub claim: String, pub witness: String, pub group: usize }
pub fn identities(snapshot: &SourceSnapshot, resolution: &Resolution,
                  check: &WorkspaceCheck) -> Result<Identities, IdentityError>;

// vibra-workspace/src/identity/claim.rs
pub struct ClaimRecord { /* the fields of @claim.v2 in order */ }
impl ClaimRecord {
    pub fn canonical_bytes(&self) -> String;
    pub fn identity(&self) -> String;   // sha256 of vibraClaimIdentityV2, 0, bytes
}
pub fn claim_of(declaration: &ResolvedDeclaration, check: &WorkspaceCheck,
                targets: &[TargetFact]) -> Result<ClaimRecord, IdentityError>;

// vibra-workspace/src/identity/witness.rs
pub struct Group { pub members: Vec<String>, pub dependencies: BTreeSet<usize> }
/// Strongly connected components of the reference graph, by an iterative
/// Tarjan over an explicit stack, in a deterministic order.
pub fn groups(resolution: &Resolution) -> Vec<Group>;
pub struct WitnessRecord { /* the fields of @witness.v2 in order */ }
```

**Facts** (`vibra-types::check::facts`, step 08):

```rust
pub struct Facts { nodes: BTreeMap<NodeKey, NodeFact> }
pub struct NodeKey { pub source_id: Arc<str>, pub span: ByteSpan }
pub struct NodeFact {
    pub role: Role,            // the nine roles of the chapter
    pub context: Context,      // the fifteen contexts
    pub status: FactStatus,    // Exact | Recovered
    pub expected: Option<(Type, bool)>,   // the type and whether it was written
    pub observed: Option<Type>,
    pub identity: Option<Identity>,       // Declaration(id) | Local(binder:…) | None for `_`
    pub application: Option<ApplicationFact>,
    pub pattern: Option<PatternFact>,
    pub enclosing: Option<String>,        // the declaration identity
}
impl CheckResult { pub fn facts(&self) -> &Facts; }
```

A fact is recorded where the checker decides it, from the checked tree,
never from syntax alone: `expr.rs` records an expression's fact when its
type is final, `pattern.rs` a pattern's, `declare.rs` a header's. A query
projects these; it computes nothing.

**Query** (`vibra-workspace::query`, steps 05 to 16, 22):

```rust
pub enum Subject {
    Position { source_id: String, offset: u64 },
    Entity(String),            // a canonical entity atom
    DiagnosticCode(DiagnosticCode),
    Workspace,
}
pub struct QueryOptions { pub include: Vec<Include>, pub expand: Vec<Expand> }
pub struct QueryResult {
    pub subject: String,
    pub resolved_kind: Option<ResolvedKind>,
    pub identity: Option<Identity>,
    pub metadata: BTreeMap<Include, serde_json::Value>,
    pub expansions: BTreeMap<Expand, serde_json::Value>,
    pub truncated: bool,       // the output bound cut a list
}
pub fn query(capture: &WorkspaceCapture, subject: &Subject, options: &QueryOptions)
             -> Result<QueryResult, QueryRefusal>;   // invalid input, or the limit alone
```

**Edit** (`vibra-workspace::edit`, steps 07, 11, 14, 19 to 21, 24):

```rust
pub struct EditPlan {
    pub revision: String, pub operation: Operation, pub target: Option<Identity>,
    pub documents: Vec<PlanDocument>,      // {path, revision}, sorted by path
    pub edits: Vec<Edit>,                  // sorted by path then span, no overlap
    pub resulting_diagnostics: Vec<Diagnostic>,
    pub postconditions: Vec<Postcondition>,  // {declaration, kind, before, after}
}
pub struct Edit { pub path: String, pub span: ByteSpan, pub replacement: String,
                  pub syntax_identity: SyntaxIdentity, pub fingerprint: String }
pub enum Operation { Format, Fix, Rename, Imports, Replace }
pub fn plan_replace(capture: &WorkspaceCapture, subject: &str, data: &str) -> Planned;
pub fn plan_rename(capture: &WorkspaceCapture, subject: &str, new_name: &str) -> Planned;
pub fn plan_imports(capture: &WorkspaceCapture, path: &str) -> Planned;
pub fn plan_fix(capture: &WorkspaceCapture, subject: &str) -> Planned;
pub enum Planned { Plan(EditPlan), InvalidInput(String), Denied(Vec<Diagnostic>) }

// vibra-workspace/src/edit/apply.rs
pub struct Expectation { pub revision: Option<String> }  // --expect-revision
pub fn apply(workspace: &Path, plan: &EditPlan, expectation: &Expectation) -> Applied;
pub enum Applied {
    Applied { resulting_revision: String },
    Stale(Diagnostic),           // @tool.stalePlan, nothing written
    Unavailable,                 // no scoped adapter on this platform
    Failed(Diagnostic),          // @module.ioError, nothing published
}
// vibra-workspace/src/edit/journal.rs
pub const JOURNAL_FILE: &str = ".vibra-journal.vibon";
pub fn replay(workspace: &Path) -> Result<Option<Replayed>, JournalFault>;
```

**Approval and presentation** (steps 09, 13):

```rust
// vibra-workspace/src/approval.rs
pub struct Approval { /* the fields of @approval.v2 (Q7) */ }
pub enum ApprovalState { None, Current, Stale(Vec<Diagnostic>) }
pub fn capture(root: &Path) -> Result<Vec<(String, Vec<u8>)>, Vec<Diagnostic>>;  // approvals/*.vibon
pub fn validate(snapshot: &SourceSnapshot, check: &WorkspaceCheck,
                identities: &Identities) -> Vec<Diagnostic>;   // Q9: staleApproval, invalidBinding
pub fn protects(snapshot: &SourceSnapshot, path: &str) -> bool;  // a @tests module under a current approval
// vibra-workspace/src/present.rs
pub struct Presentation { pub target: String, pub entry: Identity, pub claim: String,
                          pub bindings: Vec<PresentedBinding>, pub design_limits: [DesignLimit; 2],
                          pub approval: ApprovalState }
pub fn present(check: &WorkspaceCheck, identities: &Identities, target: &str) -> Result<Presentation, Refusal>;
pub fn approval_document(presentation: &Presentation, snapshot: &SourceSnapshot) -> Approval;
```

**Evaluation** (step 10):

```rust
// vibra-workspace/src/eval.rs
pub struct EvalRequest<'a> { pub subject: &'a str, pub arguments: &'a [String] }
pub enum EvalOutcome {
    Value { subject: Identity, document: String },           // @value.v2 text
    Unobservable { subject: Identity, error: ObservationError },
    Diagnostics(Vec<Diagnostic>),                            // refuted requires, check errors, limit
    InvalidInput(String),
}
pub fn eval(capture: &WorkspaceCapture, request: &EvalRequest<'_>, limits: &LimitProfile) -> EvalOutcome;
```

### What changes in existing crates

| Crate | Change | Step |
| --- | --- | --- |
| `vibra-workspace` | the modules above; `snapshot.rs` captures approval records; `semantic.rs` validates approvals; `test_runner.rs` takes a reduction budget beside the memory budget | 01, 09, 23 |
| `vibra-types` | `check/facts.rs` and one call from each of three files; `CheckResult::facts` | 01, 08 |
| `vibra-syntax` | `position.rs` | 01, 06 |
| `vibra-fmt` | `fragment.rs` | 01, 02 |
| `vibra-diagnostics` | fix capability `@safe` for two codes ([Q21](spec-questions.md#q21-which-codes-gain-a-safe-fix)); the test `no_code_is_registered_as_fixable` becomes the list of the two | 21 |
| `vibra-schema` | six payload modules, the query, index, and edit-plan schemas, the `$ref` layout of the command-result schema | 01, 05, 07, 10, 13, 22, 24 |
| `vibra-cli` | six command modules, `--expect-revision`, the output bound, the inventory flips | 01 and each command's step |
| `vibra-conformance` | three handlers, two operations, the manifest keys, the `m3_coverage` audit, the reference generator | 01, 05, 10, 11, 25, 36 |

## 2. The identity and evidence argument

For every invariant the chapters state for M3: the layer that enforces it,
the layer that checks it again, the attack cases, and what would have to be
wrong for it to fail. An `ATK-` case is a host test; a `V2-` case is in the
corpus. The step in brackets owns the case. The theory each rests on is
named in the row; the sections after the table give the argument.

| # | Invariant | Enforced by | Re-validated by | Attack cases | It fails only if |
| --- | --- | --- | --- | --- | --- |
| I1 | A declaration's anchor identity is a function of its anchor alone: a body edit keeps it, a rename or move changes it, and so does every member's | `vibra-resolve::identity` (M1) | the claim and witness records carry the identity and the query reports it; a rename plan lists every changed identity in `postconditions` | `V2-TOOL-claim-body-edit-same` [03], `V2-TOOL-claim-rename-changes` [03], `V2-TOOL-edit-rename-postconditions-nominal` [19] | the anchor hashes a field that is not one of the six |
| I2 | A claim identity is a function of canonical content: formatting, comments, line endings, the checkout's path, and the platform leave it; a signature, bound, default, guard, representation, implementation choice, or an entry's binding changes it | `identity::claim` over `fragment::canonical_text`, signature records, and recipe identities | the 05 vector is a host test; the index and the approval carry the identity and agree with a recomputation from the formatted tree | `V2-TOOL-claim-comment-same` [03], `V2-TOOL-claim-layout-same` [03], `V2-TOOL-claim-signature-changes` [03], `V2-TOOL-claim-vector` [03], `ATK-identity-crlf-same-claim` [03], `ATK-identity-path-independent` [03] | a non-canonical byte reaches the record |
| I3 | A witness covers the bodies of one recursive group and the witnesses of what they resolve to, however far; an unrelated change leaves it; no witness depends on itself | `identity::witness`: strongly connected components over resolved references, hashed as one unit | the 05 vector; a change in a dependency changes every dependent's witness and no other; the group graph is checked acyclic after construction | `V2-TOOL-witness-vector` [04], `V2-TOOL-witness-dependency-change` [04], `V2-TOOL-witness-unrelated-same` [04], `V2-TOOL-witness-group-acyclic` [04], `ATK-witness-no-self-dependency` [04] | an edge is missed, or a component is split |
| I4 | The workspace revision hashes exact input bytes, so a change of line endings is a new revision while the claims are not | `vibra-workspace::revision` (M1) | `revisions.ps1 -Check` recomputes every expected revision from the case bytes with no engine code | `V2-TOOL-revision-crlf-differs` [03] | the snapshot normalizes bytes |
| I5 | A query answers from one captured revision, names it, and rereads no disk | `WorkspaceCapture` is taken once; `query` takes the capture and no path | an architecture test lists every read of the filesystem in `vibra-workspace` and allows discovery, capture, approvals, and the journal | `V2-TOOL-query-revision-named` [05], `ATK-engine-reads-enumerated` [05] | a projection opens a file |
| I6 | There is no store: no derived file exists whose deletion changes source, approvals, identity, or execution | M3 writes no cache; the journal is a write-ahead log, not a cache, and is deleted when a publish completes | the same architecture test lists every write site and allows two: the plan's documents and the journal | `ATK-edit-writers-enumerated` [14] | a step adds a file the engine reads back |
| I7 | A plan applies only against the revision it was planned for; a stale one is `@tool.stalePlan` with nothing written | `edit::apply`: the caller's `--expect-revision` against the capture's; every document's bytes against the plan's `documents[].revision` and each edit's `fingerprint`, read through the handle opened for the replacement | the journal records the expected digest of every document, so a replay refuses a document that is neither at its old nor at its new digest | `ATK-edit-apply-expect-revision` [14], `ATK-edit-apply-stale-document` [14], `ATK-edit-apply-document-changed` [14], `ATK-edit-apply-fingerprint-mismatch` [14] | a document is read twice, once to check and once to replace |
| I8 | A plan publishes every document or none: a crash between files leaves a state the next command completes, and an interrupted application is never reported applied | the journal of [Q15](spec-questions.md#q15-atomic-application-of-several-files): written and flushed before the first replacement, deleted after the last; `applied` is set after the deletion | `replay` runs before every snapshot; a replay is idempotent | `ATK-edit-journal-crash-between-files` [14], `ATK-edit-journal-replay-idempotent` [14], `ATK-edit-journal-malformed` [14] | a document is replaced before the journal is durable |
| I9 | The engine writes only the plan's documents and the journal, under the workspace root, through the held root directory, following no link and accepting no alias | `edit::apply` opens the root once with `vibra-fs-scope` and reaches each document one name at a time; `edit::journal` creates the journal relative to the same handle | the write-site architecture test; the attack rows read the tree after a refusal | `ATK-edit-apply-symlink-refused` [14], `ATK-edit-apply-alias-refused` [14], `ATK-edit-writers-enumerated` [14] | a path is joined and reopened by name |
| I10 | An edit reaches the modules of the root project's own units only: a `@path` dependency, `@std`, and `project.vibon` are not subjects | `edit::plan`: the subject's unit is a target or `@tests` of the root project | the plan's `documents` are checked against the snapshot's units before publish | `V2-TOOL-edit-dependency-subject-invalid` [11], `V2-TOOL-edit-std-subject-invalid` [11] | a document path escapes the unit check |
| I11 | A rename is shown as an identity-affecting change; a body edit changes witnesses only | `postconditions` are computed by recomputing every identity of the local package on the staged snapshot and listing each difference by kind | the index of the resulting snapshot agrees with `postconditions[].after` | `V2-TOOL-edit-rename-postconditions-nominal` [19], `V2-TOOL-edit-rename-claim-of-callers` [19], `V2-TOOL-edit-replace-postconditions-witness-only` [11] | identities are compared by spelling, not recomputed |
| I12 | An approval names what it approved and is void when a protected test module, the entry's claim, a binding, the selection, or the toolchain changes | `approval::validate` at every `check` | the digests are recomputed from the snapshot's bytes, never read from a cache; `test`, `run`, and `edit` refuse through the check's diagnostics | `V2-GRANT-approval-current` [09], `V2-GRANT-approval-stale-test` [09], `V2-GRANT-approval-stale-claim` [09], `V2-GRANT-approval-stale-toolchain` [09], `V2-GRANT-approval-unapproved-binding` [09], `V2-RUNTIME-run-stale-approval-refused` [09] | a command checks the snapshot without validating approvals |
| I13 | Under a current approval, a protected test module, helper, or fixture cannot be edited by the tool, while the code under repair can | `edit::plan` refuses a plan with a document `approval::protects` as `@tool.actionDenied`; `fmt --write` and `lint --write` go through the same check | a staged plan that reaches publish with a protected document is refused again at publish | `V2-TOOL-edit-replace-test-denied` [11], `V2-TOOL-edit-replace-under-approval-allowed` [11], `V2-TOOL-edit-rename-into-tests-denied` [19], `ATK-edit-apply-protected-unchanged` [14], `ATK-fmt-write-protected-denied` [14] | a document is added to a plan after the check |
| I14 | A tool grants no authority and an observation restores none: `approve` acquires nothing, a query's authority projection is data, an approval document is data | `present` reads the `BindingPlan` and never calls `admit` or `start`; `query::effects` builds projections from recipes; the approval decoder yields a record | no `Context` is constructed outside `run` and the test runner, which the M2 architecture tests already pin | `ATK-approve-no-acquisition` [13], `ATK-query-authority-is-data` [15], `ATK-approval-no-signature-is-data` [09] | a presentation constructs a broker |
| I15 | Evaluation runs no procedure or plan, makes no grant, and uses no ambient authority: the subject is a `fn`, the interpreter has no host context, each call runs against a fresh snapshot | `eval`: the subject's kind is `@fn`; `vibra_interp::evaluate` with no `HostContext`, as compile-time evaluation runs | the ambient-reads architecture test already allows host APIs under `vibra-host/src/system/` only | `V2-TOOL-eval-proc-subject-invalid` [10], `ATK-eval-no-host-context` [10], `V2-TOOL-eval-fresh-snapshot` [10] | a subject with a `proc` kind passes the kind check |
| I16 | Every evaluation and every test item ends within a stated work and memory limit with an incomplete outcome, never a program value | `eval` under `LimitProfile::M1_DEFAULT` reductions and the default memory budget; the test runner's per-item reduction budget ([Q27](spec-questions.md#q27-a-work-limit-for-a-test-item)) | `limits.reached` names each limit; the other items of a suite keep their results | `V2-TOOL-eval-reductions-limit` [10], `V2-TOOL-eval-memory-exhausted` [10], `V2-PROJECT-test-reductions-limit` [23], `V2-PROJECT-test-reductions-other-items-run` [23] | a limit is checked after the work |
| I17 | Every command's output is bounded: an envelope over the limit is a valid, truncated document with `complete: false`, never truncated JSON | each payload truncates its longest list ([Q28](spec-questions.md#q28-the-output-limit-of-every-command)) | `render::emit` asserts the bound and traps on a document still over it | `V2-TOOL-query-index-bounded` [22], `ATK-output-bound-asserted` [01] | a payload has a list the truncation does not know |
| I18 | A structural fact never implies the source checked; a semantic fact is `exact`, `recovered`, or `unavailable` on its own, and stays `exact` beside an unrelated broken module; a rejected body shows no exact rows | `check::facts` records a status per node; `query::semantic` projects it | a position inside recovery and a position in a sound module of a broken workspace are rows | `V2-TOOL-query-position-in-recovery` [06], `V2-TOOL-query-semantic-beside-broken-module` [12], `V2-TOOL-query-rejected-body-no-rows` [15] | a fact is recorded before the body's errors are known |
| I19 | A position offset is on a character boundary, at most the end of the file; the smallest containing node wins, a zero-width recovery node first, the deepest then source order | `vibra-syntax::position` | the structural record's `span` contains the offset, or equals it at the end | `V2-TOOL-query-position-boundary` [06], `V2-TOOL-query-position-end-of-file` [06], `V2-TOOL-query-position-inside-scalar` [06], `V2-TOOL-query-position-past-end` [06] | the search uses closed spans |
| I20 | A target's authority is presented from the project file alone: every rule of every policy, every limit, every startup read, and the two design limits, and nothing a run would learn | `present` over the `BindingPlan` and the normalized policies | the presentation's recipe identities equal those of the startup record of a run of the same project | `ATK-approve-presents-every-rule` [13], `ATK-approve-design-limits-shown` [13], `ATK-approve-from-project-alone` [13] | the presentation reads the environment |
| I21 | A changed binding under an approval is `@grant.invalidBinding` at the recipe, once; every other voiding is `@project.staleApproval` once; one fault is reported once | `approval::validate` in the order of [Q9](spec-questions.md#q9-the-code-of-a-void-approval) | the corpus rows of each voiding have exactly one error | `V2-GRANT-approval-unapproved-binding` [09], `V2-GRANT-approval-one-fault` [09] | both checks run on one fault |
| I22 | Every machine output validates against its schema, on the denied, stale, malformed, and exhausted paths included | `Envelope::to_validated_json` before anything is written | a host test drives every command through every ending and validates each envelope | `ATK-schema-every-ending` [36] | a payload is rendered outside the envelope |
| I23 | Every new persistent document decodes its example and round-trips to its canonical bytes, and the hash vectors of 05 hold | the decoders of `@approval.v2`, `@index.v2`, `@editJournal.v2`, the plan schema | the `spec_examples` test of `vibra-conformance` reads the chapter examples | `V2-GRANT-approval-round-trip` [09], `V2-TOOL-index-round-trip` [22], `V2-TOOL-edit-plan-schema` [07] | a field is written out of schema order |
| I24 | The generated references equal the implementation: a drift fails a test, and every checked example gives its registered verdict | the generator reads the reader's tables, the registry, the inventory, and the schemas | the committed files are regenerated and compared by a test; the examples are run through the engine | `ATK-reference-drift-detected` [25], `ATK-reference-example-verdicts` [26] | a file is edited by hand and the test is not run |
| I25 | The repair scenario's evidence is registered before it is gathered: the seed is sealed, the runner sees machine-mode CLI only, the pass rule and the controls are frozen first | the registration of step 31, the wrapper and controls of step 32 | the record of step 35 names the freeze commit and every command the runner issued | `ATK-repair-wrapper-refuses-other-commands` [32], `ATK-repair-seed-sealed` [33] | a choice is made after a result is seen |

### The theories

- **Content addressing and Merkle structure (I1 to I4).** An identity is the
  hash of canonical content and of the identities of what it depends on.
  The anchor hashes six fields; the claim hashes a record whose every
  reference is an identity; the witness hashes a record whose
  `dependencies` are witnesses. Because the groups are the strongly
  connected components of the reference graph, the dependency graph of
  witnesses is a directed acyclic graph by construction, and a witness is
  computed by one pass in reverse topological order with an explicit
  stack. The workspace revision is the one deliberate exception: it hashes
  input bytes because it is the identity of the snapshot, not of a meaning.
- **Optimistic concurrency (I7).** A writer states the version it read
  (`--expect-revision`, the plan's document revisions, each edit's
  fingerprint) and the publish step compares each against the bytes it
  holds open; a mismatch refuses without writing. No lock is taken, and
  no reader is blocked.
- **Write-ahead logging (I8).** The journal is written and made durable
  before the first document changes; each document's replacement is itself
  atomic on the platform; the journal is removed last. Recovery is redo:
  a journal found at startup is replayed, and replay is idempotent because
  each document is either at its old digest or at its new one. "All or
  none" is a property of what any later reader of the tree observes after
  recovery, which is the guarantee a single-file atomic rename cannot give
  for a set and a redo log can.
- **Confinement to the opened object (I9, I10).** The same rule the drivers
  of M2 follow: authority is a held directory handle, a request is resolved
  one real name at a time from it, and nothing is reopened by path.
- **Capability presentation (I12 to I14, I20, I21).** An approval is a
  statement about exactly the authority the project file describes, so it
  names the identities of the recipes and the claim of the entry, and the
  presentation that precedes it is computed from the same data admission
  reads. Neither presents or stores a grant, so neither can leak one.
- **Pure evaluation (I15, I16).** The interpreter without a host context is
  the compile-time evaluator the checker already uses; `eval` adds an
  argument decoder and a limit profile and nothing else.

### What the argument does not cover

- **Semantic equivalence of policies.** Two policies with one meaning may
  hash differently; an approval of one is not an approval of the other.
  The chapter says so of the policy hash, and the approval inherits it.
- **The approver's identity.** An approval document carries no signature;
  whoever may write the tree may approve. Version control is the record.
- **A crash during the journal write itself.** A truncated journal is
  deleted and reported, and nothing it names was written, so the tree is
  unchanged; the plan is simply not applied.
- **Wall-clock time.** No service has a time limit
  ([Q29](spec-questions.md#q29-a-time-limit)).

## 3. The edit path on real files

A preview is computed against the capture's revision: the plan builder
stages every replacement in memory with `SourceSnapshot::with_module_bytes`,
checks the staged snapshot again, formats the affected modules, recomputes
every identity of the local package, and writes the plan: `documents` with
each file's exact-byte revision, `edits` sorted by path and span, each with
its `syntaxIdentity` and `fingerprint`, `resultingDiagnostics`, and
`postconditions`. A plan with an error in `resultingDiagnostics`, a
protected document, or an overlap never reaches publish.

**Atomic, for one file.** The document is opened for replacement through
the held root with `open_for_replace`; its bytes are read through that
handle and compared with the plan's revision and with each edit's
fingerprint; then `replace_contents` truncates and writes on the same
handle, which keeps the file's identity and other names, as the filesystem
driver of M2 does. The replacement is atomic in the sense the platform
gives a single handle: a reader sees the old bytes or the new ones.

**Atomic, for several.** Before the first replacement the engine creates
`.vibra-journal.vibon` exclusively under the root (a journal that already
exists is replayed first, so a second publish never races the first),
writes the `@editJournal.v2` record with the plan's revision and every
document's path, expected digest, and new bytes, and flushes it. It then
replaces each document in path order and deletes the journal. `applied` is
reported `true` only after the deletion. A crash between two replacements
leaves the journal; every command begins by looking for it and finishing
it, so the next observer of the tree sees every document replaced. A
truncated journal, one whose record does not decode, is deleted and
reported on standard error; nothing it names was touched, because the
first replacement waits for the flush.

**Confinement.** The engine may write exactly two things: the documents of
the plan, which are modules of the root project's targets and `@tests`, and
the journal at the root. It reaches both through one held directory handle
opened at the workspace root, one real name at a time, following no link,
accepting no case or short-name alias, through the `vibra-fs-scope`
adapter the M2 drivers use; a path with `..`, a link on the way, or a
component whose real name differs from the plan's is refused before any
write. An architecture test enumerates every call in `vibra-workspace` that
creates, replaces, or removes a file and allows the publish and journal
functions only. The engine never removes a file except the journal, never
renames one, never creates a directory, and never writes under `dep/` or
`approvals/` except through `approve --write`, which writes one approval
document through the same path.

**Platform-specific.** The scoped adapter exists on Windows. Elsewhere
`--write` is `@tool.unavailable` ([Q16](spec-questions.md#q16-applying-an-edit-off-windows)),
the preview is still returned, and the apply rows print `NOT RUN`. The
corpus holds previews only and is the same on three platforms.

## 4. The conformance corpus of M3

| Profile | Operation | What a case states |
| --- | --- | --- |
| `toolingV2` | `query` | a tree, a `subject`, `include` and `expand` lists; the expected payload as JSON (`expect.query`), or an `@index.v2` document (`expect.index`), or the verdict and diagnostics of a refusal |
| `toolingV2` | `edit` (new, [Q31](spec-questions.md#q31-case-operations-for-eval-and-edit)) | a tree, the edit kind and its operands; the expected plan as JSON (`expect.editPlan`), with `resultingDiagnostics` and `postconditions`, or the diagnostics of a denial |
| `interpreterV2` | `eval` (new, Q31) | a tree, a `subject`, `arguments`; the expected `@value.v2` document (`expect.eval`), or an observation error, or the diagnostics of a refused or limited evaluation |
| `staticV2` | `workspaceCheck` | a tree with `approvals/`: the verdict and each diagnostic of a stale approval or an unapproved binding |
| `interpreterV2` | `workspaceRun`, `workspaceTest` | a run refused under a stale approval; a test item that reaches its work limit |

- **Prefixes.** `TOOL` for every query, evaluation, edit, index, and
  identity case; `GRANT` for approvals and bindings; `PROJECT` for the
  test runner's limits; `RUNTIME` for a run under an approval. Layout:
  `conformance/cases/<PREFIX>/<id>/case.toml`.
- **Rules.** Each step owns one file, `conformance/rules/m3-NN.tsv`. No two
  steps edit one rules file.
- **Revisions.** A query or plan expectation that holds a
  `workspaceRevision` is rewritten by `revisions.ps1 -Write`, which step 05
  teaches to find the JSON field, and checked by `-Check`. An approval
  document in a case tree holds digests of the tree's own bytes, computed
  by hand or by the script's `-Tree` form, never by the engine.
- **Identities.** A claim or witness expectation is written as its record
  and hashed by a script over the chapter's framing, as the trace kit of M2
  did; the cases of steps 03 and 04 include the two vectors of 05.
- **Attack cases.** An attack the corpus can express (a stale approval, a
  denied edit, an invalid subject, a limit reached) is a case. One that
  needs a real filesystem (a link, an alias, a crash between files), a
  second process, or a forged value is a host test (`ATK-`) in the crate
  that owns the layer, listed in section 2.
- **Denied, stale, malformed, exhausted.** Each path has a case or host
  test whose expectation includes the envelope's schema validation: the
  CLI tests of each command validate every envelope, and `ATK-schema-every-ending`
  drives every command through every result of the result table.
- **One fault.** A negative case has exactly one error. A row a guide
  cannot write with one fault from forms that exist at that step is a
  finding against the guide.

## 5. What M2 deferred to M3

| Item | Placed |
| --- | --- |
| `eval` | step 10 |
| `query` | steps 05, 06, 12, 15, 16, 22 |
| `lint` | step 24 ([Q22](spec-questions.md#q22-what-lint-is)) |
| the command that presents a target's authority before a run (F8) | step 13, as `approve` ([Q8](spec-questions.md#q8-the-command-that-presents-authority)) |
| the names of reached tool limits | fix package E names the two of M2; steps 10, 23, and 01 add `evaluationReductions`, `testReductions`, and `outputBytes` under the same convention ([Q26](spec-questions.md#q26-the-limits-of-eval-and-their-names) to [Q28](spec-questions.md#q28-the-output-limit-of-every-command)); steps 10 and 23 depend on fix E for the spelling |
| `project init`, `inspect`, `add`, `remove`, `sync` | out of scope: no milestone of the release names them, and M3's scope does not. `inspect` is largely `query @workspace`; `add` and `remove` reuse the edit plan of step 07 when a milestone owns them. They stay `@tool.unavailable`; the owner table's finding stands |
| the contract call on an interface's type parameter whose bound leaves an associated type open | out of scope: a checker rule of the pure language with no M3 mechanism behind it; it stays `@tool.unavailable` and `V2-PROJECT-test-unavailable-closure` stands. A fix package of the checker owns it |
| the slow checks of the M2 exit | out of scope: the checker's work is bounded by its input alone in this release ([Q29](spec-questions.md#q29-a-time-limit)); M3 bounds what it adds. The exit record repeats the gap |
| the `effectWitnesses` expansion (owner table finding) | step 15 |
| `@tool.stalePlan` | step 14 |
| `@tool.actionDenied` | steps 11 and 14 under [Q10](spec-questions.md#q10-the-code-of-an-edit-the-approval-forbids); without that ruling it stays with MCP, which is not in this release |

## 6. Risks

1. **The repair edit is not in the chapter.** Without
   [Q1](spec-questions.md#q1-no-supported-edit-changes-a-body) the release
   gate cannot be run through the CLI. Mitigation: Q1 is the first question
   and is marked for Hannah; steps 11 and 14 are written against it; if it
   is overturned the plan's edit lane shrinks to rename, imports, and fix,
   and the gate's edit becomes a question of the roadmap.
2. **The approval has no specification.** Q7 to Q11 define it. Mitigation:
   the schema is small and mirrors the lock file; step 09 lands before any
   command depends on it; review gate 17 reads the approval code against
   the chapter sentences that exist.
3. **Facts per node change the checker's hot path.** Recording a fact for
   every node costs memory and may slow `check`. Mitigation: facts are
   recorded into a `BTreeMap` behind one hook, measured by a host test on
   the largest corpus trees (less than one fifth more time and memory), and
   step 08 may record lazily per module if the measurement fails.
4. **The journal is a new on-disk artifact.** A bug in replay could write
   wrong bytes. Mitigation: replay checks every document against its old
   or new digest and refuses otherwise; a malformed journal is deleted
   without acting; gate 27 reads the publish and replay paths line by line.
5. **Collisions in the CLI and schema crates.** Six new commands and
   payloads land in parallel. Mitigation: the seams of step 01, one module
   and one payload schema file per command, `supported` read from the
   module.
6. **Canonical text drifts from the formatter.** Two definitions of
   canonical spacing would make the 05 vectors fail. Mitigation: step 02
   derives the text from the formatter's token stream, not from a second
   printer, and the vectors are its tests.
7. **The output bound truncates the wrong list.** Mitigation: each payload
   names its one truncatable list in its step's guide; the emitter asserts
   the bound; `V2-TOOL-query-index-bounded` reaches it.
8. **Two new case operations.** If Q31 is overturned, every `eval` and
   `edit` case becomes a `vibra-cli` host test. Mitigation: the rows are
   written as cases with the question marked; the handler and the host
   test share the fixture trees, so moving a row changes no expectation.
9. **The repair run depends on a model.** Mitigation: the protocol is
   frozen first (step 31), the controls pass first (step 32), the budget
   is fixed, and the record says what the run does not show.
10. **Memory on the build machine.** Mitigation: no guide runs the
    whole-workspace test suite; each names the packages it touches and
    their dependents; the reference generator and the index tests use the
    smallest trees that reach their rows.
