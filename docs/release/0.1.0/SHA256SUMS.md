# VoiceBar 0.1.0 构建清单

构建环境：Windows 11 x64（本机，Rust 1.99.0 x86_64-pc-windows-gnu，Tauri CLI 2.12.3，Node 24.16.0）。
命令：`npx tauri build --bundles nsis,msi`（安装包）与同一 release 二进制（免安装 zip）。
签名：使用仓库配置的 minisign 私钥签名，`latest.json` 由 `scripts/make-latest-json.mjs` 生成并经 `scripts/verify-release.mjs` 校验通过。

| 文件 | 字节 | SHA-256 |
|---|---|---|
| `VoiceBar_0.1.0_x64-portable.zip` | 3,250,856 | `47d190ef07c1d0c37cd8fadebaed92e0a57adedc59b6501d9b7dc9de232f945e` |
| `VoiceBar_0.1.0_x64-setup.exe` | 2,485,504 | `507d9726099b26a76b3d07ecd158a25a6d29ee8d9bd826a383149ff78e35485e` |
| `VoiceBar_0.1.0_x64_en-US.msi` | 3,543,040 | `91221d66f7814a1040fff8d7950e0058672c54ec84317c55630b9e568cdc76c5` |
| `VoiceBar_0.1.0_x64_zh-CN.msi` | 3,538,944 | `f16395ed5e78366fc9bf21a8f0f4eb5b4502e6723e67e0b0542b1dd1d93fe7dd` |
| `latest.json` | 859 | `14df2176be8327f1524d2939bc9215a678fb3fc8d15bbc6c0f849a4c1a9dbede` |
