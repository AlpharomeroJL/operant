# Renders the OCR-able fixture image via System.Drawing. Deterministic layout.
Add-Type -AssemblyName System.Drawing
$out = Join-Path $PSScriptRoot "docs\sample.png"
$bmp = New-Object System.Drawing.Bitmap(800, 240)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::None
$g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::SingleBitPerPixelGridFit
$g.Clear([System.Drawing.Color]::White)
$font = New-Object System.Drawing.Font("Arial", 26, [System.Drawing.FontStyle]::Bold)
$brush = [System.Drawing.Brushes]::Black
$g.DrawString("OPERANT FIXTURE INVOICE", $font, $brush, 24, 30)
$g.DrawString("INV-2026-0711", $font, $brush, 24, 90)
$g.DrawString("TOTAL 142.50", $font, $brush, 24, 150)
$g.Dispose()
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "sample.png written: $out"
