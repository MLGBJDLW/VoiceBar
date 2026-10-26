# 贡献指南

VoiceBar 目前以个人维护为主。欢迎 Issue、功能建议和 PR；改动越大，越建议先在 Issue 里说明目标和方案。

## 环境

- Windows 10/11 x64，系统已安装 WebView2 Runtime。
- Node.js 22.12+（CI 使用 Node 24）。
- Rust 稳定版 + Windows 链接工具链。本仓库默认在本机使用 `x86_64-pc-windows-gnu`，CI 使用 MSVC。
- `npm ci` 安装前端依赖；Rust 依赖由 Cargo 自动获取。

## 本地检查

```powershell
npm run build                                  # tsc --noEmit + vite build
npm test                                       # 前端声浪测试 + Rust 测试
cargo test --manifest-path src-tauri/Cargo.toml
```

需要完整安装包时使用 `scripts/build.ps1`（NPX/Tauri 构建 NSIS + MSI，测试通过后才打包，并按上限清理缓存；`-Mode portable` 只产出根目录的 `VoiceBar.exe`）。不要提交 `target`、`dist`、`.nexa` 或根目录的 `VoiceBar.exe` / `WebView2Loader.dll`。

### Cargo.lock 只有一份

`Cargo.toml` 是 workspace 根（`members = ["src-tauri"]`），所以 **Cargo 只读写仓库根目录的 `Cargo.lock`**，`cargo metadata` 的 `workspace_root` 就是仓库根。`src-tauri/Cargo.lock` 是 workspace 迁移前的遗留文件，已被删除并在 `.gitignore` 中禁止重新出现（`/src-tauri/Cargo.lock`）——它当时已经缺少 `tauri-plugin-updater`，留着只会误导版本排查。

升级依赖时改 `src-tauri/Cargo.toml` 后在**仓库根**执行 `cargo update -p <crate>`（或用 `--manifest-path src-tauri/Cargo.toml`，锁文件同样落在根目录），提交根的 `Cargo.lock`。

## 提交规范

使用 Conventional Commits，版本与 CHANGELOG 由 release-plz 据此生成：

| 前缀 | 含义 | 发布影响 |
|---|---|---|
| `feat:` | 新功能 | minor |
| `fix:` | 修复 | patch |
| `perf:` | 性能 | patch |
| `docs:` / `refactor:` / `test:` / `ci:` / `build:` / `chore:` | 其他 | 不发布 |
| `feat!:` / `fix!:` 或正文 `BREAKING CHANGE:` | 破坏性改动 | major（0.x 阶段为 minor） |

示例：`feat(shortcut): 录制式全局快捷键编辑器`。

## 代码约定

- Rust：`src-tauri/src/` 下按职责分文件；错误信息面向用户、中文、可操作。密钥只经 DPAPI 落盘，日志与错误信息不得包含密钥原文。
- 前端：Vanilla TypeScript，无框架；新增动效或渲染逻辑放入 `src/effects.ts` 以便被 `tests/effects.test.mjs` 直接加载测试。
- 新增识别服务时，同时更新 `src-tauri/src/config.rs` 的校验、`src/main.ts` 的预设、README 的配置表，并为协议映射补测试。
- 涉及全局输入、快捷键、托盘的行为，请在 `docs/verification.md` 记录实际验证范围与未验证项。

## 发布流程（维护者）

1. PR 合入 `main` 后，release-plz 自动更新 `CHANGELOG.md`、`src-tauri/Cargo.toml`、**根目录 `Cargo.lock`**（workspace 锁文件）、`package.json` 与 `tauri.conf.json` 的版本，并开一个 `chore: release vX.Y.Z` PR。
2. 合入该 PR 后，release-plz 创建 `vX.Y.Z` 标签与 GitHub Release 草稿，构建工作流被触发。
3. 工作流在 Windows runner 上构建 NSIS/MSI 安装包并上传；已配置签名密钥时还会用 `scripts/make-latest-json.mjs` 生成 `latest.json` 一并上传。
4. 发布前的校验任务会核对 `x64-setup.exe`、`x64_en-US.msi` 是否齐全；有签名密钥时再运行 `scripts/verify-release.mjs`，确认 `latest.json` 的版本、下载地址与安装包 SHA-256 完全一致，任何一项不符都拒绝发布。
5. 发布前可本地预演：`cargo install release-plz` 后运行 `release-plz update`，检查生成的 diff；安装包可用 `scripts/build.ps1` 复现。

**不要**：手改已发布 Release 的 `latest.json` 或把旧版本标为 latest——应用内更新固定读取 `releases/latest/download/latest.json`。

如需手动发布，在 Actions → **Release** → Run workflow 填入已有标签即可重新构建并上传。
