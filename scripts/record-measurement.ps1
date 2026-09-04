<#
.SYNOPSIS
  Writes one measurement down, with everything needed to judge it.

.DESCRIPTION
  Dot-source this from a measuring script and call `Write-Measurement` when the
  script has decided its answer. It writes one file per measurement into
  `docs/measurements/`, and `scripts/benchmark-page.mjs` turns those into the
  public page.

      . "$PSScriptRoot\record-measurement.ps1"
      Write-Measurement -Id summon -Reading '25 ms median, of 12' -Within $true `
          -Build release -By scripts/measure-summon.ps1

  ## Why a verdict rather than a number

  `-Within` is the measuring script's own conclusion, not something the page
  works out. Every one of these scripts already knows its budget and already
  exits non-zero when it is over, and a page that re-derived the verdict would
  be a second copy of every threshold with nothing making the two agree. So the
  page reports what the script concluded. Pass `$null` for a row that is
  reported rather than enforced.

  ## Why the build is not guessed

  A release build and a development build are two orders of magnitude apart in
  the part of Sill that draws pixels, so a reading that does not say which it
  was cannot be compared to anything. Scripts that start a binary read it from
  the path; the one that reads the launcher's own log cannot know, and asks.

  ## What is written about the machine

  What a reader needs to judge the number and nothing else: the edition of
  Windows, how many logical processors and how much memory. No machine name,
  no user name, no paths. This file becomes a public page.
#>

# Asked of `scripts/machine.mjs` rather than worked out here.
#
# Two descriptions of the same desk would appear on the page as two machines,
# and a reader comparing rows would be told two readings came from different
# hardware when they came from the same one. There is no fallback on purpose:
# without node there is no page either, so failing here is failing early.
function Get-MeasurementMachine {
    $said = & node (Join-Path $PSScriptRoot 'machine.mjs')
    if ($LASTEXITCODE -ne 0 -or -not $said) {
        throw 'node could not describe this machine, and a reading with no machine on it is not checkable'
    }
    return ([string]$said).Trim()
}

# Which build a binary is, read off its path rather than asked for.
#
# Cargo puts one under target/release and the other under target/debug and
# there is no third place, so anything that starts a binary can answer this
# without being told. Refused rather than defaulted: a path that is neither is
# a build nobody can place, and guessing "release" for one of those would
# publish a development figure beside a release budget.
function Get-MeasurementBuild([string]$Exe) {
    $path = ([string]$Exe).Replace('\', '/')
    if ($path -like '*/target/release/*') { return 'release' }
    if ($path -like '*/target/debug/*') { return 'debug' }
    throw "cannot tell which build $Exe is, so nothing it measures can be recorded"
}

function Write-Measurement {
    param(
        # Must match a row of scripts/benchmarks.json. The page generator
        # refuses a measurement whose row it does not have, because a reading
        # for a row that was renamed is a reading that silently stops appearing.
        [Parameter(Mandatory)][string]$Id,

        # The number as the script would print it, units and spread included.
        # "25 ms median, of 12" says more than "25" and is what a reader
        # compares their own run against.
        [Parameter(Mandatory)][string]$Reading,

        # $true, $false, or $null where the row has no budget.
        [AllowNull()][Nullable[bool]]$Within,

        [Parameter(Mandatory)][ValidateSet('release', 'debug')][string]$Build,

        [Parameter(Mandatory)][string]$By
    )

    $root = Split-Path $PSScriptRoot -Parent
    $dir = Join-Path $root 'docs\measurements'
    New-Item -ItemType Directory -Path $dir -Force | Out-Null

    $version = (Get-Content (Join-Path $root 'package.json') -Raw | ConvertFrom-Json).version

    $record = [ordered]@{
        id      = $Id
        reading = $Reading
        within  = $Within
        build   = $Build
        machine = Get-MeasurementMachine
        version = $version
        on      = Get-Date -Format 'yyyy-MM-dd'
        by      = $By
    }

    # Written with Unix line endings and one at the end, because the page
    # generator compares what it renders against what is committed, and a file
    # that differs only in its line endings would fail that comparison on a
    # machine whose git checked it out the other way.
    $path = Join-Path $dir "$Id.json"
    $json = (($record | ConvertTo-Json -Depth 4) -replace "`r`n", "`n") + "`n"
    [System.IO.File]::WriteAllText($path, $json, (New-Object System.Text.UTF8Encoding($false)))

    Write-Host "  recorded $Id -> docs/measurements/$Id.json" -ForegroundColor DarkGray
}
