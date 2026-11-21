# v0.0.2 M2: specification questions

Each question is a place where the specification is silent, says two things,
or decides an observable behaviour by a sentence that can be read two ways.
A step guide never settles one by choosing. A question is in one of three
states:

- **Ruled.** Its entry carries a **Ruling**, and the rows under
  **Depends** are written against it.
- **With Hannah.** Q1 and Q10. Each lays out its options and, per option,
  the exact rows and steps that change, so the ruling is a mechanical
  edit. The rows they decide carry the mark *awaits Q1* or *awaits Q10* in
  their guide, and are not implemented and not counted before the ruling.
- **Open.** Q28 and Q29. Each carries a **recommended answer**; the rows
  under **Depends** are written against it and name the question, so a
  different ruling changes exactly those rows.

Chapters: [01 source](../../../spec/01-source-language.md),
[02 types](../../../spec/02-type-system.md),
[03 effects](../../../spec/03-effects.md),
[04 programs](../../../spec/04-programs-and-packages.md),
[05 tooling](../../../spec/05-tooling.md),
[06 runtime](../../../spec/06-runtime.md),
[07 diagnostics](../../../spec/07-diagnostics-and-conformance.md).

Q1 and Q10 are ruled before step 02 and step 18 start: one option of each
changes a registry row that step 02 freezes in a snapshot, and one option
of Q1 adds a case to `ScopeError`, which step 18 declares. Steps 01 and 13
depend on neither.

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

  A model need not be memory only. The runtime chapter allows that "a
  source implementation may model an interface using grants it holds",
  while the effects chapter says of a package's model that it "holds no
  real grant". The second fixture holds one and forwards to it:

  ```vibra
  type Forward of {inner: DirReadText}
    impl DirReadText
      proc read
        takes {self: Self at: RelativePath}
        returns outcome: Result(Str FsError)
        do
          return await DirReadText.read(self.inner at)
      endproc
    endimpl
  endtype
  ```

  A caller that narrows a `Forward`, viewed as a `DirReadText`, and hands
  the result on expects to have handed on less.
- **Status.** With Hannah. The options:
  - **(a) The three members are abstract.** They are bound with
    `external @compiler`, and such a member is the implementation of a
    sealed grant only, exactly as an `external @host` member is under Q2.
    Every `impl` of a standard operation interface supplies `limit`, and
    `restrict` and `sub` on the six `Dir…` interfaces. A model that wraps
    a grant forwards the narrowing to it; a memory-only model returns
    itself or keeps its own bounds.
  - **(b) A typed failure for an unsealed receiver.** `limit` returns
    `Result(Self ScopeError)`, and the three members return a new
    `ScopeError` case on a value that is not a sealed grant. The case
    needs a name, which the ruling gives.
  - **(c) Identity.** `limit` returns the value, and `restrict` and `sub`
    return `Result.ok` of it. This is what the guides are written against
    today. With it a grant-backed model keeps the whole authority of the
    grant it wraps after a caller narrowed it, and the caller is not told.
