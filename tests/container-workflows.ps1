$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot

function Require-Content {
    param([string]$Path, [string]$Pattern, [string]$Description)
    if (-not (Test-Path -LiteralPath $Path)) { throw "$Description is missing: $Path" }
    if (-not (Select-String -LiteralPath $Path -Pattern $Pattern -Quiet)) { throw "$Description is missing from $Path" }
}

$ci = Join-Path $repoRoot '.github/workflows/container-ci.yml'
$publish = Join-Path $repoRoot '.github/workflows/publish-containers.yml'
$rebuild = Join-Path $repoRoot '.github/workflows/rebuild-containers.yml'
$dependabot = Join-Path $repoRoot '.github/dependabot.yml'

Require-Content $ci '^permissions:\s*\r?$' 'CI default permissions block'
Require-Content $ci '^\s+contents: read\s*$' 'CI contents-read permission'
Require-Content $ci 'pull_request:' 'CI pull-request trigger'
Require-Content $ci 'push:' 'CI push trigger'
Require-Content $ci 'main' 'CI main branch'
Require-Content $ci '^concurrency:' 'CI superseded-run concurrency policy'
Require-Content $ci 'cancel-in-progress: true' 'CI superseded-run cancellation'
Require-Content $ci 'classify-container-ci-changes\.ps1' 'CI path classifier'
Require-Content $ci 'needs\.change-scope\.outputs\.source' 'Source job path gate'
Require-Content $ci 'needs\.change-scope\.outputs\.image' 'Image job path gate'
if (Select-String -LiteralPath $ci -Pattern '^\s+paths:' -Quiet) { throw 'CI must not restrict paths; compiler and runtime changes require container coverage.' }
Require-Content $ci 'ubuntu-latest' 'Linux amd64 runner'
Require-Content $ci 'ubuntu-24\.04-arm' 'Linux arm64 runner'
Require-Content $ci 'self-hosted, windows, windows-containers' 'Windows container-capable runner labels'
Require-Content $ci 'VIBRA_WINDOWS_CONTAINERS_RUNNER' 'Windows container runner enablement gate'
Require-Content $ci 'container-layout\.ps1' 'Container layout test'
Require-Content $ci 'container-workflows\.ps1' 'Container workflow static test'
Require-Content $ci 'container-smoke\.ps1' 'Container smoke test'
Require-Content $ci '-Dev' 'Development-image smoke test'
Require-Content $ci 'cargo test' 'Source Rust baseline'
Require-Content $ci 'cargo run -- test' 'Source Vibra baseline'
Require-Content $ci 'docker/setup-buildx-action@8d2750c68a42422c14e847fe6c8ac0403b4cbd6f' 'Pinned Buildx setup action'
Require-Content $ci 'docker/build-push-action@53b7df96c91f9c12dcc8a07bcb9ccacbed38856a' 'Pinned Docker build action'
Require-Content $ci 'type=gha,scope=vibra-linux-amd64' 'Linux amd64 persistent build cache'
Require-Content $ci 'type=gha,scope=vibra-linux-arm64' 'Linux arm64 persistent build cache'
Require-Content $ci 'cache-to: type=gha,mode=max,scope=.*ignore-error=true' 'Non-fatal persistent cache export'
Require-Content $ci 'DockerCli\.exe.*-SwitchWindowsEngine' 'Windows Docker engine switch'

