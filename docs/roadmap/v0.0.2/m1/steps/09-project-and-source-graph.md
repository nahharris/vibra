# Step 09: project-and-source-graph

Kind: vertical. The new `vibra-workspace` crate: project discovery, the
typed `@project.v2` decoder for the shapes M1 can check, the confined source
snapshot, the module path rules, and the `@sourceGraph.v1` observation, with
the `projectDecode` and `sourceGraph` handlers. No `.vib` byte is parsed
here, so the step depends only on the VIBON reader.

## Prerequisites

- Step 03 landed.
- Read: [programs chapter](../../../../spec/04-programs-and-packages.md)
  "Project file" (the record table, "Discovery and the source snapshot"),
  "Packages, units, and targets" (the first two paragraphs), "Tests" (the
  `@tests` unit paragraph); [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md) "Availability,
  discovery and resolution" table and the `@sourceGraph.v1` row of "Persistent
  observations"; [tooling chapter](../../../../spec/05-tooling.md) "Snapshot,
  claim and witness identity" for `workspaceRevision`.
- Open questions: Q11 (whether `check` covers the `@tests` unit) does not
  block this step.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-workspace-v1/src/confined_fs.rs`, `discovery.rs:78` (`discover_project`), `snapshot.rs:149` (`SourceSnapshot`) | copied and adapted: the filesystem boundary, link and junction checks, sorted walk, immutable bytes |
| `crates/vibra-workspace-v1/src/project.rs:1212` (`decode_project`) | the decoder shape; the schema and notation change |
| `crates/vibra-workspace-v1/src/source_graph.rs:199` (`SourceGraph`) | the graph shape and the `@sourceGraph.v1` encoding |
| `crates/vibra-conformance-v1/src/{project.rs,graph.rs}` | handler shapes |
| `crates/vibra-workspace-v1/tests/it/{project_decoder.rs,source_graph.rs,source_graph_tests.rs}` | the host-test matrix to port, rewritten for the new notation |

Proposed: `vibra-workspace::{confined_fs, discovery, project, snapshot,
source_graph, revision}`; `vibra-conformance::{project, graph}` handlers.

## Ordered tasks

1. `project`: decode `@project.v2` from a `DataNode`: closed records
   `project`, `package`, `target`, `path dependency`, `Git dependency`;
   unknown, duplicate, and missing fields (`@data.invalidShape`,
   `@data.duplicateField`); value constraints (`@data.invalidValue`: a
   package name that is not a camelCase identifier, a version that is not
   one semantic version, a target name that is not a one-part identifier
   atom, a Git URL with userinfo or not HTTPS, a `rev` that is not 40 lower
   hex digits); `@project.v1` and any other `format` is `@data.invalidShape`
   at the `format` field. A `@lib` target with `entry`, `effects`, or
   `grants` is `@data.invalidShape`. A `@bin` target is decoded by shape
   (its `grants` dict decoded as generic data) and marked `unavailable` for
   checking until M2; a `dependencies` entry is decoded and marked
   `unavailable` until M3. Invariant: the decoder reads no path.
2. `discovery`: from a directory or a file's parent upward to the first
   directory with a regular file `project.vibon`; `@project.notFound`; a
   malformed nearest marker stops the search; a marker that is a link is
   `@project.ioError`. Invariant: no fallback to an older ancestor.
3. Target roots: non-empty relative, no `.`/`..`, canonical directory
   inside the root (`@project.invalidTargetRoot`), pairwise disjoint
   (`@project.overlappingTargetRoots` on the later one), `tests/` disjoint
   from every root, a target named `tests` is `@project.reservedUnitName`.
   Invariant: these checks finish before any source directory is listed.
4. `snapshot`: walk each root and `tests/`; `.vib` files only;
   `@module.invalidSegment` (offset 0 of that source), `@module.caseCollision`,
   `@module.fileDirectoryCollision`, `@module.pathEscape`, `@module.ioError`;
   sorted by project-relative path; bytes copied exactly; linked files
   admitted once. Invariant: repeated snapshots of an unchanged tree are
   identical.
5. `revision`: `workspaceRevision` per the tooling chapter's framing, with
   the test vector `sha256:33d677dc…4ff1` and an equal-length edit as host
   tests.
6. `source_graph`: units (targets, `@tests`), modules with their identities,
   `@module.sourceIdCollision`; the canonical `@sourceGraph.v1` text. Every
   graph has the `@tests` unit.
7. Handlers `projectDecode` (one `project.vibon`, no discovery) and
   `sourceGraph` (a confined tree) in `vibra-conformance`.
8. Cases.

## Test matrix

| Case id | Kind | Input | Expected |
| --- | --- | --- | --- |
| `V2-PROJECT-decode-library` | positive | `{format: @project.v2 package: {name: "hello" version: "0.1.0"} targets: [{name: @hello kind: @lib root: "src/hello"}] dependencies: [:]}` | accepted; one library target |
| `V2-PROJECT-decode-binary-shape` | boundary | the chapter's example project (a `@bin` target with grants) | accepted by the decoder; the target is marked unavailable for checking (recorded in the graph as `kind: @bin`) |
| `V2-PROJECT-decode-v1-format` | negative | `{format: @project.v1 …}` | `@data.invalidShape` at `@project.v1` |
| `V2-PROJECT-decode-unknown-field` | negative | the library project plus `author: "x"` | `@data.invalidShape` at `author` |
| `V2-PROJECT-decode-duplicate-field` | negative | `package` written twice | `@data.duplicateField` at the second |
| `V2-PROJECT-decode-bad-name` | negative | `name: "My-App"` | `@data.invalidValue` at the string |
| `V2-PROJECT-decode-bad-version` | negative | `version: "^1.0"` | `@data.invalidValue` |
| `V2-PROJECT-decode-lib-with-entry` | negative | a `@lib` target with `entry: @x.main` | `@data.invalidShape` at `entry` |
| `V2-PROJECT-decode-git-rev` | negative | `rev: "abc"` in a Git dependency | `@data.invalidValue` |
| `V2-PROJECT-decode-dict-key-dup` | negative | `grants: [@a: {…} @a: {…}]` | `@data.duplicateKey` |
| `V1-PROJECT-graph-static-sorted` | positive | a tree with `src/app/b.vib`, `src/app/a.vib`, `tests/t.vib` | graph lists `a` before `b`; the `@tests` unit holds `t`; kept `V1` identity, rule unchanged |
| `V1-PROJECT-graph-static-invalid-segment` | negative | `src/app/my-mod.vib` | `@module.invalidSegment` at offset 0 of that source |
| `V1-PROJECT-graph-static-collision` | negative | `src/app/text.vib` beside `src/app/text/` | `@module.fileDirectoryCollision` |
| `V2-PROJECT-graph-case-collision` | negative | `src/app/A.vib` and `src/app/a.vib` (built by the handler on a case-sensitive host; on others the case is a host test) | `@module.caseCollision` |
| `V1-PROJECT-graph-static-overlap` | negative | roots `src/app` and `src/app/inner` | `@project.overlappingTargetRoots` on the later target |
| `V2-PROJECT-graph-tests-overlap` | negative | a target root `tests` | `@project.reservedUnitName` |
| `V2-PROJECT-graph-root-missing` | negative | root `src/nothere` | `@project.invalidTargetRoot` |
| `V1-PROJECT-graph-static-dot-root` | negative | root `./src` | `@project.invalidTargetRoot` |
| `V2-PROJECT-graph-empty-tests` | positive | a tree with no `tests/` | graph has an empty `@tests` unit |
| `V2-PROJECT-graph-not-found` | negative | a tree with no `project.vibon` | `@project.notFound` |
| host: link escape | negative | a symlink from a root to outside (host test on Windows and Unix) | `@module.pathEscape` |
| host: revision vector | positive | the chapter's three records | the exact digest; an equal-length edit changes it |

## Diagnostics and schema changes

Codes: `@data.invalidShape`, `@data.invalidValue`, `@data.duplicateField`,
`@data.duplicateKey`, `@project.notFound`, `@project.ioError`,
`@project.invalidTargetRoot`, `@project.overlappingTargetRoots`,
`@project.reservedUnitName`, `@module.invalidSegment`,
`@module.caseCollision`, `@module.fileDirectoryCollision`,
`@module.pathEscape`, `@module.ioError`, `@module.sourceIdCollision`.
Schema: `@sourceGraph.v1` (unchanged shape; bytes as lower-case hex).

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-workspace -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: `staticV2 conformance: N passed, 0 failed, 0 unavailable` with the
cases above; the revision test vector passes.

## Excluded

Resolution (10); dependencies and the lock (M3); `@bin` target validation,
grants, and the entry (M2); `project init|add|remove|sync` (M2, M3).

## Completion evidence

- [ ] `conformance/rules.tsv` has `V2-PROJECT` active and the kept `V1-PROJECT`
      identities listed as active with their anchor.
- [ ] A host test proves two snapshots of one tree are byte-identical.
- [ ] All cases pass on Windows, Linux, and macOS in CI.
- [ ] Validation commands exit 0.