- **What changes, per option.** The first three rows of the table are the
  same in every option.

  | Row or task | (a) abstract | (b) typed failure | (c) identity |
  | --- | --- | --- | --- |
  | `V2-GRANT-model-is-not-grant` [22] | passes; the model supplies the three members | unchanged | unchanged |
  | `V2-GRANT-forward-model-reads` [22] | `Ok("A")`, one event; `Forward` supplies the three members | `Ok("A")`, one event | `Ok("A")`, one event |
  | step 20, a narrowing row on a sealed grant | unchanged | `limit` yields `Result.ok` of the token | unchanged |
  | `V2-GRANT-narrow-model-limit` [22] | the `impl`'s `limit` runs and returns the model; a read answers | `Err` of the new case | the model; a read answers |
  | `V2-GRANT-narrow-model-restrict` [22] | the `impl`'s `restrict` runs: `Result.ok` of the model | `Err` of the new case | `Result.ok`; the model reads `b.bin` |
  | `V2-GRANT-narrow-model-sub` [22] | the `impl`'s `sub` runs: `Result.ok` of the model | `Err` of the new case | `Result.ok` |
  | `V2-GRANT-narrow-forward-limit` [22] | `Forward.limit` limits `inner`: `Ok("A")`, then `limitExceeded` | `Err` of the new case at the `limit`; no read | `Ok("A")`, then `Ok("A")` |
  | `V2-GRANT-narrow-forward-restrict` [22] | `Forward.restrict` restricts `inner`: `b.bin` is `denied` | `Err` of the new case at the `restrict` | `Result.ok`; `b.bin` is `Ok("B")` |
  | `V2-GRANT-impl-supplies-limit` [19] | positive: accepted | unchanged | negative: `@type.defaultOverride` |
  | a new row of step 19: an `impl` without `limit` | added: `@type.missingAbstractMember` at the `impl` | not added | not added |
  | `V2-EFFECT-impl-host-member-supplied`, `V2-EFFECT-impl-host-member-row`, `V2-EFFECT-impl-host-member-missing` [19] | each `impl` also supplies the three members, so its one fault stays one | unchanged | unchanged |
  | `V2-GRANT-narrowing-types` [19] | unchanged | the `limit` is under `try` | unchanged |
  | a new row of step 19: a `limit` result bound as a grant | not added | added: `@type.mismatch` at the initializer | not added |
  | step 19, task 6 | the three members are abstract for an `impl` | `limit` is declared with the `Result` | a default member; an `impl` that supplies one is `@type.defaultOverride` |
  | step 20, task 5 and the row "narrowing on a model" | a model receiver dispatches to its `impl`, as every member does; no request | a model receiver yields `Result.err` of the new case; no request | a model receiver yields itself; no request |
  | step 02, the row "the three grant rows" and `compiler-2.0.txt` | unchanged | `grant.limit` is `Fn({G GrantLimits} Result(G ScopeError))` | unchanged |
  | step 18, `ScopeError` and `V2-TYPE-INTERFACE-scope-error-cases` | unchanged | five cases | unchanged |
  | every row of step 22 that calls `limit` (`V2-GRANT-limit-*`, `V2-GRANT-copy-shares`) | unchanged | the `limit` is under `try` | unchanged |
  | chapters | 03 Narrowing and 03 Operation interfaces say the members are abstract for an `impl` | 03 Narrowing (the table, `ScopeError`), 06 Compiler intrinsic registry | 03 Narrowing says the members are the identity on another value |

  Q2's second half follows this ruling: under (a) it reads "an
  `external @compiler` member of an interface is abstract for every
  `impl`".
- **Depends.** Steps 02, 18, 19, 20, 22. The rows of the table.

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
- **Ruling.** An `external @host` member is abstract for every
  `impl`: the binding is the implementation of sealed grants only. An
  `external @compiler` member of an interface is a default member with a
  toolchain body, so an `impl` that supplies `restrict`, `sub`, or `limit`
  is `@type.defaultOverride`.  The sentence about the three narrowing members holds under options (b)
  and (c) of Q1 and is reversed under its option (a); the row
  `V2-GRANT-impl-supplies-limit` carries the mark *awaits Q1*.
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

- **Ruling.** `@type.mismatch` at the statement: the checker
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
- **Ruling.** One `@effect.invalidAwait` at the `await`. In a
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
- **Ruling.** The warning needs a body and a row written on the
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
- **Ruling.** Only the result operand of a written `Proc` or
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

- **Ruling.** `sub` rebases: a request through the child is the
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
- **Ruling.** A source policy is rooted at the grant it
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
- **Ruling.** Counted after normalization, over the effective
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
- **Status.** With Hannah. Two cases are separate.

  **Input whose size is known before content is read** is the same in
  every option: a file, the standard input of a world (the document holds
  it), and a process's standard input when it is a regular file. The
  adapter reports the remaining size to the gate, the broker reserves it,
  and a size over the remaining bound is `limitExceeded` with nothing
  consumed. An empty input reserves zero, so it succeeds at a zero budget.

  **Input whose size is unknown** (a pipe, a console) under a set
  `inputBytes` bound is the question. The options:
  - **(a) A read asks for a count.** `StdinText.read` and
    `StdinBytes.read` take `max: U64` and return at most that many bytes;
    the empty value is the end of input. The broker reserves `max` in
    step 3 (for an input of known size, the smaller of `max` and that
    size), and the settlement refunds what was not read. "Read to the
    end" is a library procedure of `@std.io` that loops over the member.
    Reservation always precedes the action, and no operation is special:
    this is the shape `RandomBytes.generate(count)` already has. The
    ruling also says what a text read does with a scalar that `max`
    splits; the rows below use ASCII and do not depend on it.
  - **(b) A typed refusal.** A read of an unsized stream through a grant
    with a set `inputBytes` bound is `unsupported`, decided before the
    stream is touched. A grant with no such bound reads to the end.
  - **(c) Read one byte past the bound.** The broker reserves the whole
    remaining bound and the adapter reads at most that many bytes plus
    one. Within the bound the read succeeds and the rest is refunded;
    otherwise it returns `limitExceeded` as a completed, traced call that
    consumed and charged what it read. This contradicts "A limit MUST be
    reserved before an operation could exceed its count or byte bound",
    spends a byte beyond the bound, and makes `limitExceeded` follow an
    attempt.
