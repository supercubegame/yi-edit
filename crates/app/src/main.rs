//! Yi Edit GUI 外壳。
#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ime_adapter;
mod shot;
mod theme;
#[path = "ui_safe.rs"]
mod ui;

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let arg = std::env::args().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
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
