# 仓库设置清单（一次性）

这些设置必须在 GitHub 网页或 `gh` 里手动完成，工作流本身无法代劳。

## 1. 创建仓库并推送

```powershell
gh repo create MLGBJDLW/VoiceBar --public --source . --remote origin --push
git push origin v0.1.0        # 触发 Release 工作流，构建首个安装包
```

## 2. Release Token（发版自动化必需）

`release-plz` 创建的标签如果使用默认的 `GITHUB_TOKEN`，GitHub 会阻止该标签触发后续的 Release 工作流。请在 **Settings → Secrets and variables → Actions → New repository secret** 添加：

| Secret | 值 |
|---|---|
| `RELEASE_PLZ_TOKEN` | 具备 `contents: write`（经典令牌再加 `workflow`）的 PAT，建议用 Fine-grained PAT 只授权本仓库 |

## 3. 更新签名密钥（应用内更新必需）

Tauri 的更新包必须签名，且**私钥一旦丢失就无法再给已安装的用户推送新版本**。本仓库已生成一对密钥，公钥已编译进 `src-tauri/src/updater.rs`（`PUBKEY`）：

| Secret | 值 |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | 私钥文件的**完整内容**（base64，单行） |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 私钥密码 |

**配置步骤**：把已生成、且被 `.gitignore` 排除的本地私钥内容（单行）粘贴为 Secret。

```powershell
# 输出私钥文件内容（单行 base64），复制整段到 TAURI_SIGNING_PRIVATE_KEY
Get-Content .nexa\tmp\updater-keys\voicebar.key -Raw
```

**保管要求**：离线备份私钥与密码（密码管理器 + 至少一份离线副本）。只把这一个 Secret 交给可信协作者；仓库其他维护者能看到公钥，但不应看到私钥。

**轮换密钥**（仅在私钥泄露时）：
1. 重新生成：`npx tauri signer generate -w new.key`
2. 把新的公钥（`new.key.pub` 内容）替换 `src-tauri/src/updater.rs` 里的 `PUBKEY` 常量；
3. 更新 Secret，然后发一个新版本。注意：**旧版本应用仍用旧公钥校验**，所以轮换后必须用旧密钥再签一次「桥接版本」，否则老用户无法升级到新密钥。

未配置这两个 Secret 时，Release 工作流仍会构建并发布安装包，但不会生成 `latest.json`，应用内更新会提示「还没有可用的更新包」。

## 4. 可选

| 项 | 作用 |
|---|---|
| Discussions | 在 Settings → General 打开，Issue 模板的「使用与配置问题」会跳转到这里 |
| 分支保护 main | 要求 CI 通过再合并；release-plz 通过 PR 改版本，因此不会直接写 main |
| Actions 权限 | Settings → Actions → General → Workflow permissions 选择 **Read and write**（release-plz 与 Release 工作流都需要写权限；工作流内已声明 `contents: write`，但仓库默认权限不能是 Read-only） |

## 5. 标签保护（可选）

Settings → Tags → New rule → 模式 `v*`，限制删除或强制移动版本标签，避免已发布版本被覆盖。

**重要**：`latest.json` 通过 `releases/latest/download/latest.json` 固定地址读取，因此「latest」必须始终指向最新正式版。不要把旧版本转为 latest，也不要删除最新 Release 的 `latest.json` 资产。
