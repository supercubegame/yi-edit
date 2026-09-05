//! Yi Edit 的 GUI 外壳。逻辑全在 yi-edit-core / yi-edit-fileio 里，这一层只负责画和收键。
//!
//! 两个环境变量是给 CI 用的，不设就完全不走那条路：
//! - `YI_EDIT_SHOT=out.png` 稳定几帧后截图并退出。
//! - `YI_EDIT_SHOT_SETTLE` 稳定秒数（默认 1.5）。用帧数不行：软件渲染下帧很便宜，
//!   字体与纹理还没上来帧数就到了，拍到的是一张空窗口。
#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod editor;
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
