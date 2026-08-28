[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Vibra,
    [Parameter(Mandatory)][string]$Output
)

$ErrorActionPreference = 'Stop'
$vibraPath = (Resolve-Path -LiteralPath $Vibra).Path
$outputPath = [IO.Path]::GetFullPath($Output)
$temp = Join-Path ([IO.Path]::GetTempPath()) ("vibra-vapp-repro-" + [guid]::NewGuid())

function Build-Fixture([string]$Root) {
    New-Item -ItemType Directory -Path $Root | Out-Null
    Push-Location $Root
    try {
        & $vibraPath init sample | Out-Host
        if ($LASTEXITCODE -ne 0) { throw 'vibra init failed' }
        Push-Location sample
        try {
            & $vibraPath build . --output sample.vapp | Out-Host
            if ($LASTEXITCODE -ne 0) { throw 'vibra build failed' }
            & $vibraPath package verify sample.vapp | Out-Host
            if ($LASTEXITCODE -ne 0) { throw 'vibra package verify failed' }
            & $vibraPath run sample.vapp | Out-Host
            if ($LASTEXITCODE -ne 0) { throw 'vibra .vapp execution failed' }
            return (Resolve-Path -LiteralPath sample.vapp).Path
        } finally {
            Pop-Location
        }
    } finally {
        Pop-Location
    }
}

try {
    $first = Build-Fixture (Join-Path $temp first)
    $second = Build-Fixture (Join-Path $temp second)
    $firstHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $first).Hash
    $secondHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $second).Hash
    if ($firstHash -ne $secondHash) {
        throw "Repeated .vapp builds differ: $firstHash != $secondHash"
    }
    New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($outputPath)) | Out-Null
    Copy-Item -LiteralPath $first -Destination $outputPath
    Write-Host "Deterministic .vapp SHA-256: $firstHash"
} finally {
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
}
