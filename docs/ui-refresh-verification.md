# VoiceBar UI/UX 翻新与托盘修复

日期：2026-10-29。环境：Windows，150% 显示缩放，GPUI 0.2.2 / gpui-component 0.5.1。

## 交付内容

- 统一中性深灰、薄荷色重点操作、线性图标、按钮悬停、表单主题和卡片间距。
- 工作台重组为录音入口、当前识别服务/输入方式/浮框外观快捷入口，以及本次识别内容。侧栏显示导航说明和已保存快捷键。
- 外观页增加四种真实共享几何绘制的效果选择卡、五组快速配色、桌面位置九宫格；保留外观独立保存及十秒预览。
- 引擎选择改为明确标注本地/云端和实时/批量的服务卡片；输入、启动与校对设置沿用现有配置及安全处理。
- 浮框使用紧凑圆角状态卡。顶部显示状态、声浪、计时和停止/关闭；底部为可滚动状态提示、复制与设置。长文本不再挤走操作按钮。
- 外观缩放同时作用于字体、图标、间距及操作区，避免只缩窗口导致内容越界；按钮禁止收缩，长提示和转写正文独立滚动。
- 托盘菜单提供打开设置、开始/结束录音、取消本次录音、复制识别文字、显示/收起语音条及退出。无录音/无文字时相应菜单项禁用；开始项显示已保存快捷键。

## 根因与实现

原托盘通知仅在外层 GetMessage 循环处理。Shell 直接送入 WndProc 的消息无法被该分支消费。现在窗口拥有独立、稳定的 MessageEvents 队列，回调接收 sent/posted 通知，业务操作在外层统一执行。菜单嵌套消息循环仅在实际保存、退出或快捷键事件到达时结束；空心跳不会关闭菜单，快捷键不再通过反复重投处理。

兼容旧式和 v4 回调及负坐标；保留 Explorer 重启恢复、菜单的前台窗口处理和录音输入目标恢复检查。菜单 START/STOP 分离，避免打开菜单后会话结束，再点旧“结束”项目时意外启动新录音。

浮框越界源于长状态文字未限制最小宽度，以及缩小窗口时内部尺寸没有同步缩放。验证过程中还修复了 GPUI 同一按钮重复声明 hover 导致的崩溃，以及过度收缩导致的竖排标签和卡片重叠。

主要实现：crates/voicebar/src/ui.rs、ui/chrome.rs、desktop.rs；隔离原生验证位于 ui/native_tests.rs。

## 已完成的验证

- workspace 自动测试：71 passed、0 failed、2 ignored；含新增的托盘回调、菜单状态和真实隐藏消息窗口的 sent 通知回归。
- 忽略的原生 GUI 测试单独运行成功：1 passed。使用独立配置、独立快捷键，不启动麦克风，不加载用户识别密钥，不写用户配置。
- 长正文和四倍长度状态提示：65%、100%、180% 三种大小 × 顶部/右下角。通过 GPUI 实际 prepaint bounds 检查头部、正文及底部的所有元素在卡片内且尺寸为正。
- 实际系统菜单通过 v4 回调弹出，普通心跳下保持超过 1.7 秒，再通过消息正常收起；保留了真实菜单截图。
- 实际 Windows Graphics Capture 窗口截图检查：工作台、外观、引擎、输入与启动，以及最小窗口下的外观页。导航、标题、表单和卡片呈现正常。快捷键胶囊随后补充自动换行，容纳较长组合键。
- cargo fmt --all -- --check 与 git diff --check。

实际截图位于 .nexa/tmp/ui-refresh/workbench.png、appearance.png、engine.png、input.png、appearance-narrow.png。浮框与菜单截图为 overlay-*.png 和 tray-menu.png。截图均来自原生窗口/实际合成画面，非 HTML 替代界面。

普通 GPUI 设置窗使用无重定向的交换链，GDI BitBlt 截图会遗漏该窗口并采到下层内容；因此普通设置窗的 GDI 图片不作为验收证据，已改用 Computer Use 的 Windows Graphics Capture。浮框和菜单的像素另经 read_file native 检查。

## 设计参考

- Linear 官方界面改版与侧栏图：https://linear.app/changelog/2024-03-20-new-linear-ui
- Wispr Flow 官方浮条说明：https://docs.wisprflow.ai/articles/1790396454-move-and-dock-the-flow-bar-on-desktop

参考信息层级、低噪声导航和浮条状态表达；实现保留现有原生 GPUI 架构。

## 验证边界

真实麦克风、用户云服务识别、开机自启和安装/卸载未在本轮重新实测。自动回归使用本地协议夹具，不能替代真实服务验证。已有 Win32 extern 声明及重复清单链接警告仍存在。

旧运行实例不支持热更新，本轮不强制终止其潜在录音或未保存设置。新版 EXE 需在退出旧实例后启动。

## 最终构建与交付

最终 cargo test --workspace --locked --quiet：71 passed、0 failed、2 ignored。最后一次原生 GUI 测试：1 passed；四倍长状态、六组缩放/位置和实际系统菜单全部通过。随后仅增加较长快捷键胶囊的自动换行，并通过最终 workspace 回归与生产编译。

生产 cargo build --release --locked -p voicebar 以及 scripts/build.ps1 -Mode installer -SkipTests 均退出码 0。根目录、release、payload 和便携 ZIP 内的 EXE SHA-256 一致；ZIP 仅含 LICENSE 与 VoiceBar.exe。

| 文件 | 字节 | SHA-256 |
|---|---:|---|
| VoiceBar.exe | 14900224 | a3cf8f63c06f8038ee50db242c8b94e77bdc2a51d28656f2d117f2eead0ef12f |
| artifacts/VoiceBar_0.1.0_x64-portable.zip | 6022952 | f8dea4461e72bd73234c043a770c0458f396e8d124d1515cdc32cd1752fdacdd |
| artifacts/VoiceBar_0.1.0_x64-setup.exe | 4400910 | d0c464f984de20baf43e72d681687fc41debb26aaf46e3fdc37d5dd0e352f90b |

旧 EXE 已可逆移至 .nexa/tmp/ui-refresh/VoiceBar.previous.exe；旧进程 PID 38104 保留响应。原根目录已放置新版，未把旧进程视为已热更新。构建缓存未额外清理。
