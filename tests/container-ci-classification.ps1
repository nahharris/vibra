$ErrorActionPreference = 'Stop'

$classifier = Join-Path (Split-Path -Parent $PSScriptRoot) '.github/scripts/classify-container-ci-changes.ps1'
if (-not (Test-Path -LiteralPath $classifier)) {
    throw "CI path classifier is missing: $classifier"
}

function Assert-Scope {
    param([string[]]$Paths, [bool]$Source, [bool]$Image)
    $actual = & $classifier -Paths $Paths
    if ($actual.Source -ne $Source -or $actual.Image -ne $Image) {
        throw "Unexpected scope for [$($Paths -join ', ')]: source=$($actual.Source), image=$($actual.Image)"
    }
}

Assert-Scope @('docs/containers.md') $false $false
Assert-Scope @('src/main.rs') $true $true
Assert-Scope @('stdlib/src/io.vibra') $true $true
Assert-Scope @('tests/lang-values.vibra') $true $false
Assert-Scope @('containers/Dockerfile.linux') $false $true
Assert-Scope @('tests/container-smoke.ps1') $false $true
Assert-Scope @('.github/workflows/container-ci.yml') $false $true
Assert-Scope @('.github/workflows/publish-containers.yml') $false $false
Assert-Scope @('tests/container-ci-classification.ps1') $false $false
Assert-Scope @('docs/containers.md', 'src/main.rs') $true $true
Assert-Scope @('unclassified.file') $true $true

Write-Host 'Container CI change classification checks passed.'
