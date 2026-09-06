# Vibra v1 programs and packages

Status: normative target
Implementation status: milestone 1 steps 7 and 10 complete for generic VIBON grammar, canonical data, and structural query metadata; project schemas and resolution remain later work

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
     | "(", "map", { data, data }, ")" ;
```

Records contain unique labelled fields. Maps contain alternating key/value
forms directly and require an even number of forms. Bare symbols, applications,
imports, bindings, declarations, and host operations are not data and MUST be
rejected in a VIBON document. The data is parsed and validated, never executed.
Characters, `void`, and suffixed numerics use the source reader's literal
spelling and carry the same values and exact primitive types.

Each compiler-owned format defines a closed record schema and a version atom.
Unknown, duplicate, or missing fields are errors. Generic records retain source
field order until a typed schema supplies an explicit order. Generic maps sort
keys by the complete canonical encoded key bytes. Canonical output uses the
source formatter's whitespace rules, schema or generic field order, canonical
key order for maps, LF endings, and one trailing newline.

An atom parsed by the generic VIBON grammar is an atom value. A typed schema
may declare a particular slot to be an entity reference; only then is that atom
resolved to a canonical code identity. For example, `format: @project.v1` is a
version atom, a dependency map key `@std` is an alias atom, and an entry in a
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
data loader MUST reject `.vib`. V1 has no extension fallback, content sniffing,
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
  dependencies: (map
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
map keys are `@data.duplicate-key`. The decoder retains each field and value
span with its source identity and does not resolve an atom or read a path.

| Record | Field order | Type | Requiredness and constraint |
| --- | --- | --- | --- |
| project | `format`, `package`, `targets`, `dependencies` | atom, record, array, map | all required; `format` is exactly `@project.v1`; `targets` contains at least one target; `dependencies` may be empty |
| package | `name`, `version` | string, string | both required; `name` is kebab-case and `version` is one semantic version, never a range |
| target | `name`, `kind`, `root`, `entry`, `effects` | atom, atom, string, atom, array | `name`, `kind`, and `root` required; `name` is one kebab-name atom component; `kind` is `@bin` or `@lib`; a binary requires `entry` and `effects`; a library omits both |
| path dependency | `kind`, `path`, `target` | atom, string, atom | `kind` is `@path`; `path` required; `target` optional |
| Git dependency | `kind`, `git`, `rev`, `target` | atom, string, string, atom | `kind` is `@git`; `git` is HTTPS; `rev` is exactly 40 lowercase hexadecimal characters; `target` optional |

The dependency value is selected by its `kind` field. A dependency map key is
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
source graph and resolver phases have explicit inputs. M2's offline bootstrap
is repository-owned and hash-checked; ordinary local/Git dependency sync and
lock generation remain Milestone 5 work. A syntactically valid project feature
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
canonical path inside both the project root and the owning target root; an
escaping link emits `@module.path-escape`, and a dangling or unreadable link
emits `@module.io-error`. In-root links are allowed, but canonical directory
identity is tracked: a cycle is reported as `@module.path-escape`, and an
already visited canonical directory is skipped so aliases cannot duplicate a
module. A linked file is admitted once, at the lexicographically first
project-relative path. The root directory itself MUST NOT be a link; the target
root is canonicalized as part of root validation. No source byte is read until
these confinement checks pass.

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

The Step 3 source graph contains one explicit unit for each local target and
one explicit, unresolved dependency edge for each declared dependency. A
dependency edge retains its alias, kind, target value, and source span. Graph
construction MUST NOT resolve, fetch, inspect, or silently discard a dependency;
because ordinary dependency delivery is deferred to M5, each unsupported edge
reports `@tool.unavailable` at its dependency alias while remaining present in
the graph. The graph accepts only the immutable snapshot produced by this
workspace boundary; later resolver phases receive no ambient filesystem handle.

## Packages and targets

A package has a kebab-case name and semantic version used as source identity.
V1 does not solve version ranges; dependency selection is exact. The package
name and version are provenance and never appear in a reference position, so
they are strings rather than atoms.

A target has a unique one-component kebab atom name, kind `@bin` or `@lib`, and a source root. Every
root MUST remain inside the project after canonical path resolution, and roots
MUST be pairwise disjoint: no root may equal or contain another. Every module
therefore belongs to exactly one target and has exactly one canonical path.
Overlapping roots emit `@project.overlapping-target-roots`. Target and
dependency names share one project namespace and cannot collide.

A *unit* is one local target or one dependency alias. The unit is the root of
code reference: the first component of every code-reference atom names a unit,
and the remaining components address an entity beneath it.

A binary target record MUST contain `entry` and `effects`; a library target
record MUST omit both. A library has no execution entry and no index module: its
surface is every public declaration of every module under its root, reached by
import. An `entry` on a library target is `@project.entry-on-library`. The
binary `effects` array is the entry's complete static effect ceiling and
execution consent as defined by the effects chapter.

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
`result void e` for a nominal error type `e`. Returning an error produces a
structured nonzero program result; traps remain distinct.

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
(import text @std.text)
(import model @hello.model)
```

