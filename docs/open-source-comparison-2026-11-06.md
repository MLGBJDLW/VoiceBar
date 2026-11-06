# Windows 开源语音输入对照

核查于 2026 年 11 月 6 日。以 VoiceBar 0.2.1 为基线，沿着「选择麦克风 → 开始和结束听写 → 找回文字」核查三个支持 Windows 的开源项目。对照依据为维护者文档与源码；未在相同设备、模型和音频上测量准确率、内存或延迟。[VoiceBar 基线](https://github.com/MLGBJDLW/VoiceBar/tree/bec85e45f6f24c87dba825beb6d157b3ba514d4f)

| 用户遇到的问题 | 同类产品怎样处理 | VoiceBar 0.2.1 的表现 |
|---|---|---|
| 只想口述一句话，怎么结束？ | Handy 提供按住、切换及混合模式。OpenWhispr 支持按住至说完，也可以按两次。中文输入工具 CapsWriter-Offline 还支持鼠标侧键。[Handy 快捷键选项](https://github.com/cjpais/Handy/blob/82dfab9fda18c4bc24e1f6e56d36474d38eca0bc/src/components/settings/ShortcutActivation.tsx) · [OpenWhispr 听写](https://docs.openwhispr.com/guides/dictation) · [CapsWriter 说明](https://github.com/HaujetZhao/CapsWriter-Offline/blob/84912d5218ee5e51e216c54dc15a1cb0f466eb76/readme.md) | 可以再次按键或停顿收尾。如果要在某个词后立刻结束，仍需要第二次按键。[快捷键与自动结束](https://github.com/MLGBJDLW/VoiceBar/blob/bec85e45f6f24c87dba825beb6d157b3ba514d4f/README.md#功能) |
| 服务连得上，为什么说话没有字？ | Handy 在试用页遇到静音时就地提供设备选择器。OpenWhispr 显示当前输入设备，并将未发现设备、无法访问设备分别解释。[Handy 试用界面](https://github.com/cjpais/Handy/blob/82dfab9fda18c4bc24e1f6e56d36474d38eca0bc/src/components/onboarding/screens/TryIt.tsx) · [OpenWhispr 麦克风说明](https://docs.openwhispr.com/help/customise/choose-your-microphone) | 设置中已能选择麦克风，但首页「测试录音」会启动完整识别会话。只想确认新接入的耳机能否收到声音，也要走识别流程。[录音与设备设置](https://github.com/MLGBJDLW/VoiceBar/blob/bec85e45f6f24c87dba825beb6d157b3ba514d4f/crates/voicebar/src/ui.rs#L662-L698) · [会话启动](https://github.com/MLGBJDLW/VoiceBar/blob/bec85e45f6f24c87dba825beb6d157b3ba514d4f/crates/voicebar/src/session.rs#L226-L254) |
| 已经说完并开始下一句，还能取回上一段吗？ | CapsWriter 可从托盘复制结果，并按日期归档。OpenWhispr 的历史可保留文字和音频，可用保存的录音重新转写失败的会话。[CapsWriter 说明](https://github.com/HaujetZhao/CapsWriter-Offline/blob/84912d5218ee5e51e216c54dc15a1cb0f466eb76/readme.md) · [OpenWhispr 恢复流程](https://docs.openwhispr.com/help/fix/recover-a-lost-dictation) | 首页及浮框可复制本次转写。新录音开始时会清空当前文字，取回较早片段需要事先复制。[新会话初始化](https://github.com/MLGBJDLW/VoiceBar/blob/bec85e45f6f24c87dba825beb6d157b3ba514d4f/crates/voicebar/src/runtime.rs#L120-L135) |

## 本轮升级的取舍

1. **给短句增加按住说话。** 让松键成为明确的结束动作。原有切换方式适合长段口述，也适合不方便持续按键的人。
2. **在选择设备时就能测试收音。** 单独的本地测试只需显示麦克风是否收到声音；识别服务的地址、凭据和连通性继续独立验证。这样用户可以逐步定位故障，也无需为试麦向云端发送语音。
3. **让用户选择是否在本次运行中保留最近文字。** 新会话开始后仍能找回先前完成的结果。仅在内存中保留文本，退出即清除；从录音重跑识别需要保存音频，超出这项功能的范围。

## 仍有差距的场景

- **第一次离线听写。** Handy 在引导中提供模型选择与下载，VoiceBar 的 FunASR 和 whisper.cpp 需要用户自行启动服务。要缩短这个流程，还要处理模型占用空间、下载中断和卸载回收。[Handy 模型引导](https://github.com/cjpais/Handy/blob/82dfab9fda18c4bc24e1f6e56d36474d38eca0bc/src/components/onboarding/screens/Model.tsx) · [VoiceBar 快速开始](https://github.com/MLGBJDLW/VoiceBar/blob/bec85e45f6f24c87dba825beb6d157b3ba514d4f/README.md#快速开始)
- **总被听错的专名。** CapsWriter 可在识别后按自定义规则替换固定错词。VoiceBar 已会将热词传给 FunASR、OpenAI Realtime 及批量转写接口，但服务端提示无法保证特定拼写。若增加客户端替换，需要明确流式片段尚在修订时何时替换、同一规则如何避免执行两遍。[CapsWriter 热词与规则](https://github.com/HaujetZhao/CapsWriter-Offline/blob/84912d5218ee5e51e216c54dc15a1cb0f466eb76/readme.md) · [VoiceBar 流式协议](https://github.com/MLGBJDLW/VoiceBar/blob/bec85e45f6f24c87dba825beb6d157b3ba514d4f/crates/voicebar/src/providers.rs#L470-L488) · [批量转写](https://github.com/MLGBJDLW/VoiceBar/blob/bec85e45f6f24c87dba825beb6d157b3ba514d4f/crates/voicebar/src/providers.rs#L594-L607)
- **已有音频与字幕。** CapsWriter 支持拖入文件、导出字幕与时间戳。VoiceBar 的四种写入模式围绕当前光标，移动光标时还会暂停实时写入。导入文件需要独立的任务、导出和取消流程，适合另行设计。[CapsWriter 文件转录](https://github.com/HaujetZhao/CapsWriter-Offline/blob/84912d5218ee5e51e216c54dc15a1cb0f466eb76/readme.md) · [VoiceBar 写入模式](https://github.com/MLGBJDLW/VoiceBar/blob/bec85e45f6f24c87dba825beb6d157b3ba514d4f/README.md#功能)
