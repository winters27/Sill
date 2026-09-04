<#
.SYNOPSIS
  Counts what the launcher says to the network while nobody is using it.

.DESCRIPTION
  The answer has to be nothing, and unlike every other row in
  `docs/budgets.md` that is a claim about behaviour rather than a number to
  stay under. There is no acceptable amount of traffic from a window that has
  been put away. Either it is quiet or the claim is false.

  It was false. A weather widget pinned to the chin asked a service for a
  reading every ten minutes for as long as the application was running, because
  `setInterval` in `onMount` runs until the component is destroyed and hiding a
  window destroys nothing. Six calls an hour on behalf of a window nobody could
  see. The fix was `pollWhileVisible`; this is what would have noticed.

  ## What counts as a call

  A connection to somewhere off this machine, opened by a process in the
  launcher's tree, that was not there when the watch began. Loopback is
  excluded deliberately and by name: the extension host, the dictation server
  and the MCP link all talk to Sill over localhost, they are the machine
  talking to itself, and counting them would make this fail for something that
  is not what it is about.

  ## Why the count and not a packet capture

  A capture needs a driver and administrator rights, and a measurement nobody
  can run is not a measurement. A remote endpoint appearing in the connection
  table is enough to answer the question being asked, which is whether Sill
  reaches out while it is idle. It can miss a request that opens, completes and
  closes entirely between two samples, so the sampling interval is short
  relative to any polling interval in the codebase, and the run says how long
  it watched for.

  ## One process, and it is always stopped

  It refuses to start if a Sill is already running, starts exactly one, and
  stops it in a `finally`, so a failure part way through does not leave a
  launcher sitting on somebody's desktop. That is a rule this work learned the
  hard way: an earlier measurement started the binary in a loop to toggle the
  window and left ten of them open on a machine somebody was working on.

  ## Run it with the widgets pinned

  The at-rest network figure in the budget table is specifically "widgets
  pinned", because an unpinned widget is not mounted and cannot poll. Measuring
  with nothing pinned would measure the case that was never in doubt. The
  script prints what is pinned so a run can be read for what it actually was.

.PARAMETER Exe
  Which binary to run. Defaults to the release build.

.PARAMETER Minutes
  How long to watch. The weather widget's own interval is ten minutes, so a
  watch shorter than that can miss it entirely and says so.

.PARAMETER EverySeconds
  How often to look at the connection table.

.EXAMPLE
  pwsh -File scripts/measure-network.ps1
  pwsh -File scripts/measure-network.ps1 -Minutes 25
#>
param(
    [string]$Exe = 'src-tauri/target/release/sill.exe',
    [int]$Minutes = 25,
    [int]$EverySeconds = 5,
    [int]$SettleSeconds = 45,
    # Writes the count into docs/measurements/, which is where the published
    # cost page gets it.
    [switch]$Record
)

$ErrorActionPreference = 'Stop'

if ($Record) { . (Join-Path $PSScriptRoot 'record-measurement.ps1') }

$exe = (Resolve-Path $Exe).Path

$already = Get-Process sill -ErrorAction SilentlyContinue
if ($already) {
    throw "Sill is already running (PID $($already.Id -join ', ')). Stop it first, or this measures that one."
}

