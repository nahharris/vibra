## Pre-implementation findings

Pre-implementation investigation into which lowering path effect inference must
be built on. Good news: **this issue is path-neutral.** `src/effect_semantics.rs`
operates on lowered IR, not on either frontend, and is already called from both
paths. The work lands in one place and both paths inherit it.

### What was checked

- Which path compiles today, and which path `effect_semantics` runs on.
- Whether a call graph already exists, and whether inference is a fixpoint.
- How interface dispatch is represented by the time effects are inferred.
- What `EffectRow` can and cannot express (root subsumption).
- What `vibra effects` already reports.

### 1. `effect_semantics` is IR-level and already shared

`src/effect_semantics.rs:14-18` imports `Call`, `EffectRow`, `Expr`,
`FunctionBody`, `FunctionSig`, `LetValue`, `Statement` from `crate::lower` — the
semantic IR, not the surface AST. Consequently it runs on whichever path
produced that IR, and today it runs on **both**:

- legacy: `src/lower.rs:1984-1997` (`validate_declared_effects`, called from
  `src/lower.rs:2295`) and `src/main.rs:978,1031` for `vibra effects`.
- typed: `src/typed_body.rs:381-397`, inside `materialize_typed_functions`.

This is the same seam `src/body_semantics.rs` uses
(`src/body_semantics.rs:1-5`: *"Both source frontends use these ... checks"*).
So unlike #247 and #248, nothing here has to be rewritten at cutover.

For context on the other two issues: the compiling path today is legacy —
`src/main.rs:574-579` (`run`), `src/test_runner.rs:390` (`test`),
`src/package.rs:73,161` (`build`) all go `load::load_legacy_yaml_program` →
`lower::lower_program`, via `surface_adapter` (`src/load.rs:275,297-298`).
`src/typed_program.rs:11-16` states no CLI path calls the typed path. That does
not constrain this issue.

### 2. There is no call graph and no fixpoint — this is the real gap

`src/effect_semantics.rs:4-8` is explicit:

> *"It is also a single pass with no fixpoint: a callee contributes its
> **declared** row, never its inferred one, so nothing has to be re-analysed as
> the set grows. That is a direct consequence of declarations being mandatory."*

`infer_call` (`src/effect_semantics.rs:165-178`) reads
`sigs.get(&call.callee_key)` and takes `callee.effects.labels` — the declaration.
That soundness argument **collapses the moment private functions stop
declaring**, which is precisely what this issue proposes. Inferring a private
function's row requires its callees' *inferred* rows, which means:

- a real callee-key graph (edges are `Call::callee_key` plus `Expr::Call`,
  `LetValue::Call`, and `Expr::HostCall`),
- SCC or worklist iteration to a fixpoint,
- explicit handling of recursion through private functions — which today needs
  none, because the declared row terminates the walk.

Machinery that gets you partway already exists in the CLI, not in
`effect_semantics`: `reachable_functions` (`src/main.rs:1342-1400`) walks the
whole `Statement`/`Expr` tree collecting `callee_key`s, and
`collect_function_details` / `collect_statements_details`
(`src/main.rs:1104-1145`) builds a per-function `call_edges` set. That is a
usable starting point, but it lives in `main.rs` and is presentation code; the
graph should be built in `effect_semantics` so both the checker and the report
share one definition.

### 3. Interface dispatch is already handled — for free

The issue's acceptance criterion *"inference across interface dispatch"* is
testable today with no new machinery, because dispatch never survives to the
effect pass. It collapses to a concrete callee key at lowering time:

- legacy: `src/lower.rs:6314-6353` infers the dispatch argument's type with
  `infer_expr_type`, builds an `ImplKey`, and resolves to a concrete signature
  key.
- typed: `src/typed_body.rs:2680-2784`, same shape.

Generic-typed dispatch subjects are rejected outright with `E-DISPATCH-001`
(`src/lower.rs:6321`, `src/typed_body.rs:2747` — *"monomorphisation pending"*),
and non-nominal subjects are rejected as well. There is no dynamic dispatch in
the language today, so there is no dispatch edge inference could miss. This is
exactly what `src/effect_semantics.rs:2-6` claims, and it checks out.

One caveat for test design: the typed path resolves dispatch using
`static_expr_type` (`src/typed_body.rs:2897-2932`), a deliberately conservative
syntactic environment (explicit `$let` annotations, direct call return types,
casts, enum-tag match bindings — see the doc comment at
`src/typed_body.rs:2786-2798`). Legacy uses full `infer_expr_type` with real
locals. Any test that exercises dispatch through an inferred local will pass on
legacy and fail on typed. Write dispatch tests against the legacy path (the one
that runs) and note the divergence.

### 4. Root subsumption needs two changes, both outside the inference pass

`EffectRow` is a flat set: `labels: BTreeSet<(String, String)>`
(`src/lower.rs:365-370`). `difference` (`src/lower.rs:378-380`) is plain set
difference, and `InferredEffects::undeclared` (`src/effect_semantics.rs:60-66`)
is `!declared.labels.contains(&witness.label)`. There is no partial order, so
`fs` cannot cover `fs.read`.

