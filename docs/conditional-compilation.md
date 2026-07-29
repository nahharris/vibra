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

`.vibra` is the only source extension. A conditional part is recognized only
when the unsuffixed base module exists; otherwise a dotted
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
compilation mode. All other compiler-facing CLI commands accept repeatable
`--flag <kebab-name>` arguments: `build`, `check`, `run`, `docs`, `effects`,
and `expand`. Unknown well-formed names are intentionally accepted; flags are
open invocation inputs rather than manifest declarations.

`vibra lsp --flag <name>` provides an initial server-side set. An LSP client
can replace it at initialization with `initializationOptions.compilationFlags`
and later through `workspace/didChangeConfiguration` using
`settings.vibra.compilationFlags`. A settings change recompiles the overlay and
republishes diagnostics for every open document.

## Identity and diagnostics

`.vapp` metadata records the sorted `compilation-flags` set and sorted
`selected-sources` inventory. Both participate in deterministic archive bytes;
two builds with different active sets cannot share an artifact identity even
when no conditional file happens to match. Packaged execution reconstructs the
same compilation context while verifying the embedded Wasm. Passing `--flag`
to an already compiled `.vapp` is rejected instead of pretending to recompile
it.

Flags and suffix segments must be kebab-case names. `E-FLAG-001` reports an
invalid invocation flag, `E-FLAG-002` reports malformed conditional filenames
(including repeated dots), `E-FLAG-003` reports an attempted packaged-artifact
override, and `E-FLAG-004` reports malformed LSP settings.

Manifest version 1 needs no change for the foundational slice because flags are
invocation inputs, not declared project metadata. Adding declarations,
defaults, or mutually exclusive flag groups later would require a manifest
schema version change rather than silently extending the closed v1 schema.
