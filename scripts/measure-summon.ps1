<#
.SYNOPSIS
  Measures how long it takes to reach the launcher.

.DESCRIPTION
  Two numbers, and they are the two the audit refused to let anybody claim
  without measuring: how long from starting the process to the hotkey working,
  and how long from pressing the hotkey to being able to type.

  Both are recorded by the app itself, because only it can see both ends. Rust
  knows when the window was told to show; the page knows when it painted, and
  a window that is up and blank is not a launcher you can type into. This
  script presses the key, waits, and reads what the app wrote to its log.

  It starts a fresh copy rather than measuring one already running, because
  cold start can only be measured once per process and a launcher that has
  been open all day is not what a cold start feels like.

.PARAMETER Exe
  Which binary to run. Defaults to the release build.

.PARAMETER Times
  How many summons to measure. The median is what gets quoted, so an even
  handful is enough and each one costs a couple of seconds.

.PARAMETER BudgetMs
  What the median summon is allowed to cost. Exceeding it fails, which is what
  makes this a budget rather than a report.

.PARAMETER ColdBudgetMs
  What starting up is allowed to cost before the hotkey works.

.EXAMPLE
  pwsh -File scripts/measure-summon.ps1
  pwsh -File scripts/measure-summon.ps1 -Exe src-tauri/target/debug/sill.exe
#>
param(
    [string]$Exe = 'src-tauri/target/release/sill.exe',
    [int]$Times = 8,
    # Measured at around 30 ms on a warm machine, then given room. Generous on
    # purpose: a budget tight enough to fail on a busy machine is a budget
    # somebody switches off, and a switched-off budget catches nothing. This
    # is here to catch a change in kind, not a change of five milliseconds.
    [int]$BudgetMs = 250,
    [int]$ColdBudgetMs = 4000,
    # Writes both readings into docs/measurements/, which is where the
    # published cost page gets them.
    [switch]$Record
)

$ErrorActionPreference = 'Stop'

if ($Record) { . (Join-Path $PSScriptRoot 'record-measurement.ps1') }

Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices;
public class Key {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern void keybd_event(byte k, byte s, uint f, UIntPtr e);
  public static string Text(IntPtr h){ var b=new StringBuilder(256); GetWindowText(h,b,256); return b.ToString(); }
  public static void Chord(byte[] mods, byte key){
    foreach (var m in mods) keybd_event(m,0,0,UIntPtr.Zero);
    keybd_event(key,0,0,UIntPtr.Zero);
    keybd_event(key,0,2,UIntPtr.Zero);
    for (int i = mods.Length - 1; i >= 0; i--) keybd_event(mods[i],0,2,UIntPtr.Zero);
  }
}
"@

function Front { [Key]::Text([Key]::GetForegroundWindow()) }

<#
The summon key this machine actually uses.

Read from the preferences file rather than written down here. This script
pressed a hardcoded Alt+Space for as long as it has existed, and the summon
key on the machine it was written on is `Ctrl+Alt+F9`. So every press went
somewhere else, the launcher never came to the front, and the numbers it
printed were whatever the log already held from an earlier run. A benchmark
that cannot press the button it is timing reports the past.

Refuses a chord it cannot express rather than pressing part of one. Sending
the wrong keys is worse here than measuring nothing: they land in whatever is
in front, which is somebody's own window.
#>
function Get-SummonChord {
    $file = Join-Path $env:APPDATA 'app.winters.sill\preferences.json'
    if (-not (Test-Path $file)) { throw "no preferences at $file, so the summon key is unknown" }

    $chord = (Get-Content $file -Raw | ConvertFrom-Json).hotkey.summon
    if (-not $chord) { throw 'no summon key is set, so there is nothing to press' }

    $mods = @()
    $key = $null

    foreach ($part in $chord.Split('+')) {
        switch ($part.Trim().ToLower()) {
            'ctrl'    { $mods += [byte]0x11; continue }
            'control' { $mods += [byte]0x11; continue }
            'alt'     { $mods += [byte]0x12; continue }
            'shift'   { $mods += [byte]0x10; continue }
            'super'   { $mods += [byte]0x5B; continue }
            'win'     { $mods += [byte]0x5B; continue }
            'space'   { $key = [byte]0x20; continue }
            default {
                $one = $part.Trim()
                if ($one -match '^[A-Za-z]$') { $key = [byte][char]$one.ToUpper() }
                elseif ($one -match '^[0-9]$') { $key = [byte][char]$one }
                elseif ($one -match '^[Ff]([1-9]|1[0-9]|2[0-4])$') { $key = [byte](0x70 + [int]$Matches[1] - 1) }
                else { throw "this script cannot press '$chord': it does not know the key '$one'" }
            }
        }
    }

    if ($null -eq $key) { throw "this script cannot press '$chord': it names only modifiers" }
    @{ Chord = $chord; Mods = [byte[]]$mods; Key = $key }
}

