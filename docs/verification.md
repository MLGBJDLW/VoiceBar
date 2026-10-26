# 验证记录

日期：2026-10-16 至 2026-10-26。Windows 11 x64 / Rust 1.99（x86_64-pc-windows-gnu）/ Node 24.16。

## 应用内更新（2026-10-26 续）

改动：接入 `tauri-plugin-updater`（仅 Rust 侧，无 JS 契约）；设置页「概览」新增更新卡片；Release 工作流签名并生成/上传 `latest.json`；新增 `scripts/make-latest-json.mjs` 与 `scripts/verify-release.mjs`。

已通过的检查：

| 检查 | 证据 |
|---|---|
| Rust 单测（更新模块） | `cargo test updater`：4 passed（feed 为 https 且指向 latest 资产、公钥可解出 minisign 公钥、非安装版不自称可更新、错误文案短且有指引） |
| Rust 单测（配置） | 新增 `update_check_setting_defaults_to_auto_and_rejects_typos`；全套 **36 passed / 0 failed** |
| 前端单测 | 声浪 15 + 快捷键 5 + 更新清单 11（含篡改哈希、缺平台、跨仓库 URL、版本不符、缺 MSI 五个负向用例），共 31 passed |
| 真实签名链路 | 用生成的私钥对 2,306,248 字节的 NSIS 安装包执行 `tauri signer sign --app-version 0.1.0`，`.sig` 解码后 trusted comment 含 `version:0.1.0`，`latest.json` 的 signature 为 436 字符 base64 |
| 真实校验链路 | `verify-release.mjs` 对真实安装包 + MSI 通过；把安装包末字节翻转后报 `sha256 不匹配` 并以退出码 1 失败 |
| 设置页 UI | 无头 Chromium + CDP 打开构建产物：更新卡片存在、按钮可点、检查后状态变 `ok`、安装按钮在无更新时保持隐藏、切到「从不检查」后把设置标记为未保存 |
| 响应式 | 2500px 宽：卡片宽 996px（受 `max-width` 约束）、无横向滚动；760px 宽：`.update-row` 变 `column`、无横向溢出 |

途中发现并修复的真实缺陷（与更新无关，但由本轮测试暴露）：

- **本机代理污染回环探测**：本机系统代理会把 `localhost` 的上游请求转发出去，返回 502，导致 `http_adapters_map_rejected_endpoints_and_reachable_whitelist` 与 `refused_loopback_is_not_reported_as_bad_key_for_every_kind` 误判为服务端错误。现在批量转写、连接测试与校对三个 HTTP 客户端在目标为本机回环地址时都设置 `no_proxy()`，两个测试在默认环境下通过（不再需要清空 `HTTP_PROXY`）。
- `updater` 依赖按 `~2.14` 固定：Tauri 2.14 起用 `--app-version` 把版本写进签名，并在 `requireSignedVersion` 时校验，防止「新版本号 + 旧安装包」的替换。

尚未验证（保持待验证，不视为通过）：

- 真实 NSIS 安装版在用户机器上的端到端更新：本机没有安装版 VoiceBar，`bundled()` 为 false，卡片只能显示「便携版/开发构建」路径。
- GitHub Actions 实跑：`tauri-action` 是否确实产出 `*.exe.sig`、`latest.json` 上传与 `verify-release.mjs` 校验在 runner 上的行为。
- `releases/latest/download/latest.json` 在仓库尚无 Release 时的 404 文案；以及 GitHub 被墙/超时时的提示文案。

## 0.1.0 发布准备（2026-10-26）

本轮改动：全局快捷键录制式编辑 + 独立「取消本次录音」快捷键；开机自启（HKCU Run）；托盘单击打开设置；单实例冲突处理；安装包（NSIS/MSI）与 release-plz 发版流水线；仓库根 Cargo workspace；图标改为程序化生成。

已通过的检查：

| 检查 | 证据 |
|---|---|
| 前端类型与构建 | `npx tsc --noEmit` 退出码 0；`npm run build` 成功 |
| 前端测试 | `npm test` 前端部分：声浪 15 passed、快捷键录制 5 passed |
| Rust 测试 | `cargo test`：31 passed / 0 failed（新增 `hotkeys_are_canonical_and_must_differ`、`autostart_defaults_off_and_survives_roundtrip`、`autostart_command_points_to_this_exe`） |
| 生产 exe | `npx tauri build`（`tauri.conf.json` 固定 `custom-protocol`）成功，`target/release/voicebar.exe` 6,807,552 bytes |
| 安装包 | `npx tauri build --bundles nsis,msi` 生成 NSIS 2.2 MiB、MSI 中文/英文各约 3.0–3.05 MiB；文件与 SHA-256 记录在 `docs/release/0.1.0/SHA256SUMS.md` |
| release 配置一致性 | 脚本解析 `release-plz.toml`、`.release-plz/changelog.toml`、`src-tauri/Cargo.toml`、根 `Cargo.toml`、`package.json`、`tauri.conf.json`、`package-lock.json`：版本均为 0.1.0，tag 模板 `v{{ version }}` |
| 设置页渲染 | 用本机静态服务器 + 无头 Chromium（CDP）实际打开构建产物：默认「概览」页整页截图正常（`docs/screenshots/ui-preview.png` 为输入与启动页、快捷键录制中状态），无横向溢出，版本号读取自运行时 |
| 图标 | `node scripts/render-icons.mjs` 生成 16/32/48/128/256/512 PNG 与 4 尺寸 ICO，逐尺寸目视检查 16px 仍可分辨（最终改为 5 根声柱） |

