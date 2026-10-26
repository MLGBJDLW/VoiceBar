param(
    [ValidateRange(0, 16384)]
    [int]$MaxBuildCacheMiB = 512,
    # 'portable' copies exe + WebView2Loader.dll into the project root (previous
    # behaviour). 'installer' builds the NSIS/MSI bundles used for GitHub Releases.
    [ValidateSet('portable', 'installer')]
    [string]$Mode = 'installer'
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
$node = (Get-Command node -ErrorAction SilentlyContinue).Source
if (-not $node) { $node = 'C:\Program Files\nodejs\node.exe' }
$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargo) { $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
$npm = Join-Path (Split-Path $node) 'node_modules\npm\bin\npm-cli.js'
$npxCli = Join-Path (Split-Path $node) 'node_modules\npm\bin\npx-cli.js'
if (-not (Test-Path -LiteralPath $node)) { throw 'Node.js 22+ is required.' }
if (-not (Test-Path -LiteralPath $cargo)) { throw 'A Rust Windows toolchain is required.' }
$env:Path = (Split-Path $node) + ';' + (Split-Path $cargo) + ';' + $env:Path
if (-not (Test-Path 'node_modules')) {
    & $node $npm ci --no-audit --no-fund
    if ($LASTEXITCODE -ne 0) { throw 'npm ci failed' }
}
& $node $npm run build
if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed' }
& $node --test tests/effects.test.mjs tests/shortcut.test.mjs
if ($LASTEXITCODE -ne 0) { throw 'Frontend tests failed' }
& $cargo test --manifest-path src-tauri/Cargo.toml
if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }

if ($Mode -eq 'installer') {
    # Same invocation CI uses, so a local dry run mirrors the GitHub Release.
    & $node $npxCli tauri build --bundles nsis,msi
    if ($LASTEXITCODE -ne 0) { throw 'Installer build failed' }
    Get-ChildItem -LiteralPath 'target\release\bundle' -Recurse -Include '*.exe', '*.msi' |
        Select-Object FullName, Length
} else {
    & $cargo build --release --features custom-protocol --manifest-path src-tauri/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    Copy-Item -LiteralPath 'target\release\voicebar.exe' -Destination 'VoiceBar.exe' -Force
    # Some toolchains dynamically link the WebView2 loader. Ship it beside the exe.
    $webviewLoader = 'target\release\WebView2Loader.dll'
    if (Test-Path -LiteralPath $webviewLoader) {
        Copy-Item -LiteralPath $webviewLoader -Destination 'WebView2Loader.dll' -Force
    }
    Get-Item -LiteralPath 'VoiceBar.exe' | Select-Object FullName, Length
}

# Bound retained intermediate files after a successful build.
# The next Rust build recompiles dependencies when this removes target.
& (Join-Path $PSScriptRoot 'clean-build-cache.ps1') -MaxCacheMiB $MaxBuildCacheMiB -CargoPath $cargo
