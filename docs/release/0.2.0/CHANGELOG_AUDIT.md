# v0.2.0 Changelog 逐项核对

日期：2026-10-29。

## 比较基线

上一正式发布为 v0.1.0。已执行 `git fetch origin --tags`；v0.1.0 和本次工作开始时的 HEAD / origin/main 均为 `18f177aac63df2dda86c9abf81e7f8215a1849ed`。`git log v0.1.0..HEAD` 当时为空。

因此这次待发布内容主要是积累在工作区的完整原生迁移及后续修复，不能仅靠新增提交标题概括。已查看最近 30 条提交、上一版 README / 配置源码、当前原生实现和验证记录；v0.1.0 中已经完成的依赖升级及发布修复不重复算作 v0.2.0 新功能。

## 声明与证据

| 声明 | 判定 | 代码依据及措辞限制 |
|---|---|---|
| Rust / GPUI 原生界面 | 架构迁移 | 旧版 README 和 src-tauri 为 Tauri + WebView2 + TypeScript；当前入口为 crates/voicebar/src/main.rs，ui.rs / ui/chrome.rs 使用 GPUI。旧前端及 npm 构建已移除。 |
| 边说边写与尾部修订 | 新增模式 | 旧 config.rs 仅接受 stable / final / preview，默认 final；当前 config.rs 接受 realtime 并默认使用，session.rs TailEdit 与 platform.rs 检查本次插入尾部。已有显式模式保留。 |
| 停顿自动结束 | 新增 | 旧 AppConfig 无 silence_stop_secs；当前 audio.rs SilenceDetector / session.rs 使用，配置范围 0–30，默认 0。声活跃检测不等于语义断句。 |
| 独立静默启动 | 新增选项 | 旧版已有 autostart；当前 config.rs 新增 silent_start，starts_hidden 结合显式 --hidden。不能写成首次支持自启。 |
| 外观与位置独立保存 / 十秒预览 | 新流程与修复 | 当前 ui.rs save_appearance、runtime.rs preview_overlay / save_config；保留其他未保存项，活动会话只允许外观变更。九宫格、偏移及缩放上一版已有。 |
| 原生声音响应 | 改写与改进 | 当前 voice_visual.rs Analyzer 使用频谱与响度，voice_shape.rs 共用几何；旧前端只是音频包短时幅度分段。四种效果名称和数量不属于首次新增。 |
| 快捷键录入 | 原生迁移与补齐 | v0.1.0 README 已声称按键录入、即时保存与占用提示；当前 ui/shortcuts.rs / shortcut_capture.rs 重建原生流程并提供恢复默认/禁用；desktop.rs 保存事务保留旧绑定。不能写成本版首次支持自定义快捷键。 |
| 托盘主题与消息处理 | 修复与改进 | desktop/menu.rs 使用 palette.rs 与 GPUI 共用颜色/字体；desktop.rs 保留 HMENU 模态循环及 Shell v4/旧回调。真实菜单空闲/选中/录音状态和原生键盘导航已通过。 |
| 长文字与缩放布局 | 修复 | ui.rs / ui/native_tests.rs 的长正文、长提示及 65% / 100% / 180% 布局检查；历史原生验证单独记录。 |
| 七种识别协议、结束校对、DPAPI | 已有能力保留 | v0.1.0 README 与配置已经包含。当前 providers.rs / session.rs / config.rs / platform.rs 继续使用，不列为新增。 |
| 更新与联网默认值 | 保留并迁移实现 | 旧版和当前版 update_check 默认 auto；原生 updater.rs 固定读取 GitHub latest.json 并校验 minisign / SHA-256。未声称完全离线或真实安装版升级已实测。 |
| 命令行控制 | 当前兼容性缺口 | 当前 main.rs 只解析 --hidden；全源码无 --toggle / --cancel 实现。已纠正 README 和发布说明，未把旧版宏控制误写为原生版已支持。 |
| Windows 发行包 | 打包方式改变 | 当前 scripts/build.ps1 / installer.nsi 生成 x64 NSIS 与便携 ZIP，不生成旧 MSI。签名仅用于应用更新校验，不等同于 Authenticode 发布者签名。 |
| Windows 清单资源 | 发布阻断修复 | 首次 MSVC CI / Release 因重复 RT_MANIFEST #1 出现 CVT1100 / LNK1123；gpui-component 间接启用 GPUI 默认清单。应用 .rc 改为仅图标，打包时用 Windows SDK mt.exe 校验并替换唯一清单；未把失败的首次工作流写成通过。 |

第二次远端构建已通过 MSVC 测试、生产构建、NSIS/ZIP 打包和签名，但说明更新请求未显式包含 tag_name，草稿标签变为 untagged，上传前的身份校验正确阻止了上传。后续修正为所有 Release PATCH 均显式传递标签，并在上传前保存已签名产物。该失败尝试不计为成功发布。

## 验证口径

本轮本地 GNU `cargo test --workspace --locked --quiet` 为 83 passed / 0 failed / 4 ignored。托盘原生菜单夹具单独运行 1 passed；Node 更新清单测试 11 passed。三档 DPI 的 GDI 像素检查通过；辅助功能字符串及高对比度分支的菜单标志已检查，未更改用户高对比度设置。

前期快捷键完整录入夹具仍有前台焦点前置条件未完成的记录，不能将普通状态机/注册事务回归写成全部实机按键验收通过。真实麦克风、七种云/本机服务、登录自启、安装/卸载、旧版原地更新均不计为本轮实测通过。

最终 MSVC CI 40807646952 与 Release 40807646854 均成功，v0.2.0 已正式发布且为 latest；正式下载、应用公钥验签及清理记录见 [发布后验证记录](VERIFICATION.md)。本记录区分了此前失败尝试与最终成功结果。
