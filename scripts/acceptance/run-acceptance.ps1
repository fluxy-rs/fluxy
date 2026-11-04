# M2-09 acceptance wrapper for Windows.
$ErrorActionPreference = "Stop"

$scriptDir = $PSScriptRoot
if (-not $scriptDir) {
  $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
}

if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
  Write-Error "node is required to run the M2-09 acceptance driver"
  exit 127
}

& node (Join-Path $scriptDir "acceptance.mjs") @args
exit $LASTEXITCODE
