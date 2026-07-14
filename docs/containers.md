# Container images

Vibra publishes two official images to GitHub Container Registry (GHCR):

- Runtime: `ghcr.io/nahharris/vibra`
- Development: `ghcr.io/nahharris/vibra-dev`

Both images set Vibra as their entrypoint and default to `vibra --help`.
The runtime image contains only the executable and its standard library; the
development image also contains the Rust toolchain, Cargo, Git, and the
platform build tools needed to work on Vibra itself.

## Platforms and standard library

Runtime and development tags are published for Linux amd64, Linux arm64, and
Windows LTSC 2022 amd64. Windows ARM is deferred until the base image and CI
support are available. Pull the variant matching the host's container mode.

The standard library is part of each image artifact rather than an incidental
source checkout. Linux images keep it in `/opt/vibra/stdlib`; Windows images
keep it in `C:\Vibra\stdlib`. The installed executable discovers that layout
at runtime, so generated projects can initialize, check, and run without a
repository checkout. The broader portable distribution contract, including
missing or mismatched-library diagnostics, is tracked in
[issue #15](https://github.com/nahharris/vibra/issues/15).

## Pulling and running

For a repeatable deployment, pin an immutable image digest rather than a
moving tag:

```sh
docker pull ghcr.io/nahharris/vibra@sha256:<manifest-digest>
docker run --rm ghcr.io/nahharris/vibra@sha256:<manifest-digest> --version
docker run --rm -v "$PWD:/work" -w /work ghcr.io/nahharris/vibra:latest check .
```

On Windows, Docker Desktop must be switched to **Windows containers**. The
host and container must be compatible with the LTSC 2022 base image; use a
current Windows Server 2022 or compatible Windows 11 Docker Desktop host.
PowerShell example:

```powershell
docker run --rm ghcr.io/nahharris/vibra:latest --version
docker run --rm -v "${PWD}:C:\work" -w C:\work ghcr.io/nahharris/vibra:latest check .
```

Use the development image when a workflow needs Cargo:

```sh
docker run --rm -v "$PWD:/src" -w /src ghcr.io/nahharris/vibra-dev:latest cargo test
docker run --rm -v "$PWD:/src" -w /src ghcr.io/nahharris/vibra-dev:latest cargo run -- test
```

For a GitHub Actions job that only needs the compiler, use the runtime image:

```yaml
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@<pinned-commit>
      - run: docker run --rm -v "$PWD:/work" -w /work ghcr.io/nahharris/vibra@sha256:<manifest-digest> check .
```

## Tags and rebuilds

For a stable release `X.Y.Z`, `X.Y.Z-r0` is the initial immutable container
revision. `X.Y.Z` remains the initial immutable r0/source tag. Later base-image
rebuilds publish immutable `X.Y.Z-rN` tags without changing the source release.
After a successful rebuild, only `X.Y`, `X`, and `latest` move to the newest
stable revision. Release candidates and other prereleases receive their exact
version tag only; they do not move these stable aliases. Never rely on a moving
alias when reproducibility matters.

The monthly rebuild workflow creates the next `-rN` revision after it has
rebuilt, scanned, smoke-tested, and attested every supported platform. Exact
source-version and revision tags remain immutable.

## Supply chain and maintenance

The Dockerfiles pin their base images by digest. Dependabot proposes monthly
updates for Docker base images and GitHub Actions; review those updates, then
let CI rebuild and smoke-test every platform before merging. Each published
image is scanned for HIGH and CRITICAL vulnerabilities, has an SPDX SBOM, and
receives GitHub build provenance.

Verify an image's provenance before promoting it:

```sh
gh attestation verify oci://ghcr.io/nahharris/vibra@sha256:<manifest-digest> \
  --repo nahharris/vibra
```

Repository maintainers should make each GHCR package public after its first
publish (Package settings → Change visibility) and link it to this repository.
The publishing workflow needs `packages: write`, `attestations: write`, and
`id-token: write`; it uses the repository `GITHUB_TOKEN`, so no long-lived
registry credential is required. Protect release tags and, if an environment is
used for releases, require its approval before publishing.

## Local verification

Docker must be running in the desired container mode before testing locally.
From the repository root, build the appropriate image, then run the project
smokes:

```sh
docker build -f containers/Dockerfile.linux --target runtime -t vibra:local .
pwsh ./tests/container-layout.ps1
pwsh ./tests/container-smoke.ps1 -Image vibra:local -Platform linux
```

For Windows containers, switch Docker Desktop first and build
`containers/Dockerfile.windows`; invoke the smoke test with `-Platform windows`.
Issue [#17](https://github.com/nahharris/vibra/issues/17) tracks this image
delivery work.
