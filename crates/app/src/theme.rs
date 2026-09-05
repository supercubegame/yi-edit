//! macOS 深色风格的颜色与尺寸。集中在一处，因为这些数字彼此耦合：
//! 面板宽度、分割线、行号栏宽度与行号位数改一个就要重算另一个，
//! 否则行号会与正文叠在一起 —— 而那一点只有看截图才发现得了。

use egui::Color32;

/// 窗口底色（编辑区）。
pub const BG: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x1e);
/// 侧边栏 / 工具栏底色，比编辑区略亮（macOS 侧边栏的观感）。
pub const CHROME: Color32 = Color32::from_rgb(0x24, 0x24, 0x26);
/// 发丝级分割线。
pub const HAIRLINE: Color32 = Color32::from_rgb(0x33, 0x33, 0x36);
/// 控件底色与悬停色。
pub const CONTROL: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3c);
pub const CONTROL_HOVER: Color32 = Color32::from_rgb(0x48, 0x48, 0x4a);
/// macOS 系统强调色（深色模式的蓝）。
pub const ACCENT: Color32 = Color32::from_rgb(0x0a, 0x84, 0xff);
/// 主文字与次级文字。
pub const TEXT: Color32 = Color32::from_rgb(0xeb, 0xeb, 0xf0);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x8e, 0x8e, 0x93);
/// 行号与当前行行号。
pub const GUTTER_TEXT: Color32 = Color32::from_rgb(0x5a, 0x5a, 0x5e);
pub const GUTTER_TEXT_ACTIVE: Color32 = Color32::from_rgb(0xc8, 0xc8, 0xcc);
/// 选区与匹配高亮。
pub const SELECTION: Color32 = Color32::from_rgb(0x25, 0x4a, 0x74);
pub const MATCH: Color32 = Color32::from_rgb(0x5c, 0x50, 0x28);
/// 当前行背景（比底色略亮一点就够）。
pub const CURRENT_LINE: Color32 = Color32::from_rgb(0x26, 0x26, 0x28);
/// 光标。
pub const CARET: Color32 = Color32::from_rgb(0xff, 0xd6, 0x60);

/// 工具栏 / 状态栏 高度。
pub const TOOLBAR_H: f32 = 44.0;
pub const FINDBAR_H: f32 = 36.0;
pub const STATUS_H: f32 = 26.0;
/// 侧边栏宽度。
pub const SIDEBAR_W: f32 = 220.0;
pub const JUMP_W: f32 = 96.0;
/// 行号栏宽度。**与 `LINE_NO_DIGITS` 耦合**：改一个必须重算另一个。
/// crates/meta/tests/ui_layout.rs 里有一条等号断言钉着。
pub const GUTTER_W: f32 = 72.0;
pub const LINE_NO_DIGITS: usize = 6;
/// 圆角半径（macOS 控件的观感）。
pub const RADIUS: f32 = 6.0;

pub const FONT_SIZE: f32 = 13.0;
