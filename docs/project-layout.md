# Vibra project layout

`project.vibra` is the canonical project manifest. It is YAML like Vibra source, but it is metadata, not a source module.

```yaml
manifest-version: 1
package:
  name: hello
  version: 0.1.0

targets:
  libs:
    - name: core
      root: src/core
      entry: lib.vibra
  bins:
    - name: hello
      root: src/hello
      entry: main.vibra

dependencies:
  std:
    git: https://github.com/nahharris/vibra-stdlib.git
    rev: 6b9fa5838e4f4122ff141e13a5ef737e99955dad
  math:
    git: https://github.com/example/vibra-math.git
    rev: 0123456789abcdef0123456789abcdef01234567
  local-utils:
    path: ../local-utils
```

## Fields

- `manifest-version`: must be `1`.
- `package.name`: kebab-case project name.
- `package.version`: package version string.
- `targets.libs` and `targets.bins`: named source roots. A project must declare at least one target.
- `dependencies`: named local or git dependencies.

Target names and dependency names share one namespace. A name can be used once.

## Source layout

`vibra init hello` creates:

```text
hello/
  project.vibra
  dep/
    std/
  src/
    hello/
      main.vibra
```

`vibra init hello --template lib` creates `src/hello/lib.vibra`. `vibra init hello --template workspace` creates `src/core/lib.vibra` and `src/hello/main.vibra`.

## Tests

Project tests live under `tests/` by convention. `vibra test` recursively
discovers `.vibra` files there and runs each top-level `$test` declaration.
Test files may contain several tests and do not need a `main` function.

```yaml
test:
  $import: "@std/test.vibra"

opens-file:
  $test: core
  do:
    - $test.assert: true
```

Use `--filter`, `--jobs`, `--timeout-ms`, `--fail-fast`, and
`--report yaml --report-file <path>` to control runner behavior. Permission
flags are the same as `vibra run` and apply to each test case.

Files named `foo.<flag>.vibra` are conditional module parts for `foo.vibra`
when the base file exists. For example, `math.test.vibra` shares the same
module scope as `math.vibra` and is included by `vibra test`, which enables the
`test` compilation flag. Normal compiler runs use no flags by default. See
[conditional-compilation.md](conditional-compilation.md) for multi-flag and
tooling semantics.

## Imports

Relative imports keep file-relative behavior:

```yaml
model:
  $import: ./model.vibra
```

Imports beginning with `@` resolve through project namespaces:

```yaml
io:
  $import: "@std/io.vibra"
core:
  $import: "@core/lib.vibra"
```

`@name/path` resolves `name` as either a target name or dependency name. Target imports resolve under the target `root`. Dependencies with a `project.vibra` resolve under their matching (or only) library target root; unmanifested path dependencies retain root-relative behavior. Git dependencies live under `dep/<name>` after `vibra sync`.

## Dependencies

Local dependencies:

```yaml
dependencies:
  local-utils:
    path: ../local-utils
```

Git dependencies:

```yaml
dependencies:
  math:
    git: https://github.com/example/vibra-math.git
    rev: 0123456789abcdef0123456789abcdef01234567
```

A path or Git dependency may expose a static WebAssembly library with a
package-relative `wasm: path/to/library.wasm` field. Typed wrappers bind its
exports through `$wasm.import.module: "@dependency-name"`; see
[static-wasm-ffi.md](static-wasm-ffi.md) for the ABI and safety contract.

Git dependencies must pin a full 40-hex `rev`. `vibra sync` recursively exports clean source trees into package-local `dep/<name>` directories; nested repositories use their own `dep/` directories, so diamond edges may select different revisions without a global namespace collision. Exported trees contain no `.git` metadata. Local dependencies are not copied and published Git dependencies may not declare path dependencies.

Sync writes deterministic `project.lock.vibra` metadata for every vendored package: source identity, exact revision, SHA-256 of its own clean source tree, vendor path, and dependency alias edges. Commit this lock. Offline `check` and build operations use only the vendored graph and reject missing/stale lock entries or modified source. Until the future solver in issue #80 exists, all `std` edges must select one exact revision.

Vendored dependency and stdlib documents are visible to `vibra code` queries. They are read-only to code transactions; change the upstream source and resync instead.

The design for a future SemVer-based resolver, including deterministic
selection, lock migration, and offline behavior, is documented in
[version-solving.md](version-solving.md). It is not part of manifest version 1.

`vibra init` seeds the current toolchain stdlib into `dep/std` for immediate offline use and records its canonical source and exact revision as:

```yaml
dependencies:
  std:
    git: https://github.com/nahharris/vibra-stdlib.git
    rev: 6b9fa5838e4f4122ff141e13a5ef737e99955dad
```

The compiler source tree pins the same revision through its `stdlib` Git submodule.

## Commands

```sh
vibra init hello
vibra init hello --template lib
vibra init hello --template workspace
vibra sync hello
vibra check hello
```

`vibra check` validates the manifest, target files, dependency declarations, synced git dependency paths, local dependency paths, and `@` imports. It does not build or execute targets.
