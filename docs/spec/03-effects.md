# Vibra v1 effects

Status: normative target
Implementation status: empty effect ceilings and pure execution are admitted
through M3. Milestone 4 Step 13 closed the Stage 4B contracts of this chapter,
of the host registry, and of the host world in the runtime chapter. Milestone 4
Step 14 implements the static half of this chapter in both checkers, the typed
IR, and both backends: `deffect` roots and operations with Vibra bodies, effect
rows on every function, `lambda`, function type, interface contract member, and
test, performed rows over the function-call graph, `@effect.outside-ceiling`
with its witness, `@effect.invalid-reference`, `@contract.unused-effect` for a
written root, and the row in the canonical `fn` type encoding. Step 15 adds the
target `effects` array and effectful-test consent, and Step 16 the `@host`
externals and `@std.io`; the query and index metadata and the other host
modules remain unimplemented until Milestone 4 Steps 17 to 20.

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

### Executable effects before the host

Since Milestone 4 Step 14, checking and execution admit nonempty written
ceilings and nonempty performed rows on function types, `defn`, `lambda`,
tests, and effect operations. Rows are erased: neither backend consults one, and
an effect operation with a Vibra body is an ordinary function that both run as
such. An omitted row is empty as specified below. Since Step 16 a `@host`
external is executable in both backends, and since Step 15 a project target's
effect-consent array gates an effectful entry.

## Nominal effects

`deffect` introduces a nominal root and interface-like operations. Each
operation is declared with an unqualified name and is referenced through the
root that owns it:

```vibra
(deffect read
  visibility: @public
  (defn file (at path) (result bytes fs-error)
    visibility: @public
    external: @host
    symbol: "fs.read-file")
  (defn text (at path) (result str fs-error)
    visibility: @public
    external: @host
    symbol: "fs.read-text"))
```

When this declaration belongs to module `fs`, its source root is `fs.read` and
its operations are `fs.read.file` and `fs.read.text`. The defining package and
module, not an import alias, form the canonical identity. An operation is a
declaration with its own visibility: as for a `deftype` method, it is private
unless it writes `visibility: @public`, whatever the visibility of its root.
Another module applies an operation only when the operation is public, and names
a root in a row only when the root is public.

The parameter is spelled `at` and not `path` because a lexical binder may not
repeat a name the module declares (`@name.redeclaration`), and the module `fs`
declares the type `path` at its top level. The type, value, interface, and
effect namespaces are separate, but the flat top-level spelling rule and the
no-shadowing rule of the type chapter apply to every top-level spelling.

`effects:` on an operation lists additive roots performed by its body; the
owner root is implicit. An omitted additive row is empty. An operation's
complete row is its owner root together with its additive row, and applying
that operation as a function performs the complete row rather than the owner
root alone. A `@host` external operation has no additive row, so its complete
row is exactly its owner root.

`deffect` itself takes no `effects:` attribute. A root is a name, not a row,
and additive effects are declared per operation because different operations
of one root may reach different roots.

Effect roots and operation names are unique nominal declarations. Textually
equal roots from different packages are unrelated.

An `@host` external declaration binds an operation signature to the closed,
versioned host registry of the runtime chapter. Its string symbol selects one
registry entry, its exact type is checked, and that entry's owning effect MUST
match the enclosing operation. Pure low-level behavior uses an `@compiler`
external declaration with an empty effect row. Both providers are
toolchain-owned; user packages cannot declare external bindings, extend either
registry, or use WebAssembly FFI to bypass effect checking.

