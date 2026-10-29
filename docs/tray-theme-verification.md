# 托盘菜单主题修复验证

日期：2026-10-29。应用版本：0.2.0；Windows x64，GPUI 0.2.2 / gpui-component 0.5.1。

## 根因和实现

原托盘使用普通 Win32 HMENU 字符串菜单，颜色跟随 Windows 默认设置，未使用应用的深灰与薄荷色。

现保留 HMENU 的系统行为，通过 WM_MEASUREITEM / WM_DRAWITEM 绘制背景、文字、快捷键、选中状态、禁用状态及分隔线；MENUINFO 背景画刷覆盖菜单空隙。配色及字体从 palette.rs 共享给 GPUI 与托盘：面板 #191d25、正文 #edf1f7、次要/禁用文字 #969faf、选中背景 #223b35、选中文字 #a3ead2、分隔线 #2a303b，字体为 Microsoft YaHei UI。

菜单打开时读取所在显示器 DPI 并测量实际文字宽度。项目字符串保留在 HMENU 中供辅助功能读取。itemData 使用校验后的索引，绘制状态只在菜单模态循环内有效；结束后先清除回调引用，再销毁菜单和 GDI 资源。Windows 高对比度开启或查询失败时使用系统绘制与配色。

## 本轮验证

- `cargo test --workspace --locked --quiet`：83 passed、0 failed、4 ignored，退出码 0。
- 新增 GDI 绘制回归覆盖 96 / 144 / 192 DPI，验证默认/选中/禁用/分隔线的真实像素颜色、长快捷键宽度及错误项索引拒绝；菜单字符串、禁用标志和高对比度系统绘制分支另行核对。
- 单独运行忽略的 `ui::native_overlay_tests::themed_tray_menu_renders_and_keeps_native_keyboard_behavior`：1 passed、0 failed，退出码 0。独立运行时、独立 F23 快捷键和合成会话；没有加载用户密钥、启动麦克风或写入用户配置。
- 上述原生夹具通过真实 Shell v4 回调打开菜单，捕获空闲、选中、跳过禁用项、录音状态的实际菜单区域。验证方向键、Home、Enter、Escape；Enter 触发设置，方向键跳过禁用项并能切换语音条显示。
- 第一次夹具的跳过断言依赖初始鼠标位置，失败后改为先用 Home 确定选择位置；最终夹具通过。失败结果未计入通过。
- 已人工查看空闲和选中 PNG：背景、中文、快捷键、禁用文字、分隔线与圆角选中底色显示正常。保留截图为 [原生菜单](screenshots/tray-menu-native.png)；其余临时截图位于 `.nexa/tmp/ui-refresh/tray-theme-*.png`。
- Node 发布清单测试：11 passed、0 failed。

## 验证边界

高对比度分支验证了菜单类型与标志，未切换用户的 Windows 高对比度设置。真实识别服务、麦克风、安装/卸载和旧版升级不属于这次菜单验证；前期快捷键端到端按键验收的剩余范围保留在 shortcut-capture-verification.md。

## 本地构建

`scripts/build.ps1 -Mode installer -SkipTests -MaxBuildCacheMiB 16384` 已退出码 0。应用版本与本地安装包为 0.2.0。

| 文件 | 字节 | SHA-256 |
|---|---:|---|
| VoiceBar.exe | 14937088 | 05d5cd29c7189121b960687c8563d821432545551456aafbf32ea4d9f0e7f427 |
| artifacts/VoiceBar_0.2.0_x64-setup.exe | 4410073 | 78cdd9289d518d8c7f7d8b20f16c8397b3fa5a6edde6a7a416e615df84f63e81 |
| artifacts/VoiceBar_0.2.0_x64-portable.zip | 6033034 | e2d2ab34cf1008a928a26582f682cef7d8499d97dd35012b00b497ceaccc0f36 |

根目录、release、payload 与便携 ZIP 内的 EXE SHA-256 一致，ZIP 只含 LICENSE 和 VoiceBar.exe。以上为本地 GNU 产物；GitHub Actions 使用 MSVC 构建正式 Release，字节与哈希不应混为一谈。

首次远端 MSVC 构建因应用与 GPUI 的重复 Manifest 发生 CVT1100 / LNK1123，未发布。修复后应用资源只在链接期嵌入图标，打包时用 Windows SDK mt.exe 校验并写入应用完整清单。本地重跑 83 项回归和单独菜单夹具通过；重打包成功。最终 EXE 的 PE 资源枚举确认仅有一个 RT_MANIFEST（ID 1，语言 1033），包含应用身份；mt.exe 提取后再次校验成功。

GitHub 发布及缓存清理的最终结果见本版本发布记录。
