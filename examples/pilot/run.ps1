<#
.SYNOPSIS
Makes the registered model calls of the model-writing pilot and records
every run.

.DESCRIPTION
The runner of docs/roadmap/v0.0.2/m2/steps/40-pilot-runs.md. Everything it
sends is built from docs/roadmap/v0.0.2/m2/pilot/registration.md: the
prompts from the parts between the `~~~~text` fences, checked against the
registered digests before any call; the schedule from the task and model
tables and the order the "Schedule" section gives; the invocation of each
client from the "Models" section.

  pwsh -NoProfile -File examples/pilot/run.ps1 -WriteSchedule

writes runs/schedule.tsv, or checks that the one there is the same.

  pwsh -NoProfile -File examples/pilot/run.ps1 -Setup -Cohort <haiku|luna>

makes the cohort's setup call ("Budget", "Setup calls") unless one already
succeeded, at most five per cohort, and records it under runs/setup/.

  pwsh -NoProfile -File examples/pilot/run.ps1 -Cohort <haiku|luna>

makes the cohort's runs one call at a time in the registered order. It is
resumable: a run whose completion is stored is never called again, and a
call that was started and never finished counts as a call that failed at
the provider. Each run gets a directory runs/<run>/ with record.tsv,
calls.tsv, the whole output of each call, the completion, the extracted
solution, and the grader's line; runs.tsv gets one line per finished run,
in schedule order. The cohort stops after five consecutive calls that
failed at the provider (exit code 3).

.PARAMETER Records
The records directory. The default is the registration's runs/ directory;
runs.tsv is written beside it.

.PARAMETER Registration
The registration to read. The default is the repository's.

.PARAMETER Vibra
The vibra binary that grades. The default is target/debug/vibra(.exe).

.PARAMETER StubClient
A PowerShell script that stands in for both clients, for testing the
runner. It is called with the client's name and arguments.

.PARAMETER CallTimeout
The seconds a call may take; the registered value, and the default, is 600.
#>
[CmdletBinding(DefaultParameterSetName = 'Run')]
param(
  [Parameter(ParameterSetName = 'Schedule', Mandatory)]
  [switch]$WriteSchedule,
  [Parameter(ParameterSetName = 'Setup', Mandatory)]
  [switch]$Setup,
  [Parameter(ParameterSetName = 'Setup', Mandatory)]
  [Parameter(ParameterSetName = 'Run', Mandatory)]
  [ValidateSet('haiku', 'luna')]
  [string]$Cohort,
  [string]$Records = '',
  [string]$Registration = '',
  [string]$Vibra = '',
  [string]$StubClient = '',
  [ValidateRange(1, 86400)]
  [int]$CallTimeout = 600,
  [string]$Freeze = '156f35cf2a7f10c8d9df37be43053ba74e941605'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Pilot = $PSScriptRoot
$Repository = [IO.Path]::GetFullPath((Join-Path $Pilot '../..'))
$Utf8 = [Text.UTF8Encoding]::new($false)
$StrictUtf8 = [Text.UTF8Encoding]::new($false, $true)
$FreezeSubject = 'M2 step 38: pilot freeze'
$ToolchainPaths = 'crates', 'stdlib', 'Cargo.lock', 'Cargo.toml', 'rust-toolchain.toml'
$Final = 'completion', 'providerError', 'stopped'
$StopAfter = 5
$OutputCap = 4096

if ($Registration -eq '') { $Registration = Join-Path $Repository 'docs/roadmap/v0.0.2/m2/pilot/registration.md' }
if ($Records -eq '') { $Records = Join-Path $Repository 'docs/roadmap/v0.0.2/m2/pilot/runs' }
$Registration = [IO.Path]::GetFullPath($Registration, (Get-Location).Path)
$Records = [IO.Path]::GetFullPath($Records, (Get-Location).Path)
$Table = Join-Path (Split-Path $Records) 'runs.tsv'
$ScheduleFile = Join-Path $Records 'schedule.tsv'
$SetupDirectory = Join-Path $Records 'setup'
$CatalogFile = Join-Path $SetupDirectory 'luna-catalog.json'

function Get-Sha256([byte[]]$Bytes) {
  return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($Bytes)).ToLowerInvariant()
}

function Get-Now { return [DateTime]::UtcNow.ToString("yyyy-MM-dd'T'HH:mm:ss.fff'Z'") }

function Write-Bytes([string]$Path, [byte[]]$Bytes) { [IO.File]::WriteAllBytes($Path, $Bytes) }

function Write-Text([string]$Path, [string]$Text) { [IO.File]::WriteAllText($Path, $Text, $Utf8) }

function ConvertTo-Compact($Value) { return ConvertTo-Json -InputObject $Value -Compress -Depth 50 }

# One value of a key/value record: no tab and no line break.
function Format-Value($Value) {
  if ($null -eq $Value) { return '-' }
  $text = "$Value"
  if ($text -eq '') { return '-' }
  if ($text -match "[`t`r`n]") { throw "a record value holds a tab or a line break: $text" }
  return $text
}

function Write-Record([string]$Path, [Collections.Specialized.OrderedDictionary]$Fields) {
  $lines = foreach ($key in $Fields.Keys) { "$key`t$(Format-Value $Fields[$key])" }
  Write-Text $Path (($lines -join "`n") + "`n")
}

function Read-Record([string]$Path) {
  $fields = [ordered]@{}
  foreach ($line in [IO.File]::ReadAllLines($Path)) {
    if ($line -eq '') { continue }
    $key, $value = $line -split "`t", 2
    $fields[$key] = $value
  }
  return $fields
}

# ---------------------------------------------------------------------------
# The registration.

$RegistrationBytes = [IO.File]::ReadAllBytes($Registration)
$RegistrationText = $StrictUtf8.GetString($RegistrationBytes)
if ($RegistrationText.Contains("`r")) { throw 'the registration has a CR; its parts are LF only' }
$Lines = $RegistrationText.Split("`n")

function Find-Line([string]$Text, [int]$From = 0) {
  for ($i = $From; $i -lt $Lines.Count; $i++) { if ($Lines[$i] -ceq $Text) { return $i } }
  throw "the registration has no line '$Text'"
}

# A part is the text between the line ~~~~text after its heading and the next
# line ~~~~, each line ending with one LF ("Prompts").
function Get-Part([string]$Heading) {
  $start = Find-Line $Heading
  $open = $start + 1
  while ($Lines[$open] -ceq '') { $open++ }
  if ($Lines[$open] -cne '~~~~text') { throw "no ~~~~text after '$Heading'" }
  $close = Find-Line '~~~~' ($open + 1)
  $body = $Lines[($open + 1)..($close - 1)]
  return $Utf8.GetBytes(($body -join "`n") + "`n")
}

