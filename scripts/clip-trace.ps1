<#
.SYNOPSIS
  Writes one line per Windows clipboard change, so a copy that goes missing
  can be lined up against sill.log by the second.

.DESCRIPTION
  Run it while reproducing a lost copy, then read clip-trace.log and
  %APPDATA%\app.winters.sill\sill.log around the same second. Sill prints one
  line for every write it makes to the clipboard and one for every change
  its history watcher sees, each with the sequence number Windows gives the
  change, which is the same number this script prints as seq=.

  What the two files say together:
    - no seq= increment for the copy: the source application never wrote.
    - an increment with the text, then a second increment matching a
      "clipboard write (...)" or "clipboard given back: Restored" line in
      sill.log: Sill wrote over it, and the line names the reason.
    - an increment to text= img=False files=False: something emptied the
      clipboard; only "clipboard write (extension clear)" in sill.log can.
    - text=<locked> on consecutive polls, or "lock held NN ms" /
      "last error 5" in sill.log: contention with another clipboard reader.

  It records up to 60 characters of every clipboard change, which can include
  a password or a token copied from a manager that does not set the exclusion
  format. Run it only while reproducing, and delete clip-trace.log when done.

.EXAMPLE
  powershell -STA -NoProfile -File scripts\clip-trace.ps1

  STA is required by System.Windows.Forms.Clipboard; pwsh 7 defaults to MTA.
#>
Add-Type -AssemblyName System.Windows.Forms
$u = Add-Type -Namespace W -Name U -PassThru -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetClipboardSequenceNumber();'
$out = Join-Path $env:APPDATA 'app.winters.sill\clip-trace.log'
$last = 0
"tracing to $out (Ctrl+C to stop)"
while ($true) {
  $s = $u::GetClipboardSequenceNumber()
  if ($s -ne $last) {
    $t = try { [Windows.Forms.Clipboard]::GetText() } catch { '<locked>' }
    if ($null -eq $t) { $t = '' }
    if ($t.Length -gt 60) { $t = $t.Substring(0, 60) }
    $line = '{0:HH:mm:ss.fff} seq={1} img={2} files={3} text={4}' -f (Get-Date), $s, [Windows.Forms.Clipboard]::ContainsImage(), [Windows.Forms.Clipboard]::ContainsFileDropList(), ($t -replace '\s+', ' ')
    $line
    Add-Content -Path $out -Value $line
    $last = $s
  }
  Start-Sleep -Milliseconds 200
}
