<#
Takes the README's screenshots from a running Sill, over a generated backdrop.

    pwsh -File scripts/shoot.ps1              # every shot
    pwsh -File scripts/shoot.ps1 -Only hero   # one of them

Requires `docs/media/raw/backdrop.png`, which `python scripts/shoot-compose.py
backdrop` writes, and a built `src-tauri/target/debug/sill.exe` (or pass -Exe).
The primary display must be at 100 percent scale, or the captured pixels are
not the pixels on screen.

Refuses to run while a Sill it did not start is running: it types into the
launcher and reads the screen around it, and both belong to whoever started
the program. Starting `sill.exe` a second time while one runs asks the running
one to show its window, which is how each shot is summoned without a hotkey.
`SILL_NO_AUTOHIDE` keeps the window up while the capture reads the screen.

Raw captures land in `docs/media/raw/` and are meant to be looked at before
they are committed. The shoot runs against a real install.
#>
param(
  [string]$Exe = (Join-Path $PSScriptRoot "..\src-tauri\target\debug\sill.exe"),
  [string]$Out = (Join-Path $PSScriptRoot "..\docs\media\raw"),
  [string[]]$Only = @(),
  # Whose remembered permissions to set aside for the run, and put back after.
  # An extension that has already been allowed never asks again, so the shot of
  # it asking can only be taken from a machine where it has not been.
  [string]$Forget = "hacker-news",
  [switch]$KeepRunning
)

$ErrorActionPreference = "Stop"
# `-Only a,b` arrives as one string when the script is run with -File.
$Only = @($Only | ForEach-Object { $_ -split "," } | Where-Object { $_ })
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Speech

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class Shoot {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  public delegate bool EnumProc(IntPtr hwnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int count);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd, int cmd);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
}
"@

[void][Shoot]::SetProcessDPIAware()

# The launcher opens on the display the pointer is on, and every capture reads
# the primary display. A pointer left on the second monitor puts the window
# somewhere this never photographs, and the only sign is a warning per shot.
$primary = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
[System.Windows.Forms.Cursor]::Position =
  New-Object System.Drawing.Point(($primary.Left + $primary.Width / 2), ($primary.Top + $primary.Height / 2))

$Exe = (Resolve-Path $Exe).Path
$Out = (Resolve-Path $Out).Path
$backdropPng = Join-Path $Out "backdrop.png"
if (-not (Test-Path $backdropPng)) { throw "No backdrop at $backdropPng. Run: python scripts/shoot-compose.py backdrop" }

$already = Get-Process sill -ErrorAction SilentlyContinue
if ($already) { throw "Sill is already running (pid $($already.Id -join ', ')). Quit it first; this script only drives a Sill it started." }

# The desktop for the duration: one borderless window full of backdrop, in
# its own process so it has a message loop of its own. Topmost, so a debug
# build's console stays behind it; each window is raised above it in turn.
$backdropScript = @"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
`$f = New-Object System.Windows.Forms.Form
`$f.FormBorderStyle = 'None'
`$f.StartPosition = 'Manual'
`$f.Bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
`$f.BackgroundImage = [System.Drawing.Image]::FromFile('$backdropPng')
`$f.BackgroundImageLayout = 'Stretch'
`$f.Text = 'Sill shoot backdrop'
`$f.ShowInTaskbar = `$false
`$f.TopMost = `$true
[System.Windows.Forms.Application]::Run(`$f)
"@
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($backdropScript))
$backdrop = Start-Process pwsh -ArgumentList "-NoProfile", "-WindowStyle", "Hidden", "-EncodedCommand", $encoded -PassThru
Start-Sleep -Milliseconds 1500

# Set aside before Sill starts, because the grants are read at load.
$grantsPath = Join-Path $env:APPDATA "app.winters.sill" | Join-Path -ChildPath "extension-grants.json"
$grantsKept = $null
if ($Forget -and (Test-Path $grantsPath)) {
  $grantsKept = Get-Content $grantsPath -Raw
  $grants = $grantsKept | ConvertFrom-Json
  if ($grants.items.PSObject.Properties.Name -contains $Forget) {
    $grants.items.PSObject.Properties.Remove($Forget)
    $grants | ConvertTo-Json -Depth 8 | Set-Content $grantsPath -Encoding utf8
    Write-Host "set aside what $Forget had been allowed; it will be put back"
  } else {
    $grantsKept = $null
  }
}