function Get-Section([string]$Heading) {
  $start = Find-Line $Heading
  $end = $start + 1
  $inPart = $false
  while ($end -lt $Lines.Count -and ($inPart -or -not $Lines[$end].StartsWith('## '))) {
    if ($Lines[$end] -ceq '~~~~text') { $inPart = $true } elseif ($Lines[$end] -ceq '~~~~') { $inPart = $false }
    $end++
  }
  return $Lines[$start..($end - 1)]
}

function Get-Registered {
  $tasks = foreach ($line in (Get-Section '## Tasks')) {
    if ($line -match '^\| (T\d+) \| `([a-z]+)` \| `@pilot\.([a-z]+)` \|') { [pscustomobject]@{ Id = $Matches[1]; Name = $Matches[2] } }
  }
  if (@($tasks).Count -ne 10) { throw 'the registration does not list ten tasks' }
  $models = foreach ($line in (Get-Section '## Models')) {
    if ($line -match '^\| `([a-z]+)` \| (\w+) \| `([^`]+)` \| (.+) \|$') {
      [pscustomobject]@{ Key = $Matches[1]; Family = $Matches[2]; Identifier = $Matches[3]; Setting = $Matches[4].Replace('`', '') }
    }
  }
  if (@($models).Count -ne 2 -or $models[0].Key -ne 'haiku' -or $models[1].Key -ne 'luna') { throw 'the registration does not list the models haiku and luna' }
  if ($RegistrationText -notmatch 'The system prompt of both is the one line\s+`([^`]+)`') { throw 'no system prompt in the registration' }
  $system = $Matches[1]
  if ($RegistrationText -notmatch 'with the prompt\s+`([^`]+)`') { throw 'no setup prompt in the registration' }
  $setupPrompt = $Matches[1]
  foreach ($sentence in 'the output cap set to 4,096 tokens through', 'timed out after 600 seconds', 'Five attempts per task, arm, and model', 'at most five times per cohort', 'five consecutive') {
    if (-not ($RegistrationText -replace '\s+', ' ').Contains($sentence)) { throw "the registration no longer says '$sentence'" }
  }
  # The parts and their digests ("Digests").
  $digests = @{}
  foreach ($line in (Get-Section '## Prompts')) {
    if ($line -match '^\| (.+?) \| (\d+) \| `([0-9a-f]{64})` \|$') { $digests[$Matches[1]] = @{ Bytes = [int]$Matches[2]; Sha = $Matches[3] } }
  }
  $prompts = @{}
  foreach ($line in (Get-Section '## Prompts')) {
    if ($line -match '^\| (T\d+) `([a-z]+)` \| `(endlet|noEndlet)` \| (\d+) \| `([0-9a-f]{64})` \|$') { $prompts["$($Matches[2])/$($Matches[3])"] = @{ Bytes = [int]$Matches[4]; Sha = $Matches[5] } }
  }
  $parts = @{}
  foreach ($arm in 'endlet', 'noEndlet') { $parts["card/$arm"] = @{ Name = "Card, ``$arm`` arm"; Bytes = Get-Part "### Card, ``$arm`` arm" } }
  $parts['library'] = @{ Name = 'Library, both arms'; Bytes = Get-Part '### Library, both arms' }
  foreach ($task in $tasks) { $parts["statement/$($task.Name)"] = @{ Name = "Statement of $($task.Id) ``$($task.Name)``, both arms"; Bytes = Get-Part "### Statement of $($task.Id) ``$($task.Name)``, both arms" } }
  foreach ($key in $parts.Keys) {
    $part = $parts[$key]
    $registered = $digests[$part.Name]
    if ($null -eq $registered) { throw "no registered digest for the part '$($part.Name)'" }
    if ($part.Bytes.Length -ne $registered.Bytes -or (Get-Sha256 $part.Bytes) -ne $registered.Sha) { throw "refusing to send: the part '$($part.Name)' is not the registered part" }
  }
  # A prompt is the card, one LF, the library, one LF, and the statement.
  $built = @{}
  $lf = [byte[]](10)
  foreach ($task in $tasks) {
    foreach ($arm in 'endlet', 'noEndlet') {
      $bytes = [byte[]]($parts["card/$arm"].Bytes + $lf + $parts['library'].Bytes + $lf + $parts["statement/$($task.Name)"].Bytes)
      $registered = $prompts["$($task.Name)/$arm"]
      if ($null -eq $registered) { throw "no registered digest for the prompt of $($task.Name) $arm" }
      $sha = Get-Sha256 $bytes
      if ($bytes.Length -ne $registered.Bytes -or $sha -ne $registered.Sha) { throw "refusing to send: the prompt of $($task.Name) $arm is not the registered prompt" }
      $built["$($task.Name)/$arm"] = @{ Bytes = $bytes; Sha = $sha }
    }
  }
  return [pscustomobject]@{ Tasks = @($tasks); Models = @($models); System = $system; SetupPrompt = $setupPrompt; Prompts = $built }
}

# The schedule ("Schedule"): per cohort, attempt 1 to 5; within an attempt,
# T1 to T10; within a task, endlet then noEndlet for an odd attempt and
# noEndlet then endlet for an even one. The haiku cohort comes first.
function Get-Schedule($Reg) {
  $entries = [Collections.Generic.List[object]]::new()
  $index = 0
  foreach ($model in $Reg.Models) {
    $position = 0
    foreach ($attempt in 1..5) {
      $arms = if ($attempt % 2 -eq 1) { 'endlet', 'noEndlet' } else { 'noEndlet', 'endlet' }
      foreach ($task in $Reg.Tasks) {
        foreach ($arm in $arms) {
          $index++
          $position++
          $entries.Add([pscustomobject]@{
              Index = $index; Cohort = $model.Key; Position = $position
              Run = "$($model.Key)-$($task.Name)-$arm-$attempt"
              Model = $model.Identifier; Setting = $model.Setting
              Task = $task.Name; Arm = $arm; Attempt = $attempt
              PromptSha256 = $Reg.Prompts["$($task.Name)/$arm"].Sha
            })
        }
      }
    }
  }
  return $entries
}

$ScheduleHeader = "index`tcohort`tposition`trun`tmodel`tsetting`ttask`tarm`tattempt`tpromptSha256"