$log = Join-Path $env:APPDATA 'app.winters.sill\sill.log'
$exe = (Resolve-Path $Exe).Path

if (-not (Test-Path $exe)) { throw "no binary at $Exe" }

# A fresh copy. Cold start can only be measured once per process, and one that
# has been open all day is not what starting up feels like.
Get-Process sill -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 800

# Everything already in the log is somebody else's run.
$before = if (Test-Path $log) { (Get-Item $log).Length } else { 0 }

Write-Host "starting $exe"
Start-Process $exe -WindowStyle Hidden
Start-Sleep -Seconds 10

$summon = Get-SummonChord
Write-Host "pressing $($summon.Chord), read from your preferences"

for ($i = 1; $i -le $Times; $i++) {
    [Key]::Chord($summon.Mods, $summon.Key)
    Start-Sleep -Milliseconds 1200

    # Stops rather than warns, and stops on the first one.
    #
    # If the launcher did not come to the front then the chord went to
    # whatever did, and pressing it another nineteen times types into
    # somebody's own window. One press is a bounded mistake; a loop is not.
    if ((Front) -ne 'Sill') {
        throw "press $i did not reach the launcher: '$($summon.Chord)' went to '$(Front)' instead. " +
              'Nothing was measured. Check the key is registered before running this again.'
    }

    # Away again, so the next press is a summon rather than a dismissal.
    [Key]::Chord($summon.Mods, $summon.Key)
    Start-Sleep -Milliseconds 900
}

Start-Sleep -Milliseconds 800

if (-not (Test-Path $log)) { throw "the app wrote no log at $log" }

$stream = [System.IO.File]::Open($log, 'Open', 'Read', 'ReadWrite')
$stream.Seek($before, 'Begin') | Out-Null
$reader = New-Object System.IO.StreamReader($stream)
$fresh = $reader.ReadToEnd()
$reader.Close(); $stream.Close()

$cold = [regex]::Match($fresh, 'ready in (\d+) ms')
$summons = [regex]::Matches($fresh, 'summon (\d+) ms \((\d+) to show, (\d+) to paint\)')

Write-Host ''
Write-Host '--- reaching the launcher ---'

if (-not $cold.Success) { throw 'the app never said it was ready' }
$coldMs = [int]$cold.Groups[1].Value
'{0,-22} {1,6} ms' -f 'cold start', $coldMs

if ($summons.Count -eq 0) { throw 'no summon was measured' }

$totals = @($summons | ForEach-Object { [int]$_.Groups[1].Value })
$shows  = @($summons | ForEach-Object { [int]$_.Groups[2].Value })
$paints = @($summons | ForEach-Object { [int]$_.Groups[3].Value })

$sorted = $totals | Sort-Object
$median = $sorted[[int][math]::Floor($sorted.Count / 2)]

'{0,-22} {1,6} ms   (of {2})' -f 'summon, median', $median, $summons.Count
'{0,-22} {1,6} ms' -f 'summon, best', $sorted[0]
'{0,-22} {1,6} ms' -f 'summon, worst', $sorted[-1]
'{0,-22} {1,6} ms' -f '  of which showing', (($shows | Measure-Object -Average).Average -as [int])
'{0,-22} {1,6} ms' -f '  of which painting', (($paints | Measure-Object -Average).Average -as [int])

Get-Process sill -ErrorAction SilentlyContinue | Stop-Process -Force

Write-Host ''
$summonOk = $median -le $BudgetMs
$coldOk = $coldMs -le $ColdBudgetMs

if (-not $summonOk) {
    Write-Host ("OVER BUDGET  median summon {0} ms, allowed {1}" -f $median, $BudgetMs) -ForegroundColor Red
}
if (-not $coldOk) {
    Write-Host ("OVER BUDGET  cold start {0} ms, allowed {1}" -f $coldMs, $ColdBudgetMs) -ForegroundColor Red
}

# Recorded whether or not it was within budget. A page that only wrote down
# the runs it liked would be the failure this whole readout exists against.
if ($Record) {
    $build = Get-MeasurementBuild $exe

    Write-Measurement -Id summon -Build $build -Within $summonOk `
        -By 'scripts/measure-summon.ps1' -Reading (
            '{0} ms median, {1} best, {2} worst, over {3} summons' -f
                $median, $sorted[0], $sorted[-1], $summons.Count)

    Write-Measurement -Id cold-start -Build $build -Within $coldOk `
        -By 'scripts/measure-summon.ps1' -Reading ('{0} ms' -f $coldMs)

    Write-Host ''
}

if (-not ($summonOk -and $coldOk)) { exit 1 }

Write-Host ("within budget: summon {0} ms of {1}, cold start {2} ms of {3}" -f
    $median, $BudgetMs, $coldMs, $ColdBudgetMs) -ForegroundColor Green