$env:SILL_NO_AUTOHIDE = "1"
# A build run from the repository rather than installed has no bundled host;
# point it at the one `npm run host:build` wrote so extensions can run.
$hostJs = Join-Path $PSScriptRoot "..\host\dist\host.js"
if (Test-Path $hostJs) { $env:SILL_HOST_JS = (Resolve-Path $hostJs).Path }
$sill = Start-Process $Exe -PassThru
Write-Host "started sill.exe as pid $($sill.Id)"

# A second launch that arrives before the first has finished starting is
# dropped, so wait for Sill to say it is ready, then a little more for the
# index it reads from last run.
$log = Join-Path $env:APPDATA "app.winters.sill" | Join-Path -ChildPath "sill.log"
$started = [DateTime]::Now
$ready = $false
for ($i = 0; $i -lt 100 -and -not $ready; $i++) {
  Start-Sleep -Milliseconds 200
  if ((Test-Path $log) -and (Get-Item $log).LastWriteTime -gt $started) {
    $ready = (Get-Content $log -Tail 30) -match "ready in "
  }
}
if (-not $ready) { throw "Sill did not report ready" }
Start-Sleep -Milliseconds 2500

function Find-SillWindow([string]$Title, [int]$WaitMs = 8000) {
  $deadline = [DateTime]::Now.AddMilliseconds($WaitMs)
  do {
    $script:found = [IntPtr]::Zero
    $cb = [Shoot+EnumProc]{
      param($hwnd, $lp)
      if (-not [Shoot]::IsWindowVisible($hwnd)) { return $true }
      $owner = 0
      [void][Shoot]::GetWindowThreadProcessId($hwnd, [ref]$owner)
      if ($owner -ne $script:sill.Id) { return $true }
      $sb = New-Object System.Text.StringBuilder 256
      [void][Shoot]::GetWindowText($hwnd, $sb, 256)
      if ($sb.ToString() -eq $Title) { $script:found = $hwnd; return $false }
      return $true
    }
    [void][Shoot]::EnumWindows($cb, [IntPtr]::Zero)
    if ($script:found -ne [IntPtr]::Zero) { return $script:found }
    Start-Sleep -Milliseconds 150
  } while ([DateTime]::Now -lt $deadline)
  return [IntPtr]::Zero
}

function Summon {
  # Every window this reads is matched by the pid this script started, so a
  # Sill that went away takes the rest of the run with it and says so once
  # rather than warning nine times. It went away once because somebody
  # restarted their own copy while the shoot was running, which looks from
  # here exactly like the launcher refusing to open.
  if ($sill.HasExited) {
    throw "the Sill this started (pid $($sill.Id)) is gone; nothing after this can be captured"
  }

  # A second launch asks the running Sill to show the launcher. It toggles,
  # so a launcher that was somehow still up goes away instead; check, and ask
  # once more if so.
  for ($try = 0; $try -lt 2; $try++) {
    Start-Process $Exe -Wait -WindowStyle Hidden
    Start-Sleep -Milliseconds 700
    if ((Find-SillWindow "Sill" 1500) -ne [IntPtr]::Zero) { break }
  }
  Raise "Sill"
}

function CloseWindow([string]$Title) {
  # Alt+F4 on the named window, and a check that it went.
  Raise $Title
  Key "%{F4}"
  Wait 600
  if ((Find-SillWindow $Title 500) -ne [IntPtr]::Zero) { Write-Warning "$Title did not close" }
}

function Raise([string]$Title) {
  # Above the backdrop and holding the keyboard, whatever was in front before.
  $hwnd = Find-SillWindow $Title
  if ($hwnd -eq [IntPtr]::Zero) { return }
  [void][Shoot]::SetWindowPos($hwnd, [IntPtr](-1), 0, 0, 0, 0, 0x0013)
  [void][Shoot]::SetForegroundWindow($hwnd)
  Start-Sleep -Milliseconds 200
}

