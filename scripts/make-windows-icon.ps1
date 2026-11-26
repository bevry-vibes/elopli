# Build a multi-size Windows .ico from the macOS app icon master.
# Usage: pwsh -NoProfile -File scripts/make-windows-icon.ps1
param(
	[string]$Source = "apps/macos/Elopli/Elopli/Assets.xcassets/AppIcon.appiconset/icon_1024.png",
	[string]$Destination = "apps/windows/Elopli/assets/elopli.ico"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

$sourcePath = (Resolve-Path $Source).Path
$destinationPath = [System.IO.Path]::GetFullPath(
	(Join-Path (Get-Location) $Destination)
)
$destinationDirectory = Split-Path -Parent $destinationPath
New-Item -ItemType Directory -Force -Path $destinationDirectory | Out-Null

$sizes = @(16, 20, 24, 32, 40, 48, 64, 128, 256)
$masterImage = [System.Drawing.Image]::FromFile($sourcePath)
$images = @()
try {
	foreach ($size in $sizes) {
		$bitmap = New-Object System.Drawing.Bitmap(
			$size,
			$size,
			[System.Drawing.Imaging.PixelFormat]::Format32bppArgb
		)
		$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
		try {
			$graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
			$graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
			$graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
			$graphics.Clear([System.Drawing.Color]::Transparent)
			$graphics.DrawImage($masterImage, 0, 0, $size, $size)
		}
		finally {
			$graphics.Dispose()
		}
		$stream = New-Object System.IO.MemoryStream
		try {
			$bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
			$images += [pscustomobject]@{ Size = $size; Bytes = $stream.ToArray() }
		}
		finally {
			$stream.Dispose()
			$bitmap.Dispose()
		}
	}
}
finally {
	$masterImage.Dispose()
}

$output = New-Object System.IO.MemoryStream
$writer = New-Object System.IO.BinaryWriter($output)
try {
	$writer.Write([UInt16]0)
	$writer.Write([UInt16]1)
	$writer.Write([UInt16]$images.Count)
	$offset = 6 + 16 * $images.Count
	foreach ($image in $images) {
		$dimension = if ($image.Size -ge 256) { 0 } else { $image.Size }
		$writer.Write([byte]$dimension)
		$writer.Write([byte]$dimension)
		$writer.Write([byte]0)
		$writer.Write([byte]0)
		$writer.Write([UInt16]1)
		$writer.Write([UInt16]32)
		$writer.Write([UInt32]$image.Bytes.Length)
		$writer.Write([UInt32]$offset)
		$offset += $image.Bytes.Length
	}
	foreach ($image in $images) {
		$writer.Write($image.Bytes)
	}
	$writer.Flush()
	[System.IO.File]::WriteAllBytes($destinationPath, $output.ToArray())
}
finally {
	$writer.Dispose()
	$output.Dispose()
}

Write-Host "Wrote $destinationPath"