function Format-Schedule($Entries) {
  $lines = [Collections.Generic.List[string]]::new()
  $lines.Add($ScheduleHeader)
  foreach ($e in $Entries) { $lines.Add("$($e.Index)`t$($e.Cohort)`t$($e.Position)`t$($e.Run)`t$($e.Model)`t$($e.Setting)`t$($e.Task)`t$($e.Arm)`t$($e.Attempt)`t$($e.PromptSha256)") }
  return ($lines -join "`n") + "`n"
}

# ---------------------------------------------------------------------------
# The toolchain that grades.

function Get-Toolchain {
  $subject = git -C $Repository log -1 --format=%s $Freeze
  if ($LASTEXITCODE -ne 0 -or $subject -cne $FreezeSubject) { throw "the freeze commit $Freeze is not '$FreezeSubject'" }
  $diff = git -C $Repository diff --stat $Freeze -- @ToolchainPaths
  if ($LASTEXITCODE -ne 0 -or $diff) { throw "the toolchain differs from the freeze commit: $diff" }
  $path = $Vibra
  if ($path -eq '') { $path = Join-Path $Repository ("target/debug/" + $(if ($IsWindows) { 'vibra.exe' } else { 'vibra' })) }
  $path = [IO.Path]::GetFullPath($path, (Get-Location).Path)
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "no vibra binary at $path" }
  return [pscustomobject]@{ Freeze = $Freeze; Vibra = $path; VibraSha256 = Get-Sha256 ([IO.File]::ReadAllBytes($path)) }
}

# ---------------------------------------------------------------------------
# The clients ("Models").

$Pwsh = (Get-Process -Id $PID).Path

function Get-ClientPath([string]$Name) {
  if ($StubClient -ne '') { return $Name }
  $command = Get-Command $Name -CommandType Application -ErrorAction Stop | Select-Object -First 1
  return $command.Source
}

# The environment of a call: the variables of a surrounding Claude Code or
# Codex session are removed, so the call shares no session with it.
function Get-CallEnvironment([hashtable]$Set) {
  $environment = @{}
  $removed = [Collections.Generic.List[string]]::new()
  foreach ($entry in [Environment]::GetEnvironmentVariables().GetEnumerator()) {
    $name = [string]$entry.Key
    if ($name -match '^CLAUDE' -or ($name -match '^CODEX_' -and $name -ne 'CODEX_HOME')) { $removed.Add($name); continue }
    $environment[$name] = [string]$entry.Value
  }
  foreach ($key in $Set.Keys) { $environment[$key] = $Set[$key] }
  return [pscustomobject]@{ Variables = $environment; Removed = @($removed | Sort-Object) }
}

