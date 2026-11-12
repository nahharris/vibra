# Vibra programs and packages

Status: normative target
Implementation status: M1 supplies generic VIBON data and structural query
metadata; M2 decodes projects, resolves local modules, and supports init,
check, run, and test; M3 embeds the standard library as the toolchain input.
Ordinary dependency delivery and build products remain unimplemented.
See the [release roadmap](../roadmap/README.md) for delivery status.
`run` and `test` execute the reference interpreter. Milestone 4 Step 13 closes
the Stage 4B contracts of this chapter, the `host` list and effect modules of the
standard-library manifest, the test host world, and the target `effects`
diagnostics, and implements none of them; Steps 14 to 18 do.

## VIBON data documents

Vibra Object Notation (VIBON) is the non-executable `.vibon` document grammar
for compiler-owned persistent data. `.vib` source and `.vibon` data share the
lexical reader and literal spellings, but the file extension selects one
document grammar before parsing; contents are never used to guess the mode. A
VIBON document is exactly one value from this closed subset:

```ebnf
data = string | character | boolean | integer | float | void | atom-name
     | "(", "record", { label, data }, ")"
     | "(", "array", { data }, ")"
     | "(", "tuple", { data }, ")"
     | "(", "dict", { data, data }, ")" ;
```

Records contain unique labelled fields. Dicts contain alternating key/value
forms directly and require an even number of forms. Bare symbols, applications,
imports, bindings, declarations, and host operations are not data and MUST be
rejected in a VIBON document. The data is parsed and validated, never executed.
Characters, `void`, and suffixed numerics use the source reader's literal
spelling and carry the same values and exact primitive types.

Each compiler-owned format defines a closed record schema and a version atom.
Unknown, duplicate, or missing fields are errors. Generic records retain source
field order until a typed schema supplies an explicit order. Generic dicts sort
keys by the complete canonical encoded key bytes. Canonical output uses the
source formatter's whitespace rules, schema or generic field order, canonical
key order for dicts, LF endings, and one trailing newline.

An atom parsed by the generic VIBON grammar is an atom value. A typed schema
may declare a particular slot to be an entity reference; only then is that atom
resolved to a canonical code identity. For example, `format: @project.v1` is a
version atom, a dependency dict key `@std` is an alias atom, and an entry in a
target's `effects` array is an effect-entity reference. Every schema slot
declares exactly one role, and an entity-reference slot additionally declares
the one entity kind it requires. No decoder may infer the role from the atom's
spelling.

Decoding and resolution are separate phases. Decoding validates document
grammar, schema shape, and the syntactic form of every atom, and never consults
the source graph. An entity-reference slot is resolved during project checking,
once the lock, vendored dependencies, and source graph are available; the
resolved entity MUST exist and MUST have the kind its slot requires. An
atom-value slot is never resolved in either phase.

The source graph MUST reject `.vibon` as a module extension, and a persistent
data loader MUST reject `.vib`. Vibra has no extension fallback, content sniffing,
or compatibility bridge between the two grammars. A document presented to the
wrong loader emits `@data.invalid-extension` before its contents are parsed.

This literal subset is Vibra's object notation. Project files, lock files, and
build metadata use it instead of persistent JSON. JSON remains the machine
interchange format for CLI and MCP responses.

The step 4–10 reader supplies the extension-selected shared lexical/document
mode boundary, the shared literal/name surface, and generic VIBON value
validation with canonical data formatting. Project-specific record schemas and
source-graph resolution remain later milestone work.

## Project file

A project is rooted by `project.vibon`. It contains one `@project.v1` record:

```vibon
(record
  format: @project.v1
  package: (record
    name: "hello"
    version: "0.1.0")
  targets: (array
    (record
      name: @hello
      kind: @bin
      root: "src/hello"
      entry: @hello.main.main
      effects: (array @std.fs.read @std.io.stdout)))
  dependencies: (dict
    @std (record
      kind: @git
      git: "https://github.com/nahharris/vibra-stdlib.git"
      rev: "0123456789abcdef0123456789abcdef01234567"
      target: @core)))
```

### M2 `@project.v1` schema

The M2 decoder closes the following typed schema. Records may be written in any
field order, but canonical formatting uses the order shown. Unknown fields,
missing required fields, and wrong value kinds are `@data.invalid-shape`;
duplicate record labels are `@data.duplicate-field`, and duplicate dependency
dict keys are `@data.duplicate-key`. The decoder retains each field and value
span with its source identity and does not resolve an atom or read a path.

| Record | Field order | Type | Requiredness and constraint |
| --- | --- | --- | --- |
| project | `format`, `package`, `targets`, `dependencies` | atom, record, array, dict | all required; `format` is exactly `@project.v1`; `targets` contains at least one target; `dependencies` may be empty |
| package | `name`, `version` | string, string | both required; `name` is kebab-case and `version` is one semantic version, never a range |
| target | `name`, `kind`, `root`, `entry`, `effects` | atom, atom, string, atom, array | `name`, `kind`, and `root` required; `name` is one kebab-name atom component; `kind` is `@bin` or `@lib`; a binary requires `entry` and `effects`; a library omits both |
| path dependency | `kind`, `path`, `target` | atom, string, atom | `kind` is `@path`; `path` required; `target` optional |
| Git dependency | `kind`, `git`, `rev`, `target` | atom, string, string, atom | `kind` is `@git`; `git` is HTTPS; `rev` is exactly 40 lowercase hexadecimal characters; `target` optional |

The dependency value is selected by its `kind` field. A dependency dict key is
an alias atom value whose spelling is one kebab-name component. `format`, target `name` and `kind`, and dependency `kind`
are atom values selected by their schema slots. Target `entry` is an entity
reference requiring a declaration; each target `effects` item is an entity
reference requiring an effect root; and dependency `target` is an entity
reference requiring a library target. These references are retained as typed
unresolved values by the decoder and are resolved only after the source graph
and lock inputs exist. An atom's spelling never changes its role.

