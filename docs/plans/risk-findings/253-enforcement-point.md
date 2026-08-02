## Pre-implementation findings

Investigated before implementation to correct a cost estimate this issue was
opened with. **The estimate was too optimistic and the issue body overstates
what already exists.** Details below so the implementer is not surprised.

### What was checked

`src/async_runtime.rs` (capability machinery), `src/execute.rs` (runtime
construction and host dispatch), and every construction site of `Scheduler`.

### Correction 1 — the grant machinery is inert in production

The issue body says the runtime substrate exists and only surface syntax and a
propagation rule are missing. That is **half right**. The lattice exists and is
correct, but it is not wired into anything a real program executes:

- `src/execute.rs:943` builds the runtime with `Scheduler::new([])` — the root
  scope is seeded with an **empty grant set**.
- Every `Scheduler::new` call site carrying actual grants
  (`src/async_runtime.rs:1092` onward) is inside the `#[cfg(test)]` module that
  begins at `src/async_runtime.rs:1043`.
- `open_scope` / `open_scope_with_limits` — the function containing the
  narrowing check at `src/async_runtime.rs:384-393` — is **never called from
  `src/execute.rs`**. A grep for `open_scope` in that file returns nothing.

So the monotone-narrowing check is exercised only by unit tests. In a real
program run, no grant is ever held, requested, or checked.

**Revised scope.** This issue needs three things, not two:

1. Seed the root scope from the manifest declaration (new).
2. Wire scope opening into real execution so narrowing is reachable (new — this
   was not anticipated).
3. Add the operation-time check (already anticipated).

The lattice and its tests are still genuine reuse, and the amplification error
type (`RuntimeError::CapabilityAmplification`,
`src/async_runtime.rs:223`) already exists. But this is wiring plus new
enforcement, not "surface syntax over a working runtime."

### Correction 2 — grants are resource-scoped, not root-scoped

The plan decided grants should be per effect root. The existing type is richer
and better:

```rust
// src/async_runtime.rs:109
pub struct CapabilityGrant {
    pub domain: String,
    pub resource_prefix: String,  // empty means the whole domain
}
```

`is_within` (`src/async_runtime.rs:116-126`) implements hierarchical path
containment on `resource_prefix`, with `/` as the separator.

These are two orthogonal axes, and the plan conflated them:

- **domain** — maps to the effect root (`fs.read`, `net.connect`).
- **resource_prefix** — scopes *which resources* within that domain, e.g.
  filesystem access confined to a subtree.

**Revised decision: keep domain at effect-root granularity as planned, and use
`resource_prefix` for resource scoping.** This is strictly more useful than the
plan's per-root design, costs nothing extra because `is_within` already
implements it, and gives embedders the property that actually matters in
practice — not merely "this program touches the filesystem" but "this program
touches the filesystem only under this path."

### Correction 3 — domain naming does not match the effect inventory

The existing tests use domain strings `filesystem-read` and `network`
(`src/async_runtime.rs:1163`, `1177`). The accepted effect-root inventory in
`docs/decisions/effect-system.md` uses `fs.read`, `net.connect`, and peers.

These must be reconciled, and the effect-root names should win — they are the
accepted contract and the names authors actually write. Expect to update the
existing tests as part of this work; they are not load-bearing for the naming.

### Consequence for #251

`ScopeLimits` inheritance (`src/async_runtime.rs:384`) sits in the *same*
never-called `open_scope_with_limits` path. Fuel and memory ceilings added
there will be equally inert until scope opening is wired into real execution.
**#251 and this issue share that wiring work**, and whichever lands first
should do it in a way the other can reuse rather than duplicating it.

### Still unmeasured

Whether a per-operation grant check is affordable. The plan flags this as the
dominant performance risk, citing SandCell's finding that boundary-crossing
frequency dominates sandboxing cost. Nothing found here changes that — it needs
numbers from a prototype, and the fallback (scope-entry checking only) remains
available.

### Verdict

Proceed, with the scope revised upward. The reusable assets are the grant type,
`is_within`, the amplification error, and the existing tests. The work not
previously accounted for is wiring scope lifecycle into `src/execute.rs`, which
should be scoped and reviewed as part of this issue rather than discovered
during it.
