# Capability-typed host ABI and unified security model

Date: 2026-07-15
Status: **reverted** — the capability/policy system this spec designed was
fully decommissioned; every host operation is now unconditionally available
at runtime. Kept as a historical record of the original design; do not use it
as a description of current behavior.
Related issues: #19 (static wasm FFI groundwork), README/DRAFT/PHILOSOPHY "known
escape hatch" notes.

## Problem

Vibra's stated security model is default-deny, capability-based, and statically
explicit. The implementation does not deliver that today:

1. **`$wasm` is an ambient-authority escape hatch.** Any module may declare a
   `$wasm` body, and the runtime dispatches privileged behavior by the
   *function symbol name* (`println`, `open-read`, `get`, ...), ignoring the
   declared `import.module`/`import.name` except in error text. Renaming a
   user shim to a privileged symbol invokes privileged host behavior.
2. **Two overlapping authority systems.** Legacy `grants:` declarations with
   `=grants` call-site forwarding coexist with the newer `$policy` argument
   model. `main` rejects `grants:` while most of the stdlib still requires
   them, and several host ops accept either.
3. **No static story.** Nothing verifies a `$wasm` body against the host
   surface before execution, and a program's authority cannot be derived from
   its source.
4. **Soundness gaps.** The CLI never builds `approved_policy`, so policy-based
   `main` cannot run outside Rust tests; seeded policy values carry the
   *requested* scopes rather than the approved intersection; `$policy.narrow`
   trusts static types at runtime.

The language has no adopters, so backward compatibility is a non-goal.

## Goals

- Exactly one authority model, visible in source, granular per domain and
  scope, statically checkable, and enforced at runtime.
- `$wasm` becomes a statically verified, capability-typed binding to a closed,
  versioned host ABI — not an escape hatch. User modules may declare `$wasm`
  wrappers, and doing so grants nothing: authority still has to arrive as a
  capability value.
- A program's maximum authority is derivable from `main`'s signature alone.

## Non-goals

- External (dependency-provided) wasm modules — that remains issue #19.
- Runtime plugin loading (#63).
- Changing the grants-free baseline for stdout/stderr writes.

## Design

### 1. Single authority model: `$policy` capability values

The legacy grant system is deleted: `grants:` on functions and tests,
`=grants` call-site forwarding, `$grants.*` references, `$grant-token`,
`$security.grant.*`, grant-status seeding, and the `stdlib/src/security.vibra`
module. Using any of these forms is a compile error with a stable code
(`E-SEC-001`) that names the replacement.

Authority is exclusively represented by `$policy` values:

- **Unforgeable**: cannot be cast, constructed, or literal-created
  (existing `E-CAP-001` stays).
- **Root-injected**: minted by the host only for `main`'s (or a `$test`'s)
  declared `$policy`-typed arguments.
- **Explicitly threaded**: reach privileged code only as ordinary typed
  arguments.
- **Attenuable only**: `$policy.narrow` may shrink but never widen.

A policy type maps *domains* to scope groups with a `mandatory`/`optional`
requirement, unchanged from the current `$policy` shape:

```yaml
main:
  $function: $void
  args:
    policy:
      $policy:
        fs-read:
          - requirement: mandatory
            scopes:
              - dir: ./data
        clock:
          - requirement: optional
            scopes: any
  return: $void
  do: [...]
```

Domains (unchanged set, now the only spelling): `fs-read`, `fs-write`,
`stdin-read`, `env-read`, `env-write`, `net-connect`, `net-listen`,
`process-run`, `clock`, `random`, `system-info`.

Scope selectors (unchanged): `any`, `dir`, `file`, `exact`, `prefix`.

### 2. Approved policy comes from the CLI

The existing `--allow-*` flags remain the user-facing grant surface and now
compile into an **approved policy**:

- `--allow-read PATH` → `fs-read: dir PATH`, `--allow-write` → `fs-write`,
- `--allow-stdin` → `stdin-read: any`, `--allow-env NAME` → `env-read: exact
  NAME` (`*` → `any`), similarly `env-write`, `net-connect`, `net-listen`,
  `process-run`,
- `--allow-clock` / `--allow-random` / `--allow-sys-info` → `any` for their
  domains, `--allow-all` unchanged in meaning.

