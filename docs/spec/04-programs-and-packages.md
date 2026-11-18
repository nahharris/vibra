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
list, and `[:]` the empty dict. A field name is an identifier, so no field
of any schema is a reserved word or contains a hyphen. A record's fields are
unique. A later duplicate dict key is an error in VIBON, `@data.duplicateKey`,
where source would let it replace the earlier one.

This is the only VIBON grammar. Every compiler-owned document is written in
it: project and lock files, host worlds, audit traces, build manifests,
conformance snapshots, and canonical value encodings. Their schemas, in this
and the other chapters, are stated as records, lists, and dicts and mean
these forms.

### Schemas

Each compiler-owned format defines a closed record schema and a version
atom, carried in its first field `format`. A schema states, for every field,
its type and whether it is required or optional; for every dict slot, its
key type; and for every number slot, its exact width. There is no null
value: an absent optional field is omitted from the document, and the
[JSON projection](05-tooling.md#machine-contracts) of a document writes
`null` for it. A field map that may be empty is a dict, never a record,
because `{}` is the empty tuple. A schema may give an optional field a
default, which the reader supplies when the field is absent.

A reader validates a document in this order, and reports one fault once:

1. lexical and grammatical faults, with the
   [source language](01-source-language.md#lexical-structure)'s codes:
   `@syntax.invalidEncoding`, the literal codes, `@syntax.unmatchedDelimiter`,
   and `@syntax.invalidForm` for a form outside the data grammar;
2. a repeated record field, `@data.duplicateField` at the later field, and a
   repeated dict key after decoding, `@data.duplicateKey` at the later key;
3. a `format` the reader does not implement, `@data.invalidValue` at the
   `format` field; a reader never guesses or upgrades;
4. shape, field by field in schema order: a required field missing, an
   unknown field, a value of the wrong form (a list where a record is
   required, a tuple of the wrong arity), `@data.invalidShape` at the field
   or the value;
5. value: a literal of the right form whose value the slot does not admit,
   `@data.invalidValue` at the value. This covers a number of the wrong
   width or out of range, an unsuffixed number in a slot with no number
   type, an atom outside a closed set, a string that fails a stated
   constraint, and a document whose fields relate wrongly.

An unsuffixed number is accepted by the grammar and typed by the schema slot
it lands in. A slot with no number type rejects it. In canonical output every
number carries its suffix.

An atom in VIBON is an atom value. A typed schema may declare one slot to be
an entity reference of one stated kind; only that slot is resolved, during
project checking, and the resolved entity MUST exist (`@name.unknownSymbol`
at the slot) and have that kind (`@name.wrongEntityKind` at the slot). No
decoder infers a slot's role from an atom's spelling. Decoding validates
shape and value and never consults the source graph.

### Decoded values and canonical bytes

A document is compared, sorted, and hashed by its **decoded value**, never
by its spelling. A string or character decodes to its scalar sequence, so
`"a"` and `"\u{61}"` are one key; a number decodes to its type and value, so
`1` and `1u8` are one `U8` key once the schema types the slot and `0.0f64`
and `-0.0f64` are two `F64` values; an atom decodes to its parts; a compound
decodes recursively, a record with its fields in canonical name order. Two
dict keys are duplicates when their decoded values are equal under the key
type's canonical order of the
[type chapter](02-type-system.md#dict-keys-equality-and-order). A
grammar-only reader, which has no schema, compares decoded values with the
written suffix as part of a number's value.

**Canonical serialization** is defined for a document validated against a
schema, and it is what every identity hash covers and what the toolchain
writes for every document that has no human author: lock files, build
records, snapshots, and value encodings. It is comment-free and fully
determined by the decoded value and the schema:

- UTF-8, LF line endings, one trailing newline, no trailing whitespace;
- one space between tokens; none inside brackets, before `(`, or before
  `:`; one after `:`;
- a string as the shortest literal: `\"`, `\\`, `\n`, `\r`, and `\t` for
  those scalars, `\u{H}` with the fewest upper-case hexadecimal digits for
  any other scalar below U+0020 and for U+007F, and every other scalar
  written directly; a character likewise, with `\'` for the quote;
- an integer as its minimal decimal digits and its exact suffix; a float as
  the [canonical text](06-runtime.md#floating-point-operations) of the
  runtime chapter and its exact suffix; `true` and `false`; an atom as its
  parts;
- a record's fields in schema order, with absent optional fields omitted; a
  dict's entries in the canonical order of its key type; a list's elements
  in their order;
- an empty compound as `{}`, `[]`, or `[:]`; any other compound on one line
  when the whole line fits within 88 columns, otherwise its opener, each
  entry on its own line one step of two spaces in, and its closer on its own
  line at the opener's indentation.

The **source-preserving formatter** is a different operation. `vibra fmt`
and the project edits of the [tooling chapter](05-tooling.md#formatting)
keep comments, written spellings, and the written order of dict entries, and
apply only the layout rules; they never produce canonical bytes, and a
hash is never taken over their output.

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
          resource: {kind: @argumentPath index: 0u64 source: @args}
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
| project | `format`, `package`, `targets`, `dependencies` | all required; `targets` a list with at least one target; `dependencies` a dict from `Str` name to dependency, possibly empty |
| package | `name`, `version` | strings; `name` satisfies the source identifier grammar, is not a reserved word, and is camelCase; `version` is one semantic version, never a range |
| target | `name`, `kind`, `root`, `entry`, `effects`, `grants` | `name` is a one-part atom whose part is not a reserved word; `kind` is `@bin` or `@lib`; `root` a `Str`; a binary requires `entry` (an entity reference), `effects` (a list of entity references), and `grants` (a dict from leaf atom to grant binding); a library omits all three |
| grant binding | `provider`, `resource`, `policy`, `limits`, `after` | `provider`, `resource`, `after` required; `policy` and `limits` optional; see [Targets](#targets) |
| path dependency | `kind`, `path`, `target` | `kind` is `@path`; `path` a `Str` confined beneath the project root; `target` an optional atom |
| Git dependency | `kind`, `git`, `rev`, `target` | `kind` is `@git`; `git` is an HTTPS URL without userinfo; `rev` is 40 lower-case hexadecimal digits; `target` an optional atom |

A `format` other than `@project.v2` is rejected under the
[schema rules](#schemas); there is no fallback and no automatic migration.
`project.vib` is never searched.

Project tooling preserves comments where it can and rewrites changed data
with the source-preserving formatter.

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
grammar and MUST NOT be a reserved word, or the path is
`@module.invalidSegment`. Names are compared with
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
from every target root. A target or dependency named `tests` or `std` is
`@project.reservedUnitName`.

## Packages, units, and targets

A package has a name and a semantic version, which are provenance: they are
part of every declaration's identity and never appear in a reference.
Dependency selection is exact; there is no version solving.

A **unit** is one local target, the reserved unit `@tests`, the embedded
standard library `@std`, or one dependency name. The first part of every
code-reference atom names a unit. Target and dependency names share one
namespace with the two reserved units. Because roots are disjoint, every
module belongs to exactly one unit and has exactly one canonical path.

The units a module can name are closed: its own unit, `@std`, and the
dependencies its project declares; a module of `@tests` can also name every
local target. A path whose first part is any other unit, including `@tests`
from a target and an undeclared dependency, is `@module.unknownPath` at the
reference. Together with the acyclic dependency graph this keeps the unit
graph acyclic.

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
or the project is `@effect.outsideCeiling` at the `effects` field, related
to the entry's `performs` entry or to the recipe. An interface in the list
that neither the entry nor a startup step performs is the warning
`@contract.unusedEffect` at that entry of the list. The list grants
nothing.

`grants` is a dict of **binding recipes**, one for each grant the entry
takes, keyed by the name of what it binds.

Each recipe binds one **grant leaf**. A parameter of an operation interface
type, or of `Option` of one, is a leaf, named by the parameter: `@notes`. A
parameter of a record type has one leaf for each field, named by the
parameter and the field, and so on through nested records: `@logs.reader`.
The dict's keys MUST be exactly the entry's leaves. A missing or extra key
is `@grant.invalidBinding` at the `grants` field, related to the parameter.

A recipe has:

- `provider`: an atom naming a registered issuer of the table below. The
  leaf's type, or the payload of its `Option`, MUST be one of the issuer's
  grant types.
- `resource`: a record that says which resource the issuer binds, of a kind
  the issuer accepts. A resource that is read from the process (an
  `@argumentPath` or a `@variablePath`) names in `source` the leaf whose
  grant performs that read: a leaf of type `EnvReadArgs` for an argument, a
  leaf of type `EnvReadVar` whose `names` include the variable for a
  variable. No recipe reads the environment by itself.
- `policy`: for `@host.fs.directory` only, and required there: a
  `@scopePolicy.v1` document of family `@filesystem`, defined in the
  [effects chapter](03-effects.md#scope-policies). Its rules name this
  leaf as their root. The grant permits exactly the operation the leaf's
  interface names, within the policy. A policy describes; it
  authorizes nothing by itself.
- `limits`: optional record of `invocations`, `inputBytes`, and
  `outputBytes`, each an optional `U64` upper bound for the life of the
  process.
- `after`: a list, without repeats, of leaves this recipe must be acquired
  after. It MUST contain the recipe's `source` leaf when it has one. Every
  name in it MUST be a key of `grants`. The `after` relation MUST be
  acyclic, or the project is `@grant.bindingCycle`.

| Provider | Grant types | Resource kinds |
| --- | --- | --- |
| `@host.fs.directory` | `DirReadBytes`, `DirReadText`, `DirList`, `DirWriteBytes`, `DirWriteText`, `DirStat` | `@fixedPath` with `path`; `@argumentPath` with `index` and `source`; `@variablePath` with `name` and `source` |
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
startup, through the `source` leaf's grant; the read is a startup step whose
row is that grant's interface, and it is recorded in the
[audit trace](06-runtime.md#audit-trace). In each case the issuer opens the
directory once and the grant holds that directory object; a path string is
never authority. The table is closed; a project cannot add a provider.

Project checking validates every recipe: the issuer exists and issues the
leaf's type, the resource kind is one the issuer accepts, the `source` leaf
exists with the grant type the read needs, the policy is well-formed for the
issuer and names this leaf as its root, and `after` is well-formed. Any
other case is `@grant.invalidBinding` at the offending field; an invalid
policy is `@grant.invalidPolicy`. A missing or invalid recipe fails the
build, for a required and an optional leaf alike.

### Startup order

The runtime acquires the leaves one at a time. A leaf is **ready** when
every leaf in its `after` list has been acquired or bound absent. Repeatedly,
among the ready leaves not yet acquired, the runtime acquires the one whose
full canonical atom spelling (`@notes`, `@logs.reader`) is smallest in
UTF-8 byte order, and recomputes readiness after each acquisition. Because
`after` is acyclic, the sequence reaches every leaf. This is the only
startup order; the project file's written order plays no part. The effects,
tooling, and runtime chapters refer to it.

An issuer **refuses** a leaf in exactly these cases:

| Provider | Refusal |
| --- | --- |
| `@host.fs.directory` | the path does not name a directory the process can open; an `@argumentPath` index at or beyond the end of the argument list; a `@variablePath` variable that is not present |
| `@host.io.stdin`, `@host.io.stdout`, `@host.io.stderr` | the process has no such stream |
| `@host.env.vars`, `@host.env.args`, `@host.time.wall`, `@host.time.monotonic`, `@host.random` | never |

A leaf whose `source` leaf was bound absent is refused as well, because its
read cannot happen. A refused optional leaf is bound to `Option.none()`. A
refused required leaf stops the sequence: startup fails with
`@grant.requiredUnavailable` and the entry does not run. Any other failure
to acquire (a driver that cannot enforce the grant on this platform, an
operating-system error) is not a refusal: it fails startup for a required
and an optional leaf alike, and a driver the platform does not support is
rejected at admission, as `@tool.unavailable`, before any leaf is acquired.

On a failed startup, and after the entry returns, the acquired grants are
released in reverse acquisition order. The
[runtime chapter](06-runtime.md#entry-outcome) defines the outcomes and how
the exit code is reported.

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
occupies, so its members are reached through it: `Ordering.less`. A path
that resolves to anything else, such as a member of a declaration, a case,
or a field, is `@name.wrongEntityKind` at the import.

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
record. Check, test, run, and build work offline from the project tree and
the lock. Local dependencies are not copied and are fingerprinted in every
snapshot.

| Field | Type | Meaning |
| --- | --- | --- |
| `format` | `@projectLock.v2` | required |
| `project` | `Str` digest | `sha256:` and the SHA-256 of the exact bytes of `project.vibon` when the lock was written |
| `dependencies` | dict from `Str` name to lock entry | one entry per declared dependency, no more and no fewer |

A lock entry has `kind` (`@path` or `@git`); for `@path`, `path` (`Str`);
for `@git`, `git` (`Str`), `rev` (`Str`, 40 lower-case hexadecimal digits),
`vendorPath` (`Str`, the project-relative vendor directory), and
`contentHash` (`Str`, the [content hash](#content-hash) of the vendored
tree); then `target` (optional atom) and `dependencies` (list of `Str`, the
names the dependency's own project declares, sorted by UTF-8 bytes).

```vibon
{
  format: @projectLock.v2
  project: "sha256:244210e48437b6556980a70249a99369934a352429034cef9d7bd253b3bf2c01"
  dependencies: [
    "money": {
      kind: @git
      git: "https://example.com/money.git"
      rev: "0123456789abcdef0123456789abcdef01234567"
      vendorPath: "dep/money"
      contentHash: "sha256:8ea702dc6eeac8cc73563dad98200ef105fcb23af61a46a23afce628c2ad5068"
      dependencies: []
    }
  ]
}
```

The four commands reject, at the project-data span named: a lock whose
`project` digest differs from the project file, or a dependency set that
differs from the project's, `@project.staleLock` at the lock's `format`
field; a missing vendor directory or vendored content whose content hash
differs from the lock, `@project.staleLock` at that lock entry; a path
dependency outside the project root, `@module.pathEscape` at its `path`;
and an import of an undeclared unit, `@module.unknownPath` at the import as
[Packages, units, and targets](#packages-units-and-targets) states.

#### Content hash

The content hash of a file tree is `sha256:` and 64 lower-case hexadecimal
digits of the SHA-256 of these bytes, in order:

1. the ASCII text `vibraContentHashV2`;
2. one zero byte;
3. the number of regular files, as a big-endian 64-bit integer;
4. per file, in the order of the UTF-8 bytes of their paths, two byte
   strings each preceded by its length as a big-endian 64-bit integer: the
   tree-relative path with `/` separators, then the file's exact bytes.

Directories contribute nothing, and a symbolic link or junction in the tree
is `@module.pathEscape`. Test vector: one file `src/lib.vib` holding the
bytes `fn f returns answer: Str do return "ok" endfn` hashes to
`sha256:8ea702dc6eeac8cc73563dad98200ef105fcb23af61a46a23afce628c2ad5068`.

There is no registry, version range, automatic upgrade, lifecycle script, or
dependency-provided executable.

### The standard library

The standard library is embedded in the toolchain at build time. Its
authority is that embedding: nothing a project supplies can add to it or
replace it. `check`, `run`, `test`, and `build` never read it from the
project, an installation directory, a cache, or the network.

Its manifest is a closed `@stdlibManifest.v2` record. At load the
toolchain checks every module against its digest; every `external`
declaration against the registry's signature and owner; every `native`
symbol against an implemented native; and that each language role is
claimed exactly once, on a declaration of the shape the
[type chapter](02-type-system.md#language-core-and-standard-library)
requires. A mismatch is a toolchain defect, reported as
`@runtime.invalidCheckedProgram` with the `@command.trap` result, never a
fallback.

| Field | Type | Meaning |
| --- | --- | --- |
| `format` | `@stdlibManifest.v2` | required |
| `package` | record `name: Str`, `version: Str` | the embedded package |
| `modules` | list of module entries sorted by `path` | every embedded module |
| `symbols` | record `compiler`, `host`, `native`: each a list of `Str` sorted by UTF-8 bytes | the wire symbols the modules may bind |

A module entry has `path` (the module atom), `digest` (`Str`, `sha256:` and
the SHA-256 of the module's exact bytes), and `languageRoles` (list of role
atoms, possibly empty).

```vibon
{
  format: @stdlibManifest.v2
  package: {name: "std" version: "0.0.2"}
  modules: [
    {
      path: @std.bool
      digest: "sha256:65c20520d3bde9ee2d53288d20df389fec9bce119c15722e95628d3005cf220f"
      languageRoles: [@bool]
    }
    {
      path: @std.builtin
      digest: "sha256:65c20520d3bde9ee2d53288d20df389fec9bce119c15722e95628d3005cf220f"
      languageRoles: []
    }
  ]
  symbols: {
    compiler: ["i32.add-checked" "list.length" "list.next"]
    host: ["io.stdoutText"]
    native: []
  }
}
```

The package enters resolution as unit `@std`. Its modules are:

| Module | Contents |
| --- | --- |
| `@std.core` | `Equatable`, `Ordered`, `Ordering`, `From`, `TryFrom`, `ConversionError`, `ArithmeticError` |
| `@std.bool`, `@std.str`, `@std.bytes` | the three wrapper and variant types and their members |
| `@std.option`, `@std.result` | `Option`, `Result` |
| `@std.iter` | `Iter` and the four adapter types |
| `@std.builtin` | the declarations of the builtin types, with their members and implementations; never imported, reached through the prelude |
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
snapshot but of no target. A target module cannot name `@tests`
(`@module.unknownPath`). A `test` declaration outside `@tests` is
`@module.testOutsideTests`.

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
the checker evaluates it and knows it succeeds. `files:` is the labelled
parameter of `directory`, and the `assert.equal` call types its first
operand from its second, as the
[type chapter](02-type-system.md#inference-and-checking) describes.

A test's identity is its module and its decoded name. Names compare by exact
scalar sequence and MUST be unique within a module (`@name.redeclaration`).
Discovery order is ascending module identity by UTF-8 bytes, then declaration
order.

A module that declares a test MUST contain an import that resolves to the
module `@std.assert`, under any local name
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

`TestWorld` has these members. Each is a `proc` that takes the world as its
one required parameter `{world: Self}`, takes its fixture data as labelled
parameters with the constant defaults shown, performs only `TestWorld`, and
cannot fail.

| Member | Labelled parameters | Returns |
| --- | --- | --- |
| `directory` | `files: Dict(RelativePath Str) be [:]` | a record of grants over one new fake directory holding those UTF-8 files: `{readBytes: DirReadBytes readText: DirReadText list: DirList writeBytes: DirWriteBytes writeText: DirWriteText stat: DirStat}` |
| `streams` | `stdin: Str be ""` | `{stdinText: StdinText stdinBytes: StdinBytes stdoutText: StdoutText stdoutBytes: StdoutBytes stderrText: StderrText stderrBytes: StderrBytes}` |
| `environment` | `variables: Dict(Str Str) be [:]`, `arguments: List(Str) be []` | `{variables: EnvReadVar arguments: EnvReadArgs}` |
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
and `<target>.build.vibon`, a canonical `@build.v2` record. The metadata
describes; it is not executable policy. Identical toolchain, snapshot,
target, platform, and options MUST produce byte-identical outputs.

| Field | Type | Meaning |
| --- | --- | --- |
| `format` | `@build.v2` | required |
| `toolchain` | a [`@toolchain.v2`](05-tooling.md#snapshot-claim-and-witness-identity) record | the exact toolchain |
| `revision` | `Str` | the workspace revision the build was taken from |
| `target` | atom | the target's name |
| `triple` | `Str` | the platform triple |
| `sources` | list of `{sourceId: Str digest: Str}` sorted by `sourceId` | every source the target and its dependencies contributed; `digest` is `sha256:` and the SHA-256 of the exact bytes |
| `dependencies` | list of `{name: Str contentHash: Str}` sorted by `name` | every dependency, with its content hash |
| `effects` | list of [entity reference records](06-runtime.md#types) | the target's admission ceiling, sorted by `path` |
| `grants` | dict from leaf atom to grant binding | the target's `grants` dict, as validated |
| `registry` | record `major: U32`, `minor: U32`, `symbols: List(Str)` sorted | the host registry version the executable requires and the wire symbols it binds |
| `outputs` | list of `{path: Str digest: Str}` sorted by `path` | every emitted file |
| `semanticIdentity` | `Str` | `sha256:` and the SHA-256 of `vibraSemanticIdentityV2`, a zero byte, and the canonical bytes of this record with `outputs`, `semanticIdentity`, and `artifactIdentity` absent |
| `artifactIdentity` | `Str` | `sha256:` and the SHA-256 of the executable's exact bytes |

```vibon
{
  format: @build.v2
  toolchain: {
    format: @toolchain.v2
    version: "0.0.2"
    grammar: "sourceV2"
    checker: "staticV2"
    stdlib: "sha256:65c20520d3bde9ee2d53288d20df389fec9bce119c15722e95628d3005cf220f"
    drivers: "systemV2"
    policy: "scopePolicyV1"
    registry: {major: 2u32 minor: 0u32}
  }
  revision: "sha256:33d677dc829d6e953467ca5c9f3ffe396839cb1184fbeaba1e34703ee2fa4ff1"
  target: @hello
  triple: "x86_64-pc-windows-msvc"
  sources: [
    {
      sourceId: "src/hello/main.vib"
      digest: "sha256:65c20520d3bde9ee2d53288d20df389fec9bce119c15722e95628d3005cf220f"
    }
  ]
  dependencies: []
  effects: [
    {path: @std.io.StdoutText seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"}
  ]
  grants: [
    @out: {provider: @host.io.stdout resource: {kind: @processStream} after: []}
  ]
  registry: {major: 2u32 minor: 0u32 symbols: ["io.stdoutText"]}
  outputs: [
    {path: "hello.exe" digest: "sha256:65c20520d3bde9ee2d53288d20df389fec9bce119c15722e95628d3005cf220f"}
  ]
  semanticIdentity: "sha256:65c20520d3bde9ee2d53288d20df389fec9bce119c15722e95628d3005cf220f"
  artifactIdentity: "sha256:65c20520d3bde9ee2d53288d20df389fec9bce119c15722e95628d3005cf220f"
}
```

The runtime chapter defines native execution and the conditions under which
the WebAssembly transition backend remains available until it is removed.

## Exclusions

- import cycles and package dependency cycles;
- a package registry, version ranges, and a dependency solver;
- user-supplied host worlds, recorded-run replay, and real host resources in
  tests;
- user-defined issuers, providers, or registry symbols;
- executable project files, build scripts, and lifecycle hooks.
