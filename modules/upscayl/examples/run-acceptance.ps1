<#
.SYNOPSIS
Run every example graph through `hollowdeck run` and record what happened, for
docs/results/acceptance-v1.md (issue #15).

.DESCRIPTION
Graphs 01-07 run attended (--attended); 08 and 09 run unattended, as Task Scheduler would.
Each run's console output goes to docs/results/acceptance-v1/raw/<graph>.txt and a row
(exit code, run status, summary, refusals, the upscayl event-log lines) to results.json.
The repository root is written as <repo> so the results hold no machine paths.

Run make-long-batch.ps1 first, and scripts/sync-engine.ps1 before that.
#>
param(
  [string]$Hollowdeck = (Join-Path $PSScriptRoot '..\..\..\..\HollowDeck\target\release\hollowdeck.exe'),
  [string]$DataDir = $env:HDECK_DATA_DIR,
  [string[]]$Only = @()
)
$ErrorActionPreference = 'Stop'
if (-not $DataDir) { throw 'Set -DataDir or HDECK_DATA_DIR: runs, logs and module data go there.' }
$env:HDECK_DATA_DIR = $DataDir
$here = $PSScriptRoot
$repo = (Resolve-Path (Join-Path $here '..\..\..')).Path
$results = Join-Path $repo 'docs\results\acceptance-v1'
$raw = Join-Path $results 'raw'
New-Item -ItemType Directory -Force $raw | Out-Null
$project = Join-Path $here 'upscayl-examples.hollow'
$events = Join-Path $DataDir 'logs\events.jsonl'

# UTF-8 without a byte-order mark, LF endings, on Windows PowerShell 5 too.
function Write-Utf8([string]$path, [string]$text) {
  [IO.File]::WriteAllText($path, $text.Replace("`r`n", "`n"), (New-Object Text.UTF8Encoding($false)))
}

function Scrub([string]$s) {
  if ($null -eq $s) { return $s }
  $s.Replace($repo, '<repo>').Replace($repo.Replace('\', '\\'), '<repo>')
}

$rows = @()
foreach ($g in Get-ChildItem $here -Filter '*.json' | Where-Object Name -Match '^\d\d-' | Sort-Object Name) {
  if ($Only.Count -and -not ($Only | Where-Object { $g.Name.StartsWith($_) })) { continue }
  $attended = -not ($g.Name -match '^0[89]-')
  $runArgs = @('--project', $project, 'run', $g.FullName)
  if ($attended) { $runArgs += '--attended' }
  $before = @(Get-ChildItem (Join-Path $DataDir 'runs') -Directory -ErrorAction SilentlyContinue | ForEach-Object Name)
  $t0 = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() / 1000.0
  $out = & $Hollowdeck @runArgs 2>&1 | ForEach-Object { "$_" }
  $code = $LASTEXITCODE
  $t1 = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() / 1000.0
  Write-Utf8 (Join-Path $raw ($g.BaseName + '.txt')) (Scrub (($out -join "`n") + "`n"))
  $runDir = Get-ChildItem (Join-Path $DataDir 'runs') -Directory | Where-Object { $before -notcontains $_.Name } | Sort-Object Name | Select-Object -Last 1
  $run = if ($runDir -and (Test-Path (Join-Path $runDir.FullName 'run.json'))) { Get-Content -Raw (Join-Path $runDir.FullName 'run.json') | ConvertFrom-Json } else { $null }
  $lines = @()
  if (Test-Path $events) {
    $lines = Get-Content $events | ForEach-Object { try { $_ | ConvertFrom-Json } catch { } } |
      Where-Object { $_.source -eq 'upscayl' -and $_.ts -ge $t0 -and $_.ts -le ($t1 + 1) } |
      ForEach-Object { [ordered]@{ event = $_.event; message = (Scrub $_.message); model = (Scrub $_.fields.model); license = $_.fields.license; args = (Scrub $_.fields.args) } }
  }
  $rows += [pscustomobject][ordered]@{
    graph = $g.Name
    command = 'hollowdeck --project modules/upscayl/examples/upscayl-examples.hollow run modules/upscayl/examples/' + $g.Name + $(if ($attended) { ' --attended' } else { '' })
    exit_code = $code
    seconds = [math]::Round($t1 - $t0, 1)
    status = $run.status
    error = (Scrub $run.error)
    refused = $run.refused
    summary = (Scrub $run.summary)
    events = @($lines)
  }
  "{0}  exit {1}  {2}  {3:N1} s" -f $g.Name, $code, $run.status, ($t1 - $t0)
}
$file = Join-Path $results 'results.json'
$existing = @()
if ($Only.Count -and (Test-Path $file)) {
  # Windows PowerShell 5 emits a parsed JSON array as one object; piping the variable
  # enumerates it.
  $parsed = Get-Content -Raw $file | ConvertFrom-Json
  $existing = @($parsed | Where-Object { $_.graph -and ($rows.graph -notcontains $_.graph) })
}
$all = @(@($existing) + @($rows) | Sort-Object graph)
# -InputObject keeps a one-row result an array on Windows PowerShell 5.
Write-Utf8 $file ((ConvertTo-Json -InputObject $all -Depth 6) + "`n")
