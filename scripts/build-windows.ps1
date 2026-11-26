# Build the Windows app and CLI, then create the portable ZIP, checksums,
# and a Scoop manifest. Sign the executables first when a certificate is
# supplied and signtool.exe is available.
[CmdletBinding()]
param(
	[ValidateSet("Debug", "Release")]
	[string]$Configuration = "Release",
	[string]$Toolchain = "",
	[switch]$SkipBuild,
	[switch]$NoZip,
	[string]$Version = "",
	[string]$ReleaseUrl = "",
	[string]$CertificatePath = "",
	[string]$CertificatePassword = "",
	[string]$TimestampUrl = "http://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $root

if (-not $Version) {
	$cargoToml = Get-Content (Join-Path $root "Cargo.toml") -Raw
	if ($cargoToml -match '(?m)^version = "([^"]+)"') {
		$Version = $Matches[1]
	}
	else {
		throw "cannot read the version from Cargo.toml"
	}
}

$cargoPrefix = @()
if ($Toolchain) {
	$cargoPrefix = @("+$Toolchain")
}

function Invoke-Cargo {
	param([string[]]$Arguments)
	& cargo @cargoPrefix @Arguments
	if ($LASTEXITCODE -ne 0) {
		throw "cargo $($Arguments -join ' ') failed"
	}
}

if (-not $SkipBuild) {
	$buildArguments = @("build", "--locked")
	if ($Configuration -eq "Release") {
		$buildArguments += "--release"
	}
	Invoke-Cargo ($buildArguments + @("-p", "elopli-app", "--features", "windows"))
	Invoke-Cargo ($buildArguments + @("-p", "elopli"))
}

$targetDir = if ($env:CARGO_TARGET_DIR) {
	[System.IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
}
else {
	Join-Path $root "target"
}
$profile = if ($Configuration -eq "Release") { "release" } else { "debug" }
$appExe = Join-Path $targetDir "$profile\ElopliApp.exe"
$cliExe = Join-Path $targetDir "$profile\elopli.exe"
if (-not (Test-Path $appExe)) {
	throw "missing $appExe; run without -SkipBuild"
}
if (-not (Test-Path $cliExe)) {
	throw "missing $cliExe; run without -SkipBuild"
}

$dist = Join-Path $root "dist"
$packageName = "Elopli-$Version-windows-x64"
$artifact = Join-Path $dist $packageName
$zip = Join-Path $dist "$packageName.zip"
if (Test-Path $artifact) {
	Remove-Item -Recurse -Force $artifact
}
New-Item -ItemType Directory -Force -Path $artifact | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $artifact "cli") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $artifact "docs") | Out-Null

if ($CertificatePath) {
	$signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
	if (-not $signtool) {
		Write-Warning "signtool.exe was not found; the executables are unsigned"
	}
	else {
		$signArguments = @(
			"sign", "/fd", "SHA256", "/tr", $TimestampUrl, "/td", "SHA256",
			"/f", (Resolve-Path $CertificatePath).Path
		)
		if ($CertificatePassword) {
			$signArguments += @("/p", $CertificatePassword)
		}
		$signArguments += @($appExe, $cliExe)
		& $signtool.Source @signArguments
		if ($LASTEXITCODE -ne 0) {
			throw "signtool failed"
		}
	}
}

Copy-Item $appExe (Join-Path $artifact "ElopliApp.exe")
Copy-Item $cliExe (Join-Path $artifact "cli\elopli.exe")
Copy-Item README.md, LICENSE.md, PRIVACY.md, SECURITY.md, CHANGELOG.md, THIRD-PARTY-NOTICES.md, devices.example.json $artifact
Copy-Item docs\api.md, docs\windows.md, docs\platforms.md, docs\shortcuts.md (Join-Path $artifact "docs")
if (Test-Path (Join-Path $root "docs\screenshots")) {
	Copy-Item -Recurse (Join-Path $root "docs\screenshots") (Join-Path $artifact "docs")
}

$files = Get-ChildItem -Recurse -File $artifact | Where-Object { $_.Name -ne "SHA256SUMS" } | Sort-Object FullName
$hashes = foreach ($file in $files) {
	$relative = [System.IO.Path]::GetRelativePath($artifact, $file.FullName).Replace('\', '/')
	$hash = (Get-FileHash -Algorithm SHA256 $file.FullName).Hash.ToLowerInvariant()
	"$hash  $relative"
}
$hashes | Set-Content -Encoding ascii (Join-Path $artifact "SHA256SUMS")

if (-not $NoZip) {
	Remove-Item -Force $zip -ErrorAction SilentlyContinue
	Compress-Archive -Path (Join-Path $artifact "*") -DestinationPath $zip
	$zipHash = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLowerInvariant()
	"$zipHash  $packageName.zip" | Set-Content -Encoding ascii (Join-Path $dist "SHA256SUMS")
}

$releaseBase = if ($ReleaseUrl) {
	$ReleaseUrl.TrimEnd('/')
}
else {
	"https://github.com/bevry-vibes/elopli/releases/download/v$Version"
}
$zipHash = if (Test-Path $zip) { (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLowerInvariant() } else { "" }
$scoop = [ordered]@{
	version = $Version
	description = "Native tray app for Elgato Key Light and Key Light Air"
	homepage = "https://github.com/bevry-vibes/elopli"
	license = "RPL-1.5"
	architecture = [ordered]@{
		"64bit" = [ordered]@{
			url = "$releaseBase/$packageName.zip"
			hash = $zipHash
		}
	}
	bin = @("ElopliApp.exe", "cli\elopli.exe")
	shortcuts = @(, @("ElopliApp.exe", "Elopli"))
}
$scoopJSON = $scoop | ConvertTo-Json -Depth 6
$scoopJSON | Set-Content -Encoding utf8 (Join-Path $dist "elopli.json")

Write-Host "Package: $artifact"
if (Test-Path $zip) {
	Write-Host "Zip:     $zip"
}
Write-Host "Scoop:   $(Join-Path $dist 'elopli.json')"