尚未实机验证（保持待验证，不视为通过）：

- 新版窗口在真实桌面上的显示：本机自动化会话无法捕获该窗口（进程存活但窗口未出现在可观测窗口列表中），设置窗口与托盘图标均未截图确认。
- 录制式快捷键在真实桌面按下后注册/冲突提示、`Escape` 取消、窗体标题同步。
- 挂起开机自启的注册表写入与回滚（测试只读查询当前状态，不写注册表）。
- 安装包安装/卸载流程、开始菜单与卸载项、覆盖升级。
- GitHub Actions 的实际运行（CI、release-plz、Release 构建）；工作流只做了静态检查与本地等价命令验证。

截图说明：`docs/screenshots/ui-preview.png` 在无头 Chromium 中经 CDP 调用真实页面脚本（点击「输入与启动」→ 点击快捷键框进入录制状态）后截取；期间在页面上下文中读到正确标题、版本号、无横向溢出。`docs/screenshots/hero.png` 为同一截图与小尺寸应用图标的排版合成，不含任何合成数据或伪造界面。

单实例：本轮尝试让新 exe 接管已运行的旧版本，窗口未出现在可观测列表中，进程随即退出；因此 0.1.0 的 `--toggle` 转发与托盘单击行为仍未实机确认。若用户的旧版 VoiceBar 正在运行，请先从托盘退出旧版本再启动新版本。

## 本次声浪响应与缓存清理

已修复普通语音幅度差不明显、减少动态模式音量不更新、停录残留旧分段值等问题。15 项前端声浪检查、28 项 Rust 检查、前端与生产构建通过；实际浏览器截图确认四种效果的静音、轻声、正常和大声差异。详细数据与当前原生启动验证限制见 [声浪响应验证](waveform-verification.md)。

按用户要求，构建结束后清除本项目 `src-tauri/target`：3,845,535,634 bytes → 0；另删除 13,696,112 bytes 的更早构建备份。保留根目录新版 exe、必要 DLL 和一份前版回退文件，清理后哈希检查通过。

`build.ps1` 现在在成功交付后调用 `clean-build-cache.ps1`，默认保留上限 512 MiB，超限通过 Cargo 清理本项目 target。清理后下次 Rust 构建会重新编译依赖。清理脚本的预演、真实清理和再次运行的空操作均验证通过；源码、配置、模型及共享工具链缓存均不在其清理范围。

## 前次连接与密钥修复

已复现默认 FunASR 端口未启动的 10061 错误，加入明确的服务启动提示和「测试连接」，并修复协议地址校验、密钥空白、跨服务沿用和鉴权错误分类。28 项自动测试、前端构建、生产构建通过；新版原生识别引擎页面及缺少云端 Key 的提示已观察验证。

各服务测试范围、原始错误、交付哈希和真实账户验证限制见 [连接与密钥检查记录](connection-verification.md)。

## 前次启动故障修复

根目录 exe 无窗口、随后退出的直接原因是漏交付 `WebView2Loader.dll`。PE 导入表明确依赖该 DLL，原构建目录有该文件，而根目录缺失；补齐后程序进入主函数并成功显示设置窗口。

同时修复了第二个打包问题：直接 `cargo build --release` 没有启用 Tauri `custom-protocol`，导致依赖开发服务器且没有嵌入前端资源。现在 Cargo 声明该 feature，构建脚本明确启用；遗漏它的生产构建会被编译检查阻止。构建脚本也会将生成的 `WebView2Loader.dll` 复制到 exe 旁。

本轮验证：

- 前端类型检查、生产构建、11 项 Rust 测试通过，构建脚本退出码 0。
- 根目录新 exe 和 DLL 分别与构建目录产物 SHA-256 一致。
- 开发服务器 1420 端口无监听时，根目录 `VoiceBar.exe` 已实际启动；截图确认设置首页、中文内容、语音条预览正常呈现。
- 启动日志确认 `custom-protocol=true`，已依次完成 setup、托盘创建、设置窗口创建和事件循环启动。
- 增加启动日志 `%LOCALAPPDATA%\VoiceBar\logs\startup.log`；进入主函数后的初始化失败会显示错误对话框。
- 本轮没有修改识别服务配置，没有调用云端 API。

