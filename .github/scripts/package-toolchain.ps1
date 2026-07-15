[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Binary,
    [Parameter(Mandatory)][ValidateSet('linux-amd64','linux-arm64','windows-amd64')][string]$Platform,
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$Revision,
    [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
# Canonical relocatable layout: bin/vibra[.exe] beside stdlib/.
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$stdlib = Join-Path $root 'stdlib'
$constants = Get-Content -Raw (Join-Path $root 'src/project.rs')
$stdlibRev = [regex]::Match($constants, 'STDLIB_REV: &str = "([0-9a-f]{40})"').Groups[1].Value
if (-not $stdlibRev) { throw 'Could not read STDLIB_REV from src/project.rs.' }

function Get-TreeDigest([string]$Path) {
    [string[]]$lines = Get-ChildItem -LiteralPath $Path -File -Recurse | ForEach-Object {
        $relative = [IO.Path]::GetRelativePath($Path, $_.FullName).Replace('\','/')
        "$relative $((Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant())"
    }
    [Array]::Sort($lines, [StringComparer]::Ordinal)
    $bytes = [Text.Encoding]::UTF8.GetBytes(($lines -join "`n") + "`n")
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([Convert]::ToHexString($sha.ComputeHash($bytes))).ToLowerInvariant() } finally { $sha.Dispose() }
}

$name = "vibra-$Version-$Platform"
$stage = Join-Path $OutputDirectory $name
Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force (Join-Path $stage 'bin'), (Join-Path $stage 'stdlib') | Out-Null
$exe = if ($Platform -eq 'windows-amd64') { 'vibra.exe' } else { 'vibra' }
Copy-Item -LiteralPath $Binary -Destination (Join-Path (Join-Path $stage 'bin') $exe)
Copy-Item -Path (Join-Path $stdlib '*') -Destination (Join-Path $stage 'stdlib') -Recurse
Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination $stage
$digest = Get-TreeDigest $stdlib
@"
format-version: 1
version: $Version
revision: $Revision
platform: $Platform
stdlib-git: https://github.com/nahharris/vibra-stdlib.git
stdlib-rev: $stdlibRev
stdlib-sha256: $digest
"@ | Set-Content -LiteralPath (Join-Path $stage 'release.vibra') -Encoding utf8NoBOM

if ($Platform -eq 'windows-amd64') {
    $archive = Join-Path $OutputDirectory "$name.zip"
    Remove-Item -LiteralPath $archive -Force -ErrorAction SilentlyContinue
    Compress-Archive -LiteralPath $stage -DestinationPath $archive -CompressionLevel Optimal
} else {
    $archive = Join-Path $OutputDirectory "$name.tar.gz"
    & tar --sort=name --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner -czf $archive -C $OutputDirectory $name
    if ($LASTEXITCODE -ne 0) { throw 'tar archive creation failed.' }
}
Write-Output $archive
