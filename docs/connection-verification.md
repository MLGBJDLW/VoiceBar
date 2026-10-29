# 连接与密钥检查记录

> 历史归档：本页针对旧 Tauri/WebView 版本，旧前端与构建入口已在 2026-10-28 移除。当前 GPUI 版本请见 [原生版记录](native-verification.md)。

日期：2026-10-16。Windows x64 / Rust 1.99 / Node 24.16。

## 问题与复现

用户截图为「识别服务错误：WebSocket 连接失败：IO error: 由于目标计算机……」。

在当前环境检查默认 FunASR 回环端口 10095，TCP 返回 ECONNREFUSED。读取配置位置的脱敏元数据时，Roaming / Local 下的 VoiceBar settings.json 均不存在，符合使用默认本机 FunASR 的情形。本轮没有取得可用于真实云端验证的已保存 Key。

通过实际 providers::run 路径增加回归测试：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml unavailable_local_service_explains_setup -- --nocapture
```

修复前退出码 101，实际错误为：
```text
WebSocket 连接失败：IO error: 由于目标计算机积极拒绝，无法连接。 (os error 10061)
```

这表示尚未建立连接，不足以判断 Key 有效性。默认 FunASR 需要另行启动模型服务；填写云服务 Key 不会启动本机模型。

## 修复

- 本机服务未监听时显示服务名、回环端口及启动模型 / 切换云服务的处理方式。
- 区分 HTTP 401、403、404、429、重定向、超时、TLS 与普通网络失败。
- 七种协议分别校验 WS(S) / HTTP(S) 地址；云端缺 Key 或讯飞缺 APPID / APISecret 时，在录音前提示。
- 首尾空白会被清理，密钥内部空格、换行和控制字符会被拒绝。
- 切换服务时清空旧的 Key、APISecret、APPID；已保存密钥占位符只可在相同服务、相同源站内恢复。校对 Key 也受源站约束。
- 讯飞签名中的旧 authorization / date / host 参数会被替换，避免重复签名参数。
- 增加「测试连接」：使用当前表单，不录音、不发音频、不保存设置。WS 只验证握手，HTTP 只验证端点响应；不会将其当作完整模型与账户验证。
- 录音和外观预览不会在保存失败后继续使用旧设置。
- 语音条错误可换行查看，并提供打开设置入口；错误输出会隐藏配置中的 Key / Secret 及敏感查询参数。
- 阿里选项明确命名为「百炼 Paraformer」，避免将不同阿里产品的鉴权凭据混用。

## 各服务检查范围

| 服务 | 鉴权构造 | 已验证 |
|---|---|---|
| FunASR | 可选 Bearer Token、binary 子协议 | 本机拒绝连接、401/403/429、流式片段和最终结果 |
| 阿里云 Paraformer | Bearer | 请求头、握手成功、401/403/429、本机拒绝连接、缺 Key 预检 |
| OpenAI Realtime | Bearer | 请求头、401/403/429、本机拒绝连接、缺 Key 预检、文本完成顺序 |
| Deepgram | Token | 请求头与模型/语言查询参数、401/403/429、本机拒绝连接、缺 Key 预检 |
| 讯飞 | APPID + APIKey + HMAC APISecret | 签名字段与去重、run 路径 401、403/429、本机拒绝连接、缺字段预检 |
| OpenAI 兼容 HTTP | 可选 Bearer，本机允许无 Key | 端点 401/403/429/405、本机拒绝连接、云端缺 Key 预检 |
| whisper.cpp | 可选 Bearer Token | 端点 401/403/429/405、本机拒绝连接、无需云端 Key |
| 校对 HTTP | 独立 Bearer Key | Key 脱敏 / 源站绑定 / 空白校验、云端缺 Key 预检；与转写共用 HTTP 状态和网络错误处理 |

上述均为代码和回环测试服务的检查，不代表这些云账户或真实模型已经验证。

## 验证结果

- `npm run build`：类型检查与前端生产构建通过，退出码 0。
- `cargo test --manifest-path src-tauri/Cargo.toml`：28 passed / 0 failed，退出码 0（14.26 秒）。
- `cargo build --release --features custom-protocol --manifest-path src-tauri/Cargo.toml`：生产构建通过，退出码 0。
- 根目录 exe 与 release 产物 SHA-256 一致：
  - 大小：6,828,544 bytes。
  - SHA-256：0a44d708af4e04a35f37da702421bed91e04be04f399bc09daf1f0af73f988cb。
- 同目录 WebView2Loader.dll：160,880 bytes，SHA-256 86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5。
- 上一版 exe 与 DLL 已备份至 `.nexa/tmp/connection-fix/`。
- 已启动根目录新版 exe（进程 43460，窗口 20846188）。实际截图确认首页、识别引擎页面、测试连接按钮和缺少阿里云 Key 时的完整中文提示。
- 桌面输入工具有一次不确定回执和一次过期观察回执；均先重新捕获状态，没有盲目重放。最终截图只用于证明已观察到的页面和错误状态，不计作完整桌面自动交互回归。

## 尚需实际配置验证

没有加载真实本机模型，也没有用用户 Key 执行云端识别、校对或账户余额查询。使用前应启动本机模型服务，或选择云服务并填写对应 Key，然后测试连接、保存并试录。

WebSocket 当前直连；仅设置 Windows 系统代理不会自动让此连接通过代理。本轮未配置、修改或验证代理。此前中文跨应用输入、待机资源和多显示器等未验证事项仍见 verification.md。
