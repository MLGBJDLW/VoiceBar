param(
    [Parameter(Mandatory=$true)][string]$Bundle,
    [Parameter(Mandatory=$true)][string]$Version
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$bundlePath = (Resolve-Path -LiteralPath $Bundle).Path
Push-Location -LiteralPath $root
try {
    # Key material stays in the inherited environment, never command arguments.
    & cargo run --quiet -p voicebar-release-tools -- $bundlePath $Version
    if ($LASTEXITCODE -ne 0) { throw 'Updater signing failed.' }
} finally { Pop-Location }
