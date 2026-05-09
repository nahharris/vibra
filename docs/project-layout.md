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
    path: dep/std
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

`@name/path` resolves `name` as either a target name or dependency name. Target imports resolve under the target `root`. Path dependencies resolve under their declared `path`. Git dependencies resolve under `dep/<name>` after `vibra sync`.

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

Git dependencies must pin `rev`. `vibra sync` clones or fetches them into `dep/<name>` and checks out the pinned revision. Local dependencies are not copied.

`vibra init` copies the current toolchain stdlib into `dep/std` and records it as:

```yaml
dependencies:
  std:
    path: dep/std
```

## Commands

```sh
vibra init hello
vibra init hello --template lib
vibra init hello --template workspace
vibra sync hello
vibra check hello
```

`vibra check` validates the manifest, target files, dependency declarations, synced git dependency paths, local dependency paths, and `@` imports. It does not build or execute targets.
