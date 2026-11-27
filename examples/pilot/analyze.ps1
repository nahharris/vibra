#Requires -Version 7
<#
.SYNOPSIS
The registered analysis of the model-writing pilot.

.DESCRIPTION
`analyze.ps1 <runs.tsv>` reads the run table of
docs/roadmap/v0.0.2/m2/pilot/runs.tsv and prints, as Markdown tables, what
the registration's sections "Outcome measure", "Decision rule",
"Exclusions", and "What the analysis reports" ask for. The measure and the
rule read the run table alone. The setup calls are read from the run
records beside it (`runs/setup/<cohort>-<n>/record.tsv`), the one item
that is not in the table. The script takes no parameter that changes the
measure: the schedule, the arms, the grades, and the rule are the
registration's and are written below.

`analyze.ps1 -SelfCheck` applies the same rule to tables built by hand and
exits 0 only when each one gives the answer the registration gives it.

The script calls no model and runs no tool.
#>
[CmdletBinding(DefaultParameterSetName = 'Analyze')]
param(
  [Parameter(Mandatory, Position = 0, ParameterSetName = 'Analyze')]
  [string]$Table,
  [Parameter(Mandatory, ParameterSetName = 'SelfCheck')]
  [switch]$SelfCheck
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$Invariant = [Globalization.CultureInfo]::InvariantCulture

# ---------------------------------------------------------------------------
# The registration's fixed choices.

$Models = @('haiku', 'luna')
$Identifiers = @{ haiku = 'claude-haiku-5-5'; luna = 'gpt-6-luna' }
$Arms = @('endlet', 'noEndlet')
$Tasks = @('packet', 'pair', 'positive', 'range', 'order', 'gate', 'reader', 'plan', 'optional', 'rows')
$Attempts = 5
$Grades = @('passed', 'refuted', 'unknown', 'unsupported')
$OutputCap = 4096
$Columns = @('index', 'run', 'cohort', 'position', 'task', 'arm', 'attempt', 'promptSha256', 'calls', 'outcome', 'reportedModel', 'usage', 'grade', 'closed', 'check', 'test', 'codes', 'failed', 'reason')
$Outcomes = @('completion', 'providerError', 'stopped')
$TimeLimit = 'the grader''s time limit'
$NoDocument = 'the toolchain gave no document'

$StaysText = 'The default stays: `let … be … endlet` remains the specified form'
$FavouredText = 'The other arm is favoured: the question returns to Hannah, and this changes nothing by itself'

# ---------------------------------------------------------------------------
# The decision rule, in integers, as the registration writes it.

function Get-Answer($Counts, [string[]]$Stopped) {
  $rows = foreach ($model in $Models) {
    $c = $Counts[$model]
    [long]$pe = $c.endlet.p; [long]$ne = $c.endlet.n
    [long]$pn = $c.noEndlet.p; [long]$nn = $c.noEndlet.n
    $counted = $ne -ne 0 -and $nn -ne 0
    $difference = $pn * $ne - $pe * $nn
    [pscustomobject]@{
      Model = $model; Pe = $pe; Ne = $ne; Pn = $pn; Nn = $nn
      Counted = $counted; Difference = $difference
      AtLeast = $counted -and $difference -ge 0
      TenMore = $counted -and 10 * $difference -ge $ne * $nn
    }
  }
  $because =
    if ($Stopped.Count) { "because the cohort$(if ($Stopped.Count -gt 1) { 's' }) $(Join-Names $Stopped) stopped" }
    elseif (@($rows | Where-Object { -not $_.Counted }).Count) { "because $(Join-Names @($rows | Where-Object { -not $_.Counted } | ForEach-Object Model)) has an arm with no counted run" }
    elseif (@($rows | Where-Object { -not $_.AtLeast }).Count) { "because the ``noEndlet`` arm passes less often than the ``endlet`` arm on $(Join-Names @($rows | Where-Object { -not $_.AtLeast } | ForEach-Object Model))" }
    elseif (-not @($rows | Where-Object TenMore).Count) { 'because the `noEndlet` arm passes at least 10 percentage points more often on neither model' }
    else { $null }
  $favoured = $null -eq $because
  $sentence =
    if ($favoured) { "$FavouredText; the ``noEndlet`` arm passes at least as often on both models and at least 10 percentage points more often on $(Join-Names @($rows | Where-Object TenMore | ForEach-Object Model))." }
    else { "$StaysText, $because." }
  return [pscustomobject]@{ Favoured = $favoured; Rows = $rows; Sentence = $sentence }
}

function Join-Names([string[]]$Names) {
  $quoted = @($Names | ForEach-Object { "``$_``" })
  if ($quoted.Count -le 1) { return $quoted -join '' }
  return (($quoted[0..($quoted.Count - 2)]) -join ', ') + ' and ' + $quoted[-1]
}

# ---------------------------------------------------------------------------
# Formatting.

function Format-Count([long]$Value) { return $Value.ToString('N0', $Invariant) }

function Format-Rate([long]$P, [long]$N) {
  if ($N -eq 0) { return '-' }
  return (100.0 * $P / $N).ToString('0.0', $Invariant) + '%'
}

function Format-Row([object[]]$Cells) { return '| ' + ($Cells -join ' | ') + ' |' }

function Format-Header([string[]]$Names) {
  Format-Row $Names
  Format-Row @($Names | ForEach-Object { '---' })
}

function Format-YesNo([bool]$Value) { if ($Value) { 'yes' } else { 'no' } }

# ---------------------------------------------------------------------------
# Self-check: the rule on tables built by hand.

if ($SelfCheck) {
  function New-Counts([int[]]$Values) {
    return @{
      haiku = @{ endlet = @{ p = $Values[0]; n = $Values[1] }; noEndlet = @{ p = $Values[2]; n = $Values[3] } }
      luna = @{ endlet = @{ p = $Values[4]; n = $Values[5] }; noEndlet = @{ p = $Values[6]; n = $Values[7] } }
    }
  }
  # Each case: its name; haiku endlet p n, haiku noEndlet p n, luna endlet
  # p n, luna noEndlet p n; the stopped cohorts; and whether the registered
  # rule favours the noEndlet arm.
  function New-Case([string]$Name, [int[]]$Values, [string[]]$Stopped, [bool]$Favours) {
    return [pscustomobject]@{ Name = $Name; Values = $Values; Stopped = $Stopped; Favours = $Favours }
  }
  $cases = @(
    New-Case 'equal arms (a tie)' @(45, 50, 45, 50, 40, 50, 40, 50) @() $false
    New-Case 'better by exactly 5 of 50 on one model, equal on the other' @(40, 50, 45, 50, 40, 50, 40, 50) @() $true
    New-Case 'better by 4 of 50 on one model, equal on the other' @(40, 50, 44, 50, 40, 50, 40, 50) @() $false
    New-Case 'better by 5 of 50 on both models' @(40, 50, 45, 50, 30, 50, 35, 50) @() $true
    New-Case 'better on one model, worse on the other' @(40, 50, 48, 50, 45, 50, 44, 50) @() $false
    New-Case 'the endlet arm better by 10 points on both models' @(45, 50, 40, 50, 45, 50, 40, 50) @() $false
    New-Case 'a stopped cohort' @(40, 50, 50, 50, 40, 50, 50, 50) @('luna') $false
    New-Case 'exclusions: 40 of 50 against 44 of 48, equal on the other' @(40, 50, 44, 48, 40, 50, 40, 50) @() $true
    New-Case 'exclusions: 40 of 50 against 41 of 46, equal on the other' @(40, 50, 41, 46, 40, 50, 40, 50) @() $false
    New-Case 'an arm with no counted run' @(40, 50, 45, 50, 0, 0, 40, 50) @() $false
  )
  '## Self-check: the rule on tables built by hand'
  ''
  'Each cell is `p/n`.'
  ''
  Format-Header @('Table', '`haiku` `endlet`', '`haiku` `noEndlet`', '`luna` `endlet`', '`luna` `noEndlet`', 'Stopped', 'Expected', 'Answer')
  $good = 0
  foreach ($case in $cases) {
    $v = $case.Values
    $answer = Get-Answer (New-Counts $v) $case.Stopped
    $expected = if ($case.Favours) { 'favoured' } else { 'stays' }
    $got = if ($answer.Favoured) { 'favoured' } else { 'stays' }
    if ($got -eq $expected) { $good++ }
    $stopped = if ($case.Stopped.Count) { Join-Names $case.Stopped } else { '-' }
    Format-Row @($case.Name, "$($v[0])/$($v[1])", "$($v[2])/$($v[3])", "$($v[4])/$($v[5])", "$($v[6])/$($v[7])", $stopped, $expected, $got)
  }
  ''
  foreach ($case in $cases) {
    $sentence = (Get-Answer (New-Counts $case.Values) $case.Stopped).Sentence
    "- $($case.Name): $sentence"
  }
  ''
  "selfCheck`t$good of $($cases.Count) tables give their registered answer"
  exit $(if ($good -eq $cases.Count) { 0 } else { 1 })
}

# ---------------------------------------------------------------------------
# Read and validate the run table.

$tablePath = (Resolve-Path -LiteralPath $Table).Path
$lines = [IO.File]::ReadAllLines($tablePath, [Text.UTF8Encoding]::new($false))
if ((($lines[0] -split "`t") -join ',') -cne ($Columns -join ',')) { throw "the run table's header is not the registered one" }
$runs = foreach ($line in $lines | Select-Object -Skip 1) {
  if ($line -eq '') { continue }
  $cells = $line -split "`t"
  if ($cells.Count -ne $Columns.Count) { throw "a line of the run table has $($cells.Count) fields: $line" }
  $row = [ordered]@{}
  for ($i = 0; $i -lt $Columns.Count; $i++) { $row[$Columns[$i]] = $cells[$i] }
  [pscustomobject]$row
}

# The registered schedule: per cohort, attempt 1 to 5; within an attempt the
# tasks in order; within a task endlet then noEndlet for an odd attempt and
# the reverse for an even one.
$schedule = foreach ($model in $Models) {
  $position = 0
  foreach ($attempt in 1..$Attempts) {
    foreach ($task in $Tasks) {
      $order = if ($attempt % 2) { $Arms } else { @($Arms[1], $Arms[0]) }
      foreach ($arm in $order) {
        $position++
        [pscustomobject]@{ Run = "$model-$task-$arm-$attempt"; Cohort = $model; Position = $position; Task = $task; Arm = $arm; Attempt = $attempt }
      }
    }
  }
}
$byRun = @{}
foreach ($row in $runs) {
  if ($byRun.ContainsKey($row.run)) { throw "the run $($row.run) is in the table twice" }
  $byRun[$row.run] = $row
}
foreach ($row in $runs) {
  $entry = $schedule | Where-Object Run -ceq $row.run
  if ($null -eq $entry) { throw "the run $($row.run) is not in the registered schedule" }
  if ($row.cohort -cne $entry.Cohort -or $row.task -cne $entry.Task -or $row.arm -cne $entry.Arm -or [int]$row.attempt -ne $entry.Attempt -or [int]$row.position -ne $entry.Position) {
    throw "the fields of $($row.run) disagree with its place in the schedule"
  }
  if ($row.outcome -notin $Outcomes) { throw "the run $($row.run) has the outcome '$($row.outcome)'" }
}

# A cohort with a stopped run stopped; a cohort that did not stop has every
# scheduled run in the table.
$stoppedCohorts = [string[]]@($Models | Where-Object { $m = $_; @($runs | Where-Object { $_.cohort -eq $m -and $_.outcome -eq 'stopped' }).Count })
foreach ($model in $Models) {
  if ($model -in $stoppedCohorts) { continue }
  $missing = @($schedule | Where-Object { $_.Cohort -eq $model -and -not $byRun.ContainsKey($_.Run) })
  if ($missing.Count) { throw "the cohort $model did not stop and $($missing.Count) of its runs are not in the table" }
}

function Test-Counted($Row) { return $Row.cohort -notin $stoppedCohorts -and $Row.outcome -eq 'completion' }
function Test-Pass($Row) { return (Test-Counted $Row) -and $Row.grade -ceq 'passed' }

function Get-Measure([scriptblock]$Where) {
  $selected = @($runs | Where-Object $Where | Where-Object { Test-Counted $_ })
  return @{ n = $selected.Count; p = @($selected | Where-Object { Test-Pass $_ }).Count }
}

$counts = @{}
foreach ($model in $Models) {
  $counts[$model] = @{}
  foreach ($arm in $Arms) { $counts[$model][$arm] = Get-Measure { $_.cohort -eq $model -and $_.arm -eq $arm }.GetNewClosure() }
}

# ---------------------------------------------------------------------------
# The report.

"# Pilot analysis"
''
"Run table: ``$(Split-Path -Leaf $tablePath)``, $($runs.Count) runs of the $($schedule.Count) scheduled."
''

'## Exclusions'
''
Format-Header @('Reason', '`haiku`', '`luna`', 'All')
$provider = @{}; $stoppedRuns = @{}; $countedRuns = @{}; $scheduled = @{}
foreach ($model in $Models) {
  $scheduled[$model] = @($schedule | Where-Object Cohort -eq $model).Count
  $stoppedRuns[$model] = if ($model -in $stoppedCohorts) { $scheduled[$model] } else { 0 }
  $provider[$model] = if ($model -in $stoppedCohorts) { 0 } else { @($runs | Where-Object { $_.cohort -eq $model -and $_.outcome -eq 'providerError' }).Count }
  $countedRuns[$model] = @($runs | Where-Object { $_.cohort -eq $model -and (Test-Counted $_) }).Count
  if ($provider[$model] + $stoppedRuns[$model] + $countedRuns[$model] -ne $scheduled[$model]) { throw "the runs of $model do not add up to its schedule" }
}
function Get-Total($Map) { return [long](($Models | ForEach-Object { $Map[$_] }) | Measure-Object -Sum).Sum }
Format-Row @('both calls failed at the provider (`providerError`)', $provider.haiku, $provider.luna, (Get-Total $provider))
Format-Row @('a stopped cohort', $stoppedRuns.haiku, $stoppedRuns.luna, (Get-Total $stoppedRuns))
Format-Row @('excluded', ($provider.haiku + $stoppedRuns.haiku), ($provider.luna + $stoppedRuns.luna), ((Get-Total $provider) + (Get-Total $stoppedRuns)))
Format-Row @('counted in `n`', $countedRuns.haiku, $countedRuns.luna, (Get-Total $countedRuns))
Format-Row @('scheduled', $scheduled.haiku, $scheduled.luna, (Get-Total $scheduled))
''

'## Outcome measure per model and arm'
''
Format-Header @('Model', 'Arm', 'n', 'p', 'Pass rate')
foreach ($model in $Models) {
  foreach ($arm in $Arms) {
    $c = $counts[$model][$arm]
    Format-Row @("``$model``", "``$arm``", $c.n, $c.p, (Format-Rate $c.p $c.n))
  }
}
''

'## Outcome measure per task'
''
'Each cell is `p/n`.'
''
Format-Header @('Task', '`haiku` `endlet`', '`haiku` `noEndlet`', '`luna` `endlet`', '`luna` `noEndlet`', 'Both `endlet`', 'Both `noEndlet`')
for ($i = 0; $i -lt $Tasks.Count; $i++) {
  $task = $Tasks[$i]
  $cells = @("T$($i + 1) ``$task``")
  foreach ($model in $Models) {
    foreach ($arm in $Arms) {
      $c = Get-Measure { $_.cohort -eq $model -and $_.arm -eq $arm -and $_.task -eq $task }.GetNewClosure()
      $cells += "$($c.p)/$($c.n)"
    }
  }
  foreach ($arm in $Arms) {
    $c = Get-Measure { $_.arm -eq $arm -and $_.task -eq $task }.GetNewClosure()
    $cells += "$($c.p)/$($c.n)"
  }
  Format-Row $cells
}
''

'## Outcome measure overall'
''
Format-Header @('Arm', 'n', 'p', 'Pass rate')
foreach ($arm in $Arms) {
  $c = Get-Measure { $_.arm -eq $arm }.GetNewClosure()
  Format-Row @("``$arm``", $c.n, $c.p, (Format-Rate $c.p $c.n))
}
''

'## Decision rule'
''
$answer = Get-Answer $counts $stoppedCohorts
Format-Header @('Model', 'p(m, endlet)', 'n(m, endlet)', 'p(m, noEndlet)', 'n(m, noEndlet)', 'd = p(m, noEndlet) × n(m, endlet) − p(m, endlet) × n(m, noEndlet)', 'd ≥ 0', '10 × d', 'n(m, endlet) × n(m, noEndlet)', '10 × d ≥ n × n')
foreach ($row in $answer.Rows) {
  Format-Row @("``$($row.Model)``", $row.Pe, $row.Ne, $row.Pn, $row.Nn, $row.Difference, (Format-YesNo $row.AtLeast), (10 * $row.Difference), ($row.Ne * $row.Nn), (Format-YesNo $row.TenMore))
}
''
"Stopped cohorts: $(if ($stoppedCohorts.Count) { Join-Names $stoppedCohorts } else { 'none' })."
''
"Answer: $($answer.Sentence)"
''

'## Grades per model and arm'
''
$present = @($runs | Where-Object { Test-Counted $_ } | ForEach-Object grade | Sort-Object -Unique)
$gradeColumns = @($Grades) + @($present | Where-Object { $_ -notin $Grades })
Format-Header (@('Model', 'Arm') + @($gradeColumns | ForEach-Object { "``$_``" }) + @('All'))
foreach ($model in $Models) {
  foreach ($arm in $Arms) {
    $selected = @($runs | Where-Object { $_.cohort -eq $model -and $_.arm -eq $arm -and (Test-Counted $_) })
    $cells = @("``$model``", "``$arm``")
    foreach ($grade in $gradeColumns) { $cells += @($selected | Where-Object grade -ceq $grade).Count }
    Format-Row ($cells + @($selected.Count))
  }
}
''

'## Diagnostic codes of the runs that do not pass, per arm'
''
'A cell counts the runs that do not pass and show the code.'
''
$failing = @($runs | Where-Object { (Test-Counted $_) -and -not (Test-Pass $_) })
Format-Header @('Arm', 'Code', '`haiku`', '`luna`', 'All')
foreach ($arm in $Arms) {
  $ofArm = @($failing | Where-Object arm -eq $arm)
  $codes = @($ofArm | ForEach-Object { if ($_.codes -ne '-') { $_.codes -split ',' } } | Sort-Object -Unique)
  if (-not $codes.Count) { Format-Row @("``$arm``", 'none', 0, 0, 0); continue }
  foreach ($code in $codes) {
    $with = @($ofArm | Where-Object { $code -in ($_.codes -split ',') })
    Format-Row @("``$arm``", "``$code``", @($with | Where-Object cohort -eq 'haiku').Count, @($with | Where-Object cohort -eq 'luna').Count, $with.Count)
  }
  $without = @($ofArm | Where-Object codes -eq '-')
  if ($without.Count) { Format-Row @("``$arm``", 'no code', @($without | Where-Object cohort -eq 'haiku').Count, @($without | Where-Object cohort -eq 'luna').Count, $without.Count) }
}
''

'## Runs that do not pass'
''
if ($failing.Count) {
  Format-Header @('Run', 'Grade', 'Closed', '`check`', '`test`', 'Codes', 'Failing tests', 'Reason')
  foreach ($row in $failing) {
    $codes = if ($row.codes -ne '-') { ($row.codes -split ',' | ForEach-Object { "``$_``" }) -join ', ' } else { '-' }
    Format-Row @("``$($row.run)``", "``$($row.grade)``", $row.closed, "``$($row.check)``", "``$($row.test)``", $codes, $row.failed, $row.reason)
  }
}
else { 'None.' }
''

'## Own surface: `noEndlet` runs with a closed count above zero'
''
'"Would otherwise have passed" reads the recorded results of both commands as `@command.ok`.'
''
Format-Header @('Model', '`noEndlet` runs', 'closed count above zero', 'would otherwise have passed')
foreach ($model in $Models) {
  $selected = @($runs | Where-Object { $_.cohort -eq $model -and $_.arm -eq 'noEndlet' -and (Test-Counted $_) })
  $closed = @($selected | Where-Object { $_.closed -ne '-' -and [int]$_.closed -gt 0 })
  $otherwise = @($closed | Where-Object { $_.check -ceq '@command.ok' -and $_.test -ceq '@command.ok' })
  Format-Row @("``$model``", $selected.Count, $closed.Count, $otherwise.Count)
}
''
'The number of `noEndlet` solutions the translation changed is not stored: the run table and the run records hold each run''s closed count, and neither holds the translated text or whether it differs from the solution.'
''

'## Time limit and no document'
''
$limited = @($runs | Where-Object { $_.check -eq 'stopped' -or $_.test -eq 'stopped' -or $_.check -like 'noDocument*' -or $_.test -like 'noDocument*' -or $_.reason -eq $TimeLimit -or $_.reason -eq $NoDocument })
if ($limited.Count) {
  Format-Header @('Run', '`check`', '`test`', 'Reason')
  foreach ($row in $limited) { Format-Row @("``$($row.run)``", $row.check, $row.test, $row.reason) }
}
else { 'No command was stopped at the time limit and every command gave a document.' }
''

'## Calls and reported models'
''
Format-Header @('Model', 'Requested', 'Runs', 'Calls', 'Retries', 'Reported model', 'Runs reporting it')
foreach ($model in $Models) {
  $selected = @($runs | Where-Object cohort -eq $model)
  $calls = ($selected | ForEach-Object { [int]$_.calls } | Measure-Object -Sum).Sum
  $reported = @($selected | Group-Object reportedModel | Sort-Object Name)
  $first = $true
  foreach ($group in $reported) {
    if ($first) { Format-Row @("``$model``", "``$($Identifiers[$model])``", $selected.Count, $calls, ($calls - $selected.Count), "``$($group.Name)``", $group.Count) }
    else { Format-Row @('', '', '', '', '', "``$($group.Name)``", $group.Count) }
    $first = $false
  }
}
''
$other = @($runs | Where-Object { $_.reportedModel -cne $Identifiers[$_.cohort] -and $_.reportedModel -cne 'notReported' })
if ($other.Count) { $names = Join-Names @($other | ForEach-Object run); "Runs whose provider reports another model identifier: $names." }
else { 'No run reports a model identifier other than the registered one.' }
''

'## Reported usage of the runs'
''
'The usage each client reported, summed over the cohort''s calls, as the run table records it.'
''
function Get-UsageFields($Usage) {
  $fields = [ordered]@{}
  foreach ($property in $Usage.PSObject.Properties) {
    if ($property.Value -is [long] -or $property.Value -is [int]) { $fields[$property.Name] = [long]$property.Value }
    elseif ($property.Name -eq 'output_tokens_details' -and $null -ne $property.Value) {
      foreach ($inner in $property.Value.PSObject.Properties) { if ($inner.Value -is [long] -or $inner.Value -is [int]) { $fields["output_tokens_details.$($inner.Name)"] = [long]$inner.Value } }
    }
  }
  return $fields
}
Format-Header @('Model', 'Field', 'Sum', 'Largest call')
$overCap = @()
foreach ($model in $Models) {
  $sums = [ordered]@{}; $largest = @{}
  foreach ($row in @($runs | Where-Object { $_.cohort -eq $model -and $_.usage -ne '-' })) {
    $fields = Get-UsageFields ($row.usage | ConvertFrom-Json)
    foreach ($name in $fields.Keys) {
      if (-not $sums.Contains($name)) { $sums[$name] = [long]0; $largest[$name] = [long]0 }
      $sums[$name] += $fields[$name]
      if ($fields[$name] -gt $largest[$name]) { $largest[$name] = $fields[$name] }
    }
    if ($fields.Contains('output_tokens') -and $fields['output_tokens'] -gt $OutputCap) { $overCap += [pscustomobject]@{ Run = $row.run; Output = $fields['output_tokens'] } }
  }
  foreach ($name in $sums.Keys) { Format-Row @("``$model``", "``$name``", (Format-Count $sums[$name]), (Format-Count $largest[$name])) }
}
''
if ($overCap.Count) { $names = @($overCap | ForEach-Object { '`' + $_.Run + '` (' + (Format-Count $_.Output) + ')' }) -join ', '; "Calls reporting more than $(Format-Count $OutputCap) output tokens: $names." }
else { "No call reports more than $(Format-Count $OutputCap) output tokens." }
''
'## Reported tokens per call'
''
'A call''s sum adds the fields named here. They are the clients'' own: `haiku` reports cache reads and writes beside `input_tokens`, and its thinking tokens are part of `output_tokens`; `luna`''s cached input and reasoning output are parts of its `input_tokens` and `output_tokens`, so they are not added.'
''
$CallCeiling = 20623
$PilotCeiling = 8249200
$SummedFields = @{
  haiku = @('input_tokens', 'cache_creation_input_tokens', 'cache_read_input_tokens', 'output_tokens')
  luna = @('input_tokens', 'output_tokens')
}
Format-Header @('Model', 'A call''s sum', 'Calls', 'Largest sum', 'Its run', "Calls above $(Format-Count $CallCeiling)", 'Cohort sum')
$callSums = @()
foreach ($model in $Models) {
  $sums = foreach ($row in @($runs | Where-Object { $_.cohort -eq $model -and $_.usage -ne '-' })) {
    $fields = Get-UsageFields ($row.usage | ConvertFrom-Json)
    [long]$sum = 0
    foreach ($name in $SummedFields[$model]) {
      if (-not $fields.Contains($name)) { throw "the usage of $($row.run) has no $name" }
      $sum += $fields[$name]
    }
    [pscustomobject]@{ Run = $row.run; Sum = $sum }
  }
  $sums = @($sums | Sort-Object -Property @{ Expression = 'Sum'; Descending = $true }, @{ Expression = 'Run' })
  $callSums += $sums
  $total = [long]($sums | Measure-Object -Property Sum -Sum).Sum
  $named = ($SummedFields[$model] | ForEach-Object { "``$_``" }) -join ' + '
  Format-Row @("``$model``", $named, $sums.Count, (Format-Count $sums[0].Sum), "``$($sums[0].Run)``", @($sums | Where-Object Sum -gt $CallCeiling).Count, (Format-Count $total))
}
''
$above = @($callSums | Where-Object Sum -gt $CallCeiling)
$rest = @($callSums | Where-Object Sum -le $CallCeiling | Sort-Object Sum -Descending)
$aboveText = if ($above.Count) { (@($above | ForEach-Object { '`' + $_.Run + '` (' + (Format-Count $_.Sum) + ')' }) -join ', ') } else { 'none' }
"Calls whose sum is above the registration's per-call ceiling of $(Format-Count $CallCeiling): $aboveText. Every other call sums at most $(Format-Count $rest[0].Sum) (``$($rest[0].Run)``). The registration's ceiling of the pilot is $(Format-Count $PilotCeiling)."
''

'## Setup calls'
''
'Read from `runs/setup/<cohort>-<n>/record.tsv` beside the run table.'
''
$setupDirectory = Join-Path (Join-Path (Split-Path -Parent $tablePath) 'runs') 'setup'
Format-Header @('Model', 'Setup calls', 'Passed', 'Reported model', 'Usage')
foreach ($model in $Models) {
  $directories = @(Get-ChildItem -LiteralPath $setupDirectory -Directory -ErrorAction SilentlyContinue | Where-Object Name -match "^$model-\d+$" | Sort-Object { [int]($_.Name -split '-')[1] })
  if (-not $directories.Count) { Format-Row @("``$model``", 0, 0, '-', '-'); continue }
  $passed = 0; $reportedModels = @(); $usages = @()
  foreach ($directory in $directories) {
    $record = @{}
    foreach ($line in [IO.File]::ReadAllLines((Join-Path $directory.FullName 'record.tsv'))) {
      $at = $line.IndexOf("`t"); if ($at -gt 0) { $record[$line.Substring(0, $at)] = $line.Substring($at + 1) }
    }
    if ($record['setupPassed'] -eq 'true') { $passed++ }
    $reportedModels += "``$($record['reportedModel'])``"
    if ($record['usage'] -ne '-') {
      $fields = Get-UsageFields ($record['usage'] | ConvertFrom-Json)
      $usages += (($fields.Keys | Where-Object { $_ -in 'input_tokens', 'cache_creation_input_tokens', 'cache_read_input_tokens', 'cached_input_tokens', 'output_tokens' } | ForEach-Object { "$_ $(Format-Count $fields[$_])" }) -join ', ')
    }
  }
  Format-Row @("``$model``", $directories.Count, $passed, (($reportedModels | Sort-Object -Unique) -join ', '), ($usages -join '; '))
}
