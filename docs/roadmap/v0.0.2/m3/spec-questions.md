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

A question marked **Hannah** is language design, the security model, or the
shape of a published format or command surface, and waits for the owner;
the others are proposed rulings the integrator may confirm. Each
recommendation is written so that it can be accepted or overturned in one
line, and each open question of Hannah's names the alternative in one
sentence.

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
  and to the plan's `operation` set: `edit body SUBJECT DATA [--write]`.
  `SUBJECT` is the canonical entity atom of a `fn`, a `proc`, a `fn` or
  `proc` member, or a module value of a local unit. `DATA` is one canonical
  VIBON record `{body: Str uses: List(Atom)}`: `body` is the source text of
  the new body alone (the statements between `do` and the closer of a
  callable; the expression after `be` of a module value), and `uses` lists
  the module paths the body needs imported. The engine parses `body` as a
  statement sequence (or one expression) standing alone, so a token that
  would close the body or open a declaration is invalid input; it replaces
  the subject's body section, inserts `use P enduse` for each listed path
  the module lacks, formats the candidate, checks the affected modules
  again, and compares every claim and the resolution of every reference
  outside the subject before and after on the exact formatted candidate. By
  construction the header, the guards, and every other declaration are
  unchanged, so a body edit cannot change a claim; the comparison is the
  check of that construction, and a difference is a mismatched plan
  ([Q18](#q18-overlapping-edits-and-the-order-of-a-plan)). A change to a
  signature or a guard is a claim change and has no operation in this
  milestone. This is a typed operation on a target identity with a typed
  record, which is what the chapter asks of every edit; the unit of change
  is one body, the smallest thing that can hold a defect and change no
  contract. The alternative is a typed replacement of a whole declaration
  (headers included), which lets a repair change a claim and so needs the
  claim comparison to decide what the type of the operation decides here.
- **Depends.** Steps 01, 13, 15, 21, 35, 38. Rows `V2-TOOL-edit-body-*`,
  `ATK-edit-body-other-subject`, `ATK-edit-body-resolution-guard`.

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
  and the layout between tokens replaced by one rule: one space between two
  adjacent tokens, except none after an opener `(`, `[`, `{`, none before a
  closer `)`, `]`, `}`, none before `:` and one after it, and none between
  a postfix operand and its application `(` or its `.`, so that
  `code: ExitCode`, `ExitCode(0u8)`, `f(g(x) y)`, and `user.name` read as
  written. Every token keeps the spelling the formatter gives it, a doc
  string and a string literal included, so a line ending inside a string or
  a doc string is data and stays; the text has no line ending outside a
  token. The text ends at the node's closer. The identity therefore
  promises exactly the equivalences of this normal form, no more
  ([Q6](#q6-what-each-hash-survives)). The alternative is a decoded
  normal form, in which every literal is written as its value and every
  omitted section as its default, which promises more equivalences and
  needs a second printer beside the formatter.
- **Depends.** Steps 03, 04, 09. Rows `V2-TOOL-claim-comment-same`,
  `V2-TOOL-claim-layout-same`, `V2-TOOL-claim-vector`,
  `ATK-witness-vector-framing`, and every row of step 03.

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
- **Ruling.** Fill by kind, every omitted part written as the value it
  stands for, so two spellings of one documented default hash alike:
  - `fn`, `proc`, member `fn` and `proc`: `signature` (the signature record
    with the row, empty when omitted, and the receiver slot), `defaults`,
    `guards` (`@requires`, `@ensures`), no `representation`.
  - `type`: `representation` (the resolved visibility), `signature` (its
    binders and the representation type as `typeInfo`), `guards` (the
    `@where` rows), `implementations` (every nested `impl`, sorted), empty
    `defaults`.
  - `case`: `signature` whose `typeInfo` is the record of its fields, or
    the empty tuple for a fieldless case.
  - `interface`: its claim is its contract: `signature` holds its binders
    with their bounds and, as `typeInfo`, the interface's own type, so a
    changed bound changes the claim; `kind` tells an operation interface
    from a plain one; and the shape of every required member and
    associated type is in the claim, under the field
    [Q41](#q41-where-an-interfaces-required-members-live-in-the-claim-record)
    gives it, so adding a member or an associated type changes the
    interface's claim. Each member keeps a claim of its own, so a member's
    claim stays queryable; a default member's body is in witnesses only.
  - associated type: `signature` absent; `kind`, `access`.
  - module value: `signature` with `binders: []` and the written type.
  - `test`: `kind: @test`, `signature` with `Fn({} Void)` or
    `Fn({TestWorld} Void)` and the written row as `effects`, no guards.
  - the entry of a target: `targets` with one fact per target; every other
    declaration writes `targets: []`.
  A `doc` string is in no claim: the chapter's list of what a claim covers
  names signatures, defaults, anchors, representations, guards,
  implementations, and bindings, and documentation is none of them. The
  order of `given` bounds is in a claim: the signature record lists binders
  "in written order" and binder references index that list. Nothing
  inferred by the checker enters a claim: a claim is the public contract as
  written, resolved to identities. The theory is content addressing of the
  interface, not of the implementation.
- **Depends.** Step 04. Rows `V2-TOOL-claim-kind-*`,
  `V2-TOOL-claim-omitted-row-equals-empty`,
  `V2-TOOL-claim-omitted-representation`,
  `V2-TOOL-claim-interface-bound-changes`,
  `V2-TOOL-claim-interface-member-added`,
  `V2-TOOL-claim-interface-associated-added`,
  `V2-TOOL-claim-interface-default-body-same`.

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
- **Depends.** Step 09. Rows `V2-TOOL-witness-member-edit-changes-owner`,
  `V2-TOOL-witness-member-own-group`.

### Q5. Which references make a witness dependency

- **Anchor.** [05 Snapshot, claim and witness identity](../../../spec/05-tooling.md#snapshot-claim-and-witness-identity)
  ("A dependency is pinned by its witness, so a change to the body of
  anything a declaration resolves to, however far, changes the
  declaration's witness"),
  [04 The standard library](../../../spec/04-programs-and-packages.md#the-standard-library).
- **Question.** A dependency is "one per declaration outside the group that
  a member's body resolves to". The resolver's references bind a written
  path to a declaration. A call through an interface resolves, in source,
  to the interface's member, while the checker selects an implementation's
  member, an instantiated default member, or a conformance a bound relies
  on. Does the selected body count as something the caller resolves to?
- **Program.** `fn f takes {s: Str} returns n: U64 do return Str.length(s)
  endfn`; `fn show(T) takes {x: T} given {T: Display} returns t: Str do
  return Display.render(x) endfn` applied at `I32`.
- **Ruling.** The witness graph has two kinds of edge, and both count.
  First, every reference the hashed text contains that resolution binds to
  a declaration, in the signature, the guards, and the body, to a local
  declaration, a dependency package, or `@std` alike. Second, every
  **checked edge**: a choice the checker makes that the source does not
  spell, exported by the checker's facts
  (step [08](steps/08-semantic-facts.md)) as a list of
  `{from, to, kind}` with `kind` one of `@implementationMember` (the member
  of the `impl` a call through an interface selects, a destination dispatch
  included), `@defaultMember` (a default member instantiated for a
  receiver), `@conformance` (the `impl` a bound of the caller's signature
  relies on), and `@generatedMember` (a member the checker supplies without
  source). The group is the strongly connected component of the union of
  the two edge sets, found by an iterative algorithm, so a cycle closed only
  by a checked edge is one group, and a change to a selected body changes
  the caller's witness. A library change therefore changes the witness of
  everything that names or selects the library, as "however far" implies.
  No set of declarations is exempt. The approval pins the toolchain
  separately only because a toolchain change may change a witness without
  changing any project byte.
- **Depends.** Steps 08, 09. Rows `V2-TOOL-witness-std-reference`,
  `V2-TOOL-witness-dependency-package`,
  `V2-TOOL-witness-signature-reference`,
  `V2-TOOL-witness-selected-body-changes-caller`,
  `V2-TOOL-witness-default-member-edge`,
  `V2-TOOL-witness-conformance-edge`,
  `V2-TOOL-witness-checked-edge-cycle`.

### Q6. What each hash survives

- **Anchor.** [05 Snapshot, claim and witness identity](../../../spec/05-tooling.md#snapshot-claim-and-witness-identity),
  [02 Declared types](../../../spec/02-type-system.md#declared-types).
- **Question.** The chapters say what an anchor, a claim, and a witness
  cover. They do not list, in one place, what each survives: formatting, a
  comment, a rename, a move between modules, a change in a dependency, a
  change in the standard library, a change of toolchain, a change of line
  endings, a change of the file's absolute path or of the platform, two
  spellings of one literal.
- **Program.** One module, hashed after each change.
- **Recommended answer.** **Hannah.** A hash of a normal form identifies
  programs up to the congruence the normal form decides. It must be sound
  (equal identity, equal meaning) and cannot be complete, so the contract
  promises exactly the equivalences of the normal form of
  [Q2](#q2-canonical-source-text) and no other: comments, layout between
  tokens, and whatever the canonical formatter rewrites. Every other
  difference of spelling may change a witness, and the table says so with
  a row each; a claim additionally writes documented defaults as their
  values ([Q3](#q3-what-a-claim-contains-for-each-kind-of-declaration)).
  The alternative is to promise more equivalences (decoded literals, every
  omitted section) by hashing a decoded form, which needs the second
  printer Q2 names.

  | Change | Anchor | Claim | Witness | Workspace revision |
  | --- | --- | --- | --- | --- |
  | a comment; layout between tokens; a line ending between tokens | same | same | same | changes (exact bytes, by design) |
  | a body edit | same | same | changes, and every dependent's | changes |
  | a signature, bound, default, guard, representation, or `impl` choice | same | changes | changes | changes |
  | an escape against the character it denotes, in a string or character literal | same | changes where the literal is in a default or a guard | changes | changes |
  | a line ending inside a string or a doc string | same | changes where the literal is in a default or a guard | changes | changes |
  | an explicit section against its omitted documented default (`performs {}`, `representation` equal to the visibility) | same | same (the default is written as its value) | changes (the text differs) | changes |
  | the indentation of a doc string, with equal processed text | same | same (no claim holds a doc string) | changes | changes |
  | the order of `given` bounds | same | changes | changes | changes |
  | a rename or a move to another module | changes, and every member's | changes, and every claim whose signature names it | changes, and every dependent's | changes |
  | a binding or policy of a target the declaration enters | same | changes (the entry's) | same | changes |
  | a change in a declaration it resolves to or selects, in a dependency or in `@std` | same | same unless a named type's anchor changed | changes | changes |
  | the toolchain | same | same | same unless a library declaration's text changed | changes (the `toolchain` record) |
  | the absolute path of the checkout, the platform, the build directory of the toolchain | same | same | same | same |

- **Depends.** Steps 04, 09, 10. Rows `ATK-identity-path-independent`,
  `ATK-identity-crlf-same-claim`, `V2-TOOL-claim-rename-changes`,
  `V2-TOOL-witness-dependency-change`, `V2-TOOL-revision-crlf-differs`,
  `V2-TOOL-witness-escape-differs`, `V2-TOOL-witness-string-line-ending`,
  `V2-TOOL-witness-explicit-section-differs`,
  `V2-TOOL-witness-doc-indentation-differs`,
  `V2-TOOL-claim-given-order-changes`.

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
  changed binding is reported at its recipe); `files` (list of
  `{sourceId: Str digest: Str}` sorted by `sourceId`, one per file the
  snapshot captures for the `@tests` unit, `digest` the SHA-256 of the
  file's exact bytes); `selection` (list of `Str` test selectors sorted by
  UTF-8 bytes, the acceptance selection); `toolchain` (`Str`, the SHA-256
  of the canonical bytes of the `@toolchain.v2` record). The protected
  evidence is one general rule: everything a test relies on as evidence
  lives in the `@tests` unit, so the whole unit is protected; code in a
  target is code under repair. The document's identity is `sha256:` and the
  SHA-256 of its canonical bytes, which are the file's bytes, so a person
  computes it with any hashing tool
  ([Q40](#q40-the-approval-pin-and---expect-approval)). The writer is the
  command of [Q8](#q8-the-command-that-presents-authority); the reader is
  `check`, which captures the entries of `approvals/` whose name ends
  `.vibon` and does not begin with `.` as `approval` revision records and
  validates each against the snapshot; a `.vibon` name that is no target of
  the project is a fault, and every other entry is ignored. The directory
  `approvals/` is created by hand or by version control: the writer never
  creates a directory, and writing into an absent `approvals/` is
  `@project.ioError` at the document's path. No signature or user identity
  is recorded in this release: the approver is whoever may write the tree,
  version control is the record of who did, and the pin of Q40 is what an
  approved run checks against. A document with an unknown field or a wrong
  shape is rejected under the schema rules. The alternative is a document
  per project holding every target's approval, which makes two targets'
  approvals one file and one identity.
- **Depends.** Steps 10, 14, 16. Rows `V2-GRANT-approval-decode-*`,
  `V2-GRANT-approval-current`, `ATK-approval-no-signature-is-data`,
  `ATK-approve-write-no-directory`.

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
  existing approval (`@none`, `@current`, or `@stale` with what changed)
  with its identity. With `--write`, after a check in which every error
  but the approval diagnostics of this target refuses, it writes the
  approval document of
  [Q7](#q7-the-approval-document-and-who-writes-it) through the
  publisher, to the one destination derived from the target's name. Creating
  or replacing an approval is its own operation: it is never implied by a
  plan's `--write`, and over MCP, in a later milestone, it is its own
  permission, never implied by `--allow-write`. The command acquires
  nothing and creates no grant: the payload is data about recipes, as a
  grant projection is. The payload is `{target, entry, claim, bindings,
  designLimits, approval, written, path}`. The human format prints the same
  facts as text whose wording is not a contract. Capability presentation is
  the theory: an approval names exactly the authority it grants, and the
  presentation is computed from the same `BindingPlan` admission reads.
  The alternative is to present at `run` and write the approval by hand.
- **Depends.** Steps 14, 16. Rows `ATK-approve-*`.

### Q9. The code of a void approval

- **Anchor.** [07 Diagnostics are a language surface](../../../spec/07-diagnostics-and-conformance.md#diagnostics-are-a-language-surface),
  [07 Contract, effect and authority diagnostics](../../../spec/07-diagnostics-and-conformance.md#contract-effect-and-authority-diagnostics).
- **Question.** An approval "is void when the test, the policy, or the
  claim changes". The registry has `@grant.invalidBinding` for "a missing,
  extra, ill-typed, or unapproved grant binding", which covers a changed
  policy, and `@tool.stalePlan` for a plan whose revision no longer
  matches. No code names an approval whose protected files, claim, or
  toolchain changed, or an approval that is not the one a caller expected.
- **Document.** An approval whose `files` digest differs from a module's
  bytes.
- **Recommended answer.** **Hannah.** Add `@project.staleApproval`
  (`@error`, fix capability `@none`), beside `@project.staleLock`: "an
  approval whose entry claim, protected files, selection, or toolchain no
  longer match the snapshot, or an approval that is not the one the caller
  expected: missing, replaced, or void under `--expect-approval`", primary
  span the approval's `format` field when the document exists, else no
  location; related to the changed field of the approval and, for a
  protected file, offset 0 of that file. A changed binding is reported
  once, as `@grant.invalidBinding` at the recipe ("unapproved"), and the
  claim mismatch it also causes is not reported again. Check reports both
  codes, so `test`, `run`, and every edit inherit the refusal. The
  alternative, a widened `@tool.stalePlan`, would make one code mean two
  things.
- **Depends.** Steps 10, 14, 16, 19, 36. Rows `V2-GRANT-approval-stale-*`,
  `V2-GRANT-approval-unapproved-binding`, `V2-GRANT-approval-one-fault`,
  `ATK-approval-expected-*`.

### Q10. The code of an edit the approval forbids

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans)
  ("No fix or format may alter a protected claim, test, or policy outside
  its approval workflow"), [07 Runtime and tool failures](../../../spec/07-diagnostics-and-conformance.md#runtime-and-tool-failures).
- **Question.** A plan that would change a protected file or the entry's
  claim under a current approval "fails with nothing written". The only
  code for a denied tool action is `@tool.actionDenied`, whose row says "an
  MCP or tool action the server's permissions deny".
- **Command.** `vibra --format json edit body @tests.helpers.setup DATA`
  under a current approval.
- **Recommended answer.** **Hannah.** Reuse `@tool.actionDenied`, with its
  row widened to "an MCP or tool action the server's permissions, or an
  approval in force, deny", no location, related to the approval's
  `format` field. One code for "the tool refused to act", whatever
  refused. The result is `@command.diagnostics`, the plan is returned for
  reading with `applied: false`, and `fmt --write` on a protected module
  is refused the same way. The alternative is a new code of the `project`
  domain for an edit an approval forbids.
- **Depends.** Steps 13, 15, 26, 27, 28, 30. Rows
  `V2-TOOL-edit-body-test-denied`, `V2-TOOL-edit-rename-into-tests-denied`,
  `ATK-edit-apply-protected-unchanged`, `ATK-fmt-write-protected-denied`.

### Q11. Whether `run` and `test` need an approval

- **Anchor.** [03 Target admission](../../../spec/03-effects.md#target-admission),
  [M3 README](README.md) exit evidence.
- **Question.** The exit asks that a changed claim "under an old approval"
  be rejected. Nothing says a target must have an approval to run or test.
- **Command.** `vibra run` on a project with no `approvals/` directory.
- **Ruling.** An approval is optional. With none, and without
  `--expect-approval`, `run` and `test` behave as in M2. With one, the
  rules of [Q9](#q9-the-code-of-a-void-approval) and
  [Q10](#q10-the-code-of-an-edit-the-approval-forbids) bind every command.
  With `--expect-approval` ([Q40](#q40-the-approval-pin-and---expect-approval))
  the named approval must be in force. Requiring one everywhere would make
  every M2 case stale.
- **Depends.** Step 10. Rows `V2-GRANT-approval-absent-runs`,
  `V2-GRANT-approval-body-edit-current`,
  `ATK-approval-expected-absent-without-option`.

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
- **Depends.** Steps 07, 15. Rows `V2-TOOL-edit-plan-document-revision`,
  `ATK-edit-apply-stale-document`.

### Q13. The expected syntax identity and the source fingerprint of an edit

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans).
- **Question.** An edit has "the expected syntax identity, and a source
  fingerprint". Neither is defined.
- **Recommended answer.** `syntaxIdentity` is the `nodeId` of the replaced
  node, `<sourceId>#<start>-<end>`, with its `syntaxKind` atom, as the
  position query spells them; `fingerprint` is `sha256:` and the SHA-256 of
  the replaced bytes. Applying checks both against the recaptured document,
  so a plan computed against one revision never edits another's bytes.
- **Depends.** Step 07. Rows `V2-TOOL-edit-plan-fields`,
  `ATK-edit-apply-fingerprint-mismatch`.

### Q14. A stale edit on the command line

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans),
  [v0.0.2 release gate](../README.md#release-gate), step 3.
- **Question.** Over MCP a write "needs a plan returned earlier", so a
  stale plan is natural. On the command line a mutating command previews
  and applies in one run, so the agent of the release gate cannot produce
  the "stale edit" the gate asks it to show rejected.
- **Command.** `vibra --format json edit body @gate.tally.count DATA
  --write` after another file changed.
- **Recommended answer.** **Hannah.** Add one option to every mutating
  command: `--expect-revision DIGEST`, the workspace revision the caller
  planned against. Without it `--write` applies to the current snapshot.
  With it, a different revision is `@tool.stalePlan` with nothing written,
  `applied: false`, and the plan the caller would have got. Independently
  of the option, the publisher validates the whole read set at the commit
  point ([Q17](#q17-what-edit-may-write-and-what-confines-it)). The
  repair scenario's registration requires the option of the runner, and
  the grader checks that the applied command carried the revision of the
  preview. This is optimistic concurrency: the caller states the version it
  read, and the writer refuses on a mismatch. The alternative is to require
  the option on every `--write`, which makes every write a two-command
  exchange.
- **Depends.** Steps 01, 15, 36, 38. Rows `ATK-edit-apply-stale-document`,
  `ATK-edit-apply-expect-revision`, and the row "expect-revision parsed"
  of step 01.

### Q15. Several documents cannot be published atomically on Windows

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans)
  ("publish all files, or none"; "A backend that cannot write several
  files atomically MUST refuse before writing"),
  [07 Runtime and tool failures](../../../spec/07-diagnostics-and-conformance.md#runtime-and-tool-failures)
  (`@tool.unavailable`), [05 Versioned envelope and results](../../../spec/05-tooling.md#versioned-envelope-and-results)
  (`@command.unavailable`).
- **Question.** Windows offers no multi-file commit: no documented API
  makes two replacements visible in one step, and a log that redoes the
  remaining replacements after a crash gives recovery, not isolation from a
  reader between two replacements. Is a plan over several documents
  refused, or applied document by document?
- **Command.** `edit rename @app.model.User Person --write` with references
  in two other modules.
- **Ruling.** Refused, as the sentence says. A plan over several documents
  is computed, checked, and returned in full as a preview; `--write` on it
  is refused before anything is written, as `@tool.unavailable` with the
  command-line context and the result `@command.unavailable`, the code the
  chapters give to a valid operation outside what the tool supports, which
  is the reading [Q16](#q16-applying-an-edit-off-windows) gives `--write`
  off Windows; the inventory and the exit record list "a `--write` over
  several documents" under what stays unavailable. A rename confined to one
  module still writes. One document is published by the primitive of step
  [02](steps/02-publish-primitive.md): a new file created exclusively under
  a reserved name in the destination's directory, written, flushed, then
  renamed over the destination relative to the held directory handle with
  replace-if-exists and POSIX semantics. Its contract has three parts kept
  apart: what holds by construction (the destination name never refers to
  a partial document, because partial bytes exist only under the reserved
  name); what Microsoft documents (no atomicity or durability of the
  rename); and the platform assumption the release rests on (NTFS logs a
  rename as one metadata transaction), recorded in the
  [architecture](architecture.md#3-the-edit-path-on-real-files) and
  repeated in the exit record. No journal, no replay, and no on-disk
  record other than the reserved file and the write lock of
  [Q17](#q17-what-edit-may-write-and-what-confines-it) exist.
- **Depends.** Steps 02, 15, 26. Rows `ATK-publish-*`,
  `ATK-edit-apply-several-documents-unavailable`,
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
  attack rows do. The alternative is a POSIX adapter in this release.
- **Depends.** Steps 02, 15. Rows `vibra-workspace`
  `edit_apply::not_run_off_windows` and every `ATK-edit-apply-*`,
  `ATK-publish-*`.

### Q17. What `edit` may write, and what confines it

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans)
  ("Edits reach local modules and never vendored dependencies"; "A plan
  that is stale … fails with nothing written"),
  [05 MCP server](../../../spec/05-tooling.md#mcp-server) (paths confined).
- **Question.** A `@path` dependency lies beneath the workspace root and is
  not vendored. May an edit reach it? Through which path check are writes
  confined, given that discovery follows no link below a root? And against
  which writers is a plan's revision check sound?
- **Ruling.** Three rules. **What.** Source publication admits one
  destination, a module of the root project's targets or of `@tests`,
  decided from the module list of the snapshot captured at the commit
  point, never from the plan's path; a subject in a `@path` dependency, in
  `@std`, or in `project.vibon` is invalid input, and `project.vibon`, the
  lock, a dependency, and anything under `approvals/` are not source
  documents. Writing an approval is a separate typed entry point whose
  destination is derived from the target's name alone. **How.** The
  publisher opens the workspace root once as a held directory and reaches
  the destination's directory from it one real name at a time through the
  scoped adapter, so a link anywhere on the path, a component whose real
  name differs, or a path with a `..` is refused before any write; it
  creates, writes, and renames only under the reserved name of
  [Q15](#q15-several-documents-cannot-be-published-atomically-on-windows).
  An architecture test lists every call that creates, writes, renames, or
  removes a file in `vibra-workspace` and allows the publisher and the lock.
  **When.** A write validates the whole read set at the commit point, not
  only the written document: the publisher takes the exclusive workspace
  write lock, `.vibra-write-lock` at the root, created exclusively through
  the held root and removed when the command ends; captures the workspace
  again; refuses as `@tool.stalePlan` unless the second capture's workspace
  revision equals the plan's, which compares every admitted input, the
  edited document and every unedited one; checks the destination's bytes
  against the plan's document revision and each edit's fingerprint; and
  only then renames. A lock that is present is never broken: the command is
  `@command.operationalFailure` with `@project.ioError` at the lock's path,
  and the human format names the file for a person to remove. The lock
  governs every vibra writer: `fmt --write`, every `edit … --write`,
  `lint --write`, and `approve --write`. It cannot govern an editor or
  another tool; against those the window is narrowed to the interval
  between the second capture and the rename, and the result names the
  revision that was validated as `resultingRevision`'s predecessor, the
  plan's `workspaceRevision`. A read-only command takes no lock and writes
  nothing. The theory is optimistic concurrency with validation of the read
  set at commit: a transaction that read any input another writer changed
  is aborted, which the single workspace revision decides in one
  comparison.
- **Depends.** Steps 02, 15, 21. Rows `ATK-edit-apply-symlink-refused`,
  `ATK-edit-apply-alias-refused`, `V2-TOOL-edit-dependency-subject-invalid`,
  `ATK-edit-writers-enumerated`, `ATK-edit-apply-unedited-input-changed`,
  `ATK-edit-apply-two-writers-one-revision`,
  `ATK-edit-apply-lock-present`, `ATK-read-only-tree-unchanged`.

### Q18. Overlapping edits and the order of a plan

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans)
  ("A plan that is stale, ambiguous, overlapping, ill-typed, mismatched, or
  that changes a policy without approval fails with nothing written").
- **Question.** "Edits are ordered by path and span and do not overlap."
  The command line hands no plan in, so an overlapping plan can only be
  computed by the tool. A body edit whose comparison of claims and
  resolutions outside the subject finds a difference is "mismatched" in
  the same sentence.
- **Ruling.** An overlapping, ambiguous, or mismatched plan is a typed
  refusal of the plan, as the sentence lists it, never a trap: the result
  is `@command.diagnostics`, the plan is returned with `applied: false`,
  nothing is written, and the diagnostic is the code of
  [Q42](#q42-a-code-for-an-invalid-plan). Over MCP, later, a supplied plan
  that overlaps is invalid input.
- **Depends.** Steps 07, 13. Rows `ATK-edit-plan-overlap-refused`,
  `ATK-edit-body-resolution-guard`.

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
- **Depends.** Steps 13, 26. Rows `V2-TOOL-edit-rename-*`,
  `V2-TOOL-edit-body-postconditions-witness-only`.

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
  import is not organizing: it changes what the module names. The
  alternative is to leave `edit imports` unavailable in this release.
- **Depends.** Step 27. Rows `V2-TOOL-edit-imports-*`.

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
- **Depends.** Steps 28, 30. Rows `V2-TOOL-edit-fix-*`, `V2-TOOL-lint-*`.

### Q22. What `lint` is

- **Anchor.** [05 Command surface](../../../spec/05-tooling.md#command-surface),
  [05 Command payloads](../../../spec/05-tooling.md#command-payloads) (`lint`: `accepted`, `plan`).
- **Question.** `lint [TARGET] [--write]` has a payload and covers the same
  targets as `check`, and nothing else is said of it.
- **Recommended answer.** **Hannah.** `lint` is `check` followed by one
  plan that holds every safe fix of the check's diagnostics, over the
  modules `check` covers, or `null` when there is none; `--write` applies
  the plan, which is refused as unavailable when it spans several
  documents ([Q15](#q15-several-documents-cannot-be-published-atomically-on-windows)).
  `accepted` is the check's verdict. A target with no safe fix and no error
  is `@command.ok` with `plan: null`. The alternative is `lint` as a check
  with warnings as errors and no plan.
- **Depends.** Step 30. Rows `V2-TOOL-lint-*`.

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
  call-site type arguments exist. The alternative is a whole `@value.v2`
  document per argument, with the type identity the agent must first query.
- **Depends.** Step 11. Rows `V2-TOOL-eval-arg-*`.

### Q24. A bare `requires` on the subject of `eval`

- **Anchor.** [05 Program arguments, execution and evaluation](../../../spec/05-tooling.md#program-arguments-execution-and-evaluation)
  ("An unproved static obligation is an error for eval as for check"),
  [02 Bare guards](../../../spec/02-type-system.md#bare-guards) GUARD-1.
- **Question.** A call of a function with a bare `requires` is a site
  where the obligation is discharged. `eval` is such a call with constant
  operands. Is the predicate evaluated on the arguments, and what is the
  result when it is false, when it is not established, and when its
  discharge reaches a limit?
- **Command.** `eval @app.math.half --arg 0i32` where `half` requires
  `Ordered.greater(n 0i32)`.
- **Ruling.** The call is the checked call boundary, discharged as GUARD-1
  discharges a call with constant operands: a true predicate admits the
  evaluation; a false one is `@contract.refuted` at the predicate, related
  to the argument, result `@command.diagnostics`, and nothing runs; a
  predicate the rules do not establish is `@contract.unproved` and nothing
  runs; a discharge the derivation limit stops is `@tool.limitExceeded` at
  the predicate, status `unknown`, and nothing runs. The `ensures` of the
  subject are the checker's business before any eval, as for `check`. A
  workspace whose check has an error is `@command.diagnostics` with no
  evaluation, as `test` already does.
- **Depends.** Step 11. Rows `V2-TOOL-eval-requires-refuted`,
  `V2-TOOL-eval-requires-passed`, `V2-TOOL-eval-requires-unproved`,
  `V2-TOOL-eval-requires-exhausted`, `V2-TOOL-eval-check-error`.

### Q25. The result of an evaluation that cannot be shown

- **Anchor.** [05 Program arguments, execution and evaluation](../../../spec/05-tooling.md#program-arguments-execution-and-evaluation)
  ("A result that cannot be shown is an explicit tool outcome, not a trap";
  "It MUST NOT run a procedure or a plan"),
  [05 Command payloads](../../../spec/05-tooling.md#command-payloads) (`eval`).
- **Question.** The payload has `value` or `observationError`. Which
  command result and status does an observation error carry, and is a
  diagnostic added? And what happens when a `fn` subject builds a plan
  through a value and the evaluation reaches it?
- **Ruling.** `@command.ok`, status `passed`, `value: null`,
  `observationError` set, and no envelope diagnostic: the evaluation
  completed and only its showing failed, exactly as a test item records an
  observation failure in its own record. The subject's static type
  already tells whether a value can be shown, so a subject whose result
  type is not `Encodable` is refused before evaluation as invalid input,
  and the observation error arises only for an interface value that holds
  a callable, a plan, or a grant. A `fn` cannot `await`, so an evaluation
  never runs a plan; a `fn` that builds a plan and returns it holds a value
  with no encoding, which is the observation error, and a `fn` whose body
  reaches a form the pure interpreter does not run is a defect
  (`@command.trap`), which the row shows cannot be reached from a checked
  `fn`.
- **Depends.** Step 11. Rows `V2-TOOL-eval-unencodable-type-invalid`,
  `V2-TOOL-eval-interface-value-observation-error`,
  `V2-TOOL-eval-plan-built-not-run`.

### Q26. The limits of `eval` and their names

- **Anchor.** [05 Program arguments, execution and evaluation](../../../spec/05-tooling.md#program-arguments-execution-and-evaluation) ("Limits"),
  [m2/fix-e](../m2/exit.md#what-stays-unavailable).
- **Question.** `m1Default` gives one compile-time evaluation 10,000,000
  reductions and names `memory` and `sourceNesting` in `limits.reached`;
  the fix package E names the reached limits of a compile-time evaluation
  and an obligation. `eval` "uses the same interface" and its names are
  not stated, nor its memory budget, nor a bound on the size of its output.
- **Ruling.** `eval` runs under the same 10,000,000 reductions and reports
  the limit under the name fix E gives a compile-time evaluation; this plan
  writes that name as `evaluationReductions`, and step 11 depends on fix E
  having landed so that one spelling exists. Its memory budget is the
  interpreter's default, 256 MiB, reported as `memory`. Its output is
  bounded by [Q28](#q28-the-output-limit-of-every-command). A reached work
  limit is `@tool.limitExceeded` with no location, status `unknown`,
  `complete: false`, and the result `@command.diagnostics` with that one
  diagnostic, as `check` reports a limit that stops an obligation; the
  result table gives `@command.operationalFailure` to a tool failure and to
  memory exhaustion only, so a reached work limit is not one. A value
  changed between two captures of one `eval` is impossible by construction,
  since one call takes one capture, and the row shows two calls see two
  revisions.
- **Depends.** Steps 11, 18. Rows `V2-TOOL-eval-reductions-limit`,
  `V2-TOOL-eval-memory-exhausted`, `V2-TOOL-eval-value-changed-between-calls`.

### Q27. A work limit for a test item

- **Anchor.** [05 Tests and selection](../../../spec/05-tooling.md#tests-and-selection),
  [05 Program arguments, execution and evaluation](../../../spec/05-tooling.md#program-arguments-execution-and-evaluation) ("Every executing command runs under published, versioned limits on work, memory, and output"; "a reduction being the evaluation of one checked node"),
  [07 Persistent observations](../../../spec/07-diagnostics-and-conformance.md#persistent-observations) ("A tool limit is expected as unknown, with the limit profile").
- **Question.** A test item has a memory budget and no work limit, so a
  test that does not terminate never ends. The test entry of the
  interpreter takes a memory budget alone and builds its machine with no
  bound on reductions. Which limit, which unit, which charge for an
  operation of the test world, which item result, and which suite verdict
  when items pass, fail, and exhaust together?
- **Recommended answer.** **Hannah.** `m1Default` gives each test item
  100,000,000 reductions, ten compile-time evaluations. One unit of work is
  one reduction as the chapter defines it, the evaluation of one checked
  node. An operation of the test world is charged a stated, deterministic
  amount, 1,024 reductions for the call and one more per byte of the data
  it moves (the bytes of a written or read file, a stream, a variable's
  value); no clock enters a program result. An item that reaches the limit
  ends `@test.exhausted`, whose meaning widens to "the item exhausted its
  memory or its work limit", with the diagnostic `@tool.limitExceeded` (no
  location) attributed to the item, `limits.reached` naming
  `testReductions`, `complete: false`. The suite's verdict follows the
  chapter's precedence unchanged: trap, operational exhaustion,
  unavailable, test failed, ok, so a suite with a failed item and an
  exhausted item is `@command.operationalFailure`, and both items keep their
  results; the persisted `@testRun.v2` records the exhausted item with its
  limit as unknown. No new item result atom: exhaustion of a budget is one
  outcome. The one change this needs in `vibra-interp`, a work limit on
  the test entry, is owned by step [18](steps/18-test-limits.md). The
  alternative is a count of host operations as a second, separate limit.
- **Depends.** Step 18. Rows `V2-PROJECT-test-reductions-limit`,
  `V2-PROJECT-test-reductions-other-items-run`,
  `V2-PROJECT-test-exhausted-after-failed`,
  `V2-PROJECT-test-world-operations-charged`.

### Q28. The output limit of every command

- **Anchor.** [05 Versioned envelope and results](../../../spec/05-tooling.md#versioned-envelope-and-results)
  ("When output would exceed its limit, the tool emits a bounded, valid
  document with status `unknown`. It never emits truncated JSON and never
  silently omits facts").
- **Question.** No limit on output is published, and no chapter says what
  a bounded envelope looks like when the excess is a scalar (`fmt.text`,
  `eval.value`, a captured `stdout`, one diagnostic's message) that no list
  truncation can shrink, or when the command has already committed a write.
- **Recommended answer.** **Hannah.** `m1Default` bounds the JSON envelope
  at 16 MiB, named `outputBytes`. The renderer writes into a counting sink
  and stops at the bound; it never holds an unbounded buffer. When the
  bound is reached, the command emits one frozen **exhausted envelope**
  instead: the envelope with status `unknown`, result
  `@command.diagnostics`, one diagnostic `@tool.limitExceeded` with no
  location, `limits.reached: ["outputBytes"]`, `complete: false`, and the
  command's exhausted payload, which is its payload with every list empty,
  every string that comes from the program or the source empty or `null`
  as its field admits, and the fields that state what was committed kept:
  `applied` and `resultingRevision` of an edit, `written` and `path` of
  `fmt` and `approve`. Every exhausted payload is schema-valid and fits
  under any published limit by construction, since nothing in it grows with
  the input. No list is truncated: an incomplete list would be a fact
  silently shortened, while `complete: false` with no list is an outcome
  the caller can act on by narrowing the query. The alternative is to
  truncate the longest list first and fall back to the exhausted envelope
  only for a scalar.
- **Depends.** Steps 01, 15, 25, 29. Rows `ATK-output-bound-oversized-scalar`,
  `ATK-output-bound-oversized-item`, `ATK-output-bound-applied-edit`,
  `V2-TOOL-query-expand-bounded`, `V2-TOOL-query-index-bounded`.

### Q29. A time limit

- **Anchor.** [05 Program arguments, execution and evaluation](../../../spec/05-tooling.md#program-arguments-execution-and-evaluation),
  [06 Activations and memory](../../../spec/06-runtime.md#activations-and-memory) ("Limits on work, time, and output that a tool or host enforces from outside likewise give an explicit, incomplete outcome").
- **Question.** The brief asks every M3 service for a stated limit on time.
  The chapter's published limits are on work, memory, and output.
- **Ruling.** No wall-clock limit in this release: a limit on time makes
  an observation depend on the machine, which the corpus cannot hold, while
  a reduction count is deterministic, and the test world's operations are
  charged in reductions ([Q27](#q27-a-work-limit-for-a-test-item)). Work
  and memory bound every evaluation and every test item, and the checker's
  own work is bounded by nothing but its input, which the M2 exit records
  as a gap (slow checks) that M3 does not close. The exit record of M3
  says so.
- **Depends.** Step 40 (the exit record).

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
  expectation is JSON. The alternative is a VIBON projection of the result
  for the corpus alone, which would be a second form of one document.
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
  `tree`, `project`, `edit`, one of `body`, `rename`, `imports`, `fix`,
  `subject`, `newName`, `path`, `data`, a file holding the body record;
  expectation `editPlan`, a JSON file compared as the query result is).
  Both observe a preview; no case writes. The `approve` presentation
  stays a host test, as `help` is. The `help` inventory's `operations` of
  `eval` and `edit` follow from the table. The alternative is to keep the
  table closed and hold every such row as a `vibra-cli` host test.
- **Depends.** Steps 01, 11, 13. Every `V2-TOOL-eval-*` and
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
  the resolved declaration; `type` holds `{expectedType, observedType}`;
  `effects` the effects value; `visibility` `{access, representation}`;
  `diagnostics` the diagnostics whose primary span contains the position or
  lies in the entity; `diagnostic` the registry entry of a code subject;
  `index` the index. Every semantic field inside these members is
  `{status, value}` with its own status, as the chapter says of the
  envelope. With no include, `metadata` holds `{nodeId, identity,
  resolvedKind}` only. The semantic envelope is the union of these members
  with the envelope's own fields, so one schema, `urn:vibra:schema:v2:query`,
  describes the payload with each member optional; each member's shape is
  one `$defs` entry in its own schema file, so the step that produces a
  member owns its schema. `resolvedKind` is `position`, `entity`,
  `diagnosticCode`, or `workspace`; `identity` is the `{kind, canonical}`
  of the entity or of the declaration a position lies in, else `null`. The
  alternative is two payload shapes, one per record of the chapter, chosen
  by the subject's kind.
- **Depends.** Steps 01, 05, 12. Every `V2-TOOL-query-*` row.

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
- **Depends.** Steps 25, 29. Rows `V2-TOOL-query-expand-*`.

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
  entry. The effects of a body are finalized after the body's verdict, so a
  rejected body's `performed` is `unavailable`, never a partial exact row.
- **Depends.** Step 17. Rows `V2-TOOL-query-effects-*`.

### Q36. A query over a snapshot the limit or a capture fault stops

- **Anchor.** [05 Program arguments, execution and evaluation](../../../spec/05-tooling.md#program-arguments-execution-and-evaluation)
  ("No later stage runs on the snapshot that holds such a document"),
  [05 Versioned envelope and results](../../../spec/05-tooling.md#versioned-envelope-and-results)
  ("`workspaceRevision` is `null` only when no snapshot was taken").
- **Question.** `check`, `test`, and `run` report a nesting limit alone.
  `query` is not named. A workspace whose project cannot be captured has no
  revision; one whose module is over the limit was captured and has one.
- **Ruling.** `query` is a later stage: over a snapshot with a document
  past the limit it reports the limit alone, status `unknown`, the empty
  payload, and `workspaceRevision` is the captured snapshot's revision,
  since a snapshot was taken; `null` is reserved for a refusal before the
  capture and for the contract's own exceptions (`help`, invalid input, a
  code subject). A capture failure is `@command.diagnostics` with the
  capture's diagnostics, the empty payload, and `workspaceRevision: null`,
  as `run` already answers. A position query of a file that is not a
  module of the snapshot is invalid input.
- **Depends.** Step 05. Rows `V2-TOOL-query-nesting-limit`,
  `V2-TOOL-query-capture-fault`, `V2-TOOL-query-position-not-a-module`.

### Q37. The diagnostic-code subject

- **Anchor.** [05 Index records](../../../spec/05-tooling.md#index-records) (the last paragraph),
  [07 Diagnostics are a language surface](../../../spec/07-diagnostics-and-conformance.md#diagnostics-are-a-language-surface).
- **Question.** "A diagnostic code used as a query subject reads the
  compiler's registry." What does the payload hold, and does it take a
  snapshot?
- **Ruling.** `resolvedKind: diagnosticCode`, `identity:
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
  grammar can drift from the reader. The alternative is a `vibra reference`
  command of the surface, which the chapter does not list.
- **Depends.** Steps 31, 32. Rows `reference::committed_files_are_generated`,
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
  `examples/gate/` with its tests under an approval of its `@gate` target,
  whose identity the registration pins outside the repair copy. The seeder
  is a person or a model session that is not the runner's; it changes the
  body of exactly one declaration of the code under repair so that exactly
  one registered test fails and `check` still passes, records the seed in
  a sealed file the runner never reads, and commits. The runner is one
  model session with a fixed system prompt, through a wrapper that admits a
  command by a list of command and operand shapes (not by prefix), supplies
  `--format json`, `--workspace`, `--expect-approval` with the pinned
  identity, and `--expect-revision` from the preview it relayed last,
  refuses `approve --write` and every other program, and logs every command
  and envelope. Budget: at most 40 runner commands and at most 3 applied
  edits; the grader's commands have their own budget and are not counted.
  Pass rule: the grade is of the loop, read from the runner's own ordered
  observations in the log: the failure read (`test` of the selector,
  failed), the declaration queried with its type, contracts, and effects
  (`query` with `signature`, `type`, `effects` and the obligations), a
  preview (`edit body`), an apply carrying the preview's revision and the
  same operation and subject, and the selected test rerun (passed); then an
  independent whole-suite pass by the grader; then every protected byte and
  the approval file's bytes equal to the frozen baseline; then every
  envelope valid. Every negative control (stale edit, replaced test,
  changed policy, changed claim) verifies zero writes by a byte comparison
  of the tree and has a matching positive control that writes; all pass
  before the run. Exclusions: a seed the controls show does not fail its
  test is void and reseeded before any run; a provider failure is retried
  once and recorded. The record is dated and says what the scenario does
  not show. The alternative is a grade of the end state alone (the
  selected test passes and the approval is current), which a run that
  rewrote protected bytes and renewed the approval would also pass.
- **Depends.** Steps 35, 36, 37, 38, 39.

## Approvals, plans, and identities: what the rulings leave open

### Q40. The approval pin and `--expect-approval`

- **Anchor.** [05 Snapshot, claim and witness identity](../../../spec/05-tooling.md#snapshot-claim-and-witness-identity)
  ("A report says which category changed and against which approved
  revision"), [05 Versioned envelope and results](../../../spec/05-tooling.md#versioned-envelope-and-results)
  (the closed envelope), [05 Command payloads](../../../spec/05-tooling.md#command-payloads) (`test`),
  [v0.0.2 release gate](../README.md#release-gate) step 3.
- **Question.** An approval document inside the tree shows only that the
  tree agrees with itself: whoever can write a protected file can write a
  fresh approval of it. Integrity needs a reference the subject cannot
  write (the reference-monitor argument; separation of duty in the
  Clark–Wilson model). Where does that reference live, how does a command
  name the approval it checked under, and how does a test result say
  whether it was the acceptance run?
- **Command.** `vibra --format json test @tests.main::"counts" --expect-approval sha256:…`.
- **Recommended answer.** **Hannah.** The approval document stays in the
  tree as canonical data, which version control reviews; its identity is
  `sha256:` and the SHA-256 of its canonical bytes
  ([Q7](#q7-the-approval-document-and-who-writes-it)). The envelope gains
  one field after `limits`: `approvals`, a list of `{target, identity}`
  for every approval in force for the checked snapshot, empty when none is
  or when no check ran, so every result names the approval it checked
  under or says none is in force. `check`, `test`, `run`, and every
  mutating command take `--expect-approval DIGEST`, at most once; with it,
  an approval that is missing, has another identity, or is void refuses
  the command as `@project.staleApproval` ([Q9](#q9-the-code-of-a-void-approval))
  before it does anything else, so a pin kept outside the tree (a
  registration, a script, a reviewer's note) decides. Without the option an
  approval stays optional ([Q11](#q11-whether-run-and-test-need-an-approval)).
  The `test` payload gains `approvals`, a list of `{target, identity,
  selection}` with `selection` one of `@equal` (the run's selection is the
  approved one), `@covers` (a superset, the whole suite included), `@part`
  (a subset), or `@other`, so a narrower rerun is ordinary and is never
  mistaken for the acceptance run; a change of the stored selection changes
  the approval's identity, which the pin catches. The alternative is to
  keep the identity out of the envelope and let the repair scenario's
  grader compare the approval's bytes alone.
- **Depends.** Steps 01, 10, 18, 35, 36, 39. Rows `ATK-approval-expected-*`,
  `V2-GRANT-approval-selection-*`, `ATK-approval-envelope-names-identity`.

### Q41. Where an interface's required members live in the claim record

- **Anchor.** [05 Snapshot, claim and witness identity](../../../spec/05-tooling.md#snapshot-claim-and-witness-identity)
  (the `@claim.v2` table; "public signatures with their binders and
  bounds").
- **Question.** [Q3](#q3-what-a-claim-contains-for-each-kind-of-declaration)
  rules that an interface's claim covers the shape of every required member
  and associated type, since adding one changes what an implementation must
  supply while the interface's anchor stays. The `@claim.v2` record has no
  field that can hold them: `signature` is one record, `implementations`
  are `impl` blocks, `guards` are predicates.
- **Document.** The claim of `interface P fn render takes {value: Self}
  returns t: Str endfn endinterface`, then with `type Item` added.
- **Recommended answer.** **Hannah.** `@claim.v2` gains one optional field
  after `implementations`: `members`, a list of `{name: Str kind: Atom
  signature: G}` sorted by the UTF-8 bytes of `name`, present for an
  interface only, one entry per abstract member and associated type, with
  `kind` the declaration kind atom and `signature` the member's signature
  record over the interface's binders (absent for an associated type). A
  default member is listed by name and kind with its signature, and its
  body is not in the record. The chapter's claim table gains the row. The
  alternative is to hash the members' claim identities into the
  interface's `signature.typeInfo` as the interface's type, which hides
  the shape behind a digest a reader cannot inspect.
- **Depends.** Step 04. Rows `V2-TOOL-claim-interface-member-added`,
  `V2-TOOL-claim-interface-associated-added`,
  `V2-TOOL-claim-interface-default-body-same`.

### Q42. A code for an invalid plan

- **Anchor.** [05 Transactional edit plans](../../../spec/05-tooling.md#transactional-edit-plans)
  ("A plan that is stale, ambiguous, overlapping, ill-typed, mismatched, or
  that changes a policy without approval fails with nothing written"),
  [07 Runtime and tool failures](../../../spec/07-diagnostics-and-conformance.md#runtime-and-tool-failures).
- **Question.** [Q18](#q18-overlapping-edits-and-the-order-of-a-plan)
  rules that an overlapping, ambiguous, or mismatched plan is a typed
  refusal, never a trap. The registry has `@tool.stalePlan` for a stale
  one and `@tool.actionDenied` for a denied one, and no code for the three
  other words of the sentence.
- **Document.** A body edit whose listed `use` collides with no visible
  name and yet changes the resolution of a reference outside the subject
  (reachable only with the collision check bypassed).
- **Recommended answer.** **Hannah.** Add `@tool.invalidPlan` (`@error`, fix
  capability `@none`) beside `@tool.stalePlan`: "a plan that is ambiguous,
  overlapping, or mismatched: an edit that would change more than its
  operation names", no location. The alternative is `@command.invalidInput`
  with the empty payload, which loses the plan the caller would need to see
  why.
- **Depends.** Steps 07, 13. Rows `ATK-edit-plan-overlap-refused`,
  `ATK-edit-body-resolution-guard`.

### Q43. Whether the toolchain record names the runner

- **Anchor.** [05 Snapshot, claim and witness identity](../../../spec/05-tooling.md#snapshot-claim-and-witness-identity)
  ("An approval separately protects … the runner, tool, driver, and policy
  versions"; the `@toolchain.v2` table).
- **Question.** The approval pins "the runner, tool, driver, and policy
  versions" through its `toolchain` field, the digest of the `@toolchain.v2`
  record. That record names the tool (`version`, the toolchain release),
  the drivers (`drivers`), and the policy profile (`policy`), with
  `grammar`, `checker`, `stdlib`, and `registry` beside them. No field names
  the runner, the reference interpreter and the test runner, on its own.
- **Recommended answer.** No new field: the runner ships in the toolchain
  release that `version` names, and `registry` names the host registry it
  implements, so a change of the runner is a change of `version`. The
  alternative is a `runner` field, an identity of the interpreter and the
  test runner, for a toolchain that could ship them separately.
- **Depends.** Step 10. Row `V2-GRANT-approval-stale-toolchain`.

## Where the tree contradicts the M3 chapters

These are not open questions. Each is a place where the code as it stands
differs from the specification, with the step that replaces it.

| Where | The tree | The specification | Step |
| --- | --- | --- | --- |
| `crates/vibra-cli/src/inventory.rs` | `query`, `eval`, `edit fix`, `edit rename`, `edit imports`, `lint` are `supported: false` | available in M3 | 05, 11, 28, 26, 27, 30 |
| `crates/vibra-diagnostics/src/registry.rs` | every code's fix capability is `@none` | `edit fix` needs a `@safe` code (Q21) | 28 |
| `crates/vibra-conformance/src/standard.rs` | no handler for `query`; `eval` and `edit` are no operations | `query` observes "the versioned query result" (Q30, Q31) | 01, 05, 11, 13 |
| `crates/vibra-workspace/src/snapshot.rs` | no `approval` record is captured | the revision has `approval` records (Q7) | 01, 10 |
| `crates/vibra-schema/schemas/v2/command-result.json` | payloads for `help`, `fmt`, `check`, `test`, `run` only | every payload of the table | 01 and each command's step |
| `crates/vibra-types/src/check/mod.rs` | `CheckResult` keeps declarations' types and obligations, no fact per node | a query reports expected and observed types at a position | 08 |
| `crates/vibra-workspace/src/format_plan.rs` | `fmt --write` writes a temporary file and renames it with the standard library's path functions, outside the scoped adapter | authority follows the opened directory; the engine writes through the held root | 02, 15 |
| `crates/vibra-interp/src/evaluate.rs` | the test entry builds its machine with no bound on reductions | every executing command runs under a published limit on work (Q27) | 18 |
| `crates/vibra-fs-scope/src/lib.rs` | the only replacement is truncate-and-write on a handle, which "can leave the file empty or holding a prefix" | an interrupted application is never reported as applied; a backend that cannot write atomically refuses (Q15) | 02 |
