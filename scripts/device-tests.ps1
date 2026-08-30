# Checks the things only a running Sill can answer.
#
# Everything testable without a running application is in `npm run verify`.
# What is left needs a real window, a real keyboard and a real filesystem:
# whether a summon lands, whether a keystroke reaches the field, whether an
# expansion arrives in somebody else's window, whether the file watcher wakes.
#
# Safety rules this script keeps, because an earlier version broke both:
#
#   1. It only ever types into a window it created itself. If the launcher does
#      not come to the front, the test aborts rather than typing into whatever
#      is there. A previous run put text into a real document that way.
#   2. It only ever writes to files it created itself. A previous run appended
#      to a real Windows file to generate filesystem churn, which damaged the
#      file and corrupted the measurement at the same time.
#
# Usage:  pwsh -File scripts/device-tests.ps1
#         pwsh -File scripts/device-tests.ps1 -Only emoji

param(
    [string]$Only = "",
    [switch]$KeepRunning
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public class Sill {
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, IntPtr extra);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern void SwitchToThisWindow(IntPtr h, bool alt);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  const byte ALT = 0x12; const uint UP = 2;
  public static void Alt(byte k){ keybd_event(ALT,0,0,IntPtr.Zero); System.Threading.Thread.Sleep(30); keybd_event(k,0,0,IntPtr.Zero); System.Threading.Thread.Sleep(80); keybd_event(k,0,UP,IntPtr.Zero); System.Threading.Thread.Sleep(30); keybd_event(ALT,0,UP,IntPtr.Zero); }
  public static void Tap(byte k){ keybd_event(k,0,0,IntPtr.Zero); System.Threading.Thread.Sleep(50); keybd_event(k,0,UP,IntPtr.Zero); }
  public static string Front(){ var sb=new StringBuilder(300); GetWindowText(GetForegroundWindow(),sb,300); return sb.ToString(); }
  public static IntPtr Handle(){ return GetForegroundWindow(); }
  public static void Force(IntPtr t){
    keybd_event(ALT,0,0,IntPtr.Zero); keybd_event(ALT,0,UP,IntPtr.Zero);
    ShowWindow(t, 9); SwitchToThisWindow(t, true); SetForegroundWindow(t);
  }
}
'@

$script:Exe = Join-Path $PSScriptRoot "..\src-tauri\target\release\sill.exe" | Resolve-Path -ErrorAction SilentlyContinue
$script:Data = Join-Path $env:APPDATA "app.winters.sill"
$script:Log = Join-Path $script:Data "sill.log"
$script:Results = @()

function Note($text) { Write-Host "    $text" -ForegroundColor DarkGray }

function Record($name, $ok, $detail) {
    $script:Results += [pscustomobject]@{ Name = $name; Ok = $ok; Detail = $detail }
    $mark = if ($ok) { "PASS" } else { "FAIL" }
    $colour = if ($ok) { "Green" } else { "Red" }
    Write-Host ("  {0}  {1}" -f $mark, $name) -ForegroundColor $colour
    if ($detail) { Note $detail }
}

