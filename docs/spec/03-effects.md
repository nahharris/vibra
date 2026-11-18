# Vibra effects and authority

Status: normative target
Implementation status: none of this chapter is implemented. The shipped
v0.0.1 toolchain implements effect roots declared with a dedicated form and
checked against a target's root list, with no grant values. This chapter
replaces that design. Delivery is scheduled in
[v0.0.2](../roadmap/v0.0.2/README.md). The Windows file-scope adapter that
[Filesystem policies](#filesystem-policies) requires has not been shown
feasible; that check is the first item of the release.

## Two mechanisms

Vibra controls what a program does to the world with two mechanisms, and
both are required for every operation.

- An **effect ceiling** is static. A procedure writes the set of operation
  interfaces it may perform, and the checker rejects a body that could
  perform anything outside that set.
- A **grant** is a value. Performing an operation needs a grant for it,
  passed as an ordinary argument. A grant is sealed: a program can receive
  one, pass it on, and narrow it, and can never make one.

A ceiling answers "which kinds of operation can this code perform at all". A
grant answers "on which resource, and within which limits". Naming an
interface in a ceiling gives no authority; holding a grant does not widen a
ceiling.

The checker MUST reject a body whose computed row exceeds its written
ceiling. Host dispatch MUST require a grant and enforce its policy. Missing
authority MUST NOT fall back to the process's ambient access. A denial, an
unsupported action, and an exhausted limit are `Result` values; none is
reported as success and none traps.

These rules constrain code compiled by the toolchain and the runtime and
drivers it ships. They are not a sandbox for arbitrary machine code or a
defense against another process with the same privileges. Vibra does not
track information flow and does not prove that a program holds the least
authority it needs.

## Operation interfaces

An interface that declares at least one `proc` member is an **operation
interface**. It is one effect root. Its identity is its declaration's
identity; an import name or a member name never identifies it. Every
application of a generic operation interface is the same root.

```vibra
interface AuditLog
  visibility @public
  proc append
    takes {log: Self entry: Str}
    returns outcome: Result(Void AuditError)
  endproc
endinterface
```

An interface with only `fn` members is not a root. An operation interface
may also declare `fn` members; they are pure.

The complete row of an operation member is its owning interface plus the
member's written `performs`. A member that performs nothing else omits
`performs`. Writing the owner in a member's own `performs` is
`@effect.duplicateRoot`. An ordinary `proc` writes its complete ceiling.

An implementation of an operation interface is an ordinary `impl`. It MUST
keep each member's row, types, and guards, and its body MUST fit the member's
row. Implementing an interface makes no grant: a type that implements
`DirWriteText` by forwarding to a network grant it holds needs the network
interface in the member's row, which the contract does not have, so the
implementation is rejected.

Only the embedded standard library declares members bound to the host with
`external @host`. Such a member has no body and no `performs`, and its
signature and owner MUST match its registry row. A wrong owner is
`@external.ownerMismatch`, an unknown symbol `@external.unknownSymbol`,
and a wrong signature `@external.signatureMismatch`.

## Effect ceilings

A row is a brace-delimited set of names that resolve to operation
interfaces. A name that resolves to anything else, or to nothing, is
`@effect.invalidReference`. A row that names one interface twice, under any
spelling, is `@effect.duplicateRoot` at the later entry.

```vibra
proc copyNote
  takes {reader: DirReadText writer: DirWriteText path: RelativePath}
  returns outcome: Result(Void FsError)
  performs {DirReadText DirWriteText}
  do
    let text be try await DirReadText.read(reader path) endlet
    return await DirWriteText.write(writer path text)
endproc
```

- A row is written with `performs` on a `proc`, an anonymous `proc`, an
  interface `proc` member, and a `test`, and as the last operand of a `Proc`
  or `Intent` type.
- An omitted row is empty. A ceiling is never inferred.
- A `fn` performs nothing. It may build and return a plan.
- A module-value initializer is pure.
- The checker computes the row a body can perform and requires it to be a
  subset of the ceiling. For each interface outside the ceiling it reports
  one `@effect.outsideCeiling` at the earliest `await` that contributes it,
  with the chain of calls as related spans.
- An interface in a ceiling that the body cannot perform is the warning
  `@contract.unusedEffect`.
- Callable types compare rows exactly. A `Proc({Str} Void {DirWriteText})`
  parameter accepts only a procedure with exactly that row. There are no row
  variables, no row subtyping, and no relation between roots.

A callback's row is therefore visible in the signature that accepts it:

```vibra
proc emit
  takes {
    line: Str
    sink: Proc({Str} Result(Void FsError) {DirWriteText})
  }
  returns outcome: Result(Void FsError)
  performs {DirWriteText}
  do
    return await sink(line)
endproc
```

A caller can pass a procedure that writes to a directory. It cannot pass one
whose row is `{DirWriteText NetSend}`: the types differ, and the call is
`@type.argumentMismatch`. A procedure that performs less is wrapped in an
anonymous `proc` that declares the wider row.

In metadata a row is a record with a `roots` list of interface identities,
sorted by their UTF-8 bytes.

## Cold plans and performed rows

Applying a `proc` does not run it. It evaluates the callee and the
arguments, and returns a **plan**: an immutable value of type
`Intent(R {Row})` that holds them. `await plan` runs the plan and yields its
result.

```vibra
fn prepareWrite
  takes {writer: DirWriteText at: RelativePath contents: Str}
  returns plan: Intent(Result(Void FsError) {DirWriteText})
  do
    return DirWriteText.write(writer at contents)
endfn

proc writeTwice
  takes {writer: DirWriteText at: RelativePath contents: Str}
  returns outcome: Result(Void FsError)
  performs {DirWriteText}
  do
    let plan be prepareWrite(writer at contents) endlet
    let _ be try await plan endlet
    return await plan
endproc
```

`prepareWrite` is a pure function: it writes nothing. `writeTwice` writes
twice, because each `await` is a fresh invocation of the plan.

- Building a plan MUST NOT enter the body, run a guard that has an `else`,
  dispatch a host action, or consume any limit.
- A plan can be stored, passed, returned, and awaited any number of times. It
  is never a consumed ticket and never a memoized result.
- A plan's type always carries its complete row, including through an
  interface value or a stored callable.
- `await` is valid only in a `proc` body and in a `test` body, which runs in
  a procedure context the test runner owns. Elsewhere it is
  `@effect.invalidAwait`.
- `await plan` contributes the plan's complete row to the enclosing body.
  Building a plan contributes only what evaluating its arguments performs.
- A bare `requires` on a `proc` is discharged where the plan is built. A
  `requires … else` and an `ensures … else` run on each `await`, as the
  [type chapter](02-type-system.md#guards) defines.

Named procedures may recurse; the row analysis closes such cycles. An
effectful walk over a collection is a recursive `proc` over `Iter.next`.

## Capability values

A **grant** is a sealed run-time value issued by the runtime under a target's
binding recipe or, in a test, derived from the root grant the runner issues
for the test's world. It records the operation it permits, the resource and
policy it is limited to, the world that issued it, and the limits it shares
with its relatives.

No source expression makes a grant. A path, a policy value, a wrapper type,
an `impl`, and decoded data are not grants and cannot become one. A grant has
no encoding: it cannot be written to a file, compared, or used as a dict key.
A grant from another world, or one released at shutdown, is unusable.

The type of a grant is the operation interface it permits. Authority is
requested in signatures, in three shapes:

```vibra
proc report
  takes {
    notes: DirReadText                 // required
    metrics: Option(NetSend)           // optional
    logs: {reader: DirReadText writer: DirWriteText}   // a bundle
  }
  returns outcome: Result(Void ReportError)
  performs {DirReadText DirWriteText NetSend}
  do
    match metrics
      case Option.some(value: sender)
        let _ be await NetSend.send(sender summary()) endlet
      case Option.none()
        // carry on without metrics
    endmatch
    return Result.ok(value: void)
endproc
```

- A required grant is a plain parameter.
- An optional grant is an `Option`. `Option.none()` is handled by an ordinary
  pattern. Holding a grant does not guarantee that an operation succeeds.
- A bundle is an ordinary record of grants. Passing on one field passes on
  only that grant:

```vibra
let {reader: r} be logs endlet
let _ be await summarize(r) endlet     // summarize cannot write
```

Viewing a value as an interface never reduces what the value can do. Reduced
authority is always a narrower grant, obtained in one of the two ways
below.

### Narrowing

Every standard operation interface declares pure members that return a
narrower grant of the same type. They perform nothing and open nothing.

| Member | Takes | Returns | On |
| --- | --- | --- | --- |
| `restrict` | `{grant: Self policy: FsPolicy}` | `Result(Self ScopeError)` | the six `Dir…` interfaces |
| `sub` | `{grant: Self prefix: RelativePath}` | `Result(Self ScopeError)` | the six `Dir…` interfaces |
| `limit` | `{grant: Self limits: GrantLimits}` | `Self` | every standard operation interface |

```vibra
let onlyReports: DirReadText be
  try DirReadText.sub(notes try RelativePath(["reports"]))
endlet
```

- The result's policy is the parent's policy **and** the new one. Narrowing
  never adds a right, changes the resource, or resets a limit. A policy
  broader than the parent's changes nothing.
- `sub` adds a path prefix below the same held directory. It does not check
  that the directory exists.
- `limit` keeps the smaller of each bound. Copies and children of a grant
  draw on the limits of their ancestors.
- The parent stays valid. There is no revocation and no single-use grant.
- No function combines two grants into one stronger grant.

`ScopeError` (`@std.policy`) has the cases `invalidPolicy`,
`unsupportedVersion`, `policyTooComplex`, and `foreignRoot`.
`GrantLimits` is a record of `invocations`, `inputBytes`, and `outputBytes`,
each an `Option(U64)`.

### Brands

A declared type with a private representation over a grant type certifies
who issued the grant:

```vibra
type LogDir of DirWriteText
  visibility @public
  representation @private
endtype
```

Only the module that declares `LogDir` can construct one, so a `LogDir`
parameter accepts only what that module chose to wrap. A brand says nothing
about a path at compile time.

## Target admission

A binary target's `effects` list is its static admission ceiling: the
operation interfaces the program as a whole may perform. Check, run, and
build MUST reject an entry whose row, or a startup step whose row, is not
contained in it. An interface in the list that nothing performs is a
warning. The list authorizes nothing.

Every grant leaf of the entry MUST have a binding recipe, defined in the
[programs chapter](04-programs-and-packages.md#targets). The toolchain MUST
reject a missing recipe, a recipe whose provider cannot issue the leaf's
type, a cyclic `after` relation, and a recipe that reads an argument or
variable without naming the grant leaf for that read.

What a target is allowed to do MUST be presentable in plain terms from the
project file alone: for each grant, the actual resource, the operation, the
policy, and the limits, and not only an interface name. A recipe that reads
configuration to choose a resource shows that read, and that read is a
startup step whose row counts against the admission ceiling.

At startup the runtime acquires grants in the
[startup order](04-programs-and-packages.md#startup-order) the programs
chapter defines. A required grant that cannot be acquired prevents the entry
from running. An optional grant refused in the way its provider specifies
becomes `Option.none()`; a malformed recipe or an incompatible driver is an
error, never an absence. After a failed startup, grants already acquired are
released in reverse acquisition order. Release is not rollback: an external
effect that already happened is reported, not undone.

No operation returns `Never`, and none terminates the process. The
[runtime chapter](06-runtime.md#entry-outcome) defines how a run ends.

## Scope policies

A **policy** is plain data that describes which requests a grant permits. It
is not code: there is no user-written permission predicate and no pattern
language. A policy describes; only a grant authorizes.

In source, a filesystem policy is a value of the standard types below
(`@std.policy`). In a project file it is a `@scopePolicy.v1` document.

```vibra
type FsPolicy
  case none
  case allOf {items: List(FsPolicy)}
  case anyOf {items: List(FsPolicy)}
  case files {
    prefix: RelativePath
    depth: FileDepth          // children | tree
    actions: List(FileAction) // read | replace | create | stat
    names: NameTest
  }
  case directories {
    prefix: RelativePath
    depth: DirDepth           // self | children | tree
    actions: List(DirAction)  // list | stat
  }
  visibility @public
endtype

type NameTest
  case anyName
  case exact {names: List(Str)}
  case suffix {value: Str}
  case allOf {items: List(NameTest)}
  case anyOf {items: List(NameTest)}
  visibility @public
endtype
```

```vibra
let textOnly be FsPolicy.files(
  prefix: try RelativePath([])
  depth: FileDepth.tree()
  actions: [FileAction.read()]
  names: NameTest.suffix(value: ".txt")
) endlet
let reader be try DirReadText.restrict(notes textOnly) endlet
```

### Meaning

A policy denotes a set of requests. `allOf` is intersection and `anyOf` is
union. An empty `allOf` permits everything the parent permits; an empty
`anyOf`, like `none`, permits nothing. A missing field is an error, never a
wildcard.

A rule is one whole alternative. Its fields are never separated: a policy
that permits reading under `a` and replacing under `b` does not permit
replacing under `a`.

Restriction keeps the parent as a conjunction, so no implementation needs to
decide whether one policy is a subset of another, and none may drop an
ancestor's constraint because it believes the child is narrower.

### The wire form

A `@scopePolicy.v1` document is a record of `format`, `family`
(`@filesystem` or `@http`), and `expression`. An expression is a record
whose `kind` is `@none`; `@allOf` or `@anyOf` with `items`; or a rule.
Unknown fields, kinds, and versions are rejected.

| Rule kind | Fields |
| --- | --- |
| `@fileRule` | `root`, `prefix` (list of names), `depth` (`@children`, `@tree`), `actions` (set of `@read`, `@replace`, `@create`, `@stat`), `names` |
| `@directoryRule` | `root`, `prefix`, `depth` (`@self`, `@children`, `@tree`), `actions` (set of `@list`, `@stat`) |

`root` names the grant leaf whose recipe holds the directory. `names` is a
record whose `kind` is `@anyName`; `@exact` with `names`; `@suffix` with
`value`; or `@allOf` or `@anyOf` with `items`.

A policy is normalized before it is hashed or shown: nested `allOf` or
`anyOf` of the same kind are flattened, duplicate items removed, items
sorted by their canonical bytes, single-item wrappers removed, and sets
sorted. Sequences such as path components keep their order. The hash
identifies this normal form; two policies with the same meaning may hash
differently.

A policy has at most 4,096 nodes and entries in total and a depth of at most
64, counting ancestors acquired by restriction. A larger one is
`ScopeError.policyTooComplex`. These numbers are an engineering default of
the first tool profile, not a measured capacity.

### Filesystem policies

A file rule selects files below `prefix`: `children` means exactly one
further name, `tree` any positive number of further names. A directory rule
selects directories: `self` is `prefix` itself, `children` one name further,
`tree` the prefix and everything below.

- **Actions.** `read` covers `DirReadBytes` and `DirReadText`; `replace` and
  `create` cover the two write interfaces for an existing and a new file;
  `list` covers `DirList`; `stat` covers `DirStat` for the kind of rule it
  appears in. A grant's interface and its policy both have to allow a
  request. Append, remove, and rename do not exist yet.
- **A file rule grants no listing.** Permission to read `.txt` files does
  not permit enumerating a directory.
- **Paths.** A `RelativePath` (`@std.fs`) is a list of names, built by the
  fallible constructor `RelativePath(names)`. A name is non-empty and is
  rejected if it is `.` or `..`, contains `/`, `\`, `:`, NUL, or a control
  character, ends in a dot or a space, or is a reserved Windows device name
  with or without an extension. Absolute, drive-relative, UNC, and device
  paths cannot be represented. Nothing is expanded, decoded, or stripped.
  Prefixes are compared name by name, never as strings, so `reports` does
  not cover `reportsOld`.
- **Names.** A name test compares exact Unicode scalars, with no case
  folding and no normalization. `suffix(".txt")` accepts `notes.txt` and
  `.txt` and rejects `notes.TXT` and `notes.txt.exe`. An empty suffix is
  invalid. A suffix says nothing about a file's contents.

The driver MUST enforce a file rule as follows.

1. Resolve each request from the directory object the grant holds, one name
   at a time, without following symbolic links, junctions, or other reparse
   points and without crossing a mount point.
2. Check the name the filesystem actually reports for the opened entry and
   its kind. A different-case or short-name alias MUST NOT satisfy a name
   test the real name fails.
3. Perform the access on the object that was opened and checked. Checking a
   path and then reopening it by name is not conforming.
4. Never truncate or create before authorization. Creating a new file is
   exclusive: an existing entry, under any alias, is not replaced.
5. Keep no intermediate directory handle between operations. Every
   operation walks from the grant's directory again.
6. Where the platform cannot provide these guarantees, return
   `FsError.unsupported` before acting. There is no unchecked fallback.

Two limits of this design are part of the contract and MUST appear in what a
user is shown when approving a target:

- **Authority follows the opened directory, not its path.** If the directory
  is renamed, the grant still refers to it; if another directory takes its
  old path, the grant does not refer to that one.
- **Hard links.** A permitted file name can be a hard link to a file that
  also has a name elsewhere. A policy does not establish where data came
  from or that a write is seen only under the permitted name. A read-only
  grant still cannot write.

### HTTP policies

**Reserved.** No network operation exists in this specification, so this
section carries no conformance obligation yet. It fixes the policy form a
network operation MUST use when an integration gate adds one.

An `@httpRule` has `origin` (scheme `@http` or `@https`, a host, and a port
from 1 to 65535, with default ports written out), `peers` (a set of address
prefixes the connection may reach; empty denies everything, and a DNS answer
is never approval by itself), `methods` (a non-empty set of `@GET`, `@HEAD`,
`@POST`, `@PUT`, `@PATCH`, `@DELETE`, `@OPTIONS`), and `targets` (a
non-empty list of `@anyTarget`, `@exactTarget`, or `@prefixTarget`, the
last compared by path components exactly as filesystem prefixes are, with a
query that is `@anyQuery` or `@exactQuery`).

The four fields of one rule stay together: permitting `GET` on one origin and
`POST` on another does not permit `POST` on the first. A driver MUST NOT
follow redirects automatically, use an ambient proxy, cookie store, or
credentials, or connect to an address outside `peers`; it MUST connect to
the address it checked, without a second lookup. A target restriction
describes the request sent, not what the server does with it.

### Limits

A grant's limits count invocations and payload bytes in and out, in encoded
bytes. They do not bound time, processor use, or metadata traffic. A child
grant can only tighten a limit, and every copy and descendant of a grant
draws on the same ancestor counters, which the runtime's broker owns. The
broker reserves before an action, charges actual progress, and does not
refund an action whose outcome is unknown. A counter that would overflow
denies. Exhaustion is the typed error `limitExceeded`.

## Delivery and typed outcomes

Every host-bound operation has a **delivery class** in the host registry. It
is registry data, not source: a source-declared operation member has none
and behaves as the operations it awaits.

| Class | Meaning |
| --- | --- |
| `@once` | The runtime never dispatches one invocation twice |
| `@repeatable` | The runtime may retry, only under the duplicate-safety contract the operation states |
| `@unknown` | No promise; the runtime does not retry after a possible submission |

All seventeen initial operations are `@once`. Two `await`s of one plan are
two invocations. Suspending and resuming around a host call MUST NOT repeat
it. `@once` is not exactly-once delivery: there is no deduplication across
a crash and no rollback.

Every host operation returns a `Result`. Its error type distinguishes at
least these cases, in addition to the operation's own:

| Case | Meaning |
| --- | --- |
| `denied` | The grant's policy does not permit the request; nothing was attempted |
| `limitExceeded` | A limit would be exceeded; nothing was attempted |
| `unsupported` | The platform cannot enforce the grant for this action; nothing was attempted |
| `outcomeUnknown` | The action was submitted and its result is not known |

"Nothing was attempted", "failed", and "may have happened" stay distinct.
There is no implicit conversion between error types.

## Standard operation inventory

Each row of the host registry, major version 2, is one operation interface
in an embedded module. The first parameter of every member is the grant. The
[runtime chapter](06-runtime.md#host-registry) gives each signature and its
exact behavior.

| Module | Interface and member | Wire symbol | Resource |
| --- | --- | --- | --- |
| `@std.io` | `StdinText.read` | `io.stdinText` | standard input, as text |
| `@std.io` | `StdinBytes.read` | `io.stdinBytes` | standard input, as bytes |
| `@std.io` | `StdoutText.write` | `io.stdoutText` | standard output |
| `@std.io` | `StdoutBytes.write` | `io.stdoutBytes` | standard output |
| `@std.io` | `StderrText.write` | `io.stderrText` | standard error |
| `@std.io` | `StderrBytes.write` | `io.stderrBytes` | standard error |
| `@std.fs` | `DirReadBytes.read` | `fs.readFile` | a file's bytes |
| `@std.fs` | `DirReadText.read` | `fs.readText` | a file's UTF-8 text |
| `@std.fs` | `DirList.entries` | `fs.readEntries` | a directory's entries |
| `@std.fs` | `DirWriteBytes.write` | `fs.writeFile` | create or replace a file |
| `@std.fs` | `DirWriteText.write` | `fs.writeText` | create or replace a text file |
| `@std.fs` | `DirStat.stat` | `fs.metadataStat` | metadata of a file or directory |
| `@std.env` | `EnvReadVar.read` | `env.readVar` | a named set of variables |
| `@std.env` | `EnvReadArgs.read` | `env.readArgs` | the argument list |
| `@std.time` | `WallClock.read` | `time.nowWall` | the wall clock |
| `@std.time` | `MonotonicClock.read` | `time.nowMonotonic` | the monotonic clock |
| `@std.random` | `RandomBytes.generate` | `random.generateBytes` | the random source |

Examples in this specification that use `NetSend` illustrate types only; no
such interface is in the inventory.

## Tests and model profiles

A test runs in a fresh, deterministic, in-memory world with its own grants,
counters, clock, and random source. A row gives a test no grant. A test
obtains fake grants only from the `TestWorld` it receives, as the
[programs chapter](04-programs-and-packages.md#test-worlds-and-grants)
defines; each `TestWorld` member performs `TestWorld` and nothing else, and
the test's row MUST also name whatever it then performs with the grants. No
test reaches a real provider.

A package may implement an operation interface in Vibra, for example an
in-memory directory for its own tests. Such an implementation holds no real
grant. A comparison of a real driver against its model is sampled evidence
with a stated domain; it does not prove them equivalent.

## Exclusions

- a dedicated effect declaration, effect handlers, and resumptions;
- row variables, row subtyping, and any relation between roots;
- ambient authority, permission prompts, and source-declared host bindings;
- types that identify a particular resource instance, and proof of least
  authority;
- revocation, single-use and linear grants, and information-flow tracking;
- network, process, signal, timer, and streaming operations; append, remove,
  rename, and directory creation; filtered directory listing;
- grants for actor endpoints, which the concurrency release defines;
- operating-system sandboxing of native code.
