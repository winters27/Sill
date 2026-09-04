<#
.SYNOPSIS
  Reads what keystrokes have cost, and holds them to a budget.

.DESCRIPTION
  The number this product lives or dies by. Everything ever claimed about how
  fast Sill answers a letter was the ranking cost, which is measured in a test
  and is three milliseconds. Ranking in three and drawing in ninety is a
  launcher that feels slow, and until now nothing measured the ninety.

  ## This script starts nothing and types nothing

  That is a rule rather than a limitation. An earlier version of this work drove
  the launcher: it started the binary to summon the window and sent synthetic
  keys into it. On a machine somebody is using, both are dangerous. A global
  hotkey cannot be aimed, and the one this pressed landed in a browser and
  changed the tab somebody was reading. Starting the binary again to toggle the
  window leaves a process on the desktop every time the toggle does not land.
  Neither is acceptable on a working machine, and a measurement that needs them
  is a measurement for a machine set aside for it.

  So the measuring happens where it belongs: inside the launcher, while
  somebody is genuinely using it. The window times every keystroke it answers
  and hands the readings to Rust when it is put away, which writes them to the
  log. This reads that log. It is also the better sample: real queries, a real
  index, a real display, and no synthetic timing of a synthetic word.

  Use Sill for a minute, put it away, then run this.

  ## Where the clock starts and where it stops

  It starts when the field says its value changed, and stops in two places,
  both of which are reported:

    answered   the rows for that keystroke are in the document and the frame
               that draws them has begun. This is the number the budget is
               about: it is the part Sill does and the part that has to fit
               inside a frame.

    presented  the frame after that one has begun, which is the first moment
               the pixels are certainly out. Longer by however long the display
               takes to refresh, which is the monitor waiting rather than Sill
               working.

  Both, because either alone misleads. Reporting only `answered` would be a
  keystroke-to-paint number with the paint left out. Reporting only `presented`
  charges Sill for the display's refresh rate and makes an instant answer look
  like a sixteen millisecond one.

  ## What it excludes, and cannot include

  Everything before the field heard about the key: the keyboard, its driver,
  Windows, and WebView2's own input plumbing. Nothing inside the page can see
  any of that, so no number here should be read as if it could.

  It also excludes the keystrokes that were superseded before they were drawn.
  Typing faster than the launcher answers is the ranker working as designed:
  the query for one letter is abandoned when the next arrives and never reaches
  a screen, so there is nothing to time. That is why a run says how many
  keystrokes it measured, and why a small count means somebody typed quickly
  rather than that something is broken.

.PARAMETER Since
  How far back in the log to look, in minutes. Older readings belong to a build
  that may not be this one.

.PARAMETER BudgetMs
  What the median answered reading is allowed to be.

.EXAMPLE
  pwsh -File scripts/measure-keystroke.ps1
  pwsh -File scripts/measure-keystroke.ps1 -Since 5 -BudgetMs 0
#>
param(
    [int]$Since = 60,
    # One frame at sixty hertz. Not a number picked to sit above what was
    # measured: it is the deadline the work has, because a keystroke whose
    # answer misses the frame it was typed in is a keystroke somebody sees the
    # old list through. A budget derived from the display rather than from the
    # code cannot be quietly raised the first time it fails.
    #
    # Zero switches the check off, for a debug build, where the figure means
    # nothing: Sill's pixel work measures 125 to 414 ms in debug against 3 to 7
    # in release, and no budget survives that.
    [int]$BudgetMs = 16
)

$ErrorActionPreference = 'Stop'

$log = Join-Path $env:APPDATA 'app.winters.sill\sill.log'
if (-not (Test-Path $log)) { throw "no log at $log, so nothing has been measured" }

# Opened sharing write, because Sill may still have it open.
$stream = [System.IO.File]::Open($log, 'Open', 'Read', 'ReadWrite')
$reader = New-Object System.IO.StreamReader($stream)
$all = $reader.ReadToEnd()
$reader.Close(); $stream.Close()

# 20:00:08.076 painted keystrokeAnswered x4 median 215300 us worst 298000 us
#
# The log stamps the time and not the date, so how recent a line is has to be
# decided by the clock alone. A reading from yesterday at the same minute would
# be counted, which is why the count and the visits are printed rather than
# only the verdict.
$cutoff = (Get-Date).AddMinutes(-$Since)

function Readings([string]$kind) {
    $found = @()
    $pattern = "(\d\d):(\d\d):(\d\d)\.\d+ painted $kind x(\d+) median (\d+) us worst (\d+) us"

    foreach ($m in [regex]::Matches($all, $pattern)) {
        $at = (Get-Date).Date.
            AddHours([int]$m.Groups[1].Value).
            AddMinutes([int]$m.Groups[2].Value).
            AddSeconds([int]$m.Groups[3].Value)
        if ($at -lt $cutoff) { continue }

        $found += [pscustomobject]@{
            Typed  = [int]$m.Groups[4].Value
            Median = [int]$m.Groups[5].Value
            Worst  = [int]$m.Groups[6].Value
        }
    }

    if ($found.Count -eq 0) { return $null }

    $medians = @($found | ForEach-Object { $_.Median }) | Sort-Object
    return [pscustomobject]@{
        Batches = $found.Count
        Typed   = ($found | Measure-Object -Property Typed -Sum).Sum
        # The middle of the batch medians. A mean would let one visit where the
        # machine woke a disk decide the answer.
        Median  = $medians[[int][math]::Floor($medians.Count / 2)]
        Worst   = ($found | Measure-Object -Property Worst -Maximum).Maximum
    }
}

$answered = Readings 'keystrokeAnswered'
$presented = Readings 'keystrokePresented'

Write-Host ''
Write-Host "--- keystroke to paint, last $Since min ---"

if (-not $answered) {
    Write-Host "nothing measured. Use Sill, put it away, then run this again." -ForegroundColor Yellow
    exit 1
}

'{0,-38} {1,8:N1} ms' -f 'answered, median', ($answered.Median / 1000)
'{0,-38} {1,8:N1} ms' -f 'answered, worst', ($answered.Worst / 1000)
if ($presented) {
    '{0,-38} {1,8:N1} ms' -f 'presented (a frame later), median', ($presented.Median / 1000)
    '{0,-38} {1,8:N1} ms' -f 'presented (a frame later), worst', ($presented.Worst / 1000)
}
'{0,-38} {1,8}' -f 'keystrokes measured', $answered.Typed
'{0,-38} {1,8}' -f 'visits they came from', $answered.Batches

Write-Host ''

if ($BudgetMs -le 0) {
    Write-Host "no budget applied: these figures are a report, not a budget" -ForegroundColor Yellow
    exit 0
}

if (($answered.Median / 1000) -gt $BudgetMs) {
    Write-Host ("OVER BUDGET  median keystroke answered in {0:N1} ms, allowed {1}" -f
        ($answered.Median / 1000), $BudgetMs) -ForegroundColor Red
    exit 1
}

Write-Host ("within budget: {0:N1} ms of {1}" -f ($answered.Median / 1000), $BudgetMs) -ForegroundColor Green
