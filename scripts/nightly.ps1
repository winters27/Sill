<#
.SYNOPSIS
  The budgets a shared build agent cannot hold, run on a machine that can.

.DESCRIPTION
  `docs/budgets.md` is the contract and it has two halves. One half is held by
  `npm run verify` on every push, because it is counts and ratios and those
  mean the same thing on any machine. The other half is milliseconds and
  megabytes of a running application, and there is nowhere in a shared runner
  to take those honestly: it is a borrowed virtual machine with no display, no
  graphics hardware, neighbours competing for its cores and no idea what a
  frame is. A timing gate there fails builds for reasons nobody can act on,
  which teaches everybody to ignore red, which is worse than having no gate.

  So that half runs here, on one machine, once a night, and **it is not a
  required check**. Nothing it says can block a merge. What it produces is a
  dated file that can be read beside yesterday's, which is how a slow drift
  becomes visible at all.

  ## What it needs

  A release build. Every figure in the budget table was taken against one and
  none of them survives a debug build: the ranking budget is three times looser
  in debug and the pixel work is two orders of magnitude slower. Running this
  against a debug binary produces numbers that are not comparable to anything,
  so it refuses.

  It also needs the machine left alone, and it means that literally. Parts of
  this open the launcher and type into it, and a global hotkey cannot be aimed:
  it goes to whatever is in front. **Do not run this on a machine somebody is
  using.**

  Every part stops the launcher before it starts and the run stops it again at
  the end, so nothing is left on the desktop. That is a rule rather than
  tidiness: an earlier measurement here started the binary in a loop to toggle
  the window and left ten of them open on somebody's working machine.

  Each part says how long it takes and the whole run is the better part of an
  hour, most of which is deliberate waiting: a launcher's idle cost cannot be
  measured in a hurry.

.PARAMETER Exe
  Which binary to measure. Must be a release build.

.PARAMETER Out
  Where to write the report. Defaults to a dated file beside the repository.

.PARAMETER Quick
  Shortens the waits, for checking that the run works rather than for a
  reading. Every figure a quick run produces is marked as one.

.EXAMPLE
  pwsh -File scripts/nightly.ps1
  pwsh -File scripts/nightly.ps1 -Quick
#>
param(
    [string]$Exe = 'src-tauri/target/release/sill.exe',
    [string]$Out = '',
    [switch]$Quick
)

$ErrorActionPreference = 'Continue'

$root = Split-Path $PSScriptRoot -Parent
$exe = Join-Path $root $Exe

if (-not (Test-Path $exe)) {
    Write-Host "No binary at $exe" -ForegroundColor Red
    Write-Host "Build one first: npm run tauri build -- --no-bundle"
    exit 1
}

if ($exe -match '\\target\\debug\\') {
    # Not a warning. A debug figure quoted beside a release budget is worse
    # than no figure, because it looks like a measurement.
    Write-Host "That is a debug build. Every budget in docs/budgets.md is a release figure." -ForegroundColor Red
    exit 1
}

if (-not $Out) {
    $Out = Join-Path $root ("nightly-{0}.txt" -f (Get-Date -Format 'yyyy-MM-dd'))
}

$stamp = Get-Date -Format 'yyyy-MM-dd HH:mm'
$parts = @()

