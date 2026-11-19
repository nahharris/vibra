# Blue-sky adversarial review: ledger

Status: merged after `r4-codex.md` (amendments M1 to M10 accepted by Claude); human readability open; working notes, not normative

One row per decision. Sources: `r2-claude.md`, `r2-codex.md`,
`r3-claude.md`, `r3-codex.md`, `r3-codex-cross-review.md`, `r4-codex.md`.

## How to read a row

**Status** is one of:

- **agreed**: both reviewers hold it after attacking it.
- **default**: chosen under rule 9 because evidence is insufficient. It
  is reversible and is not a measured conclusion.
- **deferred**: out of the current gate. Deferring a feature settles
  nothing about it.
- **open**: waiting on evidence named in the row.

**Support** is the provenance of the relevant evidence or reasoning.
Tags are not a ranking (rule 2). "owner" marks a scope instruction from
Hannah, which is an input and not a measurement. A default may be
informed by a measured pilot while its adoption stays a judgment.

**Reopen when** names the event that puts the row back on the table.
"None" or "Never" there means no scheduled trigger. It does not make
the row immune to a relevant counterexample, an inconsistency with
another row, or an owner scope change.

Gates: 0 benchmark and spike; 1 a useful command-line program; 2
libraries and local parallelism; 3 services and durability.

## A. Goals and author model

| # | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- |
| A1 | Vibra is built. Existing languages are baselines and sources of mechanisms, not alternatives | agreed (owner input) | owner | The owner changes it |
| A2 | Product order: command-line programs, then libraries, then services. Native UI is future work | agreed (owner input) | owner | The owner changes it |
| A3 | The mission for gate 1 is safe, useful changes to a command-line program. Replay and simulation are profiles added where a workload needs them, not a requirement on every effect | agreed | argued | A gate-1 acceptance case needs replay |
| A4 | Claims about what LLM authors find easy are hypotheses per model population, measured and dated, not laws | agreed | argued | Never closed: each new cohort is new evidence |
| A5 | The language card is short and complete, with retrievable checked examples. Card size is measured, not capped at a fixed token count | agreed | argued | A card grows until task results fall |
| A6 | Human readability of claims is its own measured endpoint, separate from model error rate and token cost | agreed | owner, argued | See X6-3 |

## B. Programs and source

| # | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- |
| B1 | Files in the repository are the source. The content-addressed store is a cache with no authority | agreed | argued | A workflow needs an edit transaction that files and compare-and-swap cannot give |
| B2 | Claim and witness keep separate hashes, so a change is classified as claim-changing or witness-only | agreed | argued | Classification proves useless in review practice |
| B3 | Approval compares against a protected approved-claim manifest with exact file hashes and tool, driver, and policy versions. The approval and checking path is protected for a stated, narrow threat model | agreed | argued | The dirty-tree, stale-file, and replaced-script tests fail |
| B4 | No store overlay: `stage`, `status`, `publish`, and `sync` are removed | agreed | argued | As B1 |
| B5 | Five declaration kinds: `type`, `fn`, `interface`, `impl`, `proc`. A target is manifest data, not a declaration | agreed | argued | A gate needs `machine` (G3) or a proof form (E6) |
| B6 | A definition is written once. A second, faster realization is optional and carries a stated reference relation and evidence scope; finite differential agreement is not equivalence | agreed | argued | A workload needs verified optimization |
| B7 | No admission bans on duplicate meanings or node budgets. Reuse suggestions and size diagnostics replace them | agreed | argued | Duplication measurably harms a corpus |
| B8 | A running instance pins its code epoch. Durable frames are versioned | agreed, applies from gate 3 | argued | Gate 3 design |
| B9 | Repository policy replaces the five language-level roles | agreed | argued | A threat in scope needs a role the repository cannot express |
| B10 | Executable, build, and evidence identity pins the resolved transitive witnesses and the target, tool, driver, and policy bindings, not the claim hash alone. A witness-only edit can keep claim approval and still invalidate execution evidence | agreed | argued | A dependency body changes without its claim changing, and stale execution evidence is not rejected |
| B11 | Persistent semantic and schema contracts are versioned as they are introduced; manifests and cache identities include interpretation versions. An unknown version is unsupported, and migration is deliberate. Journal, frame, and ABI versions enter with their gates | agreed | argued | An old artifact is read after an incompatible change to normalization, arithmetic, ABI, or schema |

## C. Syntax

| # | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- |
| C1 | The prototype surface is K3 (see X1b-8): an opening keyword, sections that each start with a word and no colon, and a typed closing keyword (`fn … endfn`, `if … endif`, `match … endmatch`, `type … endtype`). A colon only joins a pair, `name: type` or `name: value`. Braces declare a shape (`takes {…}`, `fields {…}`, case fields) and never appear in an expression. There are no separator commas: items are separated by spaces or line breaks. Layout is not significant. The registered X1 rule selected K specifically over B, D0, and P (C2); K3 is a later variant of K, not something X1 measured | default by owner decision within the measured K family | measured (pilot) for the family; owner for K3 | A held-out comparison of K3 against K, or a writer or reader problem traced to missing separators |
| C2 | What was measured for C1: full-task success of Haiku K 27/30, B 25/30, D0 20/30, P 20/30 and Luna K 24/30, D0 24/30, B 24/30, P 21/30. K's observed score is no lower than any other package in either cohort. There is no claim of significance, noninferiority, or equivalence | agreed as a statement of the data | measured | A larger run |
| C3 | Packages D0 (significant indentation, positional prefix calls) and P (uniform parentheses) are left out of the prototype. They stay as registered controls and archived data. The package comparison isolates no single cause | agreed | measured (pilot) | As C1 |
| C4 | Inside the surface, these parts were fixed by the cards and not measured one by one (X1b tested K2 and K3 as separator and declaration bundles, not each site): calls `f(a b)` with positional arguments; `x: int` annotations; statement `let` with no closer; `=` in `example`; `list(T)` type application | default | argued | The arm that tests the part (C5 to C8), or a parser or reader problem traced to it |
| C5 | Arithmetic and comparison are named calls (`int.add`, `int.lt`). Arm X2 (infix against named) was not run. With K3, infix operators would collide with space-separated items (`[a -1]`), so adding infix now also needs a separator answer | default; arm deferred | argued | Before adding infix operators: fixed precedence and grouping, a rule for negative literals between items, D0 and K controls, tasks t1 and t6 |
| C6 | Ordinary calls are positional. Record and variant constructor fields are labeled. Arm X3 (argument labels) was not run. This is unrelated to security labels (D5) | default; arm deferred | argued | Before adding declared call labels or changing constructor conventions: tasks t6 and t8 |
| C7 | Functions are passed by name, including nested named functions. No lambda. Arm X4 was not run; the scope and callback errors seen in X1 are an unresolved cost | default; arm deferred | argued | Before adding lambda syntax or claiming named helpers are as easy: tasks t5 and t6 |
| C8 | Local `let` carries its type. Arm X5 (inferred against typed) was not run | default; arm deferred | argued | Before allowing omission: tasks t4 and t5 with fixed checking strength |
| C9 | No loops and no rebinding. Repetition is recursion and library transforms. Structural termination measures are inferred where supported; otherwise the checker asks for a measure or reports unsupported | default | argued | Demonstrated friction on gate-1 programs |
| C10 | The one-repair arm (one round of checker feedback) was not run. First-attempt results are not repair evidence | deferred | none | The gate-1 diagnostic format is stable enough to freeze |
| C11 | Notations, table syntax, and protocols are out | deferred | argued | A domain shows demand with an acceptance case |
| C12 | Token cost is reported from provider usage, separately from bytes and lexical counts. Observed Haiku mean output per trial: B 1,017, K 1,076, D0 1,314, P 1,759 | agreed as method; numbers are pilot | measured | A larger run |

## D. Types

| # | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- |
| D1 | Nominal types: wrapped, record, variant. A wrapped type may carry an executable condition (refinement), checked at construction | agreed | argued | Gate-1 programs need a refinement the fragment cannot state |
| D2 | Unrestricted dependent types are out. Useful indices are not ruled out | deferred | argued | A matched task shows indices beat equivalent refinements |
| D3 | No affine or linear `own` or `resource` surface at this gate. Immutable values may be shared; the runtime mechanism is chosen at I1, and copy-on-write is a candidate, not a settled safety argument. No static uniqueness, must-consume, or must-reply guarantee is claimed | deferred, with the guarantee explicitly dropped | argued | A real resource or reply protocol needs it; retained-graph and deep-drop cases under the chosen runtime |
| D4 | Partial library operations (`list.first`, `int.div`) are checked at run time. A guard is erased only when a proof or a sound dominating check justifies it. Sampled evidence never erases a guard | agreed | argued (counterexample in `r2-codex.md` section 3) | Only with a validated proof or a sound dominating check |
| D5 | Security labels and `releases` are out. Opaque types give abstraction only. Vibra makes no information-flow promise | deferred, with the guarantee explicitly dropped | argued (the public Boolean observer leaks) | A named secrecy policy is required, with a two-run test |
| D6 | One sequence interface where useful. Complexity and uncertainty contracts stay explicit; `uncertain` is not merged into `option` | agreed | argued | Workload data |
| D7 | Nominal identity includes a stable nominal seed, not structure alone: same-shaped `cents` and `meters` stay distinct. The seed, rename, and migration policy is fixed before identity-sensitive implementation | agreed; policy open | argued | Structurally identical types become interchangeable, or identity fails to survive an intended edit |

## E. Claims and evidence

| # | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- |
| E1 | Claim clauses at gate 1 share one executable core. `requires` and `ensures` are boolean guards checked on entry and on return. `example` is a concrete call with its expected result, checked by `check` or `test`, not on every call | agreed | argued | A claim that matters cannot be stated executably |
| E2 | Gate 1 has no general solver-driven proof of behavior. Examples and tests give sampled evidence only after they run; before that a statement is unverified or unsupported. Static typing, exhaustiveness, and supported structural termination remain separate static judgments | agreed | argued | Gate 2 or 3 needs universal claims |
| E3 | Three classes are kept apart: executable guards (kept at run time, see D4), general propositions (reported as scoped evidence, not monitored), and resource budgets (enforced by the runtime, see E7) | agreed | argued (the `forall n` counterexample) | Never as a merge |
| E4 | An evidence record holds the exact statement, coverage (`universal`, `bounded`, `sampled`, `unverified`), method, trust, dependencies, revision, and the domain, bounds, or seeds | agreed | argued | Policy tests cannot be written against it |
| E5 | `proved` is shown only for universal evidence checked by the kernel or a certificate checker, relative to the declared trusted base, with no open assumption beyond it. A solver-only verdict is shown as trusted-tool evidence. A conditional proof keeps its assumption visible. A known counterexample blocks the property whatever older records say | agreed | argued | Never as a weakening |
| E6 | Proofs and lemmas are out of gate 1. If they return, they may share `fn` spelling but need explicit proof-phase and erasure rules | deferred | argued | A gate needs universal evidence |
| E7 | Cost is enforced as fuel and quotas on an execution. Cost-bound claims are descriptive at most | agreed | argued | A workload needs a guaranteed bound |
| E8 | Contracts may leave the result underdetermined; a reference realization is optional | agreed | argued | None |
| E9 | Tooling direction, not a gate-1 requirement: a scoped check that, for a call in a supported fragment, reports one of: call not allowed; no result allowed; exactly one result allowed; at least two results allowed; unknown or unsupported. "Exactly one" needs a witness and the exclusion of every alternative (a proof, or exhaustion of the whole finite output domain, not a prefix of the integers). Failing to find a second result proves nothing. The report states input, fragment, bounds, witnesses, and evidence status | agreed as direction | measured for the motive (Luna's reader errors fall on forced or underdetermined items); argued for the design | Evaluated on its own when built: zero, one, and two allowed boolean results, and an unresolved integer case |
| E10 | Mutation and example checks are scoped findings with explicit unknowns | agreed | argued | None |

## F. Effects and capabilities

| # | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- |
| F1 | Authority is held as unforgeable capability values passed as parameters. There is no `uses` clause and no effect index | agreed | argued (no counterexample found under explicit roots and sound delegation) | A required local policy, such as "this callback may log but not use the network", cannot be expressed conservatively |
| F2 | Root authority is `main`'s parameters plus a reviewed entry manifest that states the actual resources, scopes, limits, and driver identities | agreed | argued | The two-directory attenuation test fails |
| F3 | An effect-producing callable or delegated intent whose type hides its resource family gets a conservative "unknown or broad" latent-authority summary for local policy checks. The summary grants nothing: operations stay confined to the capabilities delegated through captured values and the reviewed roots. A pure callback is not assigned effects for being a function value. Exact resource-family auditing without reading the witness is the precision deferred | default | argued | A concrete policy cannot be met conservatively; the `make-writer` example under two attenuated roots, a pure mapper, and counterfeit handles |
| F4 | A port is an `interface`; a driver or model is an `impl`. Dispatch is static and generic. Handles are nominal and unforgeable: a structural record with matching names mints no authority | agreed | argued | The counterfeit-handle test fails |
| F5 | Model mode is explicit. A model cannot reach real input or output, and takes its nondeterminism from a harness-controlled `choose` whose alternatives are inspectable | agreed | argued | The isolation test fails |
| F6 | `proc` is the only effectful form at gates 1 and 2 | agreed | argued | G3 |
| F7 | Gate 1 ships in-memory fakes for arguments, console, standard input, and scoped files, plus a small real-driver integration suite. Trace-backed fakes report unsupported for requests outside the trace | agreed | argued | Fake and driver disagree on a shared fixture |
| F8 | Delivery, idempotency, and outcome metadata (once, repeatable, unknown) applies to every effectful operation from gate 1, including file and console operations: "not observed" is not "did not happen". Only durable deduplication and reconciliation storage waits for the durable profile at gate 3 | agreed | argued | Crash, retry, and cancellation cases |

## G. Execution model

| # | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- |
| G1 | Journaling is a per-target profile: none, ring, or durable. Profiles share normal transitions and differ after a crash; each states the evidence it cannot give | agreed | argued | The interrupted-operation test shows a profile promising more than it has |
| G2 | State hashing strategy is measured, not assumed | open, gate 3 | none | Gate 3 spike |
| G3 | `machine` is not in the language now. A recursive `proc` can encode a reducer but keeps explicit state, correlation, overload, and cancellation obligations; the two are not claimed equivalent | deferred | argued | The services gate, with the status-during-slow-write test |
| G4 | One local effect-order sequencer with stated queue, timer, and causal rules, including equal-instant chains. It orders effects; it does not make pure computation serial. Distributed time is out | agreed for local; distributed deferred | argued | Distribution enters scope; bounded pure parallel work with ordered effects and responsive timers |
| G5 | The actor model is not adopted as the default. Structured scoped concurrency is (H1). Owned long-lived machines are a gate-3 question | default | argued | G3 |
| G6 | Durable journals classify payload and metadata, key ownership, retention, redaction, compaction, and deletion, and state which replay guarantees each operation gives up | deferred, gate 3 | argued | A secret-bearing trace is exported, redacted, or deleted |
| G7 | Simulation and what-if execution has isolated credentials and mode provenance; it cannot reuse production operation identities or dispatch real effects | deferred, gate 3 | argued | An imported trace or what-if branch routes an old operation to a production driver |

## H. Concurrency and parallelism

| # | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- |
| H1 | Concurrency is structured and scoped, with bounded fan-out and no new syntax, only library operations. Leaving a scope joins or ends its local child tasks under a stated cleanup contract. That says nothing about an external operation already dispatched: its outcome follows H4 and F8, and late replies are handled explicitly | agreed | argued | Slow-sink, nested fan-out, early-failure tests; a child cancelled after dispatch whose late success arrives after the waiter ended |
| H2 | Data parallelism is a bounded, ordered parallel map with sequential fallback. Purity removes shared-value races; it does not promise speedup or bound memory | agreed | argued | Gate 2 measurements |
| H3 | Parallel reduction requires stated laws, domain, and order, or a fixed reduction tree. A fixed tree over floating point is allowed without claiming it equals the left fold | agreed | argued | None |
| H4 | Cancellation is a request. Its outcome may be uncertain and is reported as such | agreed | argued | None |
| H5 | No backend conclusion about threads until a concrete host and engine are chosen and measured | open, gate 2 | none | Target chosen |

## I. Runtime

| # | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- |
| I1 | The memory mechanism is not chosen. Agreed requirements on whatever is chosen: safe, quota-aware, and no constant-latency claim | open for the mechanism, gate 0 spike; agreed for the requirements | none | Retained-graph and deep-drop tests |
| I2 | Driver conformance checking is sampled and labeled with frequency and domain; a small real-driver boundary corpus is mandatory | agreed | argued | An off-model outcome is missed in a way that matters |
| I3 | Lightweight capability fakes are gate 1. Scheduler and fault simulation, and state-space exploration, are gate 3; exploration reports exactly what space was checked | agreed | argued | Gate 3 |
| I4 | Evaluation is fuel-metered, with limits on integer, text, and collection size | agreed | argued; exercised in the X1 harness | None |
| I5 | State identity for exploration covers the whole future-relevant configuration: pending operations, queues, timers, model state, children, and code epochs. Reports state domains, depth, fairness and environment assumptions, and exhaustion | deferred, gate 3 | argued | Equal user state with different outstanding outcomes loses a reachable failure |

## J. Compiler

| # | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- |
| J1 | The trusted base is published as inventories, not equated with the logical checker. Logical: parsing, resolution, elaboration, the checker. Execution: the reference evaluator or lowering and primitive semantics. Security: capability confinement, resource enforcement, and the host and driver boundary. Obligation translation and certificate checking join when introduced; persistence and recovery at gate 3. A second checker is deferred, not forbidden. The X1 harness does not verify a Vibra frontend or runtime | agreed | argued | Wrong same-typed name resolution, a mistranslated obligation, a forged handle, a bypassed fuel counter |
| J2 | Agreement between backends on finite inputs is called differential testing, and is kept separate from artifact assurance | agreed | argued | None |
| J3 | Only the analyses the current gate's workload needs are implemented. Unsupported fragments answer unsupported, never a guess | agreed | argued | None |
| J4 | Gate 1 bundles the reference runtime; a new backend is not a gate-1 requirement | agreed | argued | Performance blocks a gate-1 acceptance case |
| J5 | The core is typed and higher-order, or uses explicitly checked closure conversion that accounts for captured bindings, passed and returned functions, and indirect calls. Calling the surface first-order does not remove these obligations | agreed | argued | A captured, passed, and returned callable is lowered with the wrong type or capability provenance |

## K. Graphics and native UI

| # | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- |
| K1 | UI is out of scope for now (owner input). The candidate for later: a semantic view and normalized input behind the capability boundary, native widget drivers, an optional canvas, and a reused host stack. Tests combine semantic checks with screenshots, layout, and accessibility | deferred | owner, argued | The owner opens UI |

## L. Tooling

| # | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- |
| L1 | The first command set is `init`, `fmt`, `check`, `test`, `eval`, `show`, `find`, `run`, `build` | agreed | argued | The clean-install create, repair, test, package, and run test needs another verb |
| L2 | `check --against <approved-claim-manifest>` classifies changes; `build` records the exact checked snapshot | agreed | argued | B3 |
| L3 | `check` returns expected and found where meaningful, a validated counterexample when one is found, and otherwise a diagnostic or an explicit unknown with reason and scope. It never invents a counterexample or implies none exists | agreed | argued | None |
| L4 | `fmt` normalizes parsed programs and offers only uniquely mechanical fixes | agreed | argued | None |
| L5 | `eval` and `test` are bounded and reproducible | agreed | argued | None |
| L6 | `show` and `find` return signatures, evidence, and examples in bounded, versioned output, and distinguish checked examples from observations | agreed | argued | None |
| L7 | Generated cards, diagnostics, examples, and grammar data ship. MCP and edit-move protocols do not | deferred for the adapters | argued | A real client workflow improves with an adapter |
| L8 | No decoder dependency and no assumed token advantage enter gate 1. Whether a provider's type-constrained or grammar-constrained decoding helps is not known | agreed for the exclusion; open for the mechanism | argued; the cited figures were unverified | A provider adapter is built and measured |
| L9 | Machine-readable diagnostics and results are bounded and carry schema and tool versions, the exact checked revision or content identity, and a precise status: passed, refuted, unknown, or unsupported. Proposed by Codex in round 4 and accepted by Claude there; not a round-2 agreement | agreed | argued | A clean agent repair-and-run workflow depends on unstable prose or an unpinned tree |

## N. Delivery and method

| # | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- |
| N1 | Gate-1 acceptance: create, check, repair, test, package, and run a real file-validation or file-transform program on a clean Windows machine, covering arguments, standard input, console, scoped files, typed failures, exit status, pinned dependencies, and a distributable bundle with the reference runtime. A syntax pilot does not pass this gate | agreed | argued | The end-to-end task fails or needs undeclared host state |
| N2 | Evaluation is versioned, with protected held-out tasks, fixed resource and defect criteria, and stated rules for stopping or redesigning a mechanism. Existing languages are baselines | agreed | argued | Before a mechanism or gate claims better defect or authoring outcomes |
| N3 | Host and dependency integration is narrow, typed, and versioned, with explicit authority and assurance boundaries. Packaging is local and offline first; no registry and no promise of an effortless ecosystem. Conversion, model, and upgrade work is counted. Proposed by Codex in round 4 and accepted by Claude there; not a round-2 agreement | agreed as direction, gate 2 | argued | An indispensable real dependency is integrated and upgraded |

## X. Experiment record

| # | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- |
| X1-1 | The X1 writing pilot ran as registered: 4 packages, 10 tasks, 3 runs, 2 model families, 240 trials, artifacts frozen by hash, prompts identical across families | agreed | measured | None |
| X1-2 | Task t10 is defective (Claude's artifact): its text never names the required `parse-args`. It stays failed in the registered count; the count without t10 is a labeled post hoc sensitivity row. Not all 24 failures are attributable to the missing name. t10 gives no evidence about resuming a cut program | agreed | measured | A corrected task in a new study |
| X1-3 | Output budget, as frozen in the grader: the provider's aggregate output count, reasoning included once, must be at most 4,096; a missing, invalid, or conflicting count fails eligibility and is not zero. Raw functional success is kept separately. Generation caps were asymmetric between families. Observed: six Haiku trials exceeded the limit, all already failed | agreed | measured | None |
| X1-4 | Future task sets are checked, before fixtures are frozen, to state every entry name and type a complete program needs | agreed | argued from the measured defect in X1-2 | None |
| X6-1 | Model readers: Haiku 200/200 in every surface; Luna D0 178, K 180, P 183, B 187 of 200, five sessions per surface. Luna's errors fall on forced-result and underdetermined-result items, with some allowed results judged forbidden | agreed | measured | None |
| X6-2 | The pilot establishes no reliable surface effect on reading and no reading equivalence. Haiku is at ceiling; Luna's spread is inconclusive | agreed | argued from measured | A harder reading test |
| X6-3 | Hannah's review (one reader, each contract once, two per surface): 38/40 correct; B 10/10, K 10/10, D0 9/10, P 9/10, in that order of presentation; 48 minutes of recorded section time. Different items per surface confound any surface comparison, so no general human surface effect is identified. Her ranking of one contract (D0, K, B, P) and her comments are preference evidence, kept apart from correctness. The original X6 collection is closed | agreed as a statement of the data; the general human effect stays unresolved | measured (one reader) | A human study designed to compare surfaces |
| X-arms | Arms X2 to X5 keep their registered D0 and K matrix; a later study that changes the controls is a new study. The repair arm is deferred pending its own freeze of feedback, context, budget, denominator, and treatment of t10 | deferred | none | See C5 to C8 and C10 |
| X6-4 | The force of an `example` clause (it binds the result for that exact input) must be stated in the card and by tooling. Hannah used the example in c4 inconsistently: she answered q1 correctly (the example's own result is the only one allowed) and q2 wrongly (a different result for the same input judged allowed), commenting that the example read as a hint. Luna answered both correctly in all sessions, so this is evidence from one human, not a replicated error | agreed | measured (one reader); argued for the remedy | A reader test of the reworded card |
| X1b-1 | Follow-up pilot X1b ran as registered: K, K2 (no section colons; `takes {…}`, `fields {…}`, case fields in braces; construction stays a labeled call), K3 (K2 with no separator commas), tasks t1 to t9, 3 runs, 2 families, 162 trials, fresh K controls, separate freeze, seed 2026111502. Not a held-out confirmation: the tasks were known and the treatments were proposed after X1 and Hannah's comments | agreed | measured | None |
| X1b-2 | Primary result: Haiku K 27, K2 26, K3 26 of 27; Luna K 21, K2 22, K3 24 of 27. No challenger is eligible under the registered rule, so K is retained | agreed | measured (pilot) | X1b-3 |
| X1b-3 | Cap sensitivity: Haiku's one K3 failure is a correct t8 program at 5,212 output tokens, over the 4,096 analysis limit. Raw functional success was a prespecified secondary endpoint: K3 is 27 and 24 on it. Applying the selection rule to that endpoint would select K3; that use of the rule is post hoc and does not change X1b-2. It is reported as budget sensitivity, not as a syntax loss for K3 | agreed | measured | A confirmation of K against K3 on held-out tasks with the budget rule fixed in advance. The owner chose K3 without that confirmation (X1b-8), so it remains a reopening trigger, not a scheduled study |
| X1b-4 | K2 against K is a measured crossover: one trial higher on Luna (22 against 21), one lower on Haiku (26 against 27). K2 is not eligible under the rule. Colon-free sections remain a question of human readability and preference, not settled by X1b | agreed | measured (pilot) | A human reading comparison, or the K3 confirmation |
| X1b-5 | Brace construction of values (`item {name: x}`) was considered and withdrawn by the owner before measurement, and independently on the argument that one application syntax is simpler. It has no measured result. Rule adopted for K2 and K3: braces declare a shape, parentheses apply | default (argued, owner input) | argued | A proposal for structural record or tuple values |
| X1b-6 | Owner proposal on literal shapes: braces for fixed-shape products, brackets for homogeneous collections, a colon for a pair, no commas. The braces, brackets, and colon roles are a candidate direction for future shapes. At the time of X1b the no-comma part was not adopted, K with commas being retained by the rule (X1b-2); the owner later adopted it (X1b-8). Structural tuple types, anonymous record types and values, and dictionaries are new semantics and are deferred; they can coexist with nominal types. `[]` is only a list. Type application stays `list(T)` as a consistency default, with no evidence against `(list T)` | candidate direction; deferred for the new types | owner, argued | The library gate, the first program that needs a tuple or a dictionary, or the K3 confirmation for separators |
| X1b-7 | Naming questions from Hannah's review, each kept at its current spelling as a default: `wraps` and `.value` for wrapped types; `result` in `ensures`; `example call = value`; `text`; kebab-case names. Her comments are qualitative evidence that these read as arbitrary to one reader; there is no comparative evidence for any alternative. Integer division truncates toward zero and that must be visible wherever a contract uses it | default; open | owner (qualitative) | A naming pass before the spec is frozen |
| X1b-8 | Hannah chose K3 as the surface on 2026-11-15, after seeing X1b. This is an owner decision, not the outcome of the registered rule, which retains K (X1b-2). What supports it: K3 scored highest on Luna (24 against 21) and tied K on Haiku for raw functional success (27); what it sets aside: K3's one Haiku trial over the output budget, and the absence of a held-out confirmation. The no-comma part of her literal-shape proposal (X1b-6) is therefore adopted for calls, lists, and pair lists | owner decision | owner; measured (pilot, post hoc reading) | As C1 |

## R. Amendments of 2026-11-16 (carry-over pass)

Proposed by Claude from the owner's rulings and the round-4 agreements;
**awaiting Codex's confirmation**. Each row replaces the named parts of
earlier rows; the earlier text stays above as history. The detailed
mapping is `k3-carryover.md`. None of this syntax is measured: X1 and
X1b tested an earlier, smaller card.

| # | Amends | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- | --- |
| A-1 | C1 | Braces and brackets appear in expressions: braces for tuple and record values, types, and patterns; brackets for lists and dicts. Parentheses only apply; there are no grouping parentheses | owner decision | owner; argued | A parser or reader problem traced to a brace context |
| A-2 | C1, X1b-7 | Naming pass done: PascalCase types including builtins, camelCase values, `Str`, sized numbers only, `returns name: Type`, wrapper as `type Name of T` unwrapped by pattern. Identifier grammar `[A-Za-z][A-Za-z0-9]*`; atoms keep hyphens. A dotted name is a path or a field read | owner decision; agreed for the grammar | owner; argued | A wire tag must change, or path and field resolution collide |
| A-3 | B5 | Declaration kinds: `import`, `type`, `interface`, `fn`, `proc`, `let` (module value), `test`; `impl` is nested only. There is no module declaration | agreed | owner (tests kept); argued | G3 or E6 |
| A-4 | D1, D7 | `type Name of TypeExpr`: wrappers, records, tuples, and `case` variants, all nominal. An invariant is `where predicate`, a pure `Fn({Representation} Bool)`, checked on every ingress. `representation @private` hides constructor and pattern | agreed; the metadata is an addition | owner; argued | A refinement the predicate form cannot state |
| A-5 | C4, C6, C7, C8, X1b-5, X1b-6 | `let pattern [: Type] be expr [else …] endlet`; no `=` in the language; annotation optional and, when written, an expected type. Patterns in `let`, parameters, and `match`. Constructor labels exactly when the shape has field names. `labelled {name: Type be constant}` parameters kept; variadics cut. Anonymous `fn` and `proc` kept. Tuples, records, and dicts are in. Operands evaluate in written order. No call-site type arguments, no `as` expression, no tuple index; unions and anonymous enums deferred. `do … enddo` kept as an expression block | owner decision for `let`, patterns, braces; agreed for the rest | owner; argued | The closer pilot (X-K3c); first call that needs explicit type arguments |
| A-6 | E1, X6-4 | `example` leaves the contract. `doc` holds one Markdown string. Protected expectations are `test` declarations. `requires`, `ensures`, and `where` are run-time guards | owner decision; agreed | owner; argued | A reader test shows guards read as proofs |
| A-7 | F1, F3, F4, F6 | Both mechanisms: closed effect ceilings and capability values. `performs {DirWrite}` on declarations; `Proc({Str} Void {DirWrite})`, `Intent(Void {DirWrite})` in types; rows are sets of operation interfaces, exact, never inferred. Authority is a sealed value; interface values are kept; required grant is a parameter, optional is `Option`, narrowing is destructuring or a pure checked library function. Calling a `proc` builds a cold `Intent`; `await` runs it. Exact per-resource static identity, row polymorphism, subset subtyping, and revocation are not provided | owner decision for ceilings, spelling, interface values; agreed for the rest | owner; argued | The acceptance cases in `r4-required-optional-effects-codex.md` section 6 fail |
| A-8 | F2, B10, B11 | Entry is `proc main` taking its grants and returning `ExitCode` (a `U8` wrapper). Target data binds each parameter; a missing required binding fails the build, failed acquisition fails startup, optional refusal gives `None`. `vibra run` keeps exits 0–6 and reports the application code in `programResult`; a standalone build returns it | agreed | argued | An application code must reach the shell through `vibra run` |
| A-9 | I-section, J-section | Carried over from the current runtime: completed-call audit trace, deterministic test worlds, the 17-row host registry, saturating arithmetic with checked forms and a trap on division by zero, lazy once-per-run module values, canonical value encoding. Adapted: a call with a pending `ensures` is not a tail call; audit events record grants through a data projection under a new schema version | agreed | cited (spec 06); argued | Guarded recursion must be constant-space |
| A-10 | (method) | Carry-over rule: every construct of current Vibra has a disposition. Both inventories have stable ids; a script found 741 ids, 0 unmapped, 0 unknown references in `k3-carryover.md` on 2026-11-16 | agreed as method | measured (the coverage count only) | An inventory row is found wrong or missing |
| A-11 | D6 | Library rule: an algorithm that does not depend on the concrete type is written once, as a default interface member or a generic function. `Iter` (with `fold`), `Equatable`, `Ordered` (with `less … max`, `inRange`), `From`, `TryFrom`. Builtins declare `Iter` in nested impls; `Bytes` gains `Iter(U8)`. Lookup is by application, `items(0u64)`. `try` passes its expected type inward. Names: `List`, `Ordered`, `Bool.and` | owner rule; agreed | owner; cited (stdlib sources); argued | A real generic consumer for length, concat, or slice |
| X-K3c | X-arms | One confirmation pilot of the full K3 card, both families, frozen before any call; arms differ only in the ordinary `let` closer. Not yet preregistered | planned | none | Before the rewritten spec is called tested |

**Confirmed.** Codex reviewed A-1 to A-11 and X-K3c in
`r5-carryover-ledger-review-codex.md`: all accepted, with wording
qualifications to A-4, A-5, A-7, A-9, and A-11 that Claude accepts
(that file is the authoritative wording where it is more exact). The
rows below close the items that pass left open; both reviewers hold
them (`r5-native-how-codex.md`, `r5-roadmap-codex.md`).

**Precedence.** The effective design is the original rows as replaced
by later named amendments. Superseded text above is history, not a
second contract. One effective list is produced before the rewrite.

| # | Amends | Decision | Status | Support | Reopen when |
| --- | --- | --- | --- | --- | --- |
| A-12 | C1 | `if` and `match` are also expressions: in value position every normally completing arm ends in a final expression, the expected type flows into each arm, a value `if` needs `else`, and no common supertype is invented | agreed | argued | Golden cases show a context ambiguity |
| A-13 | B5 | Standard-library-only sections `role`, `native`, `external`, `symbol`, and the reserved type `Intrinsic(@i32)`; wire symbols are not renamed | agreed | argued | |
| A-14 | D1, A-4 | Anonymous enum types and values are deferred with unions. `where` is allowed on wrapper, tuple, and record representations only; an invariant across the cases of a variant is deferred | agreed (gate cut) | argued | The first invariant relating cases that payload invariants cannot state |
| A-15 | D7 | Nominal identity is a hash of a deterministic logical anchor: package or unit, module path, owner path, declaration kind, name, scheme version. A rename or move changes identity and is shown as such. Identity-preserving migration is future tooling | agreed | argued | Persisted values or a library upgrade need identity to survive a rename |
| A-16 | J4 | The interpreter stays permanently (reference semantics, tooling, compile-time evaluation of eligible pure functions). A native compiler and runtime are built through the Nexus pipeline (`docs/roadmap/nexus/`). Wasm is removed at native cutover | owner decision, 2026-11-16 | owner | The owner changes it |
| A-17 | I1, H5 | Memory: the Nexus context arena with precise non-atomic reference counting over acyclic immutable values, 4 GiB per context. First concurrency implementation: fixed pinned worker pool, one context per unit, messages copied. Neither is a measured performance claim | agreed | cited (Nexus N7, N22); argued | A leaked cycle, or worker tests fail the workload |
| A-18 | I4, E7 | The language has no fuel counter and no cost proof. The Nexus quota only suspends and resumes, and is unobservable. Tools set documented work, memory, and output limits on `eval`, tests, guards, and compile-time evaluation; exhaustion is an incomplete outcome, never a pass and never "guard false" | agreed | cited (Nexus N3); argued | A workload needs a guaranteed bound |
| A-19 | E1, D4 | A guard that returns false is the uncatchable trap `@runtime.guard-failed`, with guard kind, declaration, and origin. A guard that faults or exhausts keeps its own cause. `requires` and `ensures` of a `proc` run per `await`; `where` runs at construction | agreed | argued | |
| A-20 | F1, F4, F6, Nexus N26 | An `Intent` is a reusable immutable plan with the closure layout and a distinct type; each `await` is a fresh invocation. Grants are sealed values in one authority envelope with disjoint kinds (host resource, actor endpoint); the runtime checks rights, scope, world, and limits at host dispatch; accounting lives in a broker. Startup acquires bound grants in dependency order with reverse cleanup. The policy forms of a resource scope (Nexus O4) are still to be specified | agreed; O4 open | argued | A well-typed grant escape or a forbidden host action |
| A-21 | L1, L7, B1, B4 | Tooling starts from the existing command hierarchy and query vocabulary; no second `init`/`show`/`find` family. Files stay the source; a content-addressed store is at most a disposable cache; no stage or publish workflow. Minimum query, bounded eval, and revision-checked file edit belong to the language release; MCP and editor adapters follow in a named milestone | agreed | cited (spec 05, CLI source); argued | A client workflow needs more |
| A-22 | A2, N1 | Roadmap skeleton replacing v0.0.2 onward: v0.0.2 K3 language, capabilities, agent repair loop, first native slice; v0.0.3 full native profile on three targets, `vibra build`, Wasm removal, full gate 1; v0.0.4 libraries and bounded local concurrency; v0.0.5 services and selected recovery. Detail in `r5-roadmap-codex.md` section 3 | agreed | argued | The owner reorders it |
| A-23 | G5, H1 | **Conflict recorded, owner's to resolve.** Nexus O1 and O3 (owner, 2026-11-11): actors are receive loops and Vibra is actor-first. G5 and H1 (2026-11-15) chose scoped concurrency without having read that. Joint default until she answers: receive-loop actor kernel as substrate; scoped child and join plus bounded parallel map as the first user-facing library; long-lived actors with explicit lifetime rules | open (owner) | owner; argued | Her answer |

| A-24 | A-20, Nexus O4 | Resource-scope policy is closed, versioned, typed library data (`r5-scope-policy-codex.md`): whole correlated rules joined by `anyOf` and `allOf`; restriction keeps the parent as a conjunction, so no subset solver is needed; file scopes are a held root object, a component prefix, a depth, an action set, and an exact basename test; HTTP rules correlate origin, peers, methods, and target; limits are shared broker counters. Policy describes, a sealed grant authorizes; decoding data restores no authority. Stated limits: hard-link aliasing, authority follows objects not path spellings, no claim about what a remote server does. Merged amendments: an HTTP target prefix selector by components with exact-or-any query; no intermediate directory handle cached across invocations; hard-link limit stated in the consent text; a Windows adapter feasibility record in v0.0.2 M0 (an "unsupported" fallback is safe but cannot pass the CLI gate) | agreed | argued; measured for the finite algebra model only | A needed policy outside the selectors, or an adapter that cannot meet the handle contract |
| A-25 | (rule) | No dependency cycles, in any dependency graph the design defines: imports, packages, module-value initializers, startup recipes, build inputs. Per-graph rules in `r5-guards-static-codex.md` section 7. Two named exemptions, both told to the owner as assumptions: same-module recursion of functions, and recursive nominal type declarations (symbolic references, finite-size check kept, values stay acyclic). Run-time wait-for cycles between actors are a separate matter this rule does not cover | owner decision, 2026-11-16; exemptions agreed | owner; argued; local crate graph measured acyclic | The owner corrects an exemption |
| A-26 | A-19 (withdrawn), E1, E2, D4, A-9 | No guard traps; `@runtime.guard-failed` is not added. A **bare** `requires`, `ensures`, or `where` is a static obligation: the checker discharges it with a small deterministic rule set (constants, the value's own type invariant, a dominating branch on the same predicate, substitution, Boolean structure, listed integer facts, certified callee results) or rejects the code. A guard **with `else`** states its consequence (owner's idea): `requires C else V` returns V before the body; `ensures C else V` returns V in place of the candidate result (C is checked on the body's candidate; V is evaluated once with that candidate and not revalidated, so C is not promised of the final value); `where P else E` makes the constructor return `Result(T E-type)`, E a constant value. A bare `where` keeps the constructor returning T. A proven guard drops its check without changing any type. A callable whose every `requires` has an `else` converts freely to a function type. Static `proc` guards are discharged at plan construction; dynamic ones run per `await`. Predicates are limited to a certified pure, fault-free fragment. Plain trapping division and remainder are removed (checked forms only); an unobservable function value is an explicit error, not a trap. Gate claim: no trap triggered by user data in supported checked code. Detail: `r5-guards-static-codex.md` | owner direction; agreed | owner; argued; countermodels measured | Gate-1 programs show the rule set rejects too much |

Owner answers, 2026-11-16 (late):

- A-23: "Whatever you guys think is best." The joint default is
  therefore in force: receive-loop actor kernel as the substrate;
  scoped child and join plus a bounded parallel map as the first
  user-facing library; long-lived actors with explicit lifetime rules.
  Status: agreed (delegated by the owner).
- A-25: both exemptions confirmed. The owner's focus is no import
  cycles, Go style; the other per-graph rules stand as the agents'
  agreed extension.

- X-K3c (agreed 2026-11-16): the specification is written with
  `endlet` as the default. The pilot is a registered draft
  (`r5-k3c-task-card-draft-codex.md`), not frozen. No throwaway
  semantic evaluator is built: M1 validates the pure references and
  controls, and all 120 scored calls run after M2 against the real
  checker and test world, under one final freeze. Pilot execution
  moves from M0 to M2; the Windows scope feasibility record stays in
  M0.

Still open after this section: the final freeze of X-K3c at M2; the owner's go-ahead for the
rewrite.

## Guarantees explicitly given up for now

Rule 9 forbids a default that silently removes an advertised guarantee.
These are the ones removed on purpose:

1. Information-flow security (D5).
2. Static uniqueness and must-consume and must-reply protocols (D3).
3. A general proof facility for universal behavioral contracts, at
   gate 1 (E2, E6). Typing, exhaustiveness, and supported termination
   judgments stay.
4. Replay and durable recovery for every effect (A3, G1).
5. Exact static identity of the resource instance an opaque callable
   or intent may touch, and proof of least authority. The
   operation-family bound is restored by closed ceilings (A-7); this
   item was wider before 2026-11-16.
6. Actor-specific lifecycle guarantees (G3). Scoped task guarantees
   stay (H1).
7. Cost-bound guarantees (E7).
8. "Accepted pure code cannot fault." Retained primitive, refinement,
   and contract guards can fault; tool limits can stop an evaluation
   and memory exhaustion is a host outcome. A scheduler quota only
   yields (D4, E3, A-18, A-19).

Kept as design obligations, and not established by the X1 harness:
capability confinement, safe runtime behavior, and the static type
judgments.

## Dissent

None outstanding. Codex's round-4 amendments M1 to M10 (`r4-codex.md`)
are merged above; Claude accepted all of them, including the two new
proposals N3 and L9. Codex confirms or records remaining dissent by row
number after reading this merge.

Still open: the naming pass in X1b-7, the general human readability
effect (X6-3), and the mechanisms in G2, H5, I1, L8, and the identity
policy in D7. The owner's decision in X1b-3 is made: see X1b-8, which Codex
accepts as a reversible owner default with no dissent
(`r4-k3-owner-decision-codex.md`).
Rows X6-3 to X1b-7 were added by Claude after round 4; Codex
confirmed them with five amendments (`r4-x1b-ledger-review-codex.md`),
all merged.