# Runs one process with the given standard input and returns how it ended.
function Invoke-Process([string]$Program, [string[]]$Arguments, [byte[]]$Stdin, $Environment, [string]$Directory, [int]$Seconds) {
  if ($StubClient -ne '') {
    $Arguments = @('-NoProfile', '-File', $StubClient, $Program) + $Arguments
    $Program = $Pwsh
  }
  $start = [Diagnostics.ProcessStartInfo]::new($Program)
  foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
  $start.UseShellExecute = $false
  $start.RedirectStandardInput = $true
  $start.RedirectStandardOutput = $true
  $start.RedirectStandardError = $true
  $start.WorkingDirectory = $Directory
  $start.Environment.Clear()
  foreach ($key in $Environment.Keys) { $start.Environment[$key] = $Environment[$key] }
  $began = Get-Now
  $clock = [Diagnostics.Stopwatch]::StartNew()
  $process = [Diagnostics.Process]::Start($start)
  try {
    $out = [IO.MemoryStream]::new()
    $err = [IO.MemoryStream]::new()
    $reading = $process.StandardOutput.BaseStream.CopyToAsync($out)
    $readingErrors = $process.StandardError.BaseStream.CopyToAsync($err)
    try {
      $writing = $process.StandardInput.BaseStream.WriteAsync($Stdin, 0, $Stdin.Length)
      $null = $writing.Wait($Seconds * 1000)
      $process.StandardInput.Close()
    }
    catch { }
    $timedOut = -not $process.WaitForExit($Seconds * 1000)
    if ($timedOut) { $process.Kill($true); $process.WaitForExit() }
    $process.WaitForExit()
    $null = $reading.Wait(10000)
    $null = $readingErrors.Wait(10000)
    $exit = if ($timedOut) { $null } else { $process.ExitCode }
  }
  finally {
    if (-not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
    $process.Dispose()
  }
  return [pscustomobject]@{
    Start = $began; End = Get-Now; Seconds = [Math]::Round($clock.Elapsed.TotalSeconds, 3)
    Exit = $exit; TimedOut = $timedOut; Stdout = $out.ToArray(); Stderr = $err.ToArray()
  }
}

function New-EmptyDirectory {
  $path = Join-Path ([IO.Path]::GetTempPath()) ('vibraPilotCall-' + [guid]::NewGuid().ToString('N'))
  $null = New-Item -ItemType Directory -Path $path
  return $path
}

# The Claude Code client in print mode with JSON output.
function Get-ClaudeArguments($Model, [string]$System) {
  return @(
    '-p', '--model', $Model.Identifier, '--effort', 'low', '--output-format', 'json',
    '--tools', '', '--strict-mcp-config', '--mcp-config', '{"mcpServers":{}}',
    '--setting-sources', '', '--disable-slash-commands', '--no-session-persistence',
    '--system-prompt', $System
  )
}

function Invoke-Claude($Model, [string]$System, [byte[]]$Prompt) {
  $environment = Get-CallEnvironment @{ CLAUDE_CODE_MAX_OUTPUT_TOKENS = "$OutputCap" }
  $arguments = Get-ClaudeArguments $Model $System
  $directory = New-EmptyDirectory
  try { $ended = Invoke-Process (Get-ClientPath 'claude') $arguments $Prompt $environment.Variables $directory $CallTimeout }
  finally { Remove-Item -LiteralPath $directory -Recurse -Force -ErrorAction SilentlyContinue }
  $completion = $null; $reason = ''; $reported = ''; $usage = $null
  $text = $Utf8.GetString($ended.Stdout)
  $document = $null
  try { if ($text.Trim() -ne '') { $document = $text | ConvertFrom-Json -Depth 100 } } catch { $document = $null }
  if ($null -ne $document -and $document -is [pscustomobject]) {
    if ($document.PSObject.Properties.Name -contains 'modelUsage' -and $null -ne $document.modelUsage) { $reported = (@($document.modelUsage.PSObject.Properties.Name) | Sort-Object) -join ',' }
    if ($document.PSObject.Properties.Name -contains 'usage') { $usage = $document.usage }
  }
  if ($ended.TimedOut) { $reason = "timed out after $CallTimeout seconds" }
  elseif ($ended.Exit -ne 0) { $reason = "the client exited with $($ended.Exit)" }
  elseif ($null -eq $document -or $document -isnot [pscustomobject]) { $reason = 'the output is not a JSON document' }
  elseif ($document.PSObject.Properties.Name -contains 'is_error' -and $document.is_error) { $reason = 'the client reported an error' }
  elseif ($document.PSObject.Properties.Name -notcontains 'result' -or $document.result -isnot [string]) { $reason = 'no completion came back' }
  else { $completion = [string]$document.result }
  return [pscustomobject]@{
    Ended = $ended; Completion = $completion; Reason = $reason; Reported = $reported; Usage = $usage; Audit = $null
    Command = @('claude') + $arguments; Removed = $environment.Removed; Cap = "$OutputCap (CLAUDE_CODE_MAX_OUTPUT_TOKENS)"
  }
}

# The Codex client's exec command with JSON events. Every tool and feature
# is turned off through the configuration and through the model catalog the
# call names, which is the installed client's own entry for the model with
# the system prompt as its base instructions and its tool fields cleared.
function New-LunaCatalog([string]$System) {
  $directory = New-EmptyDirectory
  try { $ended = Invoke-Process (Get-ClientPath 'codex') @('debug', 'models', '--bundled') ([byte[]]::new(0)) (Get-CallEnvironment @{}).Variables $directory 120 }
  finally { Remove-Item -LiteralPath $directory -Recurse -Force -ErrorAction SilentlyContinue }
  if ($ended.Exit -ne 0) { throw 'codex debug models failed' }
  $catalog = $Utf8.GetString($ended.Stdout) | ConvertFrom-Json -Depth 100
  $entry = @($catalog.models | Where-Object slug -eq 'gpt-6-luna')
  if ($entry.Count -ne 1) { throw 'the installed client has no catalog entry gpt-6-luna' }
  $entry = $entry[0]
  $entry.base_instructions = $System
  foreach ($field in 'model_messages', 'apply_patch_tool_type', 'tool_mode', 'multi_agent_version') { $entry.$field = $null }
  $entry.experimental_supported_tools = @()
  $entry.supports_search_tool = $false
  $entry.node_repl_disabled = $true
  return (ConvertTo-Json -InputObject ([ordered]@{ models = @($entry) }) -Depth 100) + "`n"
}

function Get-LunaConfiguration([string]$Catalog, [bool]$Audit) {
  $config = [ordered]@{
    model_catalog_json = $Catalog.Replace('\', '/'); model = 'gpt-6-luna'; model_reasoning_effort = 'low'
    developer_instructions = ''; project_doc_max_bytes = 0
    'skills.include_instructions' = $false; 'skills.bundled.enabled' = $false
    include_environment_context = $false; include_permissions_instructions = $false
    include_collaboration_mode_instructions = $false; include_apps_instructions = $false
    web_search = 'disabled'; 'tools.experimental_request_user_input.enabled' = $false
    suppress_unstable_features_warning = $true
  }
  foreach ($feature in 'memories', 'plugins', 'apps', 'shell_tool', 'unified_exec', 'multi_agent', 'multi_agent_v2', 'sleep_tool', 'goals', 'view_image', 'tool_suggest', 'skill_search', 'hooks', 'workspace_dependencies', 'code_mode_host') {
    $config["features.$feature"] = $false
  }
  $config['features.skip_host_skill_discovery'] = $true
  # The audit command cannot ignore the user configuration, so every MCP
  # server configured there is turned off by name; no value is read.
  if ($Audit) {
    $codexHome = if ($env:CODEX_HOME) { $env:CODEX_HOME } else { Join-Path $HOME '.codex' }
    $file = Join-Path $codexHome 'config.toml'
    if (Test-Path -LiteralPath $file) {
      foreach ($line in [IO.File]::ReadAllLines($file)) {
        if ($line -match '^\s*\[mcp_servers\.([A-Za-z0-9_-]+)\]\s*$') { $config["mcp_servers.$($Matches[1]).enabled"] = $false }
      }
    }
  }
  $options = [Collections.Generic.List[string]]::new()
  foreach ($key in $config.Keys) { $options.Add('-c'); $options.Add("$key=$(ConvertTo-Compact $config[$key])") }
  return , $options
}

# The context audit: exactly two messages, the system prompt and the prompt,
# and no tool.
function Test-LunaContext([string]$System, [string]$PromptText) {
  $options = Get-LunaConfiguration $CatalogFile $true
  $arguments = @('--no-daemon') + $options + @('debug', 'prompt-input', $PromptText)
  $directory = New-EmptyDirectory
  try { $ended = Invoke-Process (Get-ClientPath 'codex') $arguments ([byte[]]::new(0)) (Get-CallEnvironment @{}).Variables $directory 120 }
  finally { Remove-Item -LiteralPath $directory -Recurse -Force -ErrorAction SilentlyContinue }
  $items = $null
  try { $items = @($Utf8.GetString($ended.Stdout) | ConvertFrom-Json -Depth 100) } catch { $items = $null }
  $roles = ''; $tools = 0; $messages = 0; $systemSha = '-'; $promptSha = '-'; $valid = $false
  if ($null -ne $items -and $ended.Exit -eq 0) {
    $messages = @($items).Count
    $roles = (@($items | ForEach-Object { if ($_.PSObject.Properties.Name -contains 'role') { "$($_.type)/$($_.role)" } else { "$($_.type)" } })) -join ','
    foreach ($item in $items) { if ($item.PSObject.Properties.Name -contains 'tools') { $tools += @($item.tools).Count } }
    $texts = @(foreach ($item in $items) {
        if ($item.type -eq 'message') { (@($item.content | ForEach-Object { if ($_.PSObject.Properties.Name -contains 'text') { $_.text } })) -join '' }
      })
    if ($texts.Count -ge 1) { $systemSha = Get-Sha256 $Utf8.GetBytes($texts[0]) }
    if ($texts.Count -ge 2) { $promptSha = Get-Sha256 $Utf8.GetBytes($texts[1]) }
    $valid = $messages -eq 2 -and $roles -eq 'message/developer,message/user' -and $tools -eq 0 -and
      $texts.Count -eq 2 -and $texts[0] -ceq $System -and $texts[1] -ceq $PromptText
  }
  return [pscustomobject]@{
    Valid = $valid; Exit = $ended.Exit; Messages = $messages; Roles = $roles; Tools = $tools
    SystemSha256 = $systemSha; PromptSha256 = $promptSha; OutputSha256 = Get-Sha256 $ended.Stdout; OutputBytes = $ended.Stdout.Length
  }
}

function Find-Model($Node, [Collections.Generic.HashSet[string]]$Found) {
  if ($null -eq $Node) { return }
  if ($Node -is [pscustomobject]) {
    foreach ($property in $Node.PSObject.Properties) {
      if ($property.Name -eq 'model' -and $property.Value -is [string]) { $null = $Found.Add($property.Value) }
      Find-Model $property.Value $Found
    }
  }
  elseif ($Node -is [Array]) { foreach ($item in $Node) { Find-Model $item $Found } }
}

function Invoke-Luna($Model, [string]$System, [byte[]]$Prompt) {
  $promptText = $StrictUtf8.GetString($Prompt)
  $audit = Test-LunaContext $System $promptText
  $options = Get-LunaConfiguration $CatalogFile $false
  $directory = New-EmptyDirectory
  $arguments = @('--no-daemon', 'exec', '--ignore-user-config', '--ignore-rules', '--ephemeral', '--skip-git-repo-check', '--json', '--model', $Model.Identifier, '-C', $directory) + $options + @('-')
  $shown = @('codex') + ($arguments | ForEach-Object { if ($_ -eq $directory) { '<empty temporary directory>' } elseif ($_.StartsWith('model_catalog_json=')) { 'model_catalog_json="<records>/setup/luna-catalog.json"' } else { $_ } })
  $environment = Get-CallEnvironment @{}
  $completion = $null; $reason = ''; $reported = ''; $usage = $null
  if (-not $audit.Valid) {
    Remove-Item -LiteralPath $directory -Recurse -Force -ErrorAction SilentlyContinue
    $ended = [pscustomobject]@{ Start = Get-Now; End = Get-Now; Seconds = 0; Exit = $null; TimedOut = $false; Stdout = [byte[]]::new(0); Stderr = [byte[]]::new(0) }
    return [pscustomobject]@{
      Ended = $ended; Completion = $null; Reason = 'the context audit failed'; Reported = ''; Usage = $null; Audit = $audit
      Command = $shown; Removed = $environment.Removed; Cap = 'none (the client offers no output cap)'; NotSent = $true
    }
  }
  try { $ended = Invoke-Process (Get-ClientPath 'codex') $arguments $Prompt $environment.Variables $directory $CallTimeout }
  finally { Remove-Item -LiteralPath $directory -Recurse -Force -ErrorAction SilentlyContinue }
  $events = [Collections.Generic.List[object]]::new()
  foreach ($line in $Utf8.GetString($ended.Stdout).Split("`n")) {
    if ($line.Trim().StartsWith('{')) { try { $events.Add(($line | ConvertFrom-Json -Depth 100)) } catch { } }
  }
  $found = [Collections.Generic.HashSet[string]]::new()
  foreach ($event in $events) { Find-Model $event $found }
  $reported = if ($found.Count) { (@($found) | Sort-Object) -join ',' } else { 'notReported' }
  $finished = @($events | Where-Object { $_.type -eq 'turn.completed' })
  $failed = @($events | Where-Object { $_.type -in 'error', 'turn.failed' })
  $messages = @($events | Where-Object { $_.type -eq 'item.completed' -and $_.item.type -eq 'agent_message' })
  if ($finished.Count) { $usage = $finished[-1].usage }
  if ($ended.TimedOut) { $reason = "timed out after $CallTimeout seconds" }
  elseif ($ended.Exit -ne 0) { $reason = "the client exited with $($ended.Exit)" }
  elseif ($failed.Count) { $reason = 'the client reported an error' }
  elseif (-not $finished.Count -or -not $messages.Count) { $reason = 'no completion came back' }
  else { $completion = [string]$messages[-1].item.text }
  return [pscustomobject]@{
    Ended = $ended; Completion = $completion; Reason = $reason; Reported = $reported; Usage = $usage; Audit = $audit
    Command = $shown; Removed = $environment.Removed; Cap = 'none (the client offers no output cap)'; NotSent = $false
  }
}

function Invoke-Client($Model, [string]$System, [byte[]]$Prompt) {
  if ($Model.Key -eq 'haiku') { return Invoke-Claude $Model $System $Prompt }
  return Invoke-Luna $Model $System $Prompt
}

function Get-ClientVersion($Model) {
  $name = if ($Model.Key -eq 'haiku') { 'claude' } else { 'codex' }
  $directory = New-EmptyDirectory
  try { $ended = Invoke-Process (Get-ClientPath $name) @('--version') ([byte[]]::new(0)) (Get-CallEnvironment @{}).Variables $directory 120 }
  finally { Remove-Item -LiteralPath $directory -Recurse -Force -ErrorAction SilentlyContinue }
  return $Utf8.GetString($ended.Stdout).Trim()
}

# ---------------------------------------------------------------------------
# Records.

$CallsHeader = "call`tstartUtc`tendUtc`tseconds`texit`ttimedOut`toutcome`treason`treportedModel`tusage`taudit"

# Writes the files of one finished call and returns its calls.tsv line.
function Save-Call([string]$Directory, [int]$Number, $Called) {
  $outcome = if ($null -ne $Called.Completion) { 'completion' } else { 'failed' }
  if (-not ($Called.PSObject.Properties.Name -contains 'NotSent' -and $Called.NotSent)) {
    Write-Bytes (Join-Path $Directory "call-$Number.stdout.txt") $Called.Ended.Stdout
    Write-Bytes (Join-Path $Directory "call-$Number.stderr.txt") $Called.Ended.Stderr
  }
  # The completion is stored before the call's line, so a call whose line is
  # missing was interrupted, with or without a stored completion.
  if ($null -ne $Called.Completion) { Write-Bytes (Join-Path $Directory 'completion.txt') $Utf8.GetBytes($Called.Completion) }
  $audit = '-'
  if ($null -ne $Called.Audit) {
    $a = $Called.Audit
    $audit = "valid=$($a.Valid);messages=$($a.Messages);roles=$($a.Roles);tools=$($a.Tools);systemSha256=$($a.SystemSha256);promptSha256=$($a.PromptSha256)"
  }
  $usage = if ($null -ne $Called.Usage) { ConvertTo-Compact $Called.Usage } else { '-' }
  $timed = if ($Called.Ended.TimedOut) { 'true' } else { 'false' }
  $exit = if ($null -ne $Called.Ended.Exit) { $Called.Ended.Exit } else { '-' }
  $line = "$Number`t$($Called.Ended.Start)`t$($Called.Ended.End)`t$($Called.Ended.Seconds)`t$exit`t$timed`t$outcome`t$(Format-Value $Called.Reason)`t$(Format-Value $Called.Reported)`t$usage`t$audit"
  Write-Text (Join-Path $Directory "call-$Number.tsv") ($line + "`n")
  return $line
}

function Get-CallLines([string]$Directory) {
  $lines = [Collections.Generic.List[string]]::new()
  for ($n = 1; $n -le 2; $n++) {
    $file = Join-Path $Directory "call-$n.tsv"
    if (Test-Path -LiteralPath $file) { $lines.Add([IO.File]::ReadAllText($file).TrimEnd("`n")) }
  }
  return , $lines
}

function Write-Calls([string]$Directory) {
  $lines = Get-CallLines $Directory
  Write-Text (Join-Path $Directory 'calls.tsv') (((@($CallsHeader) + $lines) -join "`n") + "`n")
}

# A call that was started and never finished: the runner was interrupted, no
# completion came back, and the call counts as one that failed at the
# provider.
function Close-Interrupted([string]$Directory) {
  for ($n = 1; $n -le 2; $n++) {
    $started = Join-Path $Directory "call-$n.started"
    if ((Test-Path -LiteralPath $started) -and -not (Test-Path -LiteralPath (Join-Path $Directory "call-$n.tsv"))) {
      $began = [IO.File]::ReadAllText($started).Trim()
      if (Test-Path -LiteralPath (Join-Path $Directory 'completion.txt')) {
        $line = "$n`t$began`t-`t-`t-`tfalse`tcompletion`tinterrupted after the completion was stored`t-`t-`t-"
      }
      else {
        $line = "$n`t$began`t-`t-`t-`tfalse`tfailed`tinterrupted: the runner stopped before the call ended`t-`t-`t-"
      }
      Write-Text (Join-Path $Directory "call-$n.tsv") ($line + "`n")
      Remove-Item -LiteralPath $started
      Write-Calls $Directory
    }
  }
}

function Get-CallOutcomes([string]$Directory) {
  $outcomes = [Collections.Generic.List[string]]::new()
  foreach ($line in (Get-CallLines $Directory)) { $outcomes.Add(($line -split "`t")[6]) }
  return , $outcomes
}

# The solution is the lines after the first line that begins with three
# backticks, up to the next such line or the end; with no such line, the
# whole text ("Extraction").
function Get-Solution([byte[]]$Completion) {
  $starts = [Collections.Generic.List[int]]::new()
  $starts.Add(0)
  for ($i = 0; $i -lt $Completion.Length; $i++) { if ($Completion[$i] -eq 10 -and $i + 1 -lt $Completion.Length) { $starts.Add($i + 1) } }
  $fence = { param([int]$at) $at + 2 -lt $Completion.Length -and $Completion[$at] -eq 96 -and $Completion[$at + 1] -eq 96 -and $Completion[$at + 2] -eq 96 }
  $open = -1
  for ($k = 0; $k -lt $starts.Count; $k++) { if (& $fence $starts[$k]) { $open = $k; break } }
  if ($open -lt 0) { return , $Completion }
  if ($open + 1 -ge $starts.Count) { return , [byte[]]::new(0) }
  $from = $starts[$open + 1]
  $to = $Completion.Length
  for ($k = $open + 1; $k -lt $starts.Count; $k++) { if (& $fence $starts[$k]) { $to = $starts[$k]; break } }
  $solution = [byte[]]::new($to - $from)
  [Array]::Copy($Completion, $from, $solution, 0, $to - $from)
  return , $solution
}

function Get-GradeLine([string]$TaskName, [string]$ArmName, [string]$File, $Toolchain) {
  $directory = New-EmptyDirectory
  try {
    $arguments = @('-NoProfile', '-File', (Join-Path $Pilot 'grade.ps1'), $TaskName, $ArmName, $File, '-Vibra', $Toolchain.Vibra)
    $saved = $StubClient
    $script:StubClient = ''
    try { $ended = Invoke-Process $Pwsh $arguments ([byte[]]::new(0)) (Get-CallEnvironment @{}).Variables $directory 3600 }
    finally { $script:StubClient = $saved }
  }
  finally { Remove-Item -LiteralPath $directory -Recurse -Force -ErrorAction SilentlyContinue }
  $text = $Utf8.GetString($ended.Stdout).TrimEnd("`r", "`n")
  if ($ended.Exit -eq 0 -and $text -ne '' -and -not $text.Contains("`n") -and $text.StartsWith("$TaskName`t$ArmName`t")) { return $text }
  $why = ($Utf8.GetString($ended.Stderr) + ' ' + $text).Trim() -replace '\s+', ' '
  return "$TaskName`t$ArmName`tgraderError`tclosed=-`tcheck=-`ttest=-`tcodes=-`tfailed=-`treason=the grader failed: $why"
}

$TableHeader = "index`trun`tcohort`tposition`ttask`tarm`tattempt`tpromptSha256`tcalls`toutcome`treportedModel`tusage`tgrade`tclosed`tcheck`ttest`tcodes`tfailed`treason"

function Get-TableLine($Entry, $Record) {
  $grade = @('-', '-', '-', '-', '-', '-', '-')
  if ($Record['outcome'] -eq 'completion') {
    $fields = [IO.File]::ReadAllText((Join-Path (Join-Path $Records $Entry.Run) 'grade.txt')).TrimEnd("`n") -split "`t"
    $named = @{}
    foreach ($field in $fields[3..($fields.Count - 1)]) { $k, $v = $field -split '=', 2; $named[$k] = $v }
    $grade = @($fields[2], $named['closed'], $named['check'], $named['test'], $named['codes'], $named['failed'], $(if ($named.ContainsKey('reason')) { $named['reason'] } else { '-' }))
  }
  else { $grade[0] = $Record['outcome'] }
  return (@($Entry.Index, $Entry.Run, $Entry.Cohort, $Entry.Position, $Entry.Task, $Entry.Arm, $Entry.Attempt, $Record['promptSha256'], $Record['calls'], $Record['outcome'], $Record['reportedModel'], $Record['usage']) + $grade) -join "`t"
}

# runs.tsv: one line per finished run, in schedule order. The two cohorts
# may write it at the same time, so it is written under a lock.
function Write-Table($Schedule) {
  $mutex = [Threading.Mutex]::new($false, 'vibraPilotRunsTable')
  $null = $mutex.WaitOne()
  try {
    $lines = [Collections.Generic.List[string]]::new()
    $lines.Add($TableHeader)
    foreach ($entry in $Schedule) {
      $file = Join-Path (Join-Path $Records $entry.Run) 'record.tsv'
      if (-not (Test-Path -LiteralPath $file)) { continue }
      $record = Read-Record $file
      if ($record['outcome'] -notin $Final) { continue }
      $lines.Add((Get-TableLine $entry $record))
    }
    $temporary = "$Table.$PID.tmp"
    Write-Text $temporary (($lines -join "`n") + "`n")
    Move-Item -LiteralPath $temporary -Destination $Table -Force
  }
  finally { $mutex.ReleaseMutex(); $mutex.Dispose() }
}

function Get-UsageText($Usage) { if ($null -ne $Usage) { return ConvertTo-Compact $Usage } return '-' }

# The record of a run, written last: its presence with a final outcome means
# the run is finished.
function Save-Record([string]$Directory, $Entry, $Context, [string]$Outcome) {
  $calls = Get-CallLines $Directory
  $first = if ($calls.Count) { ($calls[0] -split "`t")[1] } else { '-' }
  $end = if ($calls.Count) { ($calls[-1] -split "`t")[2] } else { '-' }
  $lastFields = if ($calls.Count) { $calls[-1] -split "`t" } else { $null }
  $fields = [ordered]@{
    kind = 'run'; run = $Entry.Run; index = $Entry.Index; cohort = $Entry.Cohort; position = $Entry.Position
    task = $Entry.Task; arm = $Entry.Arm; attempt = $Entry.Attempt
    promptSha256 = $Entry.PromptSha256; promptBytes = $Context.Reg.Prompts["$($Entry.Task)/$($Entry.Arm)"].Bytes.Length
    requestedModel = $Entry.Model; setting = $Entry.Setting; systemPrompt = $Context.Reg.System
    client = $Context.ClientName; clientVersion = $Context.ClientVersion
    freezeCommit = $Context.Toolchain.Freeze; vibraSha256 = $Context.Toolchain.VibraSha256
    startUtc = $first; endUtc = $end; calls = $calls.Count; outcome = $Outcome
    reportedModel = '-'; usage = '-'; outputCap = $Context.Cap; overCap = '-'
    completionSha256 = '-'; completionBytes = '-'; solutionSha256 = '-'; solutionBytes = '-'
    command = ConvertTo-Compact $Context.Command; removedEnvironment = $Context.Removed; grade = '-'
  }
  if ($null -ne $lastFields) {
    $fields.reportedModel = $lastFields[8]
    $fields.usage = $lastFields[9]
    $out = $null
    if ($lastFields[9] -ne '-') {
      $usage = $lastFields[9] | ConvertFrom-Json
      if ($usage.PSObject.Properties.Name -contains 'output_tokens') { $out = $usage.output_tokens }
    }
    $fields.overCap = if ($null -ne $out) { if ([long]$out -gt $OutputCap) { 'true' } else { 'false' } } else { 'unknown' }
  }
  $completionFile = Join-Path $Directory 'completion.txt'
  if ($Outcome -eq 'completion') {
    $completion = [IO.File]::ReadAllBytes($completionFile)
    $solution = Get-Solution $completion
    $solutionFile = Join-Path $Directory 'solution.vib'
    Write-Bytes $solutionFile $solution
    $fields.completionSha256 = Get-Sha256 $completion; $fields.completionBytes = $completion.Length
    $fields.solutionSha256 = Get-Sha256 $solution; $fields.solutionBytes = $solution.Length
    $line = Get-GradeLine $Entry.Task $Entry.Arm $solutionFile $Context.Toolchain
    Write-Text (Join-Path $Directory 'grade.txt') ($line + "`n")
    $fields.grade = $line.Replace("`t", ' | ')
  }
  # The grader's line keeps its tabs in grade.txt; the record holds it with
  # ' | ' between its fields.
  Write-Calls $Directory
  Write-Record (Join-Path $Directory 'record.tsv') $fields
}

# The number of calls that failed at the provider since the last completion,
# over the cohort's runs before the given position.
function Get-Consecutive($Schedule, [string]$CohortName, [int]$Before) {
  $count = 0
  foreach ($entry in ($Schedule | Where-Object { $_.Cohort -eq $CohortName -and $_.Position -lt $Before })) {
    $directory = Join-Path $Records $entry.Run
    if (-not (Test-Path -LiteralPath $directory)) { continue }
    foreach ($outcome in (Get-CallOutcomes $directory)) { if ($outcome -eq 'completion') { $count = 0 } else { $count++ } }
  }
  return $count
}

function Invoke-Run($Entry, $Context, $Schedule) {
  $directory = Join-Path $Records $Entry.Run
  $recordFile = Join-Path $directory 'record.tsv'
  if (Test-Path -LiteralPath $recordFile) {
    $record = Read-Record $recordFile
    if ($record['outcome'] -in $Final) { return $record['outcome'] }
  }
  $null = New-Item -ItemType Directory -Force -Path $directory
  Close-Interrupted $directory
  $consecutive = Get-Consecutive $Schedule $Entry.Cohort $Entry.Position
  $completionFile = Join-Path $directory 'completion.txt'
  if (-not (Test-Path -LiteralPath $completionFile)) {
    $outcomes = Get-CallOutcomes $directory
    foreach ($outcome in $outcomes) { if ($outcome -eq 'completion') { $consecutive = 0 } else { $consecutive++ } }
    $prompt = $Context.Reg.Prompts["$($Entry.Task)/$($Entry.Arm)"]
    if ((Get-Sha256 $prompt.Bytes) -ne $Entry.PromptSha256) { throw "refusing to send: the prompt of $($Entry.Run) is not its registered prompt" }
    for ($n = $outcomes.Count + 1; $n -le 2; $n++) {
      if ($consecutive -ge $StopAfter) { break }
      Write-Text (Join-Path $directory "call-$n.started") ((Get-Now) + "`n")
      $called = Invoke-Client $Context.Model $Context.Reg.System $prompt.Bytes
      $null = Save-Call $directory $n $called
      Remove-Item -LiteralPath (Join-Path $directory "call-$n.started")
      Write-Calls $directory
      Write-Host "$($Entry.Run) call $n`: $(if ($null -ne $called.Completion) { 'completion' } else { "failed ($($called.Reason))" })"
      if ($null -ne $called.Completion) { break }
      $consecutive++
    }
  }
  if (Test-Path -LiteralPath $completionFile) { $outcome = 'completion' }
  elseif ((Get-CallOutcomes $directory).Count -ge 2) { $outcome = 'providerError' }
  else { $outcome = 'stopped' }
  Save-Record $directory $Entry $Context $outcome
  return $outcome
}

# ---------------------------------------------------------------------------

$Reg = Get-Registered
$Schedule = Get-Schedule $Reg
$scheduleText = Format-Schedule $Schedule

if ($PSCmdlet.ParameterSetName -eq 'Schedule') {
  $null = New-Item -ItemType Directory -Force -Path $Records
  if (Test-Path -LiteralPath $ScheduleFile) {
    if ([IO.File]::ReadAllText($ScheduleFile) -cne $scheduleText) { throw 'runs/schedule.tsv differs from the schedule of the registration; it is never rewritten' }
    "schedule unchanged: $($Schedule.Count) runs"
  }
  else {
    Write-Text $ScheduleFile $scheduleText
    "schedule written: $($Schedule.Count) runs"
  }
  exit 0
}

if (-not (Test-Path -LiteralPath $ScheduleFile) -or [IO.File]::ReadAllText($ScheduleFile) -cne $scheduleText) { throw 'no schedule, or not the registered one: run -WriteSchedule and commit it before the first call' }
$model = $Reg.Models | Where-Object Key -eq $Cohort
$toolchain = Get-Toolchain
$null = New-Item -ItemType Directory -Force -Path $SetupDirectory

if ($model.Key -eq 'luna' -and -not (Test-Path -LiteralPath $CatalogFile)) {
  if (-not $Setup) { throw 'no luna catalog: the setup call writes it' }
  Write-Text $CatalogFile (New-LunaCatalog $Reg.System)
}

# What every call of the cohort is, as the records show it: the client's
# arguments, with the per-call empty directory and the catalog path named.
if ($model.Key -eq 'haiku') {
  $shownCommand = @('claude') + (Get-ClaudeArguments $model $Reg.System)
  $cap = "$OutputCap (CLAUDE_CODE_MAX_OUTPUT_TOKENS)"
}
else {
  $shownCommand = @('codex', '--no-daemon', 'exec', '--ignore-user-config', '--ignore-rules', '--ephemeral', '--skip-git-repo-check', '--json', '--model', $model.Identifier, '-C', '<empty temporary directory>') +
    ((Get-LunaConfiguration $CatalogFile $false) | ForEach-Object { if ($_.StartsWith('model_catalog_json=')) { 'model_catalog_json="<records>/setup/luna-catalog.json"' } else { $_ } }) + @('-')
  $cap = 'none (the client offers no output cap)'
}
$context = [pscustomobject]@{
  Reg = $Reg; Model = $model; Toolchain = $toolchain
  ClientName = if ($model.Key -eq 'haiku') { 'claude' } else { 'codex' }
  ClientVersion = Get-ClientVersion $model
  Command = $shownCommand; Cap = $cap; Removed = ((Get-CallEnvironment @{}).Removed -join ',')
}

function Get-SetupRecords([string]$CohortName) {
  return @(Get-ChildItem -LiteralPath $SetupDirectory -Directory -ErrorAction SilentlyContinue | Where-Object { $_.Name -match "^$CohortName-\d+$" } | Sort-Object { [int]($_.Name -split '-')[1] })
}

function Test-SetupPassed([string]$CohortName) {
  foreach ($directory in (Get-SetupRecords $CohortName)) {
    $file = Join-Path $directory.FullName 'record.tsv'
    if ((Test-Path -LiteralPath $file) -and (Read-Record $file)['setupPassed'] -eq 'true') { return $true }
  }
  return $false
}

if ($Setup) {
  if (Test-SetupPassed $Cohort) { "setup already passed for $Cohort"; exit 0 }
  $made = @(Get-SetupRecords $Cohort)
  if ($made.Count -ge 5) { throw "five setup calls of $Cohort were made and none passed: the cohort does not start" }
  $number = $made.Count + 1
  $directory = Join-Path $SetupDirectory "$Cohort-$number"
  $null = New-Item -ItemType Directory -Path $directory
  $prompt = $Utf8.GetBytes($Reg.SetupPrompt)
  Write-Text (Join-Path $directory 'call-1.started') ((Get-Now) + "`n")
  $called = Invoke-Client $model $Reg.System $prompt
  $null = Save-Call $directory 1 $called
  Remove-Item -LiteralPath (Join-Path $directory 'call-1.started')
  Write-Calls $directory
  # The Codex client's events name no model: a completion from the call that
  # requested the registered identifier, with no other identifier reported,
  # is a completion from that model.
  $identified = $called.Reported -ceq $model.Identifier -or ($model.Key -eq 'luna' -and $called.Reported -eq 'notReported')
  $passed = $null -ne $called.Completion -and $identified
  $why = if ($null -eq $called.Completion) { $called.Reason } elseif (-not $passed) { "the client reported the model '$($called.Reported)'" } else { '-' }
  $fields = [ordered]@{
    kind = 'setup'; cohort = $Cohort; number = $number; prompt = $Reg.SetupPrompt; promptSha256 = Get-Sha256 $prompt
    requestedModel = $model.Identifier; setting = $model.Setting; systemPrompt = $Reg.System
    client = $context.ClientName; clientVersion = $context.ClientVersion
    freezeCommit = $toolchain.Freeze; vibraSha256 = $toolchain.VibraSha256
    startUtc = $called.Ended.Start; endUtc = $called.Ended.End
    outcome = if ($null -ne $called.Completion) { 'completion' } else { 'failed' }
    reportedModel = Format-Value $called.Reported; usage = Get-UsageText $called.Usage; outputCap = $called.Cap
    setupPassed = if ($passed) { 'true' } else { 'false' }; reason = $why
    catalogSha256 = if ($model.Key -eq 'luna') { Get-Sha256 ([IO.File]::ReadAllBytes($CatalogFile)) } else { '-' }
    command = ConvertTo-Compact $called.Command; removedEnvironment = ($called.Removed -join ',')
  }
  Write-Record (Join-Path $directory 'record.tsv') $fields
  "setup $Cohort-$number`: $(if ($passed) { 'passed' } else { "failed: $why" })"
  exit $(if ($passed) { 0 } else { 2 })
}

if (-not (Test-SetupPassed $Cohort)) { throw "no setup call of $Cohort passed: the cohort does not start" }
$entries = @($Schedule | Where-Object Cohort -eq $Cohort)
$stopped = $false
foreach ($entry in $entries) {
  $outcome = Invoke-Run $entry $context $Schedule
  Write-Table $Schedule
  if ($outcome -eq 'stopped') { $stopped = $true }
}
if ($stopped) { "cohort $Cohort stopped by the stop rule"; exit 3 }
"cohort $Cohort complete"
exit 0