Three malformed `@host` declarations are `@syntax.invalid-attribute` at the
offending attribute: `external: @host` on anything but a member of a `deffect`,
an `effects:` row on a `@host` member (its additive row is always empty), and
`effects:` on a `deffect` itself. A `@host` member whose registry entry is owned
by a different root than its enclosing `deffect` is `@external.owner-mismatch`
at its `symbol:` string, relating the `deffect` name. A `@host` symbol that is
not in the registry is `@external.unknown-symbol`, and a declared signature that
differs from the entry's is `@type.type-argument-mismatch`, as for an
`@compiler` declaration. The three checks are
independent, so a member that is wrong in two ways is reported for both. Every
row's owner is a root of an embedded standard-library module, so a `@host` member
written in any other module is `@external.owner-mismatch`, which is how ordinary
packages cannot declare one.

## Function effect rows

An effect row is a finite, duplicate-free set of source symbols that resolve
in the effect namespace to nominal roots. An unqualified symbol resolves in
the current module. A dotted symbol begins with an explicit import alias, so
`io.stdout` resolves through `(import io @std.io)`. Source effect rows never
contain atoms. An atom entry or a symbol that does not resolve to one nominal
effect root emits `@effect.invalid-reference`. A root named twice in one row,
by one spelling or two, emits `@effect.duplicate-root` at the second entry and
relates the first.

- Omitted `effects:` is the empty row on every `defn`, `lambda`, function type,
  and test. There is no visibility or nesting exception.
- A `def` initializer cannot write `effects:`, so its ceiling is empty and its
  performed row must be empty. Effectful work belongs in a function.
- Every ordinary effectful `defn`, `lambda`, or test MUST write its complete
  ceiling.
- Every interface contract has a written or default-empty ceiling.
- An effect operation declares only its additive ceiling; its owner is
  implicit.
- The checker computes the least transitively closed performed row over the
  resolved function-call graph, but this computation never changes an omitted
  ceiling from `()`.
- For an ordinary body, the written or default-empty ceiling MUST contain the
  performed row. For an effect operation, its owner root plus its additive
  written or default-empty ceiling MUST contain the performed row.
- An unused declared effect is a warning because it weakens local reasoning.
- An interface method MUST remain within its contract ceiling. An
  implementation member writes exactly its contract member's row, since v1 has
  no row subtyping, and its performed row MUST be within that row.

Rows are order-insensitive semantically and sorted by canonical identity in
formatter and machine output. Source formatting preserves each lexical symbol
reference required by its imports while ordering the resolved roots. A
function type includes its closed effect row.
Effect variables and polymorphic rows are excluded from v1; a higher-order
function therefore declares the exact callback effect row it accepts. The
standard `iter` default methods `map` and `filter` accept callbacks with
`effects: ()` only.

**One representation of a row.** A row is a record of its roots, never a bare
set, so that a later row variable is one more field and not a new shape. In
typed IR it is `EffectRow { roots, tail: None }`, where `tail` is the absence of
a row variable; in VIBON it is `(record roots: (array @root...))`, and in JSON
it is `{"roots": [...]}`. The roots are canonical identities, sorted by their
UTF-8 bytes and without duplicates: atoms with `@` in VIBON and spellings
without `@` in JSON, as the identities of the index. A consumer MUST NOT
assume that a row has no other field, and MUST preserve a field it does not
know when it copies a row. The `fn` type encoding of the runtime chapter, the
type object of the position envelope, and the `effects` field of an index record
all carry this record.

**Calls and rows.** The **complete row** of a callee is the row that applying it
contributes to the body that applies it:

- a call of an ordinary module-level `defn` or nested method contributes the
  callee's performed row;
- a call of an effect operation contributes the operation's owner root together
  with its additive row, whatever its body performs;
- a call through a function value, whose callee is not known statically,
  contributes the row of the function value's type; and
- a call of an interface contract member contributes the contract's ceiling,
  always, and never the row of an implementation, because the implementation
  is chosen at run time or by a type that may change.

Creating a function value, such as naming a function, writing a `lambda`, or
using a contract member as a value, performs nothing; only applying it does. V1
has **no row subtyping**: two function types are equal only when their rows are
equal, so a function value is never accepted where a function type with a
different row is expected, in either direction. A caller that holds a function
of a smaller ceiling wraps it in a `lambda` that declares the row it must offer,
and an unused declared root is then the warning above.

