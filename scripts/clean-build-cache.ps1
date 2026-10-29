[CmdletBinding(SupportsShouldProcess=$true)]
param(
    [ValidateRange(0,16384)][int]$MaxCacheMiB = 512,
    [string]$CargoPath = ''
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$target = Join-Path $root 'target'
function Get-CacheBytes {
    if (-not (Test-Path -LiteralPath $target)) { return [long]0 }
    if ((Get-Item -LiteralPath $target -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw 'Refusing to clean a linked target directory.'
    }
    [long]$total = 0
    $pending = New-Object 'System.Collections.Generic.Stack[string]'
    $pending.Push($target)
    while ($pending.Count) {
        foreach ($entry in Get-ChildItem -LiteralPath $pending.Pop() -Force) {
            if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked cache entry detected.' }
            if ($entry.PSIsContainer) { $pending.Push($entry.FullName) } else { $total += $entry.Length }
        }
    }
    return $total
}
$before = Get-CacheBytes
$limit = [long]$MaxCacheMiB * 1MB
if ($before -gt $limit) {
    if (-not (Test-Path -LiteralPath (Join-Path $root 'VoiceBar.exe') -PathType Leaf)) {
        throw 'Copy the current native executable to VoiceBar.exe before cleaning.'
    }
    if ($PSCmdlet.ShouldProcess($target, 'Remove only this project Cargo cache')) {
        if (-not $CargoPath) { $CargoPath = (Get-Command cargo -ErrorAction Stop).Source }
        & $CargoPath clean --manifest-path (Join-Path $root 'Cargo.toml') --target-dir $target
        if ($LASTEXITCODE -ne 0) { throw 'Cargo cache cleanup failed.' }
    }
}
$after = Get-CacheBytes
[pscustomobject]@{CachePath=$target;BeforeBytes=$before;AfterBytes=$after;FreedBytes=($before-$after);LimitBytes=$limit} | ConvertTo-Json -Compress
