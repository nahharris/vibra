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

The repository-wide dry run, measured against `main` after checked numeric
conversion landed on the typed path (#150), reports:

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

body-valid: 57/58
body-invalid: 1

materialized-valid: 5/57
materialized-invalid: 52
```

Every non-S-expression source converts syntactically, every module source
parses into a well-shaped typed surface AST, and every one of them now lowers
to typed signatures. 57 of 58 lower all the way to an executable typed body.

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

### Materialize tier: the real readiness number

Body lowering alone does not prove a program compiles. `lower_typed_bodies`
produces *staged* statements; only `materialize_typed_functions` turns them into
executable `FunctionSig`s, which is what the real compiler consumes. Reporting
`body-valid` as readiness overstated it by an order of magnitude — 57/58 against
an actual 5/57.

52 failures, three causes:

| Cause | Occurrences | Note |
|---|---|---|
| wasm import forwards an unrecognized argument | 21 + 20 `E-WASM-003`, 1 `E-WASM-007` | `validate_wasm_import_body` matches forwarded names exactly against `sig.arg_names`, so forwarding a *field* of a record-typed parameter (`$args.actual`) is rejected. Hits most of `stdlib/src/test.vibra`'s `assert*` imports and everything calling them |
| typed signature/body set mismatch | 15 | `lower_typed_bodies`'s `TopLevel::Definition` arm lowers bodies only for `defs:`-declared functions (`AnnotationKind::Definitions`). Methods defined inline in an `impls:` block (`AnnotationKind::Implementation`, `Fresh` binding) get a registered signature and no body |
| generic functions unsupported | 12 | `src/typed_body.rs:147` rejects them outright, and `ensure_safe_type` rejects instantiated types, `SelfType`, and capability-typed values at the signature boundary. The standard library is built on generics |

This is the work remaining before the corpus can be converted.

### Body tier: 1 remaining

| Cause | Files | Note |
|---|---|---|
| `@`-prefixed project import unresolved | 1 | `examples/lsp-workspace/main.vibra` imports `@greeting/greet.vibra`; this standalone tool has no `project.vibra` resolution — a tool limitation, not a language gap |

`ExprKind::Convert` (checked numeric conversion) closed this tier's last
genuine language gap. `fallback` is now `Spanned<Literal>` (`src/ast/surface.rs`)
so it carries a real origin, satisfying the `OriginCursor` one-to-one
invariant; `convert` lowers to its own `Expr::Primitive { op: Convert }`
envelope in `validate_expr`, ported from legacy `parse_checked_conversion`/
`conversion_fallback_fits` in `lower.rs` rather than the generic `PrimitiveOp`
dispatch (`typed_primitive_valid_for` still treats `Convert` as unreachable
there, by design). The one remaining failure is this standalone tool's lack
of `project.vibra` resolution, not a compiler gap.

### What the earlier failures actually were

Worth recording, because most of them were not compiler gaps and the error
text pointed at the wrong component in several cases:

- **Interface method dispatch** (15 files) — a real gap, closed in #187.
  `error.error.kind` and `fs.writable.write-string` were the same mechanism:
  both `error:` and `writable:` are declared `$interface`.
- **Capability targets** (11 files) — a converter bug. The corpus fuses the
  domain into the head (`$capability.env-read`, `$handle.read`) and only the
  expanded `$capability` spelling was handled, so the head was emitted verbatim
  as a bare symbol. The corpus never uses the expanded spelling at all. The
  error named `policy.narrow`, which is in the compiler.
- **wasm host imports** (~22 files) — a real gap, closed in #184.
  `stdlib/src/test.vibra`'s `assert` cascaded to nearly every test file.
- **Self-qualified calls** (2 files) — neither a compiler nor a converter bug,
  but this tool mounting each entry under the empty alias, a configuration a
  real build never produces.
- **Compile-time `template`** (1 file) — this tool bypassing expansion, now
  fixed by running it before the body tier.

Unsupported and per-tier failure entries remain path-qualified and
deterministic, exactly like the original single-tier report.
