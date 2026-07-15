$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$readme = Get-Content -Raw (Join-Path $repoRoot 'README.md')
$guidePath = Join-Path $repoRoot 'docs/containers.md'

if (-not (Test-Path $guidePath)) {
    throw 'Container guide is missing: docs/containers.md'
}

$guide = Get-Content -Raw $guidePath

foreach ($needle in @(
    'ghcr.io/nahharris/vibra',
    'ghcr.io/nahharris/vibra-dev',
    'Linux amd64',
    'Linux arm64',
    'Windows LTSC 2022 amd64',
    'Windows ARM',
    'vibra --help',
    '/opt/vibra/stdlib',
    'C:\Vibra\stdlib',
    'https://github.com/nahharris/vibra/issues/15',
    'docker desktop',
    'Windows containers',
    'gh attestation verify',
    'Dependabot',
    '-rN',
    'docker run',
    'GITHUB_TOKEN',
    'runs-on: ubuntu-latest',
    '-v "$PWD:/work" -w /work'
)) {
    if ($guide -notmatch [regex]::Escape($needle)) {
        throw "Container guide is missing required content: $needle"
    }
}

if ($guide -match 'container:\s*ghcr\.io/nahharris/vibra') {
    throw 'The distroless runtime image cannot be used as a GitHub Actions job container.'
}

if ($guide -match 'After a successful rebuild, `X\.Y\.Z`, `X\.Y`') {
    throw 'Exact X.Y.Z must not move to a later container rebuild revision.'
}

if ($guide -notmatch 'X\.Y\.Z` remains the initial immutable r0/source\s+tag') {
    throw 'The guide must state that X.Y.Z remains the immutable r0/source tag.'
}

if ($readme -notmatch '\[Container images\]\(docs/containers\.md\)') {
    throw 'README must link to docs/containers.md as Container images.'
}

Write-Host 'Container documentation checks passed.'
