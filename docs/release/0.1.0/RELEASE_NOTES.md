# VoiceBar v0.1.0

<img src="docs/logo.svg" width="72" height="72" alt="VoiceBar">

**按下快捷键，说完就有字。** Windows 10/11 x64 桌面语音输入工具的首个开源版本。

## 安装

1. 下载下表任一安装包（推荐 NSIS：按用户安装，不需要管理员权限）。
2. 安装包尚未签名，SmartScreen 可能提示「未知发布者」，选择「更多信息 → 仍要运行」。
3. 需要系统 WebView2 Runtime（Windows 11 自带；Windows 10 可从微软官网安装）。

| 文件 | 说明 |
|---|---|
| `VoiceBar_0.1.0_x64-setup.exe` | NSIS 安装包，2.20 MiB |
| `VoiceBar_0.1.0_x64_zh-CN.msi` | 中文 MSI，3.05 MiB |
| `VoiceBar_0.1.0_x64_en-US.msi` | 英文 MSI，3.05 MiB |
| `VoiceBar_0.1.0_x64-portable.zip` | 免安装，解压即用 |

SHA-256 见 `SHA256SUMS.md`；同一组文件也提交在仓库 `docs/release/0.1.0/`。

## 第一次要做什么

打开「识别引擎」，选择识别服务、填写自己的 Key，点 **测试连接**，保存。默认是本机 FunASR（`ws://localhost:10095`，需先启动服务）。程序不内置模型，也不把音频发往 VoiceBar 自己的服务器。然后点任意输入框，按 **Ctrl + Alt + Space** 开始说话，再按一次结束。

## 亮点

- **快捷键直接录制**：点击输入框按下组合键即可，支持 `F8` 这类单键；另有独立的「取消本次录音」快捷键（默认 `Escape`，可在录音过程中随时按下）。保存时立即注册并检测占用，失败会回滚。

<img src="docs/screenshots/ui-preview.png" width="860" alt="输入与启动：快捷键录制、开机自启">

- **开机自启开关**：只写当前用户 `Run` 注册表项，登录后仅在托盘待命，不需要管理员权限。
- **七种识别协议**：FunASR、whisper.cpp、阿里云 Paraformer、OpenAI Realtime、Deepgram、科大讯飞听写、OpenAI 兼容 HTTP 转写；可选用语言模型二次校对。
- **克制的界面**：四种随声音起伏的动效、九宫格弹出位置与偏移；单击托盘图标打开设置。
- **隐私默认值**：密钥用 Windows DPAPI 加密，默认不保存录音与历史，空闲断开识别连接。

## 已知限制

- 仅 Windows x64；安装包未签名，也没有内置自动更新。
- WebSocket 识别直连，不跟随系统代理。
- 以管理员权限运行的其他程序中，模拟输入可能被拒绝；结果仍可在语音条中复制。
- 如果旧版 VoiceBar 正在托盘运行，请先从托盘退出再启动 0.1.0，避免单实例接管冲突。

完整说明见 [README](https://github.com/MLGBJDLW/VoiceBar#readme)。许可证 MIT。