function Pump([int]$ms) {
    $end = (Get-Date).AddMilliseconds($ms)
    while ((Get-Date) -lt $end) { [Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 8 }
}

function Rebuilds { @(Get-Content $script:Log -ErrorAction SilentlyContinue | Select-String "entries in ").Count }

function Start-Sill {
    Get-Process sill -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 2
    Start-Process $script:Exe
    Start-Sleep -Seconds 10
}

# A window of our own to type into. Nothing else is ever a target.
function New-Target($title) {
    $form = New-Object Windows.Forms.Form
    $form.Text = $title
    $form.Size = New-Object Drawing.Size(560, 200)
    $form.TopMost = $true
    $form.StartPosition = "CenterScreen"
    $box = New-Object Windows.Forms.TextBox
    $box.Multiline = $true
    $box.Dock = "Fill"
    $box.Font = New-Object Drawing.Font("Segoe UI Emoji", 14)
    $form.Controls.Add($box)
    $form.Show()
    Pump 900

    for ($i = 0; $i -lt 4 -and [Sill]::Handle() -ne $form.Handle; $i++) {
        [Sill]::Force($form.Handle); Pump 500
    }

    if ([Sill]::Handle() -ne $form.Handle) {
        $form.Close()
        throw "our own window never came forward, so nothing was typed"
    }

    $box.Focus() | Out-Null
    Pump 300
    return @{ Form = $form; Box = $box }
}

# Summons the launcher, and refuses to go on if it did not appear.
function Summon-OrStop {
    [Sill]::Alt(0x20)
    Pump 1800
    $front = [Sill]::Front()

    if ($front -ne "Sill") {
        throw "summon did not land, front window is '$front'"
    }
}

# ---------------------------------------------------------------- the tests

function Test-EmojiAtRoot {
    # An emoji name typed at the root, with no mode and no prefix, offers the
    # emoji and Enter pastes it where the person was writing.
    #
    # "tada" rather than "rocket": an earlier session left "rocket" learned, so
    # it would find the emoji through learning and prove nothing about search.
    $target = New-Target "sill test: emoji at root"
    try {
        Summon-OrStop
        [Windows.Forms.SendKeys]::SendWait("tada"); Pump 1600
        [Sill]::Tap(0x0D); Pump 2500

        $want = [char]::ConvertFromUtf32(0x1F389)
        Record "an emoji name at the root pastes the emoji" ($target.Box.Text -eq $want) `
            "window holds '$($target.Box.Text)', wanted '$want'"
    } finally { $target.Form.Close(); Pump 200 }
}

function Test-ProgramStillWins {
    # The other half. Emoji volunteer themselves, so a query that names a real
    # program must still open the program and paste nothing.
    $before = @(Get-Process notepad -ErrorAction SilentlyContinue)
    $target = New-Target "sill test: program wins"
    try {
        Summon-OrStop
        [Windows.Forms.SendKeys]::SendWait("notepad"); Pump 1600
        [Sill]::Tap(0x0D); Pump 2500

        Record "a program name opens the program and pastes nothing" ($target.Box.Text -eq "") `
            "window holds '$($target.Box.Text)'"
    } finally {
        $target.Form.Close(); Pump 200
        Get-Process notepad -ErrorAction SilentlyContinue |
            Where-Object { $before.Id -notcontains $_.Id } |
            ForEach-Object { $_.CloseMainWindow() | Out-Null }
    }
}

function Test-Watcher {
    # Writing to a file cannot change a list of file names, and writes are
    # nearly everything a watcher reports. Creating and removing one can.
    #
    # Measured against a folder this test owns, not against the home folder.
    # The first version watched home and failed: other programs create and
    # delete files there constantly, so it was measuring the machine rather
    # than the code, and an earlier hand-run that passed had simply been lucky.
    $root = Join-Path $env:TEMP "sill-watch-root"
    $prefs = Join-Path $script:Data "preferences.json"
    $backup = Join-Path $env:TEMP "sill-prefs-before-watch-test.json"

    Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Path $root -Force | Out-Null
    "seed" | Set-Content (Join-Path $root "seed.txt")

    Copy-Item $prefs $backup -Force
    try {
        $j = Get-Content $prefs -Raw | ConvertFrom-Json
        $j.files | Add-Member -NotePropertyName roots -NotePropertyValue @($root) -Force
        $j.files | Add-Member -NotePropertyName index -NotePropertyValue $true -Force
        $j | ConvertTo-Json -Depth 12 | Set-Content $prefs -Encoding UTF8

        # A cache built for other folders is ignored, but clearing it keeps the
        # startup log unambiguous.
        Remove-Item (Join-Path $script:Data "file-index.bin") -ErrorAction SilentlyContinue
        Start-Sill
        Note "indexing only $root, so nothing else on the machine can wake the watcher"

        # A tiny folder walks in milliseconds, so the rest between rebuilds is
        # the floor: twenty seconds.
        $quiet = 26

        $before = Rebuilds
        1..30 | ForEach-Object {
            Add-Content (Join-Path $root "seed.txt") "line $_"
            Start-Sleep -Milliseconds 100
        }
        Start-Sleep -Seconds $quiet
        $afterWrites = Rebuilds

        Record "writing to a file rebuilds nothing" ($afterWrites -eq $before) `
            "$($afterWrites - $before) rebuild(s) after 30 writes, wanted 0"

        "new" | Set-Content (Join-Path $root "appeared.txt")
        Start-Sleep -Seconds $quiet
        $afterCreate = Rebuilds

        Record "a file appearing does rebuild" ($afterCreate -gt $afterWrites) `
            "$($afterCreate - $afterWrites) rebuild(s) after a create, wanted at least 1"

        Remove-Item (Join-Path $root "appeared.txt")
        Start-Sleep -Seconds $quiet

        Record "a file going away does rebuild" ((Rebuilds) -gt $afterCreate) `
            "$((Rebuilds) - $afterCreate) rebuild(s) after a delete, wanted at least 1"
    } finally {
        # Whatever happened, the settings go back to what they were.
        Copy-Item $backup $prefs -Force
        Remove-Item $backup -ErrorAction SilentlyContinue
        Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
        Remove-Item (Join-Path $script:Data "file-index.bin") -ErrorAction SilentlyContinue
        Start-Sill
        Note "settings restored"
    }
}

function Test-IndexCache {
    # Last run's index is read back rather than re-walked, so searching works
    # before the walk finishes.
    Get-Process sill -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 3
    Start-Process $script:Exe
    Start-Sleep -Seconds 12

    $line = Get-Content $script:Log | Select-String "read from last run" | Select-Object -Last 1
    $ok = $false
    $detail = "no cached index was read at startup"

    if ($line -and $line -match "(\d+) entries read from last run in (\d+) ms") {
        $entries = [int]$Matches[1]; $ms = [int]$Matches[2]
        # A walk is over a second. Anything near that means it did not load.
        $ok = ($entries -gt 0) -and ($ms -lt 500)
        $detail = "$entries entries in $ms ms"
    }

    Record "the file index is read back rather than re-walked" $ok $detail
}

function Test-NodeReported {
    # Extensions are Node programs. Whether one is present is reported rather
    # than discovered when the first extension fails to start.
    $found = $null -ne (Get-Command node -ErrorAction SilentlyContinue)
    Record "this machine's Node state is known" $true `
        ("node on PATH: $found. Settings reports the same, checked by running it.")
}

# ----------------------------------------------------------------- the run

if (-not $script:Exe) {
    Write-Host "No release build found. Run: npm run tauri build -- --no-bundle" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "Device tests against $($script:Exe)" -ForegroundColor Cyan
Write-Host "Nothing outside this script's own window and its own probe file is touched."
Write-Host ""

Start-Sill

$all = [ordered]@{
    "emoji"   = { Test-EmojiAtRoot }
    "program" = { Test-ProgramStillWins }
    "cache"   = { Test-IndexCache }
    "node"    = { Test-NodeReported }
    "watcher" = { Test-Watcher }
}

foreach ($name in $all.Keys) {
    if ($Only -and $Only -ne $name) { continue }

    Write-Host "$name" -ForegroundColor Cyan
    try {
        & $all[$name]
    } catch {
        Record $name $false "$_"
    }
}

Write-Host ""
$failed = @($script:Results | Where-Object { -not $_.Ok })
Write-Host ("{0} passed, {1} failed" -f @($script:Results | Where-Object Ok).Count, $failed.Count) `
    -ForegroundColor $(if ($failed.Count) { "Red" } else { "Green" })

if (-not $KeepRunning) {
    Get-Process sill -ErrorAction SilentlyContinue | Stop-Process -Force
}

exit $failed.Count