The M2 decoder accepts only the records above and the generic VIBON literals
they contain. It performs no filesystem discovery, dependency sync, network
access, or source resolution. `root` and path-dependency `path` strings are
opaque schema values in M2: every valid VIBON string is accepted, including
relative, absolute-looking, and traversal-looking spellings. Path syntax,
normalization, containment, target-root overlap,
entry-kind, and dependency-target diagnostics belong to the later graph and
resolver phases; the decoder must report the shape error before those phases
when the record is malformed.

M2 decodes this record through a closed typed schema before it acquires any
source files. Schema-selected atom roles are retained as values until the
source graph and resolver phases have explicit inputs. The standard library is
embedded in the toolchain, as the standard-library input section below states;
ordinary local/Git dependency sync and lock generation remain Milestone 5 work. A syntactically valid project feature
outside the selected M2 profile reports `@tool.unavailable` rather than being
silently ignored or executed through a fallback.

Project tooling preserves comments when possible and rewrites changed data in
canonical form. There is no executable `(project ...)` declaration and no
legacy project format fallback. `project.vib` is not searched or accepted as a
project document.

### M2 project discovery and source snapshot

The workspace discovery API accepts an existing directory or an existing file.
For a directory it starts at that directory; for a file it starts at the
file's parent directory. It canonicalizes the start before searching and then
examines that directory followed by each parent up to the filesystem root.
The first directory containing an exact regular file named `project.vibon` is
the project root. A caller that supplies a missing path, a non-directory and
non-file path, or a path whose ancestor chain contains no such file receives
`@project.not-found` at empty span `0..0` with no source ID; discovery never
searches a sibling, child, or unrelated working directory. If the nearest
exact file exists but cannot be read or decoded, discovery reports that
failure and MUST NOT fall back to an older ancestor. The marker itself MUST be
a regular file in its containing directory; a marker symlink or junction is a
project filesystem failure. `project.vib` is not a marker and is rejected only
when a caller explicitly presents it to the data loader, which emits
`@data.invalid-extension`.

Discovery returns the canonical project root, the canonical marker path, and
the typed project value. The source identity of the marker is exactly
`project.vibon`; all later source identities are project-relative paths using
`/` separators. The discovery and snapshot APIs never read a dependency path,
clone a Git URL, consult a cache, or inspect a lock file.

Before source walking, every target `root` MUST be a non-empty relative path
with no `.` or `..` component. Its canonical directory MUST exist, be a
directory, and be contained by the canonical project root using path-component
comparison. A failure is `@project.invalid-target-root` at the target's root
string span; a non-security filesystem failure is `@project.io-error` at the
same span. Canonical target roots MUST be pairwise disjoint. Equality and
nested containment in either direction emit `@project.overlapping-target-roots`
at the later target's root span with a related span for the earlier root. These
checks MUST finish before any target directory is enumerated or any `.vib`
bytes are read.

Source walking uses `symlink_metadata` and canonical path components. A
symlink or junction encountered at a target root or below it MUST resolve to a
canonical path inside both the project root and the owning source root; an
escaping link emits `@module.path-escape`, and a dangling or unreadable link
emits `@module.io-error`. In-root links are allowed, but canonical directory
identity is tracked: a cycle is reported as `@module.path-escape`, and an
already visited canonical directory is skipped so aliases cannot duplicate a
module. A linked file is admitted once, at the lexicographically first
project-relative path. A source root itself MUST NOT be a link; a target root
is canonicalized as part of target-root validation. No source byte is read
until these confinement checks pass.

The project also reserves the optional top-level `tests/` directory as the
source root of one test-only unit named `@tests`. If the directory is absent,
the unit has no modules. If present, it MUST be a directory and is canonicalized
and walked with the same confinement, link, ordering, extension, and segment
rules as a target root. A non-directory or invalid root emits
`@project.invalid-target-root` at the empty span of project-relative path
`tests`; a test root that escapes the project emits `@module.path-escape`.
An in-project `tests/` root that is itself a symlink emits
`@project.invalid-target-root` at that same empty path span. The test root MUST
be disjoint from
every target root. A target root equal to or nested under `tests/` emits
`@project.overlapping-target-roots` at that target's `root` field, with no
related span for the synthetic test root. Validate the test root and its
disjointness from all target roots before enumerating any source. The
synthetic `@tests` unit is always reserved, including when the directory is
absent. A target name or dependency alias `tests` emits
`@project.reserved-unit-name` at the declared name or alias span.

Within each target, entries are sorted by their `/`-separated project-relative
path before descent. A regular file is a source module only when its extension
is exactly `.vib`; `.vibon`, files with another extension, and hidden/editor
files are ignored as data or unrelated files. There is no extension search and
no implicit index module. Every path segment in a source file or directory
name MUST be one kebab-name component; otherwise `@module.invalid-segment` is
reported at the path's empty span. A file `text.vib` and directory `text/`
claim the same module path and emit `@module.file-directory-collision` before
any module is parsed. Source IDs are stable project-relative paths, bytes are
copied exactly into an immutable snapshot, and repeated snapshots from an
unchanged tree have identical unit/module order, IDs, and bytes.

The source graph contains one explicit unit for each local target, the
reserved `@tests` unit, and one explicit, unresolved dependency edge for each
declared dependency. The `@tests` unit has kind `@lib` in the graph snapshot,
but it is not a project target or a dependency and has no target entry. A
dependency edge retains its alias, kind, target value, and source span. Graph
construction MUST NOT resolve, fetch, inspect, or silently discard a dependency.
Each unsupported edge reports `@tool.unavailable` at its dependency alias
while remaining present in the graph. The graph accepts only the immutable
snapshot produced by this workspace boundary. Later resolver phases receive
no ambient filesystem handle.