`RunConfig` derives the approved policy from these fields; embedders and Rust
tests may still override it directly.

### 3. Attenuated seeding (least authority)

At startup, for each `$policy`-typed argument of `main` (recognized by *type*,
not by argument name):

- The runtime computes the **intersection** of the requested policy type and
  the approved policy, domain by domain and scope by scope.
- Every `mandatory` domain group must be non-empty after intersection,
  otherwise startup fails with a diagnostic naming the missing domain/scope.
- `optional` domain groups may intersect to empty; the seeded value simply has
  no scopes there and privileged calls fail closed at use with
  `permission-denied`-style typed errors.
- The seeded runtime value carries the intersected scopes — never the raw
  requested scopes and never more than the CLI approved.

`$policy.narrow` keeps its static no-widening check and additionally
intersects against the **live source value's** scopes at runtime, so a
narrowed value can never exceed the value it came from.

Optional-capability introspection is served by result matching (calls fail
with typed `permission-denied` errors); the old `security.granted` helper is
deleted with the grant system.

### 4. Versioned host ABI registry

A new compiler module (`src/host_abi.rs`) is the single source of truth for
the host surface. Each entry declares:

- `module` (`vibra_v1`, `vibra_test`, `vibra_code`),
- `name` (import name),
- parameter shape (value kinds, including which positions are capability
  parameters and which domains those capabilities must cover),
- return shape,
- required capability domains (empty for pure ops).

The registry is exported as a machine-readable schema
(`schemas/host-abi.json`), with a Rust test asserting the file matches the
compiled-in registry.

Privileged operations that previously shared one import name but differed by
symbol get distinct import names (e.g. `path_open` splits into
`fs_open_read`, `fs_open_write`, `fs_open_append`, `fs_open_read_write`).
Standard-stream handles are minted through registry imports (`stdin_open`
requires a `stdin-read` capability; `stdout_open`/`stderr_open` are baseline)
instead of `$cast`-forged integers.

`vibra_v1` registry (capability domains in brackets):

| import | params | caps |
|---|---|---|
| `stdin_open` | (policy) → handle | stdin-read |
| `stdout_open` / `stderr_open` | () → handle | — |
| `fd_read` / `fd_read_line` | (handle) → result str | — (handle is authority) |
| `fd_write` | (handle, str) → result void | — |
| `fd_sync` / `fd_close` | (handle) → result void | — |
| `path_new` / `path_join` / `path_parent` / `path_extension` | pure path ops | — |
| `fs_open_read` | (path, policy) → result handle | fs-read |
| `fs_open_write` / `fs_open_append` | (path, policy) → result handle | fs-write |
| `fs_open_read_write` | (path, policy) → result handle | fs-read + fs-write |
| `fs_read_to_string` | (path, policy) → result str | fs-read |
| `fs_write_string_all` / `fs_append_string` | (path, str, policy) → result void | fs-write |
| `fs_exists` | (path, policy) → bool | fs-read |
| `fs_create_dir_all` / `fs_remove_file` / `fs_remove_dir` | (path, policy) → result void | fs-write |
| `fs_read_dir` | (path, policy) → result [path] | fs-read |
| `fs_metadata` | (path, policy) → result str | fs-read |
| `fs_canonicalize` | (path, policy) → result path | fs-read |
| `env_get` | (str, policy) → result str | env-read |
| `env_set` | (str, str, policy) → result void | env-write |
| `net_connect` | (str, policy) → result void | net-connect |
| `net_listen` | (str, policy) → result void | net-listen |
| `process_run` | (str, policy) → result void | process-run |
| `clock_now_unix_millis` | (policy) → uint64 | clock |
| `random_bytes` | (uint64, policy) → bytes | random |
| `system_info` | (policy) → str | system-info |

`vibra_test` (assert/fail/assert-eq-\*) and `vibra_code` (structural editing)
keep their current import names; all are capability-free because they operate
on in-memory values only.

### 5. Statically verified `$wasm`

At lowering, every `$wasm` body is validated against the registry:

