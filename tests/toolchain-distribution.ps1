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
Require $pack 'release\.vibra' 'release metadata'

Require $smoke "'init'" 'init smoke'
Require $smoke "'exec'" 'exec smoke'
Require $smoke "'check'" 'check smoke'
Require $smoke "'run'" 'source run smoke'
Require $smoke "'build'" '.vapp build smoke'
Require $smoke "'package', 'verify'" '.vapp verify smoke'
Require $smoke '\.vapp' '.vapp run smoke'
Require $smoke 'Move-Item' 'relocation smoke'

Write-Host 'Toolchain distribution contracts passed.'