function TypeText([string]$Text) {
  # SendKeys gives a few characters a meaning of their own; braces take it away.
  $escaped = ($Text.ToCharArray() | ForEach-Object { if ('+^%~(){}[]'.Contains($_)) { "{$_}" } else { "$_" } }) -join ''
  [System.Windows.Forms.SendKeys]::SendWait($escaped)
}

function Key([string]$Keys) { [System.Windows.Forms.SendKeys]::SendWait($Keys) }
function Wait([int]$Ms) { Start-Sleep -Milliseconds $Ms }

function Dismiss {
  # Escape clears the field, then Escape hides the window. A third if the
  # launcher is somehow still up, so the next summon shows rather than hides.
  Raise "Sill"
  Key "{ESC}"; Wait 250; Key "{ESC}"; Wait 400
  if ((Find-SillWindow "Sill" 300) -ne [IntPtr]::Zero) { Key "{ESC}"; Wait 400 }
}

function Capture([string]$Name, [string]$Title = "Sill", [int]$Width = 1400, [int]$Height = 800) {
  Raise $Title
  $hwnd = Find-SillWindow $Title
  if ($hwnd -eq [IntPtr]::Zero) { Write-Warning "no visible window titled '$Title' for $Name"; return }
  $r = New-Object Shoot+RECT
  [void][Shoot]::GetWindowRect($hwnd, [ref]$r)
  $ww = $r.Right - $r.Left; $wh = $r.Bottom - $r.Top
  $primary = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  if (-not $primary.Contains($r.Left, $r.Top)) { Write-Warning "$Title is at $($r.Left),$($r.Top), off the primary display; the launcher follows the cursor, so move the mouse there" }
  # Grown around the window so the backdrop frames it; never smaller than it.
  $Width = [Math]::Max($Width, $ww + 160); $Height = [Math]::Max($Height, $wh + 160)
  $screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $x = [Math]::Max($screen.Left, [Math]::Min($r.Left + ($ww - $Width) / 2, $screen.Right - $Width))
  $y = [Math]::Max($screen.Top,  [Math]::Min($r.Top  + ($wh - $Height) / 2, $screen.Bottom - $Height))
  $bmp = New-Object System.Drawing.Bitmap $Width, $Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen([int]$x, [int]$y, 0, 0, $bmp.Size)
  $g.Dispose()
  $path = Join-Path $Out "$Name.png"
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
  Write-Host "captured $Name  ($ww x $wh window at $($r.Left),$($r.Top); $Width x $Height picture)"
}

function Hold([byte]$Vk) { [Shoot]::keybd_event($Vk, 0, 0, [UIntPtr]::Zero) }
function Release([byte]$Vk) { [Shoot]::keybd_event($Vk, 0, 2, [UIntPtr]::Zero) }

function Seed-Clipboard {
  foreach ($line in @(
    "A sill is the ledge at the bottom of a window.",
    "The launcher sits on the ledge of every window you open.",
    "Ledge, sill, shelf: three words for the same idea."
  )) { Set-Clipboard -Value $line; Wait 600 }
}

