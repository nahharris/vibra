# Vibra type/effect-system planning handoff

> **Stale planning document.** This handoff assumes both the pre-cutover YAML
> source surface (superseded by the S-expression surface, see
> `docs/s-expression-migration-status.md`) and the capability/policy
> authority system (fully decommissioned; every host operation is now
> unconditionally available at runtime). Any future type/effect-system design
> building on capabilities/domains as described below needs to be redesigned
> from scratch around an authority-free host ABI, or dropped.

## Repository and implementation

- Repository: `nahharris/vibra`
- Host implementation: Rust 2021, minimum Rust `1.94.1`.
- CLI/compiler: `src/main.rs`.
- Semantic lowering and IR: `src/lower.rs`.
- Interpreter/runtime enforcement: `src/execute.rs`.
- WebAssembly backend: `src/wasm_backend.rs`.
- Closed, versioned host ABI registry: `src/host_abi.rs` and `schemas/host-abi.json`.
- Current language/design documents: `README.md`, `DRAFT.md`,
  `docs/wasm-abi.md`, and
  `docs/superpowers/specs/2026-07-15-capability-host-abi-design.md`.

Important direct dependencies include `serde`, `serde_json`, `toml`,
`quick-xml`, `wasm-encoder`, `wasmer`, `wasmer-wasix`, and `tokio`. Legacy
YAML dependencies remain during a breaking migration: `serde_yaml` and
`yaml-edit`.

## Current-state caveat: syntax migration

Vibra is intentionally undergoing a non-backwards-compatible migration from
a YAML source surface to a typed S-expression surface.

- Current production source/lowering code is largely YAML-shaped and uses
  `serde_yaml::Value`.
- The target language is a typed S-expression AST with source origins.
- New type/effect work should target the typed AST / semantic-IR boundary,
  not YAML mappings or generic `Value` trees.
- Do not introduce AST-to-YAML adapters.

The new syntax uses positional operands for required, ordered, evaluated input
and trailing `label: value` attributes for optional configuration:

```lisp
(def option
  (enum (some t) (none void))
  where: (t any)
  doc: "A value that may be absent.")
```

Generic type application is direct head application:

```lisp
(option int64)
```

It is not `(inst option int64)`. Runtime enum constructors remain distinct by
syntactic context.

## Useful repository map

```text
src/
  main.rs             CLI
  load.rs             legacy module loading/import graph
  lower.rs            legacy semantic lowering and core IR/types
  execute.rs          interpreter/runtime enforcement
  wasm_backend.rs     Wasm compilation
  host_abi.rs         closed versioned host-import registry
  async_runtime.rs    deterministic structured concurrency
  runtime/            host-side fs/io/WASI policy implementation
  project.rs          project/dependency validation
  test_runner.rs      Vibra test discovery/execution
  tooling.rs          formatter/linter diagnostics (legacy path)
  docs.rs             documentation extraction
  lsp.rs, mcp.rs      editor/agent protocols
  macro_expand.rs     legacy macro expansion

schemas/
  host-abi.json
  effects.schema.json
  type-expr.schema.json
  expression.schema.json
  module-surface.schema.json
  linter-codes.json

stdlib/               Git submodule with the Vibra standard library
examples/
tests/
DRAFT.md              language/specification draft
README.md             user-facing overview
Cargo.toml
```

The in-progress typed S-expression work introduces concepts such as:

```text
src/syntax/            lexer, parser/CST, spans, printer
src/ast/               typed surface AST and document-qualified origins
src/frontend.rs        multi-file S-expression module graph
src/typed_lower.rs     typed declaration/signature lowering
src/typed_body.rs      staged typed body lowering
```

## Existing semantic IR

`src/lower.rs` is the most important grounding file. It currently contains:

- `TypeRef`: primitives; records, tuples, arrays/maps/ranges; unions/enums/
  interfaces; aliases/generic instantiations; newtypes; references/mutability;
  policies; domain capabilities; opaque host handles; task handles; function
  types.
- `TypeAlias`: type definitions, type parameters, interface bounds, docs.
- `FunctionSig`: argument/return types, generic parameters/bounds, and Wasm or
  user body.
- `Expr`, `Statement`, `Pattern`, `Call`, and `RuntimeValue`.
- `LoweredProgram`: validated semantic program consumed by interpreter and
  Wasm backend.

Existing invariants include explicit signatures, no implicit numeric
widening/narrowing, nominal generic bounds, explicit newtype casts,
affine structured task handles, and intended interpreter/Wasm agreement.

## Existing capability system

Vibra already has authority/capability checking, but not yet a general inferred
effect system.

### Model

Three concepts are distinct:

1. `PolicyType`: aggregate root authority, normally injected at a root
   function argument.
2. `CapabilityType`: explicitly narrowed, domain-specific authority.
3. `HostHandle`: opaque runtime-minted resource authority, such as file,
   network, or process handles.

A root function declares a `$policy`. The runtime intersects that declaration
with explicit CLI approvals such as `--allow-read` and `--allow-write`. Code
must explicitly narrow it to a domain capability before using privileged APIs.

Legacy YAML example:

