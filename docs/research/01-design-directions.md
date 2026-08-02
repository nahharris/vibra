---
title: Vibra Design Directions from the Literature
category: research
status: working
updated: 2026-08-02
summary: >-
  Synthesis of sixteen papers against Vibra's current design, resolving the
  seven open tensions and ranking candidate changes by evidence per unit cost.
---

# Design directions

This resolves the tensions opened in [`00-current-state.md`](00-current-state.md)
using the distillations in [`notes/`](notes/). It is a research synthesis, not
a decision contract: nothing here is accepted until it lands in
`docs/decisions/` with tests behind it.

Every claim below is sourced to a distillation note. Where the evidence is
weak, it says so — several of these papers have sample sizes that support a
direction but not a magnitude.

## The reframe

The single most consequential number in the whole set: in LLM-generated
TypeScript, syntax accounts for roughly 6% of compilation errors and type
failures for roughly 94% (Mündler et al., PLDI'25). Grammar-level constraint
alone has a measured ceiling of ~9% error reduction. Type-constrained decoding
— a prefix automaton whose states carry a typing context, driving
type-inhabitation search over a type graph — cut errors by 74.8% on HumanEval
and 56.0% on MBPP across six models from 2B to 34B.

Vibra's philosophy currently justifies canonical syntax on the grounds that
"every extra choice is another chance for hallucination." That justification is
weak in light of this measurement. The correct justification is different and
stronger: **canonical syntax and mandatory signatures are what make
type-constrained decoding tractable and normalized retrieval possible.** Prefix
-time type search only terminates because annotations are explicit. Vibra's
existing insistence on explicit signatures, one canonical spelling, and an
idempotent formatter is *right*, for reasons the philosophy document does not
currently give.

This implies the compiler has three products, not one:

| Product | Purpose | Vibra today |
| --- | --- | --- |
| **Checker** | Reject invalid programs | Implemented |
| **Decoder** | Answer "which tokens are well-typed here?" | Gestured at by the LSP context query; not a real service |
| **Index** | Normalized structure for retrieval | Absent |

The Principles page already asks for the third ("retrieval should operate over
normalized structure — types, effects, API calls, control-flow summaries").
The second is the genuinely unexplored research direction: nobody has combined
a designed-for-LLM language with type-constrained decoding, and nobody has
asked whether a language can be *designed so that its constrained-decoding
engine is cheap*. It cost roughly 11 kLoC to build one for a TypeScript
subset. Vibra is unusually well positioned to answer that question, and it
should become an explicit design criterion: **prefer type-system features whose
decoding automaton is small.**

## Resolving the tensions

### T1 — Authority: effects *and* capabilities, joined at the scope

**Resolution: keep static effects as declaration, add runtime grants as
enforcement. Do not let the static check stand alone.**

LLMON's result is the governing evidence: declared structure paid off only
because a runtime acted on it — attention masking over declared spans gave
+29.3 points with no training. Representation without enforcement is
decoration. Vibra's effects are currently *static and erased*, which is exactly
that failure mode: the annotation describes authority that nothing checks at
run time.

FINE supplies the shape of the fix. A type-preserving lowering lets the sandbox
boundary **re-check** policy rather than trust the compiler to have gotten it
right. Plain Wasm validation cannot express this, so the check has to live in
Vibra's own host boundary.

The implementation cost is far lower than "capabilities were removed in #213"
implies. `src/async_runtime.rs` still carries a per-scope `CapabilityGrant` set
with monotone narrowing already enforced and tested. What is missing is a
surface syntax and a propagation rule, not a runtime.

Recommended shape:

- The program entry point declares its total grant set (in `project.vib`, so
  the embedder can read it without running anything).
- A function's declared effect ceiling lowers to a grant requirement.
- Scope entry re-checks the requirement against the scope's held grants;
  narrowing is monotone, and a child can never acquire authority its parent
  lacks.
- Effects stay static for checking and decoding. They stop being erased.

FINE also notes affine types are the right and cheap encoding for *one-shot*
capabilities, which is worth holding for revocable grants (see T3).

### T2 — Effect burden: infer inside, declare at boundaries

**Resolution: mandatory effect declarations on exported functions and
`deffect` operations; inference within a module; check declared ⊇ inferred.**

The transitive union that `examples/fs-roundtrip.vib` forces onto `main` is a
*derived fact*, not a signature. The decoding evidence says annotations matter
because they let prefix-time type search terminate at *signature positions* —
it says nothing in favor of restating derived transitive facts at every call
site. So Vibra can drop the burden without losing the benefit that justifies it.

Two further reductions, both cheap:

- **Root subsumption.** Let `fs` in a declaration cover `fs.read`, `fs.write`,
  and `fs.metadata`. Seven-element effect lists are mostly root-family
  enumeration.
- **Effect sets as first-class type-level values**, so a generic combinator can
  be polymorphic in the effects of its argument rather than declaring a
  ceiling that forces every caller to the maximum.

Falsifiable prediction: annotation-related diagnostics on LLM-authored Vibra
drop substantially, with no measurable regression in constrained-decoding
acceptance rate. This is testable with the existing `vibra effects` report.

### T3 — Handle lifecycle: sharing analysis first, typestate later

**Resolution: adopt "in-danger" propagation now; hold revocable-capability
typestate as the target design; do not adopt linearity.**

*Typestate via Revocable Capabilities* is the most directly relevant paper to
Vibra's problem, and its key structural insight is that **capabilities need not
be linear**. State is an abstract type member, capabilities are path-dependent
types (`f.IsOpen`), and revocation is a compile-time-only operation that
extends a killed-set. Safety comes from requiring each use's reachability
qualifier to be transitively disjoint from the killed set — so closing `fA`
invalidates anything that may reach it, while unrelated handles stay usable.
Crucially, path-dependence is what supplies resource *identity*; reachability
or capturing types alone provably cannot, since they only express "may refer
to."

That matters because Vibra's handles are copyable and the effect-system
contract explicitly defers affine ownership. Linearity would break the copyable
model and impose exactly the annotation burden T4 warns against.

But the paper is a design paper: no benchmarks, no mutable-field support, and
it assumes a DOT-style host with path-dependent types Vibra does not have.
Adopting it wholesale is a type-theory research project.

The cheap approximation comes from the 2008 safe-memory paper: **"in-danger"
propagation** — statically taint everything that shares structure with a
destroyed value. This gives borrow-checker-like safety without linearity and
without new type theory. The honest caveat, which that paper is explicit about:
the sharing analysis, not the typing rules, is the expensive part.

Recommendation: implement in-danger propagation over host handles to catch
use-after-close and double-close at compile time. Revisit path-dependent
typestate only if handle protocols grow beyond open/closed.

### T4 — Verbosity: keep signatures, cut nesting and derived annotation

**Resolution: mandatory signatures are evidence-backed. Deep nesting and
transitive restatement are not.**

The Dafny result is the strongest argument for mandatory structure anywhere in
the set: supplying models with nothing but a method signature moved
GPT-OSS-120B from 0% to 63.64% verify@5, and a 9B model to 72.73% — beating
models 13× larger. Treat the direction as real and the magnitude as unreliable;
n=11 per cell means a single problem is worth 9 percentage points.

The counter-evidence is equally important. Anka *lost* to Python by 10 points
on nested-conditional logic — added structure hurt where the domain was
control-flow-heavy. And LLMON found flattened dotted nesting outperformed trees
at the model interface. Vibra's `(do ...)`-wrapped bodies, nested matches, and
match-inside-match error handling are precisely the shape both findings warn
about.

Four concrete changes, in descending evidence strength:

1. **Forbid shadowing outright.** Anka attributes 42% of its Python failure
   cases to variable shadowing, and three papers independently converge on
   identifier-based reference to named intermediates as *the* primitive worth
   having (Anka's `INTO`, LLMON's `exec` binding, Pel's `def` + pipe). Vibra
   already has `let`; making shadowing an error rather than a style question is
   nearly free and directly targets a measured failure mode.
2. **Add result propagation.** A `try`-style form collapsing the four-line
   `match` on every fallible call is the single largest nesting-depth reduction
   available. `examples/fs-roundtrip.vib` would lose two levels.
3. **Make an unhandled `result` in statement position a diagnostic.** Today
   `(stream.write.string out "...")` can discard a `(result void stream.error)`
   silently. For a language whose thesis is safe-by-default, this is the wrong
   default.
4. **Consider a pipe form with an explicit placeholder** (Pel's `^`), which
   matches left-to-right generation order. Weakest evidence here — Pel has no
   evaluation at all and should be read as a mechanism catalog.

### T5 — What compilation preserves: two modest claims, stated separately

**Resolution: claim effect-ceiling preservation and compartment isolation.
Do not attempt verified compilation.**

Monniaux gives the number that settles this for a small team: a layout-changing
verified pass costs 20–40 lines of proof per line of implementation (stack
canaries went 97 → 1689; tail-recursion 69 → 2641) to buy under 1% runtime
overhead. SECOMP's compartment-isolation proof took ~43k LoC and a multi-year
team, still compiles only tiny programs, and its top-level theorem is not yet a
single Coq artifact. Verified compilation is not fundable here.

But two cheaper things are available, and one of them is close to free:

- **Wasm already supplies what SECOMP had to bolt onto RISC-V** — well-bracketed
  call/return and numeric-only parameters. Vibra starts nearer a defensible
  compartment design than CompCert did.
- **SECOMP's proof only works because the cross-compartment ABI carries scalars
  only.** No pointer passing, no shared memory; extending it to shared pointers
  is called a major open problem. This is a design constraint to adopt *now*,
  while the host ABI is still small, because it is very expensive to retrofit.

Two rules to record as compiler invariants immediately, while there are few
passes to reorder:

- **Hardening passes run last.** Kruse et al. show pass composition preserves
  only the *intersection* of property classes, and only under a well-formedness
  side condition. Pass ordering is a security constraint, not a performance
  preference.
- **Any security mode a caller can disable must be re-established at every
  entry point**, because attacker context runs between your calls.

Finally, a discipline about language: Monniaux proves his transformation
preserves semantics and explicitly *not* that the canary stops the attack.
Vibra should keep those two claims separate in every statement it makes, and
should never make the second.

### T6 — Retrieval: emit the index as a compiler artifact

**Resolution: add `vibra index`, evaluate it by elimination power, and put the
type-checker behind it as the exact verifier.**

Three findings reshape what this artifact should be:

- **Evaluate first-stage retrieval by rejection precision, not ranking.**
  ProjAgent's retrieval signal has 5.4% precision but ≥97.9% precision on what
  it *rejects*. A first stage that eliminates well is worth more than one that
  ranks well, provided an exact verifier sits behind it. Vibra has that
  verifier: the type checker.
- **Normalization must be symmetric.** Applying style normalization to query
  and corpus lifts weak models 28–29 points, and must not strip comments or
  identifiers. This is `vibra fmt` over both sides of the index — Vibra's
  idempotent formatter is already exactly the right tool, which is a second
  payoff from the canonical-syntax investment.
- **No universal embedder exists.** Qwen3-Embedding-600M beats a 30B decoder by
  85 points on Java xCodeEval at 1/50th the size; training objective dominates
  parameter count, and generative models used as embedders are Pareto-dominated.
  Do not assume the biggest available model is the right retriever.

The artifact is cheap because `vibra effects` already computes call edges and
declared/performed sets. A per-function record of fmt-normalized source,
signature, effect set, call edges, and error types covers what the Principles
page asks for.

One further note with an unusually direct bearing on Vibra: ProjAgent's
static-analysis feedback loop was worth only +0.9 points, and its authors
attribute that to Python's dynamic typing forcing the analysis to be
conservative. Their stated future work asks for the type inference Vibra
already has. This is the best external evidence in the set that Vibra's static
core is the right foundation for agent tooling.

### T7 — Bounded execution: extend the scope limits that already exist

**Resolution: add fuel and a memory ceiling to `ScopeLimits`; make the entry
budget explicit in `project.vib`; fix the non-reclaiming arena.**

SandCell is the most directly copyable paper in the environment set: roughly
two lines of specification per real application, boundaries following existing
module structure, containing bugs originating in `rustc` and the standard
library rather than only in `unsafe` code. That is a realistic target for a
small team.

Its cost finding is the design-critical one: **the dominant cost is
boundary-crossing frequency and data volume, not the enforcement primitive.**
Allocating cross-boundary data in a shared region up front took rouille from
89% overhead to 3%. Combined with SECOMP's scalars-only constraint, this says
the cross-compartment ABI should be designed for few, coarse, scalar-carrying
crossings from the start.

Vibra already has `ScopeLimits` and deadlines. Adding fuel and memory caps
extends existing machinery. The non-reclaiming value arena in
`src/wasm_backend.rs` must be fixed regardless, since it makes any memory
bound unenforceable.

## Ranked by evidence per unit cost

**Tier 1 — strong evidence, low cost. Do these first.**

| Change | Evidence | Tension |
| --- | --- | --- |
| Forbid shadowing | 42% of Anka's Python failures | T4 |
| Unhandled-`result` diagnostic | Safe-default principle; no counter-evidence | T4 |
| Result propagation form | Nesting depth; LLMON flat-beats-nested | T4 |
| Effect inference with boundary declaration | Decoding needs signatures, not derived facts | T2 |
| `vibra index` retrieval artifact | ProjAgent, Recall-Before-Rerank | T6 |
| Scope fuel + memory ceiling; fix arena | SandCell; currently unbacked claim | T7 |
| Scalars-only cross-compartment ABI rule | SECOMP; expensive to retrofit later | T5 |
| Record "hardening runs last" invariant | Kruse et al. | T5 |

**Tier 2 — strong evidence, medium cost.**

- Capability grants as runtime enforcement of static effects (T1). Runtime
  substrate exists; needs surface syntax and propagation.
- A type-constrained decoding service (T1/reframe). The genuinely novel
  contribution available to this project, and the reason to treat "small
  decoding automaton" as a type-system design criterion.
- In-danger propagation for handle lifecycle (T3). Sharing analysis is the
  cost centre.

**Tier 3 — weak evidence or poor cost ratio.**

- Path-dependent typestate for handles — needs type theory Vibra lacks; the
  source paper has no benchmarks and no mutable-field support.
- Contracts and refinement types — no paper in this set evaluates them for LLM
  authoring.
- Any verified-compilation proof effort — 20–40:1 proof-to-code. **Recommend
  against.**

## What not to do

- **Do not add more syntax discipline expecting correctness gains.** The
  measured ceiling is ~9%; the remaining 94% of errors are type errors.
- **Do not treat erased static effects as a security mechanism.** That is
  LLMON's representation-without-enforcement failure.
- **Do not pass pointers across compartment boundaries.** SECOMP calls shared
  pointers a major open problem.
- **Do not build a repair loop before a well-formedness gate.** Contextless
  self-healing left most models at 0%; signature-guided self-healing reached
  81.82–90.91%. Gate any Vibra repair loop on "parses and type-checks."
- **Do not treat "compiles and runs" as an admission criterion.** Compiled AI
  found 4% of artifacts compiled and ran cleanly while producing wrong output.
  An accuracy gate against golden data is mandatory.
- **Do not claim the compiler stops attacks.** Claim semantic preservation;
  keep the two statements separate.

## Documentation actions this review implies

Independent of any design change, the following are already true and should be
corrected:

- `decisions/philosophy.md` still recommends "regular YAML shapes" and reserved
  `$` keys after the S-expression cutover, and justifies canonical syntax on
  grounds this synthesis shows to be weak.
- `decisions/s-expression-language.md` still carries `policy.narrow` and
  capability grammar behind a note saying they were decommissioned, and
  specifies `fn`, `(case ...)` in `match`, and `(def name Type expr)` constants
  that the corpus does not use.
- Neither document records that `CapabilityGrant` machinery survives in the
  async runtime.
