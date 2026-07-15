[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$Image,

    [Parameter(Mandatory)]
    [ValidateSet('linux', 'windows')]
    [string]$Platform,

    [switch]$Dev,
    [switch]$CheckOnly
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

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("vibra-container-smoke-" + [guid]::NewGuid())
$workspace = if ($Platform -eq 'linux') { '/workspace' } else { 'C:\workspace' }
$project = if ($Platform -eq 'linux') { '/workspace/smoke' } else { 'C:\workspace\smoke' }
$mount = "type=bind,src=$tempRoot,dst=$workspace"

try {
    New-Item -ItemType Directory -Path $tempRoot | Out-Null
    if ($Platform -eq 'linux') {
        Set-LinuxMountPermissions -Path $tempRoot
    }

    Invoke-Docker @('run', '--rm', $Image, '--version')
    Invoke-Docker @('run', '--rm', $Image, '--help')
    Invoke-Docker @('run', '--rm', '--mount', $mount, '-w', $workspace, $Image, 'init', 'smoke')
    Invoke-Docker @('run', '--rm', '--mount', $mount, '-w', $project, $Image, 'check')
    Invoke-Docker @('run', '--rm', '--mount', $mount, '-w', $project, $Image, 'run', 'src/smoke/main.vibra')

    if ($Dev) {
        if ($Platform -eq 'linux') {
            Invoke-Docker @('run', '--rm', '--entrypoint', '/bin/sh', $Image, '-lc', 'export PATH="$(cat /opt/vibra/cargo-bin):$PATH"; cd /src && cargo test && cargo run -- test')
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