```yaml
main:
  $function:
    policy:
      $policy:
        fs-read:
        - requirement: mandatory
          scopes:
          - dir: ./tmp
        fs-write:
        - requirement: mandatory
          scopes:
          - dir: ./tmp
  return: $void
  do:
  - $let:
      read-capability:
        $policy.narrow: $args.policy
        into: $fs.read-capability
  - $let:
      opened:
        $fs.open-read: $path
        capability: $read-capability
```

The future S-expression form must preserve explicit attenuation rather than
turning permissions into ambient or implicit authority.

### Domains and scopes

`CapabilityDomain` currently includes:

```text
fs-read, fs-write,
stdin-read,
env-read, env-write,
net-connect, net-listen,
process-run,
clock, random, system-info
```

The core policy representation is approximately:

```rust
PolicyType {
  domains: BTreeMap<CapabilityDomain, Vec<PolicyGroup>>
}

PolicyGroup {
  requirement: Mandatory | Optional,
  scopes: Vec<PolicyScope>
}

PolicyScope = Any | File(String) | Dir(String) | Exact(String) | Prefix(String)
```

Scope coverage is domain-aware. Filesystem scopes use canonical paths and
ancestry; name/network-like domains use exact, prefix, or any matching as
appropriate. Narrowing is attenuation-only: a derived capability may never
exceed the root policy that produced it.

Runtime enforcement is primarily in `src/execute.rs`, `src/runtime/fs.rs`,
and `src/runtime/wasi_env.rs`.

### Static host boundary

`src/host_abi.rs` is the closed, versioned source of truth for host imports:

```rust
HostImport {
  module: "vibra_v1",
  name: "...",
  params: &[ParamKind],
  result: ValueKind,
}

ParamKind = Value(ValueKind) | Capability(&'static [CapabilityDomain])
```

The compiler validates Wasm wrappers against this registry: import existence,
ordinary parameter shape, capability parameter positions/domains, and exact
result type. A Wasm wrapper itself grants no authority. Runtime dispatch also
requires genuine runtime-minted capability values; casts and forged integers
cannot manufacture them.

Relevant diagnostics include:

```text
E-CAP-001  policies/capabilities/handles cannot be constructed or cast
E-CAP-002  host import missing or given the wrong domain capability
E-SEC-001  old grants syntax removed; use policy and explicit capability
```

### Existing effects report

`vibra effects <path>` statically reports reachable host imports and root
policies without executing code. Its schema is `schemas/effects.schema.json`:

```json
{
  "effects": [{
    "source": "...",
    "module": "vibra_v1",
    "name": "...",
    "params": [],
    "return": "...",
    "required-domains": ["fs-read"]
  }],
  "root-policy": {}
}
```

This is currently a reachable host-surface report, not a full language-level
effect inference system.

## Constraints for an effect-system plan

1. Preserve explicit authority values. Effect summaries must not grant
   permission.
2. Distinguish static effects, authority requirements, and runtime handle
   authority.
3. Compose function summaries through ordinary calls and Wasm bindings.
4. Treat `host_abi.rs` / `schemas/host-abi.json` as the primitive-effect and
   capability-requirement source of truth.
5. Model policy narrowing as deriving a more specific authority, not as
   performing the underlying effect.
6. Preserve scope coverage/intersection semantics in enough detail to avoid
   weakening static safety.
7. Give structured async tasks deliberate effect, cancellation, resource, and
   capability-attenuation semantics.
8. Keep the semantic representation source-syntax independent.

The likely integration seam is:

```text
typed AST
  -> typed declaration/body lowering
  -> semantic IR (TypeRef / FunctionSig / Call / host imports)
  -> effect/capability analysis
  -> interpreter + Wasm backend
```

Do not embed effect semantics in the reader/parser or the legacy YAML loader.

## Files/examples to inspect

Prioritize these files:

- `README.md`
- `Cargo.toml`
- `DRAFT.md`
- `src/lower.rs`
- `src/host_abi.rs`
- `src/execute.rs`
- `src/wasm_backend.rs`
- `src/async_runtime.rs`
- `schemas/host-abi.json`
- `schemas/effects.schema.json`
- `docs/superpowers/specs/2026-07-15-capability-host-abi-design.md`
- `examples/fs-roundtrip.vibra`
- `examples/hello.vibra`
- `tests/lang-functions.vibra`
- `tests/lang-generics.vibra`
- `tests/lang-tasks.vibra`
- `tests/stdlib-fs.vibra`
- `stdlib/src/fs.vibra`
- `stdlib/src/io.vibra`

## Requested outcome

Produce an implementation plan for a typed effect system that:

1. Uses the existing host ABI registry as the primitive-effect source.
2. Preserves explicit policy/capability authority and attenuation.
3. Adds effect summaries/checking to functions, calls, async tasks, and Wasm
   wrappers.
4. Works with the in-progress typed S-expression AST/frontend.
5. Can be introduced incrementally while legacy YAML lowering remains.
6. Defines diagnostics, schemas, CLI/LSP/effects-report changes, and a test
   strategy.

## Warning

Some checked-out README/examples still describe the legacy YAML syntax. The
S-expression contract is the intended source surface, while the capability
semantics above remain the relevant semantic/runtime baseline.