Effectful iteration is written as a recursive module-level function that calls
`(iter.next it)` and declares its complete host ceiling explicitly. There is no
effectful higher-order iterator combinator in v1.

Nominal constructor application, tuple and record projection,
array/dict/string/byte lookup, and the `tupleof`, `recordof`, and `enumof`
forms are pure applications. They add
no root and no edge to the function-call graph.
Effects performed while evaluating their callee or operands still contribute
normally.

A target's entry declaration follows the same rule as every other `defn`:
omission means `()`, so an effectful entry MUST declare `effects:`. The target
record independently supplies the project's execution consent ceiling.

## Performed rows are transitively closed

The performed row of a body is the union of the complete rows of everything it
reaches through the function-call graph. A `let` value, a `let-else` value and
fallback, and a `return` operand are evaluated by the body that holds them, so
their rows join that body's row, and a `return` adds no call edge of its own.
An effect operation is an ordinary
node in that graph and contributes its complete row, so an additive root
declared on an operation appears in the performed row of every transitive
caller and MUST appear in each of their written ceilings.

Effect roots do not hide effects. There is no containment, dominance, or
sub-effect relation between two nominal roots, so naming an owner root never
discharges an additive root. Given module `app` with `(import fs @std.fs)`:

```vibra
(deffect audit
  visibility: @public
  (defn record (target fs.path event str) (result void fs.fs-error)
    visibility: @public
    effects: (fs.write)
    (fs.write.text target event)))

(defn handle-request (target fs.path summary str) (result void fs.fs-error)
  effects: (audit fs.write)
  (audit.record target summary))
```

Writing `effects: (audit)` alone on `handle-request` is
`@effect.outside-ceiling`, because its performed row contains `fs.write`. Row
containment is a plain subset test over canonical identities and is decided
without consulting any effect declaration.

This is deliberate. A row answers “what may this code attempt?”, so a root
that absorbed the roots of its own operations would let any package launder
`fs.write` or `io.stdout` behind a local name, and would let a target consent
array admit roots that appear in no source row of the program. An author who
needs to vary an implementation without exposing its row uses an interface
contract ceiling, which bounds every implementation while staying visible to
callers.

Because a caller's row grows when a library adds an additive root, widening an
operation's additive row is a breaking change for consumers. That cost is
accepted: it is the same observable break as adding a failure mode to a
declared result type, and v1 resolves dependencies from local paths or exact
Git revisions, so it can only surface on a deliberate revision bump.

### The witness of `@effect.outside-ceiling`

A function, `lambda`, operation, or test whose performed row exceeds its
ceiling emits **one diagnostic for each pair of the offending function and a
root missing from its ceiling**, in canonical order of root identity within the
function. A diagnostic reports a **witness**, the chain of calls that carries
the root from the offending body down to the declaration that introduces it:

- the **primary span** is the complete call site, in the offending body, that
  comes first in source order among the applications there whose callee's
  complete row contains the root;
- the **related spans** are the remaining hops, one for each, in order. A hop is
  a call site in the body of the previous hop's callee whose own callee's
  complete row contains the root, and the chain stops at a callee that
  introduces the root: an effect operation that owns it or lists it in its
  additive row without a deeper call that performs it, an application of a
  function value, or an application of a contract member. When the chain can
  continue through more than one call, it takes the shortest chain by number of
  hops, and among equal chains the one whose call sites come first in source
  order;
