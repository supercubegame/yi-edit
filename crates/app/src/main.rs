//! Yi Edit GUI 外壳。编辑器会话逻辑在 `yi-edit-session`，这一层只负责启动、字体与绘制。

#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ime_adapter;
mod shot;
mod theme;
mod ui;

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let arg = std::env::args().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            // 最小窗口与 theme.rs 里的面板宽度耦合：侧栏+跳转+行号栏 不得超过它的一半，
            // 三根栏的高度也不得超过高度的一半。crates/meta/tests/ui_layout.rs 里两条断言守着。
            .with_min_inner_size([860.0, 520.0])
            .with_title("Yi Edit"),
        ..Default::default()
    };
    eframe::run_native(
        "Yi Edit",
        options,
        Box::new(move |cc| {
            ui::install_fonts(&cc.egui_ctx);
            ui::install_style(&cc.egui_ctx);
            Ok(Box::new(ui::YiEdit::new(arg)))
        }),
    )
}
