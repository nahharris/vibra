$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

function Require([string]$Path, [string]$Pattern, [string]$Label) {
    $full = Join-Path $root $Path
    if (-not (Test-Path -LiteralPath $full)) { throw "$Label is missing: $Path" }
    if (-not (Select-String -LiteralPath $full -Pattern $Pattern -Quiet)) { throw "$Label is missing from $Path" }
}

function Reject([string]$Path, [string]$Pattern, [string]$Label) {
    $full = Join-Path $root $Path
    if (Select-String -LiteralPath $full -Pattern $Pattern -Quiet) { throw "$Label in $Path" }
}

$workflow = '.github/workflows/publish-toolchains.yml'
$pack = '.github/scripts/package-toolchain.ps1'
$smoke = 'tests/toolchain-smoke.ps1'

Require $workflow "tags: \['v\*'\]" 'tag release trigger'
Require $workflow 'linux-amd64' 'Linux amd64 archive'
Require $workflow 'linux-arm64' 'Linux arm64 archive'
Require $workflow 'windows-amd64' 'Windows amd64 archive'
Require $workflow 'macos-amd64' 'macOS amd64 archive'
Require $workflow 'macos-arm64' 'macOS arm64 archive'
Require $workflow 'macos-15-intel' 'macOS Intel release runner'
Require $workflow 'macos-15' 'macOS arm64 release runner'
Require $workflow 'cargo test' 'Rust release gate'
Require $workflow 'cargo run.*-- test' 'Vibra release gate'
Require $workflow 'package-toolchain\.ps1' 'archive packager'
Require $workflow 'toolchain-smoke\.ps1' 'relocation smoke'
Require $workflow 'attest-build-provenance' 'GitHub build provenance'
Require $workflow 'sha256sums\.txt' 'published checksums'
Require $workflow 'softprops/action-gh-release' 'GitHub release asset upload'

$rebuild = '.github/workflows/rebuild-containers.yml'
Require $rebuild '^\s+image-ref: ghcr\.io/nahharris/vibra:staging-' 'runtime rebuild scan target'
Require $rebuild '^\s+image-ref: ghcr\.io/nahharris/vibra-dev:staging-' 'dev rebuild scan target'
Reject $rebuild 'with: \{\s*image-ref:.*:' 'unquoted tagged image in YAML flow mapping'
Reject $rebuild 'with: \{.*\$\{\{' 'expression in YAML flow mapping'

Require $pack "bin[/\\]vibra" 'toolchain binary layout'
Require $pack "stdlib" 'bundled stdlib layout'
Require $pack 'STDLIB_REV' 'stdlib revision metadata'
Require $pack 'Get-FileHash' 'stdlib digest metadata'
Require $pack 'LICENSE' 'license inclusion'
Require $pack 'release\.json' 'release metadata'
Require $pack 'target-triple' 'Rust target triple metadata'
Require $pack 'GNU tar' 'GNU tar packaging path'
Require $pack 'tar -czf' 'portable bsdtar packaging path'

$targetPath = Join-Path $root 'distribution/targets.json'
$targets = (Get-Content -Raw -LiteralPath $targetPath | ConvertFrom-Json).targets
$expectedPlatforms = @(
    'linux-amd64',
    'linux-arm64',
    'windows-amd64',
    'macos-amd64',
    'macos-arm64'
)
if ($targets.Count -ne $expectedPlatforms.Count) {
    throw "Expected $($expectedPlatforms.Count) native distribution targets, found $($targets.Count)."
}
$seenPlatforms = @{}
$seenTriples = @{}
$releaseSchema = Get-Content -Raw -LiteralPath (Join-Path $root 'schemas/release-metadata.schema.json') | ConvertFrom-Json
foreach ($target in $targets) {
    if ($target.platform -notin $expectedPlatforms) { throw "Unknown distribution platform: $($target.platform)" }
    if ($seenPlatforms[$target.platform]) { throw "Duplicate distribution platform: $($target.platform)" }
    if ($seenTriples[$target.'target-triple']) { throw "Duplicate distribution target triple: $($target.'target-triple')" }
    $seenPlatforms[$target.platform] = $true
    $seenTriples[$target.'target-triple'] = $true
    if ($target.'support-tier' -ne 'tier-1') { throw "Published target $($target.platform) must be tier-1." }
    if ($target.platform -notin $releaseSchema.properties.platform.enum) {
        throw "Release metadata schema omits platform $($target.platform)."
    }
    if ($target.'target-triple' -notin $releaseSchema.properties.'target-triple'.enum) {
        throw "Release metadata schema omits target triple $($target.'target-triple')."
    }
    Require $workflow ([regex]::Escape($target.platform)) "release matrix target $($target.platform)"
    Require $workflow ([regex]::Escape($target.runner)) "release runner $($target.runner)"
    Require $workflow ([regex]::Escape($target.'target-triple')) "release target triple $($target.'target-triple')"
}

$ci = '.github/workflows/ci.yml'
foreach ($target in $targets) {
    Require $ci ([regex]::Escape($target.platform)) "main CI target $($target.platform)"
    Require $ci ([regex]::Escape($target.runner)) "main CI runner $($target.runner)"
}
Require 'tests/vapp-repro.ps1' 'vibraPath run sample\.vapp' 'native .vapp execution smoke'

Require $smoke "'init'" 'init smoke'
Require $smoke "'exec'" 'exec smoke'
Require $smoke "'check'" 'check smoke'
Require $smoke "'run'" 'source run smoke'
Require $smoke "'build'" '.vapp build smoke'
Require $smoke "'package', 'verify'" '.vapp verify smoke'
Require $smoke '\.vapp' '.vapp run smoke'
Require $smoke 'Move-Item' 'relocation smoke'

Write-Host 'Toolchain distribution contracts passed.'
