# Native toolchain archives

Every `vX.Y.Z` tag builds relocatable archives for Linux amd64, Linux arm64,
and Windows amd64. Extract one directory and add its `bin/` directory to
`PATH`; no source checkout is required. The root contains:

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
