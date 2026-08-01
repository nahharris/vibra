# Vibra effect system

Date: 2026-08-01
Status: accepted implementation contract for issue #151
Compatibility: intentionally breaking; `effects:` becomes mandatory on every
function whose body reaches a host import

Supersedes `docs/type-effect-system-handoff.md`, which was written against the
YAML surface and the `$policy`/`$capability` authority system decommissioned in
#213. That document's premise — that effects are expressed as capability
domains threaded through call sites — does not survive the removal of the
authority layer, so this design starts from an authority-free host ABI.

## Decision

Every function declares, in its signature, the set of host effects its body may
perform. The compiler infers the body's actual effect set and rejects any
function whose body exceeds its declaration. `vibra effects` reports a program's
effect surface from those declarations rather than from a call-graph walk.

Effects are **static and fully erased**. There is no runtime representation, no
capability value, no permissions manifest, and no runtime enforcement. Host
access remains unconditionally available exactly as it is today. This is a
review and verification mechanism, not a sandbox: it tells you what a program
*can* reach and proves the claim is complete, but it does not stop a program
that lies about — well, it cannot lie, because the host ABI is ground truth
(see "Host imports"). It does not stop an *embedder* from running Vibra source
without its own sandboxing, and `PHILOSOPHY.md` continues to say so.

## The central constraint: effects are data, not compiler intrinsics

The compiler knows exactly one new thing: how to **construct** and **compare**
an effect. It does not know that `fs.read` exists, what it means, or that the
universe of effects is closed.

```lisp
(def read (effect @fs @read))     ; construct and bind, as (def t (newtype str)) does

(defn load (p fs.path) str
  (do ...)
  effects: (fs.read (effect @fs @write)))   ; a bound name, or an inline construction
```

**Effect identity is structural**: the ordered pair of atoms. `(effect @fs @read)`
written in two different modules denotes the *same* effect. Names are a
readability convenience layered on top.

A consequence worth stating, because it removes a whole class of question: giving
an effect a second name needs no aliasing mechanism. Re-declaring
`(def alt-read (effect @fs @read))` already denotes the same effect. (A bare
`(def alt-read read)` alias is rejected by `E-ADAPT-002`, but that is a
pre-existing limitation on *all* bare type aliases — a `newtype` behaves
identically — and is orthogonal to this design.)

Structural identity is what keeps the compiler out of the effect *namespace*.
It removes any need for canonical-name derivation from module paths, a closed
label registry inside the compiler, a stdlib-drift synchronisation test, an
"import the declaring module before you may name its effect" rule, and a
duplicate-label diagnostic. A third-party library declares
`(def query (effect @db @query))` and participates fully with no compiler
change.

### Why a constructor rather than bare atoms

`(def read @fs.read)` was considered and rejected. Dotted atoms already lex,
typed atoms are first class, and `TypeRef::Literal(LiteralType::Atom)` already
exists, so bare atoms would need *zero* new constructors — a real advantage.

The constructor wins on two counts. It keeps domain and action structurally
separate rather than fused into an opaque dotted string, and its operand list is
the reserved room for future handler definitions. Handlers are the one part of
this design that is genuinely hard to retrofit, so reserving syntactic room for
them now is worth one new type-expression head.

## Grammar

Added to the `type-expr` production:

```ebnf
type-expr   = … | effect-type ;
effect-type = "(", "effect", atom, atom, ")" ;
```

Both operands are required, ordered, evaluated input, so they are positional
rather than labelled, per the reader contract. Additional operands are reserved
for handlers and rejected today (`E-EFFECT-007`).

Added as a declaration attribute on `defn` and on `fn-type`:

```ebnf
effects-attr = "effects:", "(", effect-ref*, ")" ;
effect-ref   = symbol | effect-type ;
```

`effects:` is legal only on a function-shaped declaration. It is rejected on
`def`, `const`, and `macro` (`E-EFFECT-006`).

## Typing rules

