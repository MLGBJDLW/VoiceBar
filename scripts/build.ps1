param(
    [ValidateSet('portable', 'installer')][string]$Mode = 'installer',
    [ValidateSet('x86_64-pc-windows-gnu', 'x86_64-pc-windows-msvc')]
    [string]$Target = 'x86_64-pc-windows-gnu',
    [ValidateRange(0, 16384)][int]$MaxBuildCacheMiB = 512,
    [switch]$SkipTests
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectRoot
try {
    $cargo = (Get-Command cargo -ErrorAction Stop).Source
    if (-not $env:GPUI_FXC_PATH -or -not (Test-Path -LiteralPath $env:GPUI_FXC_PATH)) {
        $sdk = Join-Path "${env:ProgramFiles(x86)}" 'Windows Kits\10\bin'
        $fxc = Get-ChildItem -Path "$sdk\*\x64\fxc.exe" -File -ErrorAction SilentlyContinue |
            Sort-Object FullName -Descending | Select-Object -First 1
        if (-not $fxc) { throw 'Install the Windows SDK or set GPUI_FXC_PATH to fxc.exe.' }
        $env:GPUI_FXC_PATH = $fxc.FullName
    }
    if (-not $SkipTests) {
        & $cargo test --workspace --locked --target $Target
        if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }
    }
    & $cargo build --release --locked -p voicebar --target $Target
    if ($LASTEXITCODE -ne 0) { throw 'Native release build failed.' }
    $release = Join-Path $projectRoot "target\$Target\release"
    Copy-Item -LiteralPath (Join-Path $release 'voicebar.exe') -Destination 'VoiceBar.exe' -Force

    $payload = Join-Path $projectRoot 'artifacts\payload'
    if (Test-Path -LiteralPath $payload) {
        if ((Get-Item -LiteralPath $payload -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw 'Refusing to replace a linked payload directory.'
        }
        Remove-Item -LiteralPath $payload -Recurse -Force
    }
    New-Item -ItemType Directory -Path $payload -Force | Out-Null
    Copy-Item -LiteralPath 'VoiceBar.exe' -Destination $payload -Force
    # Ship only compiler runtime DLLs the GNU build can require. No browser DLLs.
    if ($Target.EndsWith('-gnu')) {
        $gcc = (Get-Command gcc.exe -ErrorAction SilentlyContinue).Source
        if (-not $gcc) { $gcc = (Get-Command gcc -ErrorAction SilentlyContinue).Source }
        foreach ($name in 'libgcc_s_seh-1.dll', 'libstdc++-6.dll', 'libwinpthread-1.dll') {
            if ($gcc) {
                $library = (& $gcc "-print-file-name=$name").Trim()
                if (Test-Path -LiteralPath $library -PathType Leaf) {
                    Copy-Item -LiteralPath $library -Destination $payload -Force
                    Copy-Item -LiteralPath $library -Destination $projectRoot -Force
                }
            }
        }
    }
    Copy-Item -LiteralPath 'LICENSE' -Destination $payload -Force
    $manifest = Get-Content 'crates\voicebar\Cargo.toml' -Raw -Encoding UTF8
    $version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
    if (-not $version) { throw 'Cannot read application version.' }
    $portable = Join-Path $projectRoot "artifacts\VoiceBar_${version}_x64-portable.zip"
    if (Test-Path -LiteralPath $portable) { Remove-Item -LiteralPath $portable }
    Compress-Archive -Path "$payload\*" -DestinationPath $portable
    if ($Mode -eq 'installer') {
        $nsis = (Get-Command makensis.exe -ErrorAction SilentlyContinue).Source
        if (-not $nsis) {
            $candidates = @(
                (Join-Path "${env:ProgramFiles(x86)}" 'NSIS\makensis.exe'),
                (Join-Path $env:LOCALAPPDATA 'tauri\NSIS\makensis.exe')
            )
            $nsis = $candidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
        }
        if (-not $nsis) { throw 'NSIS is required for -Mode installer. Install NSIS or use -Mode portable.' }
        $fileVersion = ($version -split '[-+]')[0] + '.0'
        $setup = Join-Path $projectRoot "artifacts\VoiceBar_${version}_x64-setup.exe"
        & $nsis "/DAPP_VERSION=$version" "/DFILE_VERSION=$fileVersion" "/DPAYLOAD_DIR=$payload" "/DOUTPUT_FILE=$setup" 'installer.nsi'
        if ($LASTEXITCODE -ne 0) { throw 'Native installer build failed.' }
        Get-Item -LiteralPath $setup | Select-Object FullName, Length
    }
    Get-Item -LiteralPath 'VoiceBar.exe', $portable | Select-Object FullName, Length
    & (Join-Path $PSScriptRoot 'clean-build-cache.ps1') -MaxCacheMiB $MaxBuildCacheMiB -CargoPath $cargo
} finally { Pop-Location }