- **What changes, per option.** The awaiting rows are written in their
  guides as option (a) gives them. `K` rows are of known size and differ
  between the options only in the `max` argument that (a) adds.

  | Row | (a) a count | (b) typed refusal | (c) one byte past |
  | --- | --- | --- | --- |
  | `V2-GRANT-limit-stdin-exact` [22] (`K`) | `read(max: 3)`, bound 3, `abc`: `Ok("abc")` | `read()`: `Ok("abc")` | `read()`: `Ok("abc")` |
  | `V2-GRANT-limit-stdin-over` [22] (`K`) | `read(max: 3)`, bound 2: `limitExceeded`; an unlimited grant then reads `abc` | `read()`: the same | `read()`: the same |
  | `V2-GRANT-limit-stdin-zero` [22] (`K`) | `read(max: 1)`, bound 0, empty input: `Ok("")` | `read()`: `Ok("")` | `read()`: `Ok("")` |
  | `V2-GRANT-limit-stdin-count` [22] (`K`) | `read(max: 2)` of `abc`, bound 2: `Ok("ab")`; an unlimited grant then reads `c` | the row is deleted | the row is deleted |
  | step 15, "stdin, known size" (three rows) | with `max` | without `max` | without `max` |
  | step 15, "stdin, unsized, within the count" | bound 10, `max: 10`, the driver holds `abc`: `Ok("abc")`; 3 charged, 7 refunded | `Err(unsupported)`; no driver call | `Ok("abc")`; 3 charged |
  | step 15, "stdin, unsized, over the bound" | bound 2, `max: 3`: `limitExceeded`; no driver call; nothing consumed | `Err(unsupported)`; no driver call | `Err(limitExceeded)`, a completed event; 3 bytes consumed and charged |
  | step 15, "stdin, unsized, a short count" | bound 2, `max: 2`, the driver holds `abc`: `Ok("ab")`; 2 charged; `c` remains | the row is deleted | the row is deleted |
  | step 16, "stdin reports its size" | unchanged | unchanged | unchanged |
  | step 16, "stdin by count" | `read(max: 2)` of `x\ny` three times: `x\n`, `y`, the empty value | the row is deleted | the row is deleted |
  | step 30, "stdin over the limit, a pipe" | bound 2, `max: 3`: `limitExceeded`; a later unlimited read returns `abc` | `Err(unsupported)`; a later unlimited read returns `abc` | `Err(limitExceeded)`, completed; a later read returns the empty value |
  | step 30, "stdin within the limit, a pipe" | bound 3, `max: 3`: `Ok("abc")` | `Err(unsupported)` | `Ok("abc")` |
  | step 30, "stdin, a file of known size" | `K` | `K` | `K` |

  Option (a) also changes rows that no other option touches, because the
  member gains a parameter and "to the end" becomes a library loop:

  | Step | What changes under (a) |
  | --- | --- |
  | 02 | the rows `io.stdinText` and `io.stdinBytes` have the further parameter `max: U64`; `host-2.0.txt` |
  | 16 | "stdin once", "stdin invalid text": each read names a `max`; a read after the end is the empty value |
  | 19 | `StdinText.read` and `StdinBytes.read` take `max`; `@std.io` gains the two read-to-the-end procedures; `V2-EFFECT-std-each-interface` |
  | 20 | "each argument form": the two stdin rows send `Count` |
  | 21 | `V2-GRANT-test-stdin-twice` reads through the library procedure |
  | 29 | `V2-RUNTIME-run-stdin`: the echoing entry uses the library procedure |
  | 30 | "stdin text", "stdin invalid text", "stdin read error": each read names a `max` |
  | 32 | "stdin passes through": the echoing entry uses the library procedure |
  | chapters | 03 Standard operation inventory; 06 Host registry, the two rows; 06 Closed operation semantics, "Standard input" and the read sentence of "All operations"; 04 The standard library |

  Under (b) the chapters gain one sentence in 06 "Standard input"; under
  (c), the exception in 03 "Delivery and typed outcomes" and 06 "Error
  types" that `limitExceeded` can follow an attempt.