## Packages and targets

A package has a kebab-case name and semantic version used as source identity.
Vibra does not solve version ranges; dependency selection is exact. The package
name and version are provenance and never appear in a reference position, so
they are strings rather than atoms.

A target has a unique one-component kebab atom name, kind `@bin` or `@lib`, and a source root. Every
root MUST remain inside the project after canonical path resolution, and roots
MUST be pairwise disjoint: no root may equal or contain another. Every module
therefore belongs to exactly one target and has exactly one canonical path.
Overlapping roots emit `@project.overlapping-target-roots`. Target and
dependency names share one project namespace and cannot collide.

A *unit* is one local target, the reserved test unit `@tests`, or one dependency
alias. The unit is the root of code reference: the first component of every
code-reference atom names a unit, and the remaining components address an
entity beneath it. Only the test runner and `@tests` modules may resolve the
`@tests` unit; a local target's import graph does not contain it.

A binary target record MUST contain `entry` and `effects`; a library target
record MUST omit both. A library has no execution entry and no index module: its
surface is every public declaration of every module under its root, reached by
import. An `entry` on a library target is `@project.entry-on-library`. The
binary `effects` array is the entry's complete static effect ceiling and
execution consent as defined by the effects chapter, which also closes the
diagnostics of an unknown, mistyped, or repeated root in it.

`entry` is a declaration reference. Its first component MUST be the target's own
name, so an entry always resolves inside its own target's root; any other unit
is `@project.entry-outside-target`. Because roots are disjoint, no declaration
is nameable by the entry of more than one target.

Three failures are distinguished. A path that resolves to nothing emits the
ordinary resolution diagnostic, `@module.unknown-path` or
`@name.unknown-symbol`. A path that resolves to an entity that is not a
module-level `defn` emits `@name.wrong-entity-kind` and names the entity it
found. A path that resolves to such a `defn` whose signature is not an entry
signature emits `@project.invalid-entry-signature`.

An entry signature has no parameters and a result of either `void` or
`result void e` for a nominal error type `e`; `never` is neither, so an entry
result of `never` or an error type of `never` is not an entry signature. An entry
that returns `err` ends `run` with `@command.program-failed` and exit code 6, as
the tooling chapter states; traps remain distinct.
The error type MUST NOT hold a function type, directly or through a declared
type, because the result is observed and a function has no observable value;
such a signature also emits `@project.invalid-entry-signature`.

The entry declaration need not be public and need not be named `main`. The
project document is a privileged referrer: naming a declaration in `entry`
creates no import edge and does not widen its visibility.

An omitted entry `effects:` is `()`. An effectful entry writes its ceiling, and
project checking compares its computed performed row with both that ceiling and
the target record.

The minimum initialized layout is:

```text
hello/
  project.vibon
  src/
    hello/
      main.vib
  tests/
```

## Modules and imports

Every `.vib` source file is one module. Its canonical module identity is its
target-relative path without the extension. A source file does not redeclare
that identity.

Every path segment under a target root MUST be a `kebab-name`, and `.vib` is the
only module extension, so every module is addressable as a dotted atom path. A
module file and a module directory of the same name MUST NOT both exist: a
module is a leaf or an interior node, never both. `text.vib` and `text/`
therefore cannot coexist under one root, and a root containing both emits
`@module.file-directory-collision` before any module is parsed.

Resolving `@unit.c1...cn` walks the components from that unit's root, descending
while a component names a directory and stopping at the first component that
names a `.vib` file. The layout rule above guarantees no step has a choice, so
the walk needs no content inspection, extension search, or directory index
fallback. Components remaining after that file are resolved against the module's
declarations as the type-system chapter defines. A path whose walk reaches no
module emits `@module.unknown-path`.

```vibra
(import text @std.str)
(import model @hello.model)
```

Every import has one explicit lexical alias and one atom entity reference whose
resolved entity MUST be a module or one public top-level declaration of a
module. The first atom component names a unit; the remaining components name a
module beneath that unit's source root, optionally followed by exactly one
declaration name. Thus
`@hello.model` resolves to the local `hello` target's `model.vib`, while
`@std.str` resolves through the `@std` dependency alias. The resolver never
guesses from the importing file's directory.

Resolution is total, but access is not. An atom path resolves to a private
declaration exactly as it resolves to a public one, and each referring position
then applies its own visibility rule: an import exposes only public
declarations, while the `entry` slot may name a private one. A private
declaration reached from an import emits `@name.private-access` at the
referring path and relates the declaration span. Duplicate aliases and
top-level declarations use `@name.redeclaration`; duplicate members under one
owner retain `@name.member-collision`. An import back edge emits
`@module.import-cycle` at the later edge and relates the earlier edge.

String paths, relative imports, absolute filesystem imports, glob imports,
implicit extension search, directory index fallback, re-exports, and import
cycles are errors. The atom is resolved only because the import grammar expects
an entity reference; the same atom in expression position remains an ordinary
value.

A module import makes only the target module alias visible. Public declarations
are referenced as `alias.name`; nested effect operations use
`alias.root.operation`. A declaration import, such as
`(import ordering @std.core.ordering)`, binds its alias to that one
declaration in every namespace the declaration occupies, so `ordering` then
names the type and `ordering.less` its variant; members of a declared type are
reached through the alias exactly as through the type name. Because the layout
rule makes a module a leaf, a path never denotes both a module and a
declaration. Apart from the prelude, the closed set of declarations the type
chapter lists (the core and role types and the values `true` and `false`),
the standard library is imported explicitly like any other package. The
prelude is fixed by the specification, cannot be extended by a package, and the
module that declares it, `@std.bool`, is in every checked graph.

## Dependencies and lock

Vibra supports the following forms.

- local dependencies whose record has `kind: @path` and `path:`; and
- Git dependencies whose record has `kind: @git`, an HTTPS `git:` URL, and a
  full 40-hex `rev:`.

