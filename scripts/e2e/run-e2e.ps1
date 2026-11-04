# M1-09 end-to-end acceptance wrapper for Windows.
$ErrorActionPreference = "Stop"

$scriptDir = $PSScriptRoot
if (-not $scriptDir) {
  $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
}

if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
  Write-Error "node is required to run the M1-09 end-to-end driver"
  exit 127
}

& node (Join-Path $scriptDir "e2e.mjs") @args
exit $LASTEXITCODE
