$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot

function Require-Content {
    param(
        [string]$Path,
        [string]$Pattern,
        [string]$Description
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "$Description is missing: $Path"
    }

    if (-not (Select-String -LiteralPath $Path -Pattern $Pattern -Quiet)) {
        throw "$Description is missing from $Path"
    }
}

$cargoToml = Join-Path $repoRoot 'Cargo.toml'
$toolchain = Join-Path $repoRoot 'rust-toolchain.toml'
$linuxDockerfile = Join-Path $repoRoot 'containers/Dockerfile.linux'
$windowsDockerfile = Join-Path $repoRoot 'containers/Dockerfile.windows'
$dockerIgnore = Join-Path $repoRoot '.dockerignore'
$smokeTest = Join-Path $repoRoot 'tests/container-smoke.ps1'
$baseImages = Join-Path $repoRoot 'containers/base-images.env'

Require-Content $cargoToml 'rust-version\s*=\s*"1\.94\.1"' 'Cargo Rust version pin'
Require-Content $toolchain 'channel\s*=\s*"1\.94\.1"' 'Rust toolchain channel pin'
Require-Content $toolchain 'profile\s*=\s*"minimal"' 'Rust toolchain minimal profile'

Require-Content $linuxDockerfile 'COPY.*stdlib.*\/opt\/vibra\/stdlib' 'Linux stdlib runtime copy'
Require-Content $linuxDockerfile 'release\.vibra.*\/opt\/vibra\/release\.vibra' 'Linux release metadata copy'
Require-Content $linuxDockerfile '\/opt\/vibra\/bin' 'Linux binary layout'
Require-Content $linuxDockerfile 'ARG RUST_BUILDER_IMAGE=rust:1\.94\.1-slim-bookworm@sha256:cf9dd0ec73e75f827fe59123fff9dc65af1a1c8363c3c31ee8d7f8ad0b6a5fb2' 'Pinned Linux Rust builder build argument default'
Require-Content $linuxDockerfile 'FROM \$\{RUST_BUILDER_IMAGE\} AS builder' 'Linux Rust builder build argument use'
Require-Content $linuxDockerfile 'apt-get install.*build-essential.*git.*libssl-dev.*pkg-config' 'Linux native build dependencies'
Require-Content $linuxDockerfile 'FROM builder AS dev' 'Linux development image stage'
Require-Content $linuxDockerfile 'cargo --version' 'Linux development Cargo availability check'
Require-Content $linuxDockerfile 'git --version' 'Linux development Git availability check'
Require-Content $linuxDockerfile 'ARG LINUX_RUNTIME_IMAGE=gcr\.io\/distroless\/cc-debian12:nonroot@sha256:ce0d66bc0f64aae46e6a03add867b07f42cc7b8799c949c2e898057b7f75a151' 'Pinned Linux runtime build argument default'
Require-Content $linuxDockerfile 'FROM \$\{LINUX_RUNTIME_IMAGE\} AS runtime' 'Linux runtime build argument use'
Require-Content $linuxDockerfile '^USER nonroot$' 'Linux runtime nonroot user'
Require-Content $linuxDockerfile 'ENTRYPOINT.*vibra' 'Linux vibra entrypoint'
Require-Content $linuxDockerfile 'CMD.*--help' 'Linux help default'

$linuxContents = Get-Content -Raw $linuxDockerfile
$dependencyBuild = $linuxContents.IndexOf('cargo build --release --locked')
$sourceCopy = $linuxContents.IndexOf('COPY src ./src')
if ($dependencyBuild -lt 0 -or $sourceCopy -lt 0 -or $dependencyBuild -gt $sourceCopy) {
    throw 'Linux Dockerfile must cache dependency compilation before copying real sources.'
}

