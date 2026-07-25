# Deterministic source dependency resolution

Status: proposed design for the post-v1 resolver. The current `git` + `rev`
manifest and lock format remain the only implemented dependency workflow.

## Decisions

The resolver is a deterministic, single-version, PubGrub-style solver over
immutable Git tags. It selects one version for each canonical package identity
across the whole graph. This is intentionally stricter than today's
package-local vendoring: two aliases may name the same package, but they cannot
select two versions. A single version keeps nominal types, the wasm ABI, and
especially `std` coherent across an application.

Resolution uses semantic versions as specified by SemVer 2.0.0. Normal ranges
use the familiar comparator syntax (`>=1.2.0 <2.0.0`), with `^1.2.3` and
`~1.2.3` as sugar. The manifest addition is:

```vibra
(dependency math
  git: "https://github.com/example/vibra-math.git"
  version: "^1.2.0")
(dependency parser
  git: "https://github.com/example/mono.git"
  subdir: "packages/parser"
  version: ">=2.0.0 <3.0.0")
```

Exactly one of `rev:` and `version:` is allowed. Existing `rev:` dependencies are
exact constraints and continue to work unchanged. `subdir:` is a clean relative
path with `/` separators; `.` and `..` segments are forbidden.

### Package identity

Identity is `(canonical-git-url, normalized-subdir)`. URL canonicalization:

1. converts GitHub-style SSH shorthand to HTTPS;
2. lowercases scheme and host, removes the default port, trailing slash, and a
   final `.git` suffix; and
3. preserves path case and percent-encoded bytes.

The normalized empty subdirectory is `/`. Redirects and repository renames do
not change identity automatically. Authors must update the source explicitly.
Two declarations that canonicalize to one identity participate in one solve.
Package names and dependency aliases are not identity.

Tags must have the exact form `v<semver>` or `<semver>`. If both forms exist,
they must resolve to the same commit or resolution fails with
`E-RESOLVE-AMBIGUOUS-TAG`. Lightweight and annotated tags are peeled to a
commit. The lock stores both the selected version and commit, so moved tags are
detected rather than silently accepted.

### Selection and prereleases

Candidates are ordered by descending semantic version, then by the UTF-8 byte
ordering of the tag name as a final deterministic tie breaker. Build metadata
does not affect precedence; two tags equal in SemVer precedence but pointing to
different commits are ambiguous. A prerelease is considered only when a
comparator in the constraint names a prerelease with the same major, minor,
and patch. There is no implicit fallback from stable releases to prereleases.

The solver derives incompatibilities in deterministic dependency order
(canonical identity bytes, then alias bytes) and chooses the highest candidate
compatible with all constraints. PubGrub conflict resolution is used because
it produces a causal incompatibility chain, not merely a failed package name.
Equivalent solutions therefore produce identical locks on every platform.

## Locking and updates

Lock format 2 will add `version`, `tag`, and `subdir` to version-selected
packages while retaining `git`, exact `rev`, tree hash, vendor path, and alias
edges. The `rev` is always authoritative for fetching and reproduction.
Packages introduced with an exact revision retain their current lock shape.
Entries are sorted by canonical identity, subdirectory, and vendor path.

Normal `vibra sync` is a minimal-change operation: every locked selection that
still satisfies all constraints is preferred over any unlocked candidate. If
an entry no longer satisfies the graph, the resolver unlocks that package and
the reverse-dependency closure whose constraints depend on its selection; all
other entries remain pinned. `vibra sync --update <alias>` unlocks the named
identity and that same closure. `--update-all` ignores version selections but
still preserves exact `rev` constraints. Candidate ordering then chooses the
highest compatible versions.

`std` is an ordinary source dependency and is solved by the same rules, but it
is a required singleton: every transitive `std` identity must canonicalize to
the root's `std` identity and accept the selected version. A different source
or incompatible range is `E-RESOLVE-STDLIB-CONFLICT`. This protects the single
compiler/runtime ABI while avoiding a special stdlib solver.

## Overrides

