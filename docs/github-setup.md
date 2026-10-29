# 仓库设置清单（一次性）

本页供维护者在 GitHub 网页或自己的终端中操作；修改这些文档和工作流不会自动修改远端仓库设置。

## 1. 发布令牌

release-plz 使用默认 `GITHUB_TOKEN` 创建的 PR 和标签，不会自动触发后续 CI / Release 工作流。在 **Settings → Secrets and variables → Actions** 添加：

| Secret | 值 |
|---|---|
| `RELEASE_PLZ_TOKEN` | 只授权本仓库的 Fine-grained PAT，Contents 与 Pull requests 为 read/write；经典 PAT 使用 repo 与 workflow 权限 |

在仓库 Actions 设置中允许工作流创建 Pull Request，并按维护策略配置 main 分支保护。没有此令牌时仍可手动运行 Release 工作流，为已有标签构建；构建不会覆盖已正式发布的资产。

## 2. 更新签名密钥

原生 Rust 更新模块继续使用 `crates/voicebar/src/updater.rs` 中的 `PUBKEY` minisign 公钥。**保留原私钥与密码**，迁移界面无需更换密钥。

| Secret | 值 |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | 原私钥的完整 base64 内容（单行） |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 私钥密码；无密码的密钥可留空 |

Secret 名称为兼容既有仓库而保留。将私钥安全录入仓库 Secret，不要放入代码、日志、Issue、PR 或构建产物。离线备份私钥与密码；丢失私钥后无法再为使用旧公钥的客户端签发更新。

`scripts/sign-update.ps1` 调用 workspace 内部 `voicebar-release-tools`，使用 Rust minisign 库签名，不依赖额外 CLI 或 JavaScript 软件包。产物为安装包旁的 base64 `.sig`，可信注释包含 `version:<semver>`。清单脚本自动调用签名脚本，读取签名并计算安装包 SHA-256。

未配置私钥 Secret 时，Release 工作流仍可构建并发布 NSIS 与便携包，但不会生成 `latest.json`；客户端不提供该版本的应用内更新。

### 轮换

只在确有需要时轮换。先离线生成新 minisign 密钥并保管，更新原生更新模块的公钥；使用旧私钥签署包含新公钥的桥接版本，保留旧密钥直到旧客户端完成迁移。确认桥接版本后再切换签名 Secret。直接用新密钥签旧公钥客户端的更新包会导致验签失败。

## 3. 构建与发布

- Windows runner 使用 Rust MSVC 工具链、Windows SDK 的 `fxc.exe` 和 NSIS，工作流会设置 `GPUI_FXC_PATH`。
- 应用版本读取 `crates/voicebar/Cargo.toml`，必须与 `vX.Y.Z` 标签相同。
- release-plz 生成标签与草稿，Release 工作流构建 `VoiceBar_<版本>_x64-setup.exe` 和 `VoiceBar_<版本>_x64-portable.zip`；有私钥时还提供安装包 `.sig` 与 `latest.json`。
- 附件始终通过唯一 Release ID 上传，检查草稿状态后才可替换。远端附件验证通过后正式发布；已发布版本不能被工作流重跑覆盖。
- Node.js 24 仅运行发布清单测试、清单脚本和图标维护脚本，应用与安装包无需 Node.js。

手动发版：先在本机审阅并推送版本标签，再在 Actions → **Release** → Run workflow 填入已有标签。此文档不执行任何推送或发布。

## 4. 可选仓库设置

| 项 | 作用 |
|---|---|
| Discussions | 启用后可集中处理使用与配置问题 |
| main 分支保护 | 要求 CI 通过再合并，版本更新经发布 PR 审阅 |
| Actions 权限 | 工作流声明所需权限；仓库与组织策略需允许对应的 Contents / Pull requests 写入 |
| 标签 ruleset，模式 `v*` | 限制删除与强制移动版本标签 |

客户端固定读取 `releases/latest/download/latest.json`。latest 应指向最新正式版，不要把旧版本改为 latest，也不要删除最新 Release 的更新清单。