Additionally, a bare root label cannot currently be *written*:
`resolve_effect_row` (`src/lower.rs:2005-2040`) parses each `effects:` entry and
requires `nominal_effect_name` (`src/lower.rs:2046-2052`) to succeed, which does
`name.split_once('.')` — a bare `fs` returns `None` and errors with
`E-EFFECT-002: declares unknown nominal effect` at `src/lower.rs:2028`. So root
subsumption is:

1. a surface/resolution change at `src/lower.rs:2019,2046` to admit a bare root, and
2. a subsumption predicate replacing the `contains` at
   `src/effect_semantics.rs:63`, and matching changes in
   `EffectRow::difference` (`src/lower.rs:378`).

`EffectRow.tail` (`src/lower.rs:367-369`) is reserved for effect polymorphism and
is always `None` — leave it alone; that is #151's territory, matching this
issue's own open question.

### 5. Over-declaration warning has no home yet

`validate_declared_effects` (`src/lower.rs:1975-2002`) takes no warnings sink and
only `bail!`s on under-declaration. Adding `warnings: &mut Vec<String>` is
trivial — the caller at `src/lower.rs:2295` has `warnings` in scope.

The typed path's copy (`src/typed_body.rs:381-397`) has no sink at all, and
`src/typed_program.rs:345-370` documents that `typed_lower`/`typed_body` thread
no warnings collector anywhere. So an over-declaration *warning* will be emitted
by legacy and silently dropped by typed until that sink exists. Worth a note in
the PR rather than a surprise later.

### 6. `vibra effects` already does most of the reporting criterion

Verified by running it (`cargo run -- effects examples/fs-roundtrip.vib`). It
already emits, per reachable function: `declared`, `performed`, `call-edges`,
and `primitive-witnesses` (built at `src/main.rs:978-1000` and
`src/main.rs:1104-1145`). `main` is folded in separately at
`src/main.rs:1029-1040`.

What is missing is only that `performed` today is the **one-level** row (callees
contribute declarations). Once inference is transitive, the same field becomes
the inferred row and the "report inferred alongside declared" criterion is met
without new report plumbing. The falsifiable prediction in the issue is
measurable with this report as-is.

Concrete target from the issue: `examples/fs-roundtrip.vib:33` declares
`effects: (fs.read fs.write io.stdout io.stderr stream.read stream.write stream.manage)`
on `main`. Note this is `main`, which is exported by definition — so whether the
headline example improves depends entirely on resolving the issue's own open
question about whether inference applies to `main`. Root subsumption alone would
cut it to `(fs io stream)`, which is most of the win and does not require
answering that question.

### VERDICT

**Buildable on the typed path today — and on the legacy path today — because it
is path-neutral. Build it in `src/effect_semantics.rs` and `lower::EffectRow`.**

Nothing here is blocked on the typed-path cutover, and nothing gets rewritten by
it. Of the three Tier-1 issues this is the only one in that position.

What is genuinely missing, in dependency order:

1. **Call graph + fixpoint** in `src/effect_semantics.rs`. The current
   single-pass design is explicitly justified by mandatory declarations
   (`src/effect_semantics.rs:4-8`); removing declarations from private functions
   removes that justification. Recursion through private functions needs a
   termination story it does not currently need. This is the bulk of the work.
2. **Root subsumption**, as two separate changes: admit a bare root in
   `resolve_effect_row` (`src/lower.rs:2019,2046`), and add a subsumption
   predicate to `EffectRow` (`src/lower.rs:378`) and
   `InferredEffects::undeclared` (`src/effect_semantics.rs:63`).
3. **Warnings sink** through `validate_declared_effects` (`src/lower.rs:1975`)
   for the over-declaration case.
4. **Visibility gate** — inference applies to module-private functions only, and
   the two paths represent visibility differently. Typed keeps it structurally in
   `TypedSignatureIndex::visibility` (`src/typed_lower.rs:53`), from the explicit
   `visibility:` attribute (`src/ast/surface.rs:1373-1378`, default public).
   Legacy `FunctionSig` (`src/lower.rs:344-358`) has **no visibility field**: the
   adapter encodes private as a `-` name prefix (`visible_key`,
   `src/surface_adapter.rs:329-333`), and enforcement is at call resolution
   (`src/lower.rs:5843-5850`). So on the legacy path the gate is
   `sig.symbol.starts_with('-')`. That works, but it means the shared
   `effect_semantics` pass cannot ask a single question of both paths. Either
   add a visibility field to `FunctionSig` (preferable, and small) or pass a
   predicate in from each caller.

Interface dispatch specifically requires **no** new work: it is statically
resolved to a concrete `callee_key` before effects are inferred
(`src/lower.rs:6314-6353`, `src/typed_body.rs:2680-2784`), and generic dispatch
is rejected rather than approximated.