Only the root manifest may contain `replacements`. A replacement is keyed by
canonical identity and supplies an exact `git` + `rev` (and optional `subdir`).
It replaces the source for every occurrence before solving; it does not relax
version constraints. The replacement package must declare a version satisfying
all original constraints, and its declared package name must match. Transitive
manifests cannot replace packages. Locks record the original identity, selected
replacement identity and revision, making the override visible and auditable.

Path replacements are deliberately excluded: they cannot be reproduced by a
committed lock. Local development can continue to use a root path dependency,
but it is outside version resolution and cannot replace a transitive identity.

## Offline and reproducibility

`vibra sync --locked --offline` performs no solve and no network access. It
requires a complete format-1 or format-2 lock and verifies every vendored tree
against its locked revision and SHA-256. A version manifest without a lock is
therefore an error offline. `--offline` without `--locked` may solve only from
a content-addressed local tag/manifest cache and emits the cache digest into
the report; it must never guess that the cache is complete.

Online resolution snapshots the candidate tag name, peeled commit, and the
dependency manifest hash. Before writing the lock, it rechecks this snapshot;
changed tags fail with `E-RESOLVE-MUTABLE-TAG`. Vendoring always checks out the
locked commit, never the tag. Thus migration to ranges changes selection, not
the reproducibility of a committed lock.

## Structured diagnostics

Resolution reports conform to
[`dependency-resolution.schema.json`](../schemas/dependency-resolution.schema.json).
Stable codes are `E-RESOLVE-CONFLICT`, `E-RESOLVE-STDLIB-CONFLICT`,
`E-RESOLVE-AMBIGUOUS-TAG`, `E-RESOLVE-MUTABLE-TAG`, and
`E-RESOLVE-OFFLINE-MISS`. A conflict contains all causal requirements as root
to leaf chains; each edge carries the requiring identity, selected version (or
`root`), dependency alias, target identity, and requirement. Chains and
available versions use the same canonical ordering as the solver. Human output
is rendered from this data and must not be parsed by tools.

Example conflict:

```yaml
resolution-version: 1
outcome: conflict
code: E-RESOLVE-CONFLICT
package: https://github.com/example/common#/
requirements:
  - chain:
      - from: root
        alias: left
        to: https://github.com/example/left#/
        requirement: ^1.0.0
      - from: https://github.com/example/left#/
        from-version: 1.0.0
        alias: common
        to: https://github.com/example/common#/
        requirement: ^1.0.0
  - chain:
      - from: root
        alias: right
        to: https://github.com/example/right#/
        requirement: ^1.0.0
      - from: https://github.com/example/right#/
        from-version: 1.0.0
        alias: common
        to: https://github.com/example/common#/
        requirement: ^2.0.0
available: [1.9.0, 2.1.0]
```

## Conformance vectors

[`dependency-solver-vectors.json`](test-vectors/dependency-solver-vectors.json)
is the normative design fixture. It covers compatible diamonds, incompatible
ranges and causal chains, rejection of multiple versions, prerelease gating,
stdlib conflicts, root replacement, minimal-change updates, and offline locked
resolution. Implementations must produce each `expected` selection or error
from only the vector's repository snapshot, manifest, and optional prior lock.

## Implementation milestones

1. Add SemVer/range parsing, Git URL/subdirectory canonicalization, and unit
   tests using the identity and prerelease vectors.
2. Add tag snapshot discovery and immutable tag-to-commit verification behind
   a repository-provider trait with an in-memory conformance provider.
3. Implement deterministic PubGrub decisions and structured conflict chains;
   run every conformance vector without filesystem or network access.
4. Introduce manifest version-range/replacement fields and lock format 2 while
   retaining format-1 exact-revision reads and writes.
5. Integrate minimal-change, targeted update, offline, and locked modes into
   `vibra sync`; vendor and hash only after a successful solve.
6. Add end-to-end Git fixtures, schema validation, migration documentation,
   and explicit format-1/format-2 interoperability tests.

Native artifacts, wasm ABI selection, registries, and runtime plugins are not
part of this design. A future artifact dependency may reuse package identity
and version selection, but must define compatibility independently.
