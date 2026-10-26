# VoiceBar 0.1.0 构建清单

构建环境：Windows 11 x64（本机，Rust 1.99.0 x86_64-pc-windows-gnu，Tauri 2.12.5，Node 24.16.0）。
命令：`npx tauri build --bundles nsis,msi`（安装包）与同一 release 二进制（免安装 zip）。

| 文件 | 字节 | SHA-256 |
|---|---|---|
| `VoiceBar_0.1.0_x64-portable.zip` | 3,003,892 | `3c14e13aad9572d654bbb98a2995e42da3385740334c9f24f7406c6e65e89623` |
| `VoiceBar_0.1.0_x64-setup.exe` | 2,306,248 | `bc4fa3221d8ca90358626781d57a9d8c024c579b67a35e6bdd7e8cbd9b700f73` |
| `VoiceBar_0.1.0_x64_en-US.msi` | 3,203,072 | `e2163533f6b68a04c7032c8ff3d00d5eee227243ec8bb97024be25a163ac75ce` |
| `VoiceBar_0.1.0_x64_zh-CN.msi` | 3,198,976 | `a221327dba4b5be80e72260b1ecb554327aae13f557b4b47039b7c6055e8b308` |