# Each part is a script that already knows its own budget and exits non-zero
# when it is over. Nothing here re-states a threshold: two places holding one
# number with nothing making them agree is how a budget quietly stops being
# the budget.
#
# `Binary` says whether the part is given the exe to run. The keystroke part is
# the odd one and deliberately so: it starts nothing and types nothing, it
# reads what the launcher recorded about itself while it was being used. Which
# is why it comes last, after the device suite, because the device suite is the
# part of this run that actually types into the launcher.
$plan = @(
    @{ Name = 'idle';      Script = 'measure-idle.ps1';      Binary = $true;  Args = @('-Label', 'nightly'); Record = @('-Record') },
    @{ Name = 'summon';    Script = 'measure-summon.ps1';    Binary = $true;  Args = @();                    Record = @('-Record') },
    @{ Name = 'network';   Script = 'measure-network.ps1';   Binary = $true;  Args = @();                    Record = @('-Record') },
    @{ Name = 'device';    Script = 'device-tests.ps1';      Binary = $false; Args = @();                    Record = @('-RecordCosts') },
    # `-Build release` is not an assumption here. This run refused a debug
    # binary two screens up, so the log these readings come from was written by
    # the release build it just measured everything else against.
    @{ Name = 'keystroke'; Script = 'measure-keystroke.ps1'; Binary = $false; Args = @('-Since', '30'); Record = @('-Record', '-Build', 'release') }
)

if ($Quick) {
    ($plan | Where-Object Name -eq 'idle').Args +=
        @('-SteadySeconds', '20', '-SettleSeconds', '10', '-SampleSeconds', '5')
    ($plan | Where-Object Name -eq 'network').Args += @('-Minutes', '2', '-SettleSeconds', '10')
}

Write-Host "Sill nightly, $stamp" -ForegroundColor Cyan
Write-Host "binary:  $exe"
Write-Host "report:  $Out"
if ($Quick) { Write-Host "QUICK: the waits are shortened, so nothing here is a reading" -ForegroundColor Yellow }
Write-Host ''

$report = @("Sill nightly $stamp", "binary $exe", "")
if ($Quick) { $report += 'QUICK RUN: waits shortened, these figures are not readings' }

foreach ($part in $plan) {
    Write-Host "--- $($part.Name) ---" -ForegroundColor Cyan

    $script = Join-Path $PSScriptRoot $part.Script
    # Sill is left running by some of these and refused by others, so each one
    # starts from nothing running.
    Get-Process sill -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 2

    $args = @($part.Args)
    if ($part.Binary) { $args = @('-Exe', $exe) + $args }

    # A quick run records nothing, deliberately. Its waits are shortened, so
    # its figures are not readings, and writing one into the published page
    # would put a number there that nothing stands behind.
    if (-not $Quick) { $args += $part.Record }

    $said = & powershell -NoProfile -ExecutionPolicy Bypass -File $script @args 2>&1
    $code = $LASTEXITCODE

    $said | ForEach-Object { Write-Host "  $_" }

    $report += ''
    $report += "=== $($part.Name) === exit $code"
    $report += ($said | ForEach-Object { "$_" })

    $parts += [pscustomobject]@{ Name = $part.Name; Code = $code }
}

Get-Process sill -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
$left = @(Get-Process sill -ErrorAction SilentlyContinue).Count
if ($left -gt 0) { Write-Host "$left launcher(s) still running after the run" -ForegroundColor Red }

$report | Set-Content $Out -Encoding UTF8

# The published page, rebuilt from what this run just wrote down. Nothing is
# sent anywhere: it writes docs/benchmark.md into the checkout, and whoever
# reads the report above decides whether to commit it.
if (-not $Quick) {
    Write-Host ''
    Write-Host '--- the published page ---' -ForegroundColor Cyan
    & node (Join-Path $PSScriptRoot 'benchmark-page.mjs')
}

Write-Host ''
Write-Host '--- nightly ---' -ForegroundColor Cyan
foreach ($part in $parts) {
    $mark = if ($part.Code -eq 0) { 'within budget' } else { "OVER (exit $($part.Code))" }
    $colour = if ($part.Code -eq 0) { 'Green' } else { 'Red' }
    Write-Host ("  {0,-12} {1}" -f $part.Name, $mark) -ForegroundColor $colour
}
Write-Host "written to $Out"

# Exits non-zero so a scheduled task can be seen to have failed, which is the
# only thing that makes a nightly worth scheduling. Nothing gates a merge on
# it, deliberately: see the note at the top.
exit @($parts | Where-Object { $_.Code -ne 0 }).Count
