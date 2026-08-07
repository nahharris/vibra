# Vibra project layout

`project.vib` is the canonical project source file. It uses the same Vibra
S-expression syntax as modules, with a `(project ...)` root interpreted by
project commands; the filename does not use a separate manifest extension.

```vibra
(project
  (package "hello" "0.1.0")
  (authority
    (grant fs.read "/safe")
    (grant net.connect "example.com:443"))
  (target core kind: @lib root: "src/core" entry: "lib.vib")
  (target hello kind: @bin root: "src/hello" entry: "main.vib")
  (dependency std
    git: "https://github.com/nahharris/vibra-stdlib.git"
    rev: "6b9fa5838e4f4122ff141e13a5ef737e99955dad")
  (dependency local-utils path: "../local-utils"))
```

## Fields

- `(package <name> <version>)` declares package identity.
- `(authority (grant <domain>.<action> [resource-prefix]) ...)` declares
  project root capability grants. Omitting it denies all effectful host
  operations during project execution.
- `(target <name> kind: <@bin|@lib> root: <root> entry: <entry>)` declares a source target.
- `(dependency <alias> ...)` declares a local, Git, or static Wasm dependency.

Target names and dependency names share one namespace. A name can be used once.

## Source layout

`vibra init hello` creates:

```text
hello/
  project.vib
  dep/
    std/
  src/
    hello/
      main.vib
```

`vibra init hello --template lib` creates `src/hello/lib.vib`. `vibra init hello --template workspace` creates `src/core/lib.vib` and `src/hello/main.vib`.

## Tests

Project tests live under `tests/` by convention. `vibra test` recursively
discovers `.vib` files there and runs each `test.case` declaration.
Test files may contain several tests and do not need a `main` function.

```vibra
(import test "@std/test.vib")

(test.scenario "files"
  (test.case "opens file" (test.assert true)))
```

Use `--filter`, `--jobs`, `--timeout-ms`, `--fail-fast`, and
`--report json --report-file <path>` to control runner behavior. Permission
flags are the same as `vibra run` and apply to each test case.

Files named `foo.<flag>.vib` are conditional module parts for `foo.vib`
when the base file exists. For example, `math.test.vib` shares the same
module scope as `math.vib` and is included by `vibra test`, which enables the
`test` compilation flag. Normal compiler runs use no flags by default. See
[conditional-compilation.md](conditional-compilation.md) for multi-flag and
tooling semantics.

## Imports

Relative imports keep file-relative behavior:

```vibra
(import model "./model.vib")
```

Imports beginning with `@` resolve through project namespaces:

```vibra
(import io "@std/io.vib")
(import core "@core/lib.vib")
```

`@name/path` resolves `name` as either a target name or dependency name. Target imports resolve under the target `root`. Dependencies with a `project.vib` resolve under their matching (or only) library target root; unmanifested path dependencies retain root-relative behavior. Git dependencies live under `dep/<name>` after `vibra sync`.

## Dependencies

Local dependencies:

```vibra
(dependency local-utils path: "../local-utils")
```

Git dependencies:

```vibra
(dependency math
  git: "https://github.com/example/vibra-math.git"
  rev: "0123456789abcdef0123456789abcdef01234567")
```

A path or Git dependency may expose a static WebAssembly library with a
package-relative `wasm: path/to/library.wasm` field. Typed wrappers bind its
exports through `(wasm "@dependency-name" "function" argument...)`; see
[static-wasm-ffi.md](static-wasm-ffi.md) for the ABI and safety contract.

Git dependencies must pin a full 40-hex `rev`. `vibra sync` recursively exports clean source trees into package-local `dep/<name>` directories; nested repositories use their own `dep/` directories, so diamond edges may select different revisions without a global namespace collision. Exported trees contain no `.git` metadata. Local dependencies are not copied and published Git dependencies may not declare path dependencies.

Sync writes deterministic `vibra.lock.json` metadata with sorted object keys and
a trailing newline for every vendored package: source identity, exact revision,
SHA-256 of its own clean source tree, vendor path, and dependency alias edges.
Commit this lock. Offline `check` and build operations use only the vendored
graph and reject missing/stale lock entries or modified source.
The design for a future SemVer-based resolver, including deterministic
selection, lock migration, and offline behavior, is documented in
[version-solving.md](version-solving.md). It is not part of manifest version 1.

`vibra init` seeds the current toolchain stdlib into `dep/std` for immediate offline use and records its canonical source and exact revision as:

```vibra
(dependency std
  git: "https://github.com/nahharris/vibra-stdlib.git"
  rev: "6b9fa5838e4f4122ff141e13a5ef737e99955dad")
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
