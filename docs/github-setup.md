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

## 3. 可选

| 项 | 作用 |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 生成 `.sig` 签名文件；启用自动更新前才需要 |
| Discussions | 在 Settings → General 打开，Issue 模板的「使用与配置问题」会跳转到这里 |
| 分支保护 main | 要求 CI 通过再合并；release-plz 通过 PR 改版本，因此不会直接写 main |
| Actions 权限 | Settings → Actions → General → Workflow permissions 选择 **Read and write**（release-plz 与 Release 工作流都需要写权限；工作流内已声明 `contents: write`，但仓库默认权限不能是 Read-only） |

## 4. 标签保护（可选）

Settings → Tags → New rule → 模式 `v*`，限制删除或强制移动版本标签，避免已发布版本被覆盖。
