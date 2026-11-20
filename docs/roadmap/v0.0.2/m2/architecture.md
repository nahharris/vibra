# v0.0.2 M2: architecture

The design record of M2. The [step table](README.md#step-table) and the
guides under [steps/](steps/) implement it; the
[specification questions](spec-questions.md) list what it assumes. The
specification is the authority: where this record and a chapter disagree,
the chapter wins and the disagreement is a finding.

## 1. Where each mechanism lives

### Crates and edges

M2 adds no crate. It adds two dependency edges, both in step 01:
`vibra-host` → `vibra-fs-scope`, and `vibra-workspace` → `vibra-host`.

```text
vibra-diagnostics ← vibra-syntax ← vibra-ir ← vibra-interp ← vibra-types
                                      ↑                           ↑
                  vibra-fs-scope ← vibra-host ←─────────── vibra-workspace
                                                                  ↑
                                                  vibra-cli, vibra-conformance
```

`vibra-workspace` re-exports the host types its callers name (`HostWorld`,
`AuditTrace`, `StartupRecord`), so `vibra-cli` and `vibra-conformance` gain
no edge.

`vibra-interp` does not depend on `vibra-host`, and `vibra-types` reaches
neither the broker nor a driver: compile-time evaluation runs with no host
context, which is how "pure evaluation has no provider at all" is kept. The
interpreter and the broker meet at one trait, `HostContext`, declared in
`vibra-ir`, which both already depend on. `vibra-workspace` is the only
crate that holds an interpreter and a broker at once.

| Mechanism | Crate and module | Step |
| --- | --- | --- |
| Row syntax, resolution, duplicates | `vibra-types::check::effects::rows` | 08 |
| Named `proc`, plan types, `await` typing | `vibra-types::check::effects::procs` | 05 |
| Operation interfaces, member rows, `impl` rows | `vibra-types::check::effects::members` | 08 |
| Performed-row analysis, witnesses, unused roots | `vibra-types::check::effects::analysis` | 09 |
| `Proc` and `Intent` types with rows, proc values, anonymous `proc` | `vibra-types::check::effects::values` | 10 |
| Grant types: sealedness, no encoding, shapes | `vibra-types::check::effects::grants` | 22 |
| The entry shape of a target | `vibra-types::check::effects::entry` | 25 |
| `Await`, host bodies, row re-validation | `vibra-ir::effects` | 03 |
| The dispatch trait and its data | `vibra-ir::authority` | 03 |
| The origin table | `vibra-ir::origin` | 03 |
| Host registry major 2, test-world registry | `vibra-ir::external::host` | 02 |
| The three grant rows | `vibra-ir::external::compiler` | 02 |
| Plans, `await`, invocation ordinals | `vibra-interp::plan` | 06 |
| Grant values, host and narrowing dispatch | `vibra-interp::host` | 20 |
| `@scopePolicy.v1`, normalization, evaluation | `vibra-host::policy` | 14 |
| Path names and the reserved-name set | `vibra-host::path` | 14 |
| Recipe data, the provider table, recipe identity | `vibra-host::recipe` | 14 |
| Grant table, narrowing, counters, `Context::host` | `vibra-host::broker` | 15 |
| The driver trait and the gate | `vibra-host::driver` | 15 |
| `@hostWorld.v2` and the fake driver | `vibra-host::world` | 16 |
| `@auditTrace.v2`, projections | `vibra-host::trace` | 17 |
| The `TestWorld` rows | `vibra-host::test_world` | 21 |
| Startup order, the three outcomes, release | `vibra-host::startup` | 28 |
| Stream, environment, clock, random drivers | `vibra-host::system` | 30 |
| The filesystem driver | `vibra-host::system::fs`, `vibra-fs-scope` | 13, 31 |
| The eight library modules | `stdlib/src/std/` | 18, 19, 21 |
| Entry and admission ceiling of a target | `vibra-workspace::entry` | 25 |
| Recipe validation into a binding plan | `vibra-workspace::binding` | 26 |
| Local dependencies | `vibra-workspace::dependencies` | 27 |
| The run: admit, start, enter, map the outcome | `vibra-workspace::run` | 29 |
| The test runner's world | `vibra-workspace::test_runner` | 07, 21 |
| `vibra run` and its payload | `vibra-cli::run`, `vibra-schema` | 32 |
| `workspaceRun` | `vibra-conformance::workspace_run` | 29 |

### The checker

The M1 collisions were in four files that every checker step edited. Step 04
removes the cause before any feature lands: it adds the module tree
`check/effects/` and one call from each shared file into it, at the places
that today report `@tool.unavailable`.

| Shared file | The one call it gains | Replaces |
| --- | --- | --- |
| `check/declare.rs` | `effects::procs::header`, `effects::values::written_type` | the `unsupported` header and `NoType::Unsupported` |
| `check/interfaces.rs` | `effects::members::contract_member`, `effects::members::impl_member` | the two `performs`/`proc` rejections |
| `check/expr.rs` | `effects::procs::await_expr`, `effects::procs::apply`, `effects::values::anonymous` | `@effect.invalidAwait` for every `await`; the two anonymous-callable rejections |
| `check/lower.rs` | `effects::analysis::check_bodies`, `effects::procs::lower` | nothing: a new pass after bodies are typed |
| `check/testing.rs` | `effects::procs::test_header` | the `takes`/`performs` rejection |

Each hook is a function in a file that one later step owns. Until that step
lands the hook returns the `@tool.unavailable` the shared file reported
before, so step 04 changes no observable behaviour. After step 04 no M2
step owns `declare.rs`, `interfaces.rs`, `expr.rs`, `lower.rs`, or
`testing.rs`.

The checker computes rows over the typed program, never over syntax:

```rust
// vibra-types/src/check/effects/analysis.rs
/// The roots one body can perform, each with the `await` that first
/// contributes it and the call that `await` runs.
pub(crate) struct Performed {
    pub roots: BTreeMap<EntityRef, Witness>,
}
pub(crate) struct Witness {
    pub first_await: ByteSpan,
    pub via: Via, // Direct(callee), Callback(parameter), Member(interface)
}
```

A body's row is the union, over its `await` nodes, of the row of the
operand's `Intent` type. The row of a plan is in its type, so the analysis
needs no call graph to be sound: recursion closes because a named `proc`
contributes its written ceiling, not its computed row. The call graph is
used only to build the shortest witness chain of `@effect.outsideCeiling`,
with an explicit work list.

### The checked IR

`vibra-ir` already has `Row`, `ProcType`, and `IntentType`
(`crates/vibra-ir/src/types.rs`), and a call whose callee has a `Proc` type
already has an `Intent` type. Step 03 adds what is missing:

```rust
// vibra-ir/src/expr.rs
pub enum Expr {
    // …
    /// `await plan`: runs the plan and yields its result.
    Await { plan: Box<Node> },
}

// vibra-ir/src/module_set.rs
pub enum FunctionBody {
    Expr(Node),
    Intrinsic(CompilerIntrinsic),
    /// A member bound with `external @host`.
    Host(HostBinding),
}
pub enum HostBinding {
    Operation(HostOp),      // one of the 17 rows
    TestWorld(TestWorldOp), // one of the 6 rows
}
pub struct CheckedFunction {
    // …
    /// Whether the body runs in a procedure context: a `proc`, or a test.
    pub context: BodyContext, // Pure | Procedure
}
```

Building a plan is the existing `Expr::Call` with a callee of a `Proc`
type; there is no plan node. `Value` gains no variant: a grant and a plan
have no neutral value, so no constant, no input, and no decoded data can
hold one.

**The dispatch trait.** Its data is typed by the registry, not by neutral
values, so the broker never interprets a program's types:

```rust
// vibra-ir/src/authority.rs
/// An index into one context's grant table. It is not authority: the
/// broker validates it on every use.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GrantToken { context: u64, slot: u32, generation: u32 }

pub enum HostArgument<'v> {
    Text(&'v str), Bytes(&'v [u8]), Path(&'v [String]), Count(u64),
}
pub struct HostRequest<'v> {
    pub op: HostOp,
    pub grant: GrantToken,
    pub arguments: &'v [HostArgument<'v>],
    pub invocation: u64,
    pub parent: Option<u64>,
    pub origin: u64,
}
/// The complete `Result` of one operation, in registry terms.
pub enum HostReply {
    Ok(HostOk),          // Void | Text | Bytes | Names | Metadata | I64 | U64
    Err(HostErrorCase),  // the row's error type and one of its cases
}
pub enum NarrowRequest<'v> {
    Restrict { grant: GrantToken, policy: &'v Value },
    Sub { grant: GrantToken, prefix: &'v [String] },
    Limit { grant: GrantToken, limits: GrantLimits },
}
pub enum Narrowed { Grant(GrantToken), Refused(ScopeErrorCase) }
pub enum Issued {
    Grant(GrantToken),
    Bundle(Vec<(&'static str, GrantToken)>),
    Output { stdout: Vec<u8>, stderr: Vec<u8> },
}
/// A defect or a host event; never a typed refusal.
pub enum HostFault { InvalidHostValue(String), MemoryExhausted }

pub trait HostContext {
    fn host(&mut self, request: HostRequest<'_>) -> Result<HostReply, HostFault>;
    fn narrow(&mut self, request: NarrowRequest<'_>) -> Result<Narrowed, HostFault>;
    fn test_world(&mut self, op: TestWorldOp, world: GrantToken,
                  arguments: &[Value], invocation: u64)
                  -> Result<Issued, HostFault>;
}
```

A typed refusal (`denied`, `limitExceeded`, `unsupported`,
`outcomeUnknown`) is a `HostReply::Err`. `HostFault` is only the two things
that are not a program result.

### The interpreter

`vibra-interp` gains two value forms and one continuation, in two new
modules; `machine.rs` gains only the calls into them.

```rust
// vibra-interp/src/value.rs
pub(crate) enum Repr<'a> {
    // …
    /// A plan: the resolved callee and its evaluated operands.
    Plan(Rc<PlanVal<'a>>),
    /// A sealed grant.
    Grant(GrantToken),
}
// vibra-interp/src/machine.rs
pub(crate) enum Kont<'a> {
    // …
    /// The invocation that an `await` started: restored when it returns.
    Await { caller_invocation: u64 },
}
```

- Applying a callee of a `Proc` type allocates a `Plan` and returns. It
  enters no frame and runs no guard.
- `await` takes the next invocation ordinal, pushes `Kont::Await`, and
  enters the plan's callee exactly as a call enters a function: a
  `requires … else`, the body, and an `ensures … else` run on each one. A
  `return await` in tail position reuses the activation.
- A `FunctionBody::Host` is entered by building a `HostRequest` and calling
  `HostContext::host`. With no context (compile-time evaluation,
  `interpret`) it is `RuntimeError::Unavailable`.
- Dispatch on a receiver whose `Repr` is `Grant` selects the interface's
  host-bound member; on any other held value it selects that type's `impl`,
  as for every interface.
- Nothing recurses on the host stack: `await` is a continuation, and a
  chain of 100,000 nested awaits is a row of step 06.

```rust
// vibra-interp/src/lib.rs
pub fn run_entry(program: &CheckedProgram, grants: Vec<EntryArgument>,
                 host: &mut dyn HostContext, budget: MemoryBudget)
                 -> Result<Execution, RuntimeError>;
pub fn run_test(program: &CheckedProgram, world: Option<GrantToken>,
                host: Option<&mut dyn HostContext>, budget: MemoryBudget)
                -> Result<TestExecution, RuntimeError>;
pub enum EntryArgument {
    Grant(GrantToken), Absent, Record(Vec<(String, EntryArgument)>),
}
```

### The host

`vibra-host` is rebuilt around one broker and one driver trait.

```rust
// vibra-host/src/broker.rs
pub struct Context<'d> { /* grant table, counters, trace, driver */ }
impl<'d> Context<'d> {
    pub fn new(driver: &'d mut dyn Driver, std: StdlibIdentity,
               revision: String) -> Self;
    /// Issues a root grant. Only `startup` and `test_world` call it.
    pub(crate) fn issue(&mut self, root: RootGrant) -> GrantToken;
    pub fn trace(&self) -> &AuditTrace;
}
impl vibra_ir::authority::HostContext for Context<'_> { /* … */ }

// vibra-host/src/driver.rs
pub trait Driver {
    /// `None`: the platform has no such driver, `@tool.unavailable`.
    fn identity(&self, provider: Provider) -> Option<DriverIdentity>;
    fn acquire(&mut self, provider: Provider, resource: &Resolved) -> Acquire;
    fn release(&mut self, resource: ResourceId);
    /// Step 4 of a host call. `gate` answers what only the opened object
    /// decides, and reserves a size learned after opening.
    fn call(&mut self, resource: ResourceId, action: &Action<'_>,
            gate: &mut dyn Gate) -> Completion;
}
pub enum Acquire { Acquired(ResourceId), Refused, Failed(String) }
pub trait Gate {
    fn admit(&mut self, found: &Found<'_>) -> Result<(), Refusal>;
}
pub struct Found<'n> {
    pub real_names: &'n [String], // as the filesystem reports them
    pub action: FoundAction,      // Read | Replace | Create | List | Stat
    pub size: Option<u64>,        // bytes to reserve, when it is a read
    pub links: Option<u32>,
}
pub enum Completion {
    NotSubmitted(HostErrorCase), // known not submitted: no event
    Completed(HostReply),        // traced
}
```

`Context::host` is the five steps of the runtime chapter, in one function:
validate the token, derive the request, evaluate the whole ancestry's
policy and reserve every counter, call `Driver::call` with a gate, then
append the event. `vibra-host::startup` and `vibra-host::test_world` are
the only callers of `Context::issue`; `issue` is `pub(crate)`.

The recipe data both sides of the project boundary share:

```rust
// vibra-host/src/recipe.rs
pub struct BindingPlan { pub target: String, pub leaves: Vec<Leaf> }
pub struct Leaf {
    pub name: LeafName,          // "@logs.reader"
    pub interface: HostInterface, // one of the 17 owners
    pub optional: bool,
    pub provider: Provider,
    pub resource: Resource,
    pub policy: Option<Policy>,
    pub limits: Limits,
    pub after: Vec<LeafName>,
    pub recipe_identity: String,
    pub span: ByteSpan,          // the recipe, in the project file
}
impl BindingPlan {
    /// The provider table, the source-leaf rule, and the `after` relation.
    pub fn validate(&self) -> Result<(), Vec<BindingFault>>;
    /// The one startup order.
    pub fn order(&self) -> Vec<&Leaf>;
}
```

`vibra-workspace::binding` decodes a target's `grants` into a
`BindingPlan` and reports `BindingFault`s as diagnostics;
`vibra-host::startup` runs `validate` again before it acquires anything.

### The workspace and the tools

```rust
// vibra-workspace/src/run.rs
pub fn run_target(snapshot: &SourceSnapshot, target: Option<&str>,
                  launch: Launch<'_>, budget: MemoryBudget) -> RunOutcome;
pub enum Launch<'a> {
    World(&'a HostWorld),                  // conformance, tests of the tool
    System { arguments: Vec<OsString> },   // `vibra run`
}
pub struct RunOutcome {
    pub result: CommandResult,             // the `@command.*` of the run
    pub application_code: Option<u8>,
    pub program_result: Option<String>,
    pub startup: StartupRecord,
    pub trace: AuditTrace,
    pub trap: Option<TrapRecord>,
    pub diagnostics: Vec<Diagnostic>,
    pub stdout: Vec<u8>, pub stderr: Vec<u8>,
}
```

`Launch::System` is constructed in `vibra-cli::run` and nowhere else; the
test runner constructs only a world. `run_target` has one exit path, which
releases the acquired grants in reverse order before it returns.

## 2. The authority argument

For every invariant of the effects chapter: the layer that enforces it, the
layer that checks it again, the attack cases, and what would have to be
wrong for it to fail. An `ATK-` case is a host test; a `V2-` case is in the
corpus. The step in brackets owns the case.

| # | Invariant | Enforced by | Re-validated by | Attack cases | It fails only if |
| --- | --- | --- | --- | --- | --- |
| A1 | A body's computed row is contained in its written ceiling | `check::effects::analysis` | `vibra-ir::effects`, at `CheckedModuleSet::try_new`, recomputing every body's row from the `Intent` types of its awaits | `ATK-ir-await-outside-row` [03], `V2-EFFECT-outside-ceiling-direct` [09], `V2-EFFECT-outside-ceiling-callback` [09] | both the checker and the IR validator compute the same wrong row |
| A2 | `await` runs only in a `proc` or a test | `check::effects::procs` | `vibra-ir::effects` (the function's `BodyContext`); the interpreter refuses `Await` in a pure frame as `@runtime.invalidCheckedProgram` | `ATK-ir-await-in-fn` [03], `ATK-interp-await-in-pure-frame` [06], `V2-EFFECT-await-in-anon-fn` [05] | three layers agree on a wrong context |
| A3 | Callable types compare rows exactly | type equality in `vibra-types` | `vibra-ir` type equality at every call, conversion, and member comparison | `ATK-ir-row-widened-at-call` [03], `V2-EFFECT-callback-row-wider` [10], `V2-EFFECT-callback-row-narrower` [10] | `Row` equality is wrong in `vibra-ir` |
| A4 | Building a plan runs nothing and consumes nothing | `vibra-interp::plan` | the trace and the counters: a built and dropped plan leaves no event and no charge | `V2-RUNTIME-plan-built-not-run` [07], `ATK-broker-no-charge-without-await` [20] | the interpreter enters a callee at application |
| A5 | A host action needs a grant, and the whole policy is enforced | `Context::host`, steps 1 to 3 | the driver's gate at step 4, on the opened object | `ATK-policy-ancestor-dropped` [15], `V2-GRANT-denied-outside-policy` [22], `ATK-fs-alias-name` [31] | the broker and the adapter both admit the request |
| A6 | Missing authority never falls back to ambient access | no driver is reachable except through `Driver::call` | the ambient-reads architecture test: `std::fs`, `std::env`, and the standard streams are named only under `vibra-host/src/system/` | `ATK-ambient-read-outside-system` [15], `ATK-ambient-read-in-interp` [20] | a crate reads the process directly and the architecture test misses it |
| A7 | A denial, an unsupported action, and an exhausted limit are values | `Context::host` returns `HostReply::Err` | the interpreter has no path from a reply to a trap; conformance compares the typed result | `V2-GRANT-denied-is-value` [22], `V2-GRANT-limit-exceeded-is-value` [22], `ATK-driver-error-not-fault` [16] | a driver returns a `HostFault` for an ordinary failure |
| A8 | No source expression makes a grant | `check::effects::grants`: an interface value type has no constructor, and widening a model makes no token | `vibra-ir`: `Value` has no grant form; the interpreter makes a `Repr::Grant` only from an `Issued` or a `Narrowed`; the broker validates every token | `V2-GRANT-no-constructor` [22], `V2-GRANT-model-is-not-grant` [22], `ATK-forged-token` [15], `ATK-ir-grant-constant` [03] | the broker accepts a token it did not issue |
| A9 | A grant has no encoding | `check::effects::grants` (`@type.functionNotEquatable`, dict keys) | `vibra-ir::constant`; the interpreter's encoder, `@tool.unsupportedValue` | `V2-GRANT-assert-equal-grant` [22], `V2-GRANT-dict-key-grant` [22], `V2-GRANT-any-holding-grant` [22] | the encoder invents a value for a token |
| A10 | A foreign or released grant is unusable | `Context::host`, step 1: context identity and generation | `Driver::call` receives a `ResourceId` the context resolved, never a token | `ATK-foreign-grant` [15], `ATK-stale-grant` [15], `ATK-released-grant` [28] | generations are reused |
| A11 | Implementing an operation interface makes no grant | dispatch: only `Repr::Grant` reaches a host body | the broker never receives a held model; an `impl` body is under A1 | `V2-GRANT-model-is-not-grant` [22], `V2-EFFECT-impl-forwards-wider` [08] | dispatch sends a model to a host body |
| A12 | Only the embedded library binds a host symbol, with the registry's owner and signature | the reader (`@syntax.invalidSection`) and the library loader | `vibra-ir::effects`: a `Host` body is valid only on the member the registry row names, with its signature and the row `{owner}` | `V2-SRC-DECL-fn-external-outside-stdlib` (M1), `ATK-ir-host-body-wrong-owner` [03], `ATK-stdlib-host-signature` [19] | the registry itself is wrong; the snapshot test guards that |
| A13 | Narrowing never adds a right, a resource, a driver, or a budget; the parent stays a conjunction | `vibra-host::broker`: a child is its parent plus one constraint, and no operation replaces a parent | `Context::host` walks the whole ancestry on every call; nothing caches an "effective" decision | `ATK-narrow-broader-policy` [15], `ATK-narrow-other-resource` [15], `V2-GRANT-restrict-keeps-parent` [22] | the ancestry walk stops early |
| A14 | Copies and descendants share ancestor counters; a counter that would overflow denies; an unknown outcome is not refunded | `vibra-host::broker` counters | property test over random narrowing trees against a model | `ATK-counter-overflow` [15], `ATK-counter-refund-unknown` [15], `V2-GRANT-limit-siblings-share` [22] | reservation is not atomic across the ancestry |
| A15 | The fields of one rule stay together | `vibra-host::policy::permits`, one rule at a time | the gate evaluates the same function on the real name and the found action | `ATK-policy-rule-fields-crossed` [14], `V2-GRANT-rule-correlation` [22] | evaluation distributes over fields |
| A16 | A policy over the size or depth limit is refused | `vibra-host::policy`, at decode and at `restrict` | `Context::host` never evaluates a policy that was not counted | `V2-GRANT-policy-4097-nodes` [26], `V2-GRANT-restrict-too-complex` [22] | the count is taken before normalization only |
| A17 | An entry row and every startup row are inside the target's `effects` | `vibra-workspace::entry` | `vibra-host::startup::admit`, before any leaf is processed | `V2-EFFECT-entry-outside-ceiling` [25], `V2-EFFECT-startup-row-outside-ceiling` [26], `ATK-admit-plan-outside-ceiling` [28] | both compare against a different list |
| A18 | Every leaf has a valid recipe from a provider that issues its type; `after` is acyclic; a process read names its source leaf | `vibra-workspace::binding` | `BindingPlan::validate` in `vibra-host::startup` | `V2-GRANT-binding-missing` [26], `V2-GRANT-binding-cycle` [26], `ATK-plan-provider-mismatch` [28] | the provider table differs between the two callers; it is one table |
| A19 | Startup follows the one order; a refused required leaf and every failure stop it; a failure is never an absence | `vibra-host::startup` | the `@startup` events and the startup record state each leaf's outcome | `V2-GRANT-startup-order-permuted` [28], `V2-GRANT-startup-failed-optional` [28], `ATK-startup-failure-not-none` [28] | an issuer's failure is mapped to `Refused` |
| A20 | On every ending the acquired grants are released in reverse order | the single exit path of `run_target` | a `Drop` guard on the startup state; the record's `released` statuses | `ATK-release-on-trap` [29], `ATK-release-on-exhaustion` [29], `V2-GRANT-release-order` [28] | a second exit path is added without the guard |
| A21 | A file request is resolved from the held directory, one name at a time, following no link | `vibra-fs-scope` | the gate checks the real names of every opened component | `ATK-fs-file-symlink` [13], `ATK-fs-junction` [13], `ATK-fs-swapped-junction` [13], `ATK-run-symlink-denied` [33] | the adapter opens by joined path |
| A22 | The name the filesystem reports decides a name test | `vibra-fs-scope` (`FileNormalizedNameInformation`) | the gate, with `Found::real_names` | `ATK-fs-case-alias` [13], `ATK-fs-short-name` [13], `ATK-run-alias-denied` [33] | the gate is given the requested name |
| A23 | Nothing is truncated or created before authorization; create is exclusive | `vibra-fs-scope`: open without truncation, then `replace_contents` | the gate is called between open and write | `ATK-fs-replace-no-truncate` [13], `ATK-fs-create-over-alias` [13], `ATK-run-denied-leaves-file` [33] | the driver writes before `Gate::admit` returns |
| A24 | No intermediate handle outlives an operation; authority follows the opened directory | `vibra-fs-scope`: every operation walks from the root handle | the renamed-root and replaced-root cases | `ATK-fs-root-renamed` [13], `ATK-run-root-replaced` [33] | the driver caches a child handle |
| A25 | Replacing a file with more than one link is `denied` | the gate, from `Found::links` | the adapter reports the count of the opened handle | `ATK-fs-hard-link-replace` [31], `ATK-run-hard-link-read-allowed` [33] | the adapter reports no count and the gate treats that as one |
| A26 | Where the platform cannot give the guarantees the result is `unsupported`, before acting | `vibra-fs-scope` (`ScopeError::Unsupported`), `Driver::identity` | admission refuses a provider with no driver as `@tool.unavailable` | `ATK-fs-no-normalized-name` [31], `ATK-driver-absent-at-admission` [30] | a fallback path is added |
| A27 | A test obtains grants only from its `TestWorld`, and reaches no real provider | the test runner builds a `Context` over a `WorldDriver` only | `Launch::System` is constructed only in `vibra-cli::run`; an architecture test fixes that | `V2-GRANT-test-without-world` [21], `ATK-test-runner-no-system-driver` [21] | the runner is given a system driver |
| A28 | A target cannot obtain a `TestWorld` | no provider issues it (`vibra-workspace::binding`) | `Context::test_world` requires a token of the test root, which only the runner issues | `V2-GRANT-entry-test-world-leaf` [26], `ATK-test-world-op-on-leaf` [21] | `startup` issues a test root |
| A29 | An `@once` operation is never dispatched twice for one invocation | the interpreter advances the continuation once per reply | the broker has no retry; one event per completed call | `V2-RUNTIME-await-twice-two-events` [20], `ATK-no-redispatch-on-error` [16] | a retry loop is added in a driver |
| A30 | A completed call is recorded once, in order; a refusal before the adapter records nothing; a projection restores nothing | `Context::host`, step 5 | `vibra-host::trace` has a decoder that yields data only | `V2-RUNTIME-trace-order` [21], `V2-GRANT-denied-no-event` [22], `ATK-trace-decode-grants-nothing` [17] | an event is appended before the reply is delivered |
| A31 | No operation terminates the process or returns `Never` | the registry signatures | no driver calls an exit function; an architecture test | `ATK-no-process-exit` [30] | a driver aborts |

### What the checked IR validates and what it trusts

The M1 table is extended. Nothing in the first column is "trusted from the
checker".

| Fact | Validated in `vibra-ir` | Trusted from the checker |
| --- | --- | --- |
| An `Await` operand is an `Intent`, and the node has its result type | yes | no |
| `Await` only in a body whose `BodyContext` is `Procedure` | yes | no |
| A body's row is contained in its ceiling | yes, recomputed | no |
| A row names operation interfaces only, each once | yes (`Row`, and the interface has a `proc` member) | no |
| An `impl` member's row equals its contract member's | yes, by type equality | no |
| A `Host` body is on the registry's member, with its signature and row | yes | no |
| No constant or neutral value holds a grant or a plan | yes, by construction | no |
| A type that contains a grant or callable is no dict key and no constant | yes (`constant.rs`) | no |
| The witness chain of an `@effect.outsideCeiling` | no: a diagnostic, not a property of an accepted program | — |
| The entry's leaves match the recipes | no: checked by `BindingPlan::validate` in `vibra-host` | — |

Grant sealedness, counters, and policy are never facts of the IR: they are
run-time state of the broker, which trusts neither the checker nor the
interpreter.

## 3. Fake and real drivers

**One seam.** `Driver` is the only trait between the broker and the world.
`WorldDriver` (`vibra-host::world`) implements it over a `@hostWorld.v2`
document; `SystemDriver` (`vibra-host::system`) implements it over the
process. Policy, counters, startup order, and the trace are in the broker,
above the seam, so both drivers are run by the same code.

**One case, two worlds.** Step 33 adds a harness that takes a case tree and
a world document, and runs the target twice:

1. under `Launch::World`, with the document;
2. under `Launch::System`, after writing the document's `files` and
   `directories` into a new temporary directory, with each `@fixedPath`
   re-rooted there, the document's `args` as the arguments, and the
   document's `env` passed as the variable set of the system driver's
   environment adapter.

It compares the application code, the captured streams, the startup
statuses, and the trace with `drivers`, each `driver`, and clock and random
results removed. Agreement on a case is sampled evidence for that case; the
harness report says so.

**Platform-specific parts.**

| Part | Windows | Linux, macOS |
| --- | --- | --- |
| Streams, environment, arguments, clocks, random | `std`, in `vibra-host::system` | the same code |
| `@host.fs.directory` | `vibra-fs-scope` (`NtCreateFile` from a held handle) | `Driver::identity` is `None`: a target that binds it is `@tool.unavailable` at admission |
| Link, junction, alias, and hard-link attack tests | run; each prints `NOT RUN` with a reason when the machine cannot make the object | compiled out with `#[cfg(windows)]` |
| The dual run | both halves | the world half only |

Every corpus case runs under a world, so the corpus is the same on the
three platforms and CI stays green there. The architecture test allows
`unsafe` only in `crates/vibra-fs-scope/src/windows.rs`.

## 4. The conformance corpus of M2

| Profile | Operation | What a case states |
| --- | --- | --- |
| `staticV2` | `workspaceCheck` | the verdict and each diagnostic with its span; a tree with a `project.vibon` |
| `staticV2` | `projectDecode` | the decode of one project file with `grants` |
| `interpreterV2` | `workspaceTest` | a `@testRun.v2` snapshot: each item's result and its complete `@auditTrace.v2` |
| `interpreterV2` | `workspaceRun` | the run observation of [Q17](spec-questions.md#q17-the-observation-of-a-workspacerun-case), with an optional `hostWorld` |

- **Prefixes.** `EFFECT` (rows, plans, `await`, ceilings), `GRANT` (grant
  values, narrowing, policies, bindings, startup), `PROJECT` (entry,
  dependencies, tests), `RUNTIME` (invocations, trace, outcomes),
  `CONTRACT` (`@contract.unusedEffect`, guards on procedures), `TOOL`
  (what stays unavailable). Layout:
  `conformance/cases/<PREFIX>/<id>/case.toml`.
- **Rules.** Each step owns one file, `conformance/rules/m2-NN.tsv`, which
  step 01 teaches the index to read beside `conformance/rules.tsv`. No two
  steps edit one rules file.
- **Worlds.** A case names at most one `@hostWorld.v2` document, outside
  its tree. A test case without one runs in the default world.
- **Traces.** A trace expectation is a whole document, written by hand
  from the chapter: the `revision` and the library `seed` are computed
  from the case's bytes by a script the case's author runs, never by the
  implementation under test.
- **Attack cases.** An attack the fake world can express is a corpus case
  (`V2-`). An attack that needs a link, an alias, a second process, or a
  forged in-memory value is a host test (`ATK-`) in the crate that owns
  the layer, listed in the table of section 2.
- **One fault.** A negative case has exactly one error. A row of a guide
  that cannot be written with one fault from forms that exist at that
  step is a finding against the guide.

## 5. Risks

1. **The checker and the IR validator share a blind spot in row
   computation.** Both read the row from `Intent` types. Mitigation: the IR
   validator is written from the chapter by a different step (03) before
   the checker's analysis (09) exists, with hand-built IR; review gate 11
   reads both against the chapter.
2. **The run observation has no specified format (Q17).** Every
   `workspaceRun` case waits on it. Mitigation: the run API and its host
   tests land first (29); the rows are written as fields; the corpus cases
   are added when the chapter names the document.
3. **`sub` semantics and the projected policy (Q7) change the broker's
   central data type.** Mitigation: step 15 keeps the ancestry as a list of
   constraints and computes the effective policy in one function, so the
   other ruling changes that function and its rows, not the grant table.
4. **The Windows adapter's residual gaps become escapes once it is behind
   a policy:** an alias is opened before it is refused; only NTFS on a
   local volume was measured. Mitigation: step 13 closes the measured gaps
   (listing, metadata, link count, the full device-name set), step 31
   returns `unsupported` for any volume that does not answer the name
   query, and gate 34 is an adversarial review of exactly these files.
5. **Host-stack recursion returns through `await`.** A deep chain of
   awaits, a deep policy, or a deep ancestry of grants could recurse.
   Mitigation: `Kont::Await`; policy evaluation and normalization with
   explicit stacks and the depth limit of 64; ancestry as a flat list; a
   100,000-deep row in steps 06 and 15, on a small-stack thread.
6. **A shared file reappears.** Mitigation: the ownership lists in every
   guide and the script that rejects two parallel steps with one file;
   step 04 for the checker; one rules file per step.
7. **Stream limits (Q10) cannot be made to fit "nothing was attempted".**
   Mitigation: the recommended answer is isolated in the stream adapters;
   the broker's reservation interface already carries a size learned after
   opening.
8. **Memory on the build machine.** Mitigation: no guide runs the
   whole-workspace test suite; each names the packages it touches and
   their dependents.