For the M2 schema check, an HTTPS Git URL has the form `https://authority` with
an authority containing either DNS labels (ASCII letters, digits, and internal
hyphens) or a bracketed IPv6 literal, plus an optional decimal port from 1 to
65535. A path, query, or fragment may follow. Userinfo, an empty or malformed
host, control/whitespace characters, and non-HTTPS schemes are rejected. URL
fetching and repository-specific validation remain outside the decoder.

A dependency alias binds one `@lib` target of the dependency package, named by
the optional `target:` field, and never binds a package as a whole. An omitted
`target:` selects the package's only `@lib` target; a package exposing more than
one requires the field and otherwise emits
`@project.ambiguous-dependency-target`. A dependency alias MUST NOT bind a
`@bin` target, though a local `@bin` target remains a unit importable inside its
own project. One package MAY therefore be bound under several aliases, one per
library target.

`vibra project sync` exports exact Git revisions into `dep/<alias>/` without
`.git` metadata. It writes `project-lock.vibon`, a canonical generated data
record. The lock contains its format, project fingerprint, dependency edges,
source identities, revisions, content hashes, and vendor paths:

```vibon
(record
  format: @project-lock.v1
  project: "sha256:..."
  dependencies: (dict
    @std (record
      kind: @git
      source: "https://github.com/nahharris/vibra-stdlib.git"
      rev: "0123456789abcdef0123456789abcdef01234567"
      target: @core
      content: "sha256:..."
      vendor: "dep/std"
      dependencies: (array))))
```

Check, test, run, and build operate offline from the project tree and lock.
They reject a missing vendor tree, stale lock, changed vendored content, path
escape, or undeclared dependency. Local dependencies are not copied and are
fingerprinted on every workspace snapshot.

### Toolchain standard-library input

M3 replaces the M2 bootstrap input, with no transition period and no fallback
between the two. The signed artifact, detached signature, toolchain public key,
and their digests are retired. A signature verified against a key that is
compiled into the same binary as the signed bytes adds no authority: whoever
can change the embedded modules can change the embedded key. Authority instead
comes only from embedding: the standard library is part of the toolchain build,
and nothing a project supplies can add to it.

The input is the repository directory `stdlib/`. Its authority is
`stdlib/manifest.vibon`, a closed `@stdlib-manifest.v1` record with fields, in
order, `format`, `package-name`, `package-version`, `modules`, `compiler`,
`host`, `native`, and `assertions`. `package-name` is `"vibra-stdlib"` and
`package-version` is `"0.3.0"`. `modules` is a dict from each module atom to a
record with `path` (a `stdlib/src/`-relative slash path), `sha256` (the
`sha256:` digest of the module's exact bytes), and `role` (`@source` or
`@test-registry`). `compiler` lists every `@compiler` symbol the modules may
bind, `host` lists every `@host` registry symbol the modules may bind, `native`
lists every native implementation symbol the modules may name with `native:`,
together with the four that implement the `str` and `bytes` wrapper constructor
and pattern, which no module names (`docs/spec/06-runtime.md`), and
`assertions` lists the test-only assertion members.

The toolchain embeds the manifest and every listed module at build time. At
load it decodes the manifest through its closed record, hashes each embedded
module against its entry, and requires every `external: @compiler` declaration
to name a listed symbol whose signature matches the registry exactly. It
requires every `external: @host` declaration likewise to name a symbol of the
`host` list and of the host registry, with the registry's signature and with the
registry's owner root equal to the root of the enclosing `deffect`, and it
requires the `host` list to hold exactly the symbols the modules bind. Every
`native:` symbol MUST likewise be listed and implemented by the toolchain, and
every language role MUST be claimed by exactly one `role:` declaration. A
mismatch is an internal toolchain defect reported as an operational provenance
diagnostic, never a fallback. `check`, `run`, and `test` never read the
standard library from the project, the build checkout, the installation
directory, a vendor tree, a cache, or the network.

The admitted modules enter the resolver as the `vibra-stdlib@0.3.0` package
with unit `@std`, keep that provenance in every declaration identity, and are
imported explicitly, except for the prelude of the type chapter, whose
declarations every module sees. Every module that declares a prelude name, a
value or a declaration that plays a role, is in every checked graph: `@std.bool`,
`@std.core`, `@std.str`, `@std.bytes`, `@std.option`, `@std.result`,
`@std.iter`, and `@std.builtin`. So `check`, `run`, and `test` always load the
embedded library. A declaration outside this
embedded package that writes `external:` acquires no provider authority and is
rejected before execution. The module set of the package is `@std.core`,
`@std.option`, `@std.result`, `@std.bool`, `@std.str`,
`@std.bytes`, `@std.iter`, the builtin-member module `@std.builtin`, and the
test-registry module `@std.assert`. Milestone 4 adds the five effect modules
`@std.io`, `@std.fs`, `@std.env`, `@std.time`, and `@std.random`, which declare
the roots, operations, and error types of the host registry in the runtime
chapter. `@std.builtin` is never imported: it declares the members and the
implementations of the builtin numeric, `char`, `array`, and `dict` types,
whose members are reached through those type paths. Adding a module or symbol is a specification change to
this list and to the runtime registry, and replaces the manifest and the package
version in place.

The combined project and standard-library graph MUST keep source IDs unique
across all packages, because a source ID selects the document used for
diagnostic spans. A project module that reuses an embedded module's source ID
emits `@module.source-id-collision`; the affected selected graph is not type
checked or executed.

Builtin type names are reserved alias and module-level value spellings, as the
type chapter states, because their members are reached by the same dotted path
an alias would start.

There is no registry, version range, lock auto-upgrade, lifecycle script, or
dependency-provided executable.

## Tests

