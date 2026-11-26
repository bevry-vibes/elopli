# Generate THIRD-PARTY-NOTICES.md from the locked Cargo metadata.
# Usage: pwsh -NoProfile -File scripts/generate-third-party-notices.ps1
param(
	[string]$Output = "THIRD-PARTY-NOTICES.md"
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $root

$metadata = cargo metadata --format-version 1 --locked | ConvertFrom-Json
$packages = $metadata.packages |
	Where-Object { $_.source -and $_.name -ne "elopli" -and $_.name -ne "elopli-core" -and $_.name -ne "elopli-session" -and $_.name -ne "elopli-ffi" -and $_.name -ne "elopli-app" -and $_.name -ne "uniffi-bindgen" } |
	Sort-Object name, version

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("# Third-party notices")
$lines.Add("")
$lines.Add("Elopli is licensed under RPL-1.5. It depends on the open-source Rust crates below.")
$lines.Add("Each crate remains under its own license; the license identifier comes from its Cargo metadata.")
$lines.Add("The allow list and source policy are enforced by `cargo deny check` with [deny.toml](deny.toml).")
$lines.Add("")
$lines.Add("| Crate | Version | License | Repository |")
$lines.Add("| --- | --- | --- | --- |")
foreach ($package in $packages) {
	$license = if ($package.license) { $package.license } else { "see crate metadata" }
	$repository = if ($package.repository) { $package.repository } else { "" }
	$lines.Add("| $($package.name) | $($package.version) | $license | $repository |")
}
$lines.Add("")
$lines.Add("Generated from `Cargo.lock`; regenerate with `scripts/generate-third-party-notices.ps1`.")

$path = Join-Path $root $Output
[System.IO.File]::WriteAllText($path, ($lines -join "`n"))
Write-Host "Wrote $path"