$shots = [ordered]@{
  root = { Summon; Wait 900; Capture root; Dismiss }
  hero = { Summon; TypeText "ter"; Wait 900; Capture hero; Dismiss }
  calculator = { Summon; TypeText "100 km to miles"; Wait 900; Capture calculator; Dismiss }
  switches = { Summon; TypeText "toggle"; Wait 1200; Capture switches; Dismiss }
  volume = { Summon; TypeText "volume"; Wait 1200; Capture volume; Dismiss }
  files = { Summon; TypeText "budgets ext:md"; Wait 1500; Capture files; Dismiss }
  clipboard = { Seed-Clipboard; Summon; TypeText "clipboard history"; Wait 1400; Key "{ENTER}"; Wait 900; TypeText "ledge"; Wait 900; Capture clipboard; Dismiss; Key "{ESC}" }
  # Tab is read by the field, so it has to arrive after the field has settled:
  # at 400ms it was swallowed and the shot was of an ordinary search.
  ask = { Summon; TypeText "What is a window sill for?"; Wait 1600; Key "{TAB}"; Wait 14000; Capture ask; Dismiss }
  chat = { Summon; TypeText "ai chat"; Wait 1400; Key "{ENTER}"; Wait 4000; Raise "AI Chat"; TypeText "In two sentences, what does a launcher do?"; Wait 300; Key "{ENTER}"; Wait 15000; Capture chat "AI Chat" 1500 960; CloseWindow "AI Chat" }
  store = { Summon; TypeText "extension store"; Wait 1400; Key "{ENTER}"; Wait 12000; TypeText "hacker news"; Wait 6000; Capture store; Dismiss; Key "{ESC}"; Wait 500 }
  # Two shots, and they cannot come from one run.
  #
  # An extension asks for each thing as it reaches for it, so Hacker News puts
  # up a card for the network and then another for files. Answering them by
  # pressing Enter on a timer does not work: a press that lands before the next
  # card is up goes to the launcher and runs whatever row is selected, which is
  # how a run of this ended up photographing the Quick AI answer with the
  # extension refused in the chin.
  #
  # So each shot gets the machine it needs. `permission` runs with the grant set
  # aside, which is what makes the card appear at all; `extension` runs with the
  # grant in place, so the list draws with nothing in the way. Two invocations:
  #
  #   pwsh -File scripts/shoot.ps1 -Only permission
  #   pwsh -File scripts/shoot.ps1 -Only extension -Forget ""
  permission = {
    Summon; TypeText "hacker news"; Wait 1400; Key "{ENTER}"; Wait 7000
    Capture permission
    Key "{ESC}"; Wait 600; Dismiss; Key "{ESC}"
  }
  extension = {
    # It fetches its feed over the network when its cache has expired, and the
    # first render landed at 14 seconds once, which photographed the empty
    # state a moment before the stories arrived.
    Summon; TypeText "hacker news"; Wait 1400; Key "{ENTER}"; Wait 26000
    Capture extension
    Dismiss; Key "{ESC}"
  }
  themes = { Summon; TypeText "theme"; Wait 1400; Key "{ENTER}"; Wait 5000; Capture themes "Settings" 1500 1000; CloseWindow "Settings" }
  ai_settings = { Summon; TypeText "who answers"; Wait 1400; Key "{ENTER}"; Wait 5000; Capture ai-settings "Settings" 1500 1000; CloseWindow "Settings" }
  dictation = {
    # Hold the trigger, say something through the speakers so a microphone in
    # the room has a waveform to draw, and capture mid-sentence. Before the
    # settings shot, so the trigger row there has seen its key.
    Hold 0x12; Hold 0x48; Wait 900
    $job = Start-Job { Add-Type -AssemblyName System.Speech; (New-Object System.Speech.Synthesis.SpeechSynthesizer).Speak("Dictation turns speech into text on this machine, and the model never leaves it.") }
    Wait 2500; Capture dictation "Dictation" 720 320
    Receive-Job $job -Wait | Out-Null; Release 0x48; Release 0x12; Wait 1500
  }
  dictation_settings = { Summon; TypeText "dictation"; Wait 1400; Key "{ENTER}"; Wait 5000; Capture dictation-settings "Settings" 1500 1000; Key "{PGDN}"; Wait 600; Key "{PGDN}"; Wait 600; Capture dictation-engine "Settings" 1500 1000; CloseWindow "Settings" }
}

try {
  foreach ($name in $shots.Keys) {
    if ($Only.Count -gt 0 -and $Only -notcontains $name) { continue }
    Write-Host "--- $name ---"
    & $shots[$name]
  }
} finally {
  if (-not $KeepRunning) {
    Stop-Process -Id $sill.Id -ErrorAction SilentlyContinue
    Write-Host "stopped sill.exe"
  }
  Stop-Process -Id $backdrop.Id -ErrorAction SilentlyContinue
  Remove-Item Env:SILL_NO_AUTOHIDE -ErrorAction SilentlyContinue
  if ($grantsKept) {
    # After Sill has stopped, or it writes its own copy over this on the way out.
    Start-Sleep -Milliseconds 800
    Set-Content $grantsPath $grantsKept -Encoding utf8 -NoNewline
    Write-Host "put back what $Forget had been allowed"
  }
}
