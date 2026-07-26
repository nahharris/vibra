# Vibra corpus migrator

One-time, developer-only dry-run utility for issue #150. It recursively scans
the repository's `.vibra` corpus (including initialized standard-library
submodules, fixtures, templates, macros, and generated-source inputs), emits schema-aware
S-expression forms in memory, and validates converted output with Vibra's
S-expression parser and typed surface AST.

The deterministic scan excludes only build/VCS work areas named `target`,
`.git`, `.worktrees`, or `worktrees`.

It never writes source files:

```sh
cargo run --manifest-path tools/corpus-migrator/Cargo.toml -- .
```

Unsupported constructs are grouped and counted. Add explicit mappings instead
of falling back to a generic YAML/S-expression dump.

Legacy mapping-form calls are converted to positional calls only when an
ordered local function signature is present in the same module. The migrator
orders explicit generic type arguments first and value arguments second, and
fails closed for unresolved qualified/imported callees, missing arguments, or
unknown labels.

Generic type applications are likewise ordered from each alias's declared
`=where` parameters across local and relative imports. Inherent and interface
method calls combine enclosing type parameters, method type parameters, and
value parameters in declaration order; unknown or missing labels are errors.

## Residual inventory

The repository-wide dry run on the issue #150 migration baseline reports:

```text
scanned: 62
already-sexpr: 4
converted: 58
typed-valid: 58
unsupported: 0
```

Unsupported entries, if introduced by future corpus changes, remain
path-qualified and deterministic. The migration baseline has no residual
unsupported YAML files: every non-S-expression source in the scanned corpus
converts to a typed-valid surface AST.
