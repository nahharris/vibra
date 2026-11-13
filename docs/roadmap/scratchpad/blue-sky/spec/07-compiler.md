# 07 Compiler

Status: draft specification; not normative

"Compiler" here means everything between text and a running artifact. It
is organized so that almost none of it has to be trusted.

## Pipeline

```text
text ──parse──▶ tree ──resolve──▶ tree with ids ──elaborate──▶ core
                                                                │
                              ┌──────────── kernel ◀────────────┘
                              ▼
                        obligations ──checkers──▶ evidence ──▶ admission

core ──lower──▶ machines ──optimize──▶ backend ──▶ artifact
                                 └── validated against the evaluator
```

| Stage | Input to output | Trusted | Checked by |
| --- | --- | --- | --- |
| Parse, print | Surface text and tree | No | The round-trip law, tested on every staged cell |
| Resolve | Names to claim ids | No | The kernel sees only ids |
| Elaborate | Surface tree to core | No | The kernel checks the core |
| Kernel | Core to verdict and obligations | Yes | Review. It is small |
| Checkers | Obligations to evidence | Only for grade `solved` | The kernel, when a certificate is produced |
| Lower | Procedures to machines | No | Validation |
| Optimize | Core to core | No | Validation |
| Backend | Core to target code | No | Validation and replay |
| Reference evaluator | Core to values | Yes | Review. It is the definition |

## The core language

Every cell elaborates to **core**: a small, explicitly typed, first-order
functional language with no surface conveniences.

```text
term = name | literal
     | (call id term*)            application of a cell or primitive
     | (make type case term*)     construction
     | (read term field)          field of a known case
     | (case term (k term)*)      exhaustive choice by variant case
     | (if term term term)
     | (let name type term term)
     | (local name params measure term term)  nested function
     | (intent op term*)
```

Elaboration removes:

| Surface | Core |
| --- | --- |
| Nested `fn` | `local` |
| Early `return`, `else if` | Nested `if` and `case` with one result |
| `[…]` and `{…}` literals | Calls to the constructors of the expected type |
| `match` with narrowing | `case` with `read` |
| `try` | `case` that returns the failure |
| Labeled arguments | Positional arguments |
| Label lifting | Explicit label types on every term |
| Interface calls | Calls to the resolved implementation |
| Generic calls | Calls with explicit type arguments |
| Notations | The declarations they elaborate to |

- A body MUST denote the same function as its elaboration. This is the
  definition of what a body means.
- Core has no `await`, no `emit`, and no statements. Procedures and
  handlers become functions that return a state and a list of intents.

## The kernel

The kernel is the only component whose acceptance means anything.

It decides, without help:

- that a core term is well typed;
- that every `case` is exhaustive and every `read` is of a known case;
- that every recursive call sits under a stated measure;
- that owned and resource values are used as required;
- that labels are respected and intents stay within `uses`;
- that a cell references only claims, and its closure is within budget.

It generates the obligations of [04](04-claims.md#obligations), each as a
closed formula with its context. It checks proof certificates. It records
verdicts.

Requirements:

- The kernel MUST NOT depend on any surface, solver, optimizer, or
  backend.
- The kernel MUST be deterministic and MUST terminate on every input.
- The kernel SHOULD be small enough to be read in full by one reviewer,
  and SHOULD have a second independent implementation used as a
  cross-check.
- No agent role can extend or configure the kernel
  ([01](01-programs.md#roles)).

## Checkers

Checkers turn obligations into evidence. They are untrusted proposers.

- **Evaluation.** Examples, generated inputs, and enumeration run on the
  reference evaluator.
- **Solver bridge.** An obligation is translated to a solver query in
  which each called function is represented by its claim. A `means`
  clause is unfolded a bounded number of times.
- **Counterexamples.** A model returned by a solver MUST be translated to
  Vibra values and confirmed by evaluation before it is reported. An
  unconfirmed model is reported as "unknown", not as a counterexample.
- **Certificates.** Where a solver emits a proof, the kernel checks it
  and the grade is `proved`. Otherwise the grade is `solved`.
- **Cost.** A bound is inferred from core by resource analysis and
  compared with the claim.

## Readings

A total, pure definition can be evaluated in more than one way. Each
reading is a tool obtained without writing another program.

| Reading | Produces | Used for | Layer |
| --- | --- | --- | --- |
| Concrete | A value | `eval`, examples, replay | 0 |
| Symbolic | A formula | Obligations | 1 |
| Generative | Inputs that satisfy a predicate | Tests, determinacy and vacuity checks, simulation | 1 |
| Abstract | A cost bound | `cost` | 3 |
| Exhaustive | Reachable states | Exploration | 3 |
| Inverse | An input from an output | Layouts and other invertible notations | 4 |
| Incremental | An output change from an input change | Derived state kept up to date | 4 |
| Superposed | Results for many variants at once | Mutation checks | 4 |

The concrete reading is the reference evaluator and defines the language.
Every other reading MUST agree with it wherever both apply.

## Procedures to machines

A procedure is lowered to a machine.

1. Each `await` is a **suspension point**. The code between suspension
   points is a step.
2. The machine's state is a variant with one case per suspension point.
   The fields of a case are the bindings live at that point.
3. The handler for a case takes the awaited outcome as its event, runs to
   the next suspension point, and returns the next state and the intent
   to request.
4. Awaiting a procedure starts a child instance and waits for its result
   event.
5. A composite intent is requested as a whole. The runtime performs its
   parts and answers once.

The derived machine is a cell. Its journal, replay, simulation, and
exploration are those of any machine. The live bindings at a suspension
point are part of the durable state, so resources and labels are checked
across `await` exactly as across a call.

## Realization

Performance comes from two places.

**Witness replacement.** The main route. An agent writes a faster witness
for an unchanged claim and the store admits it on evidence
([04](04-claims.md#meaning)). No compiler has to be clever, and no
dependent is rechecked.

**Optimization.** The optimizer is untrusted and may do anything.

- A rewrite justified by a lemma of the form "left equals right" is
  correct by that lemma's evidence.
- Any other transformation MUST be validated per build: the optimized
  code is run against the reference evaluator on the cell's examples,
  generated inputs, and available journals. The build manifest records
  the method and the sample.
- Open, Layer 4: search over lemma-justified rewrites by equality
  saturation.

## Backends

- The **reference evaluator** interprets core directly.
- The **WebAssembly backend** is the first compiled backend.
- A native backend MAY be added.

Every backend MUST:

- give bit-identical results to the reference evaluator for every pure
  call, including `f64` and the iteration order of maps and sets;
- produce journals that the reference runtime verifies by replay, and
  verify journals the reference runtime produces;
- charge steps consistently enough that budgets fault in the same step;
- run a call in tail position without consuming stack.

## Incrementality

- Every stage is a function of hashes. Results MUST be cached by cell id
  and tool version and MUST NOT be recomputed otherwise.
- Changing a witness rechecks one cell.
- Changing a claim rechecks that cell and its direct dependents. A
  dependent whose own claim is unchanged stops the propagation.

## Support for generation

- The line surface is designed so that the keyword at the start of a line
  determines its grammar. The toolchain MUST publish the grammar of each
  surface, and of each notation in scope, as data.
- The kernel MUST answer, for any hole, the expected type and the names
  in scope that fit it ([08](08-tooling.md#holes-and-moves)). A decoder
  constrained by these answers cannot emit an ill-typed leaf.