Let `ℓ` range over effect labels and `ε` over effect rows.

```
                                            ── Effect-Intro
Γ ⊢ (effect @d @a) : Effect(d, a)

Effect(d₁,a₁) ≡ Effect(d₂,a₂)   iff   d₁ = d₂ ∧ a₁ = a₂        ── Effect-Eq

Γ ⊢ e : Effect(d,a)
──────────────────────────────────────────────────── Effect-Bind
Γ, N ↦ Effect(d,a) ⊢ (def N e)

ε ::= { ℓ₁ … ℓₙ | ρ }      ρ reserved, always absent in v1      ── Row
ε₁ ⊆ ε₂   iff   labels(ε₁) ⊆ labels(ε₂)                        ── Row-Sub
```

A function's declared row is written `effects: (…)`; an absent attribute means
`ε = {}`, i.e. pure.

```
infer(body) = ε_actual      ε_actual ⊆ ε_declared
──────────────────────────────────────────────────── Fn-Check   (else E-EFFECT-001)
⊢ (defn f … effects: ε_declared) ok
```

The declaration is a **ceiling**, not an equality: declaring more than the body
performs is allowed. Inference never fills the declaration in. Per
`PHILOSOPHY.md`'s *Explicit Intent*, inference may narrow a local expression but
must never silently make a cross-module contract decision, and a function's
reachable host surface is exactly such a decision.

Inference itself is a single pass with no fixpoint, because a callee's effects
come from its *declaration* rather than from re-analysing its body:

```
infer(call f)                = ε_declared(f)
infer(wasm m n args)         = registry(m, n)
infer(task b) = infer(spawn b) = infer(b)
infer(if / match / while / for) = ⋃ infer(branch)
infer(literal | enum-constructor | cast | primitive | record | tuple | array | map) = {}
```

Enum construction, casts, and primitive operations are pure: they compute over
values already in hand.

## Host imports are ground truth

A function whose body is a bare host import does not get to state its own
effects freely:

```
body(f) = (wasm m n …)
──────────────────────────────────────────── Wasm-Exact   (else E-EFFECT-005)
ε_declared(f) ≡ registry(m, n)
```

Equality, not inclusion. The host ABI registry declares each import's effects as
ordinary data alongside its parameter shape, so a user module cannot launder an
effect by writing a raw `(wasm "vibra_v1" "fs_open_read" …)` body with
`effects: ()`. Because effect identity is structural, this check needs no name
resolution and works whether or not the declaring stdlib module is in scope.

The registry's pairs are a *convention* shared with the stdlib, not a set the
compiler validates against anything.

## Interfaces

`fn-type` carries a row, so an interface method's effects are part of its
contract:

```lisp
(def writer (interface (emit (fn-type (self self) void effects: (io.write)))))
```

```
iface method m : (…) → U ! ε_i        impl method m : (…) → U ! ε_m
──────────────────────────────────────────────────────────────────── Impl-Sub
ε_m ⊆ ε_i                                             (else E-EFFECT-003)
```

The interface declares a ceiling and each implementation must stay under it.
This is a genuine subset relation and is checked separately from
`signatures_match`, which is also reached through `unify_types` where a subset
relation would be directionally wrong. Row comparison inside `unify_types`
itself requires **equality**.

## Effect polymorphism is reserved

The row type carries a tail field that is always absent, and writing one is a
hard error (`E-EFFECT-007`).

First-class functions are currently rejected (`E-FN-001`), so no higher-order
function can propagate a callee's effects and no polymorphism is needed. The
tail exists anyway because retrofitting a row *after* first-class functions land
is the single hardest change in this design: without polymorphism every
higher-order function needs one variant per effect set, and the whole system
collapses. Reserving the shape now costs a field; adding it later costs a
rewrite.

## Effect inventory (convention)

Domains follow **stdlib module names**, not host ABI spellings — `@time @now`,
not `@clock @now`; `@sys @info`, not `@system @info`.

