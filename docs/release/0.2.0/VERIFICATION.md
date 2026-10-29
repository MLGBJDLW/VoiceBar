# v0.2.0 正式发布与清理记录

日期：2026-10-29。

## 发布身份与结果

- 正式发布：[VoiceBar v0.2.0](https://github.com/MLGBJDLW/VoiceBar/releases/tag/v0.2.0)。Release ID 为 `427400065`，发布时间为 `2026-10-29T02:55:01Z`（北京时间 10:55:01）。API 回读 `draft=false`、`prerelease=false`，`/releases/latest` 返回同一版本及 ID。
- 发行代码提交为 `772c8c2a688679572d9d022a3a891a779036f060`；已核对远端附注标签 `v0.2.0` 指向此提交。正式发布后保留该标签；本文是发布后的证据记录。
- [CI 40807646952](https://github.com/MLGBJDLW/VoiceBar/actions/runs/40807646952)：成功。MSVC workspace 测试、格式检查、生产构建，以及发布脚本与配置检查均完成。
- [Release 40807646854](https://github.com/MLGBJDLW/VoiceBar/actions/runs/40807646854)：成功。MSVC 构建、NSIS/ZIP 打包、签名、保存已签名产物、上传、重新下载校验及正式发布全部完成。
- API 返回的发布说明正文经换行规范化后，与 `RELEASE_NOTES.md` 逐字一致。新增、迁移、原有能力和未实测项目的划分见 [Changelog 核对](CHANGELOG_AUDIT.md)。

前两次尝试分别因重复 Windows Manifest 和草稿标签被重置而失败，均没有计入成功结果。相关修复与实际失败已经保留在核对记录中；第一个失败尝试留下的空草稿 `427389349` 在重新确认无附件、仍为草稿后已删除。

## 正式附件下载与核验

已从 GitHub 下载四个正式附件至 `artifacts/github-0.2.0/`。逐项比对 API 提供的 SHA-256 与字节数，全部一致：

| 文件 | 字节 | SHA-256 |
|---|---:|---|
| VoiceBar_0.2.0_x64-setup.exe | 4124519 | b82ce9fbbd87f9cd400bfe663186b9b08f7065124a2ee634e817cbcf1a95227a |
| VoiceBar_0.2.0_x64-portable.zip | 5709375 | 97d5526536827a7c4a5dd31ef19b123e24f54a619d5cf3781e7be3320234ce5d |
| VoiceBar_0.2.0_x64-setup.exe.sig | 344 | dbf18f8731cb27e0bb335e55a36613f13f709858c31ed1df9f537816e48a77f9 |
| latest.json | 767 | 26966a5fd0ccaf960dd8ffb3145be31fa96a76b5b3f394f74dce6c5d9d304eef |

- `node scripts/verify-release.mjs` 退出码 0：清单版本为 0.2.0，下载地址指向本次安装包，安装包 SHA-256 匹配。
- 使用应用 `updater.rs` 内置公钥及相同 `minisign_verify` 库，对实际下载的安装包和签名进行密码学验证，退出码 0；签名的可信注释绑定唯一 `version:0.2.0`。清单内签名与独立 `.sig` 文件一致。
- ZIP CRC 检查通过，内容恰为 `LICENSE` 和 `VoiceBar.exe`。其中 EXE 为 Windows x64 PE，13451776 字节，SHA-256 为 `bf1cf4e1a3030c5269b383a1bfface7a49f93f34a193af0fbabd51bc1fe27f6f`。
- 静态解析正式 EXE 的资源表确认恰有一个 RT_MANIFEST，ID 1 / 语言 1033；应用身份 `studio.voicebar.desktop`、执行权限、DPI 与长路径声明均与源码一致。
- 确认本机无运行中的 VoiceBar 进程后，将根目录 `VoiceBar.exe` 更新为此正式便携 EXE。清理后再次确认根 EXE 与 ZIP 内字节完全一致。未启动安装器或覆盖用户配置。

上述 MSVC 产物与此前本地 GNU 构建的体积及哈希不同；以本表作为正式下载及当前根目录 EXE 的身份依据。

## 最后执行的缓存清理

发布与附件核验完成后执行：

- `scripts/clean-build-cache.ps1 -MaxCacheMiB 0`：退出码 0，移除项目 `target` 下 13073 个文件；5613888961 字节降至 0，清理后 `target` 不存在。
- 清除已确认的旧 EXE 备份、测试程序、历史重复安装包、本地 GNU 包及 payload；仅在有 PNG 副本时移除 BMP，并清除没有活动浏览器进程使用的项目临时浏览器缓存。另清理 257035618 字节，退出码 0。
- 合计移除 **5870924579 字节，约 5.87 GB / 5.47 GiB**。该数值按已删除文件长度计，不等于文件系统分配簇或全盘空闲容量变化。
- 保留四个正式附件、根目录正式 EXE、PNG 验证截图、Git 历史及源码备份 `.nexa/tmp/native-cleanup/legacy-frontend-20261028.zip`。源码备份仍为 12632653 字节，SHA-256 `8b59a7f99bcac1f6cbd3ae39e6430089f47a77a5b894b2bf94656870092a48ce`。
- 清理后重新核对正式附件哈希与根 EXE，结果保持一致。全局 Cargo 下载缓存和其他项目目录不在本次清理范围内。

## 验证范围

本地 GNU 回归 83 项、原生托盘菜单夹具 1 项、Node 发布清单测试 11 项通过，远端 MSVC CI 与发布成功。真实麦克风、七种识别服务、安装/卸载和旧版原地升级未在本轮实测；前期快捷键完整按键夹具仍有已记录的前台焦点限制，均未写作通过。更新签名用于应用校验，不等同于 Windows Authenticode 发布者签名。
