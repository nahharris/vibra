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

The repository-wide dry run, measured against `main` after the capability and
handle shorthand conversions were fixed, reports:

```text
scanned: 62
already-sexpr: 4
converted: 58
unsupported: 0
project-manifests-excluded: 4

surface-valid: 58/58
surface-invalid: 0

signature-valid: 58/58
signature-invalid: 0

body-valid: 40/58
body-invalid: 18
```

Every non-S-expression source converts syntactically, every module source
parses into a well-shaped typed surface AST, and every one of them now lowers
to typed signatures. 40 of 58 lower all the way to an executable typed body.

Keep the tiers distinct when reading this. The original single-tier report said
`typed-valid: 58/58` while measuring only surface parsing, which read as
cutover readiness when body readiness was closer to 9%. The staged report
exists so that gap cannot hide again.

### Signature tier: complete

Two fixes closed it. Declarations named after primitive operators are permitted
(#180) — `option.and`, `option.or`, `result.and`, and `result.or` are reachable
through their qualified names, so rejecting them was wrong and it had cascaded
to every module importing `option` or `result` (22/58 to 43/58). Then `Self` is
substituted before impl conformance is checked (#182), clearing 15 `E-IMPL-005`
failures (43/58 to 58/58).

### Body tier: 18 remaining, three causes

| Cause | Files | Note |
|---|---|---|
| `unknown typed callee` for interface and inherent method dispatch | 15 | `error.error.kind` (10), `fs.writable.write-string` (5) — qualified method dispatch is not resolved on the typed path yet. This is now the whole remaining tier for practical purposes |
| `@`-prefixed project import unresolved | 1 | `examples/lsp-workspace/main.vibra` imports `@greeting/greet.vibra`; this standalone tool has no `project.vibra` resolution — a tool limitation, not a language gap |
| `ExprKind::Convert` intentionally unimplemented | 1 | `tests/lang-primitive-operations.vibra`; legacy maps it to `Expr::Primitive { op: Convert }`, which the typed primitive validators treat as unreachable, and its AST fallback is a bare literal with no origin |
| Compile-time `template` expansion not run by this tool | 1 | `tests/template.vibra`; the language supports it (#173, `frontend::expand_compile_time_data`), but this tool calls `typed_body::lower_typed_bodies` directly and bypasses `load_surface_program`, so the failure is tool wiring rather than a language gap |

The capability-target failures that previously dominated this tier are gone.
They were a converter bug rather than a compiler gap: the corpus fuses the
domain into the head (`$capability.env-read`, `$handle.read`) and only the
expanded `$capability` spelling was handled, so the head was emitted verbatim
as a bare symbol and the typed path correctly rejected a malformed capability
target. The corpus never uses the expanded spelling at all.

Unsupported and per-tier failure entries remain path-qualified and
deterministic, exactly like the original single-tier report.
