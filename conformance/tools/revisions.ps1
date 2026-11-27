<#
.SYNOPSIS
Computes the workspace revision of every conformance case, and writes or
checks the `revision` its expected observations hold.

.DESCRIPTION
The revision is computed by the formula of the tooling chapter
(docs/spec/05-tooling.md, "Snapshot, claim and witness identity") from the
case's input bytes and the toolchain record, with no code of the engine:

- the `project` record is `project.vibon` at the root of the case's tree;
- a `source` record is each file with the exact extension `.vib` below a
  target root the project names (`root: "..."`) or below `tests/`, its
  identifier the path from the tree's root with `/` between names;
- each `@path` dependency, followed breadth-first from the root project
  through every project file reached, its `path` relative to the directory
  of the project file that declares it: a `dependency` record holds the
  exact bytes of that directory's `project.vibon`, identified by its path
  from the tree's root, and a `source` record each `.vib` file below the
  root of the library target the dependency's key names; a package reached
  twice is recorded once;
- the `toolchain` record, identified `@toolchain`, is the canonical bytes of
  the `@toolchain.v2` document of release 0.0.2, whose `stdlib` is `sha256:`
  and the SHA-256 of the canonical bytes of `stdlib/manifest.vibon`.

The revision is `sha256:` and the SHA-256 of `vibraWorkspaceRevisionV2`, one
zero byte, the record count as a big-endian 64-bit integer, and the records
sorted by the UTF-8 bytes of their kind and then their identifier, each as
three byte strings preceded by their big-endian 64-bit lengths.

The manifest's canonical bytes are its text on one line: its fields are
written in schema order with no defaulted field and no comment, so
canonicalizing it is collapsing its layout. The script refuses a manifest
that has a comment.

Every expected observation of a case that holds `revision: "sha256:..."` is
an observation of the case's tree, and every such revision must be the
case's.

.PARAMETER Root
The corpus directory, such as `conformance/cases`.

.PARAMETER Write
Rewrites every expected `revision` of the corpus to the computed one.

.PARAMETER Check
Changes nothing; exits 1 and names each case whose expected `revision`
differs from the computed one.

