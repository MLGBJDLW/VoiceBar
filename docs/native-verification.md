# 原生 GPUI 版恢复与验证

日期：2026-10-28。恢复自检查点 ab4e12dd-64d9-4ad9-b45f-837bee824bd2。

应用入口为 crates/voicebar/src/main.rs；设置页和语音条使用 GPUI 0.2.2，音频、识别、校对、托盘与快捷键均在 Rust 侧。当前 Cargo workspace 只构建 crates/voicebar 与内部签名工具。旧 src/、src-tauri/、dist/、node_modules/、npm 清单、TS 配置、WebView2Loader.dll、旧前端测试与预览脚本已移除，历史 Tauri EXE/MSI/ZIP 也已清理。删除前将 47 个源文件及历史产物压缩到本地忽略目录 .nexa/tmp/native-cleanup/legacy-frontend-20261028.zip 并逐文件校验；已有业务改动可从该归档恢复。

## 弹出位置保存与语音条显示修复（2026-10-28，后续修复）

用户反馈：调整位置后无法保存，桌面没有看到语音条。旧设置窗底部实际显示「请先结束录音再更改配置」；此前所有配置保存都会被进行中的会话拦截。旧预览按钮只设置显示标志，位置与外观读取的是已保存配置，忽略了表单草稿。

- 外观面板新增「预览此位置」与「保存外观与位置」。九宫格选择立即以草稿位置弹出 10 秒；预览期间修改偏移或大小会更新窗口，预览本身不启动麦克风、不写入配置。
- 独立外观保存只替换已保存配置中的 appearance，保留其他页面的未保存编辑。录音期间允许保存外观与位置；引擎、快捷键及输入配置继续在结束会话后保存。
- 语音条在显示期间会重新应用变化的位置/大小。Win32 窗口定位、显示、隐藏移至 GPUI App 更新之外，避免其同步 resize/paint 回调重入 App 借用；首次显示和重定位后显式刷新窗口。
- 实际启动录音时清除演示预览；密钥缺失等启动错误会立即显示，避免被模拟声浪覆盖。
- 回归：首次 cargo test --workspace --locked 为 67 passed、0 failed、2 ignored；补充录音启动错误测试后，最新编译的测试程序完整运行为 68 passed、0 failed、2 ignored。两个默认忽略项分别是既有前台输入测试与新增原生窗口测试。
- 新增原生窗口测试 ui::native_overlay_tests::hidden_overlay_renders_and_repositions 已单独运行，1 passed、0 failed。它创建独立窗口，无麦克风和用户配置操作，检查窗口显示与有效 viewport。
- 实际像素核验补充上述测试：已读取顶部和右下角各一张真实桌面合成截图，确认声浪、说明与按钮可见。150% 缩放下窗口为 840×315，顶部矩形 [860,24,1700,339]，右下角 [1696,1261,2536,1576]。只截取测试自身窗口区域，截图保存在本地忽略目录 .nexa/tmp/overlay-position-check/。
- GPU 合成窗口的 PrintWindow 截图为纯黑，不能据此判断绘制成败；本次像素结论来自实际屏幕上的精确窗口区域。
- cargo fmt --all -- --check 和 git diff --check 通过。保留既有 Win32 extern 声明及重复 manifest 链接警告。

本轮自动点击旧设置窗两次返回 computer_action_uncertain；后续截图均核对为只发生悬停，页面未切换，未重放点击。旧实例仍有未保存草稿，交付时不强退该实例；需要从托盘退出旧版后重新启动修复版。真实识别、自启及安装/卸载的其他待验收项保留。

修复版生产构建及 NSIS/ZIP 打包均退出码 0；Windows SDK mt.exe 提取并校验新 EXE 的 manifest 成功。根目录 EXE、release、payload 和便携 ZIP 内的 EXE 字节一致，ZIP 只含 VoiceBar.exe 与 LICENSE。

| 修复版文件 | 字节 | SHA-256 |
|---|---:|---|
| VoiceBar.exe | 14853120 | 8ca81b862c10960b097722c4cbb485bb42e807f71933b06b65fa1c5bd1453689 |
| artifacts/VoiceBar_0.1.0_x64-setup.exe | 4382465 | bd9a8635b83388af0fccebe78845f883177d8c1ddc2519666eaf8d7c4acbc40a |
| artifacts/VoiceBar_0.1.0_x64-portable.zip | 5999640 | 4d2d1bf17c5801a9a72e321ab41484ba221b31c810eada44459bc09d628124fc |

旧 EXE 已可逆地移到 .nexa/tmp/overlay-position-check/VoiceBar.previous.exe，原进程 PID 43120 继续运行且响应正常；根目录已放置修复版。没有终止其未保存草稿或宣称旧进程已经热更新。退出托盘中的旧实例后，再打开根目录 VoiceBar.exe 生效。

以下为此前迁移与标题栏交付记录（其中交付哈希与大小对应此前版本）。

## 自绘窗口栏（2026-10-28）

- 设置窗口启用透明原生标题栏，将 Windows 默认可见标题栏替换为 GPUI 绘制的 44px 深色窗口栏；五根薄荷色声柱与工作空间保持一致。
- 拖动区与最小化、最大化/还原、关闭按钮映射到 GPUI WindowControlArea；保留系统窗口缩放、吸附与双击最大化机制。
- 关闭仍通过现有 on_window_should_close 收回托盘；系统任务栏标题与应用图标保留。
- 修改后 Rust 全套 61 passed、0 failed、1 ignored；前台 Win32 独立输入测试另行通过 1 项。新生产 EXE 已实际启动（PID 43120、窗口 1970002），1568×1136 实时截图确认默认系统标题栏消失，自绘标识、中文标题、三个控制图标及内容完整呈现，窗口已置前。
- 窗口按钮自动点击仍被 retained action-reconciliation receipts 阻止，未把最大化/还原、拖动、最小化和关闭到托盘的实际动作标为通过。只读 WM_NCHITTEST 坐标探测得到 HTCLIENT；GPUI 的命中回调读取缓存的鼠标命中状态，单独传坐标不能完成该交互验收，因此未用此探测宣称按钮有效。

