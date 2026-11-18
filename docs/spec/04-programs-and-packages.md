# Vibra programs and packages

Status: normative target
Implementation status: the shipped v0.0.1 toolchain implements VIBON data,
project discovery, the source snapshot, local module resolution, the embedded
standard library, and tests, all for the previous source surface and the
`@project.v1` record. The `@project.v2` record, grant bindings, the entry
signature, test worlds obtained through `TestWorld`, dependency delivery, and
build products defined here are not implemented. See the
[release roadmap](../roadmap/README.md).

## VIBON data documents

Vibra Object Notation (VIBON) is the `.vibon` grammar for compiler-owned
persistent data. It is the data subset of Vibra source: the literals, and
the brace and bracket forms that hold them. A VIBON value is written exactly
as the same value is written in a `.vib` file. It has no names, no calls,
and no declarations, so it is parsed and validated and never executed.

The file extension selects the grammar before parsing; contents are never
used to guess. A document presented to the wrong loader is
`@data.invalidExtension` before its contents are read.

A VIBON document is exactly one value:

```ebnf
data   = string | character | boolean | integer | float | void | atom
       | tuple | record | list | dict ;
tuple  = "{", { data }, "}" ;
record = "{", field, { field }, "}" ;
field  = identifier, ":", data ;
list   = "[", { data }, "]" ;
dict   = "[", entry, { entry }, "]" | "[", ":", "]" ;
entry  = data, ":", data ;
```