- **Depends.** Steps 02, 15, 16, 19, 20, 21, 22, 29, 30, 32. The rows of
  the two tables.

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
- **Ruling.** Every `limit` creates a counter that starts at
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
- **Ruling.** `denied`, decided in step 3 of the host call, so
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
- **Ruling.** (a) Every started `await` takes the next ordinal,
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
- **Ruling.** `@runtime.invalidHostValue`. Checked source
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
- **Ruling.** Keep the code. The reader reports
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
- **Ruling.** `RelativePath` implements `Equatable` and
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
- **Ruling.** A closed document beside `@testRun.v2`, with
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
- **Ruling.** `startup.auditTrace` holds the `@startup` events
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
- **Ruling.** The read is an invocation of the source grant: it
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
- **Ruling.** No dedicated check. A `TestWorld` parameter is a
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
- **Ruling.** (a) `@command.trap`, exit 5, with the trap
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
- **Ruling.** The gate means a refused optional leaf: a
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
- **Ruling.** No. A lock file is neither read nor required
  while every dependency is `@path`; a `@git` dependency stays
  `@tool.unavailable` at its entry. The chapter says that a lock is
  required exactly when a Git dependency is declared.
- **Depends.** Step 27. Rows `V2-PROJECT-path-dependency-no-lock`,
  `V2-PROJECT-git-dependency-unavailable`.

### Q24. The version of a driver

