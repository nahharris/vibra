[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$Image,

    [Parameter(Mandatory)]
    [ValidateSet('linux', 'windows')]
    [string]$Platform,

    [switch]$Dev,
    [switch]$CheckOnly,

    [string]$ExpectedVersion,
    [string]$ExpectedRevision
)

$ErrorActionPreference = 'Stop'

function Set-LinuxMountPermissions {
    param(
        [string]$Path,
        [switch]$CheckOnly
    )

    if ($CheckOnly) {
        return
    }

    if (-not (Get-Command chmod -ErrorAction SilentlyContinue)) {
        throw 'chmod is required to make the Linux bind mount writable by runtime uid 65532.'
    }

    & chmod 777 $Path
    if ($LASTEXITCODE -ne 0) {
        throw "Could not make Linux bind mount writable: $Path"
    }
}

function Get-WindowsDevWorkdir {
    return 'C:\src'
}

if ($CheckOnly) {
    Set-LinuxMountPermissions -Path $null -CheckOnly
    Get-WindowsDevWorkdir | Out-Null
    Write-Host 'Container smoke test syntax and parameters are ready for Docker CI.'
    exit 0
}

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    throw 'Docker is required for container smoke tests but was not found on PATH.'
}

& docker info *> $null
if ($LASTEXITCODE -ne 0) {
    throw 'Docker is installed but the daemon is unavailable; start Docker before running container smoke tests.'
}

function Invoke-Docker {
    param([string[]]$Arguments)

    & docker @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Docker command failed: docker $($Arguments -join ' ')"
    }
}

function Assert-ReleaseContract {
    param(
        [string]$Image,
        [string]$Platform,
        [string]$ExpectedVersion,
        [string]$ExpectedRevision,
        [string]$TempRoot
    )

    if ([string]::IsNullOrEmpty($ExpectedVersion) -xor [string]::IsNullOrEmpty($ExpectedRevision)) {
        throw 'ExpectedVersion and ExpectedRevision must be supplied together.'
    }
    if ($ExpectedRevision -and $ExpectedRevision -notmatch '^[0-9a-f]{40}$') {
        throw "ExpectedRevision must be a lowercase 40-character Git SHA: $ExpectedRevision"
    }

    $containerName = "vibra-release-contract-$([guid]::NewGuid().ToString('N'))"
    $metadataPath = if ($Platform -eq 'linux') { '/opt/vibra/release.vibra' } else { 'C:\Vibra\release.vibra' }
    $metadataCopy = Join-Path $TempRoot 'release.vibra'
    try {
        & docker create --name $containerName $Image --version *> $null
        if ($LASTEXITCODE -ne 0) { throw "Could not create metadata probe container from $Image." }
        Invoke-Docker @('cp', "${containerName}:${metadataPath}", $metadataCopy)
    }
    finally {
        & docker rm --force $containerName *> $null
    }

    $metadata = Get-Content -Raw -LiteralPath $metadataCopy
    foreach ($field in @('format-version: 1', 'version:', 'revision:', 'stdlib-rev:', 'stdlib-sha256:')) {
        if ($metadata -notmatch "(?m)^$([regex]::Escape($field))") {
            throw "Image release metadata is missing $field"
        }
    }

    if ($ExpectedVersion) {
        if ($metadata -notmatch "(?m)^version: $([regex]::Escape($ExpectedVersion))$") {
            throw "Embedded version does not match $ExpectedVersion."
        }
        if ($metadata -notmatch "(?m)^revision: $ExpectedRevision$") {
            throw "Embedded revision does not match $ExpectedRevision."
        }

        $versionLabel = & docker image inspect --format '{{ index .Config.Labels "org.opencontainers.image.version" }}' $Image
        if ($LASTEXITCODE -ne 0 -or $versionLabel -ne $ExpectedVersion) {
            throw "OCI version label does not match $ExpectedVersion."
        }
        $revisionLabel = & docker image inspect --format '{{ index .Config.Labels "org.opencontainers.image.revision" }}' $Image
        if ($LASTEXITCODE -ne 0 -or $revisionLabel -ne $ExpectedRevision) {
            throw "OCI revision label does not match $ExpectedRevision."
        }
    }
}

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("vibra-container-smoke-" + [guid]::NewGuid())
$workspace = if ($Platform -eq 'linux') { '/workspace' } else { 'C:\workspace' }
$project = if ($Platform -eq 'linux') { '/workspace/smoke' } else { 'C:\workspace\smoke' }
$mount = "type=bind,src=$tempRoot,dst=$workspace"

try {
    New-Item -ItemType Directory -Path $tempRoot | Out-Null
    if ($Platform -eq 'linux') {
        Set-LinuxMountPermissions -Path $tempRoot
    }

    Assert-ReleaseContract -Image $Image -Platform $Platform -ExpectedVersion $ExpectedVersion -ExpectedRevision $ExpectedRevision -TempRoot $tempRoot
    Invoke-Docker @('run', '--rm', $Image, '--version')
    Invoke-Docker @('run', '--rm', $Image, '--help')
    Invoke-Docker @('run', '--rm', '--mount', $mount, '-w', $workspace, $Image, 'init', 'smoke')
    Invoke-Docker @('run', '--rm', '--mount', $mount, '-w', $project, $Image, 'check')
    Invoke-Docker @('run', '--rm', '--mount', $mount, '-w', $project, $Image, 'run', 'src/smoke/main.vibra')

    if ($Dev) {
        if ($Platform -eq 'linux') {
            Invoke-Docker @('run', '--rm', '--entrypoint', '/bin/sh', $Image, '-lc', 'export PATH="$(cat /opt/vibra/cargo-bin):$PATH"; cargo --version; git --version; probe="$(mktemp -d)/probe"; cargo new --quiet "$probe"; cargo check --quiet --offline --manifest-path "$probe/Cargo.toml"')
        }
        else {
            Invoke-Docker @('run', '--rm', '--entrypoint', 'cmd.exe', '-w', 'C:\src', $Image, '/S', '/C', 'C:\Vibra\devcmd.cmd cmd /S /C "cargo test && cargo run -- test"')
        }
    }
}
finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "Container smoke test passed for $Image ($Platform)."
