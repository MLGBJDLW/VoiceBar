# VoiceBar 设计与协议依据

核实日期：2026-10-16。以下来自官方产品页或协议文档；产品宣传中的速度、准确率不作为本项目实测。

## 从同类产品提炼的做法

| 产品 | 官方资料中的重点 | VoiceBar 的落地选择 |
|---|---|---|
| [Wispr Flow](https://wisprflow.ai/) | 在各应用内通过语音生成整理过的文字 | 全局唤醒、保持原输入位置；识别和校对分开配置 |
| [Superwhisper](https://superwhisper.com/) | 全局语音输入、离线能力、专有词和不同书写模式 | 本地识别服务、热词提示、独立可编辑校对指令 |
| [VoiceInk](https://tryvoiceink.com/) | 本地模型、各应用输入、词典和自定义用词 | 不绑定云平台，默认不保存录音或识别历史 |
| [Handy](https://github.com/cjpais/Handy) | 快捷键录音、本地模型、将转写文本粘贴到输入框 | 原生快捷键及录音，按需创建语音条，Unicode 输入与剪贴板回退 |

以上为独立实现，未复制这些产品的界面、商标或应用源码。

## 为什么使用 Tauri 2 + Rust + Vanilla TypeScript

Rust 负责麦克风、重采样、网络连接、Windows 输入和密钥。前端只负责设置与 Canvas 声浪，不嵌入 Electron/Chromium，不依赖 React。Windows 使用系统 WebView2。

设置窗口关闭会销毁该窗口；正常输入完成后语音条短暂确认再销毁。无录音时不建立网络连接、不持有麦克风。Canvas 仅在演示或实际录音、且可见时运行；支持 30/60 fps、系统减少动态和手动减少动态。外部本地识别服务的进程与模型内存应单独计量。

## 已实现的协议

| 协议 | 传输 | 能力与边界 |
|---|---|---|
| 阿里云 Paraformer | WebSocket | 等待 task-started 后发送 PCM；处理 result-generated，结束后等待 task-finished |
| OpenAI Realtime transcription | WebSocket | 24kHz PCM，增量 delta / completed；按 item 提交顺序聚合，避免乱序确认 |
| Deepgram Listen | WebSocket | linear16/16k 单声道；暂定和 is_final 分开，CloseStream 收尾 |
| 讯飞流式听写 | WebSocket | APPID/APIKey/APISecret、RFC1123 时间、HMAC-SHA256 签名；解析 sn/pgs/rg 动态修订；本程序在 58 秒结束以适应 60 秒限制 |
| FunASR 2pass | 本机 WebSocket | online 暂定追加，offline 最终替换暂定；服务需用户预先运行 |
| OpenAI compatible | HTTP multipart | 录音结束后上传 WAV；适用于 OpenAI、Groq、硅基流动和兼容本机服务，具体模型与权限由用户服务决定 |
| whisper.cpp server | 本机 HTTP multipart | /inference，录完后转写；模型由用户服务器加载 |

不把 HTTP 批量识别标注为实时流式。不承诺一个兼容地址就支持所有厂商的专有协议。腾讯、百度、Azure 等专有流式协议未实现；Qwen ASR Realtime 与 Paraformer 的消息格式不同，当前阿里适配器为 Paraformer。

## 一手协议来源

- [OpenAI Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription)
- [阿里云 Paraformer WebSocket](https://help.aliyun.com/zh/model-studio/websocket-for-paraformer-real-time-service)
- [讯飞语音听写流式版 WebAPI](https://www.xfyun.cn/doc/asr/voicedictation/API.html)
- [Deepgram WebSocket 指南](https://developers.deepgram.com/docs/lower-level-websockets)
- [FunASR 实时服务指南](https://github.com/modelscope/FunASR/blob/main/runtime/docs/SDK_advanced_guide_online_zh.md)
- [Tauri 窗口定制](https://tauri.app/learn/window-customization/)

阿里云当前文档推荐业务空间专属域名，原 dashscope.aliyuncs.com 域名仍可使用；设置中的服务地址可改为自己的业务空间域名。各平台模型名、权限、额度及接口可能变化，应以用户账户和服务端实际响应为准。

## 纠错与输入的取舍

暂定结果只显示在语音条中，最终确认结果才允许逐段追加。流式识别的修订不会用退格删除用户文字。最终语言模型校对仅在结束后运行；统一写入模式校对后再输入，逐段模式则保留校对版本供复制。

焦点保护观察原窗口和录音期间前台窗口变化。若用户在同一窗口内移动光标或切换输入框，仍需自行确保输入位置。Windows 权限等级和输入框实现也会影响模拟输入；不能把“系统级输入”解释成对每个程序的绝对兼容保证。

## 验证范围

自动测试覆盖协议解析、乱序、动态替换、重采样和本机 WebSocket 双向流式流程。测试固定文字不代表真实语音识别准确率。未使用用户云端密钥，也未自动下载大型模型。真实识别质量、端到端延迟和各平台权限需要用户选择服务后再以实际音频验证。
