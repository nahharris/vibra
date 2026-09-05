# Vibra v1 programs and packages

Status: normative target
Implementation status: milestone 1 step 4 in progress (extension dispatch only)

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
Unknown, duplicate, or missing fields are errors. Canonical output uses the
source formatter's whitespace rules, schema field order for records, canonical
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

The step 4 reader supplies only the extension-selected shared lexical/document
mode boundary. VIBON value validation, records, schemas, and canonical data
formatting remain assigned to milestone 1 step 7.

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

Project tooling preserves comments when possible and rewrites changed data in
canonical form. There is no executable `(project ...)` declaration and no
legacy project format fallback. `project.vib` is not searched or accepted as a
project document.

## Packages and targets

A package has a kebab-case name and semantic version used as source identity.
V1 does not solve version ranges; dependency selection is exact. The package
name and version are provenance and never appear in a reference position, so
they are strings rather than atoms.

A target has a unique atom name, kind `@bin` or `@lib`, and a source root. Every
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
