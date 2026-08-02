# Vibra corpus migrator

One-time, developer-only migration utility for issue #150. It recursively scans
the repository's `.vib` corpus (including initialized standard-library
submodules, fixtures, templates, macros, and generated-source inputs), emits
schema-aware S-expression forms in memory, and validates the result at three
increasingly strict, honestly-labeled depths instead of a single conflated
number:

1. **surface-valid** — the source parses (reader/CST) and lowers into a
   well-shaped typed surface AST. This is what the tool used to call
   `typed-valid`; it proves shape only, not compileability.
2. **signature-valid** — typed declaration/signature lowering
   (`vibra::typed_lower::lower_typed_signatures`) succeeds for the file
   treated as the entry point of its own program, pulling in its transitive
   relative imports so cross-module type references resolve the same way a
   real build would see them.
3. **body-valid** — typed executable body lowering
   (`vibra::typed_body::lower_typed_bodies`) succeeds against the signature
   index from step 2. This is the tier that actually measures cutover
   readiness: a converted corpus is unreadable to the legacy compiler, so the
   language suite breaks the instant it lands unless the typed path truly
   compiles it.

Each deeper tier only runs on the files that passed the tier above it, so the
three counts form a funnel, not three independent samples. `project.vib`
package manifests are excluded from all three tiers: they use their own
top-level grammar (`(project ...)`, read by `project_context.rs`), not the
module grammar `ast::lower_document` expects, so running them through it would
be a validator/category mismatch, not a language-readiness signal. They are
still counted in `scanned` and `already-sexpr`.

The deterministic scan excludes only build/VCS work areas named `target`,
`.git`, `.worktrees`, or `worktrees`.

Dry-run is the default and never writes source files:

```sh
cargo run --manifest-path tools/corpus-migrator/Cargo.toml -- .
```

Pass `--write` explicitly to rewrite only files that converted and passed the
surface-validation tier. Already-S-expression files are left unchanged:

```sh
cargo run --manifest-path tools/corpus-migrator/Cargo.toml -- --write .
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

The repository-wide dry run reports:

```text
scanned: 62
already-sexpr: 62
converted: 0
unsupported: 0
project-manifests-excluded: 4

surface-valid: 58/58
surface-invalid: 0

signature-valid: 58/58
signature-invalid: 0

body-valid: 57/58
body-invalid: 1

materialized-valid: 19/57
materialized-invalid: 38
```

All corpus source files are already S-expressions, every module source parses
into a well-shaped typed surface AST, and every one of them lowers to typed
signatures. The one body-tier failure is a standalone-tool limitation resolving
the example workspace's project import, not a corpus syntax failure.

Keep the tiers distinct when reading this. The original single-tier report said
`typed-valid: 58/58` while measuring only surface parsing, which read as
cutover readiness when body readiness was closer to 9%. The staged report
exists so that gap cannot hide again.

The materialize tier deliberately exercises an incomplete staged typed subset;
it is diagnostic-only and does not define the compiler cutover gate. The real
compiler validates the converted corpus with `cargo run -- test`.
