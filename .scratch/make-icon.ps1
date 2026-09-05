# 生成 SecretBox 应用图标源图（1024x1024 PNG）：
# 圆角方块底 + 白色挂锁图形，配色取自 web/style.css 的 --accent
Add-Type -AssemblyName System.Drawing

$size = 1024
$bmp = New-Object System.Drawing.Bitmap($size, $size)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.Clear([System.Drawing.Color]::Transparent)

# 圆角背景
$radius = 220
$rect = New-Object System.Drawing.Rectangle(0, 0, $size, $size)
$path = New-Object System.Drawing.Drawing2D.GraphicsPath
$path.AddArc(0, 0, $radius, $radius, 180, 90)
$path.AddArc($size - $radius, 0, $radius, $radius, 270, 90)
$path.AddArc($size - $radius, $size - $radius, $radius, $radius, 0, 90)
$path.AddArc(0, $size - $radius, $radius, $radius, 90, 90)
$path.CloseFigure()
$bg = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 79, 124, 255))
$g.FillPath($bg, $path)

# 锁环（圆弧）
$pen = New-Object System.Drawing.Pen([System.Drawing.Color]::White, 76)
$pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
$pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$g.DrawArc($pen, 322, 232, 380, 380, 180, 180)

# 锁体（圆角矩形）
$bodyRect = New-Object System.Drawing.Rectangle(272, 442, 480, 380)
$bodyPath = New-Object System.Drawing.Drawing2D.GraphicsPath
$br = 70
$bodyPath.AddArc($bodyRect.X, $bodyRect.Y, $br, $br, 180, 90)
$bodyPath.AddArc($bodyRect.Right - $br, $bodyRect.Y, $br, $br, 270, 90)
$bodyPath.AddArc($bodyRect.Right - $br, $bodyRect.Bottom - $br, $br, $br, 0, 90)
$bodyPath.AddArc($bodyRect.X, $bodyRect.Bottom - $br, $br, $br, 90, 90)
$bodyPath.CloseFigure()
$white = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::White)
$g.FillPath($white, $bodyPath)

# 锁孔
$keyhole = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 79, 124, 255))
$g.FillEllipse($keyhole, 462, 540, 100, 100)
$g.FillRectangle($keyhole, 492, 600, 40, 110)

$g.Dispose()
$out = Join-Path $PSScriptRoot "app-icon.png"
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "图标已生成: $out"