Every `.vib` file under the project `tests/` root is a module in the reserved
`@tests` unit. Its canonical module identity is `@tests` followed by the
slash-separated project-relative path beneath `tests/`, without `.vib`, with
slashes replaced by dots. For example, `tests/math/add.vib` is `@tests.math.add`.
There is no root/index module. Test modules are captured into the same immutable
workspace snapshot as target modules, with their exact project-relative source
IDs and bytes. They are not part of a target's module closure. A local target
module cannot import `@tests`; that import path emits `@module.unknown-path`.
Modules within `@tests` may import other `@tests` modules under the ordinary
module-resolution, cycle, and public-visibility rules.

Tests are declarations in those modules. A `test` declaration outside the
reserved `@tests` unit emits `@tool.unavailable` in M2 and is never admitted
into a target program:

```vibra
(import assert @std.assert)

(test "greets by name"
  (assert.equal (greet "Ada") "hello, Ada"))
```

A test's identity is its canonical module identity and decoded string name.
Names compare by exact Unicode scalar sequence without normalization and MUST
be unique within one module; repeating a name emits `@name.redeclaration` at
the later test name and relates the earlier name. The same string name in a
different test module is distinct. Test discovery order is ascending canonical
module identity by UTF-8 bytes, then source declaration order within a module.
The canonical string-literal spelling used for selectors and string values is
double-quoted; `\"` escapes a quote, `\\` escapes a backslash, and `\n`, `\r`,
and `\t` escape line feed, carriage return, and tab. Other C0 controls and DEL
use `\u{hex}`, where `hex` has one
to six lowercase hexadecimal digits and no leading zero. All other Unicode
scalars appear literally. Booleans use `true` or `false`; `i32` and `u64`
values use base-10 digits with their `i32` or `u64` suffix and no leading
zeroes or plus sign; zero is `0i32` or `0u64`. Characters use the canonical
EDN spelling: `\newline`, `\return`, `\space`, and `\tab`; other controls and
BMP whitespace use `\u` plus four uppercase hexadecimal digits; other scalars
use a backslash followed by the scalar. Assertion names in failures are canonical
`@std.assert.<member>` atoms. The same literal spelling rules apply to the
`expected` and `actual` strings in structured assertion failures.
The runner checks every selected test module and the modules in its import
closure for ordinary syntax, import, and type diagnostics before executing any
selected test. With no selector, all discovered tests are selected. An
explicit selector does not check unrelated test modules or target modules. For
availability checking, each selected test has a declaration-dependency closure:
start with the test declaration and its body, follow every resolved declaration
reference in that body, then recursively follow references in each reached
declaration's type, signature, and body until no new declaration is reached.
An M2-unavailable form produces `@tool.unavailable` for each selected test
whose declaration-dependency closure contains that form. Merely importing a
module does not make every declaration in it a dependency; a valid unavailable
form outside every selected test's declaration-dependency closure does not
affect that `test` command. A shared helper's unavailable form can therefore
make the tests that reference it unavailable while leaving other selected
tests runnable. A module that declares a test MUST explicitly import
`@std.assert`; helper modules without tests do not need that import. Test
imports use ordinary unit aliases and ordinary public-visibility rules; no
test-only prelude or private-access exception exists.

The test language permits declared effect ceilings. Since Milestone 4
Step 14 a test is checked against its written ceiling like a function, an
omitted `effects:` is empty, and a test whose ceiling holds only operations with
Vibra bodies runs. Selecting and running a test is consent to the roots of its written row, with no
target array involved, and a test cannot add effects to a test declaration. An
effectful test runs in the default host world of the runtime chapter, which
provides no host operation to call until the standard effect modules exist.

### M2 assertion contract

An M2 test module MUST import `@std.assert` explicitly. If a module declares
one or more tests but has no import targeting exactly `@std.assert`, emit one
`@module.missing-required-import` diagnostic at the string name of its first
test declaration. This is an ordinary source error: all selected tests are
invalid and none execute. The embedded `@std.assert` module exports exactly these
test-only assertion members; they are resolved by their canonical module
identity and are not user-definable external declarations:

| Member | Exact signature | Passing behavior |
| --- | --- | --- |
| `assert.true` | `bool -> void` | succeeds when the operand is `true` |
| `assert.false` | `bool -> void` | succeeds when the operand is `false` |
| `assert.equal-bool` | `bool bool -> void` | succeeds when both operands are the same boolean |
| `assert.equal-char` | `char char -> void` | succeeds when both operands have the same scalar |
| `assert.equal-str` | `str str -> void` | succeeds when both operands have the same Unicode scalar sequence |
| `assert.equal-i32` | `i32 i32 -> void` | succeeds when both operands have the same signed value |
| `assert.equal-u64` | `u64 u64 -> void` | succeeds when both operands have the same unsigned value |

These callable members are admitted only from declarations in the reserved
`@tests` unit. A reference to one from a local target declaration emits
`@tool.unavailable` at the reference span and prevents target execution;
importing `@std.assert` without referencing a member does not by itself make a
target unavailable.

The table is closed: another assertion name, generic assertion, implicit
conversion, collection assertion, or deferred operand type is
`@tool.unavailable` in M2. Assertion operands are checked and evaluated
left-to-right. A test body is a body sequence with result type `void`, and its
static effects row MUST be empty; assertion calls do not add an effect. A
`return` directly in a test body has no function to exit and emits
`@type.invalid-return`, while one inside a `lambda` in the test exits that
`lambda`.