- **Anchor.** [06 Audit trace](../../../spec/06-runtime.md#audit-trace).
- **Question.** `drivers` and `providerIdentity` carry a `version: Str`. No
  chapter says what a shipped driver's version is.
- **Ruling.** The toolchain release, `"0.0.2"`, for every
  driver the toolchain ships, the fake world's included.
- **Depends.** Steps 17, 28, 30, 31. Every `auditTrace` expectation.

### Q25. The row of the grant intrinsics in the registry snapshot

- **Anchor.** [06 Compiler intrinsic registry](../../../spec/06-runtime.md#compiler-intrinsic-registry),
  [06 Registry versioning](../../../spec/06-runtime.md#registry-versioning).
- **Question.** The checked-in snapshot `registry/compiler-2.0.txt` has no
  `grant.restrict`, `grant.sub`, or `grant.limit`. "Adding a row increments
  the minor." Are they rows of 2.0, which the snapshot lacks, or of 2.1?
- **Ruling.** Rows of 2.0 with `since` 0: the chapter lists
  them in the registry that "starts at 2.0", and nothing has been released
  against the snapshot. The snapshot is corrected.
- **Depends.** Step 02. Row "registry snapshot" of its matrix.

### Q26. The import cycle between `@std.fs` and `@std.policy`

- **Anchor.** [03 Scope policies](../../../spec/03-effects.md#scope-policies),
  [03 Narrowing](../../../spec/03-effects.md#narrowing),
  [04 The standard library](../../../spec/04-programs-and-packages.md#the-standard-library),
  [04 Modules and imports](../../../spec/04-programs-and-packages.md#modules-and-imports).
- **Question.** `FsPolicy` (`@std.policy`) has a `prefix: RelativePath`,
  and `RelativePath` is in `@std.fs`; the six `Dir…` interfaces of
  `@std.fs` declare `restrict` with a `policy: FsPolicy`. Each module
  needs the other, and "Imports MUST NOT form a cycle". Which declaration
  moves?
- **Program.** `stdlib/src/std/fs.vib` with `use @std.policy enduse` and
  `stdlib/src/std/policy.vib` with `use @std.fs.RelativePath enduse`:
  `@module.importCycle`.
- **Ruling.** `FsPolicy`, `NameTest`, `FileDepth`, `DirDepth`,
  `FileAction`, and `DirAction` are declared in `@std.fs`. `@std.policy`
  keeps `ScopeError` and `GrantLimits` and imports nothing, so every
  operation module imports it. The embedded library gets no exemption from
  the cycle rule.
- **Depends.** Steps 18, 19, 21, 22. Every case that imports a policy
  type; rows `V2-TYPE-INTERFACE-std-policy-types`,
  `V2-TYPE-INTERFACE-std-fs-policy-types`.

### Q27. A malformed `effects` list

- **Anchor.** [04 Targets](../../../spec/04-programs-and-packages.md#targets),
  [07 Contract, effect and authority diagnostics](../../../spec/07-diagnostics-and-conformance.md#contract-effect-and-authority-diagnostics).
- **Question.** `effects` is "a list of entity references to operation
  interfaces, without repeats". The registry's rows for
  `@effect.invalidReference` and `@effect.duplicateRoot` speak of "a row".
  Which codes does a project get for an `effects` entry that resolves to
  nothing, resolves to something that is not an operation interface, or
  repeats?
- **Document.** `effects: [@std.fs.DirReadText @std.fs.DirReadText]`;
  `effects: [@std.fs.FsError]`; `effects: [@std.fs.Nope]`.
- **Ruling.** The list is a row in data form:
  `@effect.invalidReference` at an entry that is not an operation
  interface, including one that resolves to nothing, and
  `@effect.duplicateRoot` at a later repeat, related to the first.
- **Depends.** Step 25. Rows `V2-EFFECT-target-effects-not-interface`,
  `V2-EFFECT-target-effects-unknown`, `V2-EFFECT-target-effects-repeat`.

## Accounting and the trace

### Q28. What a read that fails after reading charges

- **Anchor.** [03 Limits](../../../spec/03-effects.md#limits),
  [06 Host calls](../../../spec/06-runtime.md#host-calls).
- **Question.** "Input bytes are the bytes the program receives", and
  "the broker reserves before an action, charges actual progress". A text
  read takes five bytes from a file or a stream and returns `invalidText`,
  or takes three and returns `other`: the program receives no byte, and
  the adapter made progress. Which is charged?
- **Program.** A grant with `inputBytes: 5u64`; a file holding the byte
  `255` and four more; `DirReadText.read`, then a read of a one-byte file.
- **Recommended answer.** Progress is what the adapter moved across the
  boundary: the bytes it took from the resource for this call, whatever
  the reply. The first read charges five and the second is
  `limitExceeded`. A count the adapter does not know is `Uncertain` and
  charges the reservation in full. The limits sentence is reworded to
  "the bytes read for the program".
- **Depends.** Steps 15, 30. Rows `ATK-counter-invalid-text-charged`,
  and "stdin read error" of step 30.

### Q29. A refusal decided after the adapter opened an entry

- **Anchor.** [06 Host calls](../../../spec/06-runtime.md#host-calls),
  [06 Error types](../../../spec/06-runtime.md#error-types),
  [06 Audit trace](../../../spec/06-runtime.md#audit-trace),
  [03 Limits](../../../spec/03-effects.md#limits).
- **Question.** A read "first learns the size of the opened object,
  reserves it", so a size over the remaining limit is found in step 4,
  after an entry was opened. The same holds for an `unsupported` that only
  the opened object shows. The trace section lists the refusals "decided
  in steps 1 to 3" as appending nothing, and names `denied` alone as an
  adapter refusal that is recorded. Is a `limitExceeded` or an
  `unsupported` of step 4 a completed, traced call? And "an invocation is
  every call that passed reservation": does such a call count as one?
- **Program.** A grant with `invocations: 1u64` and `inputBytes: 4u64`; a
  read of a five-byte file, then a read of a four-byte file.
- **Recommended answer.** Neither is traced: the error table says of both
  cases that nothing was dispatched or attempted, and the format has no
  refusal kind. A call refused for its size holds no invocation, because
  its byte reservation failed and so it did not pass reservation; a call
  refused as `unsupported` in step 4 passed reservation and holds one. So
  the first read is `limitExceeded` with no event, and the second is
  `Ok`. The chapter says in "Host calls" that reservation ends when the
  size of a read is reserved, and in "Audit trace" that these two
  refusals append nothing wherever they are decided.
- **Depends.** Steps 15, 17, 22, 31. Rows
  `ATK-counter-size-refusal-holds-no-invocation`,
  `V2-GRANT-limit-input-bytes`, `ATK-fs-no-normalized-name`, and "size
  refusal at the gate" of step 17.

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
