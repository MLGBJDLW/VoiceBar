[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [ValidateRange(0, 16384)]
    [int]$MaxCacheMiB = 512,
    [string]$CargoPath = ''
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$cacheRoot = Join-Path $projectRoot 'target'
$manifest = Join-Path $projectRoot 'Cargo.toml'
$bundleRoot = Join-Path $cacheRoot 'release\bundle'

function Get-CacheBytes {
    if (-not (Test-Path -LiteralPath $cacheRoot)) { return [long]0 }
    if ((Get-Item -LiteralPath $cacheRoot -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw 'Refusing to clean a linked target directory.'
    }
    [long]$bytes = 0
    $pending = New-Object 'System.Collections.Generic.Stack[string]'
    $pending.Push($cacheRoot)
    while ($pending.Count -gt 0) {
        foreach ($entry in Get-ChildItem -LiteralPath $pending.Pop() -Force) {
            if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Refusing to traverse a linked cache entry: $($entry.FullName)"
            }
            if ($entry.PSIsContainer) { $pending.Push($entry.FullName) }
            else { $bytes += $entry.Length }
        }
    }
    return $bytes
}

$before = Get-CacheBytes
[long]$limit = [long]$MaxCacheMiB * 1MB
$cleaned = $false
if ($before -gt $limit) {
    # Clean only after the current release bundles have been handed out of target.
    if (-not (Test-Path -LiteralPath $bundleRoot -PathType Container)) {
        throw 'Publish installers before removing the build cache (-Mode portable must copy VoiceBar.exe first).'
    }
    $unpublished = @(Get-ChildItem -LiteralPath $bundleRoot -Recurse -File -Include '*.exe', '*.msi' |
        Where-Object { $_.Name -like 'VoiceBar_*' -and -not (Get-ChildItem -LiteralPath $projectRoot -File -Filter $_.Name -ErrorAction SilentlyContinue) })
    if ($unpublished.Count -gt 0) {
        throw "Installers are still only in target/; copy them out before cleaning: $($unpublished.Name -join ', ')"
    }
    if ($PSCmdlet.ShouldProcess($cacheRoot, "Remove project-local Cargo cache exceeding $MaxCacheMiB MiB")) {
        if (-not $CargoPath) { $CargoPath = (Get-Command cargo -ErrorAction SilentlyContinue).Source }
        if (-not $CargoPath) { $CargoPath = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
        if (-not (Test-Path -LiteralPath $CargoPath -PathType Leaf)) { throw 'Cargo is required to clean its build cache.' }
        # Explicit target-dir prevents environment settings from selecting a shared cache.
        # Cargo acquires its own build-directory lock before deleting artifacts.
        & $CargoPath clean --manifest-path $manifest --target-dir $cacheRoot
        if ($LASTEXITCODE -ne 0) { throw 'Cargo cache cleanup failed.' }
        $cleaned = $true
    }
}
$after = Get-CacheBytes
if (-not $WhatIfPreference -and $after -gt $limit) { throw 'Build cache remains above the configured limit.' }
[pscustomobject]@{
    CachePath = $cacheRoot
    LimitBytes = $limit
    BeforeBytes = $before
    AfterBytes = $after
    FreedBytes = $before - $after
    Cleaned = $cleaned
    Preview = [bool]$WhatIfPreference
} | ConvertTo-Json -Compress
