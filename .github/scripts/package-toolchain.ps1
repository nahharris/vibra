[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Binary,
    [Parameter(Mandatory)][ValidateSet('linux-amd64','linux-arm64','windows-amd64','macos-amd64','macos-arm64')][string]$Platform,
    [Parameter(Mandatory)][ValidateSet(
        'x86_64-unknown-linux-gnu',
        'aarch64-unknown-linux-gnu',
        'x86_64-pc-windows-msvc',
        'x86_64-apple-darwin',
        'aarch64-apple-darwin'
    )][string]$TargetTriple,
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$Revision,
    [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
$expectedTargets = @{
    'linux-amd64' = 'x86_64-unknown-linux-gnu'
    'linux-arm64' = 'aarch64-unknown-linux-gnu'
    'windows-amd64' = 'x86_64-pc-windows-msvc'
    'macos-amd64' = 'x86_64-apple-darwin'
    'macos-arm64' = 'aarch64-apple-darwin'
}
if ($expectedTargets[$Platform] -ne $TargetTriple) {
    throw "Platform $Platform requires target triple $($expectedTargets[$Platform]), not $TargetTriple."
}
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
target-triple: $TargetTriple
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
    $tarVersion = (& tar --version 2>&1 | Out-String)
    if ($tarVersion -match 'GNU tar') {
        & tar --sort=name --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner -czf $archive -C $OutputDirectory $name
    } else {
        # macOS ships bsdtar. Its archive preserves the executable bit but does
        # not accept GNU tar's reproducibility flags.
        & tar -czf $archive -C $OutputDirectory $name
    }
    if ($LASTEXITCODE -ne 0) { throw 'tar archive creation failed.' }
}
Write-Output $archive