A passing assertion returns `void`. A false assertion records one structured
test failure with the assertion's canonical name, the canonical literal forms
of its expected and actual values, and the assertion call's primary source
span, then stops that test's body. For `assert.true` and `assert.false`,
`expected` is the required boolean value, rendered `true` or `false`, and
`actual` is the operand. For an
`assert.equal-*` member, `expected` is the first operand and `actual` is the
second operand; both are rendered with the canonical literal formatter. It
does not throw, create a `result` value,
emit a host event, or become a runtime trap. The runner continues with the
next selected test using a fresh value state and empty audit trace. A test item
therefore has exactly one of `@test.passed`, `@test.assertion-failed`,
`@test.invalid`, `@test.unavailable`, or `@test.trap`. `@test.invalid` means
duplicate test identities or error-level syntax, import, or type diagnostics
prevented the selected suite from executing; every selected item is invalid
in that case, and none executes. Warning-level diagnostics remain attached to
the suite and to each selected test whose module or import closure contains
their primary source, but do not prevent execution or change a test outcome.
Suite-wide static diagnostics are attributed to each selected test whose module
or import closure contains the diagnostic's primary source; the same diagnostic
may occur in multiple test items. Item-owned diagnostics are attributed by the
test declaration that produced them, even when selected tests share a module:
an unavailable-form diagnostic belongs to each test whose declaration-
dependency closure contains the form, and a runtime trap diagnostic belongs
only to the test that trapped. `@test.unavailable` means the selected test's
declaration-dependency closure contains a valid but M2-unavailable form; that
item does not execute, but other selected tests may run after preflight when no
ordinary diagnostics block the suite. Invalid and unavailable items have empty
audit traces. A suite containing any non-passing item is not `@command.ok`.

Static type or import errors are reported before any selected test executes.
An unavailable assertion or test form retains its source span and reports
`@tool.unavailable`; it is never silently skipped. A runtime trap remains the
separate `@test.trap` outcome with its structured trap diagnostic and stops only
that test. The runner then starts the next selected test with fresh value state
and an empty audit trace. Empty test selection is a successful empty suite only
when the selector is omitted; an unknown explicit selector is invalid input.

The runner isolates each test's values and audit trace. Every test, effectful
or not, runs in a fresh copy of the deterministic in-memory host world of the
runtime chapter, for every root and with standard output and error going to the
test's audit trace, and a test supplies no provider, response, or mode of its
own. The world's clock, random generator, filesystem, environment, arguments,
and standard input are fields of one `@host-world.v1` document, and the default
world, empty and fixed, is what a test sees when nothing supplies another. A
conformance case may supply a world. User-supplied worlds and recorded-run
replay are outside the test contract, and the project has no field for a world.
Tests cannot name a nondeterministic provider, so there is no failure for a
dependency on one.

### M3 assertion contract

Stage 3A replaces the five monomorphic `assert.equal-*` members with one
generic member and keeps `assert.true` and `assert.false` unchanged. The
removed members are not retained as aliases.

| Member | Exact signature | Passing behavior |
| --- | --- | --- |
| `assert.equal` | `(expected t) (actual t) -> void`, `where: (t any)` | succeeds when both operands have the same canonical value encoding |

The comparison is the canonical value encoding of the runtime chapter, so it is
defined for every value except a function. An operand whose type is or
contains a `fn` type emits `@type.function-not-equatable` at that operand. The
encoding also supplies the `expected` and `actual` failure strings. Because it
compares encodings rather than calling `equatable`, `assert.equal` is a
test-only surface and grants no equality to ordinary code. Every other part of
the M2 assertion contract is unchanged.

## Build products

`vibra build <target>` emits:

- `<target>.wasm`, the deterministic program or library module;
- `<target>.build.vibon`, canonical `@build.v1` data containing toolchain,
  project, dependency, source, required-effect, required registry symbols and
  the derived registry minor of the runtime chapter, and module hashes; and
- optional human-readable diagnostics on stderr.

The metadata is descriptive and not executable policy. A conforming runner
checks source targets before execution, while another Wasm host is responsible
for its own embedding policy. Identical toolchain, project snapshot, target,
and build options MUST produce byte-identical outputs.

