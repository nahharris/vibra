# Milestone 4 — WebAssembly spine, static effects, and host operations

Execution plan: [M4 steps and implementation guides](execution.md).
Integration branch: `m4`, created from `main` after the M3 merge and the
pre-M4 changes.

Outcome: Vibra can implement useful local automation with explicit static
effect boundaries and ordinary typed host failures, and every executable
program behaves identically in the reference interpreter and WebAssembly.

Deliverables:

- unoptimized Wasm lowering from the shared typed IR for the complete pure
  language of M2 and M3, including mandatory tail calls, union discriminants in
  written member order, checked lookups, and `@compiler` externals lowered
  before emission;
- standard-library native implementations written once as toolchain-owned
  native code that the interpreter calls directly and a Wasm module reaches
  through a pure native import, with the body/native differential joined to the
  interpreter/Wasm harness;
- the instance-owned value arena and opaque-index discipline of the runtime
  chapter, with no value index or instance identity reaching typed IR or build
  output, so a later ABI can move values between instances;
- a specified arena reclamation rule in the runtime chapter, stating when a
  value index stops being live and its storage is released, so a long-running
  tail-recursive program runs in bounded host memory rather than growing the
  arena monotonically; reclamation is unobservable apart from memory use, and
  index IDs are still never reused within an instance;
- an interpreter/Wasm differential harness over the entire executable corpus,
  reached through the conformance harness rather than `vibra build`;
- native `deffect` operations, default-empty ceilings, transitively closed
  performed-row checking through effect-operation additive rows,
  imported-symbol effect references, and effect metadata in general queries;
- required binary-target effect arrays and static admission of the resolved
  entry declaration;
- typed `@host` external declarations and the closed `vibra_v1` host registry,
  implemented in the interpreter and the Wasm backend together, with a
  specified rule for whether adding a registry entry is a registry minor
  version and how a build records the registry version it requires;
- whole-value console, filesystem, environment-read, clock, and random
  operations, and the program arguments of `vibra run [TARGET] -- ARG...` that
  `env.read.args` returns;
- a deterministic in-memory host world, one VIBON schema, for `vibra test` and
  the conformance harness, and ordered audit traces, with one
  versioned audit-event encoding shared by both backends that tolerates new
  event kinds, because later lines replay scheduling and delivery through it;
- effect rows in typed IR, query schemas, and build metadata that do not assume
  a row is always a closed literal set, so row variables stay an additive
  change;
- source-position metadata showing performed, enclosing, and target effect rows;
- the forms [M3 reassigned](../m3/exit-evidence.md#reassigned-to-later-milestones):
  an abstract contract member with its own generic parameters, labelled
  operands and written `types:` arguments on a contract member call, a dict
  variadic tail on a contract member, and a command result and exit code for
  an entry that returns `err`;
- a specified outcome for deep non-tail recursion, written into the runtime
  chapter before the Wasm backend is implemented: activations are held in the
  instance arena so depth is bounded only by memory, no backend consumes host
  stack per activation, and exhausting memory is the host event
  `@runtime.memory-exhausted`, which replaces the rule that host-stack
  exhaustion is a host event outside interpreter/Wasm parity;
- effectful iteration examples that walk with `iter.next` and tail recursion;
  and
- a recorded interpreter and unoptimized-Wasm performance baseline on the
  conformance and demo programs.

Delivery order: the Wasm spine lands first, with no `@host` import, so the
`@host` ABI is designed against both consumers of the typed IR and the parity
this milestone promises has a backend to measure. The arena reclamation rule
and the deep non-tail recursion outcome are specification prerequisites of the
spine: each is written into the runtime chapter before the arena or the Wasm
lowering is implemented. Stage demo: the M3 demo
library's tests produce identical results in both backends, including a deep
tail-recursive walk that must grow neither stack nor arena. Effects and host operations
follow, each landing in both backends in the same change.

Demo gate: a target that declares `@std.fs.read` reads a path chosen at runtime,
a program argument that `vibra run [TARGET] -- ARG...` supplies through
`env.read.args`, transforms the content, and writes to stdout under a declared
output root. The target therefore declares `@std.env.read` and `@std.io.stdout`
beside `@std.fs.read`. The
same source is rejected when its function or target ceiling omits a performed
root. No path grant, prompt, or runtime effect check participates. The accepted
program produces byte-identical stdout and audit traces in both backends.

Exit gate:

- an additive root declared on an effect operation reaches the performed row,
  the rejection diagnostic, and the target array of every transitive caller,
  and `@effect.outside-ceiling` names the call witness for each missing root;
- every host operation has one registry entry, owner effect, typed signature,
  and audit-event shape;
- no compiler-generated ambient host read exists outside the registry;
- success, typed host error, propagation, and trap have interpreter/Wasm
  parity;
- deep non-tail recursion has the same specified outcome in both backends;
- a tail-recursive loop over many iterations that allocates a fresh compound
  value per iteration holds a bounded live arena size in the Wasm backend; and
- interpreter-v1 conformance passes for the full language, and the Wasm
  backend matches it on every executable case.

This milestone does not claim that effectful programs are host-safe. Runtime
grants, path constraints, budgets, and scoped host handles are outside the specified M4 surface.