Require-Content $publish 'tags:\s*\[.v\*.' 'Publish tag-only trigger'
Require-Content $publish 'packages: write' 'Package publish permission'
Require-Content $publish 'attestations: write' 'Attestation permission'
Require-Content $publish 'id-token: write' 'OIDC permission'
Require-Content $publish 'Cargo\.toml' 'Cargo version validation'
Require-Content $publish 'aquasecurity/trivy-action@b6643a29fecd7f34b3597bc6acb0a98b03d33ff8' 'Pinned Trivy scanner action'
Require-Content $publish 'version:\s*v0\.72\.0' 'Explicit supported Trivy scanner version'
Require-Content $publish 'TRIVY_PLATFORM:\s*\$\{\{\s*matrix\.platform\s*\}\}' 'Architecture-aware Trivy platform'
Require-Content $publish 'HIGH,CRITICAL' 'Trivy severity gate'
Require-Content $publish 'staging' 'Immutable staging tags'
Require-Content $publish 'github\.run_id' 'Publish attempt-unique staging run id'
Require-Content $publish 'github\.run_attempt' 'Publish attempt-unique staging run attempt'
Require-Content $publish 'container-smoke\.ps1' 'Publish staged-image smoke test'
Require-Content $publish '-Dev' 'Publish development staged-image smoke test'
if ($publish -match 'windows-2022|windows/amd64|-Platform windows') {
    throw 'Publish workflow must only advertise platforms available on GitHub-hosted container runners'
}
Require-Content $publish 'upload-artifact' 'Platform digest artifact upload'
Require-Content $publish 'needs:.*scan' 'Promotion waits for scan jobs'
Require-Content $publish 'imagetools create' 'Multi-platform index promotion'
Require-Content $publish 'refs=\$\(find digests' 'Publish promotion consumes recorded digest artifacts'
Require-Content $publish 'X\.Y\.Z-r0|r0' 'Initial stable revision tag'
Require-Content $publish 'attest-build-provenance' 'Build provenance attestation'
Require-Content $publish 'sbom(=|:)\s*true' 'Publish Buildx SBOM output'
Require-Content $publish 'provenance(=|:)\s*true' 'Publish Buildx provenance output'
Require-Content $publish 'org\.opencontainers\.image\.source' 'OCI source label'
Require-Content $publish 'org\.opencontainers\.image\.title' 'OCI title label'
Require-Content $publish 'org\.opencontainers\.image\.description' 'OCI description label'
Require-Content $publish 'org\.opencontainers\.image\.licenses' 'OCI license label'
Require-Content $publish 'org\.opencontainers\.image\.version' 'OCI version label'
Require-Content $publish 'org\.opencontainers\.image\.revision' 'OCI revision label'
Require-Content $publish '--build-arg VIBRA_VERSION=' 'Embedded release version build argument'
Require-Content $publish '--build-arg VIBRA_REVISION=' 'Embedded source revision build argument'
Require-Content $publish '--build-arg SOURCE_DATE_EPOCH=' 'Reproducible source epoch build argument'
Require-Content $publish 'gh attestation verify.*oci://' 'Promoted image attestation verification'
Require-Content $publish 'docker run --rm.*RUNTIME_IMAGE.*promoted\.outputs\.runtime.*--version' 'Digest-pinned runtime example verification'
Require-Content $publish '11bd71901bbe5b1630ceea73d27597364c9af683' 'Pinned checkout action'
Require-Content $publish '9780b0c442fbb1117ed29e0efdff1e18412f7567' 'Pinned Docker login action'
Require-Content $publish '988b5a0280414f521da01fcc63a27aeeb4b104db' 'Pinned Buildx action'