[Distribution research](../roadmap/scratchpad/foundations/README.md#other-distribution-work)
covers signing, publishing build products, and multi-artifact application
containers.

## M1 addressed codebase data

A **codebase** is an explicitly opened local store of checked project revisions. It is separate from a file workspace: file commands continue to capture their own workspace snapshot, and codebase commands never discover or reread a project from disk unless the command is explicitly <code>import</code>. A revision names an immutable checked graph and its exact input attachments; a named head is a mutable pointer to one revision.

### Codebase container and portable records

M1 supports SQLite's rollback-journal physical format with `DELETE` journal
mode. Before opening SQLite, the store MUST inspect the held confined file's
header and reject a WAL or other non-rollback journal format as
`@store.unsupported-version`. It MUST NOT change the input's journal mode or
create WAL/shared-memory sidecars, including when rejecting an unsupported
store. A truncated or non-SQLite header is `@store.corrupt`.

The M1 store uses SQLite as a private local physical container. The database file is not itself a <code>.vibon</code> document, a portable record interchange format, or an address input. Every authoritative object, source attachment, revision manifest, and operation journal payload MUST be exact canonical UTF-8 VIBON bytes under the versioned schemas in this section. SQL tables and indexes are derived storage structures. SQL text, row identifiers, insertion order, pages, journal bytes, and database-file bytes MUST NOT enter any content address. The public portable record contract is the canonical VIBON payload; M1 exposes no arbitrary SQL or general record-dump operation.

The store MUST reside on a supported local filesystem. It MUST NOT follow a symlink or junction while opening a path selected for creation, backup, restore, or export. Every read-write SQLite connection MUST use <code>synchronous=FULL</code>. New store, backup, restore, and exported-tree publication MUST use an exclusive same-filesystem no-replace operation; a check followed by an overwriting rename is forbidden. The operation MUST fail closed if the filesystem cannot provide that primitive. File and containing-directory metadata MUST be synchronized where the platform supports it. The contract guarantees that process interruption exposes an absent or complete newly published destination; it does not promise directory-metadata power-loss durability on Windows. Its schema version is explicit. A reader MUST reject a schema version newer than it supports and MUST NOT guess or opportunistically migrate it.

M1 supports a trusted local workspace: the caller MUST ensure that no untrusted principal can concurrently rename or rebind the canonical workspace root or any ancestor of an operation's paths. Under this precondition, path traversal MUST reject symlinks, junctions, and other reparse points. The store is not an isolation boundary against another local principal with write access to those directories. Shared writable workspaces and privileged execution over untrusted directories do not satisfy this precondition. In particular, SQLite's final-component no-follow flag does not protect Unix parent components against hostile concurrent replacement; M1 does not claim that protection.

The caller MUST use a local filesystem that honors SQLite locking, file synchronization, and exclusive no-replace publication. Network mounts and stores concurrently rewritten or replicated by a file-synchronization service are excluded from M1. Copying a live database file is not a supported backup operation; callers use the explicit consistent backup command.

### Checked objects and entity keys

A checked **object** is the addressable unit of checked meaning. M1 creates one object for each top-level declaration form (<code>def</code>, <code>defn</code>, <code>deftype</code>, <code>defint</code>, or <code>deffect</code>), one object for each test, and one object for each explicit <code>impl</code> block. Named members nested in a declaration form—type fields and variants, methods, interface contract members, and effect operations—are owned parts of that declaration object. Members nested in an <code>impl</code> block are owned parts of its implementation object. They are not separately addressed objects. A revision maps every resolver entity identity, including owned members, to its object address and canonical part path. A lookup of a member therefore identifies both its owner object and its part within it. Changing any owned part changes the owner object's address.

A declaration entity key contains the resolver's canonical declaration identity plus its entity kind as a separate field. The identity spelling is <code>@&lt;package-name&gt;@&lt;package-version&gt;/&lt;unit&gt;[.&lt;module-segment&gt;...][.&lt;owner-segment&gt;...]</code>; source atom paths such as <code>@app.main.run</code> are not canonical identities. The identity includes package name and version, unit, module path, owner path, and final name; the entity key additionally records the kind. A rename, module move, unit move, package-name change, or package-version change changes entity identity. Equal definitions with different entity identities, including different nominal <code>deftype</code>, <code>defint</code>, or <code>deffect</code> identities, remain distinct objects.

An implementation object key is the pair of the canonical normalized receiver type and canonical normalized applied interface target. The applied target is required even if the receiver and member spelling match another implementation. Each owned implementation member maps from the existing triple of contract-member identity, applied target, and receiver type to its block object plus part path. Generic type binders in these key types are normalized by lexical binder position.

Every object member has a closed kind, a canonical semantic definition, and zero or more typed resolved edges. The workspace engine MUST supply an immutable checked projection from its existing source snapshot, resolver, and checker. That projection contains object keys, checked definitions, source-origin and part mappings, and all resolved edges. The encoder MUST be a pure projection over this immutable input: it MUST NOT read files, databases, the clock, randomness, or the network, and MUST NOT resolve names or check programs again. A recovered, unchecked, unavailable, or diagnostically rejected declaration MUST NOT enter a published checked graph.

Each object edge identifies its owning member and canonical part path, its edge kind, and its resolved target entity or trusted external. The complete closed edge-kind set, ordering, deduplication rule, and canonical representation are defined in [Checked codebase encoding](codebase-encoding.md). The edge set MUST include every resolved input that affects checked meaning, including types, interfaces, constructors, values, calls, implementation contracts, effect roots, and trusted embedded declarations. A source spelling or import alias is never a resolved target.

### Canonical checked graph and addresses

The exact checked-definition node schema, binder traversal, normalized type encoding, entity and object keys, edge schema, SCC ordering, canonical component frame, and domain-separated component/object address bytes are defined in [Checked codebase encoding](codebase-encoding.md). That chapter is the single canonical source for graph bytes and required graph vectors. This chapter defines the project attachments, revision manifest, and storage lifecycle that include those graph records.

### Source attachments and revision manifests

Source text is attached separately from checked objects. Each project source document, test source document, and <code>project.vibon</code> is captured as exact valid UTF-8 bytes. The authoritative attachment record is a canonical VIBON value with exactly the fields <code>format</code>, <code>text</code> in that order:

~~~vibon
(record format: @source.v1 text: "(defn run () i32 1)")
~~~

Text decodes to the exact captured UTF-8 bytes. The raw source hash is lowercase SHA-256 of those bytes without a prefix domain. The source-attachment address is SHA-256 over ASCII <code>vibra-codebase-source-v1</code>, one <code>0x00</code> byte, and the exact canonical VIBON wrapper bytes, rendered with the <code>sha256:</code> spelling. The source ID is the workspace source identity; the origin is the project-relative path at which export materializes it. They are separate revision metadata, so a source attachment can keep its address while origin and entity identity change. Source spans are half-open UTF-8 byte ranges into the attachment identified by source ID. A trivia-only edit changes the source hash and attachment address while preserving checked-object addresses. Export writes stored attachment text exactly and MUST NOT format it implicitly. Project and source files that cannot be checked remain ordinary workspace drafts and MUST NOT be stored as published objects or attachments.

The revision's source attachments contain the local project, ordinary source, and test documents. Trusted embedded standard-library documents remain identified by the exact module hashes in <code>trusted</code>, and MUST NOT be materialized into a local project by export. An origin for a local object or owned part MUST name an attached source; an origin for a trusted standard-library object or part MUST name the corresponding verified embedded module whose exact bytes match the revision's trusted module hash. In both cases, spans MUST be valid UTF-8 boundaries within those exact bytes. Trusted origins MUST NOT be silently dropped or redirected to a current local file.

A revision record has exactly these fields in this order: <code>format</code>, <code>package</code>, <code>trusted</code>, <code>objects</code>, <code>sources</code>:

~~~vibon
(record
  format: @codebase-revision.v1
  package: (record name: "demo" version: "0.1.0")
  trusted: (array (record kind: @registry fingerprint: "sha256:..." registry: (record format: @codebase-registry.v1 provider: @compiler version: "compiler-v1" rows: (array (record symbol: "array.length" type-parameters: (array "t") signature: "..." native: false)))) (record kind: @registry fingerprint: "sha256:..." registry: (record format: @codebase-registry.v1 provider: @host version: "vibra_v1" minor: 0u32 snapshot: "registry vibra_v1 1.0\n..."))) (record kind: @stdlib package-name: "vibra-stdlib" package-version: "0.3.0" manifest: "sha256:..." modules: (array (record module: "@vibra-stdlib@0.3.0/std.io" source-hash: "sha256:..."))))
  objects: (array (record key: (record kind: @declaration identity: "@demo@0.1.0/app.main.run" entity-kind: @function) object: "sha256:..." part: (array)))
  sources: (array (record source-id: "src/main.vib" attachment: "sha256:..." source-hash: "sha256:..." origin: "src/main.vib")))
