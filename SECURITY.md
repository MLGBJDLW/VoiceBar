# 安全策略

## 报告问题

请通过 GitHub 仓库的 **Security → Report a vulnerability**（私密安全公告）提交，不要开公开 Issue。请说明影响范围、复现步骤和受影响版本；预计 7 天内回复。

## 数据与密钥

- API Key、讯飞 APISecret 使用 Windows DPAPI 加密后存入 `%APPDATA%\studio.voicebar.desktop\settings.json`，只对当前 Windows 账户可解密。
- 音频只发送给用户自行配置的识别服务地址；校对模型只接收转写文字。程序不内置遥测、不上传诊断日志。
- 启动日志 `%LOCALAPPDATA%\VoiceBar\logs\startup.log` 仅含版本、PID、阶段与错误文本，不包含密钥与语音内容。
- 填写密钥时请避免使用带空格/换行的粘贴内容；程序会拒绝这类输入，防止密钥被截断后仍以“成功”状态保存。

## 支持范围

目前只支持 Windows 10/11 x64，且需系统 WebView2 Runtime。第三方识别服务本身的安全、计费与数据处理由对应服务商决定，不属于本项目的安全边界。
