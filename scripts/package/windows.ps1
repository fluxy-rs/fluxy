# Build the Windows installer (M5-04).
#
# Produces the release binary and, when the bundler tooling is available, the
# NSIS and MSI installers under src-tauri/target/release/bundle.
#
#   pwsh -NoProfile -File scripts/package/windows.ps1
#   pwsh -NoProfile -File scripts/package/windows.ps1 -Bundles nsis
#
# Signing is deliberately not automated here: a certificate belongs to the
# release manager. See docs/PACKAGING.md for the variables Tauri reads.

param(
    [string]$Bundles = 'nsis,msi',
    [switch]$NoBundle
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Push-Location $root

try {
    if ($NoBundle) {
        bun x tauri build --no-bundle
    } else {
        bun x tauri build --bundles $Bundles
    }
    if ($LASTEXITCODE -ne 0) { throw "tauri build failed with $LASTEXITCODE" }

    $bin = Join-Path $root 'target/release/fluxy.exe'
    if (Test-Path $bin) {
        $info = Get-Item $bin
        Write-Host ("binary: {0} ({1:N0} bytes)" -f $info.FullName, $info.Length)
        $version = (Get-Item $bin).VersionInfo
        Write-Host ("version: {0} / product: {1}" -f $version.FileVersion, $version.ProductName)
    } else {
        Write-Warning 'the release binary was not produced'
    }

    $bundleDir = Join-Path $root 'target/release/bundle'
    if (Test-Path $bundleDir) {
        Get-ChildItem $bundleDir -Recurse -Include *.exe, *.msi | ForEach-Object {
            Write-Host ("installer: {0} ({1:N0} bytes)" -f $_.FullName, $_.Length)
        }
    } else {
        Write-Host 'no bundle directory: the installer step did not run'
    }
} finally {
    Pop-Location
}
