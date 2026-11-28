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
projections, the edit plans, the publisher, the evaluator entry, and the
presentation of authority. `vibra-cli` and `vibra-conformance` are
adapters: they parse a command line or a case, call the engine, and render.
Neither computes a fact of its own, which is the rule "no second checker or
second symbol index". `vibra-schema` renders and validates the JSON
contracts and depends on `vibra-diagnostics` only, as before.

The language crates (`vibra-syntax`, `vibra-resolve`, `vibra-ir`,
`vibra-interp`, `vibra-types`) change in four places only, each a module
that records or exposes a fact the checker or the interpreter already has:

| Crate | Module | What it gives the engine |
| --- | --- | --- |
| `vibra-syntax` | `position` | the smallest node that contains an offset, the node's grammar category, and the forms and labels the grammar permits there |
| `vibra-fmt` | `fragment` | the canonical source text of one node ([Q2](spec-questions.md#q2-canonical-source-text)) |
| `vibra-types` | `check/facts` | one fact per checked node, each semantic field with its own status; the checked edges of every choice the checker makes; the final verdict of each body |
| `vibra-interp` | `evaluate` | the test entry takes a work limit beside its memory budget, the one change step 18 makes there |

`vibra-fs-scope` gains the publish primitive of step 02: exclusive creation
under a reserved name, flush, rename over the destination relative to the
held directory, and removal of a reserved file. `vibra-ir`, `vibra-host`,
and the rest of `vibra-interp` are not changed by M3. The claim uses the
signature and implementation records `vibra-ir` already encodes; the
witness uses the references `vibra-resolve` already produces and the
choices the checker already makes; `eval` calls `vibra_interp::evaluate` as
the checker does at compile time; the presentation of authority reads the
`BindingPlan` and the recipe identity of `vibra-host`. This is also what
keeps M3 clear of M4, whose work on the native track may run beside it, and
of the fix package E, which walks sequences in `vibra-ir` and every consumer
and names every reached limit; steps 11 and 18 depend on E having landed.
M3 must not restructure: `crates/vibra-ir/` (every module), the modules of
`crates/vibra-interp/` other than `evaluate.rs` (`machine.rs`, `host.rs`,
`plan.rs`, `lib.rs`), `crates/vibra-host/`, `crates/vibra-lower/`,
`crates/vibra-exec/`, `crates/vibra-codegen/`, `crates/vibra-native/`,
`crates/vibra-nexus*/`, `crates/vibra-build/`, `crates/vibra-actors/`, the
five `-v1` crates, and `stdlib/`. A guide that needs a change there stops
and reports it.

### Mechanisms

| Mechanism | Crate and module | Machine document | Step |
| --- | --- | --- | --- |
| The publish primitive: create under a reserved name, flush, rename over, remove a reserved file | `vibra-fs-scope::windows` | none | 02 |
| Canonical source text of a node | `vibra-fmt::fragment` | none; the text is hashed | 03 |
| Claim identity, `@claim.v2` | `vibra-workspace::identity::claim` | `@claim.v2` (VIBON, hashed) | 04 |
| Query subjects, the `query` command, the code subject | `vibra-workspace::query::subject`, `vibra-cli::query`, `vibra-schema::query_payload` | `urn:vibra:schema:v2:query` | 05 |
| Structural position record | `vibra-syntax::position`, `vibra-workspace::query::structural` | `sourcePositionQuery` v2, a `$def` of the query schema | 06 |
| Edit plan data and schema | `vibra-workspace::edit::plan`, `vibra-schema::edit_plan` | `urn:vibra:schema:v2:edit-plan` | 07 |
| Facts per node, checked edges, body verdicts | `vibra-types::check::facts` | none | 08 |
| Witness identity, recursive groups, `@witness.v2` | `vibra-workspace::identity::witness` | `@witness.v2` (VIBON, hashed) | 09 |
| Approvals, `@approval.v2`, their validation at `check`, the pin, test-run admission | `vibra-workspace::approval` | `@approval.v2` (VIBON); the `approvals` field of the envelope and of the `test` payload | 10 |
| Bounded evaluation, `eval` | `vibra-workspace::eval`, `vibra-cli::eval`, `vibra-schema::eval_payload` | the `eval` payload | 11 |
| Semantic envelope, entity subjects | `vibra-workspace::query::semantic` | `workspacePositionQuery` v2, a `$def` of the query schema | 12 |
| `edit body` preview | `vibra-workspace::edit::body`, `vibra-cli::edit::body` | `EditPlan` | 13 |
| Presentation of authority, `approve` preview | `vibra-workspace::present`, `vibra-cli::approve`, `vibra-schema::approve_payload` | the `approve` payload | 14 |
| The publisher: the write lock, the commit point, confinement; `--write` of `fmt` and `edit` | `vibra-workspace::edit::apply`, `vibra-cli::edit::write` | the `edit` payload's `applied` and `resultingRevision` | 15 |
| Approval writing, `approve --write` | `vibra-workspace::approve_write`, `vibra-cli::approve::write` | `@approval.v2` on disk | 16 |
| Effects projection, `effectWitnesses` | `vibra-workspace::query::effects` | the `effects` member of the query payload | 17 |
| Test work limit, limit names, the test entry's limit | `vibra-workspace::test_runner`, `vibra-interp::evaluate`, `vibra-cli::test` | the `test` payload | 18 |
| Expansions | `vibra-workspace::query::expansions` | the `expansions` member | 25 |
| `edit rename` | `vibra-workspace::edit::rename` | `EditPlan` | 26 |
| `edit imports` | `vibra-workspace::edit::imports` | `EditPlan` | 27 |
| `edit fix`, fix capabilities | `vibra-workspace::edit::fix`, `vibra-diagnostics::registry` | `EditPlan`, `fixes[]` of a diagnostic | 28 |
| The index, `@index.v2` | `vibra-workspace::query::index`, `vibra-schema::index` | `@index.v2`, `urn:vibra:schema:v2:index` | 29 |
| `lint` | `vibra-workspace::edit::lint`, `vibra-cli::lint` | the `lint` payload | 30 |
| Generated reference data, the generator | `vibra-conformance::bin::reference` | `docs/reference/data/*` | 31 |
| The syntax card and checked examples | `vibra-conformance::bin::reference`, `docs/reference/` | `docs/reference/syntax-card.md` | 32 |
| The `query`, `eval`, `edit` case handlers | `vibra-conformance::{query,eval,edit}` | case expectations | 01, 05, 11, 13 |
| Output bound of every command, the exhausted envelope | `vibra-cli::render` | every envelope | 01 |

### The seams of step 01

M2's lesson is that parallel steps collide in shared files. Step 01 puts a
seam everywhere two M3 steps would otherwise edit one file, before any
feature lands, and changes no observable behaviour:

| Shared file | Seam |
| --- | --- |
| `crates/vibra-workspace/src/lib.rs` | `mod` lines and re-exports for `identity`, `approval`, `approve_write`, `present`, `eval`, `query`, `edit`; each new module is an empty file a later step owns |
| `crates/vibra-workspace/src/snapshot.rs` | one call, `approval::capture(root)`, that returns no record until step 10 |
| `crates/vibra-workspace/src/semantic.rs` | one call, `approval::admit(&snapshot, &check, &expectations)`, that returns no diagnostic and an empty approvals list until step 10; `check_snapshot` takes the `Expectations` of the command line |
| `crates/vibra-workspace/src/test_runner.rs` | one call, `approval::admit_test_run(&snapshot, selector, &check, &expectations)`, on the path that checks a selection (`check_selection`), returning no diagnostic and an empty list until step 10; `TestRun` carries the list |
| `crates/vibra-types/src/check/expr.rs`, `pattern.rs`, `declare.rs`, `mod.rs` | one call each into `check::facts`: an expression's fact with its dispatch inputs, a pattern's, a header's, and `finish_body(declaration, verdict)` where a body's checking ends; each records nothing until step 08; `CheckResult::facts()` and `CheckResult::checked_edges()` return empty tables |
| `crates/vibra-syntax/src/lib.rs`, `crates/vibra-fmt/src/lib.rs` | `mod position;`, `mod fragment;`, each an empty module |
| `crates/vibra-cli/src/lib.rs` | one dispatch arm per new command (`query`, `eval`, `approve`, `lint`, `edit body`, `edit rename`, `edit imports`, `edit fix`) into a module that answers `@command.unavailable` until its step lands; `edit/` is a directory with one module per subcommand and `write.rs` for the common `--write` path |
| `crates/vibra-cli/src/inventory.rs` | `supported` is read from each command module's `SUPPORTED` constant, so a step flips its own module only; `OPERATION_PROFILES` gains `eval` and `edit` |
| `crates/vibra-cli/src/args.rs` | the operands and options of every new command, `--expect-revision` and `--expect-approval` included, parsed into `Invocation` now, with `Invocation::expectations()`; an unavailable command's operands are not validated, as the chapter says |
| `crates/vibra-cli/src/check.rs`, `run.rs`, `test.rs`, `fmt.rs` | each passes `invocation.expectations()` to the engine's hook; no behaviour changes until step 10 |
| `crates/vibra-cli/src/render.rs` | the output bound of [Q28](spec-questions.md#q28-the-output-limit-of-every-command): the counting sink, `OUTPUT_LIMIT`, the limit name `outputBytes`, the exhausted envelope, and each payload's `exhausted()` form |
| `crates/vibra-schema/src/envelope.rs` | one `Payload` variant per new payload, each a `Box` of a struct in a module the command's step owns; the envelope field `approvals`, empty until step 10 |
| `crates/vibra-schema/schemas/v2/command-result.json` | the payload alternatives move to one file each under `schemas/v2/payloads/`, referenced by `$ref`, so a step edits its own payload schema; the `approvals` field |
| `crates/vibra-schema/schemas/v2/payloads/query.json` | the payload with one optional member per include and expansion, each a `$ref` to its own file under `schemas/v2/query/`, so the step that produces a member owns its schema and no two parallel steps edit one file |
| `crates/vibra-workspace/src/query/mod.rs` | the dispatcher: a table from each include and expansion name to a `fn(&WorkspaceCapture, &Resolved) -> Member` in the member's own module, each answering the unavailable status until its step; a step fills its module and flips its table entry |
| `crates/vibra-conformance/src/profile.rs`, `standard.rs`, `manifest.rs` | the operations `eval` and `edit` ([Q31](spec-questions.md#q31-case-operations-for-eval-and-edit)), the manifest keys of [Q30](spec-questions.md#q30-the-query-case-inputs-expectation-comparison) and Q31, and three handlers (`query`, `eval`, `edit`) that answer unavailable until their steps |
| every `tests/it/main.rs` of the tooling crates | the `mod` line of every test file a later step owns |

After step 01 no M3 step owns `lib.rs`, `snapshot.rs`, `semantic.rs`,
`expr.rs`, `pattern.rs`, `declare.rs`, `check/mod.rs`, `args.rs`,
`render.rs`, `check.rs`, `run.rs`, `envelope.rs`, `command-result.json`,
`payloads/query.json`, `query/mod.rs`, `profile.rs`, `standard.rs`,
`manifest.rs`, or a `main.rs`; `test_runner.rs`, `vibra-cli/src/test.rs`,
and the test payload are owned by step 18 after 01, and step 10 touches
none of them.

### Boundary types

These are proposals, not claims that the APIs exist. Two agents build the
two sides of each boundary against these shapes; a change to one is a change
to both.

**The publish primitive** (`vibra-fs-scope`, step 02):

```rust
impl ScopedFile {
    /// FlushFileBuffers on the handle; the data and the file's metadata are
    /// on the disk when it returns.
    pub fn flush(&mut self) -> Result<(), ScopeError>;
    /// Renames this file to `leaf` in `parent`'s held directory, replacing
    /// an existing entry, with POSIX semantics: handles others hold on the
    /// replaced file stay valid and read the old bytes; a later open of the
    /// name reads this file. SetFileInformationByHandle(FileRenameInfoEx)
    /// with RootDirectory = the held parent.
    pub fn rename_into(self, parent: &ScopedParent<'_>, leaf: &str) -> Result<(), ScopeError>;
}
impl ScopedParent<'_> {
    /// Removes the regular file `leaf` of the held directory, following no
    /// link (FileDispositionInfoEx, POSIX semantics).
    pub fn remove_file(&self, leaf: &str) -> Result<(), ScopeError>;
}
```

**Identity** (`vibra-workspace::identity`, steps 03, 04, 09):

```rust
// vibra-fmt/src/fragment.rs
/// The canonical source text of one node: the formatter's tokens, comments
/// dropped, laid out by the one rule of Q2.
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
pub struct ClaimRecord { /* the fields of @claim.v2 in order, `members` included (Q41) */ }
impl ClaimRecord {
    pub fn canonical_bytes(&self) -> String;
    pub fn identity(&self) -> String;   // sha256 of vibraClaimIdentityV2, 0, bytes
}
pub fn claim_of(declaration: &ResolvedDeclaration, check: &WorkspaceCheck,
                targets: &[TargetFact]) -> Result<ClaimRecord, IdentityError>;

// vibra-workspace/src/identity/witness.rs
pub struct Group { pub members: Vec<String>, pub dependencies: BTreeSet<usize> }
/// Strongly connected components of the union of the resolver's references
/// and the checker's checked edges, by an iterative Tarjan over an explicit
/// stack, in a deterministic order.
pub fn groups(resolution: &Resolution, edges: &[CheckedEdge]) -> Vec<Group>;
pub struct WitnessRecord { /* the fields of @witness.v2 in order */ }
```

**Facts** (`vibra-types::check::facts`, step 08):

```rust
pub struct Fact<T> { pub status: FactStatus, pub value: Option<T> }  // Exact | Recovered | Unavailable
pub struct Facts { nodes: BTreeMap<NodeKey, NodeFact>, edges: Vec<CheckedEdge>, verdicts: BTreeMap<String, BodyVerdict> }
pub struct NodeKey { pub source_id: Arc<str>, pub span: ByteSpan }
pub struct NodeFact {
    pub role: Fact<Role>,                 // the nine roles of the chapter
    pub context: Fact<Context>,           // the fifteen contexts
    pub expected: Fact<(Type, bool)>,     // the type and whether it was written
    pub observed: Fact<Type>,
    pub identity: Fact<Identity>,         // Declaration(id) | Local(binder:…); `_` has none
    pub application: Fact<ApplicationFact>,
    pub pattern: Fact<PatternFact>,
    pub enclosing: Option<String>,        // the declaration identity
}
pub struct CheckedEdge { pub from: String, pub to: String, pub kind: EdgeKind }
pub enum EdgeKind { ImplementationMember, DefaultMember, Conformance, GeneratedMember }
impl CheckResult {
    pub fn facts(&self) -> &Facts;
    pub fn checked_edges(&self) -> &[CheckedEdge];
}
```

A fact is recorded where the checker decides it, from the checked tree,
never from syntax alone: `expr.rs` records an expression's fact when its
type is final, with the inputs of its dispatch; `pattern.rs` a pattern's;
`declare.rs` a header's; `check/mod.rs` calls `finish_body` with the body's
final verdict, after which the body's effect facts are finalized and a
rejected body's rows are marked unavailable. Each field has its own
status: at a call that fails to type, the role and context are `exact`,
the result type `recovered`, and the dispatch `unavailable`. A query
projects these; it computes nothing.

**Query** (`vibra-workspace::query`, steps 05 to 17, 25, 29):

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
}
/// The dispatcher of step 01: one entry per include and expansion, filled
/// by the step that owns the member's module.
pub type MemberFn = fn(&WorkspaceCapture, &Resolved, &QueryOptions) -> Member;
pub fn query(capture: &WorkspaceCapture, subject: &Subject, options: &QueryOptions)
             -> Result<QueryResult, QueryRefusal>;   // invalid input, or the limit alone
```

**Edit** (`vibra-workspace::edit`, steps 07, 13, 15, 26 to 28, 30):

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
pub enum Operation { Format, Fix, Rename, Imports, Body }
pub struct BodyData { pub body: String, pub uses: Vec<String> }   // DATA of `edit body` (Q1)
pub fn plan_body(capture: &WorkspaceCapture, subject: &str, data: &BodyData) -> Planned;
pub fn plan_rename(capture: &WorkspaceCapture, subject: &str, new_name: &str) -> Planned;
pub fn plan_imports(capture: &WorkspaceCapture, path: &str) -> Planned;
pub fn plan_fix(capture: &WorkspaceCapture, subject: &str) -> Planned;
pub enum Planned { Plan(EditPlan), InvalidInput(String), Invalid(EditPlan, Diagnostic),  // @tool.invalidPlan
                   Denied(EditPlan, Vec<Diagnostic>) }                                   // @tool.actionDenied

// vibra-workspace/src/edit/apply.rs (step 15)
pub struct Expectations { pub revision: Option<String>, pub approval: Option<String> }
pub const WRITE_LOCK: &str = ".vibra-write-lock";
pub fn reserved_name(leaf: &str) -> String;   // ".<leaf>.vibra-publish"
/// The commit point: lock, capture again, compare the whole read set,
/// verify the destination's bytes, publish one document, unlock.
pub fn apply(workspace: &Path, plan: &EditPlan, expectations: &Expectations) -> Applied;
pub fn publish_source(root: &ScopedDir, snapshot: &SourceSnapshot, path: &str, bytes: &[u8]) -> Result<(), PublishFault>;
pub fn publish_approval(root: &ScopedDir, target: &str, bytes: &[u8]) -> Result<(), PublishFault>;
pub enum Applied {
    Applied { resulting_revision: String },
    Stale(Diagnostic),           // @tool.stalePlan, nothing written
    Unavailable(Diagnostic),     // @tool.unavailable: no adapter on this platform, or several documents
    Locked(Diagnostic),          // @project.ioError at the lock's path, never broken
    Failed(Diagnostic),          // @module.ioError or @project.ioError, nothing published
}
```

**Approval, presentation, and writing** (steps 10, 14, 16):

```rust
// vibra-workspace/src/approval.rs
pub struct Approval { /* the fields of @approval.v2 (Q7) */ }
impl Approval { pub fn identity(&self) -> String; }   // sha256 of the canonical bytes
pub struct ApprovalInForce { pub target: String, pub identity: String }
pub enum ApprovalState { None, Current(String), Stale(String, Vec<Diagnostic>) }
pub fn capture(root: &Path) -> Result<Vec<(String, Vec<u8>)>, Vec<Diagnostic>>;  // approvals/*.vibon
/// Q9 and Q40: validates every approval; with `expectations.approval`,
/// refuses first when the named approval is missing, another, or void.
pub fn admit(snapshot: &SourceSnapshot, check: &WorkspaceCheck, identities: &Identities,
             expectations: &Expectations) -> (Vec<Diagnostic>, Vec<ApprovalInForce>);
/// The same for a test run, on the path that checks a selection; with the
/// relation of the run's selection to each approval's (Q40).
pub fn admit_test_run(snapshot: &SourceSnapshot, selector: Option<&TestSelector>, check: &CheckResult,
                      expectations: &Expectations) -> (Vec<Diagnostic>, Vec<TestApproval>);
pub struct TestApproval { pub target: String, pub identity: String, pub selection: SelectionRelation }
pub enum SelectionRelation { Equal, Covers, Part, Other }
/// Every file the snapshot captures for the `@tests` unit is protected
/// while any approval is in force.
pub fn protects(snapshot: &SourceSnapshot, in_force: &[ApprovalInForce], source_id: &str) -> bool;

// vibra-workspace/src/present.rs (step 14)
pub struct Presentation { pub target: String, pub entry: Identity, pub claim: String,
                          pub bindings: Vec<PresentedBinding>, pub design_limits: [DesignLimit; 2],
                          pub approval: ApprovalState }
pub fn present(check: &WorkspaceCheck, identities: &Identities, target: &str) -> Result<Presentation, Refusal>;
pub fn approval_document(presentation: &Presentation, snapshot: &SourceSnapshot) -> Approval;

// vibra-workspace/src/approve_write.rs (step 16)
/// A renewal: checks the candidate with this target's approval diagnostics
/// set aside and every other error refusing; then publish_approval.
pub fn write_approval(workspace: &Path, target: &str, expectations: &Expectations) -> Written;
```

**Evaluation** (step 11):

```rust
// vibra-workspace/src/eval.rs
pub struct EvalRequest<'a> { pub subject: &'a str, pub arguments: &'a [String] }
pub enum EvalOutcome {
    Value { subject: Identity, document: String },           // @value.v2 text
    Unobservable { subject: Identity, error: ObservationError },
    Diagnostics(Vec<Diagnostic>),                            // refuted or unproved requires, check errors, limit
    InvalidInput(String),
}
pub fn eval(capture: &WorkspaceCapture, request: &EvalRequest<'_>, limits: &LimitProfile) -> EvalOutcome;
```

**The exhausted envelope** (`vibra-cli::render`, step 01):

```rust
pub const OUTPUT_LIMIT: u64 = 16 * 1024 * 1024;
/// Writes `envelope` into a sink that counts; past the bound it discards
/// what it wrote and emits `envelope.exhausted()`, whose payload is the
/// command's exhausted form (Q28): every list empty, every program-derived
/// string empty or null, the commit fields kept.
pub fn emit(out: &mut dyn Write, envelope: &Envelope) -> io::Result<()>;
```

### What changes in existing crates

| Crate | Change | Step |
| --- | --- | --- |
| `vibra-fs-scope` | `flush`, `rename_into`, `remove_file` on Windows; `Unsupported` elsewhere | 02 |
| `vibra-workspace` | the modules above; `snapshot.rs` captures approval records; `semantic.rs` admits under approvals and expectations; `test_runner.rs` admits a test run and takes a work budget beside the memory budget; `format_plan.rs` loses its own writer | 01, 10, 15, 18 |
| `vibra-types` | `check/facts.rs` and one call from each of four files; `CheckResult::facts`, `checked_edges` | 01, 08 |
| `vibra-interp` | `run_test` takes `Limits` (a work limit beside the memory budget) | 18 |
| `vibra-syntax` | `position.rs` | 01, 06 |
| `vibra-fmt` | `fragment.rs` | 01, 03 |
| `vibra-diagnostics` | fix capability `@safe` for two codes ([Q21](spec-questions.md#q21-which-codes-gain-a-safe-fix)); the test `no_code_is_registered_as_fixable` becomes the list of the two | 28 |
| `vibra-schema` | six payload modules, the query, index, and edit-plan schemas, the `$ref` layout of the command-result schema, the `approvals` field | 01, 05, 07, 11, 14, 29, 30 |
| `vibra-cli` | six command modules, `--expect-revision`, `--expect-approval`, the output bound, the inventory flips | 01 and each command's step |
| `vibra-conformance` | three handlers, two operations, the manifest keys, the `m3_coverage` audit, the reference generator | 01, 05, 11, 13, 31, 40 |

## 2. The identity and evidence argument

For every invariant the chapters state for M3: the layer that enforces it,
the layer that checks it again, the attack cases, and what would have to be
wrong for it to fail. An `ATK-` case is a host test; a `V2-` case is in the
corpus. The step in brackets owns the case. The theory each rests on is
named in the row; the sections after the table give the argument.

| # | Invariant | Enforced by | Re-validated by | Attack cases | It fails only if |
| --- | --- | --- | --- | --- | --- |
| I1 | A declaration's anchor identity is a function of its anchor alone: a body edit keeps it, a rename or move changes it, and so does every member's | `vibra-resolve::identity` (M1) | the claim and witness records carry the identity and the query reports it; a rename plan lists every changed identity in `postconditions` | `V2-TOOL-claim-body-edit-same` [04], `V2-TOOL-claim-rename-changes` [04], `V2-TOOL-edit-rename-postconditions-nominal` [26] | the anchor hashes a field that is not one of the six |
| I2 | A claim identity is a function of canonical content: comments, layout between tokens, the checkout's path, and the platform leave it; a signature, bound, default, guard, representation, implementation choice, an interface's member or associated type, or an entry's binding changes it | `identity::claim` over `fragment::canonical_text`, signature records, and recipe identities | the 05 vector is a host test; the index and the approval carry the identity and agree with a recomputation from the formatted tree | `V2-TOOL-claim-comment-same` [04], `V2-TOOL-claim-layout-same` [04], `V2-TOOL-claim-signature-changes` [04], `V2-TOOL-claim-vector` [04], `V2-TOOL-claim-interface-bound-changes` [04], `ATK-identity-crlf-same-claim` [04], `ATK-identity-path-independent` [04] | a non-canonical byte reaches the record, or a part of the contract has no field |
| I3 | A witness covers the bodies of one recursive group and the witnesses of what they resolve to or select, however far; an unrelated change leaves it; no witness depends on itself | `identity::witness`: strongly connected components over resolved references and checked edges, hashed as one unit | the 05 vector's framing is a host test; a change in a dependency changes every dependent's witness and no other; the group graph is checked acyclic after construction | `ATK-witness-vector-framing` [09], `V2-TOOL-witness-dependency-change` [09], `V2-TOOL-witness-selected-body-changes-caller` [09], `V2-TOOL-witness-checked-edge-cycle` [09], `V2-TOOL-witness-unrelated-same` [09], `V2-TOOL-witness-group-acyclic` [09], `ATK-witness-no-self-dependency` [09] | an edge is missed, a choice is not exported, or a component is split |
| I4 | The workspace revision hashes exact input bytes, so a change of line endings is a new revision while the claims are not | `vibra-workspace::revision` (M1) | `revisions.ps1 -Check` recomputes every expected revision from the case bytes with no engine code | `V2-TOOL-revision-crlf-differs` [04] | the snapshot normalizes bytes |
| I5 | A query answers from one captured revision, names it, and rereads no disk; a captured snapshot that reached a limit still names its revision | `WorkspaceCapture` is taken once; `query` takes the capture and no path | an architecture test lists every read of the filesystem in `vibra-workspace` and allows discovery, capture, and approvals | `V2-TOOL-query-revision-named` [05], `V2-TOOL-query-nesting-limit` [05], `ATK-engine-reads-enumerated` [05] | a projection opens a file |
| I6 | A read-only command writes nothing and there is no store: after `check`, `query`, `eval`, `test`, and every preview the tree is byte-for-byte equal, directory listing included, and no derived file exists whose deletion changes source, approvals, identity, or execution | capture has no side effect: no replay, no cache, no lock on a read; the engine's only writers are the publisher and the lock | the write-site architecture test allows the publisher and the lock only; a host test compares a recursive listing and every byte before and after each read-only command | `ATK-read-only-tree-unchanged` [15], `ATK-edit-writers-enumerated` [15] | a command reads back a file the engine wrote, or a reader takes the lock |
| I7 | A plan applies only against the revision it was planned for, validated over the whole read set at the commit point: a stale one is `@tool.stalePlan` with nothing written; of two writers planned from one revision exactly one applies | `edit::apply`: the caller's `--expect-revision` against the first capture; the exclusive write lock; a second capture whose workspace revision must equal the plan's; the destination's bytes against the plan's document revision and each edit's fingerprint, read through the handle | the lock is created exclusively through the held root and never broken; the result names the validated revision | `ATK-edit-apply-expect-revision` [15], `ATK-edit-apply-stale-document` [15], `ATK-edit-apply-unedited-input-changed` [15], `ATK-edit-apply-two-writers-one-revision` [15], `ATK-edit-apply-fingerprint-mismatch` [15], `ATK-edit-apply-lock-present` [15] | an input is validated before the lock, or the lock is taken by only some writers |
| I8 | One document is published so that its name never refers to a partial document; a plan over several documents is refused before writing; an interrupted application is never reported applied | the publish primitive of step 02: create under the reserved name, write, flush, rename over the destination relative to the held directory with replace and POSIX semantics; `apply` refuses a plan with two documents as `@tool.unavailable` | a failure injected before the rename leaves the destination unchanged; a reader holding the destination open keeps the old bytes; `applied` is set only after the rename returns | `ATK-publish-failure-before-rename` [02], `ATK-publish-reader-keeps-old-bytes` [02], `ATK-publish-read-only-destination` [02], `ATK-edit-apply-several-documents-unavailable` [15] | bytes are written under the destination's name, or the platform assumption of section 3 fails |
| I9 | The engine writes only through the publisher, under the workspace root, through the held root directory, one real name at a time, following no link, accepting no alias, refusing a destination with more than one hard link | `publish_source` and `publish_approval` open the root once with `vibra-fs-scope` and reach the destination's directory one name at a time; the reserved file is created, written, renamed, and removed relative to that held directory | the write-site architecture test; the attack rows read the tree after a refusal | `ATK-publish-symlink-refused` [02], `ATK-publish-alias-refused` [02], `ATK-publish-hard-link-refused` [02], `ATK-edit-apply-dotdot-refused` [15], `ATK-edit-writers-enumerated` [15] | a path is joined and reopened by name |
| I10 | A source publication reaches the modules of the root project's own units only, decided from the module list of the snapshot captured at the commit point, never from the plan's path; `project.vibon`, the lock, a dependency, and `approvals/` are not source documents; an approval is written by its own entry point to the one destination its target's name derives | `edit::plan`: the subject's unit is a target or `@tests` of the root project; `publish_source`: the destination is a module of that snapshot; `publish_approval`: the path is computed from the target | a plan whose document is no module of the recaptured snapshot is refused at the commit point | `V2-TOOL-edit-dependency-subject-invalid` [13], `V2-TOOL-edit-std-subject-invalid` [13], `ATK-edit-apply-dependency-refused` [15], `ATK-edit-apply-destination-not-a-module` [15] | a destination path escapes the module check |
| I11 | A rename is shown as an identity-affecting change; a body edit changes witnesses only | `postconditions` are computed by recomputing every identity of the local package on the staged snapshot and listing each difference by kind | the index of the resulting snapshot agrees with `postconditions[].after` | `V2-TOOL-edit-rename-postconditions-nominal` [26], `V2-TOOL-edit-rename-claim-of-callers` [26], `V2-TOOL-edit-body-postconditions-witness-only` [13] | identities are compared by spelling, not recomputed |
| I12 | An approval names what it approved and is void when a protected file, the entry's claim, a binding, the selection, or the toolchain changes | `approval::admit` at every `check`, and `admit_test_run` on the path every test run takes | the digests are recomputed from the snapshot's bytes, never read from a cache; `test`, `run`, and every edit refuse through the check's diagnostics | `V2-GRANT-approval-current` [10], `V2-GRANT-approval-stale-test` [10], `V2-GRANT-approval-stale-helper` [10], `V2-GRANT-approval-stale-claim` [10], `V2-GRANT-approval-stale-selection` [10], `V2-GRANT-approval-stale-toolchain` [10], `V2-GRANT-approval-unapproved-binding` [10], `V2-RUNTIME-run-stale-approval-refused` [10] | a command checks the snapshot or a selection without admitting under approvals |
| I13 | The protected evidence is the `@tests` unit, whole: while an approval is in force no file of it can be changed by the tool, while the code under repair can; two targets' approvals over one `@tests` protect it once | `edit::plan` refuses a plan with a document `approval::protects` as `@tool.actionDenied`; `fmt --write` and `lint --write` go through the same check | a staged plan that reaches the publisher with a protected document is refused again at the commit point | `V2-TOOL-edit-body-test-denied` [13], `V2-TOOL-edit-body-under-approval-allowed` [13], `V2-GRANT-approval-two-targets-shared-tests` [10], `V2-TOOL-edit-rename-into-tests-denied` [26], `ATK-edit-apply-protected-unchanged` [15], `ATK-fmt-write-protected-denied` [15] | a document is added to a plan after the check |
| I14 | A tool grants no authority and an observation restores none: `approve` acquires nothing, a query's authority projection is data, an approval document is data | `present` reads the `BindingPlan` and never calls `admit` or `start`; `query::effects` builds projections from recipes; the approval decoder yields a record | no `Context` is constructed outside `run` and the test runner, which the M2 architecture tests already pin | `ATK-approve-no-acquisition` [14], `ATK-query-authority-is-data` [17], `ATK-approval-no-signature-is-data` [10] | a presentation constructs a broker |
| I15 | Evaluation runs no procedure or plan, makes no grant, and uses no ambient authority: the subject is a `fn`, the interpreter has no host context, each call runs against a fresh snapshot | `eval`: the subject's kind is `@fn`; `vibra_interp::evaluate` with no `HostContext`, as compile-time evaluation runs | the ambient-reads architecture test already allows host APIs under `vibra-host/src/system/` only | `V2-TOOL-eval-proc-subject-invalid` [11], `V2-TOOL-eval-plan-built-not-run` [11], `ATK-eval-no-host-context` [11], `V2-TOOL-eval-fresh-snapshot` [11] | a subject with a `proc` kind passes the kind check |
| I16 | Every evaluation and every test item ends within a stated work and memory limit with an incomplete outcome, never a program value; an operation of the test world is charged deterministically | `eval` under `LimitProfile::M1_DEFAULT` reductions and the default memory budget; the test runner's per-item reduction budget ([Q27](spec-questions.md#q27-a-work-limit-for-a-test-item)), passed to the interpreter's test entry | `limits.reached` names each limit; the other items of a suite keep their results; the suite's verdict follows the chapter's precedence | `V2-TOOL-eval-reductions-limit` [11], `V2-TOOL-eval-memory-exhausted` [11], `V2-PROJECT-test-reductions-limit` [18], `V2-PROJECT-test-reductions-other-items-run` [18], `V2-PROJECT-test-world-operations-charged` [18] | a limit is checked after the work, or the test entry is built with no bound |
| I17 | Every command's output is bounded: when the bound is reached the command emits the frozen exhausted envelope, a valid document that fits by construction and states what was committed, never truncated JSON | `render::emit` writes into a counting sink and, at the bound, emits `Envelope::exhausted()` ([Q28](spec-questions.md#q28-the-output-limit-of-every-command)) | every exhausted payload is a fixed-size document; a host test validates each command's | `ATK-output-bound-oversized-scalar` [01], `ATK-output-bound-oversized-item` [01], `ATK-output-bound-applied-edit` [15], `V2-TOOL-query-index-bounded` [29] | an exhausted payload carries a field that grows with the input |
| I18 | A structural fact never implies the source checked; each semantic field is `exact`, `recovered`, or `unavailable` on its own, established by the layer that decides it; a complete fact stays `exact` beside an unrelated broken module; a rejected body shows no exact rows | `check::facts` records a status per field; `finish_body` finalizes effects after the verdict; `query::semantic` projects them | a position inside recovery, a node with mixed statuses, and a position in a sound module of a broken workspace are rows | `V2-TOOL-query-position-in-recovery` [06], `V2-TOOL-query-semantic-beside-broken-module` [12], `V2-TOOL-query-mixed-status-node` [12], `V2-TOOL-query-rejected-body-no-rows` [17] | a node carries one status for every field, or an effect fact is recorded before the body's verdict |
| I19 | A position offset is on a character boundary, at most the end of the file; the smallest containing node wins, a zero-width recovery node first, the deepest then source order | `vibra-syntax::position` | the structural record's `span` contains the offset, or equals it at the end | `V2-TOOL-query-position-boundary` [06], `V2-TOOL-query-position-end-of-file` [06], `V2-TOOL-query-position-inside-scalar` [06], `V2-TOOL-query-position-past-end` [06] | the search uses closed spans |
| I20 | A target's authority is presented from the project file alone: every rule of every policy, every limit, every startup read, and the two design limits, and nothing a run would learn | `present` over the `BindingPlan` and the normalized policies | the presentation's recipe identities equal those of the startup record of a run of the same project | `ATK-approve-presents-every-rule` [14], `ATK-approve-design-limits-shown` [14], `ATK-approve-from-project-alone` [14] | the presentation reads the environment |
| I21 | A changed binding under an approval is `@grant.invalidBinding` at the recipe, once; every other voiding is `@project.staleApproval` once; one fault is reported once | `approval::admit` in the order of [Q9](spec-questions.md#q9-the-code-of-a-void-approval) | the corpus rows of each voiding have exactly one error | `V2-GRANT-approval-unapproved-binding` [10], `V2-GRANT-approval-one-fault` [10] | both checks run on one fault |
| I22 | Every machine output validates against its schema, on the denied, stale, malformed, and exhausted paths included | `Envelope::to_validated_json` before anything is written | a host test drives every command through every ending and validates each envelope | `ATK-schema-every-ending` [40] | a payload is rendered outside the envelope |
| I23 | Every new persistent document decodes its example and round-trips to its canonical bytes, and the hash vectors of 05 hold over their placeholder records | the decoders of `@approval.v2`, `@index.v2`, the plan schema; the framing tests of the two vectors | the `spec_examples` test of `vibra-conformance` reads the chapter examples | `V2-GRANT-approval-round-trip` [10], `V2-TOOL-index-round-trip` [29], `V2-TOOL-edit-plan-schema` [07], `ATK-witness-vector-framing` [09] | a field is written out of schema order |
| I24 | The generated references equal the implementation: a drift fails a test, and every checked example gives its registered verdict | the generator reads the reader's tables, the registry, the inventory, and the schemas | the committed files are regenerated and compared by a test; the examples are run through the engine | `ATK-reference-drift-detected` [31], `ATK-reference-example-verdicts` [32] | a file is edited by hand and the test is not run |
| I25 | The repair scenario's evidence is registered before it is gathered: the seed is sealed, the runner sees machine-mode CLI only through a wrapper that admits commands by shape, the pass rule grades the loop, and the controls are frozen first | the registration of step 35, the wrapper and controls of step 36 | the record of step 39 names the freeze commit and every command the runner issued, and compares every protected byte with the frozen baseline | `ATK-repair-wrapper-refuses-other-commands` [36], `ATK-repair-wrapper-pins-approval` [36], `ATK-repair-seed-sealed` [37] | a choice is made after a result is seen, or the grade reads the end state alone |
| I26 | An approval is checked against a pin the writer of the tree cannot reach: every envelope names the approvals in force, and with `--expect-approval` a missing, replaced, or void approval refuses the command before anything else | `approval::admit` and `admit_test_run` under the command line's `Expectations` ([Q40](spec-questions.md#q40-the-approval-pin-and---expect-approval)) | the repair wrapper supplies the pinned identity on every command; the grader compares the approval's bytes with the baseline | `ATK-approval-expected-current` [10], `ATK-approval-expected-missing` [10], `ATK-approval-expected-replaced` [10], `ATK-approval-expected-absent-without-option` [10], `ATK-approval-envelope-names-identity` [10], `ATK-repair-wrapper-pins-approval` [36] | a command path checks the snapshot without the expectations |
| I27 | A test result says which approval it ran under and whether its selection equals, covers, or is a part of the approved selection, so a narrower rerun is never mistaken for the acceptance run; a selected and a whole-suite run are both admitted under approvals | `admit_test_run` on the `check_selection` path | the `test` payload's `approvals`; the repair grader reads the relation of the final rerun | `ATK-test-selection-equal` [10], `ATK-test-selection-covers` [10], `ATK-test-selection-part` [10], `ATK-test-selection-other` [10], `ATK-test-selected-run-admitted` [10], `ATK-test-whole-suite-admitted` [10] | a test run reaches the runner without the hook |
| I28 | A body edit changes no claim and no resolution outside its subject: the header, the guards, and every other declaration are unchanged by construction, and the engine compares every claim and every resolution before and after on the formatted candidate, refusing a difference as an invalid plan | `edit::body`: the body section alone is replaced; the listed `use`s are inserted; the comparison runs on the exact formatted candidate | the postconditions of a body edit hold `@witness` entries only; the comparison is a second check of the construction | `V2-TOOL-edit-body-claims-unchanged` [13], `V2-TOOL-edit-body-use-collides` [13], `V2-TOOL-edit-body-closes-early-invalid` [13], `ATK-edit-body-resolution-guard` [13] | the body text is spliced without a parse of its own |

### The theories

- **Content addressing and Merkle structure (I1 to I4).** An identity is the
  hash of canonical content and of the identities of what it depends on.
  The anchor hashes six fields; the claim hashes a record whose every
  reference is an identity; the witness hashes a record whose
  `dependencies` are witnesses. Because the groups are the strongly
  connected components of the reference graph (source references and
  checked edges together), the dependency graph of witnesses is a directed
  acyclic graph by construction, and a witness is computed by one pass in
  reverse topological order with an explicit stack. A hash of a normal form
  identifies programs up to the congruence the normal form decides: it is
  sound (equal identity, equal meaning) and not complete, so the contract
  promises exactly the equivalences of the normal form
  ([Q6](spec-questions.md#q6-what-each-hash-survives)) and no other. The
  workspace revision is the one deliberate exception: it hashes input bytes
  because it is the identity of the snapshot, not of a meaning.
- **Optimistic concurrency with validation at commit (I7).** A writer
  states the version it read (`--expect-revision`, the plan's document
  revisions, each edit's fingerprint); at the commit point it takes the
  exclusive write lock, captures the workspace again, and compares the
  whole read set, which the single workspace revision decides in one
  comparison; a mismatch aborts without writing. The lock serializes the
  commit points of every vibra writer, so of two writers planned from one
  revision exactly one commits and the other is stale. An editor or another
  tool is outside the lock; against those the window is the interval
  between the second capture and the rename, and the result names the
  revision it validated.
- **Atomic replacement by rename (I8).** The destination's name refers to
  the old document until one rename makes it refer to the new one; partial
  bytes exist only under the reserved name. "All or none" for one document
  is a property of names, not of bytes, so it holds by construction. For
  several documents no such construction exists on the platform, and the
  chapter's sentence decides: refuse before writing.
- **Confinement to the opened object (I9, I10).** The same rule the drivers
  of M2 follow: authority is a held directory handle, a request is resolved
  one real name at a time from it, and nothing is reopened by path. The
  destination is decided from the recaptured snapshot's own module list,
  so a plan's path never names what the publisher writes.
- **Capability presentation and the reference monitor (I12 to I14, I20,
  I21, I26, I27).** An approval is a statement about exactly the authority
  the project file describes, so it names the identities of the recipes
  and the claim of the entry, and the presentation that precedes it is
  computed from the same data admission reads. Neither presents or stores a
  grant, so neither can leak one. An approval inside the tree shows only
  that the tree agrees with itself; integrity needs a reference the subject
  cannot write (the reference-monitor argument; separation of duty in the
  Clark–Wilson model), which is the pin outside the tree that
  `--expect-approval` checks against and every envelope reports.
- **Pure evaluation (I15, I16).** The interpreter without a host context is
  the compile-time evaluator the checker already uses; `eval` adds an
  argument decoder and a limit profile and nothing else; the test entry
  gains the work limit the chapter already demands of every executing
  command.
- **Facts per field (I18).** A fact's status is decided by the layer that
  establishes the fact: the grammar slot gives an exact role while the
  checker's failure leaves the type recovered and the dispatch unavailable.
  One status per node could not say this, and effects can only be final
  after the body's verdict.
- **A body edit as a typed operation (I28).** The operation's type decides
  what it can change: a body and nothing else. The comparison of claims and
  resolutions is the check of the construction, not the definition of the
  operation.

### What the argument does not cover

- **Semantic equivalence of policies.** Two policies with one meaning may
  hash differently; an approval of one is not an approval of the other.
  The chapter says so of the policy hash, and the approval inherits it.
- **The approver's identity.** An approval document carries no signature;
  whoever may write the tree may approve. Version control is the record,
  and the pin of [Q40](spec-questions.md#q40-the-approval-pin-and---expect-approval)
  is what a run that must not trust the tree checks against.
- **Durability and atomicity of the rename.** Microsoft's documentation of
  `FILE_RENAME_INFO`, `MoveFileExW`, `ReplaceFileW`, `FlushFileBuffers`,
  and `SetFileInformationByHandle` calls no replacement by rename atomic
  and says nothing of a directory's metadata reaching the disk. Section 3
  records what the release assumes.
- **Writers outside the lock.** An editor or another tool that changes a
  file between the second capture and the rename is not detected.
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
protected document, an overlap, or a body edit's mismatch never reaches the
publisher.

**One document, by construction.** The publisher opens the workspace root
once as a held directory and reaches the destination's directory from it
one real name at a time. It creates the reserved file
`.<leaf>.vibra-publish` in that directory exclusively (`create_new`
relative to the held directory), writes every byte, flushes it, then
renames it over the destination relative to the same held directory handle
with replace-if-exists and POSIX semantics. The contract has three parts,
kept apart:

1. **By construction.** The destination's name never refers to a partial
   document: partial bytes exist only under the reserved name, and the
   rename is the only step that changes what the destination's name
   refers to. A failure before the rename leaves the destination's bytes
   unchanged. `applied` is reported `true` only after the rename returns.
2. **What Microsoft documents.** `FILE_RENAME_INFO.RootDirectory`: "If
   FileName specifies a relative name, this field can be a handle to the
   directory relative to which the new name is resolved."
   `FILE_RENAME_POSIX_SEMANTICS` with replace-if-exists: "allow replacing
   a file even if there are existing handles to it. Existing handles to the
   replaced file continue to be valid… Any subsequent opens of the target
   name will open the renamed file, not the replaced file." Without it, "A
   file cannot be renamed if a file with the same name exists and has open
   handles", and a read-only or executing target fails even with replace.
   `ReplaceFileW` is not used: `REPLACEFILE_WRITE_THROUGH` "is not
   supported", and two of its errors leave the replaced file under another
   name or gone. No page calls any replacement by rename atomic, and
   `FlushFileBuffers` says nothing about a directory handle or metadata.
3. **The platform assumption the release rests on.** NTFS logs a rename as
   one metadata transaction, so after a crash the destination's name
   refers to the old document or to the new one and never to neither. The
   release records this as an assumption, here and in the exit record; it
   is not a guarantee the documentation gives.

The primitive keeps the M2 drivers' rules: no link is followed on the way
to the directory, the real name of every component is checked against the
request, and a destination with more than one hard link is refused before
the reserved file is created, as the hard-link limit of the effects chapter
demands of a replacement. A destination that is read-only is a typed
refusal (`@module.ioError` at offset 0 of the module, result
`@command.operationalFailure`) with nothing changed, found when the
destination is opened before the reserved file is created. A reader
holding the destination open across a publish keeps reading the old bytes;
a new open reads the new ones.

**A leftover.** A reserved file left by a crash before the rename is not a
module (discovery admits `.vib` files only, and the lock is at the root), so
no read-only command sees it and none removes it: a read-only command
writes nothing. The next publisher of the same destination, holding the
write lock, finds `create_new` refused with `alreadyExists`, removes the
reserved file through the held directory (its bytes are partial by
construction and no name of a module refers to them), and creates it again
once. A reserved file of another destination is left alone until that
destination is published.

**Several documents.** A plan over two or more documents is computed,
checked, and returned in full as a preview; `--write` on it is refused
before anything is written, as `@tool.unavailable` with the command-line
context and the result `@command.unavailable`
([Q15](spec-questions.md#q15-several-documents-cannot-be-published-atomically-on-windows)).
A rename confined to one module still writes. The inventory and the exit
record list a `--write` over several documents under what stays
unavailable.

**The commit point.** A write validates the whole read set, not only the
written document. The publisher: takes the exclusive write lock
`.vibra-write-lock` at the root (created exclusively through the held root,
holding the process id and the command, removed when the command ends);
captures the workspace again; refuses as `@tool.stalePlan` unless the second
capture's workspace revision equals the plan's, which compares every
admitted input, the edited document and every unedited one; opens the
destination and checks its bytes against the plan's document revision and
each edit's fingerprint; publishes; removes the lock. A lock that is present
when a writer arrives is never broken: the command is
`@command.operationalFailure` with `@project.ioError` at the lock's path,
and the human format names the file for a person to remove after making
sure no vibra command runs. The lock governs every vibra writer: `fmt
--write`, every `edit … --write`, `lint --write`, and `approve --write`. It
cannot govern an editor, a version-control checkout, or another tool;
against those the window is narrowed to the interval between the second
capture and the rename, and the result names the revision it validated:
the plan's `workspaceRevision`, which `resultingRevision` follows. A
read-only command takes no lock.

**Confinement.** A source publication admits one destination: a module of
the root project's targets or of `@tests`, decided from the module list of
the snapshot captured at the commit point, never from the plan's path.
`project.vibon`, the lock file, a dependency's modules, and anything under
`approvals/` are not source documents. Writing an approval is a separate
typed entry point, `publish_approval`, whose destination
`approvals/<target>.vibon` is derived from the target's name alone; it
never creates the directory. An architecture test enumerates every call in
`vibra-workspace` that creates, writes, renames, or removes a file and
allows the publisher's two entry points and the lock only. The engine never
removes a file except a reserved file and the lock, never renames one
except the reserved file over its destination, and never creates a
directory.

**Platform-specific.** The scoped adapter and the primitive exist on
Windows. Elsewhere `--write` is `@tool.unavailable`
([Q16](spec-questions.md#q16-applying-an-edit-off-windows)), the preview is
still returned, and the apply rows print `NOT RUN`. The corpus holds
previews only and is the same on three platforms.

## 4. The conformance corpus of M3

| Profile | Operation | What a case states |
| --- | --- | --- |
| `toolingV2` | `query` | a tree, a `subject`, `include` and `expand` lists; the expected payload as JSON (`expect.query`), or an `@index.v2` document (`expect.index`), or the verdict and diagnostics of a refusal |
| `toolingV2` | `edit` (new, [Q31](spec-questions.md#q31-case-operations-for-eval-and-edit)) | a tree, the edit kind (`body`, `rename`, `imports`, `fix`) and its operands; the expected plan as JSON (`expect.editPlan`), with `resultingDiagnostics` and `postconditions`, or the diagnostics of a denial or an invalid plan |
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
  did; the two vectors of 05 are framing tests over their placeholder
  records, since the vectors' lists are not those of a real program.
- **Attack cases.** An attack the corpus can express (a stale approval, a
  denied edit, an invalid subject, a limit reached) is a case. One that
  needs a real filesystem (a link, an alias, a hard link, a failure before
  the rename), a second process, a command-line option (`--expect-approval`,
  `--expect-revision`, `--write`), or a forged value is a host test
  (`ATK-`) in the crate that owns the layer, listed in section 2.
- **Denied, stale, malformed, exhausted.** Each path has a case or host
  test whose expectation includes the envelope's schema validation: the
  CLI tests of each command validate every envelope, and
  `ATK-schema-every-ending` drives every command through every result of
  the result table.
- **One fault.** A negative case has exactly one error. A row a guide
  cannot write with one fault from forms that exist at that step is a
  finding against the guide.

## 5. What M2 deferred to M3

| Item | Placed |
| --- | --- |
| `eval` | step 11 |
| `query` | steps 05, 06, 12, 17, 25, 29 |
| `lint` | step 30 ([Q22](spec-questions.md#q22-what-lint-is)) |
| the command that presents a target's authority before a run (F8) | steps 14 and 16, as `approve` ([Q8](spec-questions.md#q8-the-command-that-presents-authority)) |
| the names of reached tool limits | fix package E names the two of M2; steps 11, 18, and 01 add `evaluationReductions`, `testReductions`, and `outputBytes` under the same convention ([Q26](spec-questions.md#q26-the-limits-of-eval-and-their-names) to [Q28](spec-questions.md#q28-the-output-limit-of-every-command)); steps 11 and 18 depend on fix E having landed |
| `project init`, `inspect`, `add`, `remove`, `sync` | out of scope: no milestone of the release names them, and M3's scope does not. `inspect` is largely `query @workspace`; `add` and `remove` reuse the edit plan of step 07 when a milestone owns them. They stay `@tool.unavailable`; the owner table's finding stands |
| the contract call on an interface's type parameter whose bound leaves an associated type open | out of scope: a checker rule of the pure language with no M3 mechanism behind it; it stays `@tool.unavailable` and `V2-PROJECT-test-unavailable-closure` stands. A fix package of the checker owns it |
| the slow checks of the M2 exit | out of scope: the checker's work is bounded by its input alone in this release ([Q29](spec-questions.md#q29-a-time-limit)); M3 bounds what it adds. The exit record repeats the gap |
| the `effectWitnesses` expansion (owner table finding) | step 17 |
| `@tool.stalePlan` | step 15 |
| `@tool.actionDenied` | steps 13, 15, 26, 27, 28, 30 under [Q10](spec-questions.md#q10-the-code-of-an-edit-the-approval-forbids); without that ruling it stays with MCP, which is not in this release |

## 6. Risks

1. **The repair edit is not in the chapter.** Without
   [Q1](spec-questions.md#q1-no-supported-edit-changes-a-body) the release
   gate cannot be run through the CLI. Mitigation: Q1 is the first question
   and is marked for Hannah; steps 13 and 15 are written against it; if it
   is overturned the plan's edit lane shrinks to rename, imports, and fix,
   and the gate's edit becomes a question of the roadmap.
2. **The approval has no specification.** Q7 to Q11 and Q40 define it.
   Mitigation: the schema is small and mirrors the lock file; step 10 lands
   before any command depends on it; review gate 19 reads the approval
   code against the chapter sentences that exist.
3. **The publisher rests on an assumption about NTFS.** The documentation
   does not call the rename atomic. Mitigation: the three parts of the
   contract are kept apart in section 3 and in the exit record; the rows
   of step 02 show what holds by construction (a failure before the rename
   leaves the destination unchanged) with failure injection at every point
   before it; a plan over several documents is refused, so the assumption
   is never stacked.
4. **A leftover lock stops every writer.** Mitigation: the lock names its
   holder and the command; it is never broken silently, and the human
   format tells a person what to remove; the lock is created and removed
   by one function the architecture test pins.
5. **Facts per node change the checker's hot path.** Recording a fact for
   every node, with a status per field, costs memory and may slow `check`.
   Mitigation: facts are recorded into a `BTreeMap` behind one hook,
   measured by a host test on the largest corpus trees (less than one fifth
   more time and memory), and step 08 may record lazily per module if the
   measurement fails.
6. **Checked edges change the witness graph.** A missed export leaves a
   selected body out of a caller's witness. Mitigation: the export is in
   the step that owns the checker's facts; step 09 has a row per edge kind
   and the cycle row; gate 19 probes a selected body edit.
7. **Collisions in the CLI and schema crates.** Six new commands and
   payloads land in parallel. Mitigation: the seams of step 01, one module
   and one payload schema file per command, one module and one schema file
   per query member, `supported` read from the module.
8. **Canonical text drifts from the formatter.** Two definitions of
   canonical spacing would make the 05 vectors fail. Mitigation: step 03
   derives the text from the formatter's token stream, not from a second
   printer, and the vectors' framing tests are its tests.
9. **The exhausted envelope hides a result.** Mitigation: it is emitted
   only when the bound is reached, says so with `complete: false`, and
   states what was committed; a caller narrows the query.
10. **Two new case operations.** If Q31 is overturned, every `eval` and
    `edit` case becomes a `vibra-cli` host test. Mitigation: the rows are
    written as cases with the question marked; the handler and the host
    test share the fixture trees, so moving a row changes no expectation.
11. **The repair run depends on a model.** Mitigation: the protocol is
    frozen first (step 35), the controls pass first (step 36), the budget
    is fixed, the grade reads the runner's log and the frozen baseline, and
    the record says what the run does not show.
12. **Memory on the build machine.** Mitigation: no guide runs the
    whole-workspace test suite; each names the packages it touches and
    their dependents; the reference generator and the index tests use the
    smallest trees that reach their rows.
