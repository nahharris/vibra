[CmdletBinding()]
param([Parameter(Mandatory)][string]$Archive, [Parameter(Mandatory)][ValidateSet('linux-amd64','linux-arm64','windows-amd64')][string]$Platform)
$ErrorActionPreference = 'Stop'
$temp = Join-Path ([IO.Path]::GetTempPath()) ("vibra-toolchain-smoke-" + [guid]::NewGuid())
try {
    New-Item -ItemType Directory $temp | Out-Null
    if ($Archive.EndsWith('.zip')) { Expand-Archive -LiteralPath $Archive -DestinationPath $temp }
    else { & tar -xzf $Archive -C $temp; if ($LASTEXITCODE -ne 0) { throw 'archive extraction failed' } }
    $root = Get-ChildItem -LiteralPath $temp -Directory | Select-Object -First 1
    $relocated = Join-Path $temp 'relocated-toolchain'
    Move-Item -LiteralPath $root.FullName -Destination $relocated
    $binRelative = if ($Platform -eq 'windows-amd64') { 'bin/vibra.exe' } else { 'bin/vibra' }
    $exe = Join-Path $relocated $binRelative
    function Vibra([string[]]$Arguments) { & $exe @Arguments; if ($LASTEXITCODE -ne 0) { throw "vibra failed: $Arguments" } }
    Push-Location $temp
    Vibra @('--version')
    Vibra @('exec', '"archive-ok"')
    Vibra @('init', 'smoke')
    Push-Location (Join-Path $temp 'smoke')
    Vibra @('check')
    Vibra @('run', 'src/smoke/main.vibra')
    Vibra @('build', '.', '--output', 'smoke.vapp')
    Vibra @('package', 'verify', 'smoke.vapp')
    Vibra @('run', 'smoke.vapp')
    Pop-Location
    Add-Content -LiteralPath (Join-Path $relocated 'stdlib/src/io.vibra') -Value "`ntampered: true"
    & $exe init damaged 2>&1 | Out-String -OutVariable damagedError | Out-Null
    if ($LASTEXITCODE -eq 0 -or $damagedError -notmatch 'E-DIST-004') { throw 'Tampered stdlib did not fail with E-DIST-004.' }
    Remove-Item -LiteralPath (Join-Path $relocated 'release.vibra')
    & $exe init missing-metadata 2>&1 | Out-String -OutVariable missingError | Out-Null
    if ($LASTEXITCODE -eq 0 -or $missingError -notmatch 'E-DIST-002') { throw 'Missing release metadata did not fail with E-DIST-002.' }
    Pop-Location
} finally {
    while ((Get-Location).Path.StartsWith($temp)) { Pop-Location }
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Host "Relocated toolchain smoke passed for $Platform."
