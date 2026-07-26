# Vibra corpus migrator

One-time, developer-only dry-run utility for issue #150. It recursively scans
the repository's `.vibra` corpus (including initialized standard-library
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
three counts form a funnel, not three independent samples. `project.vibra`
package manifests are excluded from all three tiers: they use their own
top-level grammar (`(project ...)`, read by `project_context.rs`), not the
module grammar `ast::lower_document` expects, so running them through it would
be a validator/category mismatch, not a language-readiness signal. They are
still counted in `scanned` and `already-sexpr`.

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

The repository-wide dry run, measured against `main` after typed primitive
lowering landed (#177), reports:

```text
scanned: 62
already-sexpr: 4
converted: 58
unsupported: 0
project-manifests-excluded: 4

surface-valid: 58/58
surface-invalid: 0

signature-valid: 22/58
signature-invalid: 36

body-valid: 5/22
body-invalid: 17
```

Every non-S-expression source still converts syntactically and every
converted-or-already-S-expression module source parses into a well-shaped
typed surface AST (`surface-valid: 58/58`). That is where the good news ends:
**only 5 of the 58 module files in the corpus (roughly 9%) actually lower all
the way to an executable typed body.** The gap between "typed-valid" (the old,
conflated number) and the real depth is the entire point of this staged
report.

### Signature tier: one root cause, cascaded

All 36 `signature-invalid` files fail for the same underlying reason,
traced to exactly two declarations:

- `stdlib/src/option.vibra` declares a function `and` (`option.and`).
- `stdlib/src/result.vibra` declares a function `and` (`result.and`).

Both collide with the frozen primitive-name-resolution rule
(`docs/s-expression-migration-status.md`): an unqualified call to a name that
matches a built-in primitive operator always resolves to the primitive, so a
declaration named after one is permanently unreachable and is rejected at
signature-lowering time (`src/typed_lower.rs:266`,
`crate::lower::typed_primitive_op`). Every file that transitively imports
`option.vibra` or `result.vibra` inherits the failure when it is validated as
the entry point of its own program — which is 36 of the corpus's 58 files.
**Renaming (or re-signature-ing) these two combinators is the single highest-
leverage remaining item to raise the signature-valid count.**

### Body tier: five distinct causes across the 22 that reach it

Of the 22 files whose signatures validate, 17 fail body lowering, for five
distinct reasons:

| Cause | Files | Note |
|---|---|---|
| Typed compile-time expression lowering (`embed`/`template`/`wasm`) is not active | 13 | Root trigger sites: `stdlib/src/test.vibra`'s `assert` (wasm write, cascades to ~10 test files that import it), `examples/static-wasm-ffi/src/main.vibra`'s `foreign-sum` (direct wasm import), `tests/template.vibra` (compile-time template render) |
| `@`-prefixed project import unresolved | 1 | `examples/lsp-workspace/main.vibra` imports `@greeting/greet.vibra`; this standalone tool has no `project.vibra` resolution (same limitation it already accepts for named-call signature discovery) — a tool limitation, not a language gap |
| `Expr::EnumConstructor` not implemented on the typed path | 1 | `tests/lang-enums.vibra` calls an enum constructor (`signal.go`) that isn't registered as a callable function |
| `ExprKind::Convert` intentionally unimplemented (needs explicit fallback semantics) | 1 | `tests/lang-primitive-operations.vibra` |
| `Statement::Spawn` not implemented on the typed path | 1 | `tests/lang-tasks.vibra` |

Unsupported and per-tier failure entries remain path-qualified and
deterministic, exactly like the original single-tier report.
