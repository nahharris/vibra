# Vibra v1 effects

Status: normative target
Implementation status: not started

## Static contract

An effect answers “what may this code attempt?” during checking. V1 effects are
static and erased; they are not runtime permissions, capability values, or a
sandbox.

- a function application whose performed row exceeds its enclosing effect
  ceiling is a type error;
- every host operation belongs to one nominal effect root; and
- a binary target's declared roots must contain the entry function's complete
  performed row before that target can run or build.

The runtime receives no grant table and performs no path-level effect check. If
a user runs a target that declares `fs.read`, that run consents to `fs.read`
operations for every path the program supplies. Operating-system access rules
and failures still apply, but they are not Vibra effect grants.

## Nominal effects

`deffect` introduces a nominal root and interface-like operations whose names
are qualified by that root:

```vibra
(deffect read
  visibility: @public
  (defn read.file (path path) (result bytes fs-error)
    external: @host
    symbol: "fs.read-file")
  (defn read.text (path path) (result str fs-error)
    external: @host
    symbol: "fs.read-text"))
```

When this declaration belongs to module `fs`, its source root is `fs.read` and
its operations are `fs.read.file` and `fs.read.text`. The defining package and
module, not an import alias, form the canonical identity.

Applying an operation as a function always performs its owner root. `effects:`
on an operation lists additive roots performed by its body; the owner root is
implicit. An omitted additive row is empty. Effect roots and operation names
are unique nominal declarations. Textually equal roots from different packages
are unrelated.

An `@host` external declaration binds an operation signature to the closed,
versioned host registry. Its string symbol selects one registry entry, its
exact type is checked, and that entry's owning effect MUST match the enclosing
operation. Pure low-level behavior uses an `@compiler` external declaration
with an empty effect row. Both providers are toolchain-owned; user packages
cannot declare external bindings, extend either registry, or use WebAssembly
FFI to bypass effect checking.

## Function effect rows

An effect row is a finite, duplicate-free set of atom references that resolve
to nominal effect roots.

- Omitted `effects:` is the empty row on every `defn`, `lambda`, function type,
  and test. There is no visibility or nesting exception.
- Every ordinary effectful `defn`, `lambda`, or test MUST write its complete
  ceiling.
- Every interface contract has a written or default-empty ceiling.
- An effect operation declares only its additive ceiling; its owner is
  implicit.
- The checker computes the least performed row over the resolved function-call
  graph, but this computation never changes an omitted ceiling from `()`.
- For an ordinary body, the written or default-empty ceiling MUST contain the
  performed row. For an effect operation, its owner root plus its additive
  written or default-empty ceiling MUST contain the performed row.
- An unused declared effect is a warning because it weakens local reasoning.
- An interface method MUST remain within its contract ceiling.

Rows are order-insensitive semantically and sorted by canonical identity in
formatter and machine output. A function type includes its closed effect row.
Effect variables and polymorphic rows are excluded from v1; a higher-order
function therefore declares the exact callback effect row it accepts.

Nominal constructor application, tuple and record projection,
array/map/string/byte lookup, and closed native collection construction are
pure applications. They add no root and no edge to the function-call graph.
Effects performed while evaluating their callee or operands still contribute
normally.

`main` follows the same rule as every other `defn`: omission means `()`, so an
effectful `main` MUST declare `effects:`. The target record independently
supplies the project's execution consent ceiling.

## Target consent

Every binary target in `project.vib` contains an `effects` array:

```vibra
(record
  name: @hello
  kind: @bin
  root: "src/hello"
  entry: "main.vib"
  effects: (array @std.fs.read @std.io.stdout))
```

Project resolution maps each atom from its target/dependency alias to one
canonical package/module effect identity. Unknown roots and duplicates are
project errors. An empty array permits only an effect-free entry graph.

Before `check`, `test`, `run`, or `build` accepts a binary target, the checker
MUST prove that the performed effect row of `main` is a subset of the target
array. It MUST also prove that the performed row fits the `main` declaration's
written or default-empty ceiling. These are static admission rules and the
target array is the user's blanket consent for the selected run. It does not
constrain paths, environment keys, output destinations, invocation counts, or
data volume within a declared root. V1 has no runtime narrowing, prompt, denial
result, or embedding API that silently adds roots.

An effectful test writes its complete `effects:` row. Selecting and running
that test is consent to those roots under the same blanket semantics. A pure
test may omit the row. Library targets are not execution entries and do not
declare a target effect array; their public function ceilings report their
requirements to consumers.

## V1 standard effect inventory

The v1 toolchain reserves these standard roots:

| Module | Root | Purpose |
| --- | --- | --- |
| `io` | `stdin`, `stdout`, `stderr` | Whole-value console input and output |
| `fs` | `read`, `write`, `metadata` | Filesystem value operations |
| `env` | `read` | Environment reads |
| `time` | `now` | Injected wall and monotonic clocks |
| `random` | `generate` | Injected random bytes |

Environment writes, sleep/timers, networking, processes, signals, streaming
handles, and async operations are post-v1. Convenience functions compose the
roots above instead of acquiring hidden effects.

Time and random values come from injected host providers. Tests use
deterministic providers unless a test harness deliberately supplies recorded
responses.

## Query metadata

Effects are part of the metadata returned by the shared workspace query
service, not a dedicated query kind. A function or source-position query can
include:

- written-or-default ceilings and computed performed rows;
- resolved function callees and classified non-call applications;
- effect-operation witnesses that introduced each root;
- the public boundary that covers the row; and
- the binary target or test ceiling checked for the selected entry.

Reports use canonical identities rather than import aliases. The checker and
query result MUST share one function-call graph and performed-row result.
Compact queries may omit call witnesses; `--expand effect-witnesses` requests
them without changing the underlying metadata or admission result.

## Claims and limits

V1 effects are not exceptions, algebraic handlers, resumptions, runtime values,
permission prompts, path filters, or security capabilities. Ordinary host
failures remain the typed errors declared by operations; an ABI or runtime
invariant failure is a trap.

The effect system guarantees static containment of performed roots in written
ceilings for source accepted by a conforming compiler. It does not make the
resulting program “host-safe,” constrain a declared root at runtime, prove the
compiler or host provider secure, or safely execute arbitrary foreign Wasm.
