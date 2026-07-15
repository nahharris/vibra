[CmdletBinding()]
param([Parameter(Mandatory)][string[]]$Paths)

$source = $false
$image = $false

foreach ($rawPath in $Paths) {
    $path = $rawPath.Replace('\', '/')

    if ($path -match '^(Cargo\.toml|Cargo\.lock|rust-toolchain\.toml|src/|stdlib/|schemas/|examples/)') {
        $source = $true
        $image = $true
        continue
    }

    if ($path -match '^(containers/|\.dockerignore$|tests/container-(layout|smoke)\.ps1$|\.github/workflows/container-ci\.yml$|\.github/scripts/classify-container-ci-changes\.ps1$)') {
        $image = $true
        continue
    }

    if ($path -match '^(docs/|README\.md$|LICENSE|AGENTS\.md$|\.github/dependabot\.yml$|\.github/workflows/(publish|rebuild)-containers\.yml$|tests/container-(docs|workflows|ci-classification)\.ps1$)') {
        continue
    }

    if ($path -match '^tests/') {
        $source = $true
        continue
    }

    # Unknown files are conservatively assumed to affect both products.
    $source = $true
    $image = $true
}

[pscustomobject]@{ Source = $source; Image = $image }