Require-Content $rebuild 'schedule:' 'Monthly rebuild schedule'
Require-Content $rebuild 'workflow_dispatch:' 'Manual rebuild trigger'
Require-Content $rebuild 'ref: main' 'Rebuild reviewed default-branch base configuration checkout'
Require-Content $rebuild 'sparse-checkout: containers/base-images\.env' 'Rebuild base-image configuration-only checkout'
Require-Content $rebuild 'base-config/containers/base-images\.env' 'Rebuild base-image configuration consumption'
Require-Content $rebuild 'source "\$BASE_IMAGES_FILE"' 'Rebuild base-image environment loading'
Require-Content $rebuild '--build-arg' 'Rebuild Docker build-argument override'
Require-Content $rebuild 'v\[0-9\]' 'Latest stable tag selection'
Require-Content $rebuild 'r\$' 'Next revision calculation'
Require-Content $rebuild '^concurrency:' 'Serialized rebuild concurrency'
Require-Content $rebuild '/users/\$\{GITHUB_REPOSITORY_OWNER\}/packages/container/\$1/versions\?per_page=100' 'User-scoped GHCR package metadata revision lookup'
if (Select-String -LiteralPath $rebuild -Pattern '/orgs/\$\{GITHUB_REPOSITORY_OWNER\}/packages/container' -Quiet) { throw 'Rebuild must use the user-scoped GHCR metadata endpoint for this repository owner.' }
if (Select-String -LiteralPath $rebuild -Pattern 'git ls-remote.*-r' -Quiet) { throw 'Rebuild revisions must be derived from GHCR tags, not Git tags.' }
Require-Content $rebuild 'grep -Fx.*revision' 'Pre-promotion immutable revision collision guard'
Require-Content $rebuild 'X\.Y\.Z.*not.*overwrite|immutable exact' 'Immutable exact-tag protection'
Require-Content $rebuild 'HIGH,CRITICAL' 'Rebuild Trivy severity gate'
Require-Content $rebuild 'github\.run_id' 'Rebuild attempt-unique staging run id'
Require-Content $rebuild 'github\.run_attempt' 'Rebuild attempt-unique staging run attempt'
Require-Content $rebuild 'aquasecurity/trivy-action@b6643a29fecd7f34b3597bc6acb0a98b03d33ff8' 'Pinned rebuild Trivy scanner action'
Require-Content $rebuild 'imagetools create' 'Rebuild manifest promotion'
Require-Content $rebuild 'refs=\$\(find digests' 'Rebuild promotion consumes recorded digest artifacts'
Require-Content $rebuild 'latest' 'Moving latest alias'
Require-Content $rebuild 'container-smoke\.ps1' 'Rebuild runtime smoke test'
Require-Content $rebuild '-Dev' 'Rebuild development-image smoke test'
Require-Content $rebuild '-Platform windows' 'Rebuild Windows staged-image smoke test'
Require-Content $rebuild 'driver:\s*\$\{\{.*Windows.*docker' 'Rebuild Windows-native Buildx driver'
Require-Content $rebuild 'attest-build-provenance' 'Rebuild provenance attestation'
Require-Content $rebuild 'sbom(=|:)\s*true' 'Rebuild Buildx SBOM output'
Require-Content $rebuild 'provenance(=|:)\s*true' 'Rebuild Buildx provenance output'
Require-Content $rebuild 'org\.opencontainers\.image\.source' 'Rebuild OCI source label'
Require-Content $rebuild 'org\.opencontainers\.image\.version' 'Rebuild OCI version label'
Require-Content $rebuild 'org\.opencontainers\.image\.revision' 'Rebuild OCI revision label'
Require-Content $rebuild 'git -C source rev-parse HEAD' 'Rebuild release source revision'
Require-Content $rebuild '--build-arg VIBRA_VERSION=' 'Rebuild embedded release version'
Require-Content $rebuild '--build-arg VIBRA_REVISION=' 'Rebuild embedded source revision'
Require-Content $rebuild '--build-arg SOURCE_DATE_EPOCH=' 'Rebuild reproducible source epoch'
Require-Content $rebuild 'gh attestation verify.*oci://' 'Rebuilt image attestation verification'

Require-Content $dependabot 'package-ecosystem: github-actions' 'Dependabot GitHub Actions updates'
Require-Content $dependabot 'package-ecosystem: docker' 'Dependabot Docker updates'
Require-Content $dependabot 'interval: monthly' 'Monthly Dependabot schedule'

foreach ($workflow in @($ci, $publish, $rebuild)) {
    foreach ($match in (Select-String -LiteralPath $workflow -Pattern '^\s*-?\s*uses:\s*([^\s#]+)' -AllMatches)) {
        $reference = $match.Matches[0].Groups[1].Value
        if ($reference -notmatch '@[0-9a-f]{40}$') {
            throw "Action reference must be pinned to a full 40-character SHA in ${workflow}: $reference"
        }
    }
}

Write-Host 'Container workflow checks passed.'
