<div align="center">

<img src="docs/logo.svg" width="96" height="96" alt="VoiceBar">

# VoiceBar

**按下快捷键，说完就有字。** Windows 原生桌面语音输入条，界面与后台均由 Rust 实现，使用 GPUI 进行 GPU 加速绘制。

[![CI](https://github.com/MLGBJDLW/VoiceBar/actions/workflows/ci.yml/badge.svg)](https://github.com/MLGBJDLW/VoiceBar/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/MLGBJDLW/VoiceBar?include_prereleases&sort=semver)](https://github.com/MLGBJDLW/VoiceBar/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-a7e8cf.svg)](LICENSE)
![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11%20x64-1f6f54.svg)

[下载](#下载) · [快速开始](#快速开始) · [识别服务](#识别服务配置) · [开发](#开发) · [发布流程](#发布与版本)

</div>

VoiceBar 常驻托盘，按全局快捷键开始说话，把识别文字写到当前输入位置。你可以再次按键结束，也可以开启停顿自动结束。识别服务可使用本机模型或自己的云端账户。

## 功能

- **四种写入模式**：边说边写、确认片段逐段写入、结束后统一写入、仅预览。实时模式会根据流式中间结果修订本次输入的尾部；切换输入位置或手动编辑后暂停自动写入，完整结果仍可复制。实时模式使用 Unicode 按键并保留剪贴板；无法安全撤回的修订会暂停，批量写入可选剪贴板兼容特殊应用。
- **停顿自动结束**：说话后安静 1–30 秒自动收尾，默认关闭。嘈杂环境可继续使用快捷键手动结束。
- **开机自启与静默启动**：两项可分别设置。自启只写当前用户的注册表项；静默启动控制手动启动时是否打开设置。可从托盘打开设置。
- **全局快捷键与托盘控制**：支持 `Ctrl+Alt+Space`、`Alt+Q`、`F8` 等组合。在「输入与启动」点击快捷键控件后直接按键录入，支持恢复默认及禁用取消键，保存后立即生效。托盘也可开始、结束或取消录音。
- **自绘窗口栏**：标题栏与深色工作空间融为一体，提供窗口拖动、最大化、最小化与关闭到托盘；窗口边缘仍支持缩放与系统吸附。
- **原生语音条**：九宫格位置、X/Y 偏移和四种声音响应动效。选择位置即可预览 10 秒，「保存外观与位置」可在录音期间独立保存；其余页面的未保存设置会保留。真实频谱与响度控制幅度和宽度；短停顿保留余韵，约一秒安静后收拢静止，继续说话再展开。确认片段和录音收尾提供轻微反馈；可调颜色、透明度、强度与尺寸，减少动态模式保留音量反馈。
- **七种识别协议**：FunASR、whisper.cpp、阿里云 Paraformer、OpenAI Realtime、Deepgram、科大讯飞听写、OpenAI 兼容 HTTP 转写。
- **可选二次校对**：结束录音后，使用 OpenAI Chat Completions 兼容模型修正错别字与标点。
- **默认不保存录音与历史**：密钥使用 Windows DPAPI 加密，空闲时断开识别连接、停止动效。

## 下载

从 [Releases](https://github.com/MLGBJDLW/VoiceBar/releases) 选择原生 GPUI 版本：

| 文件 | 用途 |
|---|---|
| `VoiceBar_<版本>_x64-setup.exe` | 推荐。NSIS 按用户安装，提供开始菜单与卸载项 |
| `VoiceBar_<版本>_x64-portable.zip` | 免安装，解压后运行 `VoiceBar.exe` |
| `*.exe.sig` / `latest.json` | 配置签名密钥时提供，供应用内更新校验 |

需要 Windows 10/11 x64 与正常工作的图形驱动。原生版本无需安装 WebView2、Node.js 或前端运行时；Node 仅用于仓库维护脚本。历史版本的附件以各 Release 说明为准。

## 快速开始

1. 安装并打开 VoiceBar，在 **识别引擎** 选择协议，填写服务地址与自己的密钥，测试连接后保存。本机 FunASR / whisper.cpp 服务需自行启动，程序不内置或自动下载模型。
2. 在 **输入与启动** 中选择 **边说边写**，点击目标输入框后按 **Ctrl + Alt + Space** 开始。已有配置保留原写入模式，需手动切换一次。
3. 将 **停顿自动结束** 设为 2 或 3 秒即可自动收尾；设为 0 时继续按快捷键结束。HTTP 批量转写服务会在录音结束后返回文字。
4. 在 Windows 设置 → 隐私和安全性 → 麦克风中，允许桌面应用访问麦克风。

可分别打开 **开机时自动启动** 与 **静默启动**。登录后自启在托盘待命；显式 `--hidden` 也会隐藏设置窗口。

## 识别服务配置

| 选择 | 默认地址 | 必填 | 方式 |
|---|---|---|---|
| FunASR | `ws://localhost:10095` | 服务启用鉴权时填 Token | 实时流式 + 句末修正 |
| whisper.cpp | `http://localhost:8080/inference` | 无 | 录完后转写 |
| 阿里云 Paraformer | `wss://dashscope.aliyuncs.com/api-ws/v1/inference` | DashScope API Key | 实时流式 |
| OpenAI Realtime | `wss://api.openai.com/v1/realtime?intent=transcription` | API Key、转写模型 | 实时流式 |
| Deepgram | `wss://api.deepgram.com/v1/listen?...` | API Key | 实时流式 |
| 科大讯飞听写 | `wss://iat-api.xfyun.cn/v2/iat` | APPID、APIKey、APISecret | 实时，单次约 60 秒 |
| OpenAI 兼容 | 完整的 `/audio/transcriptions` 地址 | 服务所需 Key 与模型 | 录完后转写 |

- OpenAI 兼容适用于 OpenAI、Groq、硅基流动及本地 Whisper 服务，模型由服务器决定。
- 阿里适配 Paraformer 协议，不能直接换成 Qwen Realtime 模型名；腾讯、百度、Azure 等专有流式协议尚未接入。
- WebSocket 当前直连，Windows 系统代理不会自动生效；HTTP 转写与校对使用 reqwest 的环境代理。
- 测试连接只验证握手与可达性，密钥权限和识别效果需实际录音验证。

命令行启动：

```powershell
VoiceBar.exe --hidden   # 仅托盘启动
```

当前原生版只实现 `--hidden`；旧版 `--toggle` / `--cancel` 尚未迁移，请使用全局快捷键或托盘菜单控制录音。

## 数据与隐私

- 配置沿用 `%APPDATA%\studio.voicebar.desktop\settings.json`，密钥字段为 `dpapi:` 密文，升级保留已有设置与密钥。
- 日志为 `%LOCALAPPDATA%\VoiceBar\logs\startup.log`，不应包含密钥或语音。
- 音频只发送到配置的识别地址；校对模型只接收文字，无遥测。
- 自启项为 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\VoiceBar`。

## 开发

旧 Tauri/Vite 前端、JavaScript 依赖与 WebView2Loader 已移除；仓库只维护原生应用与发布脚本。应用位于 `crates/voicebar`，属于根目录 Cargo workspace；界面使用 **GPUI 0.2.2 + gpui-component 0.5.1**，关闭 WebView 功能。构建需要 Rust 稳定版（满足各依赖的 MSRV）、Windows 链接工具链与 Windows SDK 的 `fxc.exe`；必要时将 `GPUI_FXC_PATH` 指向该文件。生成安装包还需要 NSIS。

```powershell
cargo run -p voicebar
cargo fmt --all -- --check
cargo test --workspace --locked
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Mode portable -Target x86_64-pc-windows-gnu
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Mode installer -Target x86_64-pc-windows-msvc
```

`-Target` 支持 `x86_64-pc-windows-gnu` 与 `x86_64-pc-windows-msvc`，需预先安装对应链接工具链。便携构建产出根目录 `VoiceBar.exe`，安装包为 `artifacts/VoiceBar_<版本>_x64-setup.exe`。

发布与图标维护脚本使用 Node.js 24 内置模块，无需安装 JavaScript 依赖：

```powershell
node --test tests/update-manifest.test.mjs
node scripts/render-icons.mjs
```

图标源文件位于 `crates/voicebar/icons/app-icon.svg`。贡献规范见 [CONTRIBUTING.md](CONTRIBUTING.md)，安全策略见 [SECURITY.md](SECURITY.md)。原生版的构建、声音响应与待验证项见 [原生版验证记录](docs/native-verification.md)。历史实测范围见 [验证记录](docs/verification.md)，其旧版本记录不代表 GPUI 版已经完成同样的实测。

## 应用内更新

设置中可选择启动时检查、仅手动或从不检查。更新清单固定读取 `https://github.com/MLGBJDLW/VoiceBar/releases/latest/download/latest.json`。

原生 Rust 更新模块使用保留的 minisign 公钥验签，并校验 SHA-256 与签名绑定的版本号。只有安装在 `%LOCALAPPDATA%\VoiceBar` 的 NSIS 安装版支持原地更新；便携版和开发构建使用手动下载。

维护者生成已签名更新的步骤：

```powershell
# 先设置 TAURI_SIGNING_PRIVATE_KEY 与 TAURI_SIGNING_PRIVATE_KEY_PASSWORD
# 清单脚本会自动调用 scripts/sign-update.ps1 签名
node scripts/make-latest-json.mjs --tag v0.2.0 --repo MLGBJDLW/VoiceBar --bundle artifacts/VoiceBar_0.2.0_x64-setup.exe --out artifacts/latest.json
node scripts/verify-release.mjs --manifest artifacts/latest.json --assets-dir artifacts --tag v0.2.0 --repo MLGBJDLW/VoiceBar
```

签名脚本使用 workspace 内部工具 `voicebar-release-tools`，生成 base64 编码的 minisign `.sig`，可信注释包含 `version:<semver>`。沿用原 Secret 名称，无需额外签名 CLI。

## 发布与版本

- 应用版本来源为 `crates/voicebar/Cargo.toml`，锁文件只有根目录 `Cargo.lock`；发布规则在 `release-plz.toml`。内部签名工具不单独发版。
- release-plz 根据 Conventional Commits 创建版本与 CHANGELOG PR；合入后创建 `vX.Y.Z` 标签与草稿 Release。
- Release 工作流在 Windows MSVC runner 测试、构建 NSIS 和便携包，按唯一草稿 ID 上传；有签名密钥时生成并核对 `latest.json`，远端附件校验通过后正式发布。重跑拒绝覆盖已发布版本。
- 保留 `RELEASE_PLZ_TOKEN`、`TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 三个仓库 Secret。未配置签名密钥时仍可发布下载包，但不提供应用内更新清单。
- 一次性设置与密钥保管见 [GitHub 设置清单](docs/github-setup.md)。不要把旧版本设为 latest，也不要改写已发布的更新清单。

## License

[MIT](LICENSE)。第三方识别服务的条款、计费与数据处理由各提供方决定。
