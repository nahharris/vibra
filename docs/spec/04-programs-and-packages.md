# Vibra v1 programs and packages

Status: normative target
Implementation status: not started

## Vibra data documents

Vibra uses its own reader to represent compiler-owned data. A data document is
exactly one literal from this closed subset:

```ebnf
data = string | boolean | integer | float | unit | atom-name
     | "(", "record", { label, data }, ")"
     | "(", "array", { data }, ")"
     | "(", "tuple", { data }, ")"
     | "(", "map", { data, data }, ")" ;
```

Records contain unique labelled fields. Maps contain alternating key/value
forms directly and require an even number of forms. Bare symbols, calls,
imports, bindings, declarations, and host operations are not data and MUST be
rejected in a data document. The data is parsed and validated, never executed.

Each compiler-owned format defines a closed record schema and a version atom.
Unknown, duplicate, or missing fields are errors. Canonical output uses the
source formatter's whitespace rules, schema field order for records, canonical
key order for maps, LF endings, and one trailing newline.

This literal subset is Vibra's object notation. Project files, lock files, and
build metadata use it instead of persistent JSON. JSON remains the machine
interchange format for CLI and MCP responses.

## Project file

A project is rooted by `project.vib`. It contains one `@project-v1` record:

```vibra
(record
  format: @project-v1
  package: (record
    name: "hello"
    version: "0.1.0")
  targets: (array
    (record
      name: @hello
      kind: @bin
      root: "src/hello"
      entry: "main.vib"
      effects: (array @std/fs.read @std/io.stdout)))
  dependencies: (map
    @std (record
      kind: @git
      git: "https://github.com/nahharris/vibra-stdlib.git"
      rev: "0123456789abcdef0123456789abcdef01234567")))
```

Project tooling preserves comments when possible and rewrites changed data in
canonical form. There is no executable `(project ...)` declaration and no
legacy project format fallback.

## Packages and targets

A package has a kebab-case name and semantic version used as source identity.
V1 does not solve version ranges; dependency selection is exact.

A target has a unique atom name, kind `@bin` or `@lib`, source root, and entry
file. The root and entry must remain inside the project after canonical path
resolution. Binary entries export `main`; library entries expose their public
module declarations. Target and dependency names share one project namespace
and cannot collide.

A binary target record MUST contain `effects`; a library target record MUST
omit it. The binary array is the entry's complete static effect ceiling and
execution consent as defined by the effects chapter.

A binary entry module defines one private `main` with no parameters. Its result
is either `void` or `result void e` for a nominal error type `e`. Returning an
error produces a structured nonzero program result; traps remain distinct.
An omitted `main` `effects:` is `()`. An effectful `main` writes its ceiling,
and project checking compares its computed performed row with both that ceiling
and the target record.

The minimum initialized layout is:

```text
hello/
  project.vib
  src/
    hello/
      main.vib
  tests/
```

## Modules and imports

Every `.vib` source file is one module. Its canonical module identity is its
package-relative path without the extension. A source file does not redeclare
that identity.

```vibra
(import text "@std/text.vib")
(import model "./model.vib")
```

Every import has one explicit alias and one literal path. `@alias/path.vib`
resolves through a target or dependency name. Relative imports resolve from
the importing file and cannot leave their package. Absolute filesystem paths,
glob imports, implicit extension search, directory index fallback, re-exports,
and import cycles are errors.

An import makes only the target module alias visible. Public declarations are
referenced as `alias.name`; nested effect operations use
`alias.root.operation`. The standard library is an ordinary pinned dependency,
not an ambient prelude.

## Dependencies and lock

V1 supports:

- local dependencies whose record has `kind: @path` and `path:`; and
- Git dependencies whose record has `kind: @git`, an HTTPS `git:` URL, and a
  full 40-hex `rev:`.

`vibra project sync` exports exact Git revisions into `dep/<alias>/` without
`.git` metadata. It writes `project-lock.vib`, a canonical generated data
record. The lock contains its format, project fingerprint, dependency edges,
source identities, revisions, content hashes, and vendor paths:

```vibra
(record
  format: @project-lock-v1
  project: "sha256:..."
  dependencies: (map
    @std (record
      kind: @git
      source: "https://github.com/nahharris/vibra-stdlib.git"
      rev: "0123456789abcdef0123456789abcdef01234567"
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
(import assert "@std/assert.vib")

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
- `<target>.build.vib`, canonical `@build-v1` data containing toolchain,
  project, dependency, source, required-effect, and module hashes; and
- optional human-readable diagnostics on stderr.

The metadata is descriptive and not executable policy. A conforming runner
checks source targets before execution, while another Wasm host is responsible
for its own embedding policy. Identical toolchain, project snapshot, target,
and build options MUST produce byte-identical outputs.

Packaging, signing, publishing, and multi-artifact application containers are
post-v1 concerns.
