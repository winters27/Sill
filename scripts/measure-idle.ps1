<#
.SYNOPSIS
  Measures what Sill costs while nobody is using it.

.DESCRIPTION
  The launcher's whole premise is that it disappears when it is not being
  used, and that is a claim about numbers rather than a feeling. This produces
  those numbers the same way every time so a change can be shown to have
  helped, and so a regression is visible before it ships.

  It measures the whole process tree, not just sill.exe. The Rust core is
  around a tenth of the footprint; the rest is WebView2 and, if an extension
  has been opened, Node. Reporting only the part with our name on it would be
  flattering and useless.

  Two samples, because they answer different questions. The first is taken
  once startup has settled and is what a user meets. The second is taken
  minutes later and is what the launcher costs for the rest of the day; a
  timer that fires every few seconds is invisible in the first and obvious in
  the second.

.PARAMETER Exe
  Which binary to run. Defaults to the release build.

.PARAMETER Label
  Written into the output so two runs can be told apart, e.g. -Label before.

.PARAMETER SteadySeconds
  How long to wait before the second sample. The default is deliberately
  longer than any polling interval in the codebase.

.EXAMPLE
  powershell -File scripts/measure-idle.ps1 -Label before
  powershell -File scripts/measure-idle.ps1 -Label after

.NOTES
  Do not run this from inside a Claude Code session. Session-spawned processes
  get a virtualised %APPDATA%, so the launcher builds a throwaway profile
  instead of reading the real one, and the index size will not be yours.
  See Brain/references/Claude_Code_Sandbox.md.
#>
[CmdletBinding()]
param(
    # Resolved below rather than here. `$PSScriptRoot` is not yet bound while
    # parameter defaults are evaluated under Windows PowerShell, so a default
    # written here silently becomes a bare relative path.
    [string]$Exe,
    [string]$Label = 'run',
    [int]$SettleSeconds = 35,
    [int]$SteadySeconds = 180,
    [int]$SampleSeconds = 20
)

$ErrorActionPreference = 'Stop'

if (-not $Exe) {
    $Exe = Join-Path $PSScriptRoot '..\src-tauri\target\release\sill.exe'
}

if (-not (Test-Path $Exe)) {
    throw "No binary at $Exe. Build it first: cargo build --release --manifest-path src-tauri/Cargo.toml"
}

# A second Sill is turned away by the single-instance plugin and exits, so the
# measurement would silently attach to whatever was already running.
$already = Get-Process sill -ErrorAction SilentlyContinue
if ($already) {
    throw "Sill is already running (PID $($already.Id -join ', ')). Stop it first, or this measures that one."
}

# Every process the launcher is responsible for, found by walking the tree
# rather than by matching names: the WebView2 children belong to us, and the
# ones belonging to some other WebView2 application do not.
function Get-SillTree([int]$Root) {
    $candidates = Get-CimInstance Win32_Process |
        Where-Object { $_.Name -match '^(sill|msedgewebview2|node)\.exe$' }

    $tree = @()
    $frontier = @($Root)
    while ($frontier.Count -gt 0) {
        $next = @()
        foreach ($p in $candidates) {
            if ($frontier -contains $p.ParentProcessId) { $tree += $p; $next += $p.ProcessId }
        }
        $frontier = $next
    }
    return $tree + ($candidates | Where-Object { $_.ProcessId -eq $Root })
}

# What a process is, taken from Chromium's own --type switch rather than
# guessed. A renderer and the GPU process cost very different things and a
# reading that lumps them together cannot be acted on.
function Get-Role($Process) {
    $m = [regex]::Match($Process.CommandLine, '--type=([a-zA-Z-]+)')
    if ($m.Success) { return $m.Groups[1].Value }
    switch ($Process.Name) {
        'sill.exe' { 'rust core' }
        'node.exe' { 'extension host' }
        default    { 'browser (main)' }
    }
}

# CPU as a share of one core. Sampled as a delta over a window rather than
# read as an instantaneous figure, which on Windows is either meaningless or
# an average over the process lifetime.
function Measure-Cpu($Tree, [int]$Seconds) {
    $before = @{}
    foreach ($p in $Tree) {
        $proc = Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue
        if ($proc) { $before[$p.ProcessId] = $proc.TotalProcessorTime.TotalMilliseconds }
    }

    $started = Get-Date
    Start-Sleep -Seconds $Seconds
    $elapsed = ((Get-Date) - $started).TotalMilliseconds

    $rows = @()
    $total = 0.0
    foreach ($p in $Tree) {
        $proc = Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue
        if (-not $proc -or -not $before.ContainsKey($p.ProcessId)) { continue }
        $spent = $proc.TotalProcessorTime.TotalMilliseconds - $before[$p.ProcessId]
        $total += $spent
        if ($spent -gt 0) { $rows += [pscustomobject]@{ Role = (Get-Role $p); Ms = $spent } }
    }

    return [pscustomobject]@{
        Rows    = $rows
        Total   = $total
        Percent = 100 * $total / $elapsed
    }
}

function Write-Snapshot($Tree, [string]$When) {
    $private = ($Tree | Measure-Object -Property PrivatePageCount -Sum).Sum / 1MB
    $working = ($Tree | Measure-Object -Property WorkingSetSize -Sum).Sum / 1MB
    $renderers = @($Tree | Where-Object { $_.CommandLine -match '--type=renderer' }).Count
    $node = @($Tree | Where-Object { $_.Name -eq 'node.exe' }).Count

    Write-Output ''
    Write-Output "--- $Label, $When ---"
    foreach ($p in ($Tree | Sort-Object { $_.PrivatePageCount } -Descending)) {
        '{0,-16} {1,-9} private {2,7:N1} MB   working set {3,7:N1} MB' -f
            (Get-Role $p), $p.ProcessId, ($p.PrivatePageCount / 1MB), ($p.WorkingSetSize / 1MB)
    }
    '{0,-16} {1,-9} private {2,7:N1} MB   working set {3,7:N1} MB' -f 'TOTAL', "($($Tree.Count))", $private, $working
    "renderers {0}    extension host {1}" -f $renderers, $(if ($node) { 'running' } else { 'not running' })
}

Write-Output "Sill idle measurement [$Label]"
Write-Output "binary: $Exe"

Start-Process -FilePath $Exe | Out-Null
Start-Sleep -Seconds 5

$root = (Get-Process sill -ErrorAction SilentlyContinue | Select-Object -First 1).Id
if (-not $root) { throw 'sill.exe did not start, or exited immediately.' }
Write-Output "started as PID $root"

Start-Sleep -Seconds $SettleSeconds
$tree = Get-SillTree $root
Write-Snapshot $tree 'after startup'
$cpu = Measure-Cpu $tree $SampleSeconds
foreach ($row in $cpu.Rows) { '  {0,-16} {1,7:N1} ms' -f $row.Role, $row.Ms }
'idle CPU {0:N2}% of one core' -f $cpu.Percent

Start-Sleep -Seconds $SteadySeconds
$tree = Get-SillTree $root
Write-Snapshot $tree "steady state (+$([int](($SettleSeconds + $SteadySeconds) / 60)) min)"
$cpu = Measure-Cpu $tree $SampleSeconds
foreach ($row in $cpu.Rows) { '  {0,-16} {1,7:N1} ms' -f $row.Role, $row.Ms }
'idle CPU {0:N2}% of one core' -f $cpu.Percent

Write-Output ''
Write-Output "Leaving PID $root running. Stop it with: Stop-Process -Id $root"
