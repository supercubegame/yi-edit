//! Yi Edit GUI 外壳。编辑器会话逻辑在 `yi-edit-session`，这一层只负责启动与字体。

#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod editor {
    pub use yi_edit_session::*;
}
mod shot;
mod ui;

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let arg = std::env::args().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 400.0])
            .with_title("Yi Edit"),
        ..Default::default()
    };
    eframe::run_native(
        "Yi Edit",
        options,
        Box::new(move |cc| {
            ui::install_fonts(&cc.egui_ctx);
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(ui::YiEdit::new(arg)))
        }),
    )
}
