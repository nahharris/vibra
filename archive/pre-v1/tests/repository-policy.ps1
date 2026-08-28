$ErrorActionPreference = 'Stop'

$workflow = Join-Path (Split-Path -Parent $PSScriptRoot) '.github/workflows/owner-approval.yml'
if (-not (Test-Path -LiteralPath $workflow)) {
    throw "Owner approval workflow is missing: $workflow"
}

$content = Get-Content -Raw $workflow
function Require([string]$Pattern, [string]$Description) {
    if ($content -notmatch $Pattern) { throw "$Description is missing from $workflow" }
}

Require 'pull_request_target:' 'Default-branch pull request trigger'
Require 'pull_request_review:' 'Review-change trigger'
Require 'pull-requests:\s*read' 'Read-only pull request permission'
Require 'contents:\s*read' 'Read-only contents permission'
Require 'author_association|AUTHOR.*OWNER|AUTHOR"\s*=\s*"\$OWNER' 'Repository-owner author bypass'
Require 'github\.event\.pull_request\.head\.sha' 'Current head SHA binding'
Require 'APPROVED' 'Owner approval requirement'
Require 'CHANGES_REQUESTED|DISMISSED' 'Effective latest review handling'

if ($content -match 'actions/checkout') {
    throw 'Owner approval workflow must never execute pull request code.'
}
if ($content -match '(?m)^\s*(push|pull_request):') {
    throw 'Owner approval workflow must use default-branch event execution.'
}

Write-Host 'Repository owner-approval policy checks passed.'