Every terminal is spelled as in the
[source language](01-source-language.md#lexical-structure): literals keep
their numeric suffixes, `true` and `false` are the booleans, and `//` starts
a comment. There are no commas. `{}` is the empty tuple, `[]` the empty
list, and `[:]` the empty dict. A record's fields are unique. A later
duplicate dict key is an error in VIBON, `@data.duplicateKey`, where source
would let it replace the earlier one.

This is the only VIBON grammar. Every compiler-owned document is written in
it: project and lock files, host worlds, audit traces, build manifests,
conformance snapshots, and canonical value encodings. Their schemas, in this
and the other chapters, are stated as records, lists, and dicts and mean
these forms. The parenthesized notation of v0.0.1, such as `(record …)` and
`(array …)`, is not VIBON and no reader accepts it.

Each compiler-owned format defines a closed record schema and a version atom.
Unknown, duplicate, or missing fields are errors. A reader MUST reject a
format version it does not implement; it never guesses or upgrades. Canonical
output uses two-space indentation, schema field order, dict entries sorted
by the canonical encoded bytes of their keys, LF endings, and one trailing
newline.

An atom in VIBON is an atom value. A typed schema may declare one slot to be
an entity reference of one stated kind; only that slot is resolved, during
project checking, and the resolved entity MUST exist and have that kind. No
decoder infers a slot's role from an atom's spelling. Decoding validates
shape and never consults the source graph.

Project files, lock files, build metadata, host worlds, and canonical value
encodings use VIBON. JSON is the interchange format of CLI and MCP responses
only.

## Project file

A project is rooted by `project.vibon`, one `@project.v2` record:

```vibon
{
  format: @project.v2
  package: {name: "hello" version: "0.1.0"}
  targets: [
    {
      name: @hello
      kind: @bin
      root: "src/hello"
      entry: @hello.main.main
      effects: [@std.env.EnvReadArgs @std.fs.DirReadText @std.io.StdoutText]
      grants: [
        @args: {
          provider: @host.env.args
          resource: {kind: @processArguments}
          after: []
        }
        @notes: {
          provider: @host.fs.directory
          resource: {kind: @argumentPath index: 0u64}
          policy: {
            format: @scopePolicy.v1
            family: @filesystem
            expression: {
              kind: @fileRule
              root: @notes
              prefix: []
              depth: @tree
              actions: [@read]
              names: {kind: @suffix value: ".txt"}
            }
          }
          after: [@args]
        }
        @out: {
          provider: @host.io.stdout
          resource: {kind: @processStream}
          after: []
        }
      ]
    }
  ]
  dependencies: [:]
}
```

This target reads `.txt` files, and nothing else, anywhere under the
directory named by the program's first argument, and writes to standard
output.

| Record | Fields, in canonical order | Constraint |
| --- | --- | --- |
| project | `format`, `package`, `targets`, `dependencies` | all required; at least one target; `dependencies` may be empty |
| package | `name`, `version` | strings; `name` satisfies the source identifier grammar, in camelCase; `version` is one semantic version, never a range |
| target | `name`, `kind`, `root`, `entry`, `effects`, `grants` | `name` is a one-part atom that satisfies the source identifier grammar; `kind` is `@bin` or `@lib`; a binary requires `entry`, `effects`, and `grants`; a library omits all three |
| grant binding | `provider`, `resource`, `policy`, `limits`, `after` | `provider`, `resource`, `after` required; see [Targets](#targets) |
| path dependency | `kind`, `path`, `target` | `kind` is `@path`; `target` optional |
| Git dependency | `kind`, `git`, `rev`, `target` | `kind` is `@git`; `git` is an HTTPS URL without userinfo; `rev` is 40 lower-case hexadecimal digits; `target` optional |

Wrong shapes are `@data.invalidShape`; duplicate fields are
`@data.duplicateField`; duplicate dict keys are `@data.duplicateKey`. A
`@project.v1` document is rejected as an unsupported format; there is no
fallback and no automatic migration. `project.vib` is never searched.

Project tooling preserves comments where it can and rewrites changed data in
canonical form.

### Discovery and the source snapshot

Discovery starts at a given directory, or at a given file's parent, and
examines that directory and each ancestor. The first directory that contains
a regular file named exactly `project.vibon` is the project root. No such
file is `@project.notFound`. If the nearest marker cannot be read or
decoded, discovery reports that failure and does not fall back to an older
ancestor. A marker that is a symbolic link or junction is a project
filesystem failure.

Every target `root` MUST be a non-empty relative path without `.` or `..`
parts. Its canonical directory MUST exist inside the canonical project root,
compared by path components; otherwise it is `@project.invalidTargetRoot`.
Canonical target roots MUST be pairwise disjoint, neither equal nor nested,
or the later one is `@project.overlappingTargetRoots`. These checks finish
before any source directory is listed.

Below a root, a symbolic link or junction MUST resolve inside both the
project root and its own source root. An escaping link or a directory cycle
is `@module.pathEscape`; a dangling or unreadable link is
`@module.ioError`. A canonical directory is walked once, and a linked file
is admitted once, at its first path in sorted order. A source root MUST NOT
itself be a link. No source byte is read before these checks pass.

A regular file is a module only when its extension is exactly `.vib`. Every
file and directory name on a module's path MUST satisfy the source identifier
grammar, or the path is `@module.invalidSegment`. Names are compared with
exact case. Two paths under one root that differ only by case are
`@module.caseCollision` on every host, and on a case-insensitive filesystem
a reference whose case differs from the file's actual name is
`@module.unknownPath`. A file `text.vib` beside a directory `text/` is
`@module.fileDirectoryCollision`. Entries are sorted by project-relative
path. Source identities are project-relative paths with `/` separators, and
bytes are copied exactly into an immutable snapshot: repeated snapshots of an
unchanged tree are identical. Later phases receive the snapshot and no
filesystem access.

The optional top-level `tests/` directory is the source root of the reserved
test unit `@tests`, walked under the same rules and required to be disjoint
from every target root. A target or dependency named `tests` is
`@project.reservedUnitName`.

## Packages, units, and targets

A package has a name and a semantic version, which are provenance: they are
part of every declaration's identity and never appear in a reference.
Dependency selection is exact; there is no version solving.

A **unit** is one local target, the reserved unit `@tests`, or one dependency
name. The first part of every code-reference atom names a unit. Target and
dependency names share one namespace. Because roots are disjoint, every
module belongs to exactly one unit and has exactly one canonical path.

A library target has no entry. Its surface is every public declaration of
every module under its root.

### Targets

A binary target names an entry and states, as data, everything the program is
allowed to do.

`entry` is a declaration reference whose first part MUST be the target's own
name (`@project.entryOutsideTarget` otherwise). It MUST resolve to a
module-level `proc` with an **entry signature**:

- every parameter is a bare name, with no labelled parameters;
- every parameter type is a **grant shape**: an operation interface value
  type, `Option` of one, or a record whose fields are grant shapes; and
- the result type is `ExitCode` from `@std.process`, a wrapper over `U8`.

Anything else is `@project.invalidEntrySignature`; a path that resolves to
another kind of entity is `@name.wrongEntityKind`. The entry need not be
public or named `main`. Naming it in `entry` creates no import edge and does
not change its visibility. An `entry` on a library is
`@project.entryOnLibrary`.

```vibra
use @std.env.EnvReadArgs enduse
use @std.fs.DirReadText enduse
use @std.io.StdoutText enduse
use @std.process.ExitCode enduse

proc main
  takes {args: EnvReadArgs notes: DirReadText out: Option(StdoutText)}
  returns code: ExitCode
  performs {EnvReadArgs DirReadText StdoutText}
  do
    return ExitCode(0u8)
endproc
```

A declared type over a grant, such as `type LogDir of DirWriteText` with a
private representation, is not an entry parameter type. Only its own module
can construct it, so no project file can bind one: the entry takes the
operation interface, and the program calls that module to wrap it. A tuple
of grants is not one either, because a binding names what it binds.

`effects` is the target's **static admission ceiling**: a list of entity
references to operation interfaces, without repeats. The entry's written
`performs` row, and the row of every startup step, MUST be contained in it,
or the project is rejected. The list grants nothing. Its diagnostics are the
[effects chapter](03-effects.md)'s.

`grants` is a dict of **binding recipes**, one for each grant the entry
takes, keyed by the name of what it binds.

Each recipe binds one **leaf**. A parameter of an operation interface type,
or of `Option` of one, is a leaf, named by the parameter: `@notes`. A
parameter of a record type has one leaf for each field, named by the
parameter and the field, and so on through nested records: `@logs.reader`.
The dict's keys MUST be exactly the entry's leaves. A missing or extra key
is `@grant.invalidBinding`.

A recipe has:

- `provider`: an atom naming a registered issuer of the table below. The
  leaf's type, or the payload of its `Option`, MUST be one of the issuer's
  grant types.
- `resource`: a record that says which resource the issuer binds, of a kind
  the issuer accepts.
- `policy`: for `@host.fs.directory` only, and required there: a
  `@scopePolicy.v1` document of family `@filesystem`, defined in the
  [effects chapter](03-effects.md#scope-policies). Its rules name this
  leaf as their root. The grant permits exactly the operation the leaf's
  interface names, within the policy. A policy describes; it
  authorizes nothing by itself.
- `limits`: optional record of `invocations`, `inputBytes`, and
  `outputBytes`, each a `U64` upper bound for the life of the process.
- `after`: the names of other leaves this recipe reads from or must be
  acquired after. A recipe whose resource is taken from an argument or an
  environment variable MUST list the leaf that grants that read, and that
  leaf MUST exist: no recipe reads the environment by itself. The
  `after` relation MUST be acyclic, or the project is
  `@grant.bindingCycle`.

| Provider | Grant types | Resource kinds |
| --- | --- | --- |
| `@host.fs.directory` | `DirReadBytes`, `DirReadText`, `DirList`, `DirWriteBytes`, `DirWriteText`, `DirStat` | `@fixedPath` with `path`; `@argumentPath` with `index`; `@variablePath` with `name` |
| `@host.io.stdin` | `StdinText`, `StdinBytes` | `@processStream` |
| `@host.io.stdout` | `StdoutText`, `StdoutBytes` | `@processStream` |
| `@host.io.stderr` | `StderrText`, `StderrBytes` | `@processStream` |
| `@host.env.vars` | `EnvReadVar` | `@variableSet` with `names`, a list of exact names |
| `@host.env.args` | `EnvReadArgs` | `@processArguments` |
| `@host.time.wall` | `WallClock` | `@systemClock` |
| `@host.time.monotonic` | `MonotonicClock` | `@systemClock` |
| `@host.random` | `RandomBytes` | `@systemRandom` |

A `@fixedPath` is absolute, or relative to the process's working directory
at startup. A path taken from an argument or variable is read once, at
startup. In each case the issuer opens the directory once and the grant
holds that directory object; a path string is never authority. The table is
closed; a project cannot add a provider.


Project checking validates every recipe: the issuer exists, the policy is
well-formed for it, and the types agree. A missing or invalid recipe fails
the build, for a required and an optional leaf alike. At startup the
runtime acquires grants in the order `after` requires, with ties in the
order of their names. A required
grant that cannot be acquired fails startup before the entry runs; an
optional one that is refused in the way its issuer specifies becomes
`Option.none()`. Grants are released in the reverse order after the entry
returns. The [runtime chapter](06-runtime.md#entry-outcome) defines the
outcomes and how the exit code is reported.

## Modules and imports

Every `.vib` file is one module. Its identity is its unit followed by its
root-relative path without the extension, with separators replaced by dots.
A file does not declare its own identity.

Resolving `@unit.c1…cn` walks the parts from the unit's root, descending
while a part names a directory and stopping at the first part that names a
`.vib` file. The layout rules leave no step a choice. Parts that remain
resolve against the module's declarations. A walk that reaches no module is
`@module.unknownPath`.

```vibra
use @std.fs.DirReadText enduse
use @hello.model enduse
use @billing.cents as money enduse
```

An import names a module or one public top-level declaration of a module.
The local name is the path's last part unless `as` gives another. A module
import makes the module's public declarations reachable as `name.Decl`. A
declaration import binds the name to that declaration in every namespace it
occupies, so its members are reached through it: `Ordering.less`.

Resolution is total and access is not. A path resolves to a private
declaration as it does to a public one, and the referring position applies
its own rule: an import of a private declaration is `@name.privateAccess`,
relating the declaration, while a target's `entry` may name a private one.

**Imports MUST NOT form a cycle.** An import whose target module imports,
directly or through any chain of imports, the importing module is
`@module.importCycle` at the later edge, relating the earlier edges. The
same holds between units: package dependencies form a directed acyclic graph,
and a dependency cycle is `@project.dependencyCycle`. Declarations inside
one module may refer to each other freely.

String paths, relative imports, wildcard and selective imports, extension
search, directory index modules, and re-exports do not exist.

Apart from the prelude, the standard library is imported like any other
package. The modules that declare the prelude are in every checked graph.

## Dependencies and lock

A dependency is local (`kind: @path`) or an exact Git revision
(`kind: @git`). A dependency name binds one `@lib` target of the dependency's
package, named by `target`; when omitted, the package's only library target
is selected, and a package with several is
`@project.ambiguousDependencyTarget`. A dependency never binds a binary
target.

`vibra project sync` exports each Git revision into `dep/<name>/` without
repository metadata and writes `project-lock.vibon`, one `@projectLock.v2`
record of the project fingerprint and, per dependency, its kind, source,
revision, target, content hash, vendor path, and own dependencies. Check,
test, run, and build work offline from the project tree and the lock. They
reject a missing vendor tree, a stale lock, changed vendored content, a path
escape, and an undeclared dependency. Local dependencies are not copied and
are fingerprinted in every snapshot.

There is no registry, version range, automatic upgrade, lifecycle script, or
dependency-provided executable.

### The standard library

The standard library is embedded in the toolchain at build time. Its
authority is that embedding: nothing a project supplies can add to it or
replace it. `check`, `run`, `test`, and `build` never read it from the
project, an installation directory, a cache, or the network.

Its manifest is a closed `@stdlibManifest.v2` record listing the package
name and version, every module with its path, digest, and role, and the
symbols the modules may bind: `compiler` intrinsics, `host` operations,
`native` implementations, and `assertions`. At load the toolchain checks
every module against its digest; every `external` declaration against the
registry's signature and owner; every `native` symbol against an implemented
native; and that each language role is claimed exactly once. A mismatch is a
toolchain defect, reported as an operational diagnostic, never a fallback.

The package enters resolution as unit `@std`. Its modules are:

| Module | Contents |
| --- | --- |
| `@std.core` | `Equatable`, `Ordered`, `Ordering`, `From`, `TryFrom`, `ConversionError`, `ArithmeticError` |
| `@std.bool`, `@std.str`, `@std.bytes` | the three wrapper and variant types and their members |
| `@std.option`, `@std.result` | `Option`, `Result` |
| `@std.iter` | `Iter` and the four adapter types |
| `@std.builtin` | members and implementations of the compiler-owned types; never imported |
| `@std.assert` | assertions, usable only from `@tests` |
| `@std.test` | `TestWorld` |
| `@std.process` | `ExitCode` |
| `@std.policy` | `ScopeError`, and the policy, selector, and limit types of the [scope policies](03-effects.md#scope-policies) |
| `@std.io`, `@std.fs`, `@std.env`, `@std.time`, `@std.random` | the operation interfaces and error types of the host registry |

Adding a module or symbol is a change to this specification and to the
runtime registry. A project module whose source identity collides with an
embedded one is `@module.sourceIdCollision`.

## Tests

Every `.vib` file under `tests/` is a module of the unit `@tests`:
`tests/math/add.vib` is `@tests.math.add`. Test modules are part of the
snapshot but of no target. A target module cannot import `@tests`. A `test`
declaration outside `@tests` is `@module.testOutsideTests`.

```vibra
use @std.assert enduse
use @std.test.TestWorld enduse
use @std.fs.DirReadText enduse
use @std.fs.RelativePath enduse
use @hello.notes enduse

test "counts the lines of a note"
  takes {world: TestWorld}
  performs {TestWorld DirReadText}
  do
    let Result.ok(value: path) be RelativePath(["a.txt"]) endlet
    let {readText: reader} be
      await TestWorld.directory(world files: [path: "x\ny"])
    endlet
    assert.equal(Result.ok(value: 2u64) await notes.countLines(reader path))
endtest
```

The first `let` needs no `else`: the construction has constant operands, so
the checker evaluates it and knows it succeeds.

A test's identity is its module and its decoded name. Names compare by exact
scalar sequence and MUST be unique within a module (`@name.redeclaration`).
Discovery order is ascending module identity by UTF-8 bytes, then declaration
order.

A module that declares a test MUST import `@std.assert`
(`@module.missingRequiredImport`). There is no test-only prelude and no
private-access exception.

### Test worlds and grants

Every test runs in a fresh, deterministic, in-memory host world, in a
procedure context the runner owns. The world's clock, random source,
filesystem, environment, arguments, and standard streams are those of the
default `@hostWorld.v2` document unless a conformance case supplies
another. A project has no field for a world.

A test has no ambient authority. A test without `takes` receives no grant and
can perform no host operation. A test with `takes {world: TestWorld}`
receives the sealed root grant of its own world from the runner. `TestWorld`
is an operation interface: its procedures derive grants for that world's
resources, each scoped by explicit fixture data, and it MUST appear in the
test's `performs` row like any other operation interface. No binding recipe
can name `TestWorld`, so no target can obtain one.

`TestWorld` has these members. Each takes the world first, performs only
`TestWorld`, and cannot fail.

| Member | Further parameters | Returns |
| --- | --- | --- |
| `directory` | `files: Dict(RelativePath Str)` | a record of grants over one new fake directory holding those UTF-8 files: `{readBytes: DirReadBytes readText: DirReadText list: DirList writeBytes: DirWriteBytes writeText: DirWriteText stat: DirStat}` |
| `streams` | `stdin: Str` | `{stdinText: StdinText stdinBytes: StdinBytes stdoutText: StdoutText stdoutBytes: StdoutBytes stderrText: StderrText stderrBytes: StderrBytes}` |
| `environment` | `variables: Dict(Str Str)`, `arguments: List(Str)` | `{variables: EnvReadVar arguments: EnvReadArgs}` |
| `clocks` | none | `{wall: WallClock monotonic: MonotonicClock}` |
| `random` | none | `RandomBytes` |
| `output` | none | `{stdout: Bytes stderr: Bytes}`: what the test has written so far |

A test takes the grants it needs out of a returned record by pattern and
narrows them with the ordinary restriction functions. Each call of
`directory` makes a separate directory. Grants from one test's world are
unusable in another.

A test is checked against its written row. Selecting a test is consent to run
it in its fake world only; no test reaches a real host resource.

### Assertions

`@std.assert` declares exactly these members, usable only from `@tests`; a
reference from a target is `@module.assertionOutsideTests`.

| Member | Signature | Passes when |
| --- | --- | --- |
| `assert.true` | `Fn({Bool} Void)` | the operand is `true` |
| `assert.false` | `Fn({Bool} Void)` | the operand is `false` |
| `assert.equal(T)` | `Fn({T T} Void)`, expected then actual | both operands have the same canonical value encoding |

An operand of `assert.equal` whose type contains a callable, plan, or grant
type is `@type.functionNotEquatable`. The comparison uses the canonical
value encoding and gives ordinary code no equality.

A false assertion records one structured failure, with the assertion's name,
the canonical encodings of the expected and actual values, and the call's
span, and stops that test's body. It is not a trap, a `Result`, or a host
event. Each test ends as exactly one of `@test.passed`,
`@test.assertionFailed`, `@test.invalid`, `@test.unavailable`, or
`@test.trap`.

Before any selected test runs, the runner checks every selected test module
and its import closure. An error there makes every selected test
`@test.invalid` and none runs. A selected test whose declaration-dependency
closure contains a form the toolchain does not yet implement is
`@test.unavailable` with `@tool.unavailable` at the form; other tests still
run. A trap ends only its own test. Each test starts with fresh values, a
fresh world, and an empty audit trace. With no selector all tests are
selected; an unknown explicit selector is invalid input.

## Build products

`vibra build <target>` emits a native executable for the selected platform
and `<target>.build.vibon`, a canonical `@build.v2` record of the toolchain,
project, dependency, and source identities, the target's admission ceiling
and grant bindings, the required registry symbols and registry version, and
the hashes of the outputs. The metadata describes; it is not executable
policy. Identical toolchain, snapshot, target, platform, and options MUST
produce byte-identical outputs.

The runtime chapter defines native execution and the conditions under which
the WebAssembly backend of v0.0.1 remains available until it is removed.

## Exclusions

- import cycles and package dependency cycles;
- a package registry, version ranges, and a dependency solver;
- user-supplied host worlds, recorded-run replay, and real host resources in
  tests;
- user-defined issuers, providers, or registry symbols;
- executable project files, build scripts, and lifecycle hooks.
