# Typed runtime plugins

This first runtime-plugin milestone loads a local WebAssembly module through a
project-declared interface. It is deliberately separate from static `$wasm`
dependencies: the plugin path is chosen at runtime and is not embedded in the
application or lockfile.

```vibra
(project
  (package "plugin-host" "0.1.0")
  (plugin-interface arithmetic
    (function sum params: (int32 int32) result: int32)))
```

Load and validate an implementation with dedicated authority:

```sh
vibra plugin . --interface arithmetic --path ./plugins/math.wasm \
  --allow-plugin-load ./plugins/math.wasm
```

`--allow-read` is intentionally irrelevant: executing bytes requires
`--allow-plugin-load` for the canonical file or an ancestor directory. The
loader reads the bytes once, hashes them with SHA-256, validates every declared
function and exact scalar signature, and instantiates the module. The compact
JSON report is deterministic in declared function-name order and ends with one
LF.

The loaded module must have no imports. Consequently loading cannot mint
filesystem, network, process, environment, clock, random, static FFI, or Vibra
host authority. Authority forwarding through typed APIs is a later milestone;
the initial loader fails closed instead of granting ambient capabilities.

Stable failures use `E-PLUGIN-001` through `E-PLUGIN-007` for invalid
interfaces, paths, denied load authority, forbidden imports, missing exports,
signature mismatch, and instantiation failure.

## Follow-up milestones

- Add a source-level typed plugin handle and typed calls through the declared
  interface; handles must be affine and cannot be forged.
- Forward explicitly typed, attenuated capabilities to individual calls.
- Add Vibra-backed implementations behind the same handle abstraction.
- Define lifecycle, reload, state, and failure-isolation rules.
- Add package-relative interface imports and generated wrapper ergonomics.

Network/registry loading, ambient capability inheritance, untyped reflection,
and replacement of compile-time `import` remain out of scope.
