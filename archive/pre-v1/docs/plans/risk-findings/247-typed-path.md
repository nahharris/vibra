## Pre-implementation findings

Pre-implementation investigation into which lowering path this diagnostic can be
built on. Short version: it **cannot** be built on the typed path today, but it
should not be built *inside* `src/lower.rs` either — it belongs in
`src/body_semantics.rs`, which both paths already call.

### What was checked

- Which lowering path a real `vibra run` / `vibra test` / `vibra build` uses.
- Whether the typed path retains a queryable type for an expression node.
- Whether the type of a value in statement position is recoverable on either path.
- Whether the typed executable subset can compile `result`/`option` at all.

### 1. The typed path is not reachable from any CLI command

Every command that compiles source goes through `load::load_legacy_yaml_program`
→ `lower::lower_program`:

| command | site |
| --- | --- |
| `run` | `src/main.rs:574-579` |
| `expand` | `src/main.rs:586` |
| `effects` | `src/main.rs:953-954` |
| `test` | `src/test_runner.rs:390` |
| `build` / package | `src/package.rs:73`, `src/package.rs:161` |
| lint/tooling | `src/tooling.rs:414` |

`src/load.rs:8-11` and `src/load.rs:275,297-298` show the actual pipeline:
S-expression frontend → `surface_adapter::module_to_value_with_alias` → legacy
`Value` → `lower.rs`. `src/typed_program.rs:11-16` states it outright: *"Nothing
in `src/load.rs`, `src/main.rs`, or any existing command path calls into this
module."* The only callers of `lower_typed_program` are its own `#[cfg(test)]`
tests (`src/typed_program.rs:429,454,467,480,546,565`) and
`tools/corpus-migrator/src/main.rs:266-282`.

(Note in passing: `vibra check` is *not* a type check — `src/main.rs:498-502` →
`src/project.rs:189-197` validates the manifest, lock, and wasm imports only.)

So the typed path is live, maintained, and measured — but it is dead code with
respect to compiling user source.

### 2. Types are computed and discarded, on both paths

`typed_body::infer` (`src/typed_body.rs:1391`) delegates to
`crate::lower::infer_expr_type` — the *same* inference function the legacy path
uses. There is no separate typed type-checker. Neither path builds an
expression-node → type table: `validate_statements`
(`src/typed_body.rs:586`) returns `Vec<Statement>` and drops every inferred type
after comparing it. `TypedBodyIndex` (`src/typed_body.rs:39-49`) retains
`let_types` (the *declared* `$let` annotations) and `node_origins`, not inferred
types.

**This does not matter for this issue.** The value in statement position is
almost always a call, and at a call site the type is one map lookup away on
either path:

- typed: `signatures.functions[&call.callee_key].return_type` — exactly what
  `src/typed_body.rs:640` already does for `LetValue::Call`.
- legacy: `substituted_return_type(sig, &call.type_args)` (`src/lower.rs:5878`),
  with `sigs` in scope at the `Statement::Call` construction site
  (`src/lower.rs:6142`).

The non-call statement positions are `Statement::Eval` (built at
`src/lower.rs:5498`, `src/lower.rs:6119`; typed at `src/typed_body.rs:830,2055,2162`),
where `infer_expr_type` answers the same question directly.

### 3. The typed subset cannot compile `result`/`option` at all

`result` and `option` are generic enums —
`stdlib/src/result.vib:2` is `(def result (enum (err e) (ok t)) where: (t any e any))`.
The typed executable subset rejects every part of that:

- `ensure_safe_type` (`src/typed_body.rs:548-583`) allows only primitives,
  arrays, records, tuples, maps, and refs; enums, nominal types, interfaces,
  capabilities, and fn-types all hit the `other => bail!` at
  `src/typed_body.rs:580`.
- generic functions are rejected at `src/typed_body.rs:305`; generic calls at
  `src/typed_body.rs:920`.
- `Statement::Match` — the *correct* handling this diagnostic is supposed to
  reward — bails at `src/typed_body.rs:834`: `"typed match remains staged with
  enum and interface semantics"`.

Measured on this branch (`agent/vib-source-call-order`), every stdlib-importing
file fails typed body lowering. Corpus migrator dry run, today:

```
surface-valid:      71/71
signature-valid:    71/71
body-valid:         22/71
materialized-valid:  6/22
```

(The scan picks up ~12 untracked/gitignored scratch files — `tmp/*.vib`,
`probe*.vib` — so treat the denominators as approximate; the shape is not.)
`tools/corpus-migrator/README.md:83` still advertises `body-valid: 57/58`,
`materialized-valid: 19/57`; that is stale. The regression is a single cause:
`(option.from-value t value)` (`stdlib/src/result.vib:63`) puts the generic type
argument in `arguments`, not `type_arguments`, and
`typed_body::bind_call_arguments` (`src/typed_body.rs:2490`, error at
`src/typed_body.rs:2599`) has no type-argument splitting. The legacy path handles
this in `surface_adapter`.

There is nothing on the typed path for this diagnostic to fire on.

### 4. Where it should actually go

Building it directly into `src/lower.rs` would be rework at cutover. There is a
better seam that already exists: `src/body_semantics.rs`, whose module doc
(`src/body_semantics.rs:1-5`) says *"Both source frontends use these
return-termination and affine-task checks."* It operates purely on lowered IR
and is called from both paths:

- legacy: `src/lower.rs:2883` (`validate_function_body`), `src/lower.rs:2284,2446`
- typed: `src/typed_body.rs:378-380`

A new pass with signature roughly
`fn validate_unhandled_values(&[Statement], &HashMap<String, FunctionSig>, &HashMap<String, TypeAlias>, &mut Vec<String>) -> Result<()>`
added there is called from both sites and survives the cutover untouched. At IR
level `Statement::Call` still carries `callee_key`, so type recovery is exact,
and `Statement::Eval(Expr)` can be typed with `infer_expr_type`.

The unused-`(bind name)` warning is a second walk over `MatchArm.pattern` plus
arm body — same module, same seam, independent of this one.

### Open design question worth settling first

Recognising "this type is `result`/`option`" at IR level means matching a
nominal key: `TypeRef::Instantiated { base, .. }` where `base` is the qualified
alias key (`result.result`, `option.option` — see the corpus, e.g.
`examples/fs-roundtrip.vib:13` uses `result.result.ok`). Because the base key
depends on the importing module's alias, the check needs a resolution rule
against the `type_aliases` table rather than a string compare. Deciding whether
this is a hard-coded stdlib pair or a general `must-use` marker on the type
(the issue's first open question) directly determines whether that resolution
rule is a special case or a type-system feature. Settling that before writing
code will save a rewrite.

Also worth noting for the acceptance criteria: `--deny-warnings` already exists
and the `warnings: &mut Vec<String>` sink is threaded through legacy lowering,
but the typed path has **no warnings sink at all** in `typed_lower`/`typed_body`
(documented at `src/typed_program.rs:345-370`). If this diagnostic ships as a
warning rather than an error, the typed path will silently not emit it until
that sink exists.

### VERDICT

**Must use the legacy path — but implement it in `src/body_semantics.rs`, not
`src/lower.rs`, and there is no rework.**

Blocked on the typed path, and not marginally: the typed executable subset
cannot compile a single function whose signature mentions `result` or `option`,
and cannot lower `match`. Waiting for it is not an option on any near horizon.

The legacy path has everything needed and the shared-IR seam means the work is
path-neutral. Implementing it as a `body_semantics` pass called from both
`src/lower.rs:2883` and `src/typed_body.rs:378` means the typed path inherits
the diagnostic for free the day its coverage lands.