- **`E-WASM-002`** — unknown host module or import name.
- **`E-WASM-003`** — `$wasm.args` arity or shape does not match the registry
  entry (wrong count, non-policy argument in a capability position, policy
  argument in a value position, or a wrapper signature whose declared types
  cannot satisfy the entry).
- **`E-CAP-002`** — a capability position is not fed by a `$policy`-typed
  wrapper argument whose declared domains cover the entry's required domains.

Because `$policy` values are unforgeable and only enter at `main`/`$test`
roots, this closes the loop: **statically, a program's authority is bounded by
the policy types declared on its roots**, and each host import's requirement
is visible in the registry. User modules may declare `$wasm` wrappers freely —
a wrapper without a genuine capability argument cannot call a privileged
import, and a wrapper with one is just as auditable as the stdlib.

### 6. Structural runtime dispatch

`exec_call` dispatches strictly on `(import.module, import.name)`. Symbol- and
alias-based matching (`sym == "println"`, `sig.alias.ends_with("env")`,
suffix-matched interface methods) is deleted. Host argument values are taken
from the declared `$wasm.args` forwarding specs (which finally become
load-bearing), not positional call-site guesses.

Runtime capability checks remain as defense in depth: each privileged import
resolves its dynamic target (path, env name, host, command) against the
scopes of the *capability value actually passed*, using the existing
canonical-ancestry path rules.

### 7. Tests declare policy

`$test` declarations replace `grants:` with an optional `policy:` sibling
carrying a `$policy` type. The test runner seeds it exactly like `main`
(intersection with the CLI-approved policy of `vibra test`); profiles continue
to select tests and never grant authority. Grant-free tests stay grant-free.

```yaml
get-reads-an-explicitly-granted-variable:
  $test: env
  tags: [stdlib, capability]
  policy:
    $policy:
      env-read:
        - requirement: mandatory
          scopes:
            - exact: PATH
  do:
    - $let:
        value:
          $env.get: PATH
          policy: $args.policy
```

### 8. Static effect surface: `vibra effects`

A new CLI command prints the program's statically derived effect surface as
YAML: every host import referenced by the loaded module graph (module, name,
required domains) and the root policy requested by `main`. Output shape is
documented by `schemas/effects.schema.json`. This is the "what can this
program ever do" audit artifact; it requires no execution.

## Implementation notes

- **stdlib**: `security.vibra` is deleted; `fs`, `io`, `env`, `net`,
  `process`, `random`, `sys`, `time` move to policy arguments and the new
  import names. stdlib changes land in `nahharris/vibra-stdlib` and the
  submodule pin advances.
- **compiler**: `src/host_abi.rs` (new), `src/lower.rs` (grant removal, ABI
  validation, policy-typed root recognition), `src/execute.rs` (structural
  dispatch, attenuated seeding, runtime narrow fix), `src/runtime/wasi_env.rs`
  (derived approved policy), `src/test_runner.rs` (test policy seeding),
  `src/main.rs` (`vibra effects`).
- **docs/schemas**: README, DRAFT, PHILOSOPHY escape-hatch text replaced by
  the new model; `schemas/host-abi.json`, `schemas/effects.schema.json`,
  `schemas/function.schema.json` (grants key removed, policy documented),
  `schemas/linter-codes.json` (`E-WASM-002`, `E-WASM-003`, `E-CAP-002`,
  `E-SEC-001`).
- **tests**: Rust integration tests move off `grants:`/raw WASI imports;
  Vibra capability tests move to `policy:`; new tests cover forged-shim
  rejection, unknown-import rejection, capability-position validation,
  attenuated seeding, runtime narrowing, and `vibra effects` output.

## Security invariants (post-change)

1. No host import with a capability requirement is callable without a
   `$policy` value covering that domain, statically and at runtime.
2. `$policy` values cannot be forged, widened, or ambiently discovered; they
   enter only through `main`/`$test` signatures.
3. A seeded value's scopes never exceed `min(requested, CLI-approved)`.
4. Renaming or aliasing functions confers no authority (structural dispatch).
5. The complete host surface and each entry's requirement are machine-readable
   and covered by a sync test.

Remaining ambient authority, accepted deliberately: writes to stdout/stderr
and pure in-memory computation (`vibra_test`, `vibra_code`, path algebra).
