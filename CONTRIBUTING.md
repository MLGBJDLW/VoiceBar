# 贡献指南

VoiceBar 目前以个人维护为主。欢迎 Issue、功能建议和 PR；大改动建议先说明目标与方案。

## 环境

- Windows 10/11 x64 与 Rust 稳定版，满足依赖的最低 Rust 版本。
- MSVC 或 GNU Windows 链接工具链，CI 使用 `x86_64-pc-windows-msvc`。
- Windows SDK 的 `fxc.exe`；无法自动发现时设置 `GPUI_FXC_PATH`。
- 构建安装包需要 NSIS；发布脚本与图标维护需要 Node.js 24，无需安装 JavaScript 依赖。

## 本地检查

```powershell
cargo fmt --all -- --check
cargo test --workspace --locked
node --test tests/update-manifest.test.mjs
cargo run -p voicebar
```

完整构建由根目录 `scripts/build.ps1` 负责：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Mode portable -Target x86_64-pc-windows-gnu
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Mode installer -Target x86_64-pc-windows-msvc
```

便携构建产出根目录 `VoiceBar.exe`，NSIS 安装包产出 `artifacts/VoiceBar_<版本>_x64-setup.exe`。`-Target` 可选择 GNU 或 MSVC，需预先安装对应工具链。不要提交 `target`、`artifacts`、`.nexa`、根目录 `VoiceBar.exe` 或本地密钥。

### Cargo workspace 与锁文件

根 `Cargo.toml` 包含应用 `crates/voicebar` 和内部签名工具 `crates/release-tools`。只保留根目录 `Cargo.lock`，成员目录中的锁文件不会生效。升级依赖时修改相应成员的 `Cargo.toml`，在根目录执行 `cargo update -p <crate>` 并提交根锁文件。

## 代码约定

- 应用与原生 GPUI 界面位于 `crates/voicebar/src/`，GPUI 和 gpui-component 的版本与功能开关以 Cargo 为准。
- 密钥只经 Windows DPAPI 落盘，沿用 `%APPDATA%\studio.voicebar.desktop\settings.json`；错误与日志不得包含密钥原文。
- 新增识别协议时，同时更新 `config.rs` 校验、原生设置页的预设、协议适配和 README 配置表。
- 修改全局输入、快捷键、托盘或更新行为时，记录 Windows 上的实际验证范围与未验证项。
- 图标由 `node scripts/render-icons.mjs` 从 `crates/voicebar/icons/app-icon.svg` 生成；生成结果需确定且尺寸一致。

## 提交规范

使用 Conventional Commits，版本与 CHANGELOG 由 release-plz 据此生成：

| 前缀 | 含义 | 发布影响 |
|---|---|---|
| `feat:` | 新功能 | minor |
| `fix:` / `perf:` | 修复 / 性能 | patch |
| `docs:` / `refactor:` / `test:` / `ci:` / `build:` / `chore:` | 其他 | 不发布 |
| `feat!:` / `fix!:` 或正文 `BREAKING CHANGE:` | 破坏性改动 | major（0.x 阶段为 minor） |

## 发布流程（维护者）

1. release-plz 根据 main 的提交创建版本与 CHANGELOG PR，更新 `crates/voicebar/Cargo.toml` 及根 `Cargo.lock`；`voicebar-release-tools` 配置为 `release = false`。
2. 合入发布 PR 后创建 `vX.Y.Z` 标签与草稿 Release，触发 Windows 构建。标签须与应用 Cargo 版本完全一致。
3. 工作流先运行 workspace 格式检查、Rust 测试和更新清单测试，再调用 `scripts/build.ps1 -Mode installer -Target x86_64-pc-windows-msvc`。
4. 上传 NSIS 安装包与便携 ZIP。有签名密钥时，`make-latest-json.mjs` 自动调用 `scripts/sign-update.ps1 -Bundle <exe> -Version <版本>` 签名，再生成并验证 `latest.json`。
5. 发布任务从唯一草稿 ID 下载实际附件，检查必需文件以及清单的版本、URL 和 SHA-256，全部通过后转为正式发布。无签名密钥时仅发布下载包，不生成更新清单。
6. 手动重试可在 Actions → Release → Run workflow 填入已有标签，或在 Release PR 工作流选择 rebuild。只允许替换草稿资产；已发布版本会被拒绝覆盖。

签名保留 `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` Secret，内部使用 Rust minisign 库，不需要额外 CLI。令牌与密钥配置见 [GitHub 设置清单](docs/github-setup.md)。

本地预演可安装 `release-plz` 后运行 `release-plz update` 并审阅 diff。不要手改已发布 Release 的 `latest.json`，也不要把旧版本标为 latest；客户端固定读取最新正式版的更新清单。