本轮验证范围为启动和界面呈现。下列中文输入、待机及真实模型验证仍保持待验证状态。

## 已通过

| 检查 | 证据 |
|---|---|
| 前端类型检查与生产构建 | npm run build，退出码 0；Vanilla TypeScript / Vite |
| 原生代码编译 | cargo check，退出码 0 |
| 前端声浪自动测试 | 15 passed / 0 failed，直接调用生产渲染代码 |
| Rust 自动测试 | 最新 28 passed / 0 failed（含各适配器连接与密钥回归） |
| 生产可执行文件 | cargo build --release --features custom-protocol --manifest-path src-tauri/Cargo.toml，退出码 0 |
| 交付文件一致性 | 清理前根目录 exe 与 release 产物 SHA-256 相同；清理后根目录 exe / DLL 哈希未变，target 已移除 |
| 桌面设置页面 | 实际启动原生窗口，检查中文内容、深色布局和波形预览 |
| 原生透明语音条 | 实机捕获到无标题栏、置顶语音条，录音时显示本机服务返回的文字 |
| 全局快捷键与录音（上一轮） | 在当时新建的空白记事本标签按 Ctrl+Alt+Space，可开始和结束录音 |
| 双向流式协议 | 自动测试在 audio 通道未关闭时收到 interim，关闭后等待 final；实际麦克风链路连接到明确标记的本机固定文字测试服务 |
| 密钥加密 | Windows DPAPI round-trip 自动测试通过，get_config 掩码和保存密钥合并测试通过 |
| 文本聚合 | 讯飞动态替换、OpenAI 乱序完成、FunASR online/offline 修订测试通过 |
| 音频 | 16k / 44.1k / 48k 到单声道 16k 的计数与连续值检查通过；WAV PCM16 头检查通过 |

最终可执行文件：

- 大小：6,828,544 bytes（约 6.51 MiB）。
- SHA-256：1db8ee966303b976f8657fb955da009be137aa46746ca4c055980a50425030a3。
- 必须同目录保留 `WebView2Loader.dll`，160,880 bytes。
- 系统 WebView2 Runtime、外部模型、node_modules 和 Rust 构建缓存不包含在该 exe 大小中。

## 实机发现及修复

首次输入测试中，语音条显示正确中文，但记事本里的批量 Unicode 模拟输入出现重复末尾字符。之后将 SendInput 改为按完整 Unicode 字符发送、保持代理对完整，并以 8ms 间隔发送；每个字符前重新检查前台窗口和修饰键。

修复之后重新运行了 11 项测试并成功构建 release。该修复的中文实机复验仍待完成，不能把协议自动测试视为中文输入正确性的证明。

## 尚未通过实机验证的部分

- 本次声浪版在真实麦克风上的轻声、正常、大声试录，以及新版原生窗口重新启动（自动启动被中断回执门禁阻断）。
- 中文按键修复后的记事本复测，以及 Chrome、各 CLI 和其他桌面应用的逐项兼容性。
- 真正关闭所有界面后的待机内存、CPU，以及再次唤醒延迟。
- 用户所选云端服务的真实 API 调用、准确率和端到端延迟。
- 本地模型实际推理；本轮没有自动下载大型模型。
- 真实语言模型纠错请求及其语言质量。
- 多显示器、不同 DPI、全部九个方位和四个效果的完整交互回归。
- 狭窄视口的实机交互回归；源码含响应式布局和减少动态支持。

原生设置窗口和录音语音条已实际打开并检查。关闭设置窗口的自动操作返回了不确定回执，后续观测发现设置仍在。没有把当时设置窗口及 WebView2 仍在运行时的内存读数作为“待机内存”。

上一轮为更新 exe 结束旧进程后，Nexa 的桌面操作回执曾阻断复测。本轮已恢复应用启动验证，并修复上述两项交付问题；中文按键、真实模型和待机测量仍未复验。页面切换点击本轮被操作工具在投递前取消，因此只记录已观察到的首页呈现。

## 可复现命令

```powershell
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo build --release --features custom-protocol --manifest-path src-tauri/Cargo.toml
node tests/funasr-fixture.mjs 10095
```

正式交付使用 `scripts/build.ps1`，由脚本将 exe 和生成的 WebView2Loader.dll 一起复制到根目录。

最后一个命令仅启动固定文字的本机协议测试服务，不是语音模型。真实使用前先停止它，再启动自己的 FunASR 或配置其他识别服务。

无识别服务时，请先在设置中填写自己的服务；程序不会把合成演示或测试服务当作真实识别结果。
