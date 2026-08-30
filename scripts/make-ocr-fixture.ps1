# Draws known words into a PNG, so recognition can be checked against an answer.
Add-Type -AssemblyName System.Drawing

$text = "The quick brown fox 12345"
$bmp  = New-Object System.Drawing.Bitmap 640, 160
$g    = [System.Drawing.Graphics]::FromImage($bmp)
$g.Clear([System.Drawing.Color]::White)
$font = New-Object System.Drawing.Font "Segoe UI", 32
$g.DrawString($text, $font, [System.Drawing.Brushes]::Black, 20, 50)
$g.Dispose()

$out = Join-Path $env:TEMP "sill-ocr-fixture.png"
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()

Write-Host "wrote $out"
Write-Host "text: $text"
