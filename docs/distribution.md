# Native toolchain archives

Every `vX.Y.Z` tag builds relocatable archives for the tier-1 native targets
below. Extract one directory and add its `bin/` directory to `PATH`; no source
checkout is required.

| Platform ID | Rust target triple | Runner | Archive |
| --- | --- | --- | --- |
| `linux-amd64` | `x86_64-unknown-linux-gnu` | Ubuntu x64 | `.tar.gz` |
| `linux-arm64` | `aarch64-unknown-linux-gnu` | Ubuntu arm64 | `.tar.gz` |
| `windows-amd64` | `x86_64-pc-windows-msvc` | Windows x64 | `.zip` |
| `macos-amd64` | `x86_64-apple-darwin` | macOS Intel | `.tar.gz` |
| `macos-arm64` | `aarch64-apple-darwin` | macOS Apple silicon | `.tar.gz` |

Tier 1 means that every pull request builds the compiler, executes the
relocatable `.vapp` smoke, and proves byte-identical `.vapp` output on the
native host; every release tag additionally builds and relocates the native
archive. Other Rust host targets are best-effort source builds: they have no
published binary, CI gate, or compatibility commitment.

The machine-readable source of truth is
[`distribution/targets.json`](../distribution/targets.json), validated against
[`distribution-targets.schema.json`](../schemas/distribution-targets.schema.json).
Each archive repeats its canonical Rust target triple in `release.vibra`.

The archive root contains:

```text
vibra-X.Y.Z-<platform>/
  bin/vibra[.exe]
  stdlib/
  LICENSE
  release.vibra
```

`release.vibra` binds the compiler revision to the canonical stdlib Git
revision and a SHA-256 digest of every bundled stdlib file. `vibra init` checks
that identity and digest before copying the stdlib and reports `E-DIST-003` for
an incompatible revision or `E-DIST-004` for damaged content.

The tag workflow runs both project test suites before building assets, smokes
each archive after moving it to a new directory, publishes `sha256sums.txt`,
and records GitHub artifact provenance. It exercises `init`, `exec`, `check`,
source execution, `.vapp` build and verification, and `.vapp` execution.
Linux uses GNU tar's normalized metadata flags; macOS uses the host bsdtar so
the native executable bit survives packaging and relocation.
