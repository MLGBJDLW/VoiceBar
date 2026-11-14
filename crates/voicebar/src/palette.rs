//! Shared sRGB colors for the GPUI surfaces and the native tray menu.
//!
//! 设计思路（给新手看）：
//! - 整体是"夜间录音棚"的石墨色，层级只靠几个百分点的明度差来区分，
//!   不用大块色差，也不用重阴影。
//! - 只有一种强调色（薄荷绿 MINT），用于"正在发生的事"：选中、主按钮、聆听状态。
//! - 颜色按"从暗到亮"排列：SIDEBAR < BG < INSET/PANEL < RAISED < HOVER。

/// 侧栏背景：最暗的一层，让导航退到后面，内容区成为视觉焦点。
pub(crate) const SIDEBAR: u32 = 0x0c0d0f;
/// 主内容画布。
pub(crate) const BG: u32 = 0x111214;
/// 卡片 / 分组容器，也用于托盘菜单底色。
pub(crate) const PANEL: u32 = 0x17181b;
/// 输入框、文本区等"凹进去"的区域，比卡片略暗。
pub(crate) const INSET: u32 = 0x0f1012;
/// 次级按钮等"浮起"的控件。
pub(crate) const RAISED: u32 = 0x1e2023;
/// 鼠标悬停时的背景。
pub(crate) const HOVER: u32 = 0x25272b;
/// 常规分割线 / 边框。
pub(crate) const LINE: u32 = 0x26282c;
/// 悬停或强调时的边框。
pub(crate) const LINE_STRONG: u32 = 0x35383e;
/// 主要文字。
pub(crate) const TEXT: u32 = 0xeceef1;
/// 次要文字（说明、标签）。
pub(crate) const MUTED: u32 = 0x8b9098;
/// 第三级文字（版本号、提示等最弱的信息）。
pub(crate) const SUBTLE: u32 = 0x60656d;
/// 唯一的强调色。
pub(crate) const MINT: u32 = 0xa3ead2;
/// 强调色的深色底（选中项背景）。
pub(crate) const TINT: u32 = 0x182823;
/// 选中项的边框。
pub(crate) const TINT_LINE: u32 = 0x3d6b5d;
/// 放在薄荷色按钮上的深色文字。
pub(crate) const ON_MINT: u32 = 0x0d211a;
pub(crate) const WARNING: u32 = 0xeac58a;
pub(crate) const DANGER: u32 = 0xf39ba9;
pub(crate) const FONT_FAMILY: &str = "Microsoft YaHei UI";