## 启动设置

- 输入与启动页提供独立的「开机时自动启动」与「静默启动」。
- 自启注册命令为带引号的当前 EXE 路径加 --hidden，写入当前用户 HKCU Run。修复了旧实现把注册表根句柄传成空指针的问题。
- 手动启动由 silentStart 控制，显式 --hidden 始终只显示托盘。旧配置缺少这些字段时保持兼容，默认不启用。
- 配置沿用 %APPDATA%/studio.voicebar.desktop/settings.json 与 DPAPI；本轮未修改用户的服务密钥或识别设置。

## 声音响应

真实 16kHz PCM 每 40ms 分析一次，使用两段重叠 512 点 FFT 与 22 个对数频段。响度采用绝对 dB 映射，避免轻声和大声被归一化成同样大小。噪声底、频带能量和迟滞仅估计声音活跃程度，不作语义断句判断。

开口约 60ms 起音、约 240ms 回落；短停顿前 300ms 保持展开余韵，随后收拢，约一秒后回到 35% 宽度并冻结相位。大于 250ms 的缓存音频帧失效；新录音重置视觉状态。确认文字扩展触发单次反馈，暂定文字修订、重复结果与校对改写不重复触发。减少动态模式冻结连续相位，保留响度和宽度反馈。

四种效果共用 voice_shape.rs 的路径几何。见[离线绘制对照](waveform-native-verification.html)。此页由实际 Analyzer、Motion 和共享路径代码处理合成 PCM 后导出，浏览器已观察到安静、轻声、正常音量、240ms/600ms 停顿的形状差异；它不替代原生窗口、麦克风或真实识别服务的端到端验证。

合成谐波信号仅改变振幅时的实测：

| 状态 | RMS | 显示能量 | 展开宽度 |
|---|---:|---:|---:|
| 安静 | 0 | 0% | 35% |
| 轻声 | 0.00514 | 43% | 78% |
| 正常音量 | 0.08584 | 85% | 95% |
| 停顿 240ms | 0 | 31% | 95% |
| 停顿 600ms | 0 | 7% | 71% |
| 停顿 1.2s | 0 | 0% | 35% |
| 重新开口 | 0.08584 | 85% | 95% |

## 已通过

- Windows GNU 原生开发构建与 Cargo 格式检查。
- 修改自绘标题栏后的 Rust workspace：61 passed、0 failed、1 ignored，含音频失效、会话重置和断句反馈回归；该忽略项创建独立前台 Win32 Edit 测试窗，已单独运行并通过（1 passed、0 failed）。
- Node 发布清单测试：11 passed、0 failed。
- PowerShell 构建、清理和签名脚本语法检查。
- 最终测试与交付后，按项目默认 512 MiB 清理门槛清除 target 构建缓存；交付 EXE、NSIS、ZIP 哈希复核保持一致。
- 初次资源提取发现 GPUI 默认清单缺少根 assemblyIdentity，mt 校验拒绝。现已增加应用自己的 Windows 清单与图标资源；最终 EXE 的清单已成功提取并通过 Windows SDK mt.exe -validate_manifest（退出码 0）。控件库仍间接启用 GPUI 的资源，链接器的重复清单警告保留，但最终有效资源已校验。

## 交付构建

- 自绘标题栏生产构建与打包成功（GNU，退出码 0）：根目录 VoiceBar.exe 14,821,888 bytes；NSIS 4,380,431 bytes；便携 ZIP 5,989,472 bytes。
- 根目录 EXE 与 release 产物及便携 ZIP 内的 EXE 逐字节一致；PE 导入中没有 WebView2 或 MinGW 动态运行库，仅需系统库。
- NSIS 的实际安装/卸载尚未运行。已补齐 FileVersion，并仅在存在 DLL 时打包，两个旧 NSIS 警告已消除；payload 每次重新创建，防止旧 WebView 文件夹带。EXE 导入检查确认此 GNU 产物不需要应用目录运行库。
- 便携 ZIP 仅包含 LICENSE 与 VoiceBar.exe。清理 4,541,160,275 bytes 构建缓存后，三个交付文件 SHA-256 保持一致。

| 文件 | SHA-256 |
|---|---|
| VoiceBar.exe | 13a1ead86cce5b53455cc1d1d3fef1adca12b3b1384f335e53694978c53b6036 |
| VoiceBar_0.1.0_x64-setup.exe | cdb0c145d1e7522be5953b893eba80a5a9bc77ab9fe7860bc2aa64cb83508766 |
| VoiceBar_0.1.0_x64-portable.zip | 812eb683fa00ba12ebada67e279c952da41eb6174858d3647e50108567d2c8df |

## 待验证

- 自绘窗口栏按钮和拖动/缩放的完整交互、静默托盘、点击保存、自启开关写入/回滚、注销登录或重启后的自启、真实麦克风与识别服务。原生应用启动和新窗口呈现已通过，详见上文。
- NSIS 安装/卸载与真实安装版更新、Windows MSVC CI 和 GitHub 发布。

前一轮 desktop_automation 曾受恢复运行时不透明动作回执阻断。本轮已成功通过该工具启动并捕获实际 GPUI 窗口；computer_control 的按钮点击仍返回 “Interactive input is blocked by retained action-reconciliation receipts”。已核对已知目标截图，未重放不确定点击，也未通过其他输入路径绕过保护。