| Module    | Effects                                                    |
| --------- | ---------------------------------------------------------- |
| `fs`      | `@fs @read`, `@fs @write`, `@fs @metadata`                  |
| `net`     | `@net @connect`, `@listen`, `@send`, `@receive`             |
| `io`      | `@io @read`, `@io @write`                                   |
| `process` | `@process @spawn`, `@process @wait`                         |
| `env`     | `@env @read`, `@env @write`                                 |
| `time`    | `@time @now`                                                |
| `sys`     | `@sys @info`                                                |
| `random`  | `@random @generate`                                         |

The `str`, `bytes`, `array`, `map`, `path`, `format`, and `parse` host imports
are pure algebra and carry no effects. `vibra_test` intrinsics (`assert`,
`fail`) also carry none: they are harness primitives with no host authority, so
tests declare effects under the same rule as any other code without every test
case needing an attribute.

## Diagnostics

| Code            | Meaning                                                       |
| --------------- | ------------------------------------------------------------- |
| `E-EFFECT-001`  | body performs an effect not in `effects:`                      |
| `E-EFFECT-002`  | name in `effects:` does not resolve to an effect               |
| `E-EFFECT-003`  | impl method effects exceed the interface's declared ceiling    |
| `E-EFFECT-004`  | malformed `effects:` value, or `(effect …)` operands not atoms |
| `E-EFFECT-005`  | `(wasm …)` body's declared effects disagree with the registry  |
| `E-EFFECT-006`  | `effects:` on a non-function declaration                       |
| `E-EFFECT-007`  | reserved: handler operands, or an effect-row tail              |

`E-EFFECT-001` must name the *witness* — the undeclared effect and the call that
introduced it — not merely report a set difference:

```
E-EFFECT-001: `app.process` declares effects (fs.read) but its body performs
(effect @fs @write) via call to `fs.write-string-all`
```

Codes raised from the reader (`004`, `006`, `007`) carry real spans. Codes
raised from lowering (`001`, `002`, `003`, `005`) are reported at `0:0`, which
is the existing behaviour of every lowering diagnostic and not a regression
introduced here; the message carries the context instead. Plumbing real spans
through lowering is tracked separately.

## Out of scope

- **Capabilities and runtime authority.** Deferred until first-class functions
  exist, since capability passing needs them. Effect tracking and capability
  passing are separable: the former says what *kind* of thing a function does,
  the latter scopes it to a particular resource.
- **Effect handlers.** The `(effect …)` operand list is reserved for them, but
  they are a control-flow mechanism (resumable delimited continuations) rather
  than a type-system one, and require either a CPS transform or runtime
  continuation support.
- **Scoped or parameterised effects** (`@net` restricted to a host). Any future
  scoping belongs in a reviewable, diffable manifest rather than in the type,
  where it would push toward type-level data and make inference and error
  messages much worse.
- **Permissions manifests.**

## Implementation sequence

Each step compiles and passes both suites on its own.

1. `TypeExprKind::Effect` / `TypeRef::Effect`, the `(effect @d @a)` form, the
   adapter envelope, and every type-walker traversal.
2. `E-EFFECT-001..007` registered in `schemas/linter-codes.json` and both
   `src/tooling.rs` tables.
3. `AnnotationKind::Effects`, `EffectRow`, the `effects:` attribute, and
   `FunctionSig.effects` at all three construction sites. Parsed and resolved,
   not yet enforced.
4. `HostImport::effects` and the regenerated `schemas/host-abi.json`.
5. `src/effect_semantics.rs` body inference, **warn-only**.
6. The stdlib companion change and submodule pointer bump.
7. Flip warn to error; add the `Wasm-Exact` check.
8. `fn-type` rows and the `Impl-Sub` check.
9. Rewire `vibra effects` and bump `schemas/effects.schema.json`.

Steps 5 to 7 must land in that order. `effects:` becoming mandatory is a
breaking cross-repo change, and the stdlib lives in a submodule; any other order
makes the pointer bump the only commit where the tree builds.
