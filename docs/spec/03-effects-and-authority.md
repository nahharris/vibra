# Vibra v1 effects and authority

Status: normative target
Implementation status: not started

## Two complementary systems

An effect answers “what may this code attempt?” at compile time. An authority
grant answers “what may this program do to this host resource?” at runtime.

Both are required:

- a call outside the caller's effect ceiling is a type error;
- a host operation without a matching grant is denied at runtime; and
- possessing a resource value does not bypass either check.

Effects are not a security mechanism without grants, and grants do not make an
ill-typed effectful call legal.

## Nominal effects

`deffect` introduces a nominal root and its operations:

```vibra
(deftype file (resource @fs-read-file)
  visibility: @public)

(deffect read
  visibility: @public
  (defn open ((path path)) (result file fs-error)
    effects: ()
    (host-op @fs-open-read path))
  (defn text ((path path)) (result str fs-error)
    effects: (stream.read)
    (with-resource (input (try (read.open path)))
      (stream.read.all input))))
```

When this declaration belongs to module `fs`, its source root is `fs.read` and
its operations are `fs.read.open` and `fs.read.text`. The defining package and
module, not an import alias, form the canonical identity.

Calling an operation always performs its owner root. `effects:` on an operation
lists additive roots performed by its body; the owner root is implicit. Effect
roots and operation names are unique nominal declarations. Textually equal
roots from different packages are unrelated.

`host-op` is a closed compiler form available only to toolchain-signed standard
library modules. Its atom selects an entry in the versioned host registry and
its exact type is checked. Pure low-level operations use the separate closed
`primitive` form and perform no effect. User packages cannot declare registry
entries, invoke unknown primitives, or use raw WebAssembly to bypass an effect.

## Function effect rows

An effect row is a finite, duplicate-free set of resolved effect roots.

- Public functions and all interface members declare a complete ceiling.
- Effect operations declare their additive ceiling.
- Private functions may omit `effects:`; the checker infers the least fixed
  point over their resolved call graph.
- A written ceiling MUST contain the inferred performed row.
- An unused declared effect is a warning because it weakens local reasoning.
- An implementation method MUST remain within its interface member ceiling.

Rows are order-insensitive semantically and sorted by canonical identity in
formatter and machine output. A function type includes its closed effect row.
Effect variables and polymorphic rows are excluded from v1; a higher-order
function therefore declares the exact callback effect row it accepts.

`main` is private to the program target and may infer its row. The project
manifest, not a repeated `main` annotation, defines root runtime authority.

## Runtime grants

Project execution starts with the grants in `project.vib`. Omission means an
empty set. A written grant begins with a project target/dependency alias plus a
module/effect path; project resolution turns that spelling into the canonical
package/module effect identity. It may include provider-defined resource
constraints:

```vibra
(authority
  (grant std/fs.read path-prefix: "./data")
  (grant std/io.stdout)
  (grant std/env.read key: "APP_MODE"))
```

Grant constraints have closed schemas owned by the standard host registry.
Unknown labels, malformed paths, unsupported wildcard syntax, and constraints
for a root that defines none are project errors. Relative filesystem prefixes
resolve against the canonical project root. Path checks occur after safe
normalization and before an operation is admitted.

The runtime validates every concrete host operation against the active grant
and its resource constraint. Entry checks based on a function's effect row are
an optimization and early diagnostic only; they MUST NOT replace the operation
check.

For a binary target, project checking requires every statically reachable
effect root to have a root-level grant. A missing root grant is `E-AUTH`, not a
warning. Resource constraints still require operation-time checking because
their concrete path or key may be computed dynamically. Library targets report
required roots but do not require project grants.

CLI and embedding configuration may narrow project grants but MUST NOT amplify
them silently. An embedding host may supply root grants only through the
versioned runtime API and must opt into that authority explicitly.

## V1 standard effect inventory

The v1 toolchain reserves these standard roots:

| Module | Root | Purpose |
| --- | --- | --- |
| `io` | `stdin`, `stdout`, `stderr` | Console streams |
| `fs` | `read`, `write`, `metadata` | Sandboxed filesystem access |
| `env` | `read` | Named environment reads |
| `time` | `now` | Injected wall and monotonic clocks |
| `random` | `generate` | Injected random bytes |
| `stream` | `read`, `write`, `manage` | Operations on scoped resources |

Environment writes, sleep/timers, networking, processes, signals, and async
operations are post-v1. Convenience functions compose the primitive roots
above instead of acquiring hidden authority.

Time and random values come from injected host providers. Tests receive
deterministic providers and no grant unless their test declaration and runner
configuration both allow it.

## Reporting

`vibra effects` and the shared query service emit, for every function:

- written and inferred effect rows;
- resolved callees;
- effect-operation witnesses that introduced each root;
- the public boundary that covers the row; and
- the project grants that would match or deny each host witness.

Reports use canonical identities rather than import aliases. The checker and
report MUST share one call graph and inference result.

## Non-goals and guarantees

V1 effects are not exceptions, algebraic handlers, resumptions, dynamic
permission prompts, or runtime values. A grant denial terminates the active
program with a stable authority result after resource cleanup and is not
catchable as an ordinary domain error. An operating-system error after grant
admission remains the typed error declared by the operation. The host records
both outcomes as distinct auditable events.

The effect system guarantees static containment of performed roots in written
ceilings. Runtime grants guarantee admission checks at registered host
boundaries. Neither claim is a proof that the compiler prevents attacks or
that a granted host provider is bug-free.