function Get-Tree([int]$Root) {
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

# The machine talking to itself is not the machine talking to the network. The
# extension host, the dictation server and the MCP link are all loopback.
function Local([string]$address) {
    return $address -eq '127.0.0.1' -or
           $address -eq '::1' -or
           $address -eq '0.0.0.0' -or
           $address -eq '::' -or
           $address.StartsWith('127.')
}

# Every connection one of our processes has to somewhere off this machine.
function Remote {
    $found = @{}
    foreach ($c in (Get-NetTCPConnection -ErrorAction SilentlyContinue)) {
        if (-not $ours.ContainsKey([int]$c.OwningProcess)) { continue }
        if (Local $c.RemoteAddress) { continue }
        if ($c.State -eq 'Listen') { continue }
        $found["$($c.RemoteAddress):$($c.RemotePort)"] = $c.State
    }
    return $found
}

# Which widgets are pinned, because an unpinned widget cannot poll and a run
# with none pinned answers a question nobody asked.
$prefs = Join-Path $env:APPDATA 'app.winters.sill\preferences.json'
$pinned = 'unknown, no preferences file yet'
if (Test-Path $prefs) {
    try {
        $pinned = ((Get-Content $prefs -Raw | ConvertFrom-Json).widgets.pinned -join ', ')
        if (-not $pinned) { $pinned = 'none' }
    } catch { $pinned = "could not be read ($_)" }
}

Write-Host "Sill network at rest"
Write-Host "binary:  $exe"
Write-Host "pinned:  $pinned"
if ($Minutes -lt 11) {
    Write-Host "watching for $Minutes minutes, which is shorter than the widget interval this exists to catch" -ForegroundColor Yellow
}

$ours = @{}
$new = @{}
$samples = 0
$grew = 0

try {
    Start-Process $exe -WindowStyle Hidden
    Start-Sleep -Seconds 5

    $root = (Get-Process sill -ErrorAction SilentlyContinue | Select-Object -First 1).Id
    if (-not $root) { throw 'sill.exe did not start, or exited immediately.' }

    # Startup is allowed to reach the network: an extension catalogue and an
    # update check are things somebody asked for by running the program. What
    # is being measured is what happens afterwards, so the watch begins once it
    # has settled.
    Write-Host "settling for $SettleSeconds s, because starting up is allowed to reach out"
    Start-Sleep -Seconds $SettleSeconds

    foreach ($p in (Get-Tree $root)) { $ours[[int]$p.ProcessId] = $true }

    # Whatever is already open belongs to starting up, not to being at rest.
    $seen = Remote
    Write-Host ("watching for $Minutes min, sampling every $EverySeconds s ({0} connection(s) already open from startup)" -f $seen.Count)

    $until = (Get-Date).AddMinutes($Minutes)

    while ((Get-Date) -lt $until) {
        Start-Sleep -Seconds $EverySeconds
        $samples += 1

        foreach ($where in (Remote).Keys) {
            if ($seen.ContainsKey($where)) { continue }
            if ($new.ContainsKey($where)) { continue }

            $new[$where] = Get-Date
            Write-Host ("  reached out to $where") -ForegroundColor Yellow
        }
    }

    # What the tree looked like at each end. A renderer or a Node host appearing
    # while nobody is using the launcher is a different failure with the same
    # cause, and is worth seeing beside the connection count.
    #
    # Both counts rather than the difference between them. The difference was
    # printed first and came out as -8 on a run where the walk found nothing at
    # the end, which reads as eight processes having gone away and is not what
    # happened: the walk matches on parent ids and can miss a tree whose shape
    # moved under it. Two numbers cannot lie in that direction.
    $grew = @(Get-Tree $root).Count
} finally {
    # In a finally, and by name rather than by the id we started, because a
    # measurement that fails part way through must not leave a launcher sitting
    # on somebody's desktop. This is the rule that earlier work here broke.
    Get-Process sill -ErrorAction SilentlyContinue | Stop-Process -Force
}

Write-Host ''
Write-Host '--- network at rest ---'
'{0,-34} {1,6}' -f 'minutes watched', $Minutes
'{0,-34} {1,6}' -f 'samples taken', $samples
'{0,-34} {1,6}' -f 'processes when the watch began', $ours.Count
'{0,-34} {1,6}' -f 'processes when it ended', $grew
'{0,-34} {1,6}' -f 'new remote connections', $new.Count

foreach ($where in $new.Keys) { "    $where" }

Write-Host ''

# How long the watch ran belongs in the reading. Zero connections in two
# minutes and zero in twenty-five are not the same claim, and the widget this
# check exists for asked once every ten.
if ($Record) {
    Write-Measurement -Id network-at-rest -Build (Get-MeasurementBuild $exe) `
        -Within ($new.Count -eq 0) -By 'scripts/measure-network.ps1' -Reading (
            '{0} connection(s) in {1} minutes, over {2} samples' -f
                $new.Count, $Minutes, $samples)

    Write-Host ''
}

if ($new.Count -gt 0) {
    Write-Host ("OVER BUDGET  $($new.Count) connection(s) to somewhere off this machine while nobody was using the launcher, allowed 0") -ForegroundColor Red
    exit 1
}

Write-Host "silent: nothing left this machine in $Minutes minutes at rest" -ForegroundColor Green