.PARAMETER Tree
Prints the revision of the project tree at this path and changes nothing.
#>
[CmdletBinding()]
param(
    [string] $Root = 'conformance/cases',
    [switch] $Write,
    [switch] $Check,
    [string] $Tree
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$utf8 = [System.Text.UTF8Encoding]::new($false)

function Get-Sha256Hex([byte[]] $Bytes) {
    $hash = [System.Security.Cryptography.SHA256]::HashData($Bytes)
    'sha256:' + (($hash | ForEach-Object { $_.ToString('x2') }) -join '')
}

function Get-BigEndian64([long] $Value) {
    $bytes = [System.BitConverter]::GetBytes([uint64] $Value)
    if ([System.BitConverter]::IsLittleEndian) { [System.Array]::Reverse($bytes) }
    , $bytes
}

# One space before the next element when layout separated it from the
# previous one, except right after an opening bracket.
function Add-Separator([System.Text.StringBuilder] $Out, [bool] $PendingSpace) {
    if (-not $PendingSpace -or $Out.Length -eq 0) { return }
    if ('{['.Contains($Out[$Out.Length - 1])) { return }
    [void] $Out.Append(' ')
}
# The canonical bytes of a document written in schema order with no
# defaulted field and no comment: one space between elements and after each
# `:`, none inside brackets.
function ConvertTo-CanonicalLayout([string] $Text) {
    $out = [System.Text.StringBuilder]::new()
    $pendingSpace = $false
    $index = 0
    while ($index -lt $Text.Length) {
        $char = $Text[$index]
        if ($char -eq '"') {
            $end = $index + 1
            while ($Text[$end] -ne '"') {
                if ($Text[$end] -eq '\') { $end++ }
                $end++
            }
            Add-Separator $out $pendingSpace
            [void] $out.Append($Text.Substring($index, $end - $index + 1))
            $pendingSpace = $false
            $index = $end + 1
            continue
        }
        if ($char -eq '/' -and $index + 1 -lt $Text.Length -and $Text[$index + 1] -eq '/') {
            throw 'the manifest has a comment, which its canonical bytes omit'
        }
        if ([char]::IsWhiteSpace($char)) {
            $pendingSpace = $out.Length -gt 0
        } elseif ('}]'.Contains($char)) {
            [void] $out.Append($char)
            $pendingSpace = $false
        } else {
            Add-Separator $out $pendingSpace
            [void] $out.Append($char)
            $pendingSpace = $false
        }
        $index++
    }
    $out.ToString()
}

function Get-ToolchainRecord {
    $manifestPath = Join-Path $PSScriptRoot '../../stdlib/manifest.vibon'
    $manifest = [System.IO.File]::ReadAllText($manifestPath, $utf8)
    $canonical = ConvertTo-CanonicalLayout $manifest
    $stdlib = Get-Sha256Hex $utf8.GetBytes($canonical)
    '{format: @toolchain.v2 version: "0.0.2" grammar: "sourceV2" checker: ' +
    '"staticV2" stdlib: "' + $stdlib + '" drivers: "systemV2" policy: ' +
    '"scopePolicyV1" registry: {major: 2u32 minor: 0u32}}'
}

function Get-TreeRevision([string] $TreePath, [string] $Toolchain) {
    $treeRoot = (Resolve-Path -LiteralPath $TreePath).Path
    $projectBytes = [System.IO.File]::ReadAllBytes((Join-Path $treeRoot 'project.vibon'))
    $projectText = $utf8.GetString($projectBytes)
    $roots = @([regex]::Matches($projectText, 'root:\s*"([^"]*)"') |
        ForEach-Object { $_.Groups[1].Value }) + @('tests')
    $records = [System.Collections.Generic.List[object]]::new()
    $records.Add([pscustomobject] @{ Kind = 'project'; Id = 'project.vibon'; Bytes = $projectBytes })
    $records.Add([pscustomobject] @{
        Kind = 'toolchain'; Id = '@toolchain'; Bytes = $utf8.GetBytes($Toolchain)
    })
    $sourceRoots = [System.Collections.Generic.List[string]]::new()
    foreach ($relative in $roots) { $sourceRoots.Add((Join-Path $treeRoot $relative)) }
    $seen = [System.Collections.Generic.HashSet[string]]::new()
    [void] $seen.Add($treeRoot)
    $pending = [System.Collections.Generic.Queue[object]]::new()
    $pending.Enqueue([pscustomobject] @{ Directory = $treeRoot; Text = $projectText })
    while ($pending.Count -gt 0) {
        $package = $pending.Dequeue()
        $entries = [regex]::Matches(
            $package.Text, '"([^"]*)"\s*:\s*\{\s*kind:\s*@path\s+path:\s*"([^"]*)"')
        foreach ($entry in $entries) {
            $key = $entry.Groups[1].Value
            $directory = [System.IO.Path]::GetFullPath(
                (Join-Path $package.Directory $entry.Groups[2].Value)).TrimEnd('\', '/')
            $file = Join-Path $directory 'project.vibon'
            if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { continue }
            $bytes = [System.IO.File]::ReadAllBytes($file)
            $text = $utf8.GetString($bytes)
            $target = [regex]::Match(
                $text, "name:\s*@$([regex]::Escape($key))\s+kind:\s*@lib\s+root:\s*`"([^`"]*)`"")
            if ($target.Success) {
                $sourceRoots.Add((Join-Path $directory $target.Groups[1].Value))
            }
            if (-not $seen.Add($directory)) { continue }
            $id = $file.Substring($treeRoot.Length).TrimStart('\', '/').Replace('\', '/')
            $records.Add([pscustomobject] @{ Kind = 'dependency'; Id = $id; Bytes = $bytes })
            $pending.Enqueue([pscustomobject] @{ Directory = $directory; Text = $text })
        }
    }
    foreach ($directory in ($sourceRoots | Select-Object -Unique)) {
        if (-not (Test-Path -LiteralPath $directory -PathType Container)) { continue }
        Get-ChildItem -LiteralPath $directory -Recurse -File |
            Where-Object { $_.Name.EndsWith('.vib', [System.StringComparison]::Ordinal) } |
            ForEach-Object {
                $id = $_.FullName.Substring($treeRoot.Length).TrimStart('\', '/').Replace('\', '/')
                $records.Add([pscustomobject] @{
                    Kind = 'source'; Id = $id; Bytes = [System.IO.File]::ReadAllBytes($_.FullName)
                })
            }
    }
    $sorted = [System.Linq.Enumerable]::ToArray($records)
    [System.Array]::Sort($sorted, [System.Comparison[object]] {
        param($left, $right)
        $byKind = [string]::CompareOrdinal($left.Kind, $right.Kind)
        if ($byKind -ne 0) { return $byKind }
        [string]::CompareOrdinal($left.Id, $right.Id)
    })
    $stream = [System.IO.MemoryStream]::new()
    $prefix = $utf8.GetBytes('vibraWorkspaceRevisionV2')
    $stream.Write($prefix, 0, $prefix.Length)
    $stream.WriteByte(0)
    $count = Get-BigEndian64 $sorted.Length
    $stream.Write($count, 0, 8)
    foreach ($record in $sorted) {
        foreach ($part in @($utf8.GetBytes($record.Kind), $utf8.GetBytes($record.Id), $record.Bytes)) {
            $length = Get-BigEndian64 $part.Length
            $stream.Write($length, 0, 8)
            $stream.Write($part, 0, $part.Length)
        }
    }
    Get-Sha256Hex $stream.ToArray()
}

$toolchain = Get-ToolchainRecord

if ($Tree) {
    Get-TreeRevision $Tree $toolchain
    exit 0
}

if ($Write -eq $Check) {
    throw 'pass exactly one of -Write and -Check'
}

$pattern = 'revision: "sha256:[0-9a-f]{64}"'
$stale = [System.Collections.Generic.List[string]]::new()
$rewritten = [System.Collections.Generic.List[string]]::new()
foreach ($manifest in Get-ChildItem -LiteralPath $Root -Recurse -File -Filter 'case.toml') {
    $caseDirectory = $manifest.DirectoryName
    $caseText = [System.IO.File]::ReadAllText($manifest.FullName, $utf8)
    $treeMatch = [regex]::Match($caseText, '(?m)^tree\s*=\s*"([^"]*)"')
    if (-not $treeMatch.Success) { continue }
    $observations = Get-ChildItem -LiteralPath $caseDirectory -File |
        Where-Object { $_.Name -ne 'case.toml' } |
        Where-Object { [regex]::IsMatch([System.IO.File]::ReadAllText($_.FullName, $utf8), $pattern) }
    if (-not $observations) { continue }
    $caseId = [regex]::Match($caseText, '(?m)^id\s*=\s*"([^"]*)"').Groups[1].Value
    $revision = Get-TreeRevision (Join-Path $caseDirectory $treeMatch.Groups[1].Value) $toolchain
    $expected = "revision: `"$revision`""
    foreach ($observation in $observations) {
        $text = [System.IO.File]::ReadAllText($observation.FullName, $utf8)
        $differs = [regex]::Matches($text, $pattern) | Where-Object { $_.Value -ne $expected }
        if (-not $differs) { continue }
        if ($Check) {
            $stale.Add("$caseId ($($observation.Name))")
        } else {
            [System.IO.File]::WriteAllText(
                $observation.FullName,
                [regex]::Replace($text, $pattern, $expected),
                $utf8
            )
            $rewritten.Add($observation.FullName)
        }
    }
}

if ($Check) {
    if ($stale.Count -gt 0) {
        foreach ($case in $stale) { Write-Output "stale revision: $case" }
        exit 1
    }
    Write-Output 'every expected revision is current'
    exit 0
}

foreach ($file in $rewritten) { Write-Output "rewrote $file" }
Write-Output "rewrote $($rewritten.Count) files"
exit 0