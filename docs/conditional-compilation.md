# Conditional compilation

Issue #129 makes filename suffixes part of the compiler input contract. A
conditional file is a physical part of an existing logical module:

```text
math.vibra
math.test.vibra
math.unix.debug.vibra
```

The base file is always included. `math.test.vibra` is included only when
`test` is enabled. Every suffix segment is required, so
`math.unix.debug.vibra` is included only when both `unix` and `debug` are
enabled. Selection happens before parsing, import discovery, duplicate-symbol
checking, macro expansion, lowering, effects analysis, and artifact
fingerprinting. An inactive file therefore cannot introduce imports,
diagnostics, effects, embedded files, or duplicate definitions.

Both `.vibra` and `.vibra.yaml` are source extensions. A conditional part is
recognized only when the unsuffixed base module exists; otherwise a dotted
filename remains its own module. Conditional parts keep their physical paths in
the source database and diagnostics while sharing the base module's logical
scope. Selected parts are merged in normalized path order, preserving
determinism across hosts.

## Compilation context

The compiler API represents enabled flags as a set, not process-global state.
The same set must flow unchanged through every recursively imported module.
Changing the set changes the compiler input and therefore must change cache and
artifact identity whenever it changes selected source.

The foundational API is `load_program_with_flags`. `load_program` supplies the
empty set. The test runner supplies `test` for both discovery and isolated
execution, standardizing the former `.test.vibra` convention as an actual
compilation mode.

## Remaining command and tooling surface

The issue is not complete until all compiler entry points carry an explicit
compilation context:

- expose repeatable `--flag <kebab-name>` options on compile-oriented commands
  (`run`, `build`, `check`, `docs`, `effects`, and `expand`);
- reserve automatic `test` activation for `vibra test` while still treating
  permissions and test profiles as separate selection mechanisms;
- include enabled flags and selected physical files in deterministic `.vapp`
  build identity and inspection metadata;
- make the LSP compile overlays with the client's active flag set and
  re-publish diagnostics when that set changes;
- define editor/client configuration and structured request/schema fields for
  compilation flags;
- reject empty, malformed, or repeated-dot suffixes with stable diagnostics,
  and decide whether unknown CLI flags are accepted or must be declared by a
  future manifest version.

Manifest version 1 needs no change for the foundational slice because flags are
invocation inputs, not declared project metadata. Adding declarations,
defaults, or mutually exclusive flag groups later would require a manifest
schema version change rather than silently extending the closed v1 schema.