Require-Content $windowsDockerfile 'COPY.*stdlib.*C:\\Vibra\\stdlib' 'Windows stdlib runtime copy'
Require-Content $windowsDockerfile 'release\.vibra.*C:\\Vibra\\release\.vibra' 'Windows release metadata copy'
Require-Content $windowsDockerfile 'C:\\Vibra\\bin' 'Windows binary layout'
Require-Content $windowsDockerfile 'ARG WINDOWS_BUILDER_IMAGE=mcr\.microsoft\.com/windows/servercore:ltsc2022-amd64@sha256:c747aa0e4668af32773f6f81220fb95d49e2e55237d07fbc18a8138d1b0e4de7' 'Pinned Windows builder build argument default'
Require-Content $windowsDockerfile 'FROM \$\{WINDOWS_BUILDER_IMAGE\} AS builder' 'Windows builder build argument use'
Require-Content $windowsDockerfile 'default-toolchain 1\.94\.1-x86_64-pc-windows-msvc' 'Windows Rust 1.94.1 installation'
Require-Content $windowsDockerfile 'Git-2\.47\.1-64-bit\.exe' 'Windows Git installation'
Require-Content $windowsDockerfile 'Get-AuthenticodeSignature' 'Windows installer Authenticode verification'
Require-Content $windowsDockerfile 'Rust Project Developers' 'Rustup trusted publisher verification'
Require-Content $windowsDockerfile 'Microsoft Corporation' 'Visual Studio trusted publisher verification'
Require-Content $windowsDockerfile 'Johannes Schindelin' 'Git trusted publisher verification'
Require-Content $windowsDockerfile 'RUSTFLAGS="-C target-feature=\+crt-static"' 'Windows static CRT build flag'
Require-Content $windowsDockerfile 'FROM builder AS dev' 'Windows development image stage'
Require-Content $windowsDockerfile 'C:\\Vibra\\devcmd\.cmd' 'Windows development command wrapper'
Require-Content $windowsDockerfile 'VsDevCmd\.bat.*-arch=x64' 'Windows x64 developer command initialization'
Require-Content $windowsDockerfile 'cargo --version' 'Windows development Cargo availability check'
Require-Content $windowsDockerfile 'rustc --version' 'Windows development rustc availability check'
Require-Content $windowsDockerfile 'git --version' 'Windows development Git availability check'
Require-Content $windowsDockerfile 'ARG WINDOWS_RUNTIME_IMAGE=mcr\.microsoft\.com/windows/nanoserver:ltsc2022-amd64@sha256:132929c393e0d188628033604fe7a6ec4a5fc446546d90c2226bb56ce48d4c9a' 'Pinned Windows runtime build argument default'
Require-Content $windowsDockerfile 'FROM \$\{WINDOWS_RUNTIME_IMAGE\} AS runtime' 'Windows runtime build argument use'
Require-Content $windowsDockerfile '^USER ContainerUser$' 'Windows runtime ContainerUser'
Require-Content $windowsDockerfile 'ENTRYPOINT.*vibra\.exe' 'Windows vibra.exe entrypoint'
Require-Content $windowsDockerfile 'CMD.*--help' 'Windows help default'

Require-Content $dockerIgnore '^target$' 'Docker target exclusion'
Require-Content $dockerIgnore '^\.git$' 'Docker Git exclusion'
Require-Content $dockerIgnore '^\.worktrees$' 'Docker worktree exclusion'

Require-Content $baseImages '^RUST_BUILDER_IMAGE=rust:1\.94\.1-slim-bookworm@sha256:cf9dd0ec73e75f827fe59123fff9dc65af1a1c8363c3c31ee8d7f8ad0b6a5fb2$' 'Scheduled rebuild Linux builder base digest'
Require-Content $baseImages '^LINUX_RUNTIME_IMAGE=gcr\.io/distroless/cc-debian12:nonroot@sha256:ce0d66bc0f64aae46e6a03add867b07f42cc7b8799c949c2e898057b7f75a151$' 'Scheduled rebuild Linux runtime base digest'
Require-Content $baseImages '^WINDOWS_BUILDER_IMAGE=mcr\.microsoft\.com/windows/servercore:ltsc2022-amd64@sha256:c747aa0e4668af32773f6f81220fb95d49e2e55237d07fbc18a8138d1b0e4de7$' 'Scheduled rebuild Windows builder base digest'
Require-Content $baseImages '^WINDOWS_RUNTIME_IMAGE=mcr\.microsoft\.com/windows/nanoserver:ltsc2022-amd64@sha256:132929c393e0d188628033604fe7a6ec4a5fc446546d90c2226bb56ce48d4c9a$' 'Scheduled rebuild Windows runtime base digest'

Require-Content $smokeTest 'param\(' 'Container smoke test parameters'
Require-Content $smokeTest 'ValidateSet\(''linux'', ''windows''\)' 'Container smoke test platform validation'
Require-Content $smokeTest 'docker info' 'Container smoke test Docker availability check'
Require-Content $smokeTest '& docker @Arguments' 'Container smoke test execution'
Require-Content $smokeTest '--mount' 'Container smoke test isolated bind mount'
Require-Content $smokeTest '--entrypoint' 'Container smoke test development entrypoint override'
Require-Content $smokeTest "'init', 'smoke'" 'Container smoke test project initialization'
Require-Content $smokeTest "'check'" 'Container smoke test project check'
Require-Content $smokeTest 'cargo test' 'Container smoke test development Rust suite'
Require-Content $smokeTest 'cargo run -- test' 'Container smoke test development Vibra suite'
Require-Content $smokeTest 'cargo new --quiet' 'Linux development image Cargo project smoke test'
Require-Content $smokeTest 'cargo check --quiet --offline' 'Linux development image offline build smoke test'
Require-Content $smokeTest 'Set-LinuxMountPermissions' 'Container smoke test Linux mount permission helper'
Require-Content $smokeTest 'chmod 777' 'Container smoke test Linux writable mount permissions'
Require-Content $smokeTest "'-w', 'C:\\src'" 'Container smoke test Windows dev source workdir'

Write-Host 'Container layout checks passed.'