Every import has one explicit lexical alias and one atom entity reference whose
resolved entity MUST be a module. The first atom component names a unit; the
remaining components name a module beneath that unit's source root. Thus
`@hello.model` resolves to the local `hello` target's `model.vib`, while
`@std.text` resolves through the `@std` dependency alias. The resolver never
guesses from the importing file's directory.

Resolution is total, but access is not. An atom path resolves to a private
declaration exactly as it resolves to a public one, and each referring position
then applies its own visibility rule: an import exposes only public
declarations, while the `entry` slot may name a private one.

String paths, relative imports, absolute filesystem imports, glob imports,
implicit extension search, directory index fallback, re-exports, and import
cycles are errors. The atom is resolved only because the import grammar expects
an entity reference; the same atom in expression position remains an ordinary
value.

An import makes only the target module alias visible. Public declarations are
referenced as `alias.name`; nested effect operations use
`alias.root.operation`. The standard library is an ordinary pinned dependency,
not an ambient prelude.

## Dependencies and lock

V1 supports:

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
  dependencies: (map
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

### M2 bootstrap trust input

Before ordinary dependency delivery exists, M2 has one offline standard-library
input. The input is the repository-owned byte file
`stdlib/m2/bootstrap.vibon`, and its authority is the adjacent
`stdlib/m2/bootstrap-manifest.vibon`. The manifest is the only source of the
bootstrap identity: it records the artifact's exact `sha256:` digest, an
Ed25519 public key, a detached signature over the artifact bytes, and the
ordered import map. The checked-in public key is the toolchain key for this
repository; a project file, package name, filename, source annotation,
conformance profile, or copied declaration never supplies authority.

The Step 1 artifact identity is fixed at
`sha256:8dd00d7ecbe068205775cd74a0fdf54ffd32f8c0710da362ab938edee567e103`.
The toolchain public-key file is
`stdlib/m2/toolchain-ed25519.pub` with fixed digest
`sha256:fe5736bd57729053562bf6617fbe0acd1d81f66e9cb930341556c4808f3b1509`.
The detached signature is base64 Ed25519 over the artifact bytes; its checked
in file has digest
`sha256:f6bad514c77cf8dac2dc2309df174cb3f25425c681258db276e240a4af2a5e63`.
These values are part of the M2 contract and may change only with a reviewed
bootstrap-contract change that replaces the signature and all dependent
evidence together.

The verifier reads the manifest and artifact as bytes, checks the manifest's
format and field types, computes SHA-256 over the exact artifact bytes, and
rejects a digest mismatch before parsing or resolving any bootstrap record. It
then verifies the detached Ed25519 signature with the fixed public key whose
digest is above and rejects an invalid signature. The manifest's key path and
digest must match that fixed identity; a manifest cannot select another key.
The verifier accepts no alternate encoding,
newline normalization, path alias, symlink, archive member, network URL, or
environment override. A failure is an operational provenance diagnostic and
must not fall back to a vendored or ambient standard library.

The manifest's import map is closed in M2. `@std.text` maps to the trusted text
module and `@std.assert` maps to the trusted assertion module. Each map value
contains the canonical relative path and SHA-256 of that module's exact bytes;
the verifier hashes those bytes and compares them with the records inside the
signed artifact before admitting any declaration or test registry member. An
import is accepted only when its resolved module identity is exactly the
mapped identity; users must write the import explicitly. No standard-library
module is an ambient prelude, and ordinary packages cannot add, replace, or
rebind a bootstrap map entry.

The bootstrap record contains the C7 pure text symbols and the C9 assertion
member names. It is an allowlist and provenance input, not a second language
grammar. `stdlib/m2/src/std/text.vib` is the signed pure declaration module;
`stdlib/m2/src/std/assert.vib` is the signed marker module whose test-only
members come from the registry list. Step 8 verifies every listed byte before
admitting compiler declarations, and Step 13 supplies the assertion behavior.
M2 never performs Git, registry, or network resolution while loading this
input.

There is no registry, version range, lock auto-upgrade, lifecycle script, or
dependency-provided executable in v1.

## Tests

Tests are declarations in `.vib` modules under `tests/`:

```vibra
(import assert @std.assert)

(test "greets by name"
  (assert.equal (greet "Ada") "hello, Ada"))
```

A test has a unique module-local string name. Its omitted `effects:` is empty;
an effectful test writes its complete ceiling. Selecting and running an
effectful test is consent to those roots. Test selection never adds effects
that are not written in the test declaration.

### M2 assertion contract

An M2 test module MUST import `@std.assert` explicitly. The verified bootstrap
exports exactly these test-only assertion members; they are resolved by their
canonical module identity and are not user-definable external declarations:

| Member | Exact signature | Passing behavior |
| --- | --- | --- |
| `assert.true` | `bool -> void` | succeeds when the operand is `true` |
| `assert.false` | `bool -> void` | succeeds when the operand is `false` |
| `assert.equal-bool` | `bool bool -> void` | succeeds when both operands are the same boolean |
| `assert.equal-char` | `char char -> void` | succeeds when both operands have the same scalar |
| `assert.equal-str` | `str str -> void` | succeeds when both operands have the same Unicode scalar sequence |
| `assert.equal-i32` | `i32 i32 -> void` | succeeds when both operands have the same signed value |
| `assert.equal-u64` | `u64 u64 -> void` | succeeds when both operands have the same unsigned value |

The table is closed: another assertion name, generic assertion, implicit
conversion, collection assertion, or deferred operand type is
`@tool.unavailable` in M2. Assertion operands are checked and evaluated
left-to-right. A test body has result type `void`, and its static effects row
MUST be empty; assertion calls do not add an effect.

A passing assertion returns `void`. A false assertion records one structured
test failure with the assertion's canonical name, the canonical literal forms
of its expected and actual values, and the assertion call's primary source
span, then stops that test's body. For `assert.true` and `assert.false`,
`expected` is the required boolean literal and `actual` is the operand. For an
`assert.equal-*` member, `expected` is the first operand and `actual` is the
second operand; both are rendered with the canonical literal formatter. It
does not throw, create a `result` value,
emit a host event, or become a runtime trap. The runner continues with the
next selected test using a fresh value state and empty audit trace. A test item
therefore has exactly one of `@test.passed`, `@test.assertion-failed`,
`@test.invalid`, `@test.unavailable`, or `@test.trap`; only the first two are
ordinary assertion outcomes, and a suite containing any non-passing item is not
`@command.ok`.

Static type or import errors are reported before any selected test executes.
An unavailable assertion or test form retains its source span and reports
`@tool.unavailable`; it is never silently skipped. A runtime trap remains the
separate `@test.trap` outcome with its structured trap diagnostic. Empty test
selection is a successful empty suite only when the selector is omitted; an
unknown explicit selector is invalid input.

The runner isolates each test's values and host event log. Time and random
operations use deterministic providers by default. An unconsumed failure or
unrecorded dependency on a nondeterministic provider fails the test.

## Build products

`vibra build <target>` emits:

- `<target>.wasm`, the deterministic program or library module;
- `<target>.build.vibon`, canonical `@build.v1` data containing toolchain,
  project, dependency, source, required-effect, and module hashes; and
- optional human-readable diagnostics on stderr.

The metadata is descriptive and not executable policy. A conforming runner
checks source targets before execution, while another Wasm host is responsible
for its own embedding policy. Identical toolchain, project snapshot, target,
and build options MUST produce byte-identical outputs.

Packaging, signing, publishing, and multi-artifact application containers are
post-v1 concerns.
