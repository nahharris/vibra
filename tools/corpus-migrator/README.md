# Vibra corpus migrator

One-time, developer-only dry-run utility for issue #150. It reads the legacy
YAML source corpus under `tests/` and `examples/`, emits schema-aware
S-expression forms in memory, and validates converted output with Vibra's
S-expression parser and typed surface AST.

It never writes source files:

```sh
cargo run --manifest-path tools/corpus-migrator/Cargo.toml -- .
```

Unsupported constructs are grouped and counted. Add explicit mappings instead
of falling back to a generic YAML/S-expression dump.
