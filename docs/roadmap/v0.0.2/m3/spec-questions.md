# v0.0.2 M3: specification questions

Each question is a place where the specification is silent, says two things,
or decides an observable behaviour by a sentence that can be read two ways.
A step guide never settles one by choosing. A question is in one of two
states:

- **Ruled.** Its entry carries a **Ruling**, and the rows under
  **Depends** are written against it.
- **Open.** Its entry carries a **recommended answer**; the rows under
  **Depends** are written against it and name the question, so a
  different ruling changes exactly those rows.

Every question below is open. A question marked **Hannah** is language
design, the security model, or the shape of a published format or command
surface, and waits for the owner; the others are proposed rulings the
integrator may confirm. Each recommendation is written so that it can be
accepted or overturned in one line.

Chapters: [01 source](../../../spec/01-source-language.md),
[02 types](../../../spec/02-type-system.md),
[03 effects](../../../spec/03-effects.md),
[04 programs](../../../spec/04-programs-and-packages.md),
[05 tooling](../../../spec/05-tooling.md),
[06 runtime](../../../spec/06-runtime.md),
[07 diagnostics](../../../spec/07-diagnostics-and-conformance.md).

## The repair edit

### Q1. No supported edit changes a body

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans),
  [05 Command surface](../../../spec/05-tooling.md#command-surface),
  [v0.0.2 release gate](../README.md#release-gate), step 3.
- **Question.** The supported edits are canonical format, safe diagnostic
  fix, rename, and organize imports; "free text replacement" is not a
  compiler operation. The release gate asks an agent to repair a seeded
  defect through machine-mode CLI alone, with "a revision-checked edit".
  None of the four edits changes a body, so no supported command repairs a
  wrong constant or an off-by-one.
- **Command.** `vibra --format json edit fix @gate.tally.count`, where the
  defect is a wrong literal and no diagnostic points at it.
- **Recommended answer.** **Hannah.** Add one typed operation to the surface
  and to the plan's `operation` set: `edit replace SUBJECT DATA [--write]`.
  `SUBJECT` is the canonical entity atom of a top-level declaration or a
  `fn` or `proc` member of a local unit; `DATA` is one canonical VIBON `Str`
  whose value is source text holding zero or more `use` declarations
  followed by exactly one declaration of the same keyword form and name as
  the subject. The engine parses the text as a module fragment, replaces the
  subject's whole span with the new declaration laid out by the formatter,
  inserts each `use` the module lacks into its import block, checks the
  affected modules again, and reports the identity changes in
  `postconditions`. A different name or form is invalid input: a rename is
  `edit rename`. This is a typed operation on a target identity with a
  typed record, which is what the chapter asks of every edit; it is not a
  patch language, because the unit of change is one declaration and the
  result must parse, check, and format. The theory is the same as for
  `project add`: the record is the complete typed value of the thing
  replaced. The alternative, letting the agent write files directly and
  then `check`, has no revision check and no protected evidence.
- **Depends.** Steps 11, 14, 27, 31, 34. Rows `V2-TOOL-edit-replace-*`,
  `ATK-edit-replace-other-name`, `ATK-edit-replace-test-protected`.

### Q2. Canonical source text

- **Anchor.** [05 Snapshot, claim and witness identity](../../../spec/05-tooling.md#snapshot-claim-and-witness-identity),
  [01 Canonical format](../../../spec/01-source-language.md#canonical-format).
- **Question.** A claim records "the canonical source text" of a default
  and of a guard, and a witness "the canonical source text of the whole
  declaration". The witness vector writes
  `proc main returns code: ExitCode do return ExitCode(0u8) endproc` on one
  line, while the formatter lays a `proc` out over several lines. Which text
  is canonical?
- **Program.** The vector's `proc main`, written with a comment, a doc
  string, and the sections on separate lines.
- **Recommended answer.** **Hannah.** The canonical source text of a node is
  the formatter's token sequence for that node, with every comment dropped
  and the layout replaced by the spacing rule of
  [canonical bytes](../../../spec/04-programs-and-packages.md#decoded-values-and-canonical-bytes):
  one space between adjacent tokens, none after an opener or before a
  closer of `(`, `[`, `{`, none before `:` and one after it, so that
  `code: ExitCode` and `ExitCode(0u8)` read as the vector shows. A doc
  string is a token and stays (it is part of the public surface); a
  comment is not. The text ends at the node's closer with no newline. The
  theory is the one canonical serialization already has: a hash covers
  decoded structure, never layout.
- **Depends.** Steps 02, 03, 04. Rows `V2-TOOL-claim-comment-same`,
  `V2-TOOL-claim-layout-same`, `V2-TOOL-witness-vector`,
  `V2-TOOL-claim-vector`, and every row of step 02.

## Identity

### Q3. What a claim contains for each kind of declaration

- **Anchor.** [05 Snapshot, claim and witness identity](../../../spec/05-tooling.md#snapshot-claim-and-witness-identity),
  [06 Open signatures](../../../spec/06-runtime.md#open-signatures).
- **Question.** The `@claim.v2` record has one optional `signature`, a
  `defaults` dict, `guards`, `implementations`, and `targets`. Which fields
  a `type` with a `where`, an `interface`, an `impl` member, a module value,
  a `case`, and a `test` fill is not stated, nor whether an inferred or
  default part is in it: an omitted `performs` is the empty row, an omitted
  `representation` is the declaration's visibility, an omitted `given`
  is the bound `Any`.
- **Program.** `type Positive of I32 where Ordered.greater(value 0i32)
  visibility @public impl Equatable … endtype`; `interface AuditLog`;
  `test "t" takes {w: TestWorld} performs {TestWorld}`.
- **Recommended answer.** Fill by kind, every omitted part written as the
  value it stands for, so two spellings of one meaning hash alike:
  - `fn`, `proc`, member `fn` and `proc`: `signature` (the signature record
    with the row, empty when omitted, and the receiver slot), `defaults`,
    `guards` (`@requires`, `@ensures`), no `representation`.
  - `type`: `representation` (the resolved visibility), `signature` (its
    binders and the representation type as `typeInfo`), `guards` (the
    `@where` rows), `implementations` (every nested `impl`, sorted), empty
    `defaults`.
  - `case`: `signature` whose `typeInfo` is the record of its fields, or
    the empty tuple for a fieldless case.
  - `interface`: `access` and `kind` only, plus `implementations` for the
    `impl` blocks nested in it; its members are declarations of their own
    with claims of their own, so adding a member changes no interface
    claim and the member's claim appears.
  - associated type: `signature` absent; `kind`, `access`.
  - module value: `signature` with `binders: []` and the written type.
  - `test`: `kind: @test`, `signature` with `Fn({} Void)` or
    `Fn({TestWorld} Void)` and the written row as `effects`, no guards.
  - the entry of a target: `targets` with one fact per target; every other
    declaration writes `targets: []`.
  Nothing inferred by the checker enters a claim: a claim is the public
  contract as written, resolved to identities. The theory is content
  addressing of the interface, not of the implementation.
- **Depends.** Step 03. Rows `V2-TOOL-claim-kind-*`,
  `V2-TOOL-claim-omitted-row-equals-empty`,
  `V2-TOOL-claim-omitted-representation`.

### Q4. What a witness covers and when a member is hashed twice

- **Anchor.** [05 Snapshot, claim and witness identity](../../../spec/05-tooling.md#snapshot-claim-and-witness-identity).
- **Question.** A witness member's `body` is "the canonical source text of
  the whole declaration". A `type` declaration's text holds its members
  and `impl` blocks, each of which is a declaration with a witness of its
  own. Is a member's body in two witnesses, and does a member edit change
  the type's witness?
- **Program.** `type Box of {n: I32} fn twice takes {self: Self} returns r:
  I32 do … endfn endtype`, then an edit of `twice`'s body.
- **Recommended answer.** Yes on both counts, by the sentence as written: a
  declaration's text is its whole form, members included, so an edit of a
  member body changes the member's witness and the owner's. The owner
  resolves to nothing through its members' bodies unless its own text does,
  so `dependencies` lists only what the hashed text resolves to, which for
  an owner includes its members' references. No exception is added: one
  rule, "the whole declaration".
- **Depends.** Step 04. Rows `V2-TOOL-witness-member-edit-changes-owner`,
  `V2-TOOL-witness-member-own-group`.

### Q5. Which resolved references make a witness dependency

- **Anchor.** [05 Snapshot, claim and witness identity](../../../spec/05-tooling.md#snapshot-claim-and-witness-identity),
  [04 The standard library](../../../spec/04-programs-and-packages.md#the-standard-library).
- **Question.** A dependency is "one per declaration outside the group that
  a member's body resolves to". Does a reference to a type in a `let`
  annotation count as the body resolving to it? Do declarations of `@std`
  and of a `@path` dependency count, so that a library change changes every
  witness, and does a reference in a signature count, when the signature
  is in the whole declaration's text?
- **Program.** `fn f takes {s: Str} returns n: U64 do return Str.length(s)
  endfn`.
- **Recommended answer.** Every reference the hashed text contains that
  resolution binds to a declaration is an edge: in the signature, the
  guards, and the body, to a local declaration, a dependency package, or
  `@std` alike. So a library change changes the witness of everything that
  names the library, as the chapter's "however far" implies, and the
  toolchain record changes the revision beside it. No set of declarations
  is exempt; the group is the strongly connected component of this one
  graph, found by an iterative algorithm. The approval pins the toolchain
  separately only because a toolchain change may change a witness without
  changing any project byte.
- **Depends.** Step 04. Rows `V2-TOOL-witness-std-reference`,
  `V2-TOOL-witness-dependency-package`,
  `V2-TOOL-witness-signature-reference`.

### Q6. What each hash survives

- **Anchor.** [05 Snapshot, claim and witness identity](../../../spec/05-tooling.md#snapshot-claim-and-witness-identity),
  [02 Declared types](../../../spec/02-type-system.md#declared-types).
- **Question.** The chapters say what an anchor, a claim, and a witness
  cover. They do not list, in one place, what each survives: formatting, a
  comment, a rename, a move between modules, a change in a dependency, a
  change in the standard library, a change of toolchain, a change of line
  endings, a change of the file's absolute path or of the platform.
- **Program.** One module, hashed after each change.
- **Recommended answer.** The table below is the contract the plan tests;
  it follows from the three definitions and needs no new sentence.

  | Change | Anchor | Claim | Witness | Workspace revision |
  | --- | --- | --- | --- | --- |
  | formatting, a comment, line endings | same | same | same | changes (exact bytes, by design) |
  | a body edit | same | same | changes, and every dependent's | changes |
  | a signature, bound, default, guard, representation, or `impl` choice | same | changes | changes | changes |
  | a rename or a move to another module | changes, and every member's | changes, and every claim whose signature names it | changes, and every dependent's | changes |
  | a binding or policy of a target the declaration enters | same | changes (the entry's) | same | changes |
  | a change in a declaration it resolves to, in a dependency or in `@std` | same | same unless a named type's anchor changed | changes | changes |
  | the toolchain | same | same | same unless a library declaration's text changed | changes (the `toolchain` record) |
  | the absolute path of the checkout, the platform, the build directory of the toolchain | same | same | same | same |

- **Depends.** Steps 03, 04, 09. Rows `ATK-identity-path-independent`,
  `ATK-identity-crlf-same-claim`, `V2-TOOL-claim-rename-changes`,
  `V2-TOOL-witness-dependency-change`, `V2-TOOL-revision-crlf-differs`.

## Approvals and protected evidence

### Q7. The approval document and who writes it

- **Anchor.** [05 One workspace engine](../../../spec/05-tooling.md#one-workspace-engine) ("Formats"),
  [05 Snapshot, claim and witness identity](../../../spec/05-tooling.md#snapshot-claim-and-witness-identity) (the last paragraphs),
  [M3 README](README.md) "Protected evidence".
- **Question.** Approval documents are "canonical `.vibon` data" and a
  revision has `approval` records, but no chapter gives the document a
  schema, a place in the tree, a writer, or a reader. The command surface
  has no command that creates one.
- **Document.** None exists.
- **Recommended answer.** **Hannah.** One `@approval.v2` record per
  approved target, at `approvals/<target>.vibon` under the project root,
  with these fields in this order: `format`; `target` (atom); `entry`
  (`Str`, the entry's declaration identity); `claim` (`Str`, the entry's
  claim identity, which covers its signature, row, and every binding and
  policy through the target facts); `bindings` (list of `{leaf: Atom
  recipe: Str}` sorted by leaf, the recipe identities, repeated here so a
  changed binding is reported at its recipe); `tests` (list of `{module:
  Atom digest: Str}` sorted by module, one per module of `@tests`, `digest`
  the SHA-256 of the module's exact bytes); `selection` (list of `Str` test
  selectors sorted by UTF-8 bytes, the acceptance tests); `toolchain`
  (`Str`, the SHA-256 of the canonical bytes of the `@toolchain.v2`
  record). The protected evidence is therefore every module under `tests/`
  and the project's bindings; the code under repair is every module of the
  targets. The writer is the command of [Q8](#q8-the-command-that-presents-authority);
  the reader is `check`, which captures `approvals/*.vibon` as `approval`
  revision records and validates each against the snapshot. No signature
  or user identity is recorded in this release: the approver is whoever may
  write the tree, and version control is the record of who did. A document
  with an unknown field or a wrong shape is rejected under the schema
  rules.
- **Depends.** Steps 09, 13. Rows `V2-GRANT-approval-decode-*`,
  `V2-GRANT-approval-current`, `ATK-approval-no-signature-is-data`.

### Q8. The command that presents authority

- **Anchor.** [03 Target admission](../../../spec/03-effects.md#target-admission),
  [03 Filesystem policies](../../../spec/03-effects.md#filesystem-policies) ("Two limits of this design"),
  [07 Required coverage](../../../spec/07-diagnostics-and-conformance.md#required-coverage) 11,
  [M2 exit](../m2/exit.md#what-stays-unavailable) F8.
- **Question.** A target's authority "MUST be presentable in plain terms
  from the project file alone", two design limits "MUST appear in what a
  user is shown when approving a target", and coverage 11 asks that they
  be "shown at approval". No command of the surface shows them.
- **Command.** None exists.
- **Recommended answer.** **Hannah.** Add `vibra [GLOBAL...] approve TARGET
  [--write]` to the surface, between `run` and `eval`. Without `--write` it
  presents: the entry and its claim; for each leaf, in startup order, the
  interface, the provider, the resource as written, the startup read it
  needs (the `source` leaf), the normalized policy as a list of rules in
  plain terms and its hash, the limits, and `after`; the two design limits
  as the atoms `@authorityFollowsOpenedDirectory` and
  `@hardLinkReplaceDenied` with their sentences; and the state of the
  existing approval (`@none`, `@current`, or `@stale` with what changed).
  With `--write`, after a passing check, it writes the approval document of
  [Q7](#q7-the-approval-document-and-who-writes-it). It acquires nothing
  and creates no grant: the payload is data about recipes, as a grant
  projection is. The payload is `{target, entry, claim, bindings,
  designLimits, approval, written, path}`. The human format prints the same
  facts as text whose wording is not a contract. Capability presentation is
  the theory: an approval names exactly the authority it grants, and the
  presentation is computed from the same `BindingPlan` admission reads.
- **Depends.** Step 13. Rows `ATK-approve-*`, and the host tests of
  step 13.

### Q9. The code of a void approval

- **Anchor.** [07 Diagnostics are a language surface](../../../spec/07-diagnostics-and-conformance.md#diagnostics-are-a-language-surface),
  [07 Contract, effect and authority diagnostics](../../../spec/07-diagnostics-and-conformance.md#contract-effect-and-authority-diagnostics).
- **Question.** An approval "is void when the test, the policy, or the
  claim changes". The registry has `@grant.invalidBinding` for "a missing,
  extra, ill-typed, or unapproved grant binding", which covers a changed
  policy, and `@tool.stalePlan` for a plan whose revision no longer
  matches. No code names an approval whose protected tests, claim, or
  toolchain changed.
- **Document.** An approval whose `tests` digest differs from a module's
  bytes.
- **Recommended answer.** **Hannah.** Add `@project.staleApproval`
  (`@error`, fix capability `@none`), beside `@project.staleLock`: "an
  approval whose entry claim, protected test modules, selection, or
  toolchain no longer match the snapshot", primary span the approval's
  `format` field, related to the changed field of the approval and, for a
  test module, offset 0 of that module. A changed binding is reported once,
  as `@grant.invalidBinding` at the recipe ("unapproved"), and the claim
  mismatch it also causes is not reported again. Check reports both codes,
  so `test`, `run`, and `edit` inherit the refusal. The alternative, a
  widened `@tool.stalePlan`, would make one code mean two things.
- **Depends.** Steps 09, 13, 27, 32. Rows `V2-GRANT-approval-stale-test`,
  `V2-GRANT-approval-stale-claim`, `V2-GRANT-approval-stale-toolchain`,
  `V2-GRANT-approval-unapproved-binding`, `V2-GRANT-approval-one-fault`.

### Q10. The code of an edit the approval forbids

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans)
  ("No fix or format may alter a protected claim, test, or policy outside
  its approval workflow"), [07 Runtime and tool failures](../../../spec/07-diagnostics-and-conformance.md#runtime-and-tool-failures).
- **Question.** A plan that would change a protected test module or the
  entry's claim under a current approval "fails with nothing written". The
  only code for a denied tool action is `@tool.actionDenied`, whose row
  says "an MCP or tool action the server's permissions deny".
- **Command.** `vibra --format json edit replace @tests.main... DATA` under
  a current approval.
- **Recommended answer.** **Hannah.** Reuse `@tool.actionDenied`, with its
  row widened to "an MCP or tool action the server's permissions, or an
  approval in force, deny", no location, related to the approval's
  `format` field. One code for "the tool refused to act", whatever
  refused. The result is `@command.diagnostics`, the plan is returned for
  reading with `applied: false`, and `fmt --write` on a protected module
  is refused the same way.
- **Depends.** Steps 11, 14, 19, 20, 21. Rows
  `V2-TOOL-edit-replace-test-denied`, `V2-TOOL-edit-rename-into-tests-denied`,
  `ATK-edit-apply-protected-unchanged`, `ATK-fmt-write-protected-denied`.

### Q11. Whether `run` and `test` need an approval

- **Anchor.** [03 Target admission](../../../spec/03-effects.md#target-admission),
  [M3 README](README.md) exit evidence.
- **Question.** The exit asks that a changed claim "under an old approval"
  be rejected. Nothing says a target must have an approval to run or test.
- **Command.** `vibra run` on a project with no `approvals/` directory.
- **Recommended answer.** An approval is optional. With none, `run` and
  `test` behave as in M2. With one, the rules of [Q9](#q9-the-code-of-a-void-approval)
  and [Q10](#q10-the-code-of-an-edit-the-approval-forbids) bind every
  command. Requiring one would make every M2 case stale.
- **Depends.** Step 09. Rows `V2-GRANT-approval-absent-runs`,
  `V2-GRANT-approval-body-edit-current`.

## Edit plans

### Q12. The document revision of a plan

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans),
  [07 Diagnostic documents and spans](../../../spec/07-diagnostics-and-conformance.md#diagnostic-documents-and-spans) (`fixes[].expectedRevision`).
- **Question.** A plan's `documents` list `{path, revision}` and a fix has
  an `expectedRevision`. Neither chapter defines a document revision.
- **Document.** A plan over `src/app/main.vib`.
- **Recommended answer.** A document revision is `sha256:` and the SHA-256
  of the document's exact bytes, the same preimage a `source` revision
  record contributes, so a plan's documents are verified by the bytes the
  snapshot holds and the formula needs no new domain tag. The workspace
  revision stays the identity of the whole snapshot.
- **Depends.** Steps 07, 14. Rows `V2-TOOL-edit-plan-document-revision`,
  `ATK-edit-apply-document-changed`.

### Q13. The expected syntax identity and the source fingerprint of an edit

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans).
- **Question.** An edit has "the expected syntax identity, and a source
  fingerprint". Neither is defined.
- **Recommended answer.** `syntaxIdentity` is the `nodeId` of the replaced
  node, `<sourceId>#<start>-<end>`, with its `syntaxKind` atom, as the
  position query spells them; `fingerprint` is `sha256:` and the SHA-256 of
  the replaced bytes. Applying checks both against the staged document, so
  a plan computed against one revision never edits another's bytes even
  when the revision check is skipped.
- **Depends.** Step 07. Rows `V2-TOOL-edit-plan-fields`,
  `ATK-edit-apply-fingerprint-mismatch`.

### Q14. A stale edit on the command line

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans),
  [v0.0.2 release gate](../README.md#release-gate), step 3.
- **Question.** Over MCP a write "needs a plan returned earlier", so a
  stale plan is natural. On the command line a mutating command previews
  and applies in one run, so the agent of the release gate cannot produce
  the "stale edit" the gate asks it to show rejected.
- **Command.** `vibra --format json edit replace @gate.tally.count DATA
  --write` after another file changed.
- **Recommended answer.** **Hannah.** Add one option to every mutating
  command: `--expect-revision DIGEST`, the workspace revision the caller
  planned against. Without it `--write` applies to the current snapshot.
  With it, a different revision is `@tool.stalePlan` with nothing written,
  `applied: false`, and the plan the caller would have got. Independently
  of the option, the publish step verifies each document's bytes through
  the handle it opened for replacement, so the window between the snapshot
  and the write is closed. This is optimistic concurrency: the caller
  states the version it read, and the writer refuses on a mismatch.
- **Depends.** Steps 01, 14, 32, 34. Rows `ATK-edit-apply-stale-document`,
  `ATK-edit-apply-expect-revision`, and the row "expect-revision parsed"
  of step 01.

### Q15. Atomic application of several files

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans)
  ("publish all files, or none"; "A backend that cannot write several
  files atomically MUST refuse before writing").
- **Question.** Windows replaces one file atomically (`MoveFileExW` with
  `MOVEFILE_REPLACE_EXISTING`, or `ReplaceFileW`) and offers no multi-file
  commit. A rename that touches three modules cannot be made visible in
  one step. Is such a plan refused, or is "none" the state a reader finds
  after recovery?
- **Command.** `edit rename @app.model.User Person --write` with references
  in two other modules.
- **Recommended answer.** **Hannah.** Publish through a write-ahead
  journal: the engine writes `.vibra-journal.vibon`, one `@editJournal.v2`
  record holding the plan's revision and every document's path, expected
  digest, and new bytes, created exclusively and flushed; then replaces
  each document in path order through the held workspace directory; then
  deletes the journal. Every command starts by looking for the journal:
  one that is present and complete is replayed (each document is either at
  its expected digest and is replaced, or already at its new digest and
  is left), then deleted, before the snapshot is taken; one that is
  malformed or truncated is deleted and reported as a line on standard
  error, and nothing it names is touched, since none was written before
  it was complete. A crash between files therefore leaves a state the next
  command finishes, which is what "all or none" can mean on this platform;
  the plan is never reported applied until the journal is gone. The
  alternative reading, refusing every plan with more than one document,
  keeps the rename of a public declaration a preview-only feature; it is
  one line to choose.
- **Depends.** Step 14. Rows `ATK-edit-journal-crash-between-files`,
  `ATK-edit-journal-replay-idempotent`, `ATK-edit-journal-malformed`,
  `V2-TOOL-edit-rename-several-documents` (preview).

### Q16. Applying an edit off Windows

- **Anchor.** [03 Filesystem policies](../../../spec/03-effects.md#filesystem-policies) rule 6,
  [M2 architecture](../m2/architecture.md#3-fake-and-real-drivers).
- **Question.** The engine writes through the held workspace directory
  with the real-name and no-link guarantees of `vibra-fs-scope`, which has
  a Windows implementation only. What does `--write` do elsewhere?
- **Recommended answer.** **Hannah.** `--write` is `@tool.unavailable` on a
  platform with no scoped directory adapter, with the preview still
  computed and returned, exactly as a target with a directory leaf is
  unavailable at admission there. No unchecked fallback writer is added.
  The corpus holds previews only, so it stays the same on three platforms;
  the apply rows are host tests that print `NOT RUN` elsewhere, as M2's
  attack rows do.
- **Depends.** Step 14. Rows `vibra-workspace` `edit_apply::not_run_off_windows`
  and every `ATK-edit-apply-*`, `ATK-edit-journal-*`.

### Q17. What `edit` may write, and what confines it

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans)
  ("Edits reach local modules and never vendored dependencies"),
  [05 MCP server](../../../spec/05-tooling.md#mcp-server) (paths confined).
- **Question.** A `@path` dependency lies beneath the workspace root and is
  not vendored. May an edit reach it? Through which path check are writes
  confined, given that discovery follows no link below a root?
- **Recommended answer.** An edit reaches the modules of the root project's
  own units, the targets and `@tests`, and nothing else: a subject in a
  `@path` dependency, in `@std`, or in `project.vibon` is invalid input.
  The publish step opens the workspace root once as a held directory and
  reaches every document from it one name at a time through the scoped
  adapter, so a link anywhere on the path, a component whose real name
  differs, or a path with a `..` is refused before any write; the only
  other path it may write is the journal of [Q15](#q15-atomic-application-of-several-files),
  at the root. An architecture test lists every call that writes a file in
  `vibra-workspace` and allows the two sites.
- **Depends.** Steps 14, 17. Rows `ATK-edit-apply-symlink-refused`,
  `ATK-edit-apply-alias-refused`, `V2-TOOL-edit-dependency-subject-invalid`,
  `ATK-edit-writers-enumerated`.

### Q18. Overlapping edits and the order of a plan

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans).
- **Question.** "Edits are ordered by path and span and do not overlap. A
  plan that is … overlapping … fails." The command line hands no plan in,
  so an overlapping plan can only be computed by the tool.
- **Recommended answer.** Overlap in a computed plan is a defect: the plan
  builder sorts and checks, and an overlap is `@command.trap` with
  `@runtime.invalidCheckedProgram`. Over MCP, later, a supplied plan that
  overlaps is invalid input. No case of the corpus can show the trap; a
  host test over the builder does.
- **Depends.** Step 07. Row `ATK-edit-plan-overlap-is-defect`.

### Q19. What `edit rename` may rename, and what it reports

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans),
  [05 Snapshot, claim and witness identity](../../../spec/05-tooling.md#snapshot-claim-and-witness-identity) ("A rename or move changes it, and the edit preview says so").
- **Question.** Rename "rejects discards, generated nodes, keywords,
  private-access violations, collisions, and shadowing". Which identities
  are renameable: a module, a local, a labelled parameter whose label is
  part of the call contract, a case, a field, a type parameter, an entity
  of `@std`? And what does `postconditions` hold?
- **Recommended answer.** Renameable: a top-level declaration of a local
  unit, a member, a case, a field, a type parameter, a labelled parameter
  (every call site's label is edited, since resolution binds it), and a
  local binder (within its sequence). Not renameable, as invalid input: a
  module, which is a file; `_`; a `@std` or dependency entity; a name of a
  recovered node. `NEWNAME` that is a reserved word or a prelude name is
  invalid input. A collision or a shadowing the new name would cause is
  the plan's `resultingDiagnostics` (`@name.redeclaration`,
  `@name.reservedDeclaration`), returned as `@command.diagnostics` with
  `applied: false`. `postconditions` is a list of
  `{declaration: Str kind: Atom before: Str after: Str}` sorted by
  `declaration` then `kind`, with `kind` one of `@nominal`, `@claim`,
  `@witness`, one entry per identity of the local package that changes:
  the renamed declaration and its members (`@nominal`), every declaration
  whose claim names it (`@claim`), every declaration whose witness changes
  (`@witness`). A body edit lists `@witness` entries only.
- **Depends.** Steps 11, 19. Rows `V2-TOOL-edit-rename-*`,
  `V2-TOOL-edit-replace-postconditions-witness-only`.

### Q20. What `edit imports` does

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans)
  ("Organizing imports changes neither resolution nor operand order"),
  [01 `use`](../../../spec/01-source-language.md#use).
- **Question.** The chapters say what organizing imports must not do and
  nothing about what it does.
- **Recommended answer.** **Hannah.** `edit imports PATH` sorts the module's
  `use` declarations by the UTF-8 bytes of their paths, keeps each `as`,
  keeps a comment with the `use` it precedes, removes nothing, and adds
  nothing; a module whose imports are already in that order has an empty
  plan with `changed` false. The formatter already requires one `use` per
  form, so order is the one thing left to organize. Removing an unused
  import is not organizing: it changes what the module names.
- **Depends.** Step 20. Rows `V2-TOOL-edit-imports-*`.

### Q21. Which codes gain a safe fix

- **Anchor.** [07 Diagnostics are a language surface](../../../spec/07-diagnostics-and-conformance.md#diagnostics-are-a-language-surface) (fix capability),
  [05 Command surface](../../../spec/05-tooling.md#command-surface) (`edit fix`).
- **Question.** Every code's fix capability is `@none` today, so `edit fix`
  can never find "exactly one applicable safe fix". The capability
  "describes what the implementation can currently do", so widening it is
  not a specification change, but which fixes are safe is.
- **Recommended answer.** Two codes become `@safe` in M3, each with one
  fix: `@effect.duplicateRoot` removes the later entry of the row or list
  (the set is unchanged, so no claim changes), and
  `@module.missingRequiredImport` inserts `use @std.assert enduse` into the
  test module's import block. Both are repairs of the shapes the pilot's
  models got wrong. `@contract.unusedEffect` stays `@none`: removing a root
  from a ceiling changes the claim. `@style.casing` stays `@none`: a casing
  change is a rename, which has its own command and its collision checks.
- **Depends.** Steps 21, 24. Rows `V2-TOOL-edit-fix-*`, `V2-TOOL-lint-*`.

### Q22. What `lint` is

- **Anchor.** [05 Command surface](../../../spec/05-tooling.md#command-surface),
  [05 Command payloads](../../../spec/05-tooling.md#command-payloads) (`lint`: `accepted`, `plan`).
- **Question.** `lint [TARGET] [--write]` has a payload and covers the same
  targets as `check`, and nothing else is said of it.
- **Recommended answer.** **Hannah.** `lint` is `check` followed by one
  plan that holds every safe fix of the check's diagnostics, over the
  modules `check` covers, or `null` when there is none; `--write` applies
  the plan. `accepted` is the check's verdict. A target with no safe fix
  and no error is `@command.ok` with `plan: null`.
- **Depends.** Step 24. Rows `V2-TOOL-lint-*`.

## Bounded evaluation

### Q23. How an `eval` argument is written

- **Anchor.** [05 Program arguments, execution and evaluation](../../../spec/05-tooling.md#program-arguments-execution-and-evaluation),
  [06 Values](../../../spec/06-runtime.md#values).
- **Question.** `--arg VIBON` values are "checked against its parameter
  types". A declared value such as `Celsius(21.5f64)` is an application,
  which VIBON does not admit, and a `@value.v2` document needs the
  parameter type's identity, which an agent does not have.
- **Command.** `vibra eval @app.temp.toF --arg 21.5f64`.
- **Recommended answer.** **Hannah.** An argument is the `value` field of
  the canonical value encoding, decoded against the parameter's type as
  the `typeInfo`: a scalar as its literal, a declared record as `{kind:
  @record fields: {…}}`, a wrapper as `{kind: @wrapper value: …}`, a
  variant as `{kind: @enum variant: @c payload: {…}}`, and so on; the
  `typeInfo` field of a compound is omitted, since the parameter supplies
  it, and one that is written must equal it. The decoder checks every
  width, invariant, and nominal identity as the value decoder does. A value
  that does not decode is invalid input. The subject's labelled parameters
  take their defaults; a generic subject is invalid input, since no
  call-site type arguments exist.
- **Depends.** Step 10. Rows `V2-TOOL-eval-arg-*`.

### Q24. A bare `requires` on the subject of `eval`

- **Anchor.** [05 Program arguments, execution and evaluation](../../../spec/05-tooling.md#program-arguments-execution-and-evaluation)
  ("An unproved static obligation is an error for eval as for check"),
  [02 Bare guards](../../../spec/02-type-system.md#bare-guards) GUARD-1.
- **Question.** A call of a function with a bare `requires` is a site
  where the obligation is discharged. `eval` is such a call with constant
  operands. Is the predicate evaluated on the arguments, and what is the
  result when it is false?
- **Command.** `eval @app.math.half --arg 0i32` where `half` requires
  `Ordered.greater(n 0i32)`.
- **Recommended answer.** The call is discharged as GUARD-1 discharges a
  call with constant operands: a true predicate admits the evaluation; a
  false one is `@contract.refuted` at the predicate, related to the
  argument, result `@command.diagnostics`, and nothing runs. The `ensures`
  of the subject are the checker's business before any eval, as for
  `check`. A workspace whose check has an error is `@command.diagnostics`
  with no evaluation, as `test` already does.
- **Depends.** Step 10. Rows `V2-TOOL-eval-requires-refuted`,
  `V2-TOOL-eval-requires-passed`, `V2-TOOL-eval-check-error`.

### Q25. The result of an evaluation that cannot be shown

- **Anchor.** [05 Program arguments, execution and evaluation](../../../spec/05-tooling.md#program-arguments-execution-and-evaluation)
  ("A result that cannot be shown is an explicit tool outcome, not a trap"),
  [05 Command payloads](../../../spec/05-tooling.md#command-payloads) (`eval`).
- **Question.** The payload has `value` or `observationError`. Which
  command result and status does an observation error carry, and is a
  diagnostic added?
- **Recommended answer.** `@command.ok`, status `passed`, `value: null`,
  `observationError` set, and no envelope diagnostic: the evaluation
  completed and only its showing failed, exactly as a test item records an
  observation failure in its own record. The subject's static type
  already tells whether a value can be shown, so a subject whose result
  type is not `Encodable` is refused before evaluation as invalid input,
  and the observation error arises only for an interface value that holds
  a callable, a plan, or a grant.
- **Depends.** Step 10. Rows `V2-TOOL-eval-unencodable-type-invalid`,
  `V2-TOOL-eval-interface-value-observation-error`.

### Q26. The limits of `eval` and their names

- **Anchor.** [05 Program arguments, execution and evaluation](../../../spec/05-tooling.md#program-arguments-execution-and-evaluation) ("Limits"),
  [m2/fix-e](../m2/exit.md#what-stays-unavailable).
- **Question.** `m1Default` gives one compile-time evaluation 10,000,000
  reductions and names `memory` and `sourceNesting` in `limits.reached`;
  the fix package E names the reached limits of a compile-time evaluation
  and an obligation. `eval` "uses the same interface" and its names are
  not stated, nor its memory budget, nor a bound on the size of its output.
- **Recommended answer.** `eval` runs under the same 10,000,000 reductions
  and reports the limit under the name fix E gives a compile-time
  evaluation; this plan writes that name as `evaluationReductions` and the
  step that lands first keeps the spelling. Its memory budget is the
  interpreter's default, 256 MiB, reported as `memory`. Its output is
  bounded by [Q28](#q28-the-output-limit-of-every-command). A reached work
  limit is `@tool.limitExceeded` with no location, status `unknown`,
  `complete: false`, and the result `@command.diagnostics` with that one
  diagnostic, as `check` reports a limit that stops an obligation; the
  result table gives `@command.operationalFailure` to a tool failure and to
  memory exhaustion only, so a reached work limit is not one.
- **Depends.** Steps 10, 23. Rows `V2-TOOL-eval-reductions-limit`,
  `V2-TOOL-eval-memory-exhausted`.

### Q27. A work limit for a test item

- **Anchor.** [05 Tests and selection](../../../spec/05-tooling.md#tests-and-selection),
  [05 Program arguments, execution and evaluation](../../../spec/05-tooling.md#program-arguments-execution-and-evaluation) ("Every executing command runs under published, versioned limits on work, memory, and output").
- **Question.** A test item has a memory budget and no work limit, so a
  test that does not terminate never ends. Which limit, which item result,
  and which name?
- **Recommended answer.** **Hannah.** `m1Default` gives each test item
  100,000,000 reductions, ten compile-time evaluations. An item that
  reaches it ends `@test.exhausted`, whose meaning widens to "the item
  exhausted its memory or its work limit", with the diagnostic
  `@tool.limitExceeded` (no location) attributed to the item,
  `limits.reached` naming `testReductions`, `complete: false`, and the
  suite precedence unchanged (operational exhaustion). No new item result
  atom: exhaustion of a budget is one outcome.
- **Depends.** Step 23. Rows `V2-PROJECT-test-reductions-limit`,
  `V2-PROJECT-test-reductions-other-items-run`.

### Q28. The output limit of every command

- **Anchor.** [05 Versioned envelope and results](../../../spec/05-tooling.md#versioned-envelope-and-results)
  ("When output would exceed its limit, the tool emits a bounded, valid
  document with status `unknown`").
- **Question.** No limit on output is published, and no chapter says what
  a bounded index, plan, or expansion looks like.
- **Recommended answer.** **Hannah.** `m1Default` bounds the JSON envelope
  at 16 MiB, named `outputBytes`. A command whose payload would exceed it
  truncates the longest list of its payload from the end, keeping the
  sorted order, until the envelope fits, and sets `complete: false`,
  status `unknown`, `limits.reached: ["outputBytes"]`. The lists are: the
  index's `declarations`, `implementations`, and `references`; an
  expansion's entries; a plan's `edits` and `postconditions`; `run`'s
  `auditTrace`; `test`'s items' traces. The emitter asserts the bound; a
  document still over it is a defect trap.
- **Depends.** Steps 01, 16, 22. Rows `V2-TOOL-query-index-bounded`,
  `ATK-output-bound-asserted`.

### Q29. A time limit

- **Anchor.** [05 Program arguments, execution and evaluation](../../../spec/05-tooling.md#program-arguments-execution-and-evaluation),
  [06 Activations and memory](../../../spec/06-runtime.md#activations-and-memory) ("Limits on work, time, and output that a tool or host enforces from outside likewise give an explicit, incomplete outcome").
- **Question.** The brief asks every M3 service for a stated limit on time.
  The chapter's published limits are on work, memory, and output.
- **Recommended answer.** No wall-clock limit in this release: a limit on
  time makes an observation depend on the machine, which the corpus cannot
  hold, while a reduction count is deterministic. Work and memory bound
  every evaluation, and the checker's own work is bounded by nothing but
  its input, which the M2 exit records as a gap (slow checks) that M3 does
  not close. The exit record of M3 says so.
- **Depends.** Step 36 (the exit record).

## Queries

### Q30. The query case: inputs, expectation, comparison

- **Anchor.** [07 Case operations and inputs](../../../spec/07-diagnostics-and-conformance.md#case-operations-and-inputs)
  ("The keys of a `query` case's subject, offset, and options are not defined"),
  [07 Persistent observations](../../../spec/07-diagnostics-and-conformance.md#persistent-observations).
- **Question.** A `query` case has no manifest keys and no expectation key,
  and a query result is JSON, while observations are VIBON.
- **Recommended answer.** **Hannah.** Manifest keys under `[inputs]`:
  `subject` (the subject exactly as the command line takes it), `include`
  and `expand` (lists of strings); under `[expect]`: `query`, a JSON file
  holding the expected payload, compared as parsed JSON values after it
  validates against the query schema, with `workspaceRevision` fields
  rewritten by `revisions.ps1` as it rewrites VIBON revisions; and `index`,
  a VIBON file holding the expected `@index.v2`, compared as canonical
  bytes. A query result is the one machine contract that is JSON, so its
  expectation is JSON.
- **Depends.** Steps 01, 05. Every `V2-TOOL-query-*` row.

### Q31. Case operations for `eval` and `edit`

- **Anchor.** [07 Case operations and inputs](../../../spec/07-diagnostics-and-conformance.md#case-operations-and-inputs),
  [07 Conformance profiles](../../../spec/07-diagnostics-and-conformance.md#conformance-profiles).
- **Question.** The operation table is closed and has no row for an
  evaluation, an edit preview, or an approval presentation, so none can be
  a corpus case.
- **Recommended answer.** **Hannah.** Add two operations: `eval` under
  `interpreterV2` (inputs `tree`, `project`, `subject`, `arguments`, a
  list of VIBON texts; expectation `eval`, a `@value.v2` document, or the
  diagnostics and `observationError`) and `edit` under `toolingV2` (inputs
  `tree`, `project`, `edit`, one of `replace`, `rename`, `imports`, `fix`,
  `subject`, `newName`, `path`, `data`, a file holding the replacement
  text; expectation `editPlan`, a JSON file compared as the query result
  is). Both observe a preview; no case writes. The `approve` presentation
  stays a host test, as `help` is. The `help` inventory's `operations` of
  `eval` and `edit` follow from the table.
- **Depends.** Steps 01, 10, 11. Every `V2-TOOL-eval-*` and
  `V2-TOOL-edit-*` row; without this ruling each becomes a `vibra-cli`
  host test of the same name.

### Q32. Position subjects at a boundary, at the end, in trivia, and elsewhere

- **Anchor.** [05 Structural position and semantic envelope](../../../spec/05-tooling.md#structural-position-and-semantic-envelope),
  [05 Workspace queries](../../../spec/05-tooling.md#workspace-queries).
- **Question.** "The smallest non-empty syntax node that contains the
  offset", with spans half-open. At the boundary between two tokens the
  offset is the start of the second and not inside the first. At the end
  of the file no leaf contains the offset. In a comment, in a module that
  does not check, in a `@path` dependency, and in `@std`, what is returned?
- **Recommended answer.** A node contains an offset when `start <= offset <
  end`, so at a boundary the later token wins, and a zero-width recovery
  node at the offset wins over it, as the chapter says. At the end of the
  file the smallest node whose `end` equals the offset wins, the deepest
  first, then the document node. In a comment the node is the comment leaf,
  category `trivia`, and every semantic field is `unavailable`. In a module
  with errors the structural record is exact and each semantic field is
  `exact`, `recovered`, or `unavailable` by its own fact. A path subject
  must name a module of the root project's units; a module of a `@path`
  dependency is confined and readable, so it is admitted for a query and
  refused for an edit ([Q17](#q17-what-edit-may-write-and-what-confines-it));
  `@std` has no path, so only its entity atoms are subjects. An offset
  past the end or inside a scalar is invalid input.
- **Depends.** Steps 05, 06, 12. Rows `V2-TOOL-query-position-boundary`,
  `V2-TOOL-query-position-end-of-file`, `V2-TOOL-query-position-comment`,
  `V2-TOOL-query-position-in-recovery`, `V2-TOOL-query-position-dependency`,
  `V2-TOOL-query-position-inside-scalar`, `V2-TOOL-query-position-past-end`.

### Q33. Where the two position records go in the `query` payload

- **Anchor.** [05 Command payloads](../../../spec/05-tooling.md#command-payloads) (`query`),
  [05 Structural position and semantic envelope](../../../spec/05-tooling.md#structural-position-and-semantic-envelope).
- **Question.** The payload is `{subject, resolvedKind, identity, metadata,
  expansions}`, and the chapter also defines a structural record and a
  semantic envelope with their own `schemaVersion`. Which holds which?
- **Recommended answer.** **Hannah.** `metadata` is an object with one
  member per `--include` value given, in the order of the option table:
  `syntax` holds the structural record; `context` holds `{nodeId, role,
  context, visibleLocals, visibleImports, declarationCandidates}`;
  `source` holds `{span, text}`; `signature` holds the signature record of
  the resolved declaration; `type` holds `{expectedType, observedType}`
  (each `{status, value}`); `effects` the effects value; `visibility`
  `{access, representation}`; `diagnostics` the diagnostics whose primary
  span contains the position or lies in the entity; `diagnostic` the
  registry entry of a code subject; `index` the index. With no include,
  `metadata` holds `{nodeId, identity, resolvedKind}` only. The semantic
  envelope is the union of these members with the envelope's own fields,
  so one schema, `urn:vibra:schema:v2:query`, describes the payload with
  each member optional, and the two records of the chapter are its
  `$defs`. `resolvedKind` is `position`, `entity`, `diagnosticCode`, or
  `workspace`; `identity` is the `{kind, canonical}` of the entity or of
  the declaration a position lies in, else `null`.
- **Depends.** Steps 05, 12. Every `V2-TOOL-query-*` row.

### Q34. The shape of each expansion

- **Anchor.** [05 Workspace queries](../../../spec/05-tooling.md#workspace-queries) (`--expand`),
  [05 Index records](../../../spec/05-tooling.md#index-records).
- **Question.** `references`, `members`, `calls`, `applications`,
  `effectWitnesses`, and `declarations` are named and only
  `effectWitnesses` is given a shape.
- **Recommended answer.** Each expansion is a list under its name in
  `expansions`, sorted as the index sorts the same records:
  `references` the index's reference records whose `to` is the subject;
  `members` the declaration records of the subject's members; `calls` the
  application facts whose `callee` is the subject and whose kind is
  `@function` or `@procPlan`; `applications` every application fact whose
  `callee` is the subject; `declarations` the declaration records of the
  workspace (with `--include index`, the whole index). The records are the
  index's, in their JSON form, so an expansion and the index never
  disagree.
- **Depends.** Steps 16, 22. Rows `V2-TOOL-query-expand-*`.

### Q35. The effects value at a position

- **Anchor.** [05 Effect and authority projections](../../../spec/05-tooling.md#effect-and-authority-projections).
- **Question.** `application` is `{immediate, latent, awaitSite}` and
  `authority` is "the grant projections known at this position and,
  explicitly, what is unknown"; neither is defined further. A static
  summary "never invents which resource a run will touch".
- **Recommended answer.** `immediate` is the row evaluating the operands
  performs (empty, since operands are pure); `latent` the plan's complete
  row; `awaitSite` the span record of the `await` whose direct operand the
  application is, else `null`. `authority` is a list, one entry per
  grant-typed parameter or local visible at the position, of
  `{name, interface, leaf, policy, limits, status}`: for a parameter of a
  target's entry, `leaf`, `policy` (the normalized rules), and `limits` come
  from the recipe with `status: exact`; for every other binding they are
  `null` with `status: unavailable`, which is the explicit unknown.
  `target` is filled only when the enclosing declaration is a target's
  entry.
- **Depends.** Step 15. Rows `V2-TOOL-query-effects-*`.

### Q36. A query over a snapshot the limit or a capture fault stops

- **Anchor.** [05 Program arguments, execution and evaluation](../../../spec/05-tooling.md#program-arguments-execution-and-evaluation)
  ("No later stage runs on the snapshot that holds such a document"),
  [05 Versioned envelope and results](../../../spec/05-tooling.md#versioned-envelope-and-results).
- **Question.** `check`, `test`, and `run` report a nesting limit alone.
  `query` is not named. A workspace whose project cannot be captured has no
  revision.
- **Recommended answer.** `query` is a later stage: over a snapshot with a
  document past the limit it reports the limit alone, status `unknown`,
  the empty payload. A capture failure is `@command.diagnostics` with the
  capture's diagnostics and the empty payload, as `run` already answers.
  A position query of a file that is not a module of the snapshot is
  invalid input.
- **Depends.** Step 05. Rows `V2-TOOL-query-nesting-limit`,
  `V2-TOOL-query-capture-fault`, `V2-TOOL-query-position-not-a-module`.

### Q37. The diagnostic-code subject

- **Anchor.** [05 Index records](../../../spec/05-tooling.md#index-records) (the last paragraph),
  [07 Diagnostics are a language surface](../../../spec/07-diagnostics-and-conformance.md#diagnostics-are-a-language-surface).
- **Question.** "A diagnostic code used as a query subject reads the
  compiler's registry." What does the payload hold, and does it take a
  snapshot?
- **Recommended answer.** `resolvedKind: diagnosticCode`, `identity:
  {kind: @diagnosticCode canonical: "@type.mismatch"}`, and
  `metadata.diagnostic` = `{code, level, domain, summary, fixCapability}`
  whether or not `--include diagnostic` is given; `workspaceRevision` is
  `null`, since no snapshot is taken, as for `help`. An unknown code is
  invalid input.
- **Depends.** Step 05. Rows `V2-TOOL-query-code-known`,
  `V2-TOOL-query-code-unknown`.

## Generated references

### Q38. Where the generated references live and what generates them

- **Anchor.** [M3 README](README.md) "Generated references",
  [05 Schemas, delivery and exclusions](../../../spec/05-tooling.md#schemas-delivery-and-exclusions).
- **Question.** "The syntax card, checked examples, and grammar and schema
  data are generated from the implementation and checked against it." No
  chapter says where they are kept, in what form, or by which tool.
- **Recommended answer.** **Hannah.** Under `docs/reference/`: `README.md`
  (the index), `syntax-card.md`, `examples/` (one `.vib` or project tree
  per example, each with its registered verdict in a `examples.tsv`), and
  `data/` with `grammar.vibon` (the construct and closer table, admissible
  sections per construct, tag sets, type heads, reserved words, all read
  from `vibra-syntax`'s tables), `diagnostics.vibon` (the registry with
  levels, domains, summaries, and fix capabilities), `commands.vibon` (the
  inventory with each command's support, operations, profiles, and
  schemas), and `schemas/` (a copy of every published JSON schema). The
  generator is the binary `vibra-reference` of `vibra-conformance`, run
  as `cargo run -p vibra-conformance --bin vibra-reference -- --out
  docs/reference`; a host test regenerates into memory and fails when a
  committed file differs, and a second runs every example through the
  engine and fails when its verdict or codes differ from the table. The
  card is prose over the data: its lists are rendered from `data/`, its
  sentences from a template the step owns, so nothing it says about the
  grammar can drift from the reader.
- **Depends.** Steps 25, 26. Rows `reference::committed_files_are_generated`,
  `reference::examples_give_registered_verdicts`,
  `reference::card_shapes_from_the_pilot`.

## The repair scenario

### Q39. Who seeds, who repairs, and what the pass rule is

- **Anchor.** [v0.0.2 release gate](../README.md#release-gate), step 3,
  [M3 README](README.md) exit evidence, [M2 registration](../m2/pilot/registration.md).
- **Question.** "From a defect seeded by someone other than the author of
  the repair run" fixes the separation and nothing else: the program, the
  seeder, the runner, the tools, the budget, the pass rule.
- **Recommended answer.** **Hannah.** The program is the release program
  `examples/gate/` with its tests under an approval of its `@gate` target.
  The seeder is a person or a model session that is not the runner's; it
  changes exactly one declaration of the code under repair so that exactly
  one registered test fails and `check` still passes, records the seed in
  a sealed file the runner never reads, and commits. The runner is one
  model session with a fixed system prompt that permits `vibra --format
  json` commands only, through a wrapper that logs every command and its
  envelope and refuses any other program. Budget: at most 40 commands and
  at most 3 applied edits. Pass rule: after an applied edit the selected
  test passes, every envelope validates against its schema, `check` has
  no error, no protected file changed (the approval stays current), and
  the four rejections (stale edit, replaced test, changed policy, changed
  claim) were each demonstrated by the reference controls before the run.
  Exclusions: a seed the controls show does not fail its test is void and
  reseeded before any run; a provider failure is retried once and
  recorded. The record is dated and says what the scenario does not show.
- **Depends.** Steps 31, 32, 33, 34, 35.

## Where the tree contradicts the M3 chapters

These are not open questions. Each is a place where the code as it stands
differs from the specification, with the step that replaces it.

| Where | The tree | The specification | Step |
| --- | --- | --- | --- |
| `crates/vibra-cli/src/inventory.rs` | `query`, `eval`, `edit fix`, `edit rename`, `edit imports`, `lint` are `supported: false` | available in M3 | 05, 10, 21, 19, 20, 24 |
| `crates/vibra-diagnostics/src/registry.rs` | every code's fix capability is `@none` | `edit fix` needs a `@safe` code (Q21) | 21 |
| `crates/vibra-conformance/src/standard.rs` | no handler for `query`; `eval` and `edit` are no operations | `query` observes "the versioned query result" (Q30, Q31) | 01, 05, 10, 11 |
| `crates/vibra-workspace/src/snapshot.rs` | no `approval` record is captured | the revision has `approval` records (Q7) | 01, 09 |
| `crates/vibra-schema/schemas/v2/command-result.json` | payloads for `help`, `fmt`, `check`, `test`, `run` only | every payload of the table | 01 and each command's step |
| `crates/vibra-types/src/check/mod.rs` | `CheckResult` keeps declarations' types and obligations, no fact per node | a query reports expected and observed types at a position | 08 |
