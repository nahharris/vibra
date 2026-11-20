# v0.0.2 M2: specification questions

Each question is a place where the specification is silent, says two things,
or decides an observable behaviour by a sentence that can be read two ways.
A step guide never settles one by choosing: its rows are written against the
**recommended answer** below and name the question, so a different ruling
changes exactly the rows listed under **Depends**.

Chapters: [01 source](../../../spec/01-source-language.md),
[02 types](../../../spec/02-type-system.md),
[03 effects](../../../spec/03-effects.md),
[04 programs](../../../spec/04-programs-and-packages.md),
[05 tooling](../../../spec/05-tooling.md),
[06 runtime](../../../spec/06-runtime.md),
[07 diagnostics](../../../spec/07-diagnostics-and-conformance.md).

The five questions that most need a ruling before implementation starts are
Q7, Q13, Q1, Q10, and Q17, in that order: each decides a data type or a
persistent format that several steps share.

## Rows, plans, and callables

### Q1. Narrowing members on a value that is not a sealed grant

- **Anchor.** [03 Narrowing](../../../spec/03-effects.md#narrowing),
  [03 Operation interfaces](../../../spec/03-effects.md#operation-interfaces),
  [06 Compiler intrinsic registry](../../../spec/06-runtime.md#compiler-intrinsic-registry).
- **Question.** `restrict`, `sub`, and `limit` are `external @compiler`
  members of every standard operation interface, and "the broker implements
  them". A package may implement the interface itself. What do the three
  members return for such a value, which the broker never issued?
- **Program.**

  ```vibra
  type Memory of {text: Str}
    impl DirReadText
      proc read
        takes {self: Self at: RelativePath}
        returns outcome: Result(Str FsError)
        do
          return Result.ok(value: self.text)
      endproc
    endimpl
  endtype

  fn narrowed takes {held: DirReadText} returns same: DirReadText do
    return DirReadText.limit(held GrantLimits(
      invocations: Option.some(value: 1u64)
      inputBytes: Option.none()
      outputBytes: Option.none()
    ))
  endfn
  ```

  `narrowed(Memory(text: "x"))` type-checks. `limit` returns `Self`, so it
  has no error to return, and a trap is excluded because the program is
  supported checked source.
- **Recommended answer.** On a held value that is not a sealed grant the
  three rows are the identity: `limit` returns the value, and `restrict`
  and `sub` return `Result.ok` of it. A source model holds no host
  authority, so nothing is widened; the chapter says so in "Narrowing", and
  says that a model which wants limits implements them itself.
- **Depends.** Steps 15, 19, 20, 22. Rows `V2-GRANT-narrow-model-limit`,
  `V2-GRANT-narrow-model-restrict`, `V2-GRANT-narrow-model-sub`.

### Q2. Which members of a standard operation interface an `impl` supplies

- **Anchor.** [03 Operation interfaces](../../../spec/03-effects.md#operation-interfaces),
  [02 Implementations](../../../spec/02-type-system.md#implementations).
- **Question.** An `external @host` member has no body, and "dispatch on a
  sealed receiver selects the host-bound member". For an `impl` written in a
  package, is that member abstract (the `impl` MUST supply it) or a default
  (supplying it is `@type.defaultOverride`)? The same for the three
  narrowing members.
- **Program.** The `Memory` type of Q1 with and without its `read` member,
  and with an added `fn limit` member.
- **Recommended answer.** An `external @host` member is abstract for every
  `impl`: the binding is the implementation of sealed grants only. An
  `external @compiler` member of an interface is a default member with a
  toolchain body, so an `impl` that supplies `restrict`, `sub`, or `limit`
  is `@type.defaultOverride`.
- **Depends.** Steps 08, 19. Rows `V2-EFFECT-impl-host-member-supplied`,
  `V2-EFFECT-impl-host-member-missing`, `V2-GRANT-impl-supplies-limit`.

### Q3. An unawaited plan as a statement

- **Anchor.** [01 Bodies and statements](../../../spec/01-source-language.md#bodies-and-statements),
  [03 Cold plans](../../../spec/03-effects.md#cold-plans-and-performed-rows).
- **Question.** `DirWriteText.write(writer at text)` written as a statement,
  with the `await` forgotten, builds a plan and drops it. The source chapter
  names only a fallible value as a rejected expression statement. Is a plan
  statement accepted, a warning, or an error, and under which code?
- **Program.**

  ```vibra
  proc save
    takes {writer: DirWriteText at: RelativePath}
    returns outcome: Result(Void FsError)
    performs {DirWriteText}
    do
      DirWriteText.write(writer at "x")
      return Result.ok(value: void)
  endproc
  ```

- **Recommended answer.** `@type.mismatch` at the statement: the checker
  already requires an expression statement to have type `Void` or `Never`,
  and an `Intent` is neither. `let _ be plan endlet` discards one on
  purpose. The source chapter states the general rule, which it now only
  implies.
- **Depends.** Step 05. Row `V2-EFFECT-plan-statement-unawaited`.

### Q4. One fault for an `await` that is wrong twice

- **Anchor.** [07 Contract, effect and authority diagnostics](../../../spec/07-diagnostics-and-conformance.md#contract-effect-and-authority-diagnostics).
- **Question.** `@effect.invalidAwait` covers "`await` outside a `proc` or
  test, or of a value that is not a plan". `await 1i32` in a `fn` is both.
  Is the operand checked when the position is already wrong?
- **Program.** `fn f returns n: I32 do return await 1i32 endfn`.
- **Recommended answer.** One `@effect.invalidAwait` at the `await`. In a
  wrong position the operand is not checked; in a right position an operand
  that is not an `Intent` gives the same code, and no `@type.mismatch`.
- **Depends.** Step 05. Rows `V2-EFFECT-await-not-plan`,
  `V2-EFFECT-await-in-fn-not-plan`.

### Q5. Where `@contract.unusedEffect` applies

- **Anchor.** [03 Effect ceilings](../../../spec/03-effects.md#effect-ceilings).
- **Question.** "An interface in a ceiling that the body cannot perform" is
  a warning. Does it apply to (a) an `impl` member, whose row is fixed by
  its contract; (b) the owner root of a default `proc` member; (c) a `Proc`
  or `Intent` type; (d) a `proc` that only builds plans of that interface
  and returns them?
- **Program.** An `impl AuditLog` whose `append` returns `Result.ok` without
  awaiting anything.
- **Recommended answer.** The warning needs a body and a row written on the
  same declaration: a `proc`, an anonymous `proc`, a `test`, and the
  additive `performs` of a default member. It is not reported for (a), (b),
  or (c). For (d) it is reported: building a plan performs nothing.
- **Depends.** Step 09. Rows `V2-CONTRACT-unused-effect-impl-member`,
  `V2-CONTRACT-unused-effect-default-owner`,
  `V2-CONTRACT-unused-effect-plan-only`.

### Q6. The two wordings of `@type.suspiciousRow`

- **Anchor.** [01 Grammar](../../../spec/01-source-language.md#grammar),
  [07 Contract, effect and authority diagnostics](../../../spec/07-diagnostics-and-conformance.md#contract-effect-and-authority-diagnostics).
- **Question.** The source chapter reports it "when a result type is a tuple
  whose every component resolves to an operation interface"; the registry
  says "a result type that is a tuple of operation interfaces". Does it
  apply only to the result operand of a written `Proc` or `Intent` type
  that has no row, or also to a `returns` type, where a tuple of grants is
  an ordinary value?
- **Program.** `fn pair takes {a: DirReadText b: DirList} returns both:
  {DirReadText DirList}` against the type `Proc({Str} {DirWriteText})`.
- **Recommended answer.** Only the result operand of a written `Proc` or
  `Intent` type whose row is omitted, with at least one component. A
  `returns` type is never reported.
- **Depends.** Step 10. Rows `V2-TYPE-NAMES-suspicious-row`,
  `V2-TYPE-NAMES-suspicious-row-returns`.

## Grants, narrowing, and accounting

### Q7. Whether `sub` rebases paths, and what a narrowed grant projects

- **Anchor.** [03 Narrowing](../../../spec/03-effects.md#narrowing),
  [06 Closed operation semantics](../../../spec/06-runtime.md#closed-operation-semantics),
  [06 Audit trace](../../../spec/06-runtime.md#audit-trace).
- **Question.** "`sub` adds a path prefix below the same held directory"
  and "a grant narrowed by `sub` keeps the same acquired anchor and adds a
  path restriction". After `sub(notes ["reports"])`, does a read of
  `["a.txt"]` open `reports/a.txt` (the prefix is prepended) or is it
  denied (the request must itself begin with `reports`)? In the same way,
  is the `prefix` of a policy given to `restrict` on that child relative to
  the child or to the root? And which policy does the child's projection
  hash, when the wire form has no node for a prefix?
- **Program.**

  ```vibra
  let reports be try DirReadText.sub(notes try RelativePath(["reports"])) endlet
  let text be try await DirReadText.read(reports try RelativePath(["a.txt"]))
  endlet
  ```

- **Recommended answer.** `sub` rebases: a request through the child is the
  child's prefix followed by the request, and a policy given to `restrict`
  is relative to the grant it restricts. The broker keeps one effective
  policy per grant in root coordinates: the conjunction (`@allOf`) of every
  ancestor's policy, where a `sub` contributes
  `@anyOf [@fileRule(prefix, @tree, all four actions, @anyName)
  @directoryRule(prefix, @tree, [@list @stat])]` and a `restrict`
  contributes its policy with every rule's `prefix` re-rooted. The
  projection's `policy` is the hash of that normalized conjunction, and
  the `args` of an event hold the path as the program wrote it.
- **Depends.** Steps 14, 15, 17, 22, 31. Rows `V2-GRANT-sub-rebases`,
  `V2-GRANT-sub-then-restrict`, `V2-GRANT-sub-projection`,
  `V2-GRANT-sub-escape-denied`.

### Q8. What `restrict` can return for a source policy

- **Anchor.** [03 Narrowing](../../../spec/03-effects.md#narrowing),
  [03 Scope policies](../../../spec/03-effects.md#scope-policies).
- **Question.** A source `FsPolicy` has no `root` field and no `format`, so
  no value of it "names a grant leaf other than the one the grant descends
  from" or has an unsupported version. Can `restrict` return
  `ScopeError.foreignRoot` or `ScopeError.unsupportedVersion` at all? And
  which source values are `invalidPolicy`: an empty `suffix`, an empty
  `exact` list, a repeated action, a name in `exact` that no file can have?
- **Program.** `DirReadText.restrict(notes FsPolicy.files(prefix: root
  depth: FileDepth.tree() actions: [] names: NameTest.suffix(value: "")))`.
- **Recommended answer.** A source policy is rooted at the grant it
  restricts. `foreignRoot` and `unsupportedVersion` are returned by no
  operation of this release; the chapter says they are reserved for a
  decoded policy. `invalidPolicy` is exactly an empty `suffix`. An empty
  `actions` list and an empty `exact` list are valid and permit nothing; a
  repeated action or name is removed by normalization; a name no file can
  have matches nothing. `sub` returns only `policyTooComplex`.
- **Depends.** Steps 14, 15, 19, 22. Rows `V2-GRANT-restrict-empty-suffix`,
  `V2-GRANT-restrict-empty-actions`, `V2-GRANT-restrict-too-complex`.

### Q9. What counts toward the size and depth of a policy

- **Anchor.** [03 The wire form](../../../spec/03-effects.md#the-wire-form).
- **Question.** "A policy has at most 4,096 nodes and entries in total and
  a depth of at most 64, counting ancestors acquired by restriction." Is a
  node an expression record only, or also a name test record, a prefix
  component, an action, and an `exact` name? Does a `sub` add a level? Is
  the count taken before or after normalization?
- **Document.** A `@fileRule` whose `names` is `@exact` with 4,096 names.
- **Recommended answer.** Counted after normalization, over the effective
  conjunction of Q7. A node is one expression record or one name-test
  record; an entry is one element of a `prefix`, `actions`, or `names`
  list. Depth is the nesting of expression and name-test records, plus one
  for each `restrict` and each `sub` in the grant's ancestry. `limit` adds
  nothing.
- **Depends.** Steps 14, 15, 26. Rows `V2-GRANT-policy-4096-nodes`,
  `V2-GRANT-policy-4097-nodes`, `V2-GRANT-policy-depth-64`,
  `V2-GRANT-policy-depth-65`, `V2-GRANT-restrict-too-complex`.

### Q10. Reserving a read whose size is not known

- **Anchor.** [03 Limits](../../../spec/03-effects.md#limits),
  [06 Closed operation semantics](../../../spec/06-runtime.md#closed-operation-semantics).
- **Question.** "A read first learns the size of the opened object,
  reserves it, and reads at most that many bytes." Standard input has no
  size. With an `inputBytes` bound of 10 and 11 bytes waiting, what does
  `StdinText.read` return, how many bytes are consumed, and is an event
  appended? `limitExceeded` is defined as "nothing was attempted", and "a
  rejection before dispatch consumes nothing".
- **Program.** A test whose `streams(world stdin: "0123456789a")` grant is
  limited to `inputBytes: 10u64` and read once, then once more.
- **Recommended answer.** For a stream the broker reserves the whole
  remaining `inputBytes` bound, and the adapter reads at most that many
  bytes plus one. If the stream ends within the bound, the read succeeds
  and the unused reservation is refunded. Otherwise the call returns
  `limitExceeded` as a completed, traced call: the bytes read are consumed
  and charged, and the next read continues after them. The chapter states
  this as the one case where `limitExceeded` follows an attempt. With a
  remaining bound of zero the refusal is before dispatch and consumes
  nothing.
- **Depends.** Steps 15, 16, 30. Rows `V2-GRANT-limit-stdin-over`,
  `V2-GRANT-limit-stdin-exact`, `V2-GRANT-limit-stdin-zero`.

### Q11. Own counters and ancestor counters under `limit`

- **Anchor.** [03 Narrowing](../../../spec/03-effects.md#narrowing),
  [06 Host calls](../../../spec/06-runtime.md#host-calls).
- **Question.** "`limit` keeps the smaller of each bound" and "a child is
  bound by every ancestor's reservation limit, and its own tighter limits
  apply in addition". When a parent bounded at 8 invocations has used 6 and
  a child is made with a bound of 5, may the child run 2 or 5 times? What
  does the child's projection show as `limits`?
- **Program.** Two siblings `limit(g {invocations: 5})` of a parent bounded
  at 8, awaited alternately until one is refused.
- **Recommended answer.** Every `limit` creates a counter that starts at
  zero with the written bound. An invocation reserves on the grant's own
  counter and on every ancestor's, atomically, and is refused if any would
  pass its bound. So the child above runs twice. The projection's `limits`
  are the smallest bound of each kind along the ancestry, never a remaining
  amount.
- **Depends.** Steps 15, 17, 22. Rows `V2-GRANT-limit-siblings-share`,
  `V2-GRANT-limit-child-after-use`, `V2-GRANT-limit-projection`.

### Q12. A request for a variable outside an `EnvReadVar` grant

- **Anchor.** [06 Closed operation semantics](../../../spec/06-runtime.md#closed-operation-semantics),
  [04 Test worlds and grants](../../../spec/04-programs-and-packages.md#test-worlds-and-grants).
- **Question.** "`EnvReadVar.read` reads one permitted name. A missing name
  is `notPresent`." A name outside the recipe's `names` is not permitted:
  is the result `denied` or `notPresent`? Which names does the grant of
  `TestWorld.environment` permit?
- **Program.** A recipe with `names: ["HOME"]` and a read of `"PATH"`,
  which the process has.
- **Recommended answer.** `denied`, decided in step 3 of the host call, so
  no event is appended and the process environment is not consulted. The
  grant of `TestWorld.environment` permits every name.
- **Depends.** Steps 15, 16, 21, 30. Rows `V2-GRANT-env-name-denied`,
  `V2-GRANT-env-test-any-name`.

### Q13. Invocation ordinals and root ordinals

- **Anchor.** [06 Audit trace](../../../spec/06-runtime.md#audit-trace).
- **Question.** Three numbers of the trace are under-determined.
  (a) "Every `await` is an invocation, numbered from 1": does a call that
  appends no event take an ordinal (a `TestWorld` member, a refusal before
  the adapter, an `await` of a plan of a source `proc`)? (b) In a test,
  which invocation is the parent of an `await` in the test body? (c) `root`
  is "a run-local ordinal of the root grant": in which order are roots
  numbered, and is the `TestWorld` root one of them?
- **Program.** A test that awaits `TestWorld.directory`, then a read that
  is denied by policy, then a read that succeeds.
- **Recommended answer.** (a) Every started `await` takes the next ordinal,
  whether or not an event follows, so a trace's ordinals may have gaps. A
  startup read takes one too, in processing order. (b) The runner's run of
  a test body is invocation 1, as the launcher's run of an entry is the
  first invocation after the startup reads. (c) A root is each grant an
  issuer hands out: each acquired startup leaf, and each grant in the
  result record of a `TestWorld` member. Roots are numbered from 1 in issue
  order, the grants of one result record in the canonical order of its
  field names. The `TestWorld` root has no ordinal, since no event projects
  it. In the example the successful read is invocation 4 with parent 1.
- **Depends.** Steps 06, 15, 17, 21, 28. Every case with an `auditTrace`
  expectation; the focused rows are `V2-RUNTIME-invocation-gaps`,
  `V2-RUNTIME-test-body-parent`, `V2-RUNTIME-root-ordinals`.

### Q14. A grant that fails validation at the dispatch boundary

- **Anchor.** [06 Host calls](../../../spec/06-runtime.md#host-calls),
  [06 Traps](../../../spec/06-runtime.md#traps).
- **Question.** Step 1 validates "its world and provider, its lifetime and
  generation". The effects chapter says a foreign or released grant "is
  unusable". Is that the trap `@runtime.invalidHostValue` or the typed
  error `denied`?
- **Program.** None in source: no checked program holds a grant of another
  world or uses one after release. The input is a host test that hands the
  broker a token of another world.
- **Recommended answer.** `@runtime.invalidHostValue`. Checked source
  cannot reach the state, so it is a defect, and `denied` would hide one.
- **Depends.** Steps 15, 20. Rows `ATK-foreign-grant`, `ATK-stale-grant`,
  `ATK-forged-token`.

## Tests

### Q15. The `takes` of a test

- **Anchor.** [01 `test`](../../../spec/01-source-language.md#test).
- **Question.** "`takes` … admits exactly one parameter, of the standard
  type `TestWorld`; any other parameter list is `@syntax.invalidForm`."
  Whether a written type is `TestWorld` is known only after resolution, so
  a syntax code is reported for a type fact. Is the parameter's name free?
- **Program.** `test "t" takes {w: TestWorld} do endtest`;
  `takes {world: Str}`; `takes {a: TestWorld b: TestWorld}`.
- **Recommended answer.** Keep the code. The reader reports
  `@syntax.invalidForm` at the `takes` section for anything but one
  bare-name parameter; the checker reports the same code at the same span
  when the one type does not resolve to `@std.test.TestWorld`. The name is
  free.
- **Depends.** Step 21. Rows `V2-PROJECT-test-takes-two`,
  `V2-PROJECT-test-takes-other-type`, `V2-PROJECT-test-takes-renamed`.

### Q16. `RelativePath` as a dict key

- **Anchor.** [04 Test worlds and grants](../../../spec/04-programs-and-packages.md#test-worlds-and-grants),
  [02 Dict keys, equality, and order](../../../spec/02-type-system.md#dict-keys-equality-and-order).
- **Question.** `TestWorld.directory` takes `files: Dict(RelativePath Str)`.
  No chapter says that `RelativePath` implements `Ordered`, which a dict
  key needs, nor in which order two paths compare.
- **Program.** The test of the programs chapter:
  `TestWorld.directory(world files: [path: "x\ny"])`.
- **Recommended answer.** `RelativePath` implements `Equatable` and
  `Ordered` in `@std.fs`, comparing its names in order with the shared
  lexicographic helper, each name by Unicode scalar.
- **Depends.** Steps 19, 21. Rows `V2-GRANT-test-directory-two-files`,
  `V2-TYPE-INTERFACE-relative-path-ordered`.

## Targets, startup, and the run

### Q17. The observation of a `workspaceRun` case

- **Anchor.** [07 Case operations and inputs](../../../spec/07-diagnostics-and-conformance.md#case-operations-and-inputs),
  [07 Persistent observations](../../../spec/07-diagnostics-and-conformance.md#persistent-observations).
- **Question.** `workspaceRun` observes "startup, the entry's result, and
  the ordered audit trace", but "Persistent observations" defines no
  document for a run: `@testRun.v2` is the only executable one, and a
  case's `result` key is a `@value.v2` snapshot. In which format does a
  case state the expected startup outcomes, application code, captured
  output, and trace?
- **Document.** A case of the release program run under a world.
- **Recommended answer.** A closed document beside `@testRun.v2`, with
  `format`; `result` (the `@command.*` atom); the optional
  `applicationCode` (`U8`); `stdout` and `stderr` (bytes records); `startup`
  (a list, one record per processed leaf in processing order, of `binding`,
  `status`, and the optional `failure`); the optional `trap`; and
  `auditTrace` (one `@auditTrace.v2` document). The chapter names it and
  the manifest key. Until it does, step 29 builds the handler and its host
  tests, and every `workspaceRun` row of this plan is written as the fields
  above and stays out of the corpus.
- **Depends.** Steps 29, 33, 36, and every row whose kind is
  `workspaceRun`.

### Q18. Which events a `run` payload holds, and where its policies go

- **Anchor.** [05 Command payloads](../../../spec/05-tooling.md#command-payloads).
- **Question.** The `run` payload has `auditTrace: AuditEvent[]`, and
  `StartupRecord` has its own `auditTrace`. Is an event in both, or are
  they split by phase? The payload has no field for the `drivers` and
  `policies` of an `@auditTrace.v2` document, yet a projection's `policy`
  hash is a key of `policies`.
- **Recommended answer.** `startup.auditTrace` holds the `@startup` events
  and the `@host` events of phase `@startup`; the payload's `auditTrace`
  holds the events of phase `@entry`; no event is in both. The payload
  needs the two missing fields; until the chapter adds them, a narrowed
  grant's policy is recoverable only from a conformance trace.
- **Depends.** Step 32. Rows of the `run` payload schema test.

### Q19. Startup reads against the `source` leaf's limits and errors

- **Anchor.** [04 Startup order](../../../spec/04-programs-and-packages.md#startup-order),
  [03 Limits](../../../spec/03-effects.md#limits).
- **Question.** A startup read goes "through the `source` leaf's grant".
  Does it count against that leaf's `limits`? When the read returns a typed
  error the issuer's row does not list (`limitExceeded`, `outcomeUnknown`),
  is the dependent leaf refused or failed?
- **Document.** `@args` with `limits: {invocations: 1u64}`, `@notes` with
  `resource: {kind: @argumentPath index: 0u64 source: @args}`, and an entry
  that then awaits `EnvReadArgs.read(args)`.
- **Recommended answer.** The read is an invocation of the source grant: it
  reserves on its counters, so the entry's own read above returns
  `limitExceeded`. A read error the issuer's row does not list as a refusal
  is a failure, `@grant.acquisitionFailed`, under "anything the row does
  not list as acquired or refused is a failure".
- **Depends.** Step 28. Rows `V2-GRANT-startup-read-counts`,
  `V2-GRANT-startup-read-limit-fails`.

### Q20. The `TestWorld` root in a target's `effects`

- **Anchor.** [06 Host registry](../../../spec/06-runtime.md#host-registry).
- **Question.** "Admission refuses every one of them for a target, since no
  recipe can name `TestWorld`." Which diagnostic refuses an `effects` list
  or an entry row that names `TestWorld`?
- **Program.** An entry `proc main takes {world: TestWorld} returns code:
  ExitCode performs {TestWorld}` with `effects: [@std.test.TestWorld]`.
- **Recommended answer.** No dedicated check. A `TestWorld` parameter is a
  leaf no provider issues, `@grant.invalidBinding`; without such a
  parameter nothing can perform the root, so a row or list that names it is
  the warning `@contract.unusedEffect`. The registry sentence describes the
  consequence, and names no code.
- **Depends.** Steps 25, 26. Rows `V2-GRANT-entry-test-world-leaf`,
  `V2-CONTRACT-unused-effect-test-world`.

### Q21. Endings the outcome table does not list

- **Anchor.** [06 Entry outcome](../../../spec/06-runtime.md#entry-outcome),
  [05 Versioned envelope and results](../../../spec/05-tooling.md#versioned-envelope-and-results).
- **Question.** (a) A defect trap during startup: is the result
  `@command.trap` with `startup.status: failed`, and which leaves are
  released? (b) A release that the operating system reports as failed.
  (c) The final `status` of an acquired binding: the record says `released`
  "after its release", so is `acquired` ever the final status of a
  finished command?
- **Recommended answer.** (a) `@command.trap`, exit 5, with the trap
  record; `startup.status` is `failed` with a `null` `failure`, because no
  leaf stopped startup; every leaf acquired so far is released. (b) A
  release cannot fail a run: the binding is `released` and the failure
  goes to the tool's log. (c) No: every acquired binding of a finished
  command is `released`; `acquired` is the status inside the `@startup`
  event, which is appended before release.
- **Depends.** Steps 28, 29, 32. Rows `ATK-trap-during-startup`,
  `V2-GRANT-release-status`.

### Q22. "An unbound optional grant" in the release gate

- **Anchor.** [v0.0.2 release gate](../README.md#release-gate),
  [04 Targets](../../../spec/04-programs-and-packages.md#targets).
- **Question.** The gate shows that "an unbound optional grant arrives as
  `Option.none()`", while "a missing or invalid recipe fails the build, for
  a required and an optional leaf alike".
- **Recommended answer.** The gate means a refused optional leaf: a
  `@variablePath` whose variable is not present. The roadmap sentence is
  reworded; the specification stands.
- **Depends.** Step 36. Row `V2-GRANT-release-optional-absent`.

### Q23. Local dependencies without a lock

- **Anchor.** [04 Dependencies and lock](../../../spec/04-programs-and-packages.md#dependencies-and-lock),
  [owner table](../m0/owners.md).
- **Question.** Check and run "work offline from the project tree and the
  lock", and a lock has "one entry per declared dependency". The owner
  table puts the lock and `@project.staleLock` outside this release, and
  `project sync` has no owner. Does a project whose dependencies are all
  `@path` need a `project-lock.vibon` in this release?
- **Recommended answer.** No. A lock file is neither read nor required
  while every dependency is `@path`; a `@git` dependency stays
  `@tool.unavailable` at its entry. The chapter says that a lock is
  required exactly when a Git dependency is declared.
- **Depends.** Step 27. Rows `V2-PROJECT-path-dependency-no-lock`,
  `V2-PROJECT-git-dependency-unavailable`.

### Q24. The version of a driver

- **Anchor.** [06 Audit trace](../../../spec/06-runtime.md#audit-trace).
- **Question.** `drivers` and `providerIdentity` carry a `version: Str`. No
  chapter says what a shipped driver's version is.
- **Recommended answer.** The toolchain release, `"0.0.2"`, for every
  driver the toolchain ships, the fake world's included.
- **Depends.** Steps 17, 28, 30, 31. Every `auditTrace` expectation.

### Q25. The row of the grant intrinsics in the registry snapshot

- **Anchor.** [06 Compiler intrinsic registry](../../../spec/06-runtime.md#compiler-intrinsic-registry),
  [06 Registry versioning](../../../spec/06-runtime.md#registry-versioning).
- **Question.** The checked-in snapshot `registry/compiler-2.0.txt` has no
  `grant.restrict`, `grant.sub`, or `grant.limit`. "Adding a row increments
  the minor." Are they rows of 2.0, which the snapshot lacks, or of 2.1?
- **Recommended answer.** Rows of 2.0 with `since` 0: the chapter lists
  them in the registry that "starts at 2.0", and nothing has been released
  against the snapshot. The snapshot is corrected.
- **Depends.** Step 02. Row "registry snapshot" of its matrix.

## Where the tree contradicts the M2 chapters

These are not open questions. Each is a place where the code as it stands
differs from the specification, with the step that replaces it.

| Where | The tree | The specification | Step |
| --- | --- | --- | --- |
| `crates/vibra-ir/src/external/host.rs` | registry major 1: kebab symbols such as `io.stdin-text`, owners such as `@std.io.stdin`, string paths, no grant receiver, no delivery class | major 2: dotted camelCase symbols, one owner interface, a grant as first parameter, `@once` | 02 |
| `crates/vibra-host/src/provider.rs`, `operation.rs` | one string-path method per row; error enums without `denied`, `limitExceeded`, `unsupported`, `outcomeUnknown` | the five error types each with the four broker cases; every call through `Context::host` | 02, 15, 16 |
| `crates/vibra-host/src/world.rs` | `@hostWorld.v1` | `@hostWorld.v2` | 16 |
| `crates/vibra-host/src/event.rs` | `@auditTrace.v1`, one event kind, no grant projection | `@auditTrace.v2` with `@startup` and `@host` events | 17 |
| `crates/vibra-host/src/system.rs` | ambient absolute paths | a held directory, no link followed, the real name checked | 30, 31 |
| `crates/vibra-types/src/check/expr.rs` | every `await` is `@effect.invalidAwait` | valid in a `proc` and a `test` | 05, 07 |
| `crates/vibra-types/src/check/testing.rs` | a test with `takes` or `performs` is `@tool.unavailable` | a test receives its `TestWorld` and is checked against its row | 07, 21 |
| `crates/vibra-workspace/src/semantic.rs` | a `@bin` target is `@tool.unavailable` | entry, admission ceiling, and recipes are validated | 25, 26 |
| `crates/vibra-workspace/src/project.rs` | every dependency is unavailable | a `@path` dependency is a unit | 27 |
| `crates/vibra-ir/registry/compiler-2.0.txt` | no `grant.*` row | three rows (Q25) | 02 |
| `docs/spec/03-effects.md`, status lines | "the Windows file-scope adapter … has not been shown feasible" | [the feasibility record](../m0/file-scope-feasibility.md) shows it | the specification's owner; reported by step 37 |