~~~

Every field is required and no unlisted field is allowed. <code>trusted</code> contains records with exactly the fields shown. The standard-library record has <code>kind</code>, <code>package-name</code>, <code>package-version</code>, <code>manifest</code>, and <code>modules</code>; each module record has <code>module</code> and <code>source-hash</code>. <code>manifest</code> is raw SHA-256 of the exact embedded <code>stdlib/manifest.vibon</code> bytes, and each module <code>source-hash</code> is raw SHA-256 of that module's exact bytes. Module records sort by canonical module identity. A registry record has exactly <code>kind</code>, <code>fingerprint</code>, and <code>registry</code>. <code>registry</code> is the complete canonical <code>@codebase-registry.v1</code> VIBON record defined in [Checked codebase encoding](codebase-encoding.md); <code>fingerprint</code> is its domain-separated address. The trusted array sorts by the tuple (kind atom spelling, provider/package identity), both compared as UTF-8 bytes. It therefore places the two <code>@registry</code> records in provider order (<code>@compiler</code>, then <code>@host</code>) before the <code>@stdlib</code> record. <code>objects</code> contains records with exactly <code>key</code>, <code>object</code>, and <code>part</code> in that order. It maps every local or admitted trusted declaration identity, test identity, implementation block, and implementation member to its object address and owned-part path. It sorts by the complete canonical VIBON bytes of <code>key</code>. Sources is sorted by UTF-8 source ID and contains the raw content hash, wrapper address, source ID, and project-relative origin. The exact <code>project.vibon</code> document is one source attachment with source ID <code>project.vibon</code>. Source origins and target/project data are revision-bound. The revision does not include its parent or a publication timestamp; identical complete snapshots have the same revision address irrespective of import order or head history.

Let R be canonical bytes of the complete <code>@codebase-revision.v1</code> record. The revision address is <code>sha256:</code> followed by lowercase hexadecimal SHA-256 of the ASCII bytes <code>vibra-codebase-revision-v1</code>, one <code>0x00</code> byte, then R. Revisions are immutable. A named head is a separately stored label and revision address; a head update never changes the revision address. Head names are lowercase kebab-case, with <code>main</code> as the default. A revision contains only checked objects whose reference closure is complete within that revision or explicitly admitted trusted input. A digest alone grants no access: all reads require an explicitly authorized opened store and either a resolved head or a revision in that store.

### Store publication and recovery

After process interruption, SQLite may require rollback-journal recovery before
it can serve a read. A read-only operation MUST NOT write to perform recovery.
If SQLite cannot read without recovery, it reports `@store.io-error` with
guidance to run the explicit read-write `codebase reindex` command and retry.
A read-write open lets SQLite recover the last committed state before records
are admitted; reindex then verifies authority and rebuilds derived indexes.
This recovery is distinct from migration or reconstructing committed records.

The workspace engine captures and checks all source inputs and computes canonical objects, attachments, indexes, and the candidate revision before opening the writer transaction. Publication compares the requested expected head inside a write transaction before writing candidate records. It then stores all missing authoritative objects and attachments, revision membership, derived indexes, and the new head in the same transaction. Process interruption or a rejected publication exposes either the previous complete state or the new complete state, never a partial revision or head. Successful commits use SQLite <code>synchronous=FULL</code>. The store synchronizes files and containing directories where supported; it does not claim Windows directory-metadata durability after power loss. A failed import or stale compare MUST NOT logically publish any candidate record. Re-importing current content at the same head is a successful no-op. If a caller loses the response after commit, it MUST read the named head to determine whether publication committed; it MUST NOT infer rollback from a lost response. A hash collision with different canonical bytes is <code>@store.corrupt</code> and MUST NOT overwrite existing data. Revisions and objects are not pruned in M1.

An expected head is either explicit absence or one exact revision address. If current state does not match that expectation, publication returns a stale-head outcome without rebasing. SQLite lock contention is bounded; exhausting the bound returns a distinct retryable busy outcome, not stale. A caller MUST retry busy only by resubmitting its original expected state. M1 uses a fixed 500 millisecond SQLite busy timeout. A reader resolves a named head once at operation start and reads from the immutable revision selected by that address. Reads never fall back to a newer head, a different revision, or current files.

M1 supports a single local SQLite file and SQLite's consistent backup mechanism. Database integrity, supported schema version, authoritative record hashes, reference closure, revision membership, source wrapper hashes, and trusted fingerprints are checked at open, publish, read, verify, backup, and restore boundaries as the operation requires. An unknown newer format is rejected. Index rows are derived and can be rebuilt only from verified authoritative objects, attachments, and revision mappings. Verify never repairs authoritative records. Backup creates a consistent standalone copy at a destination that did not exist. Restore accepts only a verified backup and creates a standalone store at a destination that did not exist; neither operation overwrites an existing destination. Rejected, corrupt, or unsupported input does not publish a revision or silently change authoritative data.