- each related span carries the note `performs <root> via <callee>`, where
  `<callee>` is the callee's canonical identity, or `a function value of type
  <type>` when the hop applies a function value, or `contract member
  <identity>` when it applies an interface contract member. The primary span
  carries the same note for the first hop; and
- every span carries its own source identity, since a hop may lie in another
  module or package.

For `handle-request` above, the diagnostic for the missing root `fs.write` has
the call `(audit.record target summary)` as its primary span and the call
`(fs.write.text target event)` in `audit.record` as its one related span, noted
`performs @std.fs.write via @std.fs.write.text`. The witness is a fact of the checked
program and not of a particular rendering: the diagnostic, `--expand
effect-witnesses` of a query, and the corpus all report the same chain. A root
that the offending ceiling already lists produces no diagnostic and no witness.

A body that applies a function value or a contract member whose row contains
the missing root has a one-hop witness and marks the hop as indirect, since no
declaration beyond the type's row or the contract's ceiling is known.

## Target consent

Every binary target in `project.vibon` contains an `effects` array:

```vibon
(record
  name: @hello
  kind: @bin
  root: "src/hello"
  entry: @hello.main.main
  effects: (array @std.fs.read @std.io.stdout))
```

Project resolution maps each atom from its target/dependency alias to one
canonical package/module effect identity. These atoms are entity references
because the typed `project.vibon` schema declares that role for this field;
other atoms in VIBON remain data values. An empty array permits only an
effect-free entry graph.

An atom is resolved exactly as the atom of an `entry:` value is: its first
component names a unit, the longest module prefix is taken, and the rest names
a declaration. The project document is a privileged referrer, so it creates no
import edge and may name a root that is not `@public`, and the reserved
`@tests` and `@std.builtin` paths are not nameable from it, as from any module
that is not a test. The atom is written in its canonical form, never through an
import alias, because `project.vibon` has no imports. The roots of a target
array are a set: the formatter writes them in the canonical order of their
identities, as it does for a written row, and the order in which an author lists
them has no meaning.

An atom that names nothing, or an entity of another kind, is rejected by the
ordinary resolution diagnostics at the atom's span, not by a project code of its
own: `@module.unknown-path` or `@name.unknown-symbol` when the path resolves to
no module or declaration, and `@name.wrong-entity-kind`, naming the entity found,
when it resolves to something that is not an effect root. A root listed twice is
`@effect.duplicate-root` at the second atom, relating the first, as in any
written row. A
binary target with no `effects` field is `@data.invalid-shape`, as the
projects chapter states for every missing required field.

Because performed rows are transitively closed, this array is the closure of
the entry graph. It MUST list every root that graph reaches, including a root
introduced only by the additive row of an effect operation and never written
in the entry. Source effect rows and the target array therefore range over one
vocabulary of canonical identities: when the entry declares no unused root, the
array is exactly the entry's row rewritten from import aliases to canonical
package/module atoms, and the toolchain derives both from one computed result.

Before `check`, `test`, `run`, or `build` accepts a binary target, the checker
MUST prove that the performed effect row of its entry declaration is a subset of
the target array. It MUST also prove that the performed row fits that
declaration's written or default-empty ceiling. These are static admission
rules and the target array is the user's blanket consent for the selected run.
It does not constrain paths, environment keys, output destinations, invocation
counts, or data volume within a declared root. V1 has no runtime narrowing,
prompt, denial result, or embedding API that silently adds roots.

A root in the target array that is absent from the entry's performed row weakens
local reasoning exactly as an unused root of a ceiling does, so it is the warning
`@contract.unused-effect` at that atom. A root of the performed row that the
array omits is `@effect.outside-ceiling`, whose primary span is the entry's
`entry:` reference in `project.vibon` and whose related spans are the witness of
that root from the entry declaration. The `entry:` value is not a call site, so
it is not itself a hop: every hop of the witness, the first included, is a
related span with the source identity of its own module, in the order of the
chain and noted `performs <root> via <callee>`, and the primary span carries no
note. One diagnostic is emitted for each omitted root, in the canonical order of
the roots. An array atom that failed to resolve names no root, so a root that
only that atom meant to name is omitted from the array and reported here as
well.

Admission and the ceiling of the entry are independent facts of one computed
row. An entry whose performed row exceeds its own written ceiling is reported at
its call sites by the rule of the witness section, and a root that is also
absent from the array is reported again at the `entry:` value, since the two
sets of consent are separate.

An effectful test writes its complete `effects:` row. Selecting and running
that test is consent to those roots under the same blanket semantics: the test
is checked against its own row like any ceiling and no target array takes part,
since a test is not the entry of a target. A pure test may omit the row. Library
targets are not execution entries and do not declare a target effect array;
their public function ceilings report their requirements to consumers.

## V1 standard effect inventory

The v1 toolchain reserves these standard roots, each declared by an embedded
standard-library module whose operations are the registry entries of the runtime
chapter:

| Module | Root | Operations | Purpose |
| --- | --- | --- | --- |
| `io` | `stdin` | `text`, `bytes` | Whole-value console input |
| `io` | `stdout`, `stderr` | `text`, `bytes` | Whole-value console output |
| `fs` | `read` | `file`, `text`, `entries` | Whole-value filesystem reads |
| `fs` | `write` | `file`, `text` | Whole-value filesystem writes |
| `fs` | `metadata` | `stat` | Filesystem metadata |
| `env` | `read` | `var`, `args` | Environment variables and program arguments |
| `time` | `now` | `wall`, `monotonic` | Injected wall and monotonic clocks |
| `random` | `generate` | `bytes` | Injected random bytes |

The owner root of an operation is `@std.<module>.<root>`, so `fs.read.file` is
owned by `@std.fs.read`. Append, remove, and create-directory are deferred to
later registry minors. Environment writes, sleep/timers, networking, processes,
signals, streaming handles, and async operations are post-v1. Convenience
functions compose the roots above instead of acquiring hidden effects.

V1 has no terminating source. No host operation, intrinsic, or standard
function has the result `never`, and none is added: an activation ends by
returning, a program ends by its entry returning or by a trap or host event of
the runtime chapter, and the charter excludes processes. The only expressions of
type `never` remain `return` and a call of a user function declared `never`.

Every operation returns `(result t e)`, including the clock and random
operations, so a provider that cannot supply a value reports `unavailable`
instead of trapping. The error types are closed enums with `void` payloads,
declared in the module beside their roots, and the runtime chapter fixes every
signature, error variant, and meaning.

Time and random values come from the host provider of the run. A `run` uses the
system provider. `vibra test` always uses a deterministic in-memory **host
world**, and a conformance case may supply one, as the runtime chapter's host
world section defines. A test that depends on a nondeterministic provider has no
way to name one.

## Query metadata

Effects are part of the metadata returned by the shared workspace query
service, not a dedicated query kind. A function or source-position query can
include:

- written-or-default ceilings and computed performed rows;
- resolved function callees and classified non-call applications;
- effect-operation witnesses that introduced each root, including a root that
  reached the queried function only through an operation's additive row;
- the public boundary that covers the row; and
- the binary target or test ceiling checked for the selected entry.

Reports use canonical identities rather than import aliases, and every row is
the record of **Function effect rows**. The checker and query result MUST share
one function-call graph and performed-row result. Compact queries omit witnesses;
`--expand effect-witnesses` requests them, with the structure of the witness
section above, without changing the underlying metadata or admission result. The
tooling chapter closes the fields of the position envelope and of the index.

## Claims and limits

V1 effects are not exceptions, algebraic handlers, resumptions, runtime values,
permission prompts, path filters, or security capabilities. Sealed effect
roots and a declared dominance or sub-effect relation between roots are
excluded and require a post-v1 specification; flat rows are that design with
an empty relation, so adding one later relaxes existing programs rather than
breaking them. Ordinary host failures remain the typed errors declared by
operations; an ABI or runtime invariant failure is a trap.

The effect system guarantees static containment of performed roots in written
ceilings for source accepted by a conforming compiler. It does not make the
resulting program “host-safe,” constrain a declared root at runtime, prove the
compiler or host provider secure, or safely execute arbitrary foreign Wasm.
