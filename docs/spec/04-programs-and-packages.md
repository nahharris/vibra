# Vibra v1 programs and packages

Status: normative target
Implementation status: not started

## Project file

A project is rooted by `project.vib`, parsed by the same reader as source. It
contains one `(project ...)` form with `version: 1`.

```vibra
(project
  version: 1
  (package "hello" "0.1.0")
  (target hello kind: @bin root: "src/hello" entry: "main.vib")
  (dependency std
    git: "https://github.com/nahharris/vibra-stdlib.git"
    rev: "0123456789abcdef0123456789abcdef01234567")
  (authority
    (grant std/io.stdout))
  (limits fuel: 1000000 memory-bytes: 67108864 resources: 128))
```

The project grammar is closed. Unknown, duplicate, or missing children and
labels are errors. Labels precede repeated child forms in canonical output.

## Packages and targets

A package has a kebab-case name and semantic version used as source identity.
V1 does not solve version ranges; dependency selection is exact.

A target has a unique name, kind `@bin` or `@lib`, source root, and entry file.
The root and entry must remain inside the project after canonical path
resolution. Binary entries export `main`; library entries expose their public
module declarations. Target and dependency aliases share one project namespace
and cannot collide.

A binary entry module defines one private `main` with no parameters. Its result
is either `void` or `result void e` for a nominal error type `e`. Returning an
error produces a structured nonzero program result; traps remain distinct.
`main` may omit `effects:` and uses its inferred row, while project authority
still bounds execution.

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
resolves through a target or dependency alias. Relative imports resolve from
the importing file and cannot leave their package. Absolute filesystem paths,
glob imports, implicit extension search, directory index fallback, re-exports,
and import cycles are errors.

An import makes only the target module alias visible. Public declarations are
referenced as `alias.name`; nested effect operations use
`alias.root.operation`. The standard library is an ordinary pinned dependency,
not an ambient prelude.

## Dependencies and locks

V1 supports:

- local dependencies with `path:`; and
- Git dependencies with an HTTPS URL and full 40-hex `rev:`.

`vibra project sync` exports exact Git revisions into `dep/<alias>/` without
`.git` metadata. It writes `vibra.lock.json`, canonical UTF-8 JSON with sorted
keys and one trailing newline. The lock records format version, dependency
edges, source identities, revisions, content hashes, and vendor paths.

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
  effects: ()
  (assert.equal (greet "Ada") "hello, Ada"))
```

A test has a unique module-local string name and a complete effect ceiling.
The default runner grants no host authority and uses deterministic time and
random providers. Effectful tests require both a written ceiling and explicit
runner grants no greater than the project authority. Test selection never
grants authority.

The runner isolates each test's values, resources, fuel, memory account, and
host event log. A resource leak, nondeterministic dependency on real wall time,
or unconsumed failure fails the test.

## Build products

`vibra build <target>` emits:

- `<target>.wasm`, the deterministic program or library module;
- `<target>.vibra.json`, canonical build metadata containing toolchain,
  project, dependency, source, grant-requirement, and module hashes; and
- optional human-readable diagnostics on stderr.

The metadata is not embedded source and is not executable authority. A host
must still supply grants and budgets when loading the Wasm artifact. Identical
toolchain, project snapshot, target, and build options MUST produce
byte-identical outputs.

Packaging, signing, publishing, and multi-artifact application containers are
post-v1 concerns.
